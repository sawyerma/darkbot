# DarkNode – P6/P7/P8: E2E-Demo, Graphite-Vertrag, Negativ- und Dauerabnahme V1

Stand: 27.10.2026. Verbindliches Soll, alle produktiven Tests NOT_RUN. P1- und P5-Detailgates haben Vorrang. Ein früher M1-Durchstich ersetzt weder P6 noch den vollständigen Node-Editor.

## P6: Ein Bot, vollständiger Zyklus

**Konfiguration:** exakt ein qualifiziertes BTCUSDT- (oder versioniert anderes festes) Symbol, ein unveränderlicher qualifizierter Graph, One-way+Isolated, ein Account-/Risk-Owner, höchstens eine aktive Richtung/Position je Bot und Exit/Rearm nur nach bestätigtem Exchange-FLAT. Papiermodus besitzt **keine** Trade-Rechte; DEMO erhält getrennte Demo-Read-/Write-Freigabe und niemals Zugriff auf LIVE-Credentials.

**Proof-Lanes:** (A) P1 echte öffentliche Daten → Rust Bars/Indicators/Signal → NO_SEND-Risk und UI, (B) Paper mit reproduzierbaren historischen/Live-Daten und klar SIMULATED-Güte, (C) read-only privater UTA-Demo-Account-Sync, (D) ausdrücklich autorisierter Demo-Entry mit Preset-SL/Partial Fill → qualifizierte Stop-Readback → Signal-Exit → Restorder-/Stopcheck → confirmed FLAT → Rearm → nächste Flanke.

**P6-Assertions:** repeated TRUE-Condition erzeugt nicht mehrere Entries; LONG↔SHORT-Gegensignal erzeugt keinen Flip; INVALID-Fenster sperrt neue Entries; keine LIVE-Order bei Demo/Paper; nach Reconnect keine zweite clientOid; Funding-/Fee-/PnL-/Wallet-Größen passen zum Exchange-Readback; UI geschlossen = autonomer Rust-Service; kein vorzeitiges READY beim ersten WS-Login. Anzahl Zyklen, Zeitdauer und Anforderungen **vor** Testlauf festlegen. Kein einzelner Mock-Fill als E2E-PASS.

## P7: Fünf Tabs, keine neue Navigations- oder Handelsautorität

| Tab | Sollinhalte / Quelle | Quality-/Negativfall |
|---|---|---|
| Overview | Bot/Feed/Graph/Gate/Positionskurzstatus; echte Publisher-IDs und Gründe | Quelle fehlt ⇒ UNAVAILABLE, nicht PASS |
| Builder | React Flow: linke Palette exakt Marktdaten, Indikatoren, Bedingungen, Aktionen, Risiko; Inline-Parameter/Ports; drei Golden-Beispiele; Save Draft→Validate→Backtest→Qualify→Activate | kein permanenter Inspector, keine Browser-Strategieausführung, ungültiger Graph nie aktiv |
| Backtest | Versionierte Source-/Graph-/Fee-/Fillmodelle, OOS/Walk-forward, Equity-/PnL-Diagramm, Kosten-/Coverage-/Uncertainty-Hinweise | historische Simulation nie als Bitget-Live-Fill ausgeben |
| Live | nur durch Rust bestätigte Bot-/Account-/Positions-/Stop-/Order-/Riskwerte, Source-Freshness, Command-/Receipt-Phase und getrennte HALT/EMERGENCY-Aktion | Buttons rufen **nur** typisierte/ACL-geprüfte Gateway-Commands auf; Browser kann Risk nicht umgehen |
| Wallet | tatsächliche Bitget-USDT-Futures-Equity/Balance/Available/Margin, alle Positionen, eigene/fremde Orders, Stopdeckung, Fees/Funding/PnL, Bot-Risk-Reservation, Currency und As-of | WS stale ⇒ letztes qualifiziertes Bild als STALE, kein 0/FLAT, Demo/Live strikt markiert |

**Wire-Vertrag:** DarkNode-Fachprotos über bestehendes Gateway. Kein separater WS, neue JSON-in-bytes, ungenutzte Scheinstatuswerte. Schema- und Field-Census-Gates DN-PROTO-001–008; bei jeder relevanten Backend-Änderung Generator, Gateway-Deskriptor, TS-Adapter, Store und UI-Nachweis gemeinsam aktuell. Volle Historie/Charts über qualifizierte Paging-/History-API und bestehende Chartbibliothek, keine STATE-Broadcasts mit Multi-MB-Rohtrades.

