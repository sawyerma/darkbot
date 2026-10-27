# DarkNode – WALLET_ACCOUNT_SYNC_V1: Bitget-Konto, Positionen und Orders in Echtzeit

**Stand: 27.10.2026 · Revision V1.0 · Verbindlicher P5/P7-Vertrag, nicht implementiert, alle Tests NOT_RUN.** Im bestehenden DarkNode-UX exakt der Tab **Wallet**; keine neue App, Sidebar, Broker-Engine, TradingView-Kontoverbindung oder weitere WS-Verbindung zum Browser. `SIGN/ARM/LIVE` bleiben bis zur separaten Gate-Freigabe verboten.

## 1. Fachliche Bedeutung

Der „Wallet“-Tab ist **eine Live-Konto-/Portfolioansicht des bei Bitget verwahrten Bot-Unterkontos**, **keine** eigene Onchain-Wallet, MetaMask, eigener Custody-Ledger oder Broker-Ersatz. Beträge sind erst dann wahr, wenn die Bitget-Account-/Position-/Order-Autorität sie bestätigt hat. Der Bot hat Read/Trade nur mit freigegebenen API-Rechten, niemals Withdrawal, Transfer oder automatisches Funding vom Hauptkonto. **API-Verbindungsstatus ist nicht gleich Positions-/Bilanzqualität**.

V1: ein qualifiziertes API-fähiges Bitget-Unterkonto, USDT-M-Futures, ein Bot/ein festes Symbol; spätere Bot-Budgets sind **lokal reservierte Risiko-Budgets**, keine voneinander getrennten Bitget-Wallets. UTA V3 und tatsächliche One-way-/Isolated-Capabilities bleiben `DN-D08=OPEN` bis privatem Demo-Nachweis. Subaccount-UID, Region, Mode (DEMO/LIVE), Symbol, Valutierung, Snapshotzeit und Datenqualität stets sichtbar. Ein verbundener DEMO-Account darf nie als Echtgeld-Kontostand etikettiert werden.

## 2. Datenquellen, Initialisierung und kontinuierliche Aktualisierung

**EIN Rust Account-/Portfolio-/Position-/Risk-/Order-Owner pro Bitget-Konto.** Dessen bereits bestehender authentisierter UTA-V3 Private-WS-Adapter verbindet sich mit
`wss://ws.bitget.com/v3/ws/private` (Demo nach Bitget-Profil: `wss://wspap.bitget.com/v3/ws/private`) und abonniert `instType=UTA`, `topic=account, position, order, fill, strategy-order`; optional `fast-fill` nur nach getrennter Capability-/Fidelity-Abnahme (deckt laut Bitget Strategie-/Copy-Trading-Fills nicht ab). Kein separater UI-/Wallet-Exchange-Client: **das GUI hört nur auf Rust→bestehendes Protobuf-Gateway**.

| Quelle | Abnahme-/Verwendungsregel |
|---|---|
| WS `account` | Erstsubscription mit Full Snapshot, später Updates bei Order-Fill, Settlement, Transfers, Bewertung; `totalEquity`, `effEquity`, `imr`, `mmr`, `unrealisedPnL`, Coin-`balance/available/equity/usdValue`. `totalEquity` wird von Bitget in **USD** definiert, USDT-Saldo in **USDT** – nicht ohne Umrechnung gleichsetzen. |
| WS `position` | Snapshot/Updates über symbol/size/available/frozen/avgPrice/leverage/marginMode/markPrice/unrealisedPnl/realised/funding, soweit kanal- und accountseitig vorhanden; maßgebliche Exchange-Positionsgröße, nicht Fillsumme oder Bot-Absicht. |
| WS `order` | **Keine vollständige Initial-Orderliste auf Subscribe!** Nur Status-Events; Ausgangszustand zwingend per REST offene Orders. ACK≠Fill/Cancel-complete. |
| WS `fill` | Private eigene Ausführungen via `execId`, `orderId`, Menge/Preis/Gebühr/Side. Idempotentes Journal + Position-Abgleich, kein Trade-ID-/Zeitstempel-Alleinbeweis für vollständige Position. |
| WS `strategy-order` | Stop-/TP-Schutzstatus beobachten, aber vorhandene Stop-/Strategy-Orders bei Boot/Reconnect zusätzlich per REST listen; echte Stopdeckung nach Fill überprüfen. |
| REST `GET /api/v3/account/info,settings,assets` | Account-Identität, Modus, UID, Berechtigungen, Equity und Assets als Snapshot/Fehlerabgleich. |
| REST `GET /api/v3/position/current-position` | Full Exchange-Position/Margin/Liquidations-/Leverage-Readback. |
| REST `GET /api/v3/trade/unfilled-orders,unfilled-strategy-orders` | **Alle** relevanten offenen Orders/Schutzorders als Boot-/Reconnect-/Periodik-Abgleich; `order-info/history-orders/history-strategy-orders/fills` zur klärenden Reconciliation. |
| Public Tick/Markprice aus P1 | Für gesondert markierte indikative Preismarkierung/Chart-Update; **keine** Umdeutung zum aktuell von Bitget bestätigten Account-/Equity-Snapshot. |

