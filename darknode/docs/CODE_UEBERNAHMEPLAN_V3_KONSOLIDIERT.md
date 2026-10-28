# DarkNode – KONSOLIDIERTER CODE-ÜBERNAHMEPLAN V3.1 – ERSTES ENTWURFSMUSTER

**Status: ERSTES ENTWURFSMUSTER / NICHT KOMPILIERT / NICHT GETESTET / KEINE PRODUKTIONSFREIGABE**  
**Stand:** 28.10.2026  
**Quelle 1:** Benutzerentwurf „CODE_UEBERNAHME_V2_RUST.md“ (2969 Zeilen; SHA-256 `40aec5fc336dd07ad4dfda3bc5bbdd135d7d4736fc7cad1cc2e0993df559d817`)  
**Quelle 2:** Benutzerergänzung „§16 WS_AI“ (852 Zeilen; SHA-256 `d1f12288ef281029100de4d545fc7785a165fca8fdbc314cfc4d2ec2c41dd284`)  
**Quelle 3:** „WS_AI Chart-Stack / History-Nachladen“ (192 Zeilen; SHA-256 `f523a410cde445f17f37cc605b4a7843c901fc464c30832e17039945fd5c8ab9`).  
**Quelle 4:** „WS_AI Chart-/REST-Limiter-/OHLC-Folgenachprüfung“ (317 Zeilen; SHA-256 `f238a43cb98918259b11a16e4cb365abd8e833e8194c03958fd1f8778f819782`).  
**Ergänzende historische Vorlage:** „CODE_UEBERNAHME_V1“: ausschließlich `auth.rs` und `orders.rs`, die der V2-Entwurf ausdrücklich nur referenziert (SHA-256 `132e9b5e7af3ae772adedbb0f96c0695b3d8669e7708ad17bdc894e4b8f38cec`).  
**Anker für interne Codeverweise:** `sawyerma/ws_ai@1f27ed319b70eb94ba6990799f120743819f1534`; Project_0/BSC/DarkBot unter `sawyerma/darkbot` nur in ihren jeweils angegebenen, separat zu prüfenden Commits.  
**Aktion in diesem Dokument:** Quellunterlagen zusammengeführt, Interimsentwurf geordnet, zwei eindeutig fehlerhafte Codebeispiele gezielt korrigiert und weitere kritische Abnahmeblocker ausdrücklich festgehalten. **Kein Rust-/TypeScript-Produktcode eingebaut, kompiliert oder ausgeführt; die beigefügten Muster sind keine Freigabe.**

## A. Entscheidung und Architektur – vor jedem Codeimport lesen

**Wir besitzen bereits:** DarkBot Graphite/React, das bestehende Protobuf-Gateway und dessen **einen** Browser-WebSocket, ältere Chart-/WS-/Aggregations-/Backfilllogik in `ws_ai`, M4-/Rust-Grundkomponenten und Evidenz in Project_0/BSC. DarkNode benötigt **keine** zweite Tradingplattform, keinen zweiten Frontend-WebSocket, keine neue Chartengine und keine Python-Runtime im Hotpath. Der statische DarkNode-Builder-Entwurf ist keine bereits produktiv eingebundene Node-Engine.

**Auswahlreihenfolge pro einzelner Funktion:** (1) vorhandener DarkBot-Produktcode, (2) eigener historischer `ws_ai`-Code, (3) intern bewährte generische Project_0-/BSC-Rust-Bausteine, (4) nur bei nachgewiesener Lücke fremde Open-Source-Implementierungen. Die drei internen Quellen werden nach *tatsächlicher* vorhandener Funktionalität verglichen, nicht pauschal hierarchisch ersetzt.

**Einbauprinzip:** Fremde Datei lesen → exakte Funktion/Verhalten/Fehlerfall identifizieren → in bestehenden Rust-Kern oder bestehendes React-Feature **ohne neue Owner/Transport-Lane** adaptieren → eigene Daten-/Crate-Typen verwenden → Testfall mit unverändertem Stamm-ID und Quell-SHA registrieren → erst nach Kompilierung und Test PASS vergeben.

| V1-Grenze | Festlegung |
|---|---|
| Trading-Autorität | **Ein** serieller Rust Risk-/Account-/Order-Owner; Bitget ist Autorität für Positionen und Stop-Wirksamkeit. I/O-Ausführung darf parallel erfolgen, schafft **keinen** zweiten Fach-Owner. |
| Backtest | Gleicher Rust-Indikator-/Graph-/Signalkern wie Live; separater simulierter Execution-Port, niemals reale Order-/Kontostände vortäuschen. |
| Daten | SBE V5/Originaltrades → L0/L1 → eigene 1s-/MTF-Bars L2; kleine SQLite-Metadaten; keine Redis-/ClickHouse-Pflicht. |
| Frontend | Bestehendes Graphite/React + Gateway/Protobuf; vorhandene `ws_ai`-Lightweight-Charts-Implementierung nur dort portieren, wo sie im aktuellen FE tatsächlich fehlt. |
| Indikatoren | Eigenständig mathematisch in **Rust** implementieren; **kein Pine, kein Pine-Parser, keine Pine-Parität**. |
| V1-Orderpolitik | One-way/Isolated als Soll, Freigabe erst durch Bitget-Capability/Readback; nur ENTRY aus bestätigt FLAT, keine blind wiederholte Order, initialer Exchange-SL nach jedem Fill bestätigen. |
| Host | MacBook M4 Max 128 GiB; bestehende Project_0/BSC-M4-Erkenntnisse übernehmen; GPU nur für spätere, getrennte Offline-Batches. |
| Safety | P5 S0–S7, `COUNTDOWN-CANCEL-ALL=OFF`, P5-A01–A28 und Crash C00–C16 bleiben vollständig verpflichtend. |

## B. Direkte Codeübernahme – **interner Bestand zuerst**

| Nr. | Echte vorhandene Quelldatei/Funktion | Konkret übernehmbares Verhalten | Ziel / Aktion | Status |
|---|---|---|---|---|
| INT-01 | `ws_ai/backend/websocket/ws_reconnect_manager.py` – `_reconnect_loop`, `_calculate_reconnect_delay` | Backoff, Jitter, State-Tracking, Alarm statt Endaufgabe | `dn-bitget/src/ws/backoff.rs`; Flapping, Nachrichtenverlust korrigieren | PORT+FIX, NOT_RUN |
| INT-02 | `ws_ai/backend/websocket/ws_lanes.py` – `record_message_received`, `is_stale`, `get_health` | Lane Health, Stale/GAP bei Disconnect oder Decodefehler | `dn-bitget/src/ws/lane.rs`; keine fiktive lückenlose Livequalität | PORT+FIX, NOT_RUN |
| INT-03 | `ws_ai/backend/websocket/ws_rate_limiters.py` – `TokenBucketRateLimiter`, `DequeRateLimiter` | Burst-Kontrolle und Ratebegrenzung | Rust begrenzte REST-/Reconnect-Limiter; Zeitbasis monotonic, keine Infinit-Rekursion | MUSTER, NOT_RUN |
| INT-04 | `ws_ai/backend/exchanges/bitget/services/bitget_client.py` – `connect`, `_subscribe_to_trades`, `_process_message` | Bitget-Subscription-/Lifecycle-Erfahrungen | Nur Ablauf, **kein** JSON-Decoder anstelle von SBE V5 | MUSTER, NOT_RUN |
| INT-05 | `ws_ai/backend/websocket/ws_message_parsers.py` – `BitgetMessageParser.parse_trade_message` | Mapping native Symbol / Trade-ID / Preis / Side | SBE V5 generiert; alten **First-Trade-only**-Fehler nicht kopieren | NICHT portieren |
| INT-06 | `ws_ai/backend/services/adapter/candle_agg_1s.py` – `on_trade`, `flush_stale` | OHLCV, Quotevolumen, Count, Zeitfenster | `dn-agg/src/trades_1s.rs`, Out-of-order/Overflow/GAP fix | PORT+FIX, NOT_RUN |
| INT-07 | `ws_ai/backend/services/adapter/stream_aggregator.py` – `ResolutionRegistry`, `MultiResCandleAgg` | Angefragte TFs mit TTL und MTF-Nutzungsverhalten | Registry übernehmen; **MTF aus finalen 1s** berechnen | PORT/MUSTER, NOT_RUN |
| INT-08 | `ws_ai/backend/services/usecases/historical_candles_from_trades.py` – `build_1s_window` | Rebuild aus Rohtrades im Zeitfenster | `dn-agg/src/rebuild.rs` nutzt **denselben** Aggregator, alle Quellen, stabile Ordnung | PORT+FIX, NOT_RUN |
| INT-09 | `ws_ai/backend/services/usecases/unified_historical.py` – `history` | Zeit-Cursor, Backfill-Fenster, Coverage-/Resume-Bedarf | `dn-backfill` mit Bitgets tatsächlich getesteten Public-Endpunkten | PORT+FIX, NOT_RUN |
| INT-10 | `ws_ai/frontend/src/pages/TradingPage/hooks/useChartView.ts` – `injectWhitespaceGaps`, `limitRealAndBuildWithGaps` | Keine Fake-OHLC, echter Candles-Limit, Lückenvisualisierung | Nur **belegte** GAP aus Coverage, keine stundenweise „leere Sekunde = Datenverlust“ | REUSE+FIX, NOT_RUN |
| INT-11 | `ws_ai/frontend/src/shared/components/CandleChart/*`, `useCandleChart.ts`, `chartAdapter.ts` | Vorhandenes Lightweight Charts/Theme/Lifecycle | Bestehendes DarkBot-Frontend nach Funktionsinventur erweitern, **kein zweiter Chart** | REUSE WHERE MISSING |
| INT-12 | `ws_ai/frontend/src/services/ws/WebSocketPool.ts`, `useWsLane.ts` | RefCount, Generationen, Resubscribe, rAF-Batching | Nur Verhalten in **DarkBot-GatewayConnection** prüfen/ergänzen, **kein zweiter Browser-WS** | MUSTER |
| INT-13 | `ws_ai/backend/ai/tier3/backtesting_engine.py` | **Kein** Backtest: Placeholder mit Nullmetriken | **NICHT** in P4 übernehmen | REJECT |
| INT-14 | `project_0/native/crates/common/src/channel.rs`, `timing.rs`, `runtime/src/storage/writer.rs` | Bounded Queue, Wakeup, Timing, MarketWriter | vorhandene generische Bausteine sauber referenzieren; OrderIntent eigene Durability | MONOREPO-REUSE |
| INT-15 | `bsc/engine/crates/geth_ipc/src/host.rs` | M4-Topologie/QoS-/Host-Code | nur generische Hostfunktionen, keine BSC-Fachlogik | MONOREPO-REUSE |
| INT-16 | `darkbot/frontend/src/core/realtime/GatewayConnection.ts`, `frontend/src/app/projectRegistry.ts`, `gateway/src/engine.rs` | ein Protobuf-Gateway-WS, Registry, Project-/Topic-Validierung | **bestehende Bestandteile behalten, DarkNode fachlich ergänzen** | KEEP |

**Externe Ergänzungen erst bei der danach verbleibenden Lücke:** Hummingbot Bitget UTA-V3 Auth/Private WS/Positionen/OrderTracker (offener PR; API-Contract selbst verifizieren); NautilusTrader Risk-/Reservation-Muster (LGPL, nicht pauschal kopieren); LEAN FillModel (Apache-2.0, nur sinnvolle Ausführungsregeln); HftBacktest Queue/Latency (MIT, L2 optional); Aeron SBE-Rust-Generator (Apache-2.0, schemagebundene Codegenerierung). **Kein gesamter fremder Bot wird installiert.**

## C. Übernahmesperren / Review-Befunde, die V2 noch nicht vollständig löst

Die nachfolgenden Punkte gelten als **BLOCKER FÜR CODE-MERGE/DEMO**, nicht automatisch als bereits reproduzierte Laufzeitfehler. Belege sind die mitgelieferten Codebeispiele; weitere externe Bitget-API-Behauptungen bedürfen Endpoint-/Demo-Verifikation. Korrekturen am Code dürfen nicht ausschließlich in Kommentaren existieren.

| ID | Priorität | Konkreter Ort im Quellentwurf | Fehlergrenze und erforderlicher Codevertrag / Test |
|---|---|---|---|
| V3-B01 | **CRITICAL** | `dn-core/owner/mod.rs::next_ids` | `ids=0` nach Neustart und `clientOid=dn{ids}{4-byte graph hash}` kann erneut dieselbe ID erzeugen. **Persistenter atomarer pro-Account-Namespace/Zähler oder kollisionsfeste ID mit Registry-Unique-Constraint und Restart-Golden**; Bitget-Zeichen-/Längenlimits belegen. Kein SEND bei ID-Konflikt. |
| V3-B02 | **CRITICAL** | `dn-core/protection/coverage.rs::evaluate` + `StopReadback` | Stopprüfung filtert nur Seite/Trigger/Menge; **Symbol, Account, Position-Key, Intent/Bot, Ordertyp, Status, reduce-only-Gültigkeit** fehlen. Fremder Stop darf keine Coverage belegen. Pflicht: Tests mit fremdem Symbol/Account/Position-Mode. |
| V3-B03 | **CRITICAL** | `dn-core/owner/mod.rs::position_lots` | Fehlender Positions-Eintrag wird als `0` behandelt, auch wenn noch **kein vollständiger Account-Readback** existiert. Nur qualifiziert `Complete+fresh+scope+generation` bedeutet bestätigt FLAT; sonst ENTRY und EMERGENCY-Entscheide blockieren. |
| V3-B04 | **CRITICAL** | `dn-core/owner/mod.rs::on_private` Orderauswahl | `clientOid == ... || orderId == ...` erlaubt widersprüchliche Zuordnung bei unterschiedlichen Intents. Separat auflösen, IDs kreuzvalidieren, Konflikt beweissicher journalisieren, keine Order-/Fill-Zuweisung. Gleiche Regel wie in `book_fill`. |
| V3-B05 | **CRITICAL** | `dn-core/owner/mod.rs::on_signal`, `HealthUpdate` | `daily_pnl_e8` initial 0 und ohne eigenständigen Nachweis-/Freshness-Status; im Ist-Code kann diese Null als gültige Verlustquelle erscheinen. ENTRY nur mit bestätigter, cashflowbereinigter Tagesverlustbasis und qualifiziertem Wallet-Readback. |
| V3-B06 | **CRITICAL** | `dn-core/protection/coverage.rs`, Owner-Tick | `PlaceStop`-/`ReadStops`-/`CancelStop`-Aktionen können bei jedem Tick erneut erzeugt werden; ACK/Readback-Race, Ratelimit, Doppelstops, wiederholte Emergency-Commands. **Persistente Aktion-IDs, Inflight-/UNKNOWN-/Reconcile-Gates, Retry-Budget und Statuspoll-Cooldown** je Stop-Intent. |
| V3-B07 | **CRITICAL** | `dn-core/owner/mod.rs::place_stop`, `dn-bitget/private/orders.rs` | Schutzauftrag nutzt generischen `order_type="market"`-Journalweg und `Purpose::Stop`; Bitget Strategy-Order-Typ, `clientOid`-Abdeckung, `reduceOnly`, Cancel-/Modify-Regeln sind **je Endpoint** zu beweisen. Stop-API getrennt typisieren, kein stiller Place-Order-Body für einen TPSL-Auftrag. |
| V3-B08 | **CRITICAL** | `dn-types/order_fsm.rs::on_fill` + `dn-store/book_fill_tx` | Original-V2 setzte bei lokal vollgebuchtem Fill bereits `FILLED` und gab die Reservation frei, bevor Exchange+Position reconciliiert waren. **Hier wurde diese einzelne Transition korrigiert; Pflicht: FSM-/Journal-/Restart-Regressionskette**, Endstatus nur via qualifizierte `Reconciled(...)`. |
| V3-B09 | **HIGH** | `dn-core/owner/mod.rs::on_truth`, REST-Fills | Reconcile prüft Zeitstempel eines Positionspolls, aber noch nicht sicher Snapshot-Coverage, unabhängige Order-/Fill-/Stop-Versionen und alle paginierten Quellen. Correlation-/Generation-IDs und tatsächliche Causal-Watermarks benötigen eigenen Contract. |
| V3-B10 | **HIGH** | `dn-bitget/private/executor.rs::run_executor` | Ein einzelner sequenzieller `execute`-Call kann durch langsame historische/REST-Leseanfrage die Stop-/Emergency-Kommandos aufhalten. **Priority/Fairness und getrennte begrenzte I/O-Lanes mit einem autorisierten Signer**; Owner bleibt allein. |
| V3-B11 | **HIGH** | `dn-core/risk/s1.rs::check_entry` | Gegenläufige UNKNOWN-/Pending-Entries werden bei Worst-Exposure durch den Same-Side-Filter ignoriert. V1: **jeder** unqualifizierte offene risikoerhöhende Auftrag sperrt einen neuen Entry; alle Net-/Gross-Exposure und Reservierungen berücksichtigen. Außerdem `checked_sub` statt ungesicherter Stopdistanz-Subtraktion. |
| V3-B12 | **HIGH** | `dn-core/owner/mod.rs::Activate/require_entry_allowed` | LIVE-Freigabe darf nicht aus STOP-Readback-Alter, drei Bool-Gates und `OpenBlockCount==0` allein folgen. Position/Orders/Stops, Account-/Mode-/Leverage-/Fee-/DailyPnL-/Risk-/Credential-/Market-Version müssen konsistent, frisch und nach dem letzten Reconnect vollständig gelesen sein. |
| V3-B13 | **HIGH** | `dn-store/order_journal.rs::book_fill_tx` | Fill-IDs und WS/REST-Ökonomie können bei unterschiedlichen Quellen unterschiedlich präzise/aggregiert sein; `same_economics` darf erst nach **belegter Exchange-Identitäts- und Normalisierungsspezifikation** endgültige Konflikte oder Duplikate bewerten. Unbekannte Identität ⇒ Journal-Sperre. |
| V3-B14 | **HIGH** | `dn-store/order_journal.rs` | SQLite-`synchronous=FULL`, `fullfsync`, WAL sind nur Konfigurationsanfang. Crashpunkt `fsync` vor SEND, DB-/WAL-/Checkpoint-/DiskFull-/fsync-Failure, Backup/Restore und Host-Migration müssen **empirisch mit Kausalität** geprüft werden. Keine P0-Latenzannahmen übertragen. |
| V3-B15 | **HIGH** | `dn-backtest/fill.rs::market_after/stop_market` | Aus öffentlichen Aggressor-Fills folgt nicht die verfügbare Ausführungskapazität/Queue des eigenen Market-Auftrags. Mark-/Trade-Eventsequenzen sind nicht in allen Streams direkt vergleichbar. Die Trade-only-Ausführung ist **Szenario**, keine realistische garantierte Market-Fill-Wahrheit. Fehlende Daten ⇒ `Indeterminate` oder ausdrücklich konservative Bounds. |
| V3-B16 | **HIGH** | `dn-agg/trades_1s.rs::Builder::add` | `DuplicateKey` wird nur bei identischem erstem/letztem Key entdeckt; doppelte Zwischen-Trade-IDs/Seq können Mengen erneut buchen. **Trade-ID-Dedupe im kanonischen L1-Ingress und im Aggregator-Test**, Payload-Konflikt=INVALID. |
| V3-B17 | **HIGH** | `dn-agg/mtf.rs::fold_into` | High-/Low-Tie wird nur nach ms, nicht nach eindeutigem Source-Key entschieden; Gap in tradefreier 1s-Strecke wird ohne vorhandenen 1s-Bar nicht zuverlässig in MTF-Qualität übertragen. Vektoren gegen **P1-T27** mit allen Kompositionsfeldern und Coverage-Range. |
| V3-B18 | **HIGH** | `dn-agg/rebuild.rs` | `source_seq` aus archiviertem WS und REST-History ist noch nicht als **globale kanonische Reihenfolge** belegt. Gleiche Millisekunde darf nicht mit erfundener Totalordnung als wahr verkauft werden; konflikthafte Reihenfolge=qualitätsbegrenzt. |
| V3-B19 | **HIGH** | `dn-bitget/ws/lane.rs::on_valid_message` | Eine erfolgreich verarbeitete Nachricht darf den vorherigen GAP nicht als **repariert** bezeichnen. Verbindungs-LIVENESS ist getrennt von marktdatenbezogener Coverage. GAP bleibt bis verifizierter Backfill-/Sequenz-Recovery offen. |
| V3-B20 | **MEDIUM** | `frontend/src/features/darknode/chart/series.ts` | `buildSeries` kann bei GAP-Whitespace und echtem Bar am selben Zeitstempel doppelte Serienpunkte erzeugen. Vor Anzeige auf eindeutige Zeitindizes mergen und Gaps nur dort rendern, wo keine echte Bar liegt. Umrechnung von Preis-Ticks zu JavaScript-`number` ausschließlich zur Anzeige; nicht für Geld- oder Orderentscheidungen. |
| V3-B21 | **MEDIUM** | `dn-backtest/fill.rs::gap_fills_worse` | Ausgangstest hatte nach Stop-Trigger keinen Folgetrade und musste fehlschlagen. **Hier wurde nur der Testinput durch einen späteren Trade ergänzt**; noch keine Kompilierung und kein PASS. |
| V3-B22 | **HIGH** | `dn-types/exec.rs::TypedOrder::canonical_bytes` | Kanonischer Requesthash umfasst im Beispiel nicht alle Order-/Account-Felder (Account, Symbol, Orderart, Margin, Stop-Kausalität); Hash ist **nicht** allein genügende dauerhafte Send-Identität. Versionierte kanonische Voll-Payload/Intent-Hash-Regel und Golden über Restart sicherstellen. |
| V3-B23 | **HIGH** | `dn-core/owner/mod.rs::recover` | `ProvenNotSent` nur aus belastbarem Send-Handoff-/Journalbeleg. `INTENT_DURABLE` vor Dispatcher-Übergabe ist ein Kandidat, aber Netzwerk-/Queue-Hand-off-Crash und Journal-FS-Verlust brauchen beweisbare Trennung, keine Schlussfolgerung allein aus In-Memory-Flag. |
| V3-B24 | **HIGH** | `dn-core/owner/mod.rs::evaluate_protection` | Beim Start nach Crash sind `expected_stop`, `stops`, `position_changed_ms`, `unprotected_since`, Notaus-Zustand im Beispiel überwiegend volatile Defaults. Exchange-Stop- und Intent-/Positionszustände vollständig rekonstruieren **bevor** normaler Entry/STOP-Cancel/Emergency aktiviert wird. |

**Zusätzliche fachliche Einschränkungen:** Stop-`replace-before-cancel` ist nur eine Sicherheitsregel, wenn Bitget nicht gleichzeitig Schutzorders der betroffenen Position widersprüchlich ausführt. Bei wirklich unbekanntem Stop-Status kein blindes zweites Place. Bei Position=FLAT ist eine *verwaiste* Stoporder nicht ohne Exchange-Readback automatisch zu löschen. Jeweils die existierenden P5-Sicherheitsgates nutzen, keine neue parallele Gate-Hierarchie.

## D. Korrigierte Code-Pakete für den späteren Auftrag

| Paket | Codequelle zuerst | Einbau (keine neuen parallelen Systeme) | Abnahmebedingung |
|---|---|---|---|
| **CODE-00** | DarkBot+ws_ai+Project_0+BSC | Repo-/Crate-/Frontend-Inventory; tatsächliche Duplikate beseitigen; License/NOTICE/SBOM; Protobuf-Domäne/Topics nach Governance | Bestands-/Importmanifest, kein neuer WS/Renderer |
| **CODE-01** | ws_ai Reconnect/Lanes; P0 Channel/QoS | `dn-bitget/ws/backoff.rs`, `lane.rs` als **kleine Rust-Module**, P0 Wakeup übernehmen | DN-P1-WS-001–006, Gap vs Liveness |
| **CODE-02** | Aeron + öffentliche Bitget-Schemas; alter Bitget-Parser nur Negativmuster | V5-SBE Codegen mit realen Rawframes, Group-/Header-/Version-/seq,pseq-Checks | P1 T01–T06, ursprünglicher 8/62-Framefehler aufgeklärt |
| **CODE-03** | ws_ai `candle_agg_1s`, `stream_aggregator`, `historical_candles_from_trades`; P0 Writer | ein Rust-1s-Agg, MTF aus 1s, Rebuild aus L1, L0/L1/L2 binary | P1 T08–T12/T23/T25–T27, Agg-Regressions |
| **CODE-04** | ws_ai `unified_historical`, Bitget-Public-Read-Recherche | Cursor/Rate-Limit/Coverage/Backfill, Warmup→Live-Cutover | P1 T13–T15 + P2-Gates |
| **CODE-05** | eigene Rust-Indikator-/Graph-Planung + intern vorhandene Rechenmodule | mathematische Rust-Nodes, GraphCompiler, Bool3, unveränderter GraphHash | P3 Goldens, kein Pine |
| **CODE-06** | LEAN/HftBacktest nur fehlende Simulationsalgorithmen | derselbe Signal-/Risk-Kern, separater Sim-Port, Fill/Fee/Funding/Stop-Unsicherheit | P4 OOS/NoLookahead/Gaps + DN-P4-001–010 |
| **CODE-07** | Hummingbot UTA, Bitget.Net + eigener P5-Phasenplan | Rust REST/Auth/Private WS/Wallet, paginierte Snapshots, Stop-/Order-Readback | nur Read-only-Demo bis separates Write-Gate; P5-A01–A28 |
| **CODE-08** | P0/BSC Journal/QoS + Hummingbot Tracker + Nautilus/Freqtrade Testmuster | ein autoritativer Rust-Owner, persistente IDs, Reservations, Fill-/Stop-FSM, Recovery | C00–C16, P5 Safety/STOP, DN-P5-ALL-001 |
| **CODE-09** | DarkBot FE + vorhandener ws_ai Chart/History-Merge | Rust→Proto→bestehendes Gateway→fünf DarkNode-Tabs; Demo nur nach Autorisierung | DN-PROTO, P6/P7/P8, kein extra WS/Chart |

Die Nummerierung wurde **bewusst nach fachlicher Abhängigkeit geordnet**. Falls bestehende CODE-00..09-IDs bereits anders im verbindlichen Projektplan vergeben wurden, **keine alten IDs still umdeuten**: in diesem Dokument heißen sie ausdrücklich `V3-CODE-00..09`; vor Git-Übernahme mit bestehendem Plan abgleichen. Keine abgeschlossene Aufgabe ohne überprüften Testprotokolleintrag.

## E. Test-IDs und Nachweispflicht – keine neuen PASS behaupten

| Testfamilie | Bereits in den bereitgestellten Vorlagen | Geplanter Umgang |
|---|---|---|
| P1 | T01–T27, K01–K15, D01–D08 | Bestehende IDs beibehalten und in `TESTPROTOKOLL.md` fortführen |
| P5 | P5-A01–A28, C00–C16 | Unverändert; P5-Demo-Write/LIVE nur mit entsprechender Autorisierung |
| P5-Adaptierung | DN-P5-UP-001–028 (V1/V2, verschiedene Def.-Grade) | Auf doppelte/fehlende IDs prüfen, als **geplante** Regressionen registerführen; nicht automatisch PASS |
| P1-WS | DN-P1-WS-001–006 | Hier in §16 spezifiziert, noch `NOT_RUN` |
| P1-Aggregation | DN-P1-AGG-001–008 | Hier in §16 spezifiziert, noch `NOT_RUN` |
| P4-Simulation | DN-P4-001–010 (teils im V1-/V2-Entwurf) | Auf vorhandene IDs prüfen; Rust-Tests unkompiliert → `NOT_RUN` |
| Neu durch V3-Codeprüfung | V3-B01–B24 | **Review-Befund-IDs, keine Test-IDs**; die zugehörigen Negativtests im bestehenden Testregister ergänzen |

**Verifizierungsreihenfolge:** zunächst Dokumentationsreview → `cargo fmt --check` → `cargo check --workspace` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo test --workspace` → Bitget-Rawframe-/SQL-/Crash-Determinismus → Fake-private-API-Faults → **separat autorisierte** Demo-Reads/Writes → Soak → erst eigene P9-Gatefreigabe. Keine der vorliegenden Dateien führt diese Schritte aus. P0/BSC-Host-Belege sind `REUSED_EVIDENCE`, kein DarkNode-Safety-PASS.

**Fertig-Bedingung pro SRC-ID:** Upstream-SHA+Datei+Funktion, gewünschter Algorithmus, eigener Zielpfad, vollständige Lizenzentscheidung, Adaptergrenze, Assertions einschließlich Negativfall, Code-SHA, beobachtete Test-Evidenz und Reviewer; dann erst `ADOPTED`. Bis dahin `PLANNED`.

## F. Nachweis-/Lizenzliste

- **Eigene Vorarbeit:** `sawyerma/ws_ai@1f27ed319b70eb94ba6990799f120743819f1534` – konkrete Dateien in §B und §16; Project_0-/BSC-Dateien im selben DarkBot-Monorepo; `frontend/src/core/realtime/GatewayConnection.ts` und Graphite als fortbestehende Browser-/Gateway-Wahrheit.
- **Hummingbot UTA V3:** [offener PR #8343](https://github.com/hummingbot/hummingbot/pull/8343), im Codeentwurf fest auf `06e491ca144045568a57d43e746a6e9b183241af` referenziert; ungemergter Code ist kein Release-PASS. Apache-2.0.
- **LEAN:** `Common/Orders/Fills/FillModel.cs@22404ea28b8d3f3c9eb74d6ed9f0444426f8e417`, Apache-2.0; **nicht** C#-Engine portieren.
- **HftBacktest:** Rust-Queue/Partial-/Latenzmodell @`5f3ec40b2afb764e0fea112f941ed85523ef4e88`, MIT, optional wenn echte L2-History vorliegt.
- **NautilusTrader:** Risk-Engine `6a64881b9f54f3ca79a322e980dbef51fd5482a9`, LGPL-3.0, ausschließlich Muster bis Lizenzentscheidung.
- **Freqtrade:** Stop-Recovery-Regressionen `dafa0b6f4e1af30a80aedf730b5509ca59d0d7cc`, GPL-3.0, keine fremde Testdateikopie.
- **Aeron:** RustGenerator `07a9f7dc6dd48d953cb60f33d6f4649016af6d53`, Apache-2.0, tatsächliche Bitget-V5-Bytekompatibilität noch nachzuweisen.

---

# TEIL I – Rust-V2-Entwurf (§0–§15, vollstaendig inkl. Code)

> **Leseregel:** Die nachfolgenden Codebeispiele wurden aus der vom Nutzer gelieferten V2-Fassung übernommen. Die zwei lokal korrigierten Stellen sind in Abschnitt C als `V3-B08` und `V3-B21` benannt. **Alle anderen in Abschnitt C genannten Codefehler müssen noch vor Integration behoben werden.** Diese Beispiele sind Designmaterial, weder eine fertige Workspace-Codebasis noch eine Demo-/Live-Freigabe.

## 0. Was gegenüber V1 korrigiert ist

| Prüfpunkt | Korrektur in V2 | Datei |
|---|---|---|
| `book_fill`: Identität über `OR` | `clientOid` und `orderId` werden getrennt aufgelöst. Widerspruch ⇒ Fill wird gesichert, keine Mengenbuchung, Kontosperre | `dn-store/src/order_journal.rs` |
| Dedupe zu schwach | Vergleich aller wirtschaftlichen Felder (Seite, Menge, Preis, Gebühr, Gebührenwährung, Zeit) und der Order-IDs. WS und REST zählen als zwei Quellen desselben Fills | dito, Tabelle `fill_observation` |
| Überfüllung | Wird vor der Buchung erkannt. Fill bleibt beweissicher gespeichert, Order → UNKNOWN, Konto → RECONCILING. Alles in einer Transaktion | dito |
| Zustand + Risiko atomar | Fill, Orderzustand, Übergangsprotokoll und Reservierung in **einer** SQLite-Transaktion | dito |
| macOS-Durability | `fullfsync=ON` und `checkpoint_fullfsync=ON`. Keine übertragenen P0-Latenzwerte; gemessen wird in DN-MAC-04 | dito |
| Owner blockiert auf Netzwerk | Der Owner sendet Befehle an einen IO-Executor (einziger Signer) und verarbeitet die Ergebnisse seriell aus seiner Inbox. Private Ereignisse haben Vorrang vor Signalen | `dn-core/src/owner/mod.rs`, `dn-bitget/src/private/executor.rs` |
| Recovery: FILLED zu früh | FILLED nur mit Börsenstatus **und** vollständig gebuchten Fills **und** abgeglichener Position. Sonst bleibt UNKNOWN | `dn-types/src/order_fsm.rs` |
| Verspätete ACKs, Status vor Fills | Kein Rückschritt durch späte ACKs; „Status vor Fills“ fordert Fills nach | dito |
| Positionen: Snapshot-Scope, überlappende Polls | Generation pro Poll, dauerhaftes WS-Watermark je Schlüssel mit Tombstones. Der WS-Snapshot löscht nur in nachgewiesenem Scope | `dn-core/src/account/positions.rs` |
| Pagination-Endbedingung | Vertrag pro Endpoint (`EndRule`). Unbelegtes Verhalten ⇒ `Coverage::Unproven` ⇒ darf keinen Snapshot ersetzen | `dn-bitget/src/private/pagination.rs` |
| Market-Fill im Backtest | Liquidität nur aus gleichseitigen Aggressor-Trades, begrenzte Teilnahmequote, versionierte Annahmen, Teil- und `Indeterminate`-Ergebnisse | `dn-backtest/src/fill.rs` |
| Gleiche Millisekunde | Reihenfolge `(exch_ts_ms, source_seq)` nach P1-T26 | dito |
| Stop-Kette fehlt | Teilfill → Journal → Positions-Readback → Stopmenge → Stop-Readback → CONFIRMED_FULL, inklusive Ersatz-vor-Storno und Notschließung bei Fristablauf | `dn-core/src/protection/coverage.rs` + Owner |
| Risk unvollständig | Instrument-Version, gesamte offene Exposure (auch vor dem Senden und UNKNOWN), S1-Budget, Tagesverlust, Orderrate, Verlust am Stop | `dn-core/src/risk/s1.rs` |

## 1. Dateistruktur und Abhängigkeiten

```text
darknode/native/crates/
  dn-types/src/{lib.rs, trading.rs, order_fsm.rs, exec.rs}      reine Verträge, keine I/O
  dn-bitget/src/private/{auth.rs*, orders.rs*, pagination.rs, executor.rs}
  dn-store/src/order_journal.rs + sql/order_journal_v2.sql
  dn-core/src/{account/positions.rs, protection/coverage.rs, risk/s1.rs, owner/mod.rs}
  dn-backtest/src/fill.rs
  (* = unverändert aus V1)