**Benutzerführung & sichere Controls:** Verwechslung von Paper, Demo und Live verhindern; deutlich sichtbarer Account-/Mode-/Bot-/Graph-Hash; kleine Screens/Keyboard/Fokus/Screenreader; kein Trade-Button direkt Bitget; Confirmation bei Not-Aus und getrennte fachliche Wirkung (HALT ≠ Flat/Close). Kein sechster Tab; Monitoring/Alerts in vorhandene Reiter integrieren. Keine unverifizierten Finanzsummen als globale Equity (insbesondere mehrere Konten/USDT-vs-USD).

**UI-Prüfmatrix:** Routing und eigene Projektkarte, Tab-wechsel/Subscriptions/Disconnection, on/offline/stale, 2^53-IDs/Decimal/negative PnL, falsche ProjektId/Topic/Payload, verlorene Command-ACKs, Station-Snapshot vs Incremental, partial fills/stop gap, DB-/Gemeldete Version vs UI-Graph, fehlender Publisher und Array-/Paginggrenzen; DOM-/Visual Regression gegen festgelegte Baselines, inklusive reinem PROTO_FIELD_CENSUS-Consumer-Coverage-Nachweis. Placeholder darf nicht als CONNECTED zählen.

## P8: Qualifizierter Fehler-/Last-/24x7-Abnahmeplan

| Lane | Pflichtversuche | Abnahmekriterium |
|---|---|---|
| R / Replay | deterministische Markt-/Graph-Events, WS-Gaps/Sequenz/Fork, Mark/Last, N1–N3, P1 Bar-T25–27 | bitgleiche Trace/Gate-Urteile, NO_ENTRY bei Datenlücken |
| S / Safety | P5-A01–A28, C00–C16 aus P5_ORDER_STATE_UND_CRASH_MATRIX_V1.md, Fee-/Size-/Stop-/ADL-/Emergency-Races | keine unkontrollierten Orderduplikate, Schutzmengen geprüft |
| W / Wallet/Wire | WebSocket initial+REST Merge, Gap/Stale/Foreign Orders, Gateway/Rust→UI, Fuzz/Goldens | kein Phantom-FLAT, keine Cross-Project-/Type-Mismatches |
| L / Latency | P5 L1–L9, Queue Tail-/fsync, App-/Chart-Backpressure, CPU/Heap, REST-vs-WS | p50/p95/p99/p99.9 inkl. Max/Ausfälle, messungsbasierte vorab festgelegte Grenzwerte |
| O / Operations | 24h-/mehrtägiger öffentlicher Feed und P1 7 Tage, unabhängiger Watchdog/Alarm, Key-Revoke, Restore/Upgrade mit offener Position | keine stillen Ausfälle; komplett replayfähige Recovery-Nachweise |
| E / Economics | vorregistrierte OOS/WFA, Fees/Spread/Slippage/Adverse Selection/Non-Fill/Funding, maker/taker | dokumentierte positive Netto-Evidenz mit Unsicherheit oder **ökonomisches Gate nicht bestanden** |

**Soak-/Benches:** Host-SHA, Infrastruktur, Uhrqualität, Release-Flags, WS-Verbindungen, Samplezahl, Warm/Cold, Rate-Limits, Börsen- und Netzwerk-Fehler, Genuine-vs-Fixture-Daten, p99.9, absolute Fehlerzahl, Grafana/Logs/Evidencehash dokumentieren. Numerische SLOs nach R6/Hostspike **vor** jeweiligen Acceptance-Runs einfrieren; nicht rückwirkend passend wählen. Ein Pass ohne genügende Samples und Fehlerfenster ist kein PASS.

## Abschluss P6/P7/P8

Eine einzige M1→P6→P7→P8-evidence chain am finalen SHA, reproduzierbare Runner und für jede Gate-ID PASSED/BLOCKED/NOT_RUN/FAILED mit Artefakt. P8 SAFE=PASS und OOS=PASS getrennt dokumentieren. Kein automatisches SIGN/ARM/LIVE, P9 bleibt separater Autorisierungsschritt.