**Konsistenter Startup-Reconcile (kein naives Subscribe→REST-Overwrite):**
1. Konto-/Key-/Accountmode prüfen. Private WS anmelden und relevante Topics erfolgreich abonnieren; ab diesem Zeitpunkt eingehende Events mit Connection-ID/Receive-Zeit in begrenztem Buffer annehmen.
2. WS-Account-/Positions-Snapshots und **REST-Listen** für offene Orders, Schutzorders, Position und Wallet einlesen. Da zwischen REST- und WS-Nachrichten keine global garantierte Transaktionssequenz für alle Topics besteht, dürfen ältere REST-Antworten neuere WS-Ereignisse nicht blind überschreiben. Deduplizieren nach typisierten IDs/Versionen, Original-Eventtypen und Zeitpunkten; unklare Reihenfolge ⇒ `RECONCILING/UNKNOWN`, weitere REST-Neuabfrage nach eventgenauer Abstimmung. Ohne belegten konsistenten Schnitt `NOT_READY`.
3. Alle Instrumente/Positionsseiten des Unterkontos prüfen, auch außerhalb des Bot-Symbols (Fremdpositionen, Fremdorders, fremde Stop-/Margin-Reservierungen). Bot-Positions-/Budget-Zuordnung und UID-Exposure streng separat führen; Fremdänderungen nach S7 behandeln.
4. Erst danach `SYNCED` und – nur bei **sämtlichen** übrigen S0–S7-Gates – `ACTIVE` zulassen. Reconnect, WS-Error, Sequence-/Dedupe-Konflikt, langes Event-Schweigen, Snapshot-Staleness oder differierende REST-Ergebnisse sofort `STALE/RECONCILING`; Einträge und Risikolimits aus Exchange-Daten erneut prüfen, keine LOCAL=FLAT-Annahme.

**Regelbetrieb:** private WS **ereignisgetrieben** statt pro Sekunde denselben REST-Satz zu pollen; ergänzend begrenzte REST-Snapshot-Periodik und eventgetriggerte Readbacks nach Fill, Stop-Modify, unbekanntem Orderstatus, Alarm, Accountänderung, Reconnect und Neustart. Intervalle, Freshness-/Disconnect-Schwellen und API-Rate-/Backoff-Budget erst nach Messung festlegen und in versionierter Konfiguration dokumentieren. Ping/Pong oder erfolgreiche WS-Verbindung **allein** beweist keine korrekte Kontodeckung. GUI muss keinen eigenen Poll gegen Bitget auslösen.

## 3. Gemeinsames typisiertes Datenmodell für Risk und Wallet

