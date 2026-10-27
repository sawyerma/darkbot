# DarkNode – P0 Research-, Interface- und Entscheidungsvertrag V1

Stand: 27.10.2026. Status: verbindlicher P0-*Sollvertrag*, Quellen-Spikes und Repo-Implementierungsprüfungen noch offen. Übergeordnet: GATE_1.md und PHASENPLAN.md.

## 1. Einziges System und Baseline

- Autoritätsbaum: öffentliche Bitget-Frames → Rust typed L1/L2 → ein Rust-Indikator-/Graph-Kern → SignalIntent → ein Account-Risk-/Order-Owner → Bitget UTA V3; Backtest/Replays verwenden den gleichen Graph-/Signal-Kern. Rust publiziert ausschließlich geeignete Protobuf-Projektionen zum bestehenden Gateway → bestehende React-Graphite-SPA.
- Versionsregister SOURCE_BASELINE muss für darkbot, ws_ai, jedes upstream Open-Source-Projekt *Commit, Dateipfad, SHA256 falls vorhanden, Lizenz, Zweck, Status KEEP/REFERENCE/PORT/REJECT* tragen. Bisher gepinnte Upstreams stehen in P5_OPEN_SOURCE_CODE_AUDIT_AND_TESTPLAN_V1.md; Quellenprüfung ≠ alle Tests bestanden.
- Project_0, BSC, Solana bleiben fachlich getrennt. Keine Frontend-/Gateway-Duplikate, keine gemeinsamen Trading-Regeln oder kopierten 1-ms-Ziele. API-Key niemals Teil eines Control-Themas.

## 2. Namespace-/Topic-Design für die spätere Governance-Abnahme

**Planungsvorschlag, noch kein geschriebenes Schema oder reserviertes Runtime-Topic:** Paket darkbot.darknode.v1, ProjectId=4, Domain-Topicraum 70–89. Vorgesehene konkret eindeutige Einträge:

| TopicId als P0-Vorschlag | Topic-Name | Klasse | Einzige Autorität / Frontend |
|---:|---|---|---|
| 70 | darknode.status | STATE | Rust owner → Overview/Live Readiness |
| 71 | darknode.market.state | STATE | Rust Market owner → Feed/Time-/Gap-Qualität |
| 72 | darknode.strategy.state | STATE | Rust Strategie-Owner → Graphversion, Validität und Signalstatus |
| 73 | darknode.backtest.state | STATE | Rust Backtest-Owner → qualifizierte Job-/Result-Referenzen |
| 74 | darknode.wallet.state | STATE | Rust Account owner → Account/Equity/Balance/Quality |
| 75 | darknode.positions.state | STATE | **derselbe** Rust Account owner → Position/Margin/Stopdeckung |
| 76 | darknode.orders.state | STATE | **derselbe** Rust Account owner → offene/UNKNOWN Order-/Stop-Mengen |
| 77 | darknode.control.state | STATE | Rust Control owner → effective policy, Version, Receipts |
| 80 | darknode.execution.events | EVENT | persistentes Rust Execution-Journal → Fill/Order/Emergency-Ereignisse |
| 81 | darknode.risk.events | EVENT | persistentes Risk-Journal → Gate-Entscheide/Alarme |

Nicht alle Topics im MVP zwingend aktiv; bei fehlender echter Projektion Descriptor available=false. Große Bars/Tickhistorien **nicht** als STATE pushen, sondern History-/Paging-Vertrag bestehendem Gateway entsprechend entwerfen. Die Zuordnung wird in P0 zusammen mit Payload-/ACL-/EVENT-Cursor/STATE-Generation-/Max-Size-Regeln final geprüft; nur **nach** Governance durch Codegen implementieren. Der gemeinsame Gateway kann keine zweite Fach-/Trading-Autorität werden.

## 3. Minimale Datenschnittstellen

**Market:** InstrumentVersion, TradeEvent mit originaler unveränderter Trade-ID, Herkunft, Exchange-/Receive-Zeiten und Tick-/Lot-Skala; BookDelta seq/pseq/DepthAction; Bar mit partial/final, UTC-Fenster, OHLCV, trade_count, Herkunft, Gap, Hash. P1_BAR_SCHEMA_AND_TRADE_ORDER_V1.md ist alleinige Wahrheit für Bar-/Trade-Reihenfolge. Fehlende/ungeordnete Trades = INVALID, niemals erfundene Bar.