```

Abhängigkeitsrichtung ohne Zyklen: `dn-types` ← `dn-store`, `dn-bitget`, `dn-core`, `dn-backtest`. `dn-core` hängt von `dn-store` und `darkbot-common` ab, nie von `dn-bitget`. Das Binary `darknode` verdrahtet alles. `dn-core` und `dn-backtest` sind die in CODE_UEBERNAHME_V1 §4 vorgesehenen logischen Gruppen; ob sie eigene Crates werden, entscheidet P0.

```bash
cargo add -p dn-store   rusqlite --features bundled   # gleiche SQLite auf macOS und Linux
cargo add -p dn-store   blake3
cargo add -p dn-core    blake3
cargo add -p dn-bitget  serde --features derive
cargo add -p dn-bitget  serde_json hmac sha2 base64 form_urlencoded   # JSON nur am Adapterrand
# Versionen beim Anlegen festlegen und über Cargo.lock pinnen; cargo-deny-Allowlist (CODE-00)
```

```toml
# dn-core/Cargo.toml und dn-bitget/Cargo.toml
[dependencies]
darkbot-common = { path = "../../../../project_0/native/crates/common" }  # bounded channel + Wakeup
dn-types = { path = "../dn-types" }
```

## 2. `dn-types/src/trading.rs`

```rust
//! Gemeinsame Handelstypen (P1-F01/F04). Ganzzahlig, keine Floats, kein JSON.
//! Zustandsnamen exakt nach P5_ORDER_STATE_UND_CRASH_MATRIX_V1 §1.

pub type Lots = i64;
pub type Ticks = i64;
pub type QuoteE8 = i128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub const fn as_str(self) -> &'static str {
        match self {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "buy" => Some(Side::Buy),
            "sell" => Some(Side::Sell),
            _ => None,
        }
    }
    pub const fn opposite(self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
    pub const fn sign(self) -> i64 {
        match self {
            Side::Buy => 1,
            Side::Sell => -1,
        }
    }
}

macro_rules! id_newtype {
    ($($name:ident),* $(,)?) => {$(
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub String);
        impl $name {
            pub fn as_str(&self) -> &str { &self.0 }
        }
    )*};
}
id_newtype!(IntentId, ClientOid, ExchangeOrderId, ExecId, ReservationId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstrumentVersion(pub u32);

/// Versionierter Instrument-Snapshot (P1.3, T22).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstrumentRules {
    pub instrument_id: u32,
    pub version: InstrumentVersion,
    pub tick_size_e8: i64, // BTCUSDT 0,1    → 10_000_000
    pub lot_size_e8: i64,  // BTCUSDT 0,0001 → 10_000
    pub min_lots: Lots,
    pub max_lots: Lots,
    pub max_market_lots: Lots,
    pub min_notional_e8: QuoteE8,
}

impl InstrumentRules {
    /// lots × lot_size × ticks × tick_size in Quote×1e8.
    /// Gilt für Preise (Nominalwert) und für Preisabstände (Verlust am Stop).
    pub fn value_e8(&self, lots: Lots, ticks: Ticks) -> Option<QuoteE8> {
        i128::from(lots)
            .checked_mul(i128::from(self.lot_size_e8))?
            .checked_mul(i128::from(ticks))?
            .checked_mul(i128::from(self.tick_size_e8))?
            .checked_div(100_000_000)
    }
}

/// Konto-/Risk-Achse (P5_ORDER_STATE §1). HALTED ist Default nach Neustart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountState {
    Uninitialized,
    Reconciling,
    Halted,
    Active,
    Reducing,
    EmergencyRecovery,
}

/// Order-Achse (P5_ORDER_STATE §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OrderState {
    Draft,
    Validated,
    Reserved,
    IntentDurable,
    SubmitPending,
    Acknowledged,
    Working,
    PartialFilled,
    Filled,
    CancelPending,
    Cancelled,
    Rejected,
    NotSent,
    Unknown,
}

impl OrderState {
    pub const ALL: [OrderState; 14] = [
        Self::Draft, Self::Validated, Self::Reserved, Self::IntentDurable, Self::SubmitPending,
        Self::Acknowledged, Self::Working, Self::PartialFilled, Self::Filled, Self::CancelPending,
        Self::Cancelled, Self::Rejected, Self::NotSent, Self::Unknown,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Validated => "VALIDATED",
            Self::Reserved => "RESERVED",
            Self::IntentDurable => "INTENT_DURABLE",
            Self::SubmitPending => "SUBMIT_PENDING",
            Self::Acknowledged => "ACKNOWLEDGED",
            Self::Working => "WORKING",
            Self::PartialFilled => "PARTIAL_FILLED",
            Self::Filled => "FILLED",
            Self::CancelPending => "CANCEL_PENDING",
            Self::Cancelled => "CANCELLED",
            Self::Rejected => "REJECTED",
            Self::NotSent => "NOT_SENT",
            Self::Unknown => "UNKNOWN",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|x| x.as_str() == s)
    }
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Filled | Self::Cancelled | Self::Rejected | Self::NotSent)
    }
    /// Kann noch Exposure erzeugen ⇒ Reservierung bleibt (Invariante 3).
    pub const fn may_still_fill(self) -> bool {
        matches!(
            self,
            Self::SubmitPending | Self::Acknowledged | Self::Working
                | Self::PartialFilled | Self::CancelPending | Self::Unknown
        )
    }
    /// Reserviert, aber nachweislich noch nicht an die IO-Schicht übergeben.
    pub const fn pre_send(self) -> bool {
        matches!(self, Self::Reserved | Self::IntentDurable)
    }
}

/// Schutz-Achse (P5_ORDER_STATE §1, StopCoverage).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopCoverage {
    Absent,
    Requested,
    ExchangeAckUnknown,
    ConfirmedPartial,
    ConfirmedFull,
    Stale,
    ProtectionUnknown,
    ClosingReconciling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerBy {
    Mark,
    Market,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Entry,
    Exit,
    Stop,
    EmergencyClose,
}

impl Purpose {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Entry => "ENTRY",
            Self::Exit => "EXIT",
            Self::Stop => "STOP",
            Self::EmergencyClose => "EMERGENCY_CLOSE",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Entry, Self::Exit, Self::Stop, Self::EmergencyClose]
            .into_iter()
            .find(|p| p.as_str() == s)
    }
}
```

## 3. `dn-types/src/order_fsm.rs`: Order-Zustandsmaschine

Liegt in `dn-types`, damit Journal und Owner dieselbe reine Funktion nutzen. Nur so ist die Fill-Buchung mit Zustandswechsel in einer Transaktion möglich, ohne Crate-Zyklus.

```rust
//! Reine Übergangsfunktion nach P5_ORDER_STATE_UND_CRASH_MATRIX_V1 §1–§3.
//! UNKNOWN ist sticky: nur vollständige Börsenbelege lösen ihn (K9).

use crate::trading::{Lots, OrderState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExchangeOrderTruth {
    Working { exchange_filled: Lots },
    Filled { exchange_filled: Lots },
    Cancelled { exchange_filled: Lots },
    Rejected,
    /// clientOid nach Ablauf des per Demo festgelegten Fensters nicht vorhanden (C05/C15).
    AbsentAfterWindow,
}

impl ExchangeOrderTruth {
    pub fn exchange_filled(&self) -> Lots {
        match self {
            Self::Working { exchange_filled }
            | Self::Filled { exchange_filled }
            | Self::Cancelled { exchange_filled } => *exchange_filled,
            Self::Rejected | Self::AbsentAfterWindow => 0,
        }
    }
}

/// Börsenwahrheit + eigene Buchung + Positionsabgleich.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileEvidence {
    pub truth: ExchangeOrderTruth,
    pub order_lots: Lots,
    /// Summe im Fill-Journal (APPLIED + CLOSED_ORDER), nie aus Statusfeldern.
    pub booked_lots: Lots,
    /// Vollständiger REST-Positions-Snapshot nach dem letzten Fill dieser Order ohne Konflikt.
    pub position_reconciled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderEvent {
    Validated,
    Reserved,
    IntentDurable,
    SubmitStarted,
    /// Owner hat Beweis: Befehl erreichte die IO-Schicht nie (Queue abgelehnt, Neustart vor SUBMIT_PENDING).
    ProvenNotSent,
    ExchangeAccepted,
    ExchangeWorking,
    FillBooked { booked_cum: Lots, order_lots: Lots },
    CancelRequested,
    /// CancelAck ≠ Cancelled (Invariante aus §1).
    CancelAcked,
    ExchangeRejected,
    TransportUnknown,
    /// Status meldet mehr Ausführung als gebucht ⇒ Fills nachlesen.
    StatusAheadOfFills,
    Reconciled(ReconcileEvidence),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transition {
    To(OrderState),
    /// Zustand bleibt; Belege nachfordern.
    Stay { reason: &'static str },
    /// Widerspruch ⇒ Order UNKNOWN, Konto RECONCILING, Alarm.
    Conflict { reason: &'static str },
}

pub fn next(state: OrderState, ev: &OrderEvent) -> Transition {
    use OrderEvent as E;
    use OrderState as S;
    use Transition::{Conflict, Stay, To};

    match ev {
        E::FillBooked { booked_cum, order_lots } => on_fill(state, *booked_cum, *order_lots),
        E::Reconciled(evidence) => on_reconcile(state, evidence),
        E::ExchangeAccepted => match state {
            S::SubmitPending => To(S::Acknowledged),
            // Verspätetes ACK: niemals Rückschritt.
            S::Acknowledged | S::Working | S::PartialFilled | S::Filled | S::CancelPending | S::Cancelled => To(state),
            S::Unknown => Stay { reason: "LATE_ACK_EVIDENCE_ONLY" },
            S::Rejected | S::NotSent => Conflict { reason: "ACK_ON_CLOSED_ORDER" },
            S::Draft | S::Validated | S::Reserved | S::IntentDurable => Conflict { reason: "ACK_BEFORE_SEND" },
        },
        E::ExchangeWorking => match state {
            S::SubmitPending | S::Acknowledged => To(S::Working), // WS-Order-Event vor REST-ACK
            S::Working | S::PartialFilled | S::CancelPending => To(state),
            S::Unknown => Stay { reason: "WORKING_EVIDENCE_ONLY" },
            _ => Conflict { reason: "WORKING_IN_WRONG_STATE" },
        },
        E::StatusAheadOfFills => {
            if state.may_still_fill() {
                Stay { reason: "FETCH_FILLS" }
            } else {
                Conflict { reason: "STATUS_AHEAD_ON_CLOSED_ORDER" }
            }
        }
        E::CancelAcked => match state {
            S::CancelPending | S::Unknown => Stay { reason: "CANCEL_ACK_IS_NOT_CANCELLED" },
            S::Filled | S::Cancelled => To(state),
            _ => Conflict { reason: "CANCEL_ACK_IN_WRONG_STATE" },
        },
        _ => match (state, ev) {
            (S::Draft, E::Validated) => To(S::Validated),
            (S::Validated, E::Reserved) => To(S::Reserved),
            (S::Reserved, E::IntentDurable) => To(S::IntentDurable),
            (S::IntentDurable, E::SubmitStarted) => To(S::SubmitPending),
            (S::IntentDurable | S::SubmitPending, E::ProvenNotSent) => To(S::NotSent),
            (S::SubmitPending, E::ExchangeRejected) => To(S::Rejected),
            (S::Acknowledged | S::Working | S::PartialFilled, E::CancelRequested) => To(S::CancelPending),
            (s, E::TransportUnknown) if s.may_still_fill() => To(S::Unknown),
            (S::Unknown, _) => Stay { reason: "UNKNOWN_IS_STICKY" },
            _ => Conflict { reason: "ILLEGAL_TRANSITION" },
        },
    }
}

fn on_fill(state: OrderState, cum: Lots, order: Lots) -> Transition {
    use OrderState as S;
    use Transition::{Conflict, Stay, To};
    if cum <= 0 {
        return Conflict { reason: "FILL_NON_POSITIVE" };
    }
    if cum > order {
        return Conflict { reason: "OVERFILL" };
    }
    let full = cum == order;
    match state {
        S::Unknown => Stay { reason: "FILL_BOOKED_STATE_UNKNOWN" },
        // Ein vollstaendig lokal gebuchter Fill ist noch keine bestaetigte Exchange-Position.
        // FILLED gibt es ausschliesslich nach Reconciled(Exchange Filled + Journal + Position).
        S::SubmitPending | S::Acknowledged | S::Working | S::PartialFilled => {
            To(S::PartialFilled)
        }
        S::CancelPending if full => Stay { reason: "FULL_FILL_PENDING_RECONCILE" },
        S::CancelPending => Stay { reason: "LATE_FILL_DURING_CANCEL" },
        S::Filled => Conflict { reason: "FILL_AFTER_FILLED" },
        S::Cancelled | S::Rejected | S::NotSent => Conflict { reason: "FILL_ON_CLOSED_ORDER" },
        S::Draft | S::Validated | S::Reserved | S::IntentDurable => Conflict { reason: "FILL_BEFORE_SEND" },
    }
}

fn on_reconcile(state: OrderState, e: &ReconcileEvidence) -> Transition {
    use ExchangeOrderTruth as T;
    use OrderState as S;
    use Transition::{Conflict, Stay, To};
    if !state.may_still_fill() {
        return Conflict { reason: "RECONCILE_ON_CLOSED_ORDER" };
    }
    if !e.position_reconciled {
        return Stay { reason: "POSITION_NOT_RECONCILED" };
    }
    let ex = e.truth.exchange_filled();
    if e.booked_lots < ex {
        return Stay { reason: "FILLS_MISSING_IN_JOURNAL" };
    }
    if e.booked_lots > ex {
        return Conflict { reason: "JOURNAL_EXCEEDS_EXCHANGE" };
    }
    match &e.truth {
        T::Working { .. } if ex == 0 => To(S::Working),
        T::Working { .. } => To(S::PartialFilled),
        T::Filled { .. } if ex == e.order_lots => To(S::Filled),
        T::Filled { .. } => Conflict { reason: "FILLED_QTY_MISMATCH" },
        T::Cancelled { .. } => To(S::Cancelled),
        T::Rejected => To(S::Rejected),
        T::AbsentAfterWindow => To(S::NotSent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use OrderEvent as E;
    use OrderState as S;
    use Transition::{Conflict, Stay, To};

    fn ev(truth: ExchangeOrderTruth, booked: Lots, pos_ok: bool) -> E {
        E::Reconciled(ReconcileEvidence { truth, order_lots: 10, booked_lots: booked, position_reconciled: pos_ok })
    }

    /// DN-P5-UP-016 (K9): FILLED nur mit vollständigen Fills und abgeglichener Position.
    #[test]
    fn filled_requires_booked_fills_and_position() {
        let t = || ExchangeOrderTruth::Filled { exchange_filled: 10 };
        assert_eq!(next(S::Unknown, &ev(t(), 4, true)), Stay { reason: "FILLS_MISSING_IN_JOURNAL" });
        assert_eq!(next(S::Unknown, &ev(t(), 10, false)), Stay { reason: "POSITION_NOT_RECONCILED" });
        assert_eq!(next(S::Unknown, &ev(t(), 10, true)), To(S::Filled));
    }

    /// DN-P5-UP-017: verspätetes ACK nach Teilfill ⇒ kein Rückschritt.
    #[test]
    fn late_ack_does_not_regress() {
        assert_eq!(next(S::PartialFilled, &E::ExchangeAccepted), To(S::PartialFilled));
        assert_eq!(next(S::Unknown, &E::ExchangeAccepted), Stay { reason: "LATE_ACK_EVIDENCE_ONLY" });
    }

    /// C05: verlorene Antwort ⇒ UNKNOWN, sticky.
    #[test]
    fn lost_answer_is_sticky() {
        assert_eq!(next(S::SubmitPending, &E::TransportUnknown), To(S::Unknown));
        assert_eq!(next(S::Unknown, &E::CancelRequested), Stay { reason: "UNKNOWN_IS_STICKY" });
    }

    /// C09 / P5-A22
    #[test]
    fn late_fill_during_cancel_and_after_cancel() {
        assert_eq!(next(S::CancelPending, &E::FillBooked { booked_cum: 3, order_lots: 10 }), Stay { reason: "LATE_FILL_DURING_CANCEL" });
        assert_eq!(next(S::Cancelled, &E::FillBooked { booked_cum: 3, order_lots: 10 }), Conflict { reason: "FILL_ON_CLOSED_ORDER" });
    }
}
```

## 4. `dn-types/src/exec.rs`: Verträge zwischen Owner, IO-Executor und Journal

```rust
//! Typisierte Nachrichten. JSON entsteht und endet ausschließlich im Bitget-Adapter (DN-D18).

use crate::order_fsm::ExchangeOrderTruth;
use crate::trading::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetStop {
    pub trigger_ticks: Ticks,
    pub trigger_by: TriggerBy,
}

/// V1: One-way, Isolated, Market. Margin-Modus fest im Adapter (K11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedOrder {
    pub intent_id: IntentId,
    pub client_oid: ClientOid,
    pub instrument_version: InstrumentVersion,
    pub side: Side,
    pub qty_lots: Lots,
    pub reduce_only: bool,
    pub purpose: Purpose,
    /// Bei Entry: Preset-SL; bei Purpose::Stop: Triggerpreis der Schutzorder.
    pub preset_stop: Option<PresetStop>,
}

impl TypedOrder {
    /// Kanonische Bytes für `request_hash` (BLAKE3). Feste Reihenfolge, kein JSON.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(96);
        for s in [self.intent_id.as_str(), self.client_oid.as_str()] {
            v.extend_from_slice(&(s.len() as u32).to_le_bytes());
            v.extend_from_slice(s.as_bytes());
        }
        v.extend_from_slice(&self.instrument_version.0.to_le_bytes());
        v.push(self.side as u8);
        v.extend_from_slice(&self.qty_lots.to_le_bytes());
        v.push(u8::from(self.reduce_only));
        v.push(self.purpose as u8);
        match &self.preset_stop {
            None => v.push(0),
            Some(p) => {
                v.push(1);
                v.extend_from_slice(&p.trigger_ticks.to_le_bytes());
                v.push(p.trigger_by as u8);
            }
        }
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillSource {
    Ws,
    Rest,
}

impl FillSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ws => "ws",
            Self::Rest => "rest",
        }
    }
}

/// Eigener Fill. Dezimalwerte vom Adapter kanonisiert (Mantisse ohne nachlaufende Nullen).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrivateFill {
    pub exec_id: ExecId,
    pub client_oid: Option<ClientOid>,
    pub exchange_order_id: Option<ExchangeOrderId>,
    pub side: Side,
    pub qty_lots: Lots,
    pub price_ticks: Ticks,
    pub fee_mantissa: i64,
    pub fee_scale: i32,
    pub fee_coin: String,
    pub exch_ts_ms: i64,
    pub source: FillSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PositionKey {
    pub category: String,
    pub symbol: String,
    pub pos_side: String, // wie geliefert; One-way-Darstellung per Demo (P5-A05)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExchangePosition {
    pub key: PositionKey,
    /// Vorzeichenbehaftet: Long > 0. REST `total` / WS `size` (K7).
    pub size_lots: Lots,
    pub avg_price_ticks: Ticks,
    pub leverage_e2: i64,
    pub liq_price_ticks: Option<Ticks>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopQty {
    Lots(Lots),
    WholePosition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopReadback {
    pub exchange_id: String,
    pub client_oid: Option<ClientOid>,
    pub side: Side,
    pub reduce_only: bool,
    pub trigger_ticks: Ticks,
    pub trigger_by: TriggerBy,
    pub qty: StopQty,
    pub live: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coverage {
    /// Alle Seiten nach belegtem Endpoint-Vertrag gelesen.
    Complete,
    /// Vollständigkeit nicht beweisbar ⇒ darf keinen Zustand ersetzen.
    Unproven,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot<T> {
    pub rows: Vec<T>,
    pub coverage: Coverage,
    pub pages: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WsAction {
    Snapshot,
    Update,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusHint {
    Working,
    PartiallyFilled,
    Filled,
    Cancelled,
    Unrecognized(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrivateEvent {
    Fill(PrivateFill),
    Order {
        client_oid: Option<ClientOid>,
        exchange_order_id: ExchangeOrderId,
        status: StatusHint,
        exchange_filled: Lots,
    },
    Position { action: WsAction, entries: Vec<ExchangePosition> },
    /// WS-Hinweis; ersetzt KEINEN vollständigen Stop-Readback.
    StopHint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IoCommand {
    Place(TypedOrder),
    PlaceStop(TypedOrder), // tpsl mit clientOid (V1-Typ)
    CancelStop { exchange_id: String },
    Lookup { intent_id: IntentId, client_oid: ClientOid, exchange_order_id: Option<ExchangeOrderId> },
    ReadFills { intent_id: IntentId, exchange_order_id: ExchangeOrderId },
    ReadPositions,
    ReadStops,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendResult {
    Accepted { exchange_order_id: ExchangeOrderId },
    /// Nur für Codes der per Demo verifizierten Liste „definitiv nicht platziert“.
    Rejected { code: String },
    /// Timeout, Verbindungsfehler, 5xx, unlesbar, nicht verifizierter Code.
    NoDefinitiveAnswer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IoResult {
    Placed { intent_id: IntentId, result: SendResult },
    LookupDone {
        intent_id: IntentId,
        exchange_order_id: Option<ExchangeOrderId>,
        truth: Result<ExchangeOrderTruth, String>,
    },
    Fills { intent_id: IntentId, fills: Result<Snapshot<PrivateFill>, String> },
    Positions(Result<Snapshot<ExchangePosition>, String>),
    Stops { read_at_ms: i64, stops: Result<Snapshot<StopReadback>, String> },
    StopCancelled { exchange_id: String, ok: bool },
}
```

## 5. `dn-bitget/src/private/pagination.rs`: Vertrag pro Endpoint

```rust
//! Cursor-Pagination Bitget V3. Endbedingung ist ENDPOINT-spezifisch und belegpflichtig.
//! Herkunft der Grundsemantik: Hummingbot `_paginated_rows` @06e491ca (Apache-2.0).

use dn_types::exec::{Coverage, Snapshot};
use std::collections::HashSet;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndRule {
    /// Demo belegt: Endpoint hält `limit` ein ⇒ Ende bei kurzer Seite oder ohne Cursor.
    ShortPageOrNoCursor { evidence_run_id: &'static str },
    /// Unbelegt ⇒ Ende nur bei leerer Seite oder ohne Cursor; Ergebnis bleibt Unproven.
    Unverified,
}

#[derive(Clone, Copy, Debug)]
pub struct PageContract {
    pub endpoint_key: &'static str, // Registry-Schlüssel
    pub limit: usize,
    pub max_pages: usize,
    pub end_rule: EndRule,
}

pub struct Page<T> {
    pub rows: Vec<T>,
    pub cursor: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PageError {
    InvalidContract,
    Request(String),
    RepeatedCursor,
    PageLimitReached { pages: usize, rows: usize },
}

impl fmt::Display for PageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

pub fn fetch_all<T, E: fmt::Display>(
    c: &PageContract,
    mut fetch: impl FnMut(Option<&str>, usize) -> Result<Page<T>, E>,
) -> Result<Snapshot<T>, PageError> {
    if c.limit == 0 || c.max_pages == 0 {
        return Err(PageError::InvalidContract);
    }
    let mut rows = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen = HashSet::new();
    let mut limit_violated = false;

    for page_no in 1..=c.max_pages {
        let page = fetch(cursor.as_deref(), c.limit).map_err(|e| PageError::Request(e.to_string()))?;
        let n = page.rows.len();
        limit_violated |= n > c.limit; // Endpoint ignoriert limit ⇒ Vertrag verletzt
        rows.extend(page.rows);
        let next = page.cursor.filter(|s| !s.is_empty());

        let ended = match c.end_rule {
            EndRule::ShortPageOrNoCursor { .. } => n < c.limit || next.is_none(),
            EndRule::Unverified => n == 0 || next.is_none(),
        };
        if ended {
            let proven = matches!(c.end_rule, EndRule::ShortPageOrNoCursor { .. }) && !limit_violated;
            return Ok(Snapshot {
                rows,
                coverage: if proven { Coverage::Complete } else { Coverage::Unproven },
                pages: page_no,
            });
        }
        if let Some(cur) = next {
            if !seen.insert(cur.clone()) {
                return Err(PageError::RepeatedCursor);
            }
            cursor = Some(cur);
        }
    }
    Err(PageError::PageLimitReached { pages: c.max_pages, rows: rows.len() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROVEN: PageContract = PageContract {
        endpoint_key: "bitget.v3.position.current",
        limit: 2,
        max_pages: 10,
        end_rule: EndRule::ShortPageOrNoCursor { evidence_run_id: "TEST" },
    };

    fn page(rows: Vec<u32>, cur: Option<&str>) -> Page<u32> {
        Page { rows, cursor: cur.map(str::to_owned) }
    }

    /// DN-P5-UP-001/002
    #[test]
    fn proven_contract_reads_all_pages_and_ends_on_short_page() {
        let mut it = vec![page(vec![1, 2], Some("a")), page(vec![3], Some("b"))].into_iter();
        let s = fetch_all(&PROVEN, |_, _| -> Result<Page<u32>, String> { Ok(it.next().unwrap()) }).unwrap();
        assert_eq!((s.rows, s.coverage, s.pages), (vec![1, 2, 3], Coverage::Complete, 2));
    }

    /// DN-P5-UP-018: unbelegter Endpoint ⇒ nie Complete.
    #[test]
    fn unverified_contract_is_unproven() {
        let c = PageContract { end_rule: EndRule::Unverified, ..PROVEN };
        let mut it = vec![page(vec![1], Some("a")), page(vec![], Some("b"))].into_iter();
        let s = fetch_all(&c, |_, _| -> Result<Page<u32>, String> { Ok(it.next().unwrap()) }).unwrap();
        assert_eq!(s.coverage, Coverage::Unproven);
    }

    /// Endpoint liefert mehr als limit ⇒ Vertrag verletzt ⇒ Unproven.
    #[test]
    fn limit_violation_is_unproven() {
        let s = fetch_all(&PROVEN, |_, _| -> Result<Page<u32>, String> { Ok(page(vec![1, 2, 3], None)) }).unwrap();
        assert_eq!(s.coverage, Coverage::Unproven);
    }

    /// DN-P5-UP-003/004
    #[test]
    fn repeated_cursor_and_page_limit_fail_closed() {
        let r = fetch_all(&PROVEN, |_, _| -> Result<Page<u32>, String> { Ok(page(vec![1, 2], Some("x"))) });
        assert_eq!(r, Err(PageError::RepeatedCursor));
    }
}
```

## 6. `dn-store/sql/order_journal_v2.sql`

```sql
CREATE TABLE IF NOT EXISTS order_intent (
  intent_id          TEXT PRIMARY KEY,
  client_oid         TEXT NOT NULL UNIQUE,
  exchange_order_id  TEXT UNIQUE,
  bot_id             TEXT NOT NULL,
  graph_hash         BLOB NOT NULL,
  account_uid        TEXT NOT NULL,
  instrument_id      INTEGER NOT NULL,
  instrument_version INTEGER NOT NULL,
  category           TEXT NOT NULL,
  symbol             TEXT NOT NULL,
  side               TEXT NOT NULL CHECK (side IN ('buy','sell')),
  reduce_only        INTEGER NOT NULL CHECK (reduce_only IN (0,1)),
  purpose            TEXT NOT NULL CHECK (purpose IN ('ENTRY','EXIT','STOP','EMERGENCY_CLOSE')),
  order_type         TEXT NOT NULL,
  qty_lots           INTEGER NOT NULL CHECK (qty_lots > 0),
  ref_price_ticks    INTEGER NOT NULL,
  stop_ticks         INTEGER,
  request_hash       BLOB NOT NULL,
  state              TEXT NOT NULL,
  filled_lots        INTEGER NOT NULL DEFAULT 0 CHECK (filled_lots >= 0 AND filled_lots <= qty_lots),
  created_wall_ms    INTEGER NOT NULL,
  updated_wall_ms    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS risk_reservation (
  reservation_id  TEXT PRIMARY KEY,
  intent_id       TEXT NOT NULL UNIQUE REFERENCES order_intent(intent_id),
  side            TEXT NOT NULL,
  reserved_lots   INTEGER NOT NULL CHECK (reserved_lots > 0),
  notional_e8     TEXT NOT NULL,   -- i128 als Dezimaltext (SQLite hat kein i128)
  status          TEXT NOT NULL CHECK (status IN ('ACTIVE','RELEASED'))
);

CREATE TABLE IF NOT EXISTS order_transition (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  intent_id   TEXT NOT NULL REFERENCES order_intent(intent_id),
  from_state  TEXT NOT NULL,
  to_state    TEXT NOT NULL,
  cause       TEXT NOT NULL,
  at_wall_ms  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS fill (
  exec_id            TEXT PRIMARY KEY,
  intent_id          TEXT REFERENCES order_intent(intent_id),
  client_oid         TEXT,
  exchange_order_id  TEXT,
  side               TEXT NOT NULL,
  qty_lots           INTEGER NOT NULL CHECK (qty_lots > 0),
  price_ticks        INTEGER NOT NULL,
  fee_mantissa       INTEGER NOT NULL,
  fee_scale          INTEGER NOT NULL,
  fee_coin           TEXT NOT NULL,
  exch_ts_ms         INTEGER NOT NULL,
  resolution         TEXT NOT NULL CHECK (resolution IN
                       ('APPLIED','ORPHAN','IDENTITY_CONFLICT','OVERFILL','CLOSED_ORDER')),
  booked_wall_ms     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS fill_observation (
  exec_id           TEXT NOT NULL REFERENCES fill(exec_id),
  source            TEXT NOT NULL CHECK (source IN ('ws','rest')),
  observed_wall_ms  INTEGER NOT NULL,
  PRIMARY KEY (exec_id, source)
);

CREATE TABLE IF NOT EXISTS account_block (
  block_id         INTEGER PRIMARY KEY AUTOINCREMENT,
  reason           TEXT NOT NULL,
  intent_id        TEXT,
  exec_id          TEXT,
  opened_wall_ms   INTEGER NOT NULL,
  cleared_wall_ms  INTEGER
);
```

## 7. `dn-store/src/order_journal.rs`

```rust
//! Dauerhaftes Order-/Fill-Journal (P5 Invariante 2). Eigene Lane, nie der Market-Writer (K17).
//! Jede Zustandsänderung = eine IMMEDIATE-Transaktion = ein Commit mit synchronous=FULL.
//! macOS: fullfsync erzwingt F_FULLFSYNC (fsync leert dort den Laufwerkscache nicht).
//! Latenz wird für DarkNode in DN-MAC-04 gemessen; P0-Werte werden nicht übertragen.

use dn_types::exec::{PrivateFill, TypedOrder};
use dn_types::order_fsm::{next, OrderEvent, Transition};
use dn_types::trading::*;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::path::Path;

const SCHEMA: &str = include_str!("../sql/order_journal_v2.sql");

#[derive(Debug)]
pub enum JournalError {
    Sql(rusqlite::Error),
    NotWal(String),
    Overflow,
    Corrupt(&'static str),
    InvalidFill(&'static str),
    UnknownIntent,
}

impl From<rusqlite::Error> for JournalError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sql(e)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FillBooking {
    Applied { intent_id: IntentId, booked_cum: Lots, order_lots: Lots, state: OrderState, note: Option<&'static str> },
    ClosedOrderFill { intent_id: IntentId, reason: &'static str },
    Duplicate { confirmed_by_new_source: bool },
    PayloadConflict,
    IdentityConflict { reason: &'static str },
    Orphan,
    Overfill { intent_id: IntentId },
}

impl FillBooking {
    /// P5 Invariante 5: alles Unklare ⇒ NO_ENTRY / RECONCILING.
    pub fn requires_reconcile(&self) -> bool {
        match self {
            Self::Duplicate { .. } => false,
            Self::Applied { note, .. } => matches!(note, Some("FILL_BOOKED_STATE_UNKNOWN")),
            _ => true,
        }
    }
    pub fn changes_position(&self) -> bool {
        matches!(self, Self::Applied { .. } | Self::ClosedOrderFill { .. } | Self::Overfill { .. })
    }
}

pub struct NewIntent {
    pub order: TypedOrder,
    pub bot_id: String,
    pub graph_hash: [u8; 32],
    pub account_uid: String,
    pub instrument_id: u32,
    pub category: String,
    pub symbol: String,
    pub order_type: &'static str,
    pub ref_price_ticks: Ticks,
    pub stop_ticks: Option<Ticks>,
}

pub struct NewReservation {
    pub reservation_id: ReservationId,
    pub lots: Lots,
    pub notional_e8: QuoteE8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenIntent {
    pub intent_id: IntentId,
    pub client_oid: ClientOid,
    pub exchange_order_id: Option<ExchangeOrderId>,
    pub side: Side,
    pub reduce_only: bool,
    pub purpose: Purpose,
    pub qty_lots: Lots,
    pub filled_lots: Lots,
    pub state: OrderState,
}

pub struct OrderJournal {
    conn: Connection,
}

impl OrderJournal {
    pub fn open(path: &Path) -> Result<Self, JournalError> {
        let conn = Connection::open(path)?;
        let mode: String = conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(JournalError::NotWal(mode));
        }
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "fullfsync", "ON")?;            // nur macOS wirksam
        conn.pragma_update(None, "checkpoint_fullfsync", "ON")?; // nur macOS wirksam
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// C01–C02: Intent + Reservierung + Übergänge DRAFT→…→INTENT_DURABLE atomar.
    pub fn commit_intent(&mut self, i: &NewIntent, r: &NewReservation, now: i64) -> Result<(), JournalError> {
        let o = &i.order;
        let hash = blake3::hash(&o.canonical_bytes());
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO order_intent (intent_id, client_oid, bot_id, graph_hash, account_uid, instrument_id,
               instrument_version, category, symbol, side, reduce_only, purpose, order_type, qty_lots,
               ref_price_ticks, stop_ticks, request_hash, state, created_wall_ms, updated_wall_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?19)",
            params![
                o.intent_id.as_str(), o.client_oid.as_str(), i.bot_id, &i.graph_hash[..], i.account_uid,
                i.instrument_id, o.instrument_version.0, i.category, i.symbol, o.side.as_str(),
                o.reduce_only, o.purpose.as_str(), i.order_type, o.qty_lots, i.ref_price_ticks,
                i.stop_ticks, &hash.as_bytes()[..], OrderState::IntentDurable.as_str(), now
            ],
        )?;
        tx.execute(
            "INSERT INTO risk_reservation (reservation_id, intent_id, side, reserved_lots, notional_e8, status)
             VALUES (?1,?2,?3,?4,?5,'ACTIVE')",
            params![r.reservation_id.as_str(), o.intent_id.as_str(), o.side.as_str(), r.lots, r.notional_e8.to_string()],
        )?;
        for (from, to) in [
            (OrderState::Draft, OrderState::Validated),
            (OrderState::Validated, OrderState::Reserved),
            (OrderState::Reserved, OrderState::IntentDurable),
        ] {
            insert_transition(&tx, &o.intent_id, from, to, "COMMIT_INTENT", now)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn apply_event(&mut self, id: &IntentId, ev: &OrderEvent, cause: &str, now: i64) -> Result<Transition, JournalError> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let tr = apply_event_tx(&tx, id, ev, cause, now)?;
        tx.commit()?;
        Ok(tr)
    }

    /// ACK mit Börsen-ID. Gehört die ID schon einer anderen Order ⇒ Konflikt.
    pub fn apply_ack(&mut self, id: &IntentId, xid: &ExchangeOrderId, now: i64) -> Result<Transition, JournalError> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let owner: Option<String> = tx
            .query_row("SELECT intent_id FROM order_intent WHERE exchange_order_id = ?1", params![xid.as_str()], |r| r.get(0))
            .optional()?;
        let tr = match owner {
            Some(other) if other != id.0 => conflict(&tx, id, load_state(&tx, id)?, "ACK_ID_BELONGS_TO_OTHER_INTENT", now)?,
            _ => {
                let n = tx.execute(
                    "UPDATE order_intent SET exchange_order_id = ?2
                     WHERE intent_id = ?1 AND (exchange_order_id IS NULL OR exchange_order_id = ?2)",
                    params![id.as_str(), xid.as_str()],
                )?;
                if n == 0 {
                    conflict(&tx, id, load_state(&tx, id)?, "ACK_ID_DIFFERS_FROM_KNOWN", now)?
                } else {
                    apply_event_tx(&tx, id, &OrderEvent::ExchangeAccepted, "ACK", now)?
                }
            }
        };
        tx.commit()?;
        Ok(tr)
    }

    /// Fill-Buchung: Dedupe, getrennte Identität, Überfüllung, Zustand + Reservierung – atomar.
    pub fn book_fill(&mut self, f: &PrivateFill, now: i64) -> Result<FillBooking, JournalError> {
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = book_fill_tx(&tx, f, now)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn open_intents(&self) -> Result<Vec<OpenIntent>, JournalError> {
        let mut st = self.conn.prepare(
            "SELECT intent_id, client_oid, exchange_order_id, side, reduce_only, purpose, qty_lots, filled_lots, state
             FROM order_intent WHERE state NOT IN ('FILLED','CANCELLED','REJECTED','NOT_SENT')",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?, r.get::<_, bool>(4)?, r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?, r.get::<_, i64>(7)?, r.get::<_, String>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, oid, xid, side, ro, purpose, qty, filled, state) = row?;
            out.push(OpenIntent {
                intent_id: IntentId(id),
                client_oid: ClientOid(oid),
                exchange_order_id: xid.map(ExchangeOrderId),
                side: Side::parse(&side).ok_or(JournalError::Corrupt("SIDE"))?,
                reduce_only: ro,
                purpose: Purpose::parse(&purpose).ok_or(JournalError::Corrupt("PURPOSE"))?,
                qty_lots: qty,
                filled_lots: filled,
                state: OrderState::parse(&state).ok_or(JournalError::Corrupt("STATE"))?,
            });
        }
        Ok(out)
    }

    pub fn open_block_count(&self) -> Result<i64, JournalError> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM account_block WHERE cleared_wall_ms IS NULL", [], |r| r.get(0))?)
    }

    /// Nur nach vollständigem Reconcile durch den Owner (P5 S5: ACTIVE nur nach vollem Readback).
    pub fn clear_blocks(&mut self, now: i64) -> Result<(), JournalError> {
        self.conn.execute("UPDATE account_block SET cleared_wall_ms = ?1 WHERE cleared_wall_ms IS NULL", params![now])?;
        Ok(())
    }
}

// ---------- interne Funktionen (alle innerhalb einer offenen Transaktion) ----------

struct IntentRow {
    intent_id: IntentId,
    exchange_order_id: Option<String>,
    side: Side,
    qty_lots: Lots,
    filled_lots: Lots,
    state: OrderState,
}

enum IntentKey {
    ClientOid,
    ExchangeOrderId,
}

fn load_intent(tx: &Transaction<'_>, key: IntentKey, v: &str) -> Result<Option<IntentRow>, JournalError> {
    let sql = match key {
        IntentKey::ClientOid => "SELECT intent_id, exchange_order_id, side, qty_lots, filled_lots, state FROM order_intent WHERE client_oid = ?1",
        IntentKey::ExchangeOrderId => "SELECT intent_id, exchange_order_id, side, qty_lots, filled_lots, state FROM order_intent WHERE exchange_order_id = ?1",
    };
    let row = tx
        .query_row(sql, params![v], |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, String>(5)?,
            ))
        })
        .optional()?;
    let Some((id, xid, side, qty, filled, state)) = row else {
        return Ok(None);
    };
    Ok(Some(IntentRow {
        intent_id: IntentId(id),
        exchange_order_id: xid,
        side: Side::parse(&side).ok_or(JournalError::Corrupt("SIDE"))?,
        qty_lots: qty,
        filled_lots: filled,
        state: OrderState::parse(&state).ok_or(JournalError::Corrupt("STATE"))?,
    }))
}

fn load_state(tx: &Transaction<'_>, id: &IntentId) -> Result<OrderState, JournalError> {
    let s: Option<String> = tx
        .query_row("SELECT state FROM order_intent WHERE intent_id = ?1", params![id.as_str()], |r| r.get(0))
        .optional()?;
    let s = s.ok_or(JournalError::UnknownIntent)?;
    OrderState::parse(&s).ok_or(JournalError::Corrupt("STATE"))
}

fn insert_transition(tx: &Transaction<'_>, id: &IntentId, from: OrderState, to: OrderState, cause: &str, now: i64) -> Result<(), JournalError> {
    tx.execute(
        "INSERT INTO order_transition (intent_id, from_state, to_state, cause, at_wall_ms) VALUES (?1,?2,?3,?4,?5)",
        params![id.as_str(), from.as_str(), to.as_str(), cause, now],
    )?;
    Ok(())
}

fn set_state(tx: &Transaction<'_>, id: &IntentId, from: OrderState, to: OrderState, cause: &str, now: i64) -> Result<(), JournalError> {
    tx.execute(
        "UPDATE order_intent SET state = ?2, updated_wall_ms = ?3 WHERE intent_id = ?1",
        params![id.as_str(), to.as_str(), now],
    )?;
    insert_transition(tx, id, from, to, cause, now)
}

fn release_reservation(tx: &Transaction<'_>, id: &IntentId) -> Result<(), JournalError> {
    tx.execute(
        "UPDATE risk_reservation SET status = 'RELEASED' WHERE intent_id = ?1 AND status = 'ACTIVE'",
        params![id.as_str()],
    )?;
    Ok(())
}

fn block(tx: &Transaction<'_>, reason: &str, intent: Option<&IntentId>, exec: Option<&ExecId>, now: i64) -> Result<(), JournalError> {
    tx.execute(
        "INSERT INTO account_block (reason, intent_id, exec_id, opened_wall_ms) VALUES (?1,?2,?3,?4)",
        params![reason, intent.map(|i| i.as_str()), exec.map(|e| e.as_str()), now],
    )?;
    Ok(())
}

fn conflict(tx: &Transaction<'_>, id: &IntentId, from: OrderState, reason: &'static str, now: i64) -> Result<Transition, JournalError> {
    set_state(tx, id, from, OrderState::Unknown, reason, now)?;
    block(tx, reason, Some(id), None, now)?;
    Ok(Transition::Conflict { reason })
}

fn apply_event_tx(tx: &Transaction<'_>, id: &IntentId, ev: &OrderEvent, cause: &str, now: i64) -> Result<Transition, JournalError> {
    let cur = load_state(tx, id)?;
    let tr = next(cur, ev);
    match &tr {
        Transition::To(s) if *s != cur => {
            set_state(tx, id, cur, *s, cause, now)?;
            if s.is_terminal() {
                release_reservation(tx, id)?;
            }
        }
        Transition::To(_) | Transition::Stay { .. } => {}
        Transition::Conflict { reason } => {
            conflict(tx, id, cur, reason, now)?;
        }
    }
    Ok(tr)
}

struct FillRow {
    client_oid: Option<String>,
    exchange_order_id: Option<String>,
    side: String,
    qty: i64,
    price: i64,
    fee_m: i64,
    fee_s: i32,
    fee_coin: String,
    ts: i64,
}

impl FillRow {
    /// Alle wirtschaftlich relevanten Felder + Orderidentität. Die Quelle (WS/REST) zählt nicht.
    fn same_economics(&self, f: &PrivateFill) -> bool {
        fn compatible(a: &Option<String>, b: Option<&str>) -> bool {
            match (a.as_deref(), b) {
                (Some(x), Some(y)) => x == y,
                _ => true,
            }
        }
        self.side == f.side.as_str()
            && self.qty == f.qty_lots
            && self.price == f.price_ticks
            && self.fee_m == f.fee_mantissa
            && self.fee_s == f.fee_scale
            && self.fee_coin == f.fee_coin
            && self.ts == f.exch_ts_ms
            && compatible(&self.client_oid, f.client_oid.as_ref().map(|c| c.as_str()))
            && compatible(&self.exchange_order_id, f.exchange_order_id.as_ref().map(|x| x.as_str()))
    }
}

fn load_fill(tx: &Transaction<'_>, exec: &ExecId) -> Result<Option<FillRow>, JournalError> {
    Ok(tx
        .query_row(
            "SELECT client_oid, exchange_order_id, side, qty_lots, price_ticks, fee_mantissa, fee_scale, fee_coin, exch_ts_ms
             FROM fill WHERE exec_id = ?1",
            params![exec.as_str()],
            |r| {
                Ok(FillRow {
                    client_oid: r.get(0)?, exchange_order_id: r.get(1)?, side: r.get(2)?,
                    qty: r.get(3)?, price: r.get(4)?, fee_m: r.get(5)?, fee_s: r.get(6)?,
                    fee_coin: r.get(7)?, ts: r.get(8)?,
                })
            },
        )
        .optional()?)
}

fn insert_fill(tx: &Transaction<'_>, f: &PrivateFill, intent: Option<&IntentId>, resolution: &str, now: i64) -> Result<(), JournalError> {
    tx.execute(
        "INSERT INTO fill (exec_id, intent_id, client_oid, exchange_order_id, side, qty_lots, price_ticks,
                           fee_mantissa, fee_scale, fee_coin, exch_ts_ms, resolution, booked_wall_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            f.exec_id.as_str(), intent.map(|i| i.as_str()),
            f.client_oid.as_ref().map(|c| c.as_str()), f.exchange_order_id.as_ref().map(|x| x.as_str()),
            f.side.as_str(), f.qty_lots, f.price_ticks, f.fee_mantissa, f.fee_scale, f.fee_coin,
            f.exch_ts_ms, resolution, now
        ],
    )?;
    Ok(())
}

/// true, wenn diese Quelle den Fill zum ersten Mal meldet.
fn observe(tx: &Transaction<'_>, f: &PrivateFill, now: i64) -> Result<bool, JournalError> {
    Ok(tx.execute(
        "INSERT INTO fill_observation (exec_id, source, observed_wall_ms) VALUES (?1,?2,?3)
         ON CONFLICT(exec_id, source) DO NOTHING",
        params![f.exec_id.as_str(), f.source.as_str(), now],
    )? == 1)
}

fn unresolved(tx: &Transaction<'_>, f: &PrivateFill, resolution: &'static str, now: i64, out: FillBooking) -> Result<FillBooking, JournalError> {
    insert_fill(tx, f, None, resolution, now)?;
    observe(tx, f, now)?;
    block(tx, resolution, None, Some(&f.exec_id), now)?;
    Ok(out)
}

fn book_fill_tx(tx: &Transaction<'_>, f: &PrivateFill, now: i64) -> Result<FillBooking, JournalError> {
    if f.qty_lots <= 0 {
        return Err(JournalError::InvalidFill("QTY_NOT_POSITIVE"));
    }

    // 1) Bekannte execId: Duplikat oder Konflikt. Fehlende IDs ergänzen, nie überschreiben.
    if let Some(old) = load_fill(tx, &f.exec_id)? {
        let new_source = observe(tx, f, now)?;
        if old.same_economics(f) {
            tx.execute(
                "UPDATE fill SET client_oid = COALESCE(client_oid, ?2), exchange_order_id = COALESCE(exchange_order_id, ?3)
                 WHERE exec_id = ?1",
                params![
                    f.exec_id.as_str(),
                    f.client_oid.as_ref().map(|c| c.as_str()),
                    f.exchange_order_id.as_ref().map(|x| x.as_str())
                ],
            )?;
            return Ok(FillBooking::Duplicate { confirmed_by_new_source: new_source });
        }
        block(tx, "FILL_PAYLOAD_CONFLICT", None, Some(&f.exec_id), now)?;
        return Ok(FillBooking::PayloadConflict);
    }

    // 2) Identität getrennt auflösen (kein OR).
    let by_oid = match &f.client_oid {
        Some(o) => load_intent(tx, IntentKey::ClientOid, o.as_str())?,
        None => None,
    };
    let by_xid = match &f.exchange_order_id {
        Some(x) => load_intent(tx, IntentKey::ExchangeOrderId, x.as_str())?,
        None => None,
    };
    let intent = match (by_oid, by_xid) {
        (Some(a), Some(b)) if a.intent_id != b.intent_id => {
            return unresolved(tx, f, "IDENTITY_CONFLICT", now, FillBooking::IdentityConflict { reason: "OID_AND_ORDERID_POINT_TO_DIFFERENT_INTENTS" });
        }
        (Some(a), _) => a,
        (None, Some(b)) => b,
        (None, None) => return unresolved(tx, f, "ORPHAN", now, FillBooking::Orphan),
    };
    if let (Some(known), Some(x)) = (&intent.exchange_order_id, &f.exchange_order_id) {
        if known.as_str() != x.as_str() {
            return unresolved(tx, f, "IDENTITY_CONFLICT", now, FillBooking::IdentityConflict { reason: "ORDERID_MISMATCH" });
        }
    }
    if intent.side != f.side {
        return unresolved(tx, f, "IDENTITY_CONFLICT", now, FillBooking::IdentityConflict { reason: "SIDE_MISMATCH" });
    }

    // 3) Überfüllung vor jeder Mengenbuchung.
    let new_cum = intent.filled_lots.checked_add(f.qty_lots).ok_or(JournalError::Overflow)?;
    if new_cum > intent.qty_lots {
        insert_fill(tx, f, Some(&intent.intent_id), "OVERFILL", now)?;
        observe(tx, f, now)?;
        conflict(tx, &intent.intent_id, intent.state, "OVERFILL", now)?;
        return Ok(FillBooking::Overfill { intent_id: intent.intent_id });
    }

    // 4) Menge + Zustand + Reservierung in derselben Transaktion.
    let tr = next(intent.state, &OrderEvent::FillBooked { booked_cum: new_cum, order_lots: intent.qty_lots });
    let resolution = if matches!(tr, Transition::Conflict { .. }) { "CLOSED_ORDER" } else { "APPLIED" };
    insert_fill(tx, f, Some(&intent.intent_id), resolution, now)?;
    observe(tx, f, now)?;
    tx.execute(
        "UPDATE order_intent SET filled_lots = ?2, updated_wall_ms = ?3,
                exchange_order_id = COALESCE(exchange_order_id, ?4)
         WHERE intent_id = ?1",
        params![intent.intent_id.as_str(), new_cum, now, f.exchange_order_id.as_ref().map(|x| x.as_str())],
    )?;

    Ok(match tr {
        Transition::To(s) => {
            if s != intent.state {
                set_state(tx, &intent.intent_id, intent.state, s, "FILL", now)?;
                if s.is_terminal() {
                    release_reservation(tx, &intent.intent_id)?;
                }
            }
            FillBooking::Applied { intent_id: intent.intent_id, booked_cum: new_cum, order_lots: intent.qty_lots, state: s, note: None }
        }
        Transition::Stay { reason } => FillBooking::Applied {
            intent_id: intent.intent_id, booked_cum: new_cum, order_lots: intent.qty_lots, state: intent.state, note: Some(reason),
        },
        Transition::Conflict { reason } => {
            conflict(tx, &intent.intent_id, intent.state, reason, now)?;
            FillBooking::ClosedOrderFill { intent_id: intent.intent_id, reason }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dn_types::exec::FillSource;

    fn tmp(name: &str) -> std::path::PathBuf {
        let ns = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("dn_journal_{name}_{}_{ns}.sqlite", std::process::id()))
    }

    fn intent(id: &str, oid: &str, qty: Lots) -> NewIntent {
        NewIntent {
            order: TypedOrder {
                intent_id: IntentId(id.into()), client_oid: ClientOid(oid.into()),
                instrument_version: InstrumentVersion(1), side: Side::Buy, qty_lots: qty,
                reduce_only: false, purpose: Purpose::Entry, preset_stop: None,
            },
            bot_id: "bot".into(), graph_hash: [0; 32], account_uid: "uid".into(), instrument_id: 1,
            category: "USDT-FUTURES".into(), symbol: "BTCUSDT".into(), order_type: "market",
            ref_price_ticks: 801_507, stop_ticks: Some(799_000),
        }
    }

    fn res(id: &str, lots: Lots) -> NewReservation {
        NewReservation { reservation_id: ReservationId(id.into()), lots, notional_e8: 1 }
    }

    fn fill(exec: &str, oid: Option<&str>, xid: Option<&str>, qty: Lots, fee: i64, src: FillSource) -> PrivateFill {
        PrivateFill {
            exec_id: ExecId(exec.into()), client_oid: oid.map(|s| ClientOid(s.into())),
            exchange_order_id: xid.map(|s| ExchangeOrderId(s.into())), side: Side::Buy,
            qty_lots: qty, price_ticks: 801_507, fee_mantissa: fee, fee_scale: 4,
            fee_coin: "USDT".into(), exch_ts_ms: 1_000, source: src,
        }
    }

    fn submitted(j: &mut OrderJournal, id: &str, oid: &str, qty: Lots, xid: Option<&str>) {
        j.commit_intent(&intent(id, oid, qty), &res(&format!("r{id}"), qty), 1).unwrap();
        j.apply_event(&IntentId(id.into()), &OrderEvent::SubmitStarted, "t", 2).unwrap();
        if let Some(x) = xid {
            j.apply_ack(&IntentId(id.into()), &ExchangeOrderId(x.into()), 3).unwrap();
        }
    }

    /// DN-P5-UP-019: IDs zeigen auf verschiedene Orders ⇒ keine Mengenbuchung, Sperre.
    #[test]
    fn identity_conflict_books_no_quantity() {
        let mut j = OrderJournal::open(&tmp("idc")).unwrap();
        submitted(&mut j, "A", "oa", 10, Some("X1"));
        submitted(&mut j, "B", "ob", 10, Some("X2"));
        let b = j.book_fill(&fill("e1", Some("oa"), Some("X2"), 5, 1, FillSource::Ws), 4).unwrap();
        assert!(matches!(b, FillBooking::IdentityConflict { .. }));
        assert!(j.open_intents().unwrap().iter().all(|i| i.filled_lots == 0));
        assert_eq!(j.open_block_count().unwrap(), 1);
    }

    /// DN-P5-UP-020: WS + REST desselben Fills = Bestätigung; abweichende Gebühr = Konflikt.
    #[test]
    fn dedupe_compares_all_economics() {
        let mut j = OrderJournal::open(&tmp("dup")).unwrap();
        submitted(&mut j, "A", "oa", 10, Some("X1"));
        j.book_fill(&fill("e1", Some("oa"), Some("X1"), 5, 1, FillSource::Ws), 4).unwrap();
        assert_eq!(j.book_fill(&fill("e1", None, Some("X1"), 5, 1, FillSource::Rest), 5).unwrap(), FillBooking::Duplicate { confirmed_by_new_source: true });
        assert_eq!(j.book_fill(&fill("e1", Some("oa"), Some("X1"), 5, 1, FillSource::Ws), 6).unwrap(), FillBooking::Duplicate { confirmed_by_new_source: false });
        assert_eq!(j.book_fill(&fill("e1", Some("oa"), Some("X1"), 5, 2, FillSource::Rest), 7).unwrap(), FillBooking::PayloadConflict);
    }

    /// DN-P5-UP-021: Überfüllung ⇒ Fill gesichert, Menge unverändert, Order UNKNOWN.
    #[test]
    fn overfill_is_kept_and_order_goes_unknown() {
        let mut j = OrderJournal::open(&tmp("over")).unwrap();
        submitted(&mut j, "A", "oa", 10, Some("X1"));
        j.book_fill(&fill("e1", Some("oa"), Some("X1"), 6, 1, FillSource::Ws), 4).unwrap();
        let b = j.book_fill(&fill("e2", Some("oa"), Some("X1"), 6, 1, FillSource::Ws), 5).unwrap();
        assert!(matches!(b, FillBooking::Overfill { .. }));
        let a = &j.open_intents().unwrap()[0];
        assert_eq!((a.filled_lots, a.state), (6, OrderState::Unknown));
    }

    /// DN-P5-UP-017: Fill vor ACK lernt die orderId; spätes ACK ⇒ kein Rückschritt.
    #[test]
    fn fill_before_ack_then_late_ack() {
        let mut j = OrderJournal::open(&tmp("late")).unwrap();
        submitted(&mut j, "A", "oa", 10, None);
        j.book_fill(&fill("e1", Some("oa"), Some("X9"), 4, 1, FillSource::Ws), 4).unwrap();
        let t = j.apply_ack(&IntentId("A".into()), &ExchangeOrderId("X9".into()), 5).unwrap();
        assert_eq!(t, Transition::To(OrderState::PartialFilled));
    }
}
```

## 8. `dn-core/src/risk/s1.rs`: S1 vor jeder Order

```rust
//! P5 §5a S1: Limits in Lots und Nominalwert, nie in Margin. Volle offene Exposure inkl.
//! vor dem Senden reservierter und UNKNOWN-Orders. V1: Entry nur aus bestätigtem FLAT.

use dn_types::trading::*;

#[derive(Clone, Debug)]
pub struct S1Limits {
    pub max_position_lots: Lots,
    pub max_notional_e8: QuoteE8,
    pub max_loss_at_stop_e8: QuoteE8, // Bot-Risikobudget je Trade (WALLET §3)
    pub max_open_orders: u32,
    pub max_entries_per_minute: u32,
    pub daily_loss_limit_e8: QuoteE8, // positiv; Tagesverlust cashflowbereinigt (WALLET §3)
}

#[derive(Clone, Debug)]
pub struct PendingOrder {
    pub side: Side,
    pub remaining_lots: Lots,
    pub reduce_only: bool,
    pub state: OrderState,
}

pub struct RiskInputs<'a> {
    pub rules: &'a InstrumentRules, // aktuelle Katalog-Version
    pub position_lots: Lots,        // Bitget-bestätigt, vorzeichenbehaftet
    pub pending: &'a [PendingOrder],
    pub entries_last_minute: u32,
    pub daily_pnl_e8: QuoteE8,      // Bitget-Equity jetzt − UTC 00:00, cashflowbereinigt
}

#[derive(Clone, Debug)]
pub struct EntryRequest {
    pub side: Side,
    pub lots: Lots,
    pub ref_price: Ticks,
    pub stop_price: Ticks,
    pub cost_reserve_ticks: Ticks, // Round-Trip-Gebühr (privater Satz, K16) + Slippage-Reserve
    pub instrument_version: InstrumentVersion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskReject {
    InstrumentVersionMismatch,
    PositionNotFlat,
    DailyLossLimit,
    RateLimit,
    TooManyOpenOrders,
    BelowMinLots,
    AboveMaxLots,
    AboveMaxMarketLots,
    BelowMinNotional,
    StopWrongSide,
    PositionLimit,
    NotionalLimit,
    LossAtStopLimit,
    Overflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reservation {
    pub lots: Lots,
    pub notional_e8: QuoteE8,
    pub worst_position_lots: Lots,
    pub loss_at_stop_e8: QuoteE8,
}

fn counts(p: &PendingOrder) -> bool {
    p.state.may_still_fill() || p.state.pre_send()
}

pub fn check_entry(req: &EntryRequest, inp: &RiskInputs<'_>, lim: &S1Limits) -> Result<Reservation, RiskReject> {
    use RiskReject::*;
    let r = inp.rules;
    if req.instrument_version != r.version {
        return Err(InstrumentVersionMismatch);
    }
    if inp.position_lots != 0 {
        return Err(PositionNotFlat); // kein Flip, kein Pyramiding (P5 Matrix §3)
    }
    if inp.daily_pnl_e8 <= -lim.daily_loss_limit_e8 {
        return Err(DailyLossLimit);
    }
    if inp.entries_last_minute >= lim.max_entries_per_minute {
        return Err(RateLimit);
    }
    if inp.pending.iter().filter(|p| counts(p)).count() as u32 >= lim.max_open_orders {
        return Err(TooManyOpenOrders);
    }
    if req.lots < r.min_lots {
        return Err(BelowMinLots);
    }
    if req.lots > r.max_lots {
        return Err(AboveMaxLots);
    }
    if req.lots > r.max_market_lots {
        return Err(AboveMaxMarketLots);
    }
    let notional = r.value_e8(req.lots, req.ref_price).ok_or(Overflow)?;
    if notional < r.min_notional_e8 {
        return Err(BelowMinNotional);
    }
    let stop_dist = match req.side {
        Side::Buy => req.ref_price - req.stop_price,
        Side::Sell => req.stop_price - req.ref_price,
    };
    if stop_dist <= 0 {
        return Err(StopWrongSide);
    }
    // Ungünstigster Fall: alle risikoerhöhenden Restmengen füllen.
    let mut worst = inp.position_lots;
    for p in inp.pending.iter().filter(|p| counts(p) && !p.reduce_only && p.side == req.side) {
        let d = p.side.sign().checked_mul(p.remaining_lots).ok_or(Overflow)?;
        worst = worst.checked_add(d).ok_or(Overflow)?;
    }
    worst = worst.checked_add(req.side.sign().checked_mul(req.lots).ok_or(Overflow)?).ok_or(Overflow)?;
    let abs = worst.checked_abs().ok_or(Overflow)?;
    if abs > lim.max_position_lots {
        return Err(PositionLimit);
    }
    if r.value_e8(abs, req.ref_price).ok_or(Overflow)? > lim.max_notional_e8 {
        return Err(NotionalLimit);
    }
    let loss = r.value_e8(req.lots, stop_dist.checked_add(req.cost_reserve_ticks).ok_or(Overflow)?).ok_or(Overflow)?;
    if loss > lim.max_loss_at_stop_e8 {
        return Err(LossAtStopLimit);
    }
    Ok(Reservation { lots: req.lots, notional_e8: notional, worst_position_lots: worst, loss_at_stop_e8: loss })
}
```

## 9. `dn-core/src/account/positions.rs`: Generationen und Watermark

```rust
//! Bitget ist Positionsautorität (P5 §5 Nr. 6). REST nur mit Coverage::Complete.
//! Generation je Poll; dauerhaftes WS-Watermark je Schlüssel inkl. Tombstones;
//! WS-Snapshot löscht nur im nachgewiesenen Scope.

use dn_types::exec::{Coverage, ExchangePosition, PositionKey, Snapshot, WsAction};
use std::collections::{BTreeMap, BTreeSet};

struct Entry {
    pos: ExchangePosition,
    ws_seq: u64,
}

#[derive(Default)]
pub struct PositionBook {
    open: BTreeMap<PositionKey, Entry>,
    tombstones: BTreeMap<PositionKey, u64>,
    ws_seq: u64,
    issued_generation: u64,
    applied_generation: u64,
    ws_snapshot_scope: Option<BTreeSet<String>>, // None = unbelegt ⇒ Snapshot löscht nichts
}

pub struct PollTicket {
    generation: u64,
    ws_seq_at_start: u64,
    categories: BTreeSet<String>,
    known_at_start: BTreeSet<PositionKey>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PollReject {
    CoverageUnproven,
    Superseded,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub added: u32,
    pub updated: u32,
    pub removed: u32,
    pub conflicts: Vec<PositionKey>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum WsApply {
    Applied,
    SnapshotScopeUnknown,
}

impl PositionBook {
    /// Scope erst nach Demo-Nachweis setzen (Offener Punkt: deckt instType=UTA alle Kategorien ab?).
    pub fn set_ws_snapshot_scope(&mut self, scope: Option<BTreeSet<String>>) {
        self.ws_snapshot_scope = scope;
    }

    pub fn confirmed(&self, key: &PositionKey) -> Option<&ExchangePosition> {
        self.open.get(key).map(|e| &e.pos)
    }

    pub fn applied_generation(&self) -> u64 {
        self.applied_generation
    }

    pub fn begin_poll(&mut self, categories: BTreeSet<String>) -> PollTicket {
        self.issued_generation += 1;
        PollTicket {
            generation: self.issued_generation,
            ws_seq_at_start: self.ws_seq,
            categories,
            known_at_start: self.open.keys().cloned().collect(),
        }
    }

    fn touched_after(&self, k: &PositionKey, seq: u64) -> bool {
        self.open.get(k).is_some_and(|e| e.ws_seq > seq) || self.tombstones.get(k).is_some_and(|s| *s > seq)
    }

    pub fn finish_poll(&mut self, t: PollTicket, snap: Snapshot<ExchangePosition>) -> Result<ReconcileReport, PollReject> {
        if snap.coverage != Coverage::Complete {
            return Err(PollReject::CoverageUnproven);
        }
        if t.generation <= self.applied_generation {
            return Err(PollReject::Superseded); // ein neuerer Poll wurde bereits angewandt
        }
        let mut rep = ReconcileReport::default();
        let mut reported = BTreeSet::new();
        for p in snap.rows {
            if !t.categories.contains(&p.key.category) {
                continue;
            }
            reported.insert(p.key.clone());
            if self.touched_after(&p.key, t.ws_seq_at_start) {
                let same = match self.open.get(&p.key) {
                    Some(e) => e.pos == p,
                    None => p.size_lots == 0,
                };
                if !same {
                    rep.conflicts.push(p.key.clone()); // neuere WS-Daten gewinnen; Konflikt ⇒ neuer Poll
                }
                continue;
            }
            if p.size_lots == 0 {
                if self.open.remove(&p.key).is_some() {
                    rep.removed += 1;
                }
                continue;
            }
            let key = p.key.clone();
            let seq = self.open.get(&key).map_or(0, |e| e.ws_seq);
            match self.open.insert(key, Entry { pos: p, ws_seq: seq }) {
                None => rep.added += 1,
                Some(_) => rep.updated += 1,
            }
        }
        for k in &t.known_at_start {
            let prunable = !reported.contains(k)
                && t.categories.contains(&k.category)
                && !self.touched_after(k, t.ws_seq_at_start);
            if prunable && self.open.remove(k).is_some() {
                rep.removed += 1;
            }
        }
        self.applied_generation = t.generation;
        Ok(rep)
    }

    pub fn apply_ws(&mut self, action: WsAction, entries: Vec<ExchangePosition>) -> WsApply {
        self.ws_seq += 1;
        let seq = self.ws_seq;
        let mut result = WsApply::Applied;
        if action == WsAction::Snapshot {
            match &self.ws_snapshot_scope {
                None => result = WsApply::SnapshotScopeUnknown,
                Some(scope) => {
                    let keep: BTreeSet<&PositionKey> = entries.iter().map(|p| &p.key).collect();
                    let gone: Vec<PositionKey> = self
                        .open
                        .keys()
                        .filter(|k| scope.contains(&k.category) && !keep.contains(k))
                        .cloned()
                        .collect();
                    for k in gone {
                        self.open.remove(&k);
                        self.tombstones.insert(k, seq);
                    }
                }
            }
        }
        for p in entries {
            if p.size_lots == 0 {
                self.open.remove(&p.key);
                self.tombstones.insert(p.key, seq);
            } else {
                self.tombstones.remove(&p.key);
                self.open.insert(p.key.clone(), Entry { pos: p, ws_seq: seq });
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(size: i64) -> ExchangePosition {
        ExchangePosition {
            key: PositionKey { category: "USDT-FUTURES".into(), symbol: "BTCUSDT".into(), pos_side: "long".into() },
            size_lots: size, avg_price_ticks: 800_000, leverage_e2: 300, liq_price_ticks: None,
        }
    }

    fn cats() -> BTreeSet<String> {
        ["USDT-FUTURES".to_string()].into_iter().collect()
    }

    fn snap(rows: Vec<ExchangePosition>) -> Snapshot<ExchangePosition> {
        Snapshot { rows, coverage: Coverage::Complete, pages: 1 }
    }

    /// DN-P5-UP-022: überlappende Polls – der ältere wird verworfen.
    #[test]
    fn older_poll_is_superseded() {
        let mut b = PositionBook::default();
        let t1 = b.begin_poll(cats());
        let t2 = b.begin_poll(cats());
        b.finish_poll(t2, snap(vec![pos(5)])).unwrap();
        assert_eq!(b.finish_poll(t1, snap(vec![])), Err(PollReject::Superseded));
        assert!(b.confirmed(&pos(0).key).is_some());
    }

    /// DN-P5-UP-023: WS-Änderung nach Pollstart wird von REST nicht überschrieben.
    #[test]
    fn ws_after_poll_start_wins() {
        let mut b = PositionBook::default();
        let t = b.begin_poll(cats());
        b.apply_ws(WsAction::Update, vec![pos(5)]);
        let rep = b.finish_poll(t, snap(vec![pos(3)])).unwrap();
        assert_eq!(rep.conflicts.len(), 1);
        assert_eq!(b.confirmed(&pos(0).key).unwrap().size_lots, 5);
    }

    /// DN-P5-UP-024: WS-Snapshot mit unbelegtem Scope löscht nichts.
    #[test]
    fn snapshot_without_scope_does_not_prune() {
        let mut b = PositionBook::default();
        b.apply_ws(WsAction::Update, vec![pos(5)]);
        assert_eq!(b.apply_ws(WsAction::Snapshot, vec![]), WsApply::SnapshotScopeUnknown);
        assert!(b.confirmed(&pos(0).key).is_some());
    }

    /// DN-P5-UP-018: unbelegter REST-Snapshot ersetzt keinen Zustand.
    #[test]
    fn unproven_snapshot_rejected() {
        let mut b = PositionBook::default();
        let t = b.begin_poll(cats());
        let s = Snapshot { rows: vec![], coverage: Coverage::Unproven, pages: 1 };
        assert_eq!(b.finish_poll(t, s), Err(PollReject::CoverageUnproven));
    }
}
```

## 10. `dn-core/src/protection/coverage.rs`: Stop-Kette (S3)

```rust
//! S3: Stop ist ein serverseitig BESTÄTIGTER Zustand.
//! Kette: Teilfill → Fill-Journal → bestätigte Position → erforderliche Stopmenge
//!        → vollständiger Stop-Readback → CONFIRMED_FULL.
//! Invariante 6: Entries nur bei CONFIRMED_FULL ohne fremde/falsche Stops.
//! Invariante 7: Ersatz VOR Storno (keine Schutzlücke durch Cancel/Replace).

use dn_types::exec::{ExchangePosition, StopQty, StopReadback};
use dn_types::trading::{Lots, Side, StopCoverage, Ticks, TriggerBy};

#[derive(Clone, Debug)]
pub struct StopPolicy {
    pub trigger_by: TriggerBy,
    pub tolerance_ticks: Ticks,
    pub max_readback_age_ms: i64,
    /// Danach Notschließung (S3: „Scheitert die Schutzherstellung …“). Wert aus Demo-Messung P5-A18.
    pub unprotected_deadline_ms: i64,
}

pub struct ProtectionInput<'a> {
    pub position: Option<&'a ExchangePosition>,
    pub position_changed_ms: i64,
    pub expected_stop_ticks: Option<Ticks>,
    pub stops: &'a [StopReadback],
    /// Zeitpunkt des letzten VOLLSTÄNDIGEN Stop-Readbacks (Coverage::Complete).
    pub readback_at_ms: Option<i64>,
    pub stop_request_in_flight: bool,
    pub stop_request_unknown: bool,
    pub unprotected_since_ms: Option<i64>,
    pub now_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtectionAction {
    Readback,
    PlaceStop { lots: Lots, trigger_ticks: Ticks },
    CancelStop { exchange_id: String },
    EmergencyClose { lots: Lots },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectionDecision {
    pub coverage: StopCoverage,
    pub entries_allowed: bool,
    pub actions: Vec<ProtectionAction>,
}

pub fn evaluate(inp: &ProtectionInput<'_>, pol: &StopPolicy) -> ProtectionDecision {
    let size = inp.position.map_or(0, |p| p.size_lots);

    if size == 0 {
        let actions: Vec<_> = inp
            .stops
            .iter()
            .filter(|s| s.live)
            .map(|s| ProtectionAction::CancelStop { exchange_id: s.exchange_id.clone() })
            .collect();
        return ProtectionDecision { coverage: StopCoverage::Absent, entries_allowed: actions.is_empty(), actions };
    }

    let abs = size.abs();
    let close_side = if size > 0 { Side::Sell } else { Side::Buy };
    let fresh = inp
        .readback_at_ms
        .is_some_and(|t| t >= inp.position_changed_ms && inp.now_ms - t <= pol.max_readback_age_ms);

    let (mut coverage, mut actions, no_wrong) = if !fresh {
        (StopCoverage::Stale, vec![ProtectionAction::Readback], false)
    } else if let Some(expected) = inp.expected_stop_ticks {
        let mut covered: Lots = 0;
        let mut wrong = Vec::new();
        for s in inp.stops.iter().filter(|s| s.live) {
            let ok = s.side == close_side
                && s.reduce_only
                && s.trigger_by == pol.trigger_by
                && (s.trigger_ticks - expected).abs() <= pol.tolerance_ticks;
            if ok {
                covered = covered.saturating_add(match s.qty {
                    StopQty::Lots(l) => l,
                    StopQty::WholePosition => abs,
                });
            } else {
                wrong.push(s.exchange_id.clone());
            }
        }
        let missing = abs - covered;
        let no_wrong = wrong.is_empty();
        let mut acts = Vec::new();
        let cov = if missing == 0 {
            // Korrekte Deckung bestätigt ⇒ JETZT falsche Stops entfernen.
            acts.extend(wrong.into_iter().map(|id| ProtectionAction::CancelStop { exchange_id: id }));
            StopCoverage::ConfirmedFull
        } else if missing < 0 {
            acts.push(ProtectionAction::Readback);
            StopCoverage::ProtectionUnknown
        } else if inp.stop_request_unknown {
            acts.push(ProtectionAction::Readback);
            StopCoverage::ExchangeAckUnknown
        } else if inp.stop_request_in_flight {
            StopCoverage::Requested
        } else {
            acts.push(ProtectionAction::PlaceStop { lots: missing, trigger_ticks: expected });
            if covered > 0 { StopCoverage::ConfirmedPartial } else { StopCoverage::ProtectionUnknown }
        };
        (cov, acts, no_wrong)
    } else {
        (StopCoverage::ProtectionUnknown, vec![ProtectionAction::Readback], false)
    };

    let deadline_hit = inp
        .unprotected_since_ms
        .is_some_and(|t| inp.now_ms - t >= pol.unprotected_deadline_ms);
    if coverage != StopCoverage::ConfirmedFull && deadline_hit {
        actions.push(ProtectionAction::EmergencyClose { lots: abs });
        coverage = StopCoverage::ClosingReconciling;
    }

    ProtectionDecision {
        entries_allowed: coverage == StopCoverage::ConfirmedFull && no_wrong,
        coverage,
        actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dn_types::exec::PositionKey;

    fn position(size: i64) -> ExchangePosition {
        ExchangePosition {
            key: PositionKey { category: "USDT-FUTURES".into(), symbol: "BTCUSDT".into(), pos_side: "long".into() },
            size_lots: size, avg_price_ticks: 800_000, leverage_e2: 300, liq_price_ticks: Some(700_000),
        }
    }

    fn stop(id: &str, lots: i64, trigger: i64) -> StopReadback {
        StopReadback {
            exchange_id: id.into(), client_oid: None, side: Side::Sell, reduce_only: true,
            trigger_ticks: trigger, trigger_by: TriggerBy::Mark, qty: StopQty::Lots(lots), live: true,
        }
    }

    fn pol() -> StopPolicy {
        StopPolicy { trigger_by: TriggerBy::Mark, tolerance_ticks: 0, max_readback_age_ms: 5_000, unprotected_deadline_ms: 10_000 }
    }

    fn input<'a>(p: &'a ExchangePosition, s: &'a [StopReadback], read: i64, since: Option<i64>, now: i64) -> ProtectionInput<'a> {
        ProtectionInput {
            position: Some(p), position_changed_ms: 100, expected_stop_ticks: Some(790_000), stops: s,
            readback_at_ms: Some(read), stop_request_in_flight: false, stop_request_unknown: false,
            unprotected_since_ms: since, now_ms: now,
        }
    }

    /// DN-P5-UP-025 / P5-A18: Teilfill auf 4 Lots, Stop deckt 3 ⇒ Rest nachlegen, keine Entries.
    #[test]
    fn partial_coverage_tops_up() {
        let p = position(4);
        let s = [stop("s1", 3, 790_000)];
        let d = evaluate(&input(&p, &s, 200, Some(150), 300), &pol());
        assert_eq!(d.coverage, StopCoverage::ConfirmedPartial);
        assert_eq!(d.actions, vec![ProtectionAction::PlaceStop { lots: 1, trigger_ticks: 790_000 }]);
        assert!(!d.entries_allowed);
    }

    /// Readback älter als Positionsänderung ⇒ STALE.
    #[test]
    fn stale_readback_forces_readback() {
        let p = position(4);
        let d = evaluate(&input(&p, &[], 50, None, 300), &pol());
        assert_eq!((d.coverage, d.actions), (StopCoverage::Stale, vec![ProtectionAction::Readback]));
    }

    /// Invariante 7: falscher Stop wird erst nach bestätigtem Ersatz storniert.
    #[test]
    fn replace_before_cancel() {
        let p = position(4);
        let wrong = [stop("bad", 4, 780_000)];
        let d1 = evaluate(&input(&p, &wrong, 200, Some(150), 300), &pol());
        assert_eq!(d1.actions, vec![ProtectionAction::PlaceStop { lots: 4, trigger_ticks: 790_000 }]);
        let both = [stop("bad", 4, 780_000), stop("good", 4, 790_000)];
        let d2 = evaluate(&input(&p, &both, 400, Some(150), 500), &pol());
        assert_eq!(d2.coverage, StopCoverage::ConfirmedFull);
        assert_eq!(d2.actions, vec![ProtectionAction::CancelStop { exchange_id: "bad".into() }]);
        assert!(!d2.entries_allowed);
    }

    /// DN-P5-UP-026 / P5-A19: Frist überschritten ⇒ Notschließung der bestätigten Menge.
    #[test]
    fn deadline_triggers_emergency_close() {
        let p = position(4);
        let d = evaluate(&input(&p, &[], 20_000, Some(100), 20_100), &pol());
        assert_eq!(d.coverage, StopCoverage::ClosingReconciling);
        assert!(d.actions.contains(&ProtectionAction::EmergencyClose { lots: 4 }));
    }
}
```

## 11. `dn-bitget/src/private/executor.rs`: einziger Signer

```rust
//! IO-Executor: EINZIGER Halter von Credentials und Signer (P5 §4, Invariante 1).
//! Genau ein Versuch je Befehl, keine Retries. Ergebnisse gehen nie verloren.

use darkbot_common::channel::{ChannelError, Consumer, Producer};
use dn_types::exec::{IoCommand, IoResult};
use std::time::Duration;

/// Implementiert mit auth.rs (V1 §3.1), orders.rs (V1 §3.3) und pagination.rs (V2 §5).
pub trait PrivateApi {
    fn execute(&mut self, cmd: IoCommand) -> IoResult;
}

pub fn run_executor<R, A: PrivateApi>(cmds: Consumer<IoCommand>, results: Producer<R>, wrap: fn(IoResult) -> R, api: &mut A) {
    loop {
        let cmd = match cmds.pop_timeout_or_closed(Duration::from_secs(1)) {
            Ok(Some(c)) => c,
            Ok(None) => continue,
            Err(()) => return, // Owner beendet
        };
        let mut msg = wrap(api.execute(cmd));
        loop {
            match results.try_push(msg) {
                Ok(()) => break,
                Err(ChannelError::Full(back)) => {
                    msg = back; // Ergebnis (z. B. ACK) darf nie verworfen werden
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(ChannelError::Disconnected(_)) => return,
            }
        }
    }
}
```

## 12. `dn-core/src/owner/mod.rs`: serieller Owner, nicht blockierend

```rust
//! EIN serieller Account-/Order-/Risk-Owner je UTA-Unterkonto (P5 Invariante 1).
//! Kein Netzwerkaufruf im Owner: Befehle → IO-Executor, Ergebnisse → priorisierte Inbox.
//! Doorbell-Muster aus Project_0 channel.rs (A23-01): ein Wakeup für beide Inboxen.

use crate::account::positions::{PollTicket, PositionBook, WsApply};
use crate::protection::coverage::{evaluate, ProtectionAction, ProtectionDecision, ProtectionInput, StopPolicy};
use crate::risk::s1::{check_entry, EntryRequest, PendingOrder, RiskInputs, RiskReject, S1Limits};
use darkbot_common::channel::{ChannelError, Consumer, Producer, Wakeup, Woken};
use dn_store::order_journal::{FillBooking, JournalError, NewIntent, NewReservation, OrderJournal};
use dn_types::exec::*;
use dn_types::order_fsm::{ExchangeOrderTruth, OrderEvent, ReconcileEvidence, Transition};
use dn_types::trading::*;
use std::collections::BTreeSet;
use std::time::Duration;

pub enum OwnerInput {
    Private(PrivateEvent),
    Io(IoResult),
    Control(ControlCommand),
    Health(HealthUpdate),
}

/// Autorisierte, quittierte Gateway-Commands (WALLET §4, P5 S5).
pub enum ControlCommand {
    Halt,
    Activate,
    EmergencyClose,
}

/// Gates aus P1.9 / S7, vom Owner nie selbst „gesund“ gesetzt.
#[derive(Default)]
pub struct HealthUpdate {
    pub feed_fresh: Option<bool>,
    pub clock_qualified: Option<bool>,
    pub mode_and_leverage_verified: Option<bool>,
    pub daily_pnl_e8: Option<QuoteE8>,
}

pub struct SignalIntent {
    pub side: Side,
    pub lots: Lots,
    pub ref_price: Ticks,
    pub stop_price: Ticks,
}

#[derive(Debug)]
pub enum OwnerError {
    EntryBlocked(&'static str),
    Risk(RiskReject),
    Journal(JournalError),
    IoUnavailable,
}

fn jr<T>(r: Result<T, JournalError>) -> Result<T, OwnerError> {
    r.map_err(OwnerError::Journal)
}

pub struct OwnerConfig {
    pub bot_id: String,
    pub graph_hash: [u8; 32],
    pub account_uid: String,
    pub category: String,
    pub symbol: String,
    pub position_key: PositionKey,
    pub rules: InstrumentRules,
    pub limits: S1Limits,
    pub stop_policy: StopPolicy,
    pub cost_reserve_ticks: Ticks,
}

pub struct Owner {
    cfg: OwnerConfig,
    account: AccountState,
    resume_to_active: bool, // nach Laufzeit-Reconcile; nach Neustart immer false (HALTED-Default)
    journal: OrderJournal,
    positions: PositionBook,
    poll: Option<PollTicket>,
    io: Producer<IoCommand>,
    clock: fn() -> i64,
    ids: u64,
    // Gates
    feed_fresh: bool,
    clock_qualified: bool,
    mode_and_leverage_verified: bool,
    daily_pnl_e8: QuoteE8,
    entry_times: Vec<i64>,
    // S3-Zustand
    stops: Vec<StopReadback>,
    stops_read_at: Option<i64>,
    position_changed_ms: i64,
    last_position_poll_ms: i64,
    last_fill_ms: i64,
    expected_stop: Option<Ticks>,
    stop_in_flight: bool,
    stop_unknown: bool,
    emergency_in_flight: bool,
    unprotected_since: Option<i64>,
    protection: ProtectionDecision,
    last_tick_ms: i64,
}

impl Owner {
    pub fn new(cfg: OwnerConfig, journal: OrderJournal, io: Producer<IoCommand>, clock: fn() -> i64) -> Self {
        Self {
            cfg, account: AccountState::Uninitialized, resume_to_active: false, journal,
            positions: PositionBook::default(), poll: None, io, clock, ids: 0,
            feed_fresh: false, clock_qualified: false, mode_and_leverage_verified: false,
            daily_pnl_e8: 0, entry_times: Vec::new(),
            stops: Vec::new(), stops_read_at: None, position_changed_ms: 0,
            last_position_poll_ms: 0, last_fill_ms: 0, expected_stop: None,
            stop_in_flight: false, stop_unknown: false, emergency_in_flight: false,
            unprotected_since: None,
            protection: ProtectionDecision { coverage: StopCoverage::Absent, entries_allowed: false, actions: vec![] },
            last_tick_ms: 0,
        }
    }

    /// Priorität: private Ereignisse/IO/Control vollständig vor höchstens einem Signal.
    pub fn run(mut self, priority: Consumer<OwnerInput>, signals: Consumer<SignalIntent>, wake: Wakeup) {
        if self.recover().is_err() {
            self.account = AccountState::Halted;
        }
        loop {
            let token = wake.token();
            let mut worked = false;
            while let Some(m) = priority.try_pop() {
                worked = true;
                self.handle(m);
            }
            if let Some(s) = signals.try_pop() {
                worked = true;
                let now = (self.clock)();
                if let Err(e) = self.on_signal(s, now) {
                    self.fail(e);
                }
            }
            let now = (self.clock)();
            if now - self.last_tick_ms >= 100 {
                self.on_tick(now);
            }
            if !worked {
                if let Woken::TimedOut = wake.wait(token, Duration::from_millis(100)) {
                    self.on_tick((self.clock)());
                }
            }
        }
    }

    fn handle(&mut self, m: OwnerInput) {
        let now = (self.clock)();
        let r = match m {
            OwnerInput::Private(ev) => self.on_private(ev, now),
            OwnerInput::Io(res) => self.on_io(res, now),
            OwnerInput::Control(c) => self.on_control(c, now),
            OwnerInput::Health(h) => {
                if let Some(v) = h.feed_fresh { self.feed_fresh = v; }
                if let Some(v) = h.clock_qualified { self.clock_qualified = v; }
                if let Some(v) = h.mode_and_leverage_verified { self.mode_and_leverage_verified = v; }
                if let Some(v) = h.daily_pnl_e8 { self.daily_pnl_e8 = v; }
                Ok(())
            }
        };
        if let Err(e) = r {
            self.fail(e);
        }
    }

    /// Journalfehler ⇒ HALTED; nie erneut senden (C02/C11-Prinzip).
    fn fail(&mut self, e: OwnerError) {
        if let OwnerError::Journal(_) = e {
            self.account = AccountState::Halted;
            self.resume_to_active = false;
            // Alarm-Outbox (P1.9) hier anbinden.
        }
    }

    fn enter_reconciling(&mut self) {
        if self.account == AccountState::Active {
            self.resume_to_active = true;
        }
        if self.account != AccountState::EmergencyRecovery {
            self.account = AccountState::Reconciling;
        }
    }

    fn request(&mut self, cmd: IoCommand) -> Result<(), OwnerError> {
        self.io.try_push(cmd).map_err(|_| OwnerError::IoUnavailable)
    }

    fn start_position_poll(&mut self) -> Result<(), OwnerError> {
        if self.poll.is_none() {
            let cats: BTreeSet<String> = [self.cfg.category.clone()].into_iter().collect();
            self.poll = Some(self.positions.begin_poll(cats));
            self.request(IoCommand::ReadPositions)?;
        }
        Ok(())
    }

    fn require_entry_allowed(&self) -> Result<(), OwnerError> {
        use OwnerError::EntryBlocked as B;
        if self.account != AccountState::Active { return Err(B("ACCOUNT_NOT_ACTIVE")); }
        if !self.feed_fresh || !self.clock_qualified { return Err(B("FEED_OR_CLOCK")); }
        if !self.mode_and_leverage_verified { return Err(B("MODE_LEVERAGE_UNVERIFIED")); }
        if jr(self.journal.open_block_count())? > 0 { return Err(B("RECONCILE_PENDING")); }
        if !self.protection.entries_allowed { return Err(B("PROTECTION_NOT_CONFIRMED")); }
        Ok(())
    }

    fn pending(&self) -> Result<Vec<PendingOrder>, OwnerError> {
        Ok(jr(self.journal.open_intents())?
            .into_iter()
            .map(|i| PendingOrder { side: i.side, remaining_lots: i.qty_lots - i.filled_lots, reduce_only: i.reduce_only, state: i.state })
            .collect())
    }

    fn position_lots(&self) -> Lots {
        self.positions.confirmed(&self.cfg.position_key).map_or(0, |p| p.size_lots)
    }

    fn next_ids(&mut self) -> (IntentId, ClientOid) {
        self.ids += 1;
        let id = IntentId(format!("{}-{}-{}", self.cfg.bot_id, (self.clock)(), self.ids));
        // clientOid deterministisch aus Bot + Graph + Nummer (TEIL3); Länge per Bitget-Limit prüfen.
        let oid = ClientOid(format!("dn{}{:x}", self.ids, u32::from_le_bytes([
            self.cfg.graph_hash[0], self.cfg.graph_hash[1], self.cfg.graph_hash[2], self.cfg.graph_hash[3],
        ])));
        (id, oid)
    }

    fn new_intent(&self, order: TypedOrder, ref_price: Ticks, stop: Option<Ticks>) -> NewIntent {
        NewIntent {
            order, bot_id: self.cfg.bot_id.clone(), graph_hash: self.cfg.graph_hash,
            account_uid: self.cfg.account_uid.clone(), instrument_id: self.cfg.rules.instrument_id,
            category: self.cfg.category.clone(), symbol: self.cfg.symbol.clone(),
            order_type: "market", ref_price_ticks: ref_price, stop_ticks: stop,
        }
    }

    /// Journal → SUBMIT_PENDING → IO-Queue. Queue verweigert ⇒ Beweis „nie gesendet“.
    fn journal_and_send(&mut self, cmd: IoCommand, intent: NewIntent, res: NewReservation, now: i64) -> Result<(), OwnerError> {
        let id = intent.order.intent_id.clone();
        jr(self.journal.commit_intent(&intent, &res, now))?;                               // C01/C02
        jr(self.journal.apply_event(&id, &OrderEvent::SubmitStarted, "SUBMIT", now))?;     // C03
        match self.io.try_push(cmd) {
            Ok(()) => Ok(()),
            Err(ChannelError::Full(_)) | Err(ChannelError::Disconnected(_)) => {
                jr(self.journal.apply_event(&id, &OrderEvent::ProvenNotSent, "IO_QUEUE_REFUSED", now))?;
                Err(OwnerError::IoUnavailable)
            }
        }
    }

    fn on_signal(&mut self, s: SignalIntent, now: i64) -> Result<(), OwnerError> {
        self.require_entry_allowed()?;
        self.entry_times.retain(|t| now - t < 60_000);
        let pending = self.pending()?;
        let req = EntryRequest {
            side: s.side, lots: s.lots, ref_price: s.ref_price, stop_price: s.stop_price,
            cost_reserve_ticks: self.cfg.cost_reserve_ticks, instrument_version: self.cfg.rules.version,
        };
        let inputs = RiskInputs {
            rules: &self.cfg.rules, position_lots: self.position_lots(), pending: &pending,
            entries_last_minute: self.entry_times.len() as u32, daily_pnl_e8: self.daily_pnl_e8,
        };
        let res = check_entry(&req, &inputs, &self.cfg.limits).map_err(OwnerError::Risk)?;
        let (intent_id, client_oid) = self.next_ids();
        let order = TypedOrder {
            intent_id: intent_id.clone(), client_oid, instrument_version: self.cfg.rules.version,
            side: s.side, qty_lots: s.lots, reduce_only: false, purpose: Purpose::Entry,
            preset_stop: Some(PresetStop { trigger_ticks: s.stop_price, trigger_by: self.cfg.stop_policy.trigger_by }),
        };
        let intent = self.new_intent(order.clone(), s.ref_price, Some(s.stop_price));
        let reservation = NewReservation {
            reservation_id: ReservationId(format!("r-{}", intent_id.as_str())),
            lots: res.lots, notional_e8: res.notional_e8,
        };
        self.expected_stop = Some(s.stop_price);
        self.entry_times.push(now);
        self.journal_and_send(IoCommand::Place(order), intent, reservation, now)
    }

    fn on_io(&mut self, r: IoResult, now: i64) -> Result<(), OwnerError> {
        match r {
            IoResult::Placed { intent_id, result } => match result {
                SendResult::Accepted { exchange_order_id } => {
                    if let Transition::Conflict { .. } = jr(self.journal.apply_ack(&intent_id, &exchange_order_id, now))? {
                        self.enter_reconciling();
                    }
                    self.stop_in_flight = false;
                    self.request(IoCommand::ReadStops) // Preset-SL ≠ bestätigt (S3)
                }
                SendResult::Rejected { code } => {
                    jr(self.journal.apply_event(&intent_id, &OrderEvent::ExchangeRejected, &code, now))?;
                    self.stop_in_flight = false;
                    Ok(())
                }
                SendResult::NoDefinitiveAnswer => {
                    jr(self.journal.apply_event(&intent_id, &OrderEvent::TransportUnknown, "NO_DEFINITIVE_ANSWER", now))?;
                    self.stop_unknown = self.stop_in_flight;
                    self.enter_reconciling();
                    self.lookup(&intent_id)
                }
            },
            IoResult::LookupDone { intent_id, exchange_order_id, truth } => {
                if let Some(x) = exchange_order_id {
                    jr(self.journal.apply_ack(&intent_id, &x, now))?;
                }
                match truth {
                    Ok(t) => self.on_truth(intent_id, t, now),
                    Err(_) => Ok(()), // UNKNOWN bleibt; on_tick fragt erneut
                }
            }
            IoResult::Fills { intent_id, fills } => {
                if let Ok(snap) = fills {
                    for f in &snap.rows {
                        self.book(f, now)?;
                    }
                }
                self.lookup(&intent_id)
            }
            IoResult::Positions(res) => {
                let Some(ticket) = self.poll.take() else { return Ok(()) };
                match res.map(|snap| self.positions.finish_poll(ticket, snap)) {
                    Ok(Ok(rep)) => {
                        self.last_position_poll_ms = now;
                        if !rep.conflicts.is_empty() {
                            self.enter_reconciling();
                            self.start_position_poll()?;
                        }
                        self.position_changed_ms = now;
                    }
                    _ => self.enter_reconciling(),
                }
                self.evaluate_protection(now)
            }
            IoResult::Stops { read_at_ms, stops } => {
                match stops {
                    Ok(s) if s.coverage == Coverage::Complete => {
                        self.stops = s.rows;
                        self.stops_read_at = Some(read_at_ms);
                        self.stop_unknown = false;
                    }
                    _ => self.stops_read_at = None, // unvollständig ⇒ STALE
                }
                self.evaluate_protection(now)
            }
            IoResult::StopCancelled { .. } => self.request(IoCommand::ReadStops),
        }
    }

    fn book(&mut self, f: &PrivateFill, now: i64) -> Result<(), OwnerError> {
        let b = jr(self.journal.book_fill(f, now))?;
        if b.requires_reconcile() {
            self.enter_reconciling();
        }
        if b.changes_position() {
            // S3-Kette Schritt 1–2: Position neu bestätigen, Stops neu lesen.
            self.last_fill_ms = now;
            self.position_changed_ms = now;
            self.start_position_poll()?;
            self.request(IoCommand::ReadStops)?;
        }
        Ok(())
    }

    fn on_private(&mut self, ev: PrivateEvent, now: i64) -> Result<(), OwnerError> {
        match ev {
            PrivateEvent::Fill(f) => self.book(&f, now),
            PrivateEvent::Position { action, entries } => {
                if self.positions.apply_ws(action, entries) == WsApply::SnapshotScopeUnknown {
                    self.start_position_poll()?;
                }
                self.position_changed_ms = now;
                self.evaluate_protection(now)
            }
            PrivateEvent::StopHint => self.request(IoCommand::ReadStops),
            PrivateEvent::Order { client_oid, exchange_order_id, status, exchange_filled } => {
                let Some(it) = jr(self.journal.open_intents())?.into_iter().find(|i| {
                    client_oid.as_ref() == Some(&i.client_oid) || i.exchange_order_id.as_ref() == Some(&exchange_order_id)
                }) else {
                    self.enter_reconciling(); // Fremdorder (S7)
                    return Ok(());
                };
                let ev = match status {
                    StatusHint::Working if exchange_filled == 0 => OrderEvent::ExchangeWorking,
                    StatusHint::Unrecognized(_) => OrderEvent::TransportUnknown, // K8: nie „nicht gefunden“
                    _ if exchange_filled > it.filled_lots => OrderEvent::StatusAheadOfFills,
                    _ => return self.lookup(&it.intent_id), // Endzustand nur über vollständige Belege
                };
                let tr = jr(self.journal.apply_event(&it.intent_id, &ev, "WS_ORDER", now))?;
                if let Transition::Stay { reason: "FETCH_FILLS" } = tr {
                    self.request(IoCommand::ReadFills { intent_id: it.intent_id, exchange_order_id })?;
                }
                Ok(())
            }
        }
    }

    fn lookup(&mut self, id: &IntentId) -> Result<(), OwnerError> {
        let Some(it) = jr(self.journal.open_intents())?.into_iter().find(|i| &i.intent_id == id) else {
            return Ok(());
        };
        self.request(IoCommand::Lookup { intent_id: it.intent_id, client_oid: it.client_oid, exchange_order_id: it.exchange_order_id })
    }

    fn on_truth(&mut self, id: IntentId, truth: ExchangeOrderTruth, now: i64) -> Result<(), OwnerError> {
        let Some(it) = jr(self.journal.open_intents())?.into_iter().find(|i| i.intent_id == id) else {
            return Ok(());
        };
        if truth.exchange_filled() > it.filled_lots {
            if let Some(x) = it.exchange_order_id {
                return self.request(IoCommand::ReadFills { intent_id: id, exchange_order_id: x });
            }
        }
        // Position gilt als abgeglichen, wenn ein vollständiger Poll NACH dem letzten Fill angewandt wurde.
        let position_reconciled = self.last_position_poll_ms >= self.last_fill_ms && self.poll.is_none();
        let ev = OrderEvent::Reconciled(ReconcileEvidence {
            truth, order_lots: it.qty_lots, booked_lots: it.filled_lots, position_reconciled,
        });
        if let Transition::Stay { reason: "POSITION_NOT_RECONCILED" } = jr(self.journal.apply_event(&id, &ev, "RECONCILE", now))? {
            self.start_position_poll()?;
        }
        self.maybe_leave_reconciling(now)
    }

    /// RECONCILING verlassen nur bei: keine UNKNOWN-Order, keine offene Sperre, frische Position und Stops.
    fn maybe_leave_reconciling(&mut self, now: i64) -> Result<(), OwnerError> {
        if self.account != AccountState::Reconciling {
            return Ok(());
        }
        let any_unknown = jr(self.journal.open_intents())?.iter().any(|i| i.state == OrderState::Unknown);
        let fresh = self.poll.is_none()
            && self.last_position_poll_ms >= self.last_fill_ms
            && self.stops_read_at.is_some_and(|t| now - t <= self.cfg.stop_policy.max_readback_age_ms);
        if !any_unknown && fresh {
            jr(self.journal.clear_blocks(now))?;
            self.account = if self.resume_to_active { AccountState::Active } else { AccountState::Halted };
            self.resume_to_active = false;
        }
        Ok(())
    }

    fn evaluate_protection(&mut self, now: i64) -> Result<(), OwnerError> {
        let decision = evaluate(
            &ProtectionInput {
                position: self.positions.confirmed(&self.cfg.position_key),
                position_changed_ms: self.position_changed_ms,
                expected_stop_ticks: self.expected_stop,
                stops: &self.stops,
                readback_at_ms: self.stops_read_at,
                stop_request_in_flight: self.stop_in_flight,
                stop_request_unknown: self.stop_unknown,
                unprotected_since_ms: self.unprotected_since,
                now_ms: now,
            },
            &self.cfg.stop_policy,
        );
        self.unprotected_since = match decision.coverage {
            StopCoverage::ConfirmedFull | StopCoverage::Absent => None,
            _ => self.unprotected_since.or(Some(now)),
        };
        let actions = decision.actions.clone();
        self.protection = decision;
        for a in actions {
            match a {
                ProtectionAction::Readback => self.request(IoCommand::ReadStops)?,
                ProtectionAction::PlaceStop { lots, trigger_ticks } => self.place_stop(lots, trigger_ticks, now)?,
                ProtectionAction::CancelStop { exchange_id } => self.request(IoCommand::CancelStop { exchange_id })?,
                ProtectionAction::EmergencyClose { lots } => self.emergency_close(lots, now)?,
            }
        }
        Ok(())
    }

    /// Ersatz-Stop: eine Anfrage zur Zeit; reduce-only; über Journal (kein Send ohne fsync).
    fn place_stop(&mut self, lots: Lots, trigger: Ticks, now: i64) -> Result<(), OwnerError> {
        if self.stop_in_flight || self.stop_unknown {
            return Ok(());
        }
        let side = if self.position_lots() > 0 { Side::Sell } else { Side::Buy };
        let (id, oid) = self.next_ids();
        let order = TypedOrder {
            intent_id: id.clone(), client_oid: oid, instrument_version: self.cfg.rules.version,
            side, qty_lots: lots, reduce_only: true, purpose: Purpose::Stop,
            preset_stop: Some(PresetStop { trigger_ticks: trigger, trigger_by: self.cfg.stop_policy.trigger_by }),
        };
        let intent = self.new_intent(order.clone(), trigger, Some(trigger));
        let res = NewReservation { reservation_id: ReservationId(format!("r-{}", id.as_str())), lots, notional_e8: 0 };
        self.stop_in_flight = true;
        self.journal_and_send(IoCommand::PlaceStop(order), intent, res, now)
    }

    /// EMERGENCY_RECOVERY: nur bestätigte Restmenge, reduce-only, kein Entry/Flip, eine Anfrage zur Zeit.
    fn emergency_close(&mut self, lots: Lots, now: i64) -> Result<(), OwnerError> {
        if self.emergency_in_flight {
            return Ok(()); // erst Readback/Lookup, nie blind wiederholen (S3)
        }
        let pos = self.position_lots();
        if pos == 0 {
            return Ok(());
        }
        let qty = lots.min(pos.abs());
        let (id, oid) = self.next_ids();
        let order = TypedOrder {
            intent_id: id.clone(), client_oid: oid, instrument_version: self.cfg.rules.version,
            side: if pos > 0 { Side::Sell } else { Side::Buy }, qty_lots: qty, reduce_only: true,
            purpose: Purpose::EmergencyClose, preset_stop: None,
        };
        let intent = self.new_intent(order.clone(), 0, None);
        let res = NewReservation { reservation_id: ReservationId(format!("r-{}", id.as_str())), lots: qty, notional_e8: 0 };
        self.account = AccountState::EmergencyRecovery;
        self.emergency_in_flight = true;
        self.journal_and_send(IoCommand::Place(order), intent, res, now)
    }

    fn on_control(&mut self, c: ControlCommand, now: i64) -> Result<(), OwnerError> {
        match c {
            ControlCommand::Halt => {
                self.account = AccountState::Halted;
                self.resume_to_active = false;
                Ok(())
            }
            // ACTIVE nur aus HALTED und nur bei vollständigem Readback (P5 S5, §1).
            ControlCommand::Activate => {
                let ready = self.account == AccountState::Halted
                    && jr(self.journal.open_block_count())? == 0
                    && self.feed_fresh && self.clock_qualified && self.mode_and_leverage_verified
                    && self.poll.is_none()
                    && self.stops_read_at.is_some_and(|t| now - t <= self.cfg.stop_policy.max_readback_age_ms);
                if ready {
                    self.account = AccountState::Active;
                }
                Ok(())
            }
            // Getrennt von HALT (P5-A24): kontrolliertes Schließen der bestätigten Position.
            ControlCommand::EmergencyClose => {
                let lots = self.position_lots().abs();
                self.emergency_close(lots, now)
            }
        }
    }

    fn on_tick(&mut self, now: i64) {
        self.last_tick_ms = now;
        let r = (|| -> Result<(), OwnerError> {
            for it in jr(self.journal.open_intents())? {
                if it.state == OrderState::Unknown {
                    self.lookup(&it.intent_id)?;
                }
            }
            let stale = self.stops_read_at.map_or(true, |t| now - t > self.cfg.stop_policy.max_readback_age_ms);
            if stale {
                self.request(IoCommand::ReadStops)?;
            }
            self.evaluate_protection(now)?;
            self.maybe_leave_reconciling(now)
        })();
        if let Err(e) = r {
            self.fail(e);
        }
    }

    /// Neustart: UNINITIALIZED → RECONCILING → HALTED (Default). Nie Auto-ACTIVE.
    pub fn recover(&mut self) -> Result<(), OwnerError> {
        let now = (self.clock)();
        self.account = AccountState::Reconciling;
        self.resume_to_active = false;
        for it in jr(self.journal.open_intents())? {
            match it.state {
                // C03: vor SUBMIT_PENDING abgebrochen ⇒ beweisbar nie gesendet; KEIN Auto-Resend.
                OrderState::Reserved | OrderState::IntentDurable => {
                    jr(self.journal.apply_event(&it.intent_id, &OrderEvent::ProvenNotSent, "RESTART_BEFORE_SUBMIT", now))?;
                }
                // C04–C10: Börsenstand klären.
                s if s.may_still_fill() => {
                    if s != OrderState::Unknown {
                        jr(self.journal.apply_event(&it.intent_id, &OrderEvent::TransportUnknown, "RESTART", now))?;
                    }
                    self.request(IoCommand::Lookup {
                        intent_id: it.intent_id, client_oid: it.client_oid, exchange_order_id: it.exchange_order_id,
                    })?;
                }
                _ => {}
            }
        }
        self.start_position_poll()?;
        self.request(IoCommand::ReadStops)
    }
}
```

Dieser Owner-Code ist das umfangreichste Stück. Drei Stellen sind bewusst als Platzhalter markiert und gehören vor Integration ausdefiniert:
- **Alarm-Outbox in `fail`:** wird in P1.9 angebunden.
- **clientOid-Format:** Länge und erlaubte Zeichen müssen gegen das Bitget-Limit geprüft werden.
- **Owner-Tests:** Die Crash-Injektion C00–C16 läuft gegen einen Fake-`PrivateApi`. Der eigentliche Nachweis erfolgt im gemeinsamen DN-P5-ALL-001-Runner, nicht in einer eigenen Testengine.

## 13. `dn-backtest/src/fill.rs`: versionierte Annahmen, echte Reihenfolge

```rust
//! Teil A – Trade-Replay (Hauptweg). Teil B – LEAN-Bar-Port (nur Stufe-1-Screening, nie Qualifikation).
//! Teil B portiert QuantConnect LEAN Common/Orders/Fills/FillModel.cs @22404ea2 (Apache-2.0,
//! © QuantConnect Corporation): StopMarketFill, InternalLimitFill. Änderungen: Ganzzahl-Ticks.
//! Jedes Ergebnis trägt die Kennung seiner Annahmen; ohne ausreichende Daten ⇒ Indeterminate.

use dn_types::trading::{Lots, Side, Ticks, TriggerBy};

/// Rohtrade in kanonischer Reihenfolge (exch_ts_ms, source_seq) nach P1-T26.
#[derive(Clone, Copy, Debug)]
pub struct Trade {
    pub exch_ts_ms: i64,
    pub source_seq: u64,
    pub price_ticks: Ticks,
    pub qty_lots: Lots,
    pub aggressor: Side,
}

#[derive(Clone, Copy, Debug)]
pub struct MarkTick {
    pub ts_ms: i64,
    pub seq: u64,
    pub mark_ticks: Ticks,
}

/// Versionierte Ausführungsannahmen. BASE und CONSERVATIVE werden immer beide gerechnet.
#[derive(Clone, Copy, Debug)]
pub struct ExecutionAssumptions {
    pub id: &'static str,             // z. B. "EXEC-BASE-v1"
    pub submit_latency_ms: i64,
    pub trigger_latency_ms: i64,
    pub max_wait_ms: i64,
    /// Anteil am gleichseitigen Aggressor-Volumen, den wir maximal nehmen (‰).
    pub participation_permille: i64,
    /// Zusätzlicher Aufschlag je Fill gegen uns (Impact/Slippage).
    pub impact_ticks: Ticks,
    /// Stop-Auslösung bei Gleichheit (Bitget-Regel per Demo, DN-P4-003).
    pub trigger_on_touch: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimPart {
    pub ts_ms: i64,
    pub seq: u64,
    pub price_ticks: Ticks,
    pub qty_lots: Lots,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Uncertainty {
    NoLiquidityWithinWindow,
    MissingMarkPrice,
    UnsortedInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimFill {
    Filled { parts: Vec<SimPart>, cost_ticks_lots: i128, assumptions: &'static str },
    /// Teilausführung im Fenster – der Rest gilt als NICHT ausgeführt (kein Auffüllen).
    Partial { parts: Vec<SimPart>, remaining_lots: Lots, assumptions: &'static str },
    NotFilled,
    Indeterminate { reason: Uncertainty, assumptions: &'static str },
}

fn is_sorted(trades: &[Trade]) -> bool {
    trades.windows(2).all(|w| (w[0].exch_ts_ms, w[0].source_seq) <= (w[1].exch_ts_ms, w[1].source_seq))
}

/// Market ab Position `after` (ts, seq). Liquidität nur aus gleichseitigen Aggressor-Trades:
/// ein Buy-Aggressor-Trade lief zum Ask ⇒ Spread steckt im Preis, kein separates Spreadmodell.
pub fn market_after(side: Side, qty: Lots, after: (i64, u64), trades: &[Trade], a: &ExecutionAssumptions) -> SimFill {
    if !is_sorted(trades) {
        return SimFill::Indeterminate { reason: Uncertainty::UnsortedInput, assumptions: a.id };
    }
    let mut remaining = qty;
    let mut cost: i128 = 0;
    let mut parts = Vec::new();
    for t in trades.iter().filter(|t| (t.exch_ts_ms, t.source_seq) > after && t.aggressor == side) {
        if t.exch_ts_ms - after.0 > a.max_wait_ms {
            break;
        }
        let cap = t.qty_lots.saturating_mul(a.participation_permille) / 1000;
        if cap <= 0 {
            continue;
        }
        let take = remaining.min(cap);
        let px = match side {
            Side::Buy => t.price_ticks + a.impact_ticks,
            Side::Sell => t.price_ticks - a.impact_ticks,
        };
        cost += i128::from(take) * i128::from(px);
        remaining -= take;
        parts.push(SimPart { ts_ms: t.exch_ts_ms, seq: t.source_seq, price_ticks: px, qty_lots: take });
        if remaining == 0 {
            return SimFill::Filled { parts, cost_ticks_lots: cost, assumptions: a.id };
        }
    }
    if parts.is_empty() {
        SimFill::Indeterminate { reason: Uncertainty::NoLiquidityWithinWindow, assumptions: a.id }
    } else {
        SimFill::Partial { parts, remaining_lots: remaining, assumptions: a.id }
    }
}

/// Market-Entry zur Signalzeit + Latenz.
pub fn market(side: Side, qty: Lots, order_ts_ms: i64, trades: &[Trade], a: &ExecutionAssumptions) -> SimFill {
    market_after(side, qty, (order_ts_ms + a.submit_latency_ms, 0), trades, a)
}

pub struct StopOrder {
    pub side: Side, // Sell schließt Long
    pub stop_ticks: Ticks,
    pub qty_lots: Lots,
    pub armed_after: (i64, u64),
    pub trigger_by: TriggerBy,
}

fn crosses(side: Side, price: Ticks, stop: Ticks, touch: bool) -> bool {
    match (side, touch) {
        (Side::Sell, false) => price < stop,
        (Side::Sell, true) => price <= stop,
        (Side::Buy, false) => price > stop,
        (Side::Buy, true) => price >= stop,
    }
}

/// Stop-Market: Auslösung nach Last oder Mark in exakter Reihenfolge, dann Market ab Auslösung.
pub fn stop_market(o: &StopOrder, trades: &[Trade], marks: Option<&[MarkTick]>, a: &ExecutionAssumptions) -> SimFill {
    let trigger = match o.trigger_by {
        TriggerBy::Market => trades
            .iter()
            .find(|t| (t.exch_ts_ms, t.source_seq) > o.armed_after && crosses(o.side, t.price_ticks, o.stop_ticks, a.trigger_on_touch))
            .map(|t| (t.exch_ts_ms, t.source_seq)),
        TriggerBy::Mark => match marks {
            None => return SimFill::Indeterminate { reason: Uncertainty::MissingMarkPrice, assumptions: a.id },
            Some(m) => m
                .iter()
                .find(|x| (x.ts_ms, x.seq) > o.armed_after && crosses(o.side, x.mark_ticks, o.stop_ticks, a.trigger_on_touch))
                .map(|x| (x.ts_ms, x.seq)),
        },
    };
    let Some((ts, seq)) = trigger else { return SimFill::NotFilled };
    market_after(o.side, o.qty_lots, (ts + a.trigger_latency_ms, seq), trades, a)
}

#[derive(Debug, PartialEq, Eq)]
pub enum FirstTouch {
    None,
    Stop,
    Target,
    Ambiguous,
}

/// SL vs. TP in exakter Rohtrade-Reihenfolge (K20, P1-T26).
pub fn first_touch_long(stop: Ticks, target: Ticks, trades: &[Trade]) -> FirstTouch {
    for t in trades {
        match (t.price_ticks <= stop, t.price_ticks >= target) {
            (true, true) => return FirstTouch::Ambiguous, // nur bei stop ≥ target ⇒ Konfigurationsfehler
            (true, false) => return FirstTouch::Stop,
            (false, true) => return FirstTouch::Target,
            (false, false) => {}
        }
    }
    FirstTouch::None
}

/// Teil B – nur Screening (Stufe 1).
pub mod lean_bar {
    use dn_types::trading::{Side, Ticks};

    pub struct Bar {
        pub high: Ticks,
        pub low: Ticks,
        pub close: Ticks,
    }

    /// LEAN StopMarketFill.
    pub fn stop_market(side: Side, stop: Ticks, bar: &Bar, slip: Ticks) -> Option<Ticks> {
        match side {
            Side::Sell if bar.low < stop => Some(stop.min(bar.close - slip)),
            Side::Buy if bar.high > stop => Some(stop.max(bar.close + slip)),
            _ => None,
        }
    }

    /// LEAN InternalLimitFill – Fill bei Berührung, ohne Queue ⇒ für Maker zu optimistisch (K19).
    pub fn limit(side: Side, limit: Ticks, bar: &Bar) -> Option<Ticks> {
        match side {
            Side::Buy if bar.low < limit => Some(bar.high.min(limit)),
            Side::Sell if bar.high > limit => Some(bar.low.max(limit)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: ExecutionAssumptions = ExecutionAssumptions {
        id: "EXEC-TEST", submit_latency_ms: 0, trigger_latency_ms: 0, max_wait_ms: 1_000,
        participation_permille: 100, impact_ticks: 1, trigger_on_touch: false,
    };

    fn t(ts: i64, seq: u64, px: Ticks, qty: Lots, ag: Side) -> Trade {
        Trade { exch_ts_ms: ts, source_seq: seq, price_ticks: px, qty_lots: qty, aggressor: ag }
    }

    /// DN-P4-007: gleiche ms ⇒ source_seq entscheidet SL vs. TP.
    #[test]
    fn same_ms_order_by_seq() {
        let tr = [t(10, 1, 100, 1, Side::Sell), t(10, 2, 120, 1, Side::Buy)];
        assert_eq!(first_touch_long(100, 120, &tr), FirstTouch::Stop);
        let tr2 = [t(10, 1, 120, 1, Side::Buy), t(10, 2, 100, 1, Side::Sell)];
        assert_eq!(first_touch_long(100, 120, &tr2), FirstTouch::Target);
    }

    /// DN-P4-008: Teilnahmequote begrenzt die Menge je Trade; nur gleichseitige Aggressoren.
    #[test]
    fn participation_and_aggressor_side() {
        let tr = [t(1, 1, 100, 50, Side::Buy), t(2, 1, 99, 500, Side::Sell), t(3, 1, 101, 50, Side::Buy)];
        match market(Side::Buy, 10, 0, &tr, &A) {
            SimFill::Filled { parts, .. } => {
                assert_eq!(parts.iter().map(|p| p.qty_lots).collect::<Vec<_>>(), vec![5, 5]);
                assert!(parts.iter().all(|p| p.price_ticks >= 101));
            }
            other => panic!("{other:?}"),
        }
    }

    /// DN-P4-009: Rest ohne Liquidität ⇒ Partial, kein Auffüllen.
    #[test]
    fn partial_when_window_too_thin() {
        let tr = [t(1, 1, 100, 50, Side::Buy)];
        assert!(matches!(market(Side::Buy, 10, 0, &tr, &A), SimFill::Partial { remaining_lots: 5, .. }));
    }

    /// DN-P4-010: Mark-Trigger ohne Mark-Daten ⇒ Indeterminate.
    #[test]
    fn mark_trigger_without_marks_is_indeterminate() {
        let o = StopOrder { side: Side::Sell, stop_ticks: 100, qty_lots: 1, armed_after: (0, 0), trigger_by: TriggerBy::Mark };
        assert!(matches!(stop_market(&o, &[], None, &A), SimFill::Indeterminate { reason: Uncertainty::MissingMarkPrice, .. }));
    }

    /// DN-P4-004: Gap durch den Stop ⇒ Fill am schlechteren Preis nach der Auslösung.
    #[test]
    fn gap_fills_worse() {
        // Nach dem Trigger ist ein weiterer Trade fuer ein modelliertes Fill noetig.
        let tr = [t(1, 1, 105, 10, Side::Sell), t(2, 1, 90, 100, Side::Sell), t(3, 1, 88, 100, Side::Sell)];
        let o = StopOrder { side: Side::Sell, stop_ticks: 100, qty_lots: 5, armed_after: (0, 0), trigger_by: TriggerBy::Market };
        let SimFill::Partial { parts, .. } | SimFill::Filled { parts, .. } = stop_market(&o, &tr, None, &A) else { panic!() };
        assert!(parts[0].price_ticks <= 89);
    }
}
```

Hinweis zum letzten Test: Gegenüber der Ausgangsversion wurde ein dritter Trade **nach** der Stop-Auslösung ergänzt. Dadurch hat der konservative Replay-Algorithmus überhaupt ein späteres Liquiditätsereignis; dies ist **noch kein Nachweis** eines Bitget-Fills.

## 14. Neue Test-IDs (vor Nutzung im TESTPROTOKOLL registrieren)

| ID | Prüft | Ergänzt |
|---|---|---|
| DN-P5-UP-016 | FILLED nur mit gebuchten Fills und abgeglichener Position | P5-A12, A27 |
| UP-017 | Verspätetes ACK ⇒ kein Rückschritt; Fill vor ACK lernt orderId | P5-A08 |
| UP-018 | Unbelegter Pagination-Vertrag ⇒ `Unproven` ⇒ ersetzt keinen Zustand | P5-A26 |
| UP-019 | IDs auf verschiedene Orders ⇒ keine Mengenbuchung, Sperre | P5-A27 |
| UP-020 | Dedupe über alle wirtschaftlichen Felder; WS+REST = Bestätigung | P5-A08 |
| UP-021 | Überfüllung ⇒ Fill gesichert, Order UNKNOWN | P5-A27 |
| UP-022 | Überlappende Polls ⇒ älterer verworfen | P5-A20 |
| UP-023 | WS nach Pollstart gewinnt gegen REST | P5-A20 |
| UP-024 | WS-Snapshot ohne Scope löscht nichts | P5-A20 |
| UP-025 | Teilfill ⇒ Stop-Nachlage, Entries gesperrt | P5-A18 |
| UP-026 | Schutzfrist ⇒ Notschließung bestätigter Menge | P5-A19 |
| UP-027 | IO-Queue verweigert ⇒ NOT_SENT mit Beweis, kein Send | C03/C04 |
| UP-028 | Journalfehler nach Send ⇒ HALTED, kein Resend | C02/C05 |
| DN-P4-007 … 010 | ms-Reihenfolge, Teilnahmequote, Partial, Mark-Trigger | P4 |

## 15. Weiterhin offen (nur per Demo klärbar, unverändert aus V1 §6)

1. Preset-SL-Verhalten bei Teilfill.
2. Body von `set-leverage` und `cancel-order`.
3. Liste der V3-Statusstrings.
4. WS-Login-Zeitstempel: Sekunden oder ms.
5. Liste der definitiven Ablehnungscodes.
6. Nachweisfenster für `AbsentAfterWindow`.
7. Stop-Trigger bei Gleichheit.
8. WS-Snapshot-Scope für `instType=UTA`.
9. Ob WS- und REST-Fills dieselbe `exch_ts_ms` liefern. Sonst muss `same_economics` die Zeit tolerant vergleichen, und zwar erst nach Messung.

---

# TEIL II – Interne WS_AI-Wiederverwendung (§16, vollstaendig inkl. Code)

> **Leseregel:** Die historischen Python-/TS-Module werden **nicht direkt** als neue Runtime eingebaut. Die aufgeführten Portierungen sind Entwürfe mit offenen Tests; V3-B16–B20 nennen noch fehlende Sicherheits-/Qualitätskorrekturen.

## 16.1 Verbindliche Reihenfolge der Quellen

Für jede Funktion wird in dieser Reihenfolge gesucht. Fremdcode kommt erst, wenn die vorherige Stufe nichts Brauchbares hat.

1. **`ws_ai`:** unsere eigene bewährte Logik, ohne Python, JSON, Redis und Browser-WS
2. **Project_0 / BSC:** vorhandene Rust-Bausteine (`darkbot_common::channel`, Journal-Writer, M4-Messungen)
3. **DarkBot:** Gateway, Protobuf, Graphite-Frontend, Realtime-Client
4. **Open Source:** nur für das, was 1–3 nicht abdecken. Nach dem jetzigen Stand ist das der Backtest-Kern (LEAN, HftBacktest) und die sichere UTA-V3-Ausführung (Hummingbot).

## 16.2 Register: Datei für Datei

Legende: **PORT** = Logik nach Rust, **PORT+FIX** = nach Rust mit Fehlerkorrektur, **MUSTER** = nur Verhalten übernehmen, **NICHT** = verwerfen.

| `ws_ai`-Datei | Was gut ist (belegt) | Gefundene Fehler (belegt) | Art | DarkNode-Ziel | Tests |
|---|---|---|---|---|---|
| `backend/websocket/ws_reconnect_manager.py` | Exponentieller Backoff `base·2^min(n−1,10)` mit Deckel und Jitter-Faktor 0,9–1,1; Zustände; Statistik | **F1:** Nach `max_reconnect_attempts` (Default 50) wird `should_reconnect = False` gesetzt – der Bot gibt die Verbindung **endgültig auf**. **F2:** Der Versuchszähler wird bei **jedem** erfolgreichen Connect auf 0 gesetzt, auch wenn die Verbindung sofort wieder fällt; Flapping hebelt den Backoff aus. **F3:** Nachrichten-Handler-Fehler werden nur geloggt, die Nachricht ist verloren. | PORT+FIX | `dn-bitget/src/ws/backoff.rs` (§16.3) | DN-P1-WS-001…003 |
| `backend/websocket/ws_lanes.py` | Lane je Instrument, Zähler, STALE über Zeit seit letzter Nachricht | **F4:** Ohne erste Nachricht gilt eine Lane **nie** als STALE (Kommentar: „Noch keine Messages erhalten ist OK“) – eine Verbindung, die nie liefert, sieht gesund aus. **F5:** Handler-Fehler setzt nur ERROR, die Nachricht ist weg, eine Lücke wird nicht erfasst. | PORT+FIX | `dn-bitget/src/ws/lane.rs` (§16.4) | DN-P1-WS-004…006, T19 |
| `backend/websocket/ws_message_parsers.py` (früheres Audit) | Feldzuordnung Bitget | Beim Bitget-Trade-Parser endet die Verarbeitung nach dem ersten Trade (`return trade_obj`) | NICHT (ersetzt durch SBE-Codegen) | `dn-bitget/src/public/sbe_v5/` | T01, K01 |
| `backend/services/adapter/candle_agg_1s.py` | 1s-Buckets, OHLCV, Quote-Volumen, Anzahl, Stale-Flush 1,5 s, Versionsrang (Backfill `ts·1000+1` < Live `ts·1000+1000+c`) | **F6:** Jede Ausnahme in `on_trade` → `logger.error` + `return None`: stiller Datenverlust. **F7 (neu, schwer):** Gibt nur **einen** offenen Bucket je Instrument. Ein verspäteter Trade mit älterer Sekunde schließt den laufenden Bucket vorzeitig und eröffnet einen **zweiten** Bucket für die schon fertige Sekunde. Der bekommt eine **höhere** Version und **überschreibt** in der `ReplacingMergeTree` die vollständige Kerze mit einer 1-Trade-Kerze. **F8 (neu):** Bei Rundungsfehlern setzt `_q38` das Quote-Volumen still auf 0; Werte über dem Bereich werden still gekappt. | PORT+FIX | `dn-agg/src/trades_1s.rs` (§16.5) | T08–T10, T25–T27, DN-P1-AGG-001…004 |
| `backend/services/adapter/stream_aggregator.py` | `ResolutionRegistry`: Basis-TFs (`1s, 1m, 5m, 1h`) immer aktiv, angefragte TFs mit TTL 600 s auf monotoner Uhr | **F9 (neu):** Jeder TF wird **separat aus Trades** gebaut statt aus 1s; ohne Quote-Volumen, Anzahl und Kauf/Verkauf; derselbe Late-Trade-Fehler wie F7 | MUSTER (Registry) / NICHT (Aggregation) | `dn-agg/src/mtf.rs`, `dn-agg/src/live_tf.rs` (§16.6) | DN-P1-AGG-005 |
| `backend/services/usecases/historical_candles_from_trades.py` | Fenster-Rebuild 1s aus gespeicherten Trades, idempotent, Backfill niedriger Rang | **F10 (neu, schwer):** `argMin(price, timestamp)` / `argMax(price, timestamp)` sind bei gleicher Millisekunde **nicht deterministisch**; laut unserer Messung teilen sich 13.989 von 20.000 Trades einen Zeitstempel ⇒ Open und Close zufällig. **F11 (neu):** Filter auf genau **eine** `source` ⇒ Sekunden mit Live- **und** Backfill-Trades werden unvollständig gebaut. Zudem gewinnt Live per Rang auch dann, wenn die Live-Kerze lückenhaft war. **F12 (neu):** Fenster `timestamp <= end` ist geschlossen ⇒ Randsekunde wird teilweise gebaut. | PORT+FIX | `dn-agg/src/rebuild.rs` (§16.7) – **derselbe** Aggregator wie live | T23, T25, K04, DN-P1-AGG-006…008 |
| `backend/services/usecases/unified_historical.py`, `backfill_loop`, `gap_scan` (früheres Audit) | Cursor-Backfill, Lücken zuerst, Resume, fein→grob-Scan | Laut früherem Audit siehe WS_AI_IDEEN 7–10, 21–22 | PORT | `dn-backfill/` | T13–T15 |
| `frontend/src/services/ws/WebSocketPool.ts` | Eine Verbindung je Schlüssel, Referenzzählung, verzögertes Schließen (50 ms gegen React-StrictMode-Thrash), **Generationszähler** gegen Callbacks alter Sockets | **F13 (neu):** Nicht-String-Frames werden still verworfen (`typeof ev.data !== "string" → return`) ⇒ mit binärem Protobuf **unbrauchbar**. Listener-Fehler werden geschluckt. Kein Jitter. | MUSTER (nur wo DarkBots Realtime-Client es nicht hat) | `frontend/src/core/realtime/` (DarkBot) | P7, DN-PROTO-008 |
| `frontend/src/services/ws/useWsLane.ts` (früheres Audit) | Resubscribe bei jedem OPEN, Historie einmal laden, rAF-Bündelung | – | MUSTER | DarkNode-Store | T16–T18 |
| `frontend/src/pages/TradingPage/hooks/useChartView.ts` | **Nie synthetisches OHLC**; Lücken als Whitespace-Punkte; das Limit zählt nur echte Kerzen; bei großen Lücken Stride; Live überschreibt Historie je Bucket | **F14 (neu):** Lücken werden aus Zeitabständen **geraten**. Eine 1s-Sekunde ohne Trade (normal) sieht genauso aus wie ein Datenverlust. | PORT+FIX (TS) | `frontend/src/features/darknode/chart/series.ts` (§16.8) | P7, K12 |
| `frontend/src/shared/components/CandleChart/` | Funktionierende Lightweight-Charts-Komponenten | nicht gelesen | MUSTER, nach Lesen entscheiden | DarkBot Graphite | P7 |
| `backend/websocket/ws_rate_limiters.py` | Token-Bucket (laut Dateiname und Import) | nicht gelesen | Konzept | `dn-bitget/src/rest/limiter.rs` | P5-A09 |
| `backend/ai/tier3/backtesting_engine.py` | – | **Platzhalter**: `run_backtest` liefert feste Nullwerte für Rendite, Sharpe, Drawdown und Trefferquote | NICHT | P4 aus LEAN/HftBacktest (V2 §13) | – |

Damit bestätigt sich die Aufteilung: Verbindung, Lanes, Aggregation, Rekonstruktion und Chart-Datenfluss kommen aus `ws_ai` (korrigiert). Backtest-Kern und UTA-V3-Ausführung bleiben neu.

## 16.3 `dn-bitget/src/ws/backoff.rs`

```rust
//! Reconnect-Backoff. Formel aus ws_ai backend/websocket/ws_reconnect_manager.py @1f27ed3
//! (`_calculate_reconnect_delay`): base·2^min(n−1, …), Deckel, Jitter-Faktor 0,9–1,1.
//! Korrekturen: F1 nie aufgeben (24/7) – stattdessen Alarm; F2 Reset erst nach stabiler Verbindung;
//! Zufall injiziert ⇒ deterministische Tests.

#[derive(Clone, Copy, Debug)]
pub struct BackoffPolicy {
    pub base_ms: u64,
    pub max_ms: u64,
    pub jitter_permille: u64, // 100 ⇒ Faktor 0,9–1,1; muss ≤ 1000 sein
    pub stable_after_ms: i64,
    pub alarm_after_attempts: u32,
}

#[derive(Debug, Default)]
pub struct Backoff {
    attempt: u32,
    connected_since: Option<i64>,
}

impl Backoff {
    pub fn on_connected(&mut self, now_ms: i64) {
        self.connected_since = Some(now_ms);
    }

    /// F2: Zähler nur zurücksetzen, wenn die Verbindung lange genug hielt.
    pub fn on_disconnected(&mut self, now_ms: i64, p: &BackoffPolicy) {
        if let Some(t) = self.connected_since.take() {
            if now_ms - t >= p.stable_after_ms {
                self.attempt = 0;
            }
        }
    }

    /// `rnd` aus injizierter Quelle, wird auf [0, 2·jitter] begrenzt.
    pub fn next_delay_ms(&mut self, p: &BackoffPolicy, rnd: u64) -> u64 {
        self.attempt = self.attempt.saturating_add(1);
        let exp = (self.attempt - 1).min(16);
        let raw = p.base_ms.saturating_mul(1u64 << exp).min(p.max_ms);
        let j = rnd.min(2 * p.jitter_permille);
        raw.saturating_mul(1000 - p.jitter_permille + j) / 1000
    }

    /// F1: kein Aufgeben, nur Alarm (P1.9 Alarm-Outbox).
    pub fn alarm(&self, p: &BackoffPolicy) -> bool {
        self.attempt >= p.alarm_after_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: BackoffPolicy = BackoffPolicy { base_ms: 1_000, max_ms: 300_000, jitter_permille: 100, stable_after_ms: 30_000, alarm_after_attempts: 5 };

    /// DN-P1-WS-001: ws_ai-Formel ohne Jitter (rnd = jitter ⇒ Faktor 1,0).
    #[test]
    fn exponential_with_cap() {
        let mut b = Backoff::default();
        let d: Vec<u64> = (0..12).map(|_| b.next_delay_ms(&P, 100)).collect();
        assert_eq!(&d[..4], &[1_000, 2_000, 4_000, 8_000]);
        assert!(d.iter().all(|x| *x <= 300_000));
    }

    /// DN-P1-WS-002 (F2): Flapping setzt den Zähler nicht zurück.
    #[test]
    fn flapping_does_not_reset() {
        let mut b = Backoff::default();
        b.next_delay_ms(&P, 100);
        b.next_delay_ms(&P, 100);
        b.on_connected(0);
        b.on_disconnected(500, &P);
        assert_eq!(b.next_delay_ms(&P, 100), 4_000);
        b.on_connected(10_000);
        b.on_disconnected(50_000, &P);
        assert_eq!(b.next_delay_ms(&P, 100), 1_000);
    }

    /// DN-P1-WS-003 (F1): nach vielen Versuchen Alarm, aber weiter Delays.
    #[test]
    fn never_gives_up() {
        let mut b = Backoff::default();
        for _ in 0..100 {
            assert!(b.next_delay_ms(&P, 0) > 0);
        }
        assert!(b.alarm(&P));
    }
}
```

## 16.4 `dn-bitget/src/ws/lane.rs`

```rust
//! Lane-Zustand nach P1-F09: LIVE / STALE / GAP / RECOVERING / UNAVAILABLE.
//! Herkunft: ws_ai backend/websocket/ws_lanes.py @1f27ed3 (Stale über letzte Nachricht, Zähler).
//! Korrekturen: F4 Erst-Nachrichts-Frist; F5 Decoder-/Handler-Fehler ⇒ UNAVAILABLE + Lücke;
//! jede Unterbrechung erzeugt ein Lückenintervall für den Coverage-Ledger (P1.7, D05).
//! Zeiten = lokale Empfangszeit; der Ledger erweitert Intervalle konservativ auf Börsenzeit.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneState {
    Live,
    Stale,
    Gap,
    Recovering,
    Unavailable,
}

#[derive(Clone, Copy, Debug)]
pub struct LanePolicy {
    pub first_message_deadline_ms: i64,
    /// Für die Trades-Lane gilt books1 derselben Verbindung als Lebenszeichen (ruhiger Markt ≠ Ausfall).
    pub stale_after_ms: i64,
}

#[derive(Debug)]
pub struct Lane {
    pub state: LaneState,
    policy: LanePolicy,
    subscribed_at: Option<i64>,
    last_msg_at: Option<i64>,
    open_gap_since: Option<i64>,
    /// Abgeschlossene Lücken [start, end) – werden vom Ledger abgeholt.
    pub closed_gaps: Vec<(i64, i64)>,
    pub messages: u64,
    pub decode_failures: u64,
}

impl Lane {
    /// Vor dem ersten Empfang ist nichts belegt ⇒ GAP ab Start.
    pub fn new(policy: LanePolicy, now_ms: i64) -> Self {
        Self { state: LaneState::Gap, policy, subscribed_at: None, last_msg_at: None,
               open_gap_since: Some(now_ms), closed_gaps: Vec::new(), messages: 0, decode_failures: 0 }
    }

    fn open_gap(&mut self, now_ms: i64) {
        if self.open_gap_since.is_none() {
            self.open_gap_since = Some(self.last_msg_at.unwrap_or(now_ms));
        }
    }

    pub fn on_subscribed(&mut self, now_ms: i64) {
        self.subscribed_at = Some(now_ms);
        self.state = LaneState::Recovering;
    }

    /// Nur für VALIDE, vollständig verarbeitete Nachrichten. Bei Büchern erst nach angewandtem Snapshot.
    pub fn on_valid_message(&mut self, now_ms: i64) {
        self.messages += 1;
        self.last_msg_at = Some(now_ms);
        if let Some(start) = self.open_gap_since.take() {
            self.closed_gaps.push((start, now_ms));
        }
        self.state = LaneState::Live;
    }

    pub fn on_disconnected(&mut self, now_ms: i64) {
        self.open_gap(now_ms);
        self.subscribed_at = None;
        self.state = LaneState::Gap;
    }

    /// seq/pseq-Bruch: Buch ungültig bis zum neuen Snapshot.
    pub fn on_sequence_gap(&mut self, now_ms: i64) {
        self.open_gap(now_ms);
        self.state = LaneState::Recovering;
    }

    /// F5: unbekanntes Template/Schema oder Verarbeitungsfehler ⇒ nicht stillschweigend weiter.
    pub fn on_decode_failure(&mut self, now_ms: i64) {
        self.decode_failures += 1;
        self.open_gap(now_ms);
        self.state = LaneState::Unavailable;
    }

    pub fn on_tick(&mut self, now_ms: i64) {
        match self.state {
            // F4: abonniert, aber nie eine Nachricht ⇒ STALE.
            LaneState::Recovering => {
                if self.subscribed_at.is_some_and(|t| now_ms - t > self.policy.first_message_deadline_ms) {
                    self.state = LaneState::Stale;
                }
            }
            LaneState::Live => {
                if self.last_msg_at.is_some_and(|t| now_ms - t > self.policy.stale_after_ms) {
                    self.open_gap(now_ms);
                    self.state = LaneState::Stale;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const P: LanePolicy = LanePolicy { first_message_deadline_ms: 5_000, stale_after_ms: 10_000 };

    /// DN-P1-WS-004 (F4)
    #[test]
    fn silent_subscription_becomes_stale() {
        let mut l = Lane::new(P, 0);
        l.on_subscribed(0);
        l.on_tick(6_000);
        assert_eq!(l.state, LaneState::Stale);
    }

    /// DN-P1-WS-005: Unterbrechung erzeugt genau eine Lücke ab letzter Nachricht.
    #[test]
    fn disconnect_records_gap_from_last_message() {
        let mut l = Lane::new(P, 0);
        l.on_subscribed(0);
        l.on_valid_message(100);
        l.on_disconnected(5_000);
        l.on_subscribed(6_000);
        l.on_valid_message(7_000);
        assert_eq!(l.closed_gaps, vec![(0, 100), (100, 7_000)]);
    }

    /// DN-P1-WS-006 (F5)
    #[test]
    fn decode_failure_is_unavailable() {
        let mut l = Lane::new(P, 0);
        l.on_decode_failure(10);
        assert_eq!(l.state, LaneState::Unavailable);
    }
}
```

## 16.5 `dn-agg/src/trades_1s.rs`: 1s-Aggregator

Ein Code für live, Rekonstruktion und Backtest (T23, K04).

```rust
//! 1s-Bars aus Rohtrades. Herkunft: ws_ai backend/services/adapter/candle_agg_1s.py @1f27ed3
//! (OHLCV, Quote-Volumen, Anzahl, Stale-Flush). Regeln P1-F07/F11/F12:
//! UTC, [t, t+1000), Zuordnung nach Börsenzeit, Abschluss per Watermark, veröffentlichte Bars unveränderlich.
//! Korrekturen: F6 keine stillen Fehler (Result); F7 mehrere offene Sekunden, verspätete Trades verändern
//! keinen finalen Bar und erzeugen kein Duplikat (LATE-Ereignis); F8 Ganzzahlen statt Decimal-Clamp.
//! Neu (TEIL1/T27): Zeit von High/Low, Kauf/Verkauf, größter Trade, Trades je ms, erste/letzte Trade-ID.

use dn_types::trading::{Lots, Side, Ticks};
use std::collections::BTreeMap;

pub const AGGREGATOR_VERSION: u16 = 1;

/// Kanonische Reihenfolge (exch_ts_ms, source_seq) nach P1-T26.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggTrade {
    pub exch_ts_ms: i64,
    pub source_seq: u64,
    pub trade_id: String, // opak, verlustfrei (P1-F05)
    pub price_ticks: Ticks,
    pub qty_lots: Lots,
    pub aggressor: Side,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarQuality {
    Complete,
    /// Das Intervall überschneidet eine belegte Datenlücke ⇒ für Backtests sperrbar.
    Gap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bar {
    pub start_ms: i64,
    pub span_ms: i64,
    pub open: Ticks,
    pub high: Ticks,
    pub low: Ticks,
    pub close: Ticks,
    pub ts_high_ms: i64,
    pub ts_low_ms: i64,
    pub volume_lots: Lots,
    pub quote_ticks_lots: i128,
    pub trades: u32,
    pub buy_lots: Lots,
    pub sell_lots: Lots,
    pub buy_trades: u32,
    pub sell_trades: u32,
    pub max_trade_lots: Lots,
    pub max_trades_same_ms: u32,
    pub first_key: (i64, u64),
    pub last_key: (i64, u64),
    pub first_trade_id: String,
    pub last_trade_id: String,
    pub quality: BarQuality,
    pub aggregator_version: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggError {
    NonPositive,
    Overflow,
    DuplicateKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AggEvent {
    Final(Bar),
    /// P1-F12: finaler Bar bleibt unverändert; Research-Rebuild unter neuer Dataset-Version.
    Late { trade_ts_ms: i64, second_start_ms: i64 },
}

struct Builder {
    bar: Bar,
    ms_counts: BTreeMap<i64, u32>,
}

impl Builder {
    fn new(start: i64, t: &AggTrade) -> Result<Self, AggError> {
        let key = (t.exch_ts_ms, t.source_seq);
        let quote = i128::from(t.price_ticks).checked_mul(i128::from(t.qty_lots)).ok_or(AggError::Overflow)?;
        let (buy, sell, bt, st) = match t.aggressor {
            Side::Buy => (t.qty_lots, 0, 1, 0),
            Side::Sell => (0, t.qty_lots, 0, 1),
        };
        let mut ms_counts = BTreeMap::new();
        ms_counts.insert(t.exch_ts_ms, 1);
        Ok(Self {
            bar: Bar {
                start_ms: start, span_ms: 1_000,
                open: t.price_ticks, high: t.price_ticks, low: t.price_ticks, close: t.price_ticks,
                ts_high_ms: t.exch_ts_ms, ts_low_ms: t.exch_ts_ms,
                volume_lots: t.qty_lots, quote_ticks_lots: quote, trades: 1,
                buy_lots: buy, sell_lots: sell, buy_trades: bt, sell_trades: st,
                max_trade_lots: t.qty_lots, max_trades_same_ms: 0,
                first_key: key, last_key: key,
                first_trade_id: t.trade_id.clone(), last_trade_id: t.trade_id.clone(),
                quality: BarQuality::Complete, aggregator_version: AGGREGATOR_VERSION,
            },
            ms_counts,
        })
    }

    fn add(&mut self, t: &AggTrade) -> Result<(), AggError> {
        let b = &mut self.bar;
        let key = (t.exch_ts_ms, t.source_seq);
        if key == b.first_key || key == b.last_key {
            return Err(AggError::DuplicateKey);
        }
        // Open/Close nach kanonischer Reihenfolge, NICHT nach Ankunft (F7/F10).
        if key < b.first_key {
            b.open = t.price_ticks;
            b.first_key = key;
            b.first_trade_id = t.trade_id.clone();
        }
        if key > b.last_key {
            b.close = t.price_ticks;
            b.last_key = key;
            b.last_trade_id = t.trade_id.clone();
        }
        if t.price_ticks > b.high || (t.price_ticks == b.high && t.exch_ts_ms < b.ts_high_ms) {
            b.high = t.price_ticks;
            b.ts_high_ms = t.exch_ts_ms;
        }
        if t.price_ticks < b.low || (t.price_ticks == b.low && t.exch_ts_ms < b.ts_low_ms) {
            b.low = t.price_ticks;
            b.ts_low_ms = t.exch_ts_ms;
        }
        b.volume_lots = b.volume_lots.checked_add(t.qty_lots).ok_or(AggError::Overflow)?;
        let q = i128::from(t.price_ticks).checked_mul(i128::from(t.qty_lots)).ok_or(AggError::Overflow)?;
        b.quote_ticks_lots = b.quote_ticks_lots.checked_add(q).ok_or(AggError::Overflow)?;
        b.trades = b.trades.checked_add(1).ok_or(AggError::Overflow)?;
        match t.aggressor {
            Side::Buy => {
                b.buy_lots = b.buy_lots.checked_add(t.qty_lots).ok_or(AggError::Overflow)?;
                b.buy_trades += 1;
            }
            Side::Sell => {
                b.sell_lots = b.sell_lots.checked_add(t.qty_lots).ok_or(AggError::Overflow)?;
                b.sell_trades += 1;
            }
        }
        b.max_trade_lots = b.max_trade_lots.max(t.qty_lots);
        *self.ms_counts.entry(t.exch_ts_ms).or_insert(0) += 1;
        Ok(())
    }

    fn finish(self, quality: BarQuality) -> Bar {
        let mut b = self.bar;
        b.max_trades_same_ms = self.ms_counts.values().copied().max().unwrap_or(0);
        b.quality = quality;
        b
    }
}

pub struct Agg1s {
    open: BTreeMap<i64, Builder>,
    /// Alle Sekunden mit start < finalized_until sind final (exklusiv).
    finalized_until_ms: i64,
    grace_ms: i64,
    gaps: Vec<(i64, i64)>,
    pub late_trades: u64,
}

impl Agg1s {
    /// `grace_ms` = gemessene Karenz (R3/R4), aus der Registry.
    pub fn new(grace_ms: i64) -> Self {
        Self { open: BTreeMap::new(), finalized_until_ms: i64::MIN, grace_ms, gaps: Vec::new(), late_trades: 0 }
    }

    /// Lücke [start, end) aus Lane/Coverage-Ledger (§16.4).
    pub fn mark_gap(&mut self, start_ms: i64, end_ms: i64) {
        if end_ms > start_ms {
            self.gaps.push((start_ms, end_ms));
        }
    }

    pub fn on_trade(&mut self, t: &AggTrade, out: &mut Vec<AggEvent>) -> Result<(), AggError> {
        if t.qty_lots <= 0 || t.price_ticks <= 0 {
            return Err(AggError::NonPositive); // F6: Fehler sichtbar, Aufrufer markiert Qualität
        }
        let start = t.exch_ts_ms.div_euclid(1_000) * 1_000;
        if start < self.finalized_until_ms {
            self.late_trades += 1;
            out.push(AggEvent::Late { trade_ts_ms: t.exch_ts_ms, second_start_ms: start });
            return Ok(());
        }
        match self.open.get_mut(&start) {
            Some(b) => b.add(t),
            None => {
                self.open.insert(start, Builder::new(start, t)?);
                Ok(())
            }
        }
    }

    /// Watermark = höchste gesicherte Börsenzeit (Regel aus R3). Sekunde S ist final, wenn S+1000 ≤ wm − grace.
    pub fn advance(&mut self, watermark_ms: i64, out: &mut Vec<AggEvent>) {
        let until = (watermark_ms - self.grace_ms).div_euclid(1_000) * 1_000;
        if until <= self.finalized_until_ms {
            return;
        }
        let still_open = self.open.split_off(&until);
        let done = std::mem::replace(&mut self.open, still_open);
        for (start, b) in done {
            let q = if self.gaps.iter().any(|(s, e)| start < *e && start + 1_000 > *s) {
                BarQuality::Gap
            } else {
                BarQuality::Complete
            };
            out.push(AggEvent::Final(b.finish(q)));
        }
        self.finalized_until_ms = until;
        self.gaps.retain(|(_, e)| *e > until);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(ts: i64, seq: u64, px: Ticks, qty: Lots, ag: Side) -> AggTrade {
        AggTrade { exch_ts_ms: ts, source_seq: seq, trade_id: format!("{ts}-{seq}"), price_ticks: px, qty_lots: qty, aggressor: ag }
    }

    fn finals(ev: &[AggEvent]) -> Vec<&Bar> {
        ev.iter().filter_map(|e| if let AggEvent::Final(b) = e { Some(b) } else { None }).collect()
    }

    /// T08: 999 ms gehört zu [0,1000), 1000 ms zu [1000,2000).
    #[test]
    fn half_open_seconds() {
        let mut a = Agg1s::new(0);
        let mut ev = vec![];
        a.on_trade(&t(999, 1, 100, 1, Side::Buy), &mut ev).unwrap();
        a.on_trade(&t(1_000, 1, 101, 1, Side::Buy), &mut ev).unwrap();
        a.advance(2_000, &mut ev);
        let f = finals(&ev);
        assert_eq!((f[0].start_ms, f[0].close, f[1].start_ms, f[1].open), (0, 100, 1_000, 101));
    }

    /// DN-P1-AGG-001 (F7): Verspäteter Trade für eine OFFENE frühere Sekunde schließt nichts vorzeitig.
    #[test]
    fn out_of_order_within_open_window() {
        let mut a = Agg1s::new(1_500);
        let mut ev = vec![];
        a.on_trade(&t(1_100, 1, 100, 1, Side::Buy), &mut ev).unwrap();
        a.on_trade(&t(900, 1, 99, 1, Side::Sell), &mut ev).unwrap();
        assert!(ev.is_empty());
        a.advance(4_000, &mut ev);
        assert_eq!(finals(&ev).len(), 2);
    }

    /// DN-P1-AGG-002 (F7): Trade für FINALE Sekunde ⇒ LATE, kein zweiter Bar, kein Überschreiben.
    #[test]
    fn late_trade_never_rewrites_final_bar() {
        let mut a = Agg1s::new(0);
        let mut ev = vec![];
        a.on_trade(&t(500, 1, 100, 2, Side::Buy), &mut ev).unwrap();
        a.advance(1_000, &mut ev);
        a.on_trade(&t(700, 2, 50, 1, Side::Sell), &mut ev).unwrap();
        a.advance(5_000, &mut ev);
        assert_eq!(finals(&ev).len(), 1);
        assert!(ev.iter().any(|e| matches!(e, AggEvent::Late { second_start_ms: 0, .. })));
    }

    /// T26/DN-P1-AGG-003 (F10): gleiche ms, Open/Close nach source_seq – unabhängig von der Ankunft.
    #[test]
    fn same_ms_open_close_by_seq() {
        let mut a = Agg1s::new(0);
        let mut ev = vec![];
        a.on_trade(&t(10, 3, 103, 1, Side::Buy), &mut ev).unwrap();
        a.on_trade(&t(10, 1, 101, 1, Side::Buy), &mut ev).unwrap();
        a.on_trade(&t(10, 2, 102, 1, Side::Sell), &mut ev).unwrap();
        a.advance(1_000, &mut ev);
        let b = finals(&ev)[0];
        assert_eq!((b.open, b.close, b.max_trades_same_ms, b.buy_lots, b.sell_lots), (101, 103, 3, 2, 1));
    }

    /// DN-P1-AGG-004: Lücke markiert Qualität; kein Flat-Bar für tradefreie Sekunden.
    #[test]
    fn gap_marks_quality_and_no_flat_bars() {
        let mut a = Agg1s::new(0);
        let mut ev = vec![];
        a.mark_gap(1_500, 2_500);
        a.on_trade(&t(1_100, 1, 100, 1, Side::Buy), &mut ev).unwrap();
        a.on_trade(&t(3_100, 1, 100, 1, Side::Buy), &mut ev).unwrap();
        a.advance(4_000, &mut ev);
        let f = finals(&ev);
        assert_eq!(f.len(), 2); // Sekunde 2 hat keinen Bar
        assert_eq!((f[0].quality, f[1].quality), (BarQuality::Gap, BarQuality::Complete));
    }
}
```

## 16.6 `dn-agg/src/mtf.rs` und `live_tf.rs`

```rust
//! MTF aus finalen 1s-Bars (Monoid) statt jeder TF separat aus Trades (F9).
//! Damit sind 5s/10s/30s/1m/… exakt gleich „Rohtrades → TF direkt“ (T27).

use crate::trades_1s::{AggError, Bar, BarQuality};

pub fn fold_into(acc: &mut Bar, x: &Bar) -> Result<(), AggError> {
    if x.first_key < acc.first_key {
        acc.open = x.open;
        acc.first_key = x.first_key;
        acc.first_trade_id = x.first_trade_id.clone();
    }
    if x.last_key > acc.last_key {
        acc.close = x.close;
        acc.last_key = x.last_key;
        acc.last_trade_id = x.last_trade_id.clone();
    }
    if x.high > acc.high || (x.high == acc.high && x.ts_high_ms < acc.ts_high_ms) {
        acc.high = x.high;
        acc.ts_high_ms = x.ts_high_ms;
    }
    if x.low < acc.low || (x.low == acc.low && x.ts_low_ms < acc.ts_low_ms) {
        acc.low = x.low;
        acc.ts_low_ms = x.ts_low_ms;
    }
    acc.volume_lots = acc.volume_lots.checked_add(x.volume_lots).ok_or(AggError::Overflow)?;
    acc.quote_ticks_lots = acc.quote_ticks_lots.checked_add(x.quote_ticks_lots).ok_or(AggError::Overflow)?;
    acc.trades = acc.trades.checked_add(x.trades).ok_or(AggError::Overflow)?;
    acc.buy_lots = acc.buy_lots.checked_add(x.buy_lots).ok_or(AggError::Overflow)?;
    acc.sell_lots = acc.sell_lots.checked_add(x.sell_lots).ok_or(AggError::Overflow)?;
    acc.buy_trades += x.buy_trades;
    acc.sell_trades += x.sell_trades;
    acc.max_trade_lots = acc.max_trade_lots.max(x.max_trade_lots);
    acc.max_trades_same_ms = acc.max_trades_same_ms.max(x.max_trades_same_ms); // ms liegt immer in genau einer Sekunde
    if x.quality == BarQuality::Gap {
        acc.quality = BarQuality::Gap;
    }
    Ok(())
}

/// Inkrementell: 1s-Finals rein, TF-Bar raus, sobald das TF-Fenster vollständig final ist.
pub struct MtfAgg {
    pub span_ms: i64,
    current: Option<Bar>,
}

impl MtfAgg {
    pub fn new(span_ms: i64) -> Self {
        Self { span_ms, current: None }
    }

    pub fn on_final_1s(&mut self, b: &Bar, out: &mut Vec<Bar>) -> Result<(), AggError> {
        let start = b.start_ms.div_euclid(self.span_ms) * self.span_ms;
        match &mut self.current {
            Some(c) if c.start_ms == start => fold_into(c, b),
            _ => {
                if let Some(done) = self.current.take() {
                    out.push(done);
                }
                let mut nb = b.clone();
                nb.start_ms = start;
                nb.span_ms = self.span_ms;
                self.current = Some(nb);
                Ok(())
            }
        }
    }

    /// `finalized_until_ms` aus Agg1s: TF-Bar erst schließen, wenn sein ganzes Fenster final ist.
    pub fn advance(&mut self, finalized_until_ms: i64, out: &mut Vec<Bar>) {
        if self.current.as_ref().is_some_and(|c| c.start_ms + self.span_ms <= finalized_until_ms) {
            if let Some(done) = self.current.take() {
                out.push(done);
            }
        }
    }
}
```

```rust
//! dn-agg/src/live_tf.rs – aus ws_ai stream_aggregator.py `ResolutionRegistry` (@1f27ed3):
//! Basis-TFs immer aktiv, angefragte TFs mit TTL auf monotoner Uhr.
//! In DarkNode steuert sie NUR den Live-Push an Charts. Berechnet und gespeichert (L2) werden
//! alle konfigurierten TFs immer, aus 1s und rebuildbar.

use std::collections::{BTreeMap, BTreeSet};

pub struct LiveTfRegistry {
    defaults: BTreeSet<i64>,
    ttl_ms: i64,
    requested: BTreeMap<i64, i64>, // span_ms → letzter Zugriff (monoton)
}

impl LiveTfRegistry {
    pub fn new(defaults: impl IntoIterator<Item = i64>, ttl_ms: i64) -> Self {
        Self { defaults: defaults.into_iter().collect(), ttl_ms, requested: BTreeMap::new() }
    }

    pub fn touch(&mut self, span_ms: i64, mono_now_ms: i64) {
        self.requested.insert(span_ms, mono_now_ms);
    }

    pub fn active(&mut self, mono_now_ms: i64) -> BTreeSet<i64> {
        let ttl = self.ttl_ms;
        self.requested.retain(|_, last| mono_now_ms - *last <= ttl);
        self.defaults.iter().copied().chain(self.requested.keys().copied()).collect()
    }
}
```

## 16.7 `dn-agg/src/rebuild.rs`: historische Rekonstruktion

Derselbe Aggregator wie live.

```rust
//! Ersetzt ws_ai historical_candles_from_trades.py (@1f27ed3, ClickHouse-SQL) durch denselben Rust-Code wie live.
//! Korrekturen: F10 deterministische Ordnung (ts, seq) statt argMin/argMax; F11 alle Quellen gemeinsam,
//! dedupliziert, Qualität statt Quellen-Rang; F12 halboffenes Fenster [start, end).

use crate::trades_1s::{Agg1s, AggError, AggEvent, AggTrade, Bar};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq)]
pub enum RebuildError {
    Unaligned,
    Agg(AggError),
    /// Gleiche Trade-ID, abweichende Nutzlast (P1-F06) ⇒ kein Bar, Konfliktbericht.
    Conflict { trade_id: String },
}

pub fn rebuild_1s(
    trades: impl IntoIterator<Item = AggTrade>,
    start_ms: i64,
    end_ms: i64,
    gaps: &[(i64, i64)],
) -> Result<Vec<Bar>, RebuildError> {
    if start_ms % 1_000 != 0 || end_ms % 1_000 != 0 || end_ms <= start_ms {
        return Err(RebuildError::Unaligned);
    }
    // Quellen zusammenführen (Live-L1 + Archiv + REST-Reparatur), Dedupe über Trade-ID.
    let mut by_id: HashMap<String, AggTrade> = HashMap::new();
    for t in trades.into_iter().filter(|t| t.exch_ts_ms >= start_ms && t.exch_ts_ms < end_ms) {
        match by_id.get(&t.trade_id) {
            None => {
                by_id.insert(t.trade_id.clone(), t);
            }
            Some(old) => {
                let same = old.exch_ts_ms == t.exch_ts_ms && old.price_ticks == t.price_ticks
                    && old.qty_lots == t.qty_lots && old.aggressor == t.aggressor;
                if !same {
                    return Err(RebuildError::Conflict { trade_id: t.trade_id });
                }
            }
        }
    }
    let mut sorted: Vec<AggTrade> = by_id.into_values().collect();
    sorted.sort_by_key(|t| (t.exch_ts_ms, t.source_seq));

    let mut agg = Agg1s::new(0);
    for (s, e) in gaps {
        agg.mark_gap(*s, *e);
    }
    let mut ev = Vec::new();
    for t in &sorted {
        agg.on_trade(t, &mut ev).map_err(RebuildError::Agg)?;
    }
    agg.advance(end_ms, &mut ev);
    Ok(ev.into_iter().filter_map(|e| if let AggEvent::Final(b) = e { Some(b) } else { None }).collect())
}
```

`source_seq` muss für Archiv- und REST-Trades dieselbe kanonische Ordnung abbilden wie live (T26-Regel). Solange T26 nicht entschieden ist, kann die Reihenfolge von Trades derselben Millisekunde zwischen Quellen abweichen. Der Rebuild ist dann zwar deterministisch, aber noch nicht live-gleich. Das prüft DN-P1-AGG-008 (Live vs. Rebuild am selben Capture, Hash-Gleichheit).

## 16.8 `frontend/src/features/darknode/chart/series.ts`

```ts
// Herkunft: ws_ai frontend/src/pages/TradingPage/hooks/useChartView.ts @1f27ed3
//   – nie synthetisches OHLC, Whitespace-Punkte, Limit zählt nur echte Kerzen, Stride bei großen Lücken.
// Korrektur F14: Lücken kommen aus dem Backend (Protobuf-Qualität + Coverage-Ranges), nicht aus Zeitabständen.
// Preise kommen als Ganzzahl-Ticks + Scale; Umrechnung in number NUR für die Anzeige.

export type BarQuality = "COMPLETE" | "GAP";

export interface ChartBarIn {
  startMs: number;
  openTicks: bigint; highTicks: bigint; lowTicks: bigint; closeTicks: bigint;
  volumeLots: bigint;
  quality: BarQuality;
}

export interface GapRange { startMs: number; endMs: number } // [start, end) aus Coverage-Ledger

export type SeriesPoint =
  | { time: number; open: number; high: number; low: number; close: number }
  | { time: number }; // Whitespace

const toNum = (v: bigint, scale: number) => Number(v) / 10 ** scale; // Anzeige only

export function buildSeries(
  bars: ChartBarIn[], gaps: GapRange[], stepMs: number,
  priceScale: number, maxReal: number, maxWhitespacePerGap: number,
): SeriesPoint[] {
  const real = [...bars].sort((a, b) => a.startMs - b.startMs);
  const window = maxReal > 0 && real.length > maxReal ? real.slice(-maxReal) : real;
  const out: SeriesPoint[] = window.map((b) => ({
    time: b.startMs / 1000,
    open: toNum(b.openTicks, priceScale), high: toNum(b.highTicks, priceScale),
    low: toNum(b.lowTicks, priceScale), close: toNum(b.closeTicks, priceScale),
  }));
  if (window.length === 0) return out;
  const from = window[0].startMs;
  const to = window[window.length - 1].startMs;
  // Whitespace NUR für belegte Lücken; tradefreie Sekunden bleiben leer (normaler Markt).
  for (const g of gaps) {
    const s = Math.max(g.startMs, from);
    const e = Math.min(g.endMs, to);
    const n = Math.floor((e - s) / stepMs);
    if (n <= 0) continue;
    const stride = maxWhitespacePerGap > 0 ? Math.ceil(n / maxWhitespacePerGap) : n + 1;
    for (let k = 0; k < n; k += stride) out.push({ time: (s + k * stepMs) / 1000 });
  }
  return out.sort((a, b) => a.time - b.time);
}
```

Muster aus `WebSocketPool.ts`, die beim Abgleich mit DarkBots Realtime-Client (P0-Feldinventur) geprüft und nur bei Fehlen ergänzt werden:
- Generationszähler gegen Callbacks alter Verbindungen
- Referenzzählung mit verzögertem Schließen
- Resubscribe bei jedem OPEN

Einen zweiten Browser-WS gibt es nicht; binäre Protobuf-Frames werden nie verworfen (F13).

## 16.9 Auswirkungen auf die Code-Pakete

| Paket | Änderung durch §16 |
|---|---|
| CODE-01 | Zusätzlich `dn-bitget/src/ws/{backoff,lane}.rs` aus `ws_ai` (§16.3–16.4) |
| CODE-02 | `ws_message_parsers.py` wird **nicht** portiert; SBE-Codegen ersetzt ihn (T01) |
| CODE-03 | 1s-Aggregator, MTF, Rebuild, Live-TF-Registry aus `ws_ai` korrigiert (§16.5–16.7) statt Neuentwurf |
| CODE-05 | Backtest-Kern bleibt neu (LEAN/HftBacktest, V2 §13); `ws_ai` hat nur einen Platzhalter |
| CODE-06 | Backfill-Logik aus `unified_historical`/`backfill_loop`/`gap_scan` (WS_AI_IDEEN 7–10, 21–22) in `dn-backfill` |
| CODE-08 | Chart-Serienaufbau aus `useChartView.ts` korrigiert (§16.8); Pool-Muster nur nach Abgleich |

**Neue Test-IDs** (vor Nutzung im TESTPROTOKOLL registrieren):

| ID | Prüft |
|---|---|
| DN-P1-WS-001 | Backoff-Formel und Deckel |
| DN-P1-WS-002 | Kein Reset durch Flapping |
| DN-P1-WS-003 | Nie aufgeben, Alarm |
| DN-P1-WS-004 | Stumme Subscription ⇒ STALE |
| DN-P1-WS-005 | Lückenintervall bei Disconnect |
| DN-P1-WS-006 | Decoder-Fehler ⇒ UNAVAILABLE |
| DN-P1-AGG-001 | Out-of-order im offenen Fenster |
| DN-P1-AGG-002 | Late-Trade überschreibt keinen finalen Bar |
| DN-P1-AGG-003 | Open/Close bei gleicher ms nach Seq |
| DN-P1-AGG-004 | Lückenqualität, kein Flat-Bar |
| DN-P1-AGG-005 | MTF aus 1s = MTF direkt aus Trades |
| DN-P1-AGG-006 | Rebuild halboffen |
| DN-P1-AGG-007 | Quellen-Dedupe und Konflikt |
| DN-P1-AGG-008 | Live vs. Rebuild hash-gleich |

## 16.10 Bewusst nicht aus `ws_ai` übernommen

| Was | Warum |
|---|---|
| Versionsrang „Live schlägt Backfill“ (`ver = ts·1000+1000+c`) | F7/F11: Rang nach Quelle statt nach Vollständigkeit überschreibt vollständige Daten mit lückenhaften. Ersetzt durch Dataset-Version + Qualität (P1-F06/F12). |
| ClickHouse als Kerzen-Wahrheit, Redis, Python im Datenpfad | Architekturentscheid (P1 §1.1) |
| JSON-Parsing im Browser-Pool | DN-D18; Protobuf binär |
| `return None` bei Aggregatorfehlern, Aufgeben nach 50 Reconnects | F1, F6 |
| `backtesting_engine.py` | Platzhalter ohne Logik |

---

# TEIL III – von V2 ausdrücklich referenzierte V1-Funktionen (nur Auth/Order-DTO)

**Grund:** Die V2-Vorlage schreibt, `auth.rs` und `orders.rs` seien unverändert aus V1 zu übernehmen, enthält sie aber nicht. Damit diese Datei wirklich eigenständig lesbar ist, folgen **nur diese beiden historischen Quellblöcke** aus dem früheren Entwurf. **Keine neue Freigabe:** Auth-Golden im Original sogar `#[ignore]`, mehrere API-Felder nur nach Bitget-Demo/Endpointvertrag gültig. 

### 3.1 `dn-bitget/src/private/auth.rs`

```rust
//! Bitget UTA V3 – Signatur für REST und privaten WS-Login.
//!
//! Verfahren portiert aus Hummingbot
//!   hummingbot/connector/derivative/bitget_unified_perpetual/bitget_unified_perpetual_auth.py
//!   @06e491ca144045568a57d43e746a6e9b183241af (Apache-2.0)
//!   Funktionen: _union_params, _generate_signature, rest_authenticate, get_ws_auth_payload
//! Änderungen: signiert exakt die zu sendenden Bytes; Secrets nie in Debug/Logs;
//! WS-Zeitstempel-Einheit explizit (Hummingbot: Sekunden, Bitget-Quick-Start: ms → Demo-Test).

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::fmt;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

impl Method {
    pub const fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

/// Schlüsselmaterial. `Debug` gibt nie Inhalte aus.
pub struct Credentials {
    api_key: String,
    secret: Vec<u8>,
    passphrase: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credentials(<redacted>)")
    }
}

impl Credentials {
    pub fn new(api_key: String, secret: String, passphrase: String) -> Self {
        Self { api_key, secret: secret.into_bytes(), passphrase }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    InvalidKey,
}

fn hmac_b64(secret: &[u8], msg: &[u8]) -> Result<String, AuthError> {
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::InvalidKey)?;
    mac.update(msg);
    Ok(B64.encode(mac.finalize().into_bytes()))
}

/// `timestamp + METHOD + path[?query] + body` – entspricht Hummingbot `_union_params`.
/// `body` = exakt die Bytes, die über die Leitung gehen (bei GET leer).
pub fn prehash(ts: &str, method: Method, path_and_query: &str, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(ts.len() + 4 + path_and_query.len() + body.len());
    v.extend_from_slice(ts.as_bytes());
    v.extend_from_slice(method.as_str().as_bytes());
    v.extend_from_slice(path_and_query.as_bytes());
    v.extend_from_slice(body);
    v
}

/// Query genau einmal bauen und dieselbe Zeichenkette signieren UND senden.
/// `form_urlencoded` kodiert Leerzeichen als '+', wie Pythons `urlencode`.
pub fn path_with_query(path: &str, params: &[(&str, &str)]) -> String {
    if params.is_empty() {
        return path.to_owned();
    }
    let q = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter().copied())
        .finish();
    format!("{path}?{q}")
}

pub struct SignedHeaders {
    pub access_key: String,
    pub access_sign: String,
    pub access_timestamp: String,
    pub access_passphrase: String,
}

impl fmt::Debug for SignedHeaders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedHeaders")
            .field("access_timestamp", &self.access_timestamp)
            .finish_non_exhaustive()
    }
}

/// REST: Zeitstempel in Millisekunden (Hummingbot und Bitget-Doku übereinstimmend).
pub fn sign_rest(
    c: &Credentials,
    unix_ms: u64,
    method: Method,
    path_and_query: &str,
    body: &[u8],
) -> Result<SignedHeaders, AuthError> {
    let ts = unix_ms.to_string();
    let sig = hmac_b64(&c.secret, &prehash(&ts, method, path_and_query, body))?;
    Ok(SignedHeaders {
        access_key: c.api_key.clone(),
        access_sign: sig,
        access_timestamp: ts,
        access_passphrase: c.passphrase.clone(),
    })
}

/// OFFEN (DN-P5-UP-013): Hummingbot sendet Sekunden, P5 V1.1/Quick-Start nennt ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WsTsUnit {
    Seconds,
    Millis,
}

pub struct WsLogin {
    pub api_key: String,
    pub passphrase: String,
    pub timestamp: String,
    pub sign: String,
}

/// Signatur über `ts + "GET" + "/user/verify"` (Hummingbot `get_ws_auth_payload`).
pub fn ws_login(c: &Credentials, unix_ms: u64, unit: WsTsUnit) -> Result<WsLogin, AuthError> {
    let ts = match unit {
        WsTsUnit::Seconds => (unix_ms / 1000).to_string(),
        WsTsUnit::Millis => unix_ms.to_string(),
    };
    let sign = hmac_b64(&c.secret, &prehash(&ts, Method::Get, "/user/verify", b""))?;
    Ok(WsLogin {
        api_key: c.api_key.clone(),
        passphrase: c.passphrase.clone(),
        timestamp: ts,
        sign,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DN-P5-UP-014: Testvektoren aus der Hummingbot-Referenzfunktion erzeugt
    /// (Skript in §3.1a), Ergebnis hier als Konstante eingetragen.
    const GOLDEN_GET_ASSETS: &str = "<aus §3.1a erzeugen und eintragen>";

    #[test]
    #[ignore = "Golden erst nach Erzeugung mit §3.1a eintragen"]
    fn rest_signature_matches_hummingbot_reference() {
        let c = Credentials::new("k".into(), "test-secret".into(), "p".into());
        let h = sign_rest(&c, 1_700_000_000_000, Method::Get, "/api/v3/account/assets", b"").unwrap();
        assert_eq!(h.access_sign, GOLDEN_GET_ASSETS);
    }

    #[test]
    fn query_is_built_once_in_given_order() {
        let p = path_with_query("/api/v3/trade/fills", &[("orderId", "123"), ("limit", "100")]);
        assert_eq!(p, "/api/v3/trade/fills?orderId=123&limit=100");
    }

    #[test]
    fn debug_never_leaks_secrets() {
        let c = Credentials::new("KEY".into(), "SECRET".into(), "PASS".into());
        let s = format!("{c:?}");
        assert!(!s.contains("KEY") && !s.contains("SECRET") && !s.contains("PASS"));
    }
}
```

**§3.1a – Golden-Erzeugung (nur Werkzeug, nicht im Datenpfad).** Das Skript bildet Hummingbots `_union_params` und `_generate_signature` 1:1 nach:

```python
import base64, hmac
def union(ts, method, path, body): return str(ts) + method.upper() + path + ("" if body in ("None", "null") else body)
def sig(secret, msg): return base64.b64encode(hmac.new(secret.encode(), msg.encode(), "sha256").digest()).decode().strip()
print(sig("test-secret", union("1700000000000", "GET", "/api/v3/account/assets", "")))
print(sig("test-secret", union("1700000000", "GET", "/user/verify", "")))
```

### 3.3 `dn-bitget/src/private/orders.rs`: Place-Order-Body und Statusabbildung

JSON entsteht ausschließlich hier am Adapterrand (DN-D18). Die Feldreihenfolge ist durch die Struct fest, damit die signierten Bytes reproduzierbar sind.

```rust
//! V3 place-order – Felder und Regeln aus Hummingbot `_place_order`
//! (bitget_unified_perpetual_derivative.py @06e491ca, Apache-2.0):
//!   category/symbol/qty/side/orderType/clientOid/marginMode pro Order,
//!   reduceOnly="yes" beim Schließen AUCH in One-way, posSide nur in Hedge.
//! NICHT übernommen: Header X-CHANNEL-API-CODE (Hummingbot-Brokerkennung).
//! Zu verifizieren (Demo, P5-A17): Feldnamen des Preset-Stops (stopLoss/slTriggerBy).

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOrderV3<'a> {
    pub category: &'a str,   // aus Instrument-Katalog/Registry, nicht hart kodiert
    pub symbol: &'a str,
    pub qty: String,         // exakt aus Lots formatiert (format_lots)
    pub side: &'a str,       // "buy" | "sell"
    pub order_type: &'a str, // V1: "market"
    pub client_oid: &'a str,
    pub margin_mode: &'a str, // V1 fest "isolated" (K11)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<&'a str>, // Some("yes") bei jedem Close
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sl_trigger_by: Option<&'a str>, // "market" | "mark" (laut Bitget.Net PR #115, ungeprüft)
}

/// Body genau einmal serialisieren; diese Bytes werden signiert UND gesendet.
pub fn body_bytes(o: &PlaceOrderV3<'_>) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(o)
}

/// Lots → exakte Dezimalzeichenkette. Beispiel: lots=149, scale=4 → "0.0149".
pub fn format_lots(lots: i64, scale: u32) -> Option<String> {
    if lots < 0 {
        return None;
    }
    let div = 10i64.checked_pow(scale)?;
    let int = lots / div;
    let frac = lots % div;
    Some(if scale == 0 {
        int.to_string()
    } else {
        format!("{int}.{frac:0width$}", width = scale as usize)
    })
}

/// Rohstatus der Börse. Unbekannt ⇒ niemals "nicht gefunden" (K8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusHint {
    Working,
    PartiallyFilled,
    Filled,
    Cancelled,
    Unrecognized(String),
}

/// Liste aus Demo-Antworten vervollständigen (Hummingbot `_order_state_for` nicht gelesen).
pub fn map_v3_status(s: &str) -> StatusHint {
    match s {
        "new" | "live" => StatusHint::Working,
        "partially_filled" => StatusHint::PartiallyFilled,
        "filled" => StatusHint::Filled,
        "cancelled" | "canceled" => StatusHint::Cancelled,
        other => StatusHint::Unrecognized(other.to_owned()),
    }
}

/// K13: beide beobachteten Schreibweisen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldMode {
    OneWay,
    Hedge,
}

pub fn parse_hold_mode(s: &str) -> Option<HoldMode> {
    match s {
        "one_way_mode" | "single_hold" => Some(HoldMode::OneWay),
        "hedge_mode" | "double_hold" => Some(HoldMode::Hedge),
        _ => None, // ⇒ HALTED
    }
}
```

---

# TEIL IV – Abnahme zur späteren Implementierung

**Stand zum Erstellen dieser Datei:** Quellstand zusammengeführt, Dokumentations- und Reviewprüfungen vorgenommen. **Kein Code kompiliert, keine Tests ausgeführt, keine Bitget-Order gesendet.** Die internen Freigaben von `GATE_1.md`, `PHASENPLAN.md`, `PHASENPLAN_P1_V1.md`, `PHASENPLAN_P5_V1.md`, `WALLET_ACCOUNT_SYNC_V1.md` und `TESTPROTOKOLL.md` haben Vorrang. Bei einem Widerspruch zwischen hier enthaltenem Code und Abschnitt C gilt der **Sperr-/Korrekturvertrag** von C.

| Prüfschritt | Status zum Erstellzeitpunkt |
|---|---|
| Text-V2 + WS_AI-§16 vollständig enthalten | **JA, dokumentarisch** |
| V1-Auth und V1-Order-DTO ergänzt | **JA, dokumentarisch** |
| Gleiche-Millisecond-Reihenfolge Bitget L1 ↔ REST | **OPEN (P1-T26)** |
| V5-Decoder am echten XML getestet | **NOT_RUN/OPEN** |
| Rust-Workspace tatsächlich vorhanden | **NICHT AUS DIESER DATEI BELEGT** |
| Code gebaut / geclippyed / getestet | **NOT_RUN** |
| Safety-Blocker V3-B01–B24 geschlossen | **OPEN** |
| Demo-Live Bitget-Kontowahrheit/Stops getestet | **NOT_RUN** |
| Echtgeldfreigabe | **GESPERRT** |

# TEIL V – WS_AI Chart-Stack, Historien-Paging und Börsen-Adapter (Anhang zu §16)

> **Quelle 3, vollständig als historischer Erstbefund übernommen.** Die darauf folgende **Folgenachprüfung in Teil VI geht bei neu gelesenen Dateien und C2/C5/C6 vor**: Aussagen der Quelle 3 wie „tradingViewScroll.ts nicht gelesen“ sind damit historische Aussagen, **keine zum heutigen Zeitpunkt noch offenen Prüfaufträge**. `ws_ai` enthält eigene Lightweight-Charts-Dateien; der geplante v5-NPM-Import ist lediglich ein *konditionaler Migrationsvorschlag*, kein Auftrag zu einer zweiten Chartbibliothek. Bestehendes DarkBot-Gateway/Graphite bleibt der Host. Sämtliche im Anhang abgedruckten TS-Codebeispiele sind ungeprüfte **ENTWURFSMUSTER**.

Ja, es gibt noch Brauchbares: vor allem den **kompletten Chart-Stack** (sieben Dateien) und die Kette zum Nachladen der Historie. Ich habe dafür diese Runde den Quelltext gelesen (`ws_ai` @`1f27ed3`):
- `CandleChart.tsx`, `useCandleChart.ts`, `chartAdapter.ts`
- `useSafeCandleChart.ts`, `chartLazyLoader.ts`, `package.json`

Nicht gelesen habe ich `tradingViewScroll.ts`, `chartThemes.ts`, `LoadingBlockOverlay.tsx` und die Börsen-Adapter. Den Verzeichnisbaum kann ich bei GitHub nicht abrufen; zu den Börsen stütze ich mich auf das frühere Audit von `ws_config.py` und `ws_manager.py`.

## Deine drei Aussagen, geprüft

**1. „Lightweight Charts wurde verwendet“: Ja, aber anders als gedacht.**
- Die Bibliothek steht **nicht** in `package.json`.
- Sie wird zur Laufzeit als lokale Datei aus `/public` nachgeladen (`lightweight-charts.standalone.production.js`), als globales `window.LightweightCharts`.
- Die Version ist damit nirgends gepinnt. Der Code nutzt die **v4-API** (`chart.addCandlestickSeries(...)`). In v5 heißt das `chart.addSeries(CandlestickSeries, ...)`. Die Portierung muss diese API-Stelle also anpassen; die genaue Version steht im Kopf der Datei in `public/`.

**2. „Historie laden funktionierte super“: Die Bausteine sind gut, im gelesenen Code stecken aber drei Fehler.**

| Gut gelöst (belegt) | Gefundener Fehler (belegt) |
|---|---|
| Nachladen startet, wenn der sichtbare Bereich weniger als 5 Bars vom linken Rand entfernt ist; 400 ms Sperre gegen Mehrfachauslösung | **C1:** Das Nachlade-Signal ist ein **globales** `window`-Ereignis `wsai:need_history` ohne Chart-ID. Bei zwei Charts laden beide nach. Genau das verbietet P1-F15. |
| Erstansicht: die letzten 500 echten Kerzen | **C2:** `chartAdapter.apply` schreibt bei **unverändertem** letzten Zeitpunkt nur den letzten Punkt per `update`. Vorangestellte ältere Kerzen werden in diesem Pfad nicht an den Chart übergeben. Zusätzlich schneidet `useChartView` auf die letzten N echten Kerzen. Falls das Nachladen bei dir sichtbar funktionierte, läuft es über eine Stelle, die ich nicht gelesen habe (etwa `tradingViewScroll.ts`). Test T18 klärt das direkt. |
| Inkrementell: `setData` beim ersten Mal, danach nur `update` für neue Punkte; volles `setData`, wenn die Zeit zurückspringt (Symbolwechsel) | **C3:** Alle `safe*`-Funktionen schlucken Fehler mit leerem `catch {}`. Ein kaputter Datensatz bleibt unsichtbar. |
| Generationszähler gegen Callbacks eines alten Charts; sauberes Aufräumen von Observern | **C4:** Wenn das Laden der Bibliothek fehlschlägt, wartet ein **zweiter** Aufrufer ewig. Er pollt alle 50 ms nach `window.LightweightCharts`, ohne Timeout und ohne Fehler. |
| Lücken als Whitespace, nie synthetisches OHLC; Auto-Scroll nur, wenn der Nutzer am rechten Rand steht | – |
| Theme-Wechsel über MutationObserver, Größe über ResizeObserver | – |

**3. „Extrem viele Börsen sind erfolgreich gelaufen“: Im Code sind 8 Börsen verdrahtet. Laufzeitbelege habe ich keine.**
- Verdrahtet sind Binance, Bybit, Coinbase, Gate.io, HTX, MEXC, OKX und Bitget: WS-URL, Subscribe-Format, Parser und eine Factory, die Börsen für den Backfill automatisch findet.
- Wichtig: Das sind die **Spot**-Trade-Kanäle (z. B. `wss://stream.binance.com:9443/ws`, `wss://stream.bybit.com/v5/public/spot`). DarkNode handelt **Futures**; deren Endpunkte und Kanäle sind andere. Übertragbar ist das Wissen über die Eigenheiten je Börse, nicht die Endpunkte selbst.
- Solche Eigenheiten sind etwa: Symbolformat (`BTC-USDT`, `BTC_USDT`, Kleinschreibung bei Binance), HTX mit GZIP und eigenem Ping/Pong, MEXC mit Protobuf.

## Ergänzung zum Register (§16.2)

| `ws_ai`-Datei | Art | DarkNode-Ziel (P7, DarkBot-Graphite) |
|---|---|---|
| `frontend/src/lib/chartLazyLoader.ts` | NICHT (C4) | `lightweight-charts` per npm pinnen, Code-Splitting per `import()` |
| `frontend/src/hooks/useSafeCandleChart.ts` | PORT+FIX (C3, v5-API) | `features/darknode/chart/safeChart.ts` |
| `CandleChart/chartAdapter.ts` | PORT+FIX (C2) | `features/darknode/chart/adapter.ts` (unten) |
| `CandleChart/useCandleChart.ts` | PORT+FIX (C1) | `features/darknode/chart/useDarknodeChart.ts` (unten) |
| `CandleChart/CandleChart.tsx` | PORT | Graphite-Komponente; Lade-, Fehler- und Overlay-Zustände übernehmen |
| `CandleChart/LoadingBlockOverlay.tsx` | MUSTER (nicht gelesen) | Backfill-Fortschritt im Chart (WS_AI_IDEEN Nr. 28) |
| `CandleChart/tradingViewScroll.ts` | erst lesen, dann entscheiden | Auto-Scroll-Verhalten |
| Börsen-Adapter (8×, Spot) | MUSTER + Testfälle | Erst P10: Eigenheiten je Börse als Fixtures. Einträge bleiben `UNVERIFIED` in der Registry, bis gegen aktuelle Futures-Doku geprüft. |

Lizenz: Lightweight Charts ist Apache-2.0. Der NOTICE-Text und der Link zu tradingview.com gehören in die Seite. Das galt auch schon für die eingebettete Datei in `ws_ai`.

## Korrigierter Code (TypeScript, nicht kompiliert)

### `features/darknode/chart/adapter.ts`: Voranstellen korrekt anwenden

Behebt C2 und C3.

```ts
// Herkunft: ws_ai frontend/src/shared/components/CandleChart/chartAdapter.ts @1f27ed3
//   – Whitespace durchreichen, Dedupe je Zeit, inkrementelles update für neue Punkte.
// Korrekturen: C2 Voranstellen erkennt und wendet ältere Seiten an, Sichtfenster bleibt stehen;
//              C3 Fehler werden gezählt und gemeldet, nicht geschluckt.

export type LWPoint =
  | { time: number; open: number; high: number; low: number; close: number }
  | { time: number }; // Whitespace

export interface SeriesApi {
  setData(p: LWPoint[]): void;
  update(p: LWPoint): void;
}
export interface TimeScaleApi {
  getVisibleLogicalRange(): { from: number; to: number } | null;
  setVisibleLogicalRange(r: { from: number; to: number }): void;
}

export interface AdapterErrors { count: number; last?: string }

export function createChartAdapter(series: SeriesApi, ts: TimeScaleApi, onError: (e: AdapterErrors) => void) {
  let first: number | null = null;
  let last: number | null = null;
  const errors: AdapterErrors = { count: 0 };

  const guard = (fn: () => void) => {
    try { fn(); } catch (e) { errors.count += 1; errors.last = String(e); onError({ ...errors }); }
  };

  function apply(points: LWPoint[]) {
    if (points.length === 0) return;
    const newFirst = points[0].time;
    const newLast = points[points.length - 1].time;

    if (first === null || last === null || newLast < last) {
      guard(() => series.setData(points)); // Erstbefüllung oder Neustart
    } else if (newFirst < first) {
      // C2: ältere Seite vorangestellt ⇒ setData + Sichtfenster um die Anzahl neuer Punkte verschieben
      const prepended = points.findIndex((p) => p.time >= first!);
      const range = ts.getVisibleLogicalRange();
      guard(() => series.setData(points));
      if (range && prepended > 0) {
        guard(() => ts.setVisibleLogicalRange({ from: range.from + prepended, to: range.to + prepended }));
      }
    } else {
      // Live: nur Punkte ab dem bisherigen letzten (gleicher Zeitpunkt = update der offenen Kerze)
      for (const p of points) if (p.time >= last) guard(() => series.update(p));
    }
    first = Math.min(first ?? newFirst, newFirst);
    last = newLast;
  }

  return { apply, reset: () => { first = null; last = null; } };
}
```

### `features/darknode/chart/useDarknodeChart.ts`: Nachladen pro Chart

Behebt C1: kein globales Ereignis, sondern ein typisierter Request mit `chart_id` und `request_id`.

```ts
// Herkunft: ws_ai useCandleChart.ts @1f27ed3 – Linker-Rand-Schwelle (5 Bars) + Sperre (400 ms),
// Erstansicht letzte N echte Kerzen, Resize/Theme-Observer.
// Korrektur C1: kein window-Event; ein Request je Chart mit chart_id/request_id (P1-F15),
// höchstens eine laufende Seite je Chart/TF, Freigabe auch bei Fehler/Timeout (WS_AI_IDEEN Nr. 24).

export interface HistoryRequester {
  // schickt Protobuf-HistoryRequest über das bestehende Gateway; Promise endet bei Antwort/Fehler/Timeout
  requestOlder(chartId: string, requestId: string, endExclusiveMs: number): Promise<void>;
}

export function bindLeftEdgePaging(
  ts: { subscribeVisibleLogicalRangeChange(cb: (r: { from: number } | null) => void): void;
        unsubscribeVisibleLogicalRangeChange(cb: (r: { from: number } | null) => void): void },
  chartId: string,
  oldestMs: () => number | null,
  requester: HistoryRequester,
  threshold = 5,
  cooldownMs = 400,
) {
  let inFlight = false;
  let lastFire = 0;
  let seq = 0;

  const onRange = (r: { from: number } | null) => {
    if (!r || !Number.isFinite(r.from) || r.from >= threshold) return;
    const now = Date.now();
    const oldest = oldestMs();
    if (inFlight || oldest === null || now - lastFire < cooldownMs) return;
    inFlight = true;
    lastFire = now;
    seq += 1;
    requester
      .requestOlder(chartId, `${chartId}:${seq}`, oldest) // end_exclusive = ältester geladener Start
      .finally(() => { inFlight = false; });           // Fehler gibt die Sperre frei
  };

  ts.subscribeVisibleLogicalRangeChange(onRange);
  return () => ts.unsubscribeVisibleLogicalRangeChange(onRange);
}
```

### Laden der Bibliothek

Behebt C4: npm statt Global, kein ewiges Warten.

```ts
// Ersetzt ws_ai chartLazyLoader.ts: gepinnte npm-Version, Code-Splitting per import(),
// ein gemeinsames Promise – ein Fehler erreicht ALLE Aufrufer (C4).
let lib: Promise<typeof import("lightweight-charts")> | null = null;

export function loadCharts() {
  lib ??= import("lightweight-charts").catch((e) => { lib = null; throw e; });
  return lib;
}

export async function createCandleChart(el: HTMLElement, chartOpts: object, seriesOpts: object) {
  const { createChart, CandlestickSeries } = await loadCharts(); // v5-API; gegen gepinnte Version prüfen
  const chart = createChart(el, chartOpts);
  const series = chart.addSeries(CandlestickSeries, seriesOpts);
  return { chart, series };
}
```

## Neue Tests (vor Nutzung im TESTPROTOKOLL registrieren)

| ID | Prüft | Ergänzt |
|---|---|---|
| DN-P7-CH-001 | Vorangestellte Seite erscheint im Chart, Sichtfenster springt nicht (C2) | T18, K12 |
| DN-P7-CH-002 | Zwei Charts laden unabhängig; keiner reagiert auf den Rand des anderen (C1) | T16, P1-F15 |
| DN-P7-CH-003 | Antwort mit fremder oder veralteter `request_id` wird verworfen | T17 |
| DN-P7-CH-004 | Ladefehler der Bibliothek erreicht alle Aufrufer, nichts hängt (C4) | P7 |
| DN-P7-CH-005 | Adapter-Fehler werden gezählt und angezeigt (C3) | P7 |
| DN-P7-CH-006 | Live-Update derselben offenen Kerze per `update`, kein `setData` | K06 |

## Was jetzt noch zu lesen wäre

Damit das Register für `ws_ai` vollständig ist:
- `tradingViewScroll.ts`, `chartThemes.ts`, `LoadingBlockOverlay.tsx` und `types.ts` im Chart-Ordner. Danach steht fest, ob C2 in der Praxis woanders abgefangen wurde.
- `ws_rate_limiters.py` für den REST-Limiter.
- Die Börsen-Adapter unter `backend/exchanges/`. Wenn du mir die Dateipfade nennst oder den Verzeichnisinhalt einfügst, lese ich sie gezielt. Den Baum selbst kann ich bei GitHub nicht abrufen.

# TEIL VI – WS_AI OHLC-Read, REST-Limiter und Chart-Folgenachprüfung

> **Quelle 4, vollständig übernommen.** Die Funktionen `read_tf.rs`, `limiter.rs`, `scroll.ts` und `BackfillBand.tsx` sind **ungetestete Prototypen**. Die hier zu lesende Aussage „Pine-Parität für Golden-Vectors“ ist **ausdrücklich NICHT VERBINDLICH**: gemäß Gate 1 und V3-A bleiben ausschließlich eigenständig mathematisch definierte Rust-Indikatoren mit unabhängigen Golden-Vektoren; **kein Pine und keine Pine-Parität**. `ws_ai`-Spot-Adapter werden nicht als Bitget-Futures-Adapter qualifiziert.

Erledigt. Gelesen habe ich `tradingViewScroll.ts`, `LoadingBlockOverlay.tsx`, `ws_rate_limiters.py` und `unified_ohlc.py` (die Lese-Seite der Kerzen); dazu aus der letzten Runde die komplette Aggregationskette. Damit ist **C2 entschieden: Er wird nirgends abgefangen.** `tradingViewScroll.ts` regelt nur Rechtsrand und Auto-Scroll und wendet keine vorangestellten Daten an.

Nicht gelesen habe ich:
- `types.ts` und `chartThemes.ts` (Typen und Farben, die ohnehin durch Graphite-Tokens ersetzt werden)
- die Börsen-Adapter. Ich kann Verzeichnisse auf GitHub nicht auflisten. Wenn du die Ausgabe von `git ls-files backend/exchanges` hier einfügst, lese ich sie gezielt. Für V1 blockiert das nichts, weil diese Adapter Spot-Kanäle betreffen und Mehrbörsen erst in P10 kommen.

## 1. Neue Befunde

| Datei | Gut (belegt) | Fehler (belegt) | Art → Ziel |
|---|---|---|---|
| `CandleChart/tradingViewScroll.ts` | Auto-Scroll nur, wenn der Nutzer am rechten Rand steht (Toleranz 0,5 Bars); freier Raum rechts per `rightOffset` aus der Scroll-Position | **C5:** `unsubscribe = chart.timeScale().subscribeVisibleLogicalRangeChange(...)`. In Lightweight Charts gibt `subscribe…` nichts zurück, abgemeldet wird über `unsubscribe…(handler)`. `destroy()` meldet den Handler also nie ab, er läuft nach dem Abbau weiter. (Folgt aus der LWC-API; per Test DN-P7-CH-007 bestätigen.) | PORT+FIX → `features/darknode/chart/scroll.ts` |
| `CandleChart/LoadingBlockOverlay.tsx` | Backfill-Band über dem Zeitbereich `start_sec…end_sec` mit Phase, Fortschritt und Batch (WS_AI_IDEEN Nr. 28) | **C6:** `requestAnimationFrame`-Schleife ruft `setCoords` mit **jedem** Frame und immer neuem Objekt auf. Das sind etwa 60 React-Renders pro Sekunde, solange das Overlay aktiv ist. Farbe `rgba(255,0,0,0.12)` ist hart kodiert. | PORT+FIX → nur bei Bereichs- und Größenänderung neu berechnen, Graphite-Token |
| `backend/websocket/ws_rate_limiters.py` | Zwei saubere Verfahren: gleitendes Zeitfenster (Deque, z. B. 300 Connects / 5 min) und Token-Bucket mit Burst; Laufzeit-Umkonfiguration; Statistik | **R1:** `except Exception: return True`: Fehler im Limiter ⇒ Verbindung **erlaubt** (fail-open, Bann-Risiko). **R2:** `time.time()` statt monotoner Uhr. **R3:** Der REST-Alias `acquire()` nutzt **dasselbe** Connect-Limit (300/5 min) auch für REST-Abfragen; Bitgets REST-Limits sind aber je Endpunkt (z. B. 10/s pro IP). **R4:** Keine Reaktion auf 429/Retry-After. **R5:** Prüfen und Buchen sind getrennt, bei parallelen Coroutinen kann das Limit überschritten werden. | PORT+FIX → `dn-bitget/src/rest/limiter.rs` (§3.2) |
| `backend/services/usecases/unified_ohlc.py` | **Beliebige Auflösung beim Lesen** aus der 1s-Tabelle (`bucket = intDiv(ts, res)·res`); **exklusives Ende** `ts < end` für sauberes Paging; Limits (Default 500, max 5000); Schutz gegen Vollscans | **O1:** Gewinner je Sekunde per `argMax(*, ver)` ⇒ Live schlägt Backfill auch bei lückenhafter Live-Kerze (wie F7/F11). **O2:** Der jüngste Bucket wird ohne „offen/fertig“-Kennzeichen geliefert, obwohl er noch läuft (verletzt P1-F11). **O3:** `get_multiple_symbols` liefert bei Fehlern still `[]`, also nicht unterscheidbar von „keine Daten“. **O4:** Ausgabe als `float`. | PORT+FIX → `dn-agg/src/read_tf.rs` (§3.1) |

## 2. Kerzen bauen: das Beste aus allen Quellen

Für jede Regel steht unten, woher sie kommt und was DarkNode verbindlich macht. NautilusTrader liefert nur Konzepte, keinen Code (LGPL). Die Nautilus-Optionsnamen nenne ich aus dem Gedächtnis; sie sind in dieser Runde nicht gegen deren Quelltext geprüft.

| Regel | Quelle(n) | DarkNode-Festlegung |
|---|---|---|
| Basis ist 1s aus Rohtrades | `ws_ai` `candle_agg_1s`, TEIL1 | `trades_1s.rs` (V2 §16.5) |
| Fenster `[t, t+Δ)`, UTC, Börsenzeit | P1-F07, `ws_ai` `_bucket` | wie V2 |
| Reihenfolge bei gleicher ms nach `(ts, seq)` | P1-T26; korrigiert `ws_ai` F10 | wie V2 |
| Abschluss per Watermark + Karenz | `ws_ai` Stale-Flush 1,5 s (Wanduhr); Nautilus: zeitgesteuerter Abschluss | Watermark auf Börsenzeit, Karenz gemessen (R3) |
| Kein Flat-Bar ohne Trades | TEIL1, R4; `useChartView` „kein synthetisches OHLC“; Nautilus-Option „Bars ohne Updates bauen“ = **aus** | wie V2 |
| Erster Bar nach Start/Reconnect ist unvollständig | Nautilus-Option „ersten unvollständigen Bar überspringen“ | Statt Überspringen: Qualität `GAP`, weil die Lane-Lücke (V2 §16.4) das Intervall überdeckt. Nichts geht verloren, und der Backtest kann solche Bars sperren. |
| Zeitstempel des Bars = Startzeit | TradingView-Konvention; Nautilus kann wahlweise Endzeit | Startzeit (unabhängige mathematische Golden-Vektoren; keine Pine-Parität) |
| Höhere TFs aus 1s, nicht separat aus Trades | korrigiert `ws_ai` F9 | `mtf.rs` (V2 §16.6) |
| **Beliebige TF beim Lesen** | `ws_ai` `unified_ohlc` | `read_tf.rs` mit Fertig-Flag (§3.1). Konfigurierte TFs liegen zusätzlich vorberechnet im L2-Cache. |
| Exklusiver Cursor beim Paging | `ws_ai` `unified_ohlc`, WS_AI_IDEEN Nr. 11 | `end_exclusive`, auf TF-Grenze ausgerichtet |
| Feste Binär-Records mit Ganzzahlpreisen, `ts_event`/`ts_recv` | Databento DBN (Apache-2.0, Muster) | L2-Record-Layout |
| Kein Quellen-Rang „Live schlägt Archiv“ | korrigiert `ws_ai` O1/F11 | Dataset-Version + Qualität (P1-F06/F12) |
| Gegenprobe gegen Börsenkerzen | K11 | Bitget-1m vs. eigene 1m im Überlappungszeitraum |

## 3. Code (nicht kompiliert)

### 3.1 `dn-agg/src/read_tf.rs`: beliebige Timeframe beim Lesen

```rust
//! Beliebige Auflösung aus finalen 1s-Bars. Herkunft: ws_ai backend/services/usecases/unified_ohlc.py @1f27ed3
//! (Read-time-Aggregation, exklusives Ende, Seitenlimit). Korrekturen: O1 kein Versions-Rang (Qualität bleibt
//! erhalten), O2 jeder Bar trägt `complete`, O3 Fehler statt stiller Leerliste, O4 Ganzzahlen statt float.

use crate::mtf::fold_into;
use crate::trades_1s::{AggError, Bar};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TfBar {
    pub bar: Bar,
    /// P1-F11: false ⇒ Fenster noch nicht vollständig final.
    pub complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TfPage {
    pub bars: Vec<TfBar>, // aufsteigend
    /// Cursor für die nächste ältere Seite (= Start des ältesten gelieferten Buckets).
    pub next_end_exclusive_ms: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    InvalidSpan,
    InvalidLimit,
    Agg(AggError),
}

/// `one_s`: finale 1s-Bars, aufsteigend. `end_exclusive_ms` wird auf die TF-Grenze abgerundet:
/// es werden nur ganze Buckets VOR dem Cursor geliefert (der laufende Bucket kommt per Live-Push).
pub fn read_tf(
    one_s: &[Bar],
    span_ms: i64,
    end_exclusive_ms: i64,
    limit: usize,
    finalized_until_ms: i64,
) -> Result<TfPage, ReadError> {
    if span_ms < 1_000 || span_ms % 1_000 != 0 {
        return Err(ReadError::InvalidSpan);
    }
    if limit == 0 {
        return Err(ReadError::InvalidLimit);
    }
    let end = end_exclusive_ms.div_euclid(span_ms) * span_ms;
    let mut out: Vec<TfBar> = Vec::with_capacity(limit);
    let mut cur: Option<Bar> = None;

    for b in one_s.iter().rev().filter(|b| b.start_ms < end) {
        let bucket = b.start_ms.div_euclid(span_ms) * span_ms;
        match &mut cur {
            Some(c) if c.start_ms == bucket => fold_into(c, b).map_err(ReadError::Agg)?,
            _ => {
                if let Some(done) = cur.take() {
                    let complete = done.start_ms + span_ms <= finalized_until_ms;
                    out.push(TfBar { bar: done, complete });
                    if out.len() == limit {
                        break;
                    }
                }
                let mut nb = b.clone();
                nb.start_ms = bucket;
                nb.span_ms = span_ms;
                cur = Some(nb);
            }
        }
    }
    if out.len() < limit {
        if let Some(done) = cur.take() {
            let complete = done.start_ms + span_ms <= finalized_until_ms;
            out.push(TfBar { bar: done, complete });
        }
    }
    out.reverse();
    let next_end_exclusive_ms = if out.len() == limit { out.first().map(|b| b.bar.start_ms) } else { None };
    Ok(TfPage { bars: out, next_end_exclusive_ms })
}
```

Tests (zu registrieren):
- **DN-P1-AGG-009:** `read_tf(60 s)` aus 1s ist identisch mit `MtfAgg(60 s)` aus derselben Quelle.
- **DN-P1-AGG-010:** Seitenfolge über `next_end_exclusive_ms` ohne Doppel und ohne Loch.
- **DN-P1-AGG-011:** Ein laufender Bucket wird nie als `complete` geliefert.

### 3.2 `dn-bitget/src/rest/limiter.rs`: REST-Limiter je Endpunkt

```rust
//! Herkunft: ws_ai backend/websocket/ws_rate_limiters.py @1f27ed3 (Token-Bucket mit Burst).
//! Korrekturen: R1 fail-closed (unbekannter Endpunkt/Fehler ⇒ warten, nie freigeben);
//! R2 monotone Zeit (vom Aufrufer, Instant-basiert); R3 Limits je Registry-Endpunkt + gemeinsames
//! IP/UID-Budget (Muster „verknüpfte Limits“ aus Hummingbot RATE_LIMITS, Apache-2.0, nur Konzept);
//! R4 429/Retry-After ⇒ Sperre + Halbierung, Erfolg ⇒ langsame Erholung (AIMD, WS_AI_IDEEN Nr. 8);
//! R5 genau ein Besitzer (IO-Executor) ⇒ Prüfen und Buchen ohne Race.

use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
struct Bucket {
    capacity_milli: u64,
    tokens_milli: u64,
    rate_milli_per_s: u64,     // aktuelle Rate ×1000
    max_rate_milli_per_s: u64, // aus Registry
    min_rate_milli_per_s: u64,
    last_ms: i64,
    blocked_until_ms: i64,
}

impl Bucket {
    fn new(rate_per_s: u64, burst: u64, now_ms: i64) -> Self {
        let r = rate_per_s.saturating_mul(1_000);
        Self {
            capacity_milli: burst.max(1).saturating_mul(1_000),
            tokens_milli: burst.max(1).saturating_mul(1_000),
            rate_milli_per_s: r,
            max_rate_milli_per_s: r,
            min_rate_milli_per_s: (r / 8).max(100),
            last_ms: now_ms,
            blocked_until_ms: i64::MIN,
        }
    }

    fn refill(&mut self, now_ms: i64) {
        let dt = (now_ms - self.last_ms).max(0) as u64;
        self.tokens_milli = (self.tokens_milli + dt * self.rate_milli_per_s / 1_000).min(self.capacity_milli);
        self.last_ms = now_ms;
    }

    /// 0 = sofort möglich, sonst Wartezeit in ms.
    fn wait_ms(&mut self, now_ms: i64) -> u64 {
        if now_ms < self.blocked_until_ms {
            return (self.blocked_until_ms - now_ms) as u64;
        }
        self.refill(now_ms);
        if self.tokens_milli >= 1_000 {
            0
        } else {
            (1_000 - self.tokens_milli) * 1_000 / self.rate_milli_per_s.max(1)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Ip,
    Uid,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LimitDecision {
    Go,
    WaitMs(u64),
    /// R1: Endpunkt nicht in der Registry ⇒ nie senden.
    UnknownEndpoint,
}

pub struct RestLimiter {
    endpoints: HashMap<String, (Bucket, Scope)>,
    shared: HashMap<Scope, Bucket>,
}

impl RestLimiter {
    /// Werte ausschließlich aus der Endpoint-Registry (P1-R04), nicht aus Code-Konstanten.
    pub fn new(endpoints: &[(String, u64, u64, Scope)], shared: &[(Scope, u64, u64)], now_ms: i64) -> Self {
        Self {
            endpoints: endpoints.iter().map(|(k, r, b, s)| (k.clone(), (Bucket::new(*r, *b, now_ms), *s))).collect(),
            shared: shared.iter().map(|(s, r, b)| (*s, Bucket::new(*r, *b, now_ms))).collect(),
        }
    }

    /// Prüft Endpunkt- UND gemeinsames Budget; bucht nur, wenn beide frei sind (R5).
    pub fn acquire(&mut self, key: &str, now_ms: i64) -> LimitDecision {
        let Some((ep, scope)) = self.endpoints.get_mut(key) else {
            return LimitDecision::UnknownEndpoint;
        };
        let w1 = ep.wait_ms(now_ms);
        let w2 = self.shared.get_mut(scope).map_or(0, |b| b.wait_ms(now_ms));
        let w = w1.max(w2);
        if w > 0 {
            return LimitDecision::WaitMs(w);
        }
        ep.tokens_milli -= 1_000;
        if let Some(b) = self.shared.get_mut(scope) {
            b.tokens_milli -= 1_000;
        }
        LimitDecision::Go
    }

    /// R4: 429 ⇒ Sperre (Retry-After oder 1 s) und Rate halbieren.
    pub fn on_rate_limited(&mut self, key: &str, retry_after_ms: Option<u64>, now_ms: i64) {
        if let Some((ep, _)) = self.endpoints.get_mut(key) {
            ep.blocked_until_ms = now_ms + retry_after_ms.unwrap_or(1_000) as i64;
            ep.rate_milli_per_s = (ep.rate_milli_per_s / 2).max(ep.min_rate_milli_per_s);
        }
    }

    /// Erfolg ⇒ +5 % Rate bis zum Registry-Maximum.
    pub fn on_success(&mut self, key: &str) {
        if let Some((ep, _)) = self.endpoints.get_mut(key) {
            ep.rate_milli_per_s = (ep.rate_milli_per_s + ep.rate_milli_per_s / 20).min(ep.max_rate_milli_per_s);
        }
    }
}
```

Tests:
- **DN-P5-UP-029:** unbekannter Endpunkt wird nie gesendet.
- **DN-P5-UP-030:** gemeinsames UID-Budget greift über mehrere Endpunkte.
- **DN-P5-UP-031:** 429 sperrt und halbiert, Erfolg erholt.

Damit ist auch Bitgets 10/s pro IP für `fills-history` sauber abgebildet. Der Wert kommt aus der Registry.

### 3.3 Chart-Korrekturen (TypeScript)

```ts
// scroll.ts – Herkunft: ws_ai CandleChart/tradingViewScroll.ts @1f27ed3 (Auto-Scroll nur am rechten Rand,
// rightOffset aus scrollPosition). Korrektur C5: Handler wird mit unsubscribe…(handler) abgemeldet.
export function bindScroll(chart: any) {
  const ts = chart.timeScale();
  let auto = true;
  const handler = () => {
    const sp = ts.scrollPosition();
    chart.applyOptions({ timeScale: { rightOffset: Math.max(0, Math.min(5000, Math.round(-sp))) } });
    auto = sp >= -0.5;
  };
  ts.subscribeVisibleLogicalRangeChange(handler);
  handler();
  return {
    maybeAutoScroll: () => { if (auto) ts.scrollToRealTime(); },
    destroy: () => ts.unsubscribeVisibleLogicalRangeChange(handler),
  };
}
```

```tsx
// BackfillBand.tsx – Herkunft: ws_ai LoadingBlockOverlay.tsx @1f27ed3 (Band über start_sec…end_sec).
// Korrektur C6: Neuberechnung nur bei Bereichs-/Größenänderung, setState nur bei echter Änderung; Graphite-Token.
import { useEffect, useState } from "react";

export function BackfillBand({ chart, startSec, endSec }: { chart: any; startSec: number; endSec: number }) {
  const [x, setX] = useState<{ l: number; w: number } | null>(null);
  useEffect(() => {
    if (!chart) return;
    const ts = chart.timeScale();
    const calc = () => {
      const a = ts.timeToCoordinate(startSec);
      const b = ts.timeToCoordinate(endSec);
      const next = Number.isFinite(a) && Number.isFinite(b) ? { l: Math.min(a, b), w: Math.abs(b - a) } : null;
      setX((prev) => (prev?.l === next?.l && prev?.w === next?.w ? prev : next));
    };
    ts.subscribeVisibleLogicalRangeChange(calc);
    ts.subscribeSizeChange?.(calc);
    calc();
    return () => {
      ts.unsubscribeVisibleLogicalRangeChange(calc);
      ts.unsubscribeSizeChange?.(calc);
    };
  }, [chart, startSec, endSec]);
  if (!x) return null;
  return <div className="dn-backfill-band" style={{ left: x.l, width: x.w }} />; // Farbe aus Graphite-Token
}
```

Tests:
- **DN-P7-CH-007:** Nach `destroy()` wird kein Scroll-Handler mehr aufgerufen (C5).
- **DN-P7-CH-008:** Das Band rendert nicht pro Frame, nur bei Bereichs- oder Größenänderung (C6).

## 4. Stand des `ws_ai`-Registers

| Bereich | Status |
|---|---|
| WS-Verbindung, Lanes, Reconnect, Rate-Limiter | gelesen, portiert mit Korrekturen |
| Aggregation (1s, MTF, Rebuild, Lesen beliebiger TF) | gelesen, portiert mit Korrekturen |
| Chart-Stack (Komponente, Hooks, Adapter, Scroll, Backfill-Band, Loader) | gelesen, portiert mit Korrekturen C1–C6 |
| Backfill, Lücken, Historien-Protokoll | im früheren Audit gelesen, in WS_AI_IDEEN 7–11, 21–28 |
| Parser | ersetzt durch SBE-Codegen |
| Backtest | Platzhalter, nicht übernommen |
| `types.ts`, `chartThemes.ts` | nicht gelesen (durch Graphite ersetzt) |
| Börsen-Adapter | offen: Pfadliste nötig; relevant erst für P10 |

# TEIL VII – verbindliche Zusammenführung und zusätzliche Review-Sperren

**Geltung / Konfliktregel:** Der am Anfang stehende Architektur- und Reviewvertrag (A–F) hat Vorrang vor unverändert übernommenen Beispielblöcken aus V2/§16/Teil V/Teil VI. Die späteren ergänzenden Quellbefunde C1–C6, R1–R5, O1–O4 sind als neue **Code-Review-Anforderungen**, nicht als durchgelaufene DarkNode-Tests zu behandeln. Keine unbekannte Annahme darf zu `PASS` werden. Einbettung als Docs-Entwurf ist **kein** Implementierungsauftrag für alle Beispiele, insbesondere kein Zwang zu NPM-Lightweight-Charts-v5, zur zweiten WebSocket-Verbindung oder zur Spot-Mehrbörsen-Anbindung.

## VII.1 Übernahmeentscheid pro altem WS_AI-Baustein

| Quellkomponente | Übernahmeziel | Verbindlicher Umfang / Abgrenzung | Reviewstatus |
|---|---|---|---|
| `CandleChart/chartAdapter.ts`, `useCandleChart.ts` | Bestehender DarkBot-Chart-Adapter / ggf. `features/darknode/chart/{adapter,useDarknodeChart}.ts` | Chart-spezifischer History-Request, exklusiver Cursor, sichtbaren Bereich bei Prepend stabil halten, Live-Inkrementalpfad; **kein** globales Event | `PORT+FIX C1/C2/C3` – NOT_RUN |
| `chartLazyLoader.ts`, v4 Standalone in WS_AI | Vorhandenes Chart-Runtime-Inventar und Build | **Zuerst** eingebettete Dateiversion/Lizenz/NOTICE sowie vorhandene DarkBot-Chart-API prüfen. NPM-v5 nur als ausdrücklich gewählte *Migration*, nicht parallel installieren | `CONDITIONAL / C4` – OPEN |
| `useSafeCandleChart.ts` | existierende Chart-Fault-/Quality-Lane | Fehler sichtbar melden, Adapterzustand erst nach **erfolgreichem** SetData/Update fortschreiben | `PORT+FIX C3` – NOT_RUN |
| `tradingViewScroll.ts`, `LoadingBlockOverlay.tsx` | Charts im vorhandenen Graphite | Rechte-Rand-Scroll beibehalten, Listener korrekt deregistrieren, Backfill-Band event-getrieben ohne rAF-Renderloop | `PORT+FIX C5/C6` – NOT_RUN |
| `backend/websocket/ws_rate_limiters.py` | `dn-bitget/rest/limiter.rs` | monotone Zeit, per Endpoint/UID/IP-Autorität, atomarer Token-Verbrauch, 429/RetryAfter, **kein** fail-open, Emergency-Lane berücksichtigen | `PORT+FIX R1–R5` – NOT_RUN |
| `unified_ohlc.py` | `dn-agg/src/read_tf.rs` | echte finale 1s-Bars, beliebige zulässige TF, `[start,end)`, paginierter Cursor, Qualitäts-/Dataset-Provenienz, keine `float`-Produktwerte | `PORT+FIX O1–O4` – NOT_RUN |
| `ws_ai` 8× Spot-Adapter | keine Bitget-V1-Lane | nur später nicht priorisierte Referenz für Multi-Exchange-Feldvarianten; **keine** Behauptung erfolgreicher Futures-Verifikation | `DEFER / P10` |

## VII.2 Neue konkrete Codeblocker gegenüber dem Vorlagenstand

| ID | Codeort | Verbindliche Korrektur vor tatsächlicher Integration | Abnahme / Status |
|---|---|---|---|
| V3.1-B25 | Chart-Adapter `createChartAdapter::guard/apply` (Teil V) | Bei `setData/update`-Fehler keine internen First/Last-Cursor fortschreiben; Payload vor Anwendung nach Zeit sortieren/deduplizieren, Backend-Qualität bewahren, Fehler sichtbar machen. Nicht nur Catch+Counter. | DN-P7-CH-001/005, NOT_RUN |
| V3.1-B26 | Chart-History `bindLeftEdgePaging` (Teil V) | Eindeutige `chart_id`/`request_id` über Remount/Restart, Cancel/Unmount und Stale-Replies; `catch`/`finally` ohne unhandled Rejection, Paging nicht nur via 400-ms-Cooldown. | DN-P7-CH-002/003, NOT_RUN |
| V3.1-B27 | Chart-Loader v4→v5 (Teil V) | Keine doppelte Library. Tatsächlich vorhandene eingebettete WS_AI-Version, gegenwärtigen DarkBot-Renderer und API zuerst verifizieren; bei ausgewählter v5-Migration korrektes `addSeries` und NOTICE im bestehenden Frontend. | DN-P7-CH-004/006, NOT_RUN |
| V3.1-B28 | Scroll-/Band-Code Teil VI | Re-Entrancy und Feedbackloop durch `chart.applyOptions` im Scroll-Handler, Observer-Aufräumung, Chart-Instance-Wechsel, LWC-Zeittypen, Null/NaN-Koordinaten und `ResizeObserver` konkret verifizieren. Kein 60-fps-React-Update. | DN-P7-CH-007/008, NOT_RUN |
| V3.1-B29 | `dn-bitget/src/rest/limiter.rs::acquire` (Teil VI) | **CRITICAL:** `self.shared.get_mut(scope).map_or(0, ...)` gewährt bei fehlendem gemeinsamen IP-/UID-Bucket trotzdem Freigabe! Erforderlicher Scope fehlt ⇒ `UnknownEndpoint`/fail-closed. Kein Send ohne vollständige Budget-Reservation. | DN-P5-UP-029/030, NOT_RUN |
| V3.1-B30 | `dn-bitget/src/rest/limiter.rs` (Teil VI) | Overflow-safe monotone Refill/Zeitsprünge und Rate=0, Shared-Budget nach 429, Endpoint-spezifisches Rate-Limit; Exit/Stop-Sicherheitsaktionen nicht durch Backfill-Starvation blockieren; testbare Prioritäts-/Budget-Fairness. | DN-P5-UP-030/031, NOT_RUN |
| V3.1-B31 | `dn-agg/src/read_tf.rs` (Teil VI) | Nur qualifizierte, vollständige 1s-Source und gültige Aggregator-/Instrumentversion; TF-Komposition darf fehlende Sekunden/GAP nicht als `COMPLETE` darstellen; finalisierten TFs erst bei bestätigtem Watermark und Coverage. | DN-P1-AGG-009/011, NOT_RUN |
| V3.1-B32 | `read_tf` Paging (Teil VI) | **Kein** Endcursor nur wegen `out.len()==limit` ohne belegte weitere Daten; auch über mehrseitige Grenzen stabile, monotone `end_exclusive_ms`-Abfolge, Dedupe-/Rebuild-Provenienz. | DN-P1-AGG-010, NOT_RUN |
| V3.1-B33 | Bitget WS_AI Quellen/`Spot` Börsen (Teil V) | 8 verdrahtete Spot-Kanäle sind kein Beweis erfolgreicher Futures-Integrationen; Bitget UTA V3 und Public SBE V5 nur an tatsächlich bestätigten Endpoints/Schema umsetzen. | P1-T01–T27 und P5-A01–A28, NOT_RUN |
| V3.1-B34 | Inhalte Teil VI, Tabellenzeile „Pine-Parität“ | **ENTFERNT**. Bar-Startzeit ist ein Datenformat-/UTC-Vertrag, kein Indikator- oder Script-Paritätsauftrag. Rust-Indikatoren unabhängig mathematisch testen. | P1-T26/T27, P3-Math, NOT_RUN |

## VII.3 Neue Test-IDs – beim Einbau ins zentrale TESTPROTOKOLL verbindlich

| Testblock | Einzel-IDs | Sollnachweis | Status |
|---|---|---|---|
| Chart und History | `DN-P7-CH-001–008` | Prepend/Paging, mehrere Charts, Stale-Request, Loader-Fehler, sichtbare Adapter-Fehler, Live-Update, Listener-Cleanup, Backfill-Overlay | **NOT_RUN** |
| On-demand-TF/Aggregation | `DN-P1-AGG-009–011` | TF aus 1s gleich MTF, exklusives Paging ohne Loch/Doppel, keine offene Kerze als final | **NOT_RUN** |
| REST-Limiter | `DN-P5-UP-029–031` | Unbekannter Endpoint gesperrt, Shared-IP/UID-Kapazität, 429 mit Recovery | **NOT_RUN** |

**Bezug zu bereits im V3 enthaltenen Prüffällen:** Bestehende P1-T01–T27, P5-A01–A28, C00–C16, DN-PROTO-001–008, DN-P1-WS-001–006, DN-P1-AGG-001–008 und DN-P5-UP-001–028 behalten ihre Original-ID. IDs niemals wiederverwenden oder für gleiche Case eine zweite Nummer erfinden. Zum Testprotokoll nur `NOT_RUN`-SOLL-Fälle hinzufügen, nicht PASS. **Alle bisherigen V3-B01–B24 bleiben ausdrücklich offen.**

## VII.4 Abnahmestand dieses Git-Entwurfs

- **Fertig:** diese Dokumentationskonsolidierung mit Quell-SHA/Provenienz und zusätzlichen 14 Solltests (8 Chart, 3 Aggregation, 3 REST-Limiter); keine Upstream-Datei ungeprüft als neue Runtime übernommen.
- **Nicht fertig:** Code-Review-Befunde V3-B01–B24 und V3.1-B25–B34 schließen; Alt-Chart-Bestand des aktuellen DarkBot-Frontends zeilengenau auditieren; Bitget-SBE-V5-Decoder/UTA-V3-Feld- und Stoppbelege; tatsächlicher `cargo build`, `cargo clippy`, `cargo test`, Frontend-Goldens und gesamte P5-Safety-Abnahme.
- **Sicherheitsstatus:** `GATE 1 OPEN`, `P1/P5 NOT_RUN`, `DEMO-WRITE NOT AUTHORIZED`, `LIVE GESPERRT`. Dieser Commit dokumentiert **ausschließlich das erste Entwurfsmuster**. Ein nachfolgender Implementierungsauftrag muss jede technische Änderung separat prüfen und testen.