`AccountSnapshot` (Account-ID/Mode, exchange_ts, received_mono_ts, reconciled_at, source, quality, currency) und `CoinBalance` (coin, wallet_balance, available, equity, usd_value, debts/bonus soweit anwendbar), `ExchangePosition` (symbol, mode, pos_side, size, available, frozen, avg_entry, leverage, margin, mark, est_liq, realised/unrealised_pnl, funding), `ExchangeOrder` und `StopCoverage` (orderId, clientOid, qty, filled/remaining, side, status, trigger, exchange-confirmation), `BotRiskReservation` (max notional, committed pending/exposure, risk budget, used allocation), `AccountSyncState` (SYNCED, RECONCILING, STALE, DISCONNECTED, BLOCKED, last_good_snapshot, age/reason).

- Bitgets **totalEquity USD**, **USDT available**, **effEquity**, **initial/maintenance margin** und lokale **Bot-Reservation** sind unterschiedliche Größen. `available` ist nicht dasselbe wie „zusätzlich erlaubter Trade-Nominalwert“. Risk-Owner berechnet aus Exchange-Status, Instrumentdaten, Kontobudget, bestehenden/UNKNOWN Orders, Stop-/Gap-/Fee-/Funding-Reserve und S1 Limits `max_additional_entry_qty` bzw. `NO_ENTRY` – ohne dieselbe Börsenmargin doppelt abzuziehen.
- Anzeigen von kontogesamten und einem einzelnen Bot zugeordneten Beträgen **nicht vermischen**. `Bot capital assigned` ist lokale autorisierte Obergrenze, kein transferierter oder physisch abgesonderter Bitget-Saldo. Tagesverlust nach P5 V1.2 cashflowbereinigt; Transfers, Funding, Gebühren, externe Trading-Aktionen und PnL gesondert journalisieren.
- Typisierte `snapshot/update`-Semantik je Channel **nicht ungeprüft mit Replace/Merge gleichsetzen**. Wenn ein Field fehlt oder ein partieller Update nicht eindeutig interpretierbar ist, muss der State degradiert und per REST reconciliiert werden. Keine Float-Geldarithmetik, keine stillen Defaults auf 0 oder FLAT, keine interne JSON-Wahrheit.
- Der Risk-/Order-Owner liest **denselben Account-State** wie der UI-Publisher (plus restriktive interne Reservations/Flags); UI hat **keinen** eigenen Börsen-Wahrheitszustand und darf nie die Trade-Entscheidung treiben. Protobuf/ACL nur minimal erforderliche öffentliche Anzeige-/Control-Felder; API-Key/Secret niemals in Browser, Topic, Log, DBREC oder URL.

## 4. Fester UI-Vertrag – nur im bestehenden DarkNode-Layout

| Bereich | Verpflichtende Anzeige (Exchange- oder abgeleitete Herkunft kennzeichnen) |
|---|---|
| **Wallet: Konto** | DEMO/LIVE + Subaccount/UID-Kürzel, Sync-Güte/`age_ms`/Timestamp, Coin-Währung und USD-Konversion |
| **Wallet: Guthaben** | Total Equity (USD), pro Coin Balance/Available (USDT für Futures ausdrücklich), Exchange-Margin/effEquity, unrealized/realized PnL, Funding/Fees soweit qualifiziert |
| **Wallet: Positionen** | **alle** Exchange-Positionen, Symbol, Long/Short, Base-Menge, Notional, Einstieg, Hebel, Margin-Mode, Margin, Markt-/Mark-Preis, geschätzter Liquidationspreis, Open PnL, Stop-Preis und echte Exchange-Stopdeckung |
| **Wallet: Orders/Schutz** | offene/teilgefüllte/UNKNOWN Orders, clientOid/orderId (bei Bedarf gekürzt), Preis/Menge, Stop/TP-Status; keine vermeintlich bestätigte Schutzorder aus lokalem Request |
| **Wallet: Bot-Risiko** | einstellbare/verifizierte Bot-Risikobudgetgrenze, bereits reserviert, Maximal-Notional, errechnete **noch erlaubte** Positionsgröße und expliziter Grund für NO_ENTRY/STOP_UNKNOWN/RECONCILING |
| **Wallet: Historie** | zeitlich nachvollziehbare ausgeführte Fills, Gebühren, Funding, Closed/Unrealized PnL und Account-Flows; Restore aus Exchange-REST plus durable Journal |
| **Overview / Live** | kompakte Highlights/aktuelle Position im jeweiligen vorhandenen Reiter aus **demselben** Protobuf-State; keine duplizierte Exchange-Anbindung, kein sechster Tab |

Bei `STALE/RECONCILING/DISCONNECTED` Werte mit **„Stand: <Zeit> – nicht aktuell“** markieren, Freigaben/Freshness nicht vortäuschen und keine vorhandenen Positionen einfach löschen oder zu 0 machen. Bei konkurrierenden Trading-Akteuren (Bitget-App, manuelle Order) die tatsächliche Exchange-Position **anzeigen** und gleichzeitig Bot-Neuentries fail-closed behandeln. Wallet-Kontroll- und Not-Aus-Aktionen ausschließlich durch separat autorisierte, quittierte Gateway-Commands, **nie** direkter Bitget-Aufruf aus der UI. Frontend darf jederzeit schließen; Rust-Backend-/Private-WS-/Risk-Owner läuft autonom weiter.

## 5. Nachweise (bestehende P5-A01–A28 und P7, keine Test-Schein-PASS)

| Testbereich | Verbindliche konkrete Assertions | Status |
|---|---|---|
| **P5-A02/A13** | echtes Account/UID/Subaccount/DEMO-vs-LIVE/Asset-Snapshot, coin/currency/USDT-vs-USD-Einheiten, API least privilege | NOT_RUN |
| **P5-A08/A12/A26/A27** | WS verbinden→Events puffern→REST-Snapshot→Update-Kausalität, **order channel initial keine offenen Orders**, Crash/Restart, Lücken/duplizierte/verspätete Events, Unknown/foreign Order, Transfer, Bitget-App-Änderung | NOT_RUN |
| **P5-A11/A14** | Konto-Equity vs Coin-Balance vs `available`, margin/debts/bonus, lokale Reservationen, Fremdpositionsbelastung, maximal erlaubte Orderqty inkl. UNKNOWN-Orders, kein Hebel-/Double-Count-Bypass | NOT_RUN |
| **P5-A17–A20/A28** | nach jedem Fill/Partial/ADL Stopgröße/Trigger vs Exchange-Position, getrennte Stop-Order-REST- und private WS-Readbacks, keine Schein-Sicherheit | NOT_RUN |
| **P5-A24/A26 + P7** | UI-Schließen stoppt Rust-WS nicht; Frontend ohne Key; Wallet/Live/Overview zeigen denselben versionierten Protobuf-Snapshot, stale/mismatch markiert, keine Nullposition bei Offline, FE-ACL/Receipt | NOT_RUN |
| **P8** | Konto-WS-Ingress→Risk-/Wallet-Publish p50/p95/p99, Periodik, Event-Bursts, Gap/Restart/Rate-Limits/REST-Backoff über mehrtägigen Soak | NOT_RUN |

**Grenzen:** Nicht als „1:1 alle Bitget-Web/App-Funktionen“ versprechen. V1 ist eine **1:1-synchronisierte, definierte Menge an Konto-/Futures-/Orderzuständen**; Ein-/Auszahlungen und Konto-Administration bleiben in Bitget. Positiver WS-Login/Subscription ist kein Beweis vollständiger Konto-Synchronisierung; private Demo-Read-only + reale Reconciliation-Tests sind zwingend.

## 6. Offizielle Referenzen

- [Bitget Account Channel](https://www.bitget.com/docs/uta/websocket/private/Account-Channel)
- [Bitget Positions Channel](https://www.bitget.com/docs/uta/websocket/private/Positions-Channel)
- [Bitget Orders Channel und kein Initial-Snapshot](https://www.bitget.com/docs/uta/best-practices-guide)
- [Bitget Fill Channel](https://www.bitget.com/docs/uta/websocket/private/Fill-Channel)
- [Bitget Private WS / Fast-Fill-Erweiterung](https://www.bitget.com/docs/uta/changelog/2026-03)
- [Bitget UTA API und REST-Endpunktkatalog](https://www.bitget.com/docs/uta/rest-api)