**Graph:** Immutable StrategyVersion/GraphHash, NodeTypeVersion, typisierte Ports/Units/Timeframe/Validity, Bool3, SignalIntent long/short/exit. Graph kennt keine API-Keys, signierten Requests oder Order-Client.

**Execution:** OrderIntent mit originaler strategy_version, bot_id, account_uid, symbol, position_mode, margin_mode, side/reduceOnly, quantity/price/stop policy, risk_snapshot_id, clientOid, PayloadHash, dedup/journal sequence; OrderEvent und StopCoverage. Vor externem Send journalisieren und fsync. Bitget ist Positionsautorität.

**UI:** separate Message-Typen für Wallet, Positionen, Orders, Strategie, Backtest, Kontrollstatus; Presence, Quality, Money/Qty/Currency und UID/TradeId exakt. Keine JSON-in-bytes oder beliebigen generischen Stringfelder als sicherheitskritischer Vertrag. DN_PROTO_CONTRACT_V1.md definiert den automatischen Census und acht Gates.

## 4. Research-Entscheidungsregister / evidence-needed

| ID | Sollentscheidung / Nachweisziel | Status |
|---|---|---|
| R1 / DN-D08/09 | UTA V3 One-way + Isolated für V1 festgelegt; Bitget Account Mode, Stop-/TP-, Partial-/Reduce-only-Verhalten nachlesen und Demo validieren | PRODUKT FEST / DEMO NOT_RUN |
| R2 | Bitget Public SBE Depth50 V5, raw trade IDs/Order, Rate-Limits, Historie, 7-Tage-Soak | P1 R2 PARTIAL / CODE NOT_RUN |
| R3 | Private WS/REST, Account-/Order-/Stop-Reconcile, Lost ACK, Reboot/Fills | P5 NOT_RUN |
| R4 | Eigene mathematische Definitionen für Williams/Andean/BX, 5s-/MTF-/Barclose-Regeln und unabhängige Golden-Rechnungen | FORMEL-/GOLDEN-TEST OFFEN |
| R5 | NodeType-/Unit-/Validity-/Graph-IR und alle drei Beispielstrategien Golden und negativ | PLAN SPEZIFIZIERT / TEST OFFEN |
| R6 | Bitget UTA-V3 Adapter eigene Rust-Domäne, vor Bibliothekseinsatz versionierte Lizenz-/Kompatibilitätspfade; Kosten/latency p99 gegen Alternativen | CODE-SPIKE OFFEN |
| R7 | Candle-/Trade-/L2-Fillannahmen; konservatives Intrabar-Modell, Funding/Fees/Maker-/Adverse Selection | PLAN SPEZIFIZIERT / KALIBRIERUNG OFFEN |
| R8 / DN-D14 | genaue Topic-/Payload-/Command-/Owner-/ACL-Bindungen, Generator, Publisher-/UI-Goldens | PLAN SPEZIFIZIERT / GOVERNANCE + CODE OFFEN |

Exakte Account-Mode- und Leverage-Verifikation: GET Konto-/Settings-/Positionen/Assets, validiertes Symbol/Instrument, Pre-Set Leverage mit tatsächlicher Account-Bindung, nur per separater Demo-Freigabe Set-Leverage und erneuter Readback. **Kein** Moduswechsel bei unbekannten Orders/Positionen und kein Default-Cross. Volle Symbol-/Margin-/RateLimit-Fähigkeiten nie allein aus BTCUSDT Public-Doku ableiten.

## 5. P0-Artefakt-DoD und Review

SOURCE_BASELINE, WS_AI_PORT_INVENTORY, ADRS, PROTO-/Capability-Metadaten und fachliche Owner-Karte sind am gleichen SHA dokumentiert. Für JEDE offene Frage gibt es einen genauen Experiment-Input/Endpoint oder Source-Datei, expected acceptance, Owner und Belegpfad. Fehlt realer Input, BLOCKED/NOT_RUN mit Grund. Kein Gate-1-PASS ohne die expliziten Kriterien aus GATE_1.md. Kein Sign, Demo-Mutation oder Live durch diese Research-Arbeiten.