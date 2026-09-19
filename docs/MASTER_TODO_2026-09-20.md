# DarkBot Phase 1 – Master TODO
Stand: 2026-09-20
Basis: SPEC 1.3.2, aktueller GitHub-Stand R8_2b (`48c21d9f1219321fef5ccb3b77e24b38d469f4c3`)

## Zweck dieser Datei

Diese Datei ist die zentrale, zusammengeführte TODO-Liste für DarkBot Phase 1. Sie enthält:

- alle aktuell bekannten Blocker und offenen technischen Punkte,
- die während des aktuellen Live-PAPER-Tests neu gefundenen Aufgaben,
- die aus R8_2/R9 noch ausdrücklich offenen Punkte,
- die nächsten Schritte bis zum belastbaren Edge-Nachweis,
- die Gates vor einem späteren Freeze.

Die Strategie selbst bleibt unverändert:

`Pump.fun CREATE -> so früh wie möglich erkennen -> PAPER BUY -> halten -> Creator SELL = sofort EXIT -> Netto-Target = EXIT`

Aktuell:
- `observation_ms = 0`
- `entry_delay_ms = 0`
- `exit_delay_ms = 0`
- `target_net_multiple = 1.5`
- `ordinary_stop_enabled = false`
- `creator_sell_exit_enabled = true`
- `strategy_timeout_ms = 590000`
- `max_open_positions = 1`

LIVE-Broadcast bleibt in Phase 1 technisch verboten.

---

# PRIORITÄT P0 – MUSS VOR WEITEREM EDGE-TEST GELÖST WERDEN

## TODO P0.1 – Creator-Sell-Exit-Liveness bei `DECODE_GAP` vollständig untersuchen und beheben

### Problem

Im realen aktuellen PAPER-Lauf wurde ein Creator-Sell nach unserem erfolgreichen Entry erkannt und in der Datenbank gespeichert, aber für die offene Position wurde kein SELL-Attempt erzeugt.

Damit wurde die zentrale Strategieanforderung verletzt:

`Creator verkauft -> sofortiger Exit-Versuch`

### Reproduktionsfall

Run:

`edge_probe_01_paper_20260919T220756Z_804f03`

DB:

`data/edge_probe_01.sqlite`

Betroffene Position:

`edge_probe_01_paper_20260919T220756Z_804f03_p2`

Mint:

`9CbtNYyjtqD5rMHBKtczGf5EVUqiLCy8x8mUVJgupump`

Creator:

`E8Yma8FN33iJ4aMvCXfQTn5z1qkPqJ7Bp3XJEWNGpv2x`

Entry:

`open_mono_ns = 1657166201665750`

Creator-Sell:

`first_seen_mono_ns = 1657260603098791`

Differenz:

`94.401.433.041 ns = ca. 94,40 Sekunden nach Entry`

Der Creator-Sell wurde eindeutig erkannt:
- Eventtyp `SELL`
- Actor entspricht gespeichertem Creator
- `creator_sells_seen = 1`

Trotzdem:
- Position blieb `OPEN`
- `origin_trigger = NULL`
- `exit_reason = NULL`
- `net_pnl_lamports = NULL`
- es existiert kein SELL-Attempt für `p2`

### Kontrollfall

Im selben Run funktionierte die Logik für Position `p1`:

- BUY FILLED
- SELL FILLED
- `origin_trigger = CREATOR_SELL`
- `exit_reason = CREATOR_SELL`
- `state = CLOSED`
- `net_pnl_lamports = -4.357.966`

Creator-Sells werden also grundsätzlich erkannt. Der Fehler betrifft die Zuverlässigkeit/Liveness des Exit-Pfades.

### Diagnose – bestätigt

Die zusätzliche Live-DB-Abfrage hat die Ursache wesentlich eingegrenzt.

Für `p2` existiert folgende Exit-Decision:

- `decision_done_mono_ns = 1657260603247500`
- Creator-Sell: `1657260603098791`
- Reaktionszeit vom Empfang des Creator-Sells bis zur Exit-Decision:
  `148.709 ns ≈ 0,149 ms`
- `action = EXIT`
- `reason = EXIT_LATCHED_DECODE_GAP`

Zusätzlich wurde in der Position inzwischen gespeichert:

- `origin_trigger = CREATOR_SELL`

Damit ist bewiesen:

1. Der Creator-Sell wurde erkannt.
2. Der Creator-Sell wurde als Exit-Trigger gelatcht.
3. `_position_tick()` / Exit-Entscheidung wurde tatsächlich ausgeführt.
4. Die Exit-Entscheidung erfolgte extrem schnell nach dem Creator-Sell.
5. Der SELL wurde nicht wegen Fee-/Cost-Freshness blockiert.
6. Die letzten Fee-Snapshots vor dem Creator-Sell waren für NETWORK_FEE, PRIORITY_POLICY, PROTOCOL_GLOBAL_ACCOUNT und RENT_EXEMPTION jeweils `VALID`.
7. Der konkrete Blocker war `DECODE_GAP`.
8. Deshalb wurde kein SELL-Attempt erzeugt.
9. Die Position blieb offen und wurde am Run-Ende als `OPEN_AT_RUN_END` markiert.

Der Fehler ist daher **kein Creator-Sell-Erkennungsfehler**, sondern ein **Exit-Liveness-Problem bei einem bereits gelatchten Creator-Sell während eines Decode-Gaps**.

### Technischer Kontext

Der Reducer öffnet bei einer partiellen/unklaren Pump-Transaktion einen `decode_gap_since`, weil unbekannte Pump-Ereignisse die tatsächliche Curve zwischen bekannten Zuständen verändert haben könnten.

`_position_tick()` erkennt den Creator-Sell trotzdem und latcht `CREATOR_SELL`.

`_maybe_exit()` verweigert jedoch bei offenem Decode-Gap einen SELL-Quote und protokolliert:

`EXIT_LATCHED_DECODE_GAP`

Das ist hinsichtlich Datenintegrität zunächst korrekt: Es darf kein fiktiver Fill auf einer möglicherweise falschen Curve erzeugt werden.

Das Problem entsteht bei der Liveness:

Wenn nach dem Creator-Sell kein weiterer vertrauenswürdiger State des Mints mehr kommt, kann der Decode-Gap nicht über den normalen Event-Pfad geschlossen werden. Der gelatchte Exit kann dadurch lange offen bleiben bzw. bis Run-Ende/Timeout hängen.

### Noch zu klärende Root-Cause-Details

Astra soll den exakten Ursprung des Decode-Gaps im Creator-Sell-Transaktionskontext bestimmen:

- Welche Observation/Transaktion öffnete `decode_gap_since`?
- War die Creator-Sell-Transaktion selbst `PARTIAL`?
- Welcher unbekannte/undecodierbare Pump-Event-Discriminator verursachte die Partial-Einstufung?
- Lag der unbekannte Event vor oder nach dem bekannten SELL innerhalb derselben Transaktion?
- Enthielt der bekannte SELL belastbare Reservewerte?
- Ist die konservative Nichtanwendung des SELL-State gemäß SPEC zwingend?
- Welches bereits erlaubte Ereignis kann einen gelatchten Exit nach einem Decode-Gap sicher wieder aktivieren?
- Falls heute nur ein weiteres Market-Event dies tut: ist ein zusätzlicher Timer-/Fee-/State-Recovery-Mechanismus gemäß SPEC erforderlich?

Astra darf nicht einfach den Decode-Gap ignorieren oder auf unsicheren Reserven verkaufen. Die Lösung muss die Datenintegrität erhalten und gleichzeitig verhindern, dass ein gelatchter Creator-Sell ohne sinnvollen Recovery-Pfad hängen bleibt.

### Erwartetes Verhalten nach Fix

Sobald ein Creator-Sell nach erfolgreichem Entry erkannt wird:

1. `CREATOR_SELL` dauerhaft latchen.
2. `origin_trigger = CREATOR_SELL`.
3. Sofort einen SELL versuchen.
4. Ist ein sicherer SELL momentan nicht möglich, darf der Trigger niemals verloren gehen.
5. Der Exit muss automatisch erneut versucht werden, sobald die notwendige Bedingung wieder gültig ist.
6. Dafür darf kein zufälliges weiteres BUY-/SELL-Event des Coins erforderlich sein.
7. Keine fiktiven Fills.
8. Keine veraltete Fee-Basis.
9. Kein zusätzlicher schwerer RPC-/DB-/Hashing-Pfad im BUY-Hot-Path.
10. Keine parallelen Duplicate-SELL-Attempts.

### Pflicht-Regressionstests

Mindestens:

#### Fall A
CREATE -> BUY FILLED -> Creator SELL -> SELL-Attempt sofort.

#### Fall B
Creator SELL trifft ein, während SELL-Kosten temporär unqualifiziert sind.

Erwartung:
- Trigger bleibt gelatcht.
- kein Verlust des Exit-Wunsches.
- automatischer Retry bei wieder gültigen Kosten.
- kein weiteres Coin-Trade-Event erforderlich.

#### Fall C
Fee-Snapshot wird nach Creator-Sell wieder gültig.

Erwartung:
- SELL wird automatisch ausgelöst.

#### Fall D
Nach Creator-Dump kommen keinerlei weitere Trades.

Erwartung:
- kein Hängen bis zum normalen 590-s-Timeout.

#### Fall E
Creator-Sell und Target liegen gleichzeitig/nahe beieinander.

Priorität muss bleiben:

`CREATOR_SELL -> TARGET -> TIMEOUT`

#### Fall F
Mehrere Timer/Fee-Callbacks.

Erwartung:
- kein paralleler Duplicate-SELL.

#### Fall G
Realer `edge_probe_01`-Fall per Replay/Fixture reproduzieren.

### Acceptance Criteria

Der Bug ist erst geschlossen, wenn:

- die Root Cause dokumentiert ist,
- der reale Ablauf reproduziert wurde,
- ein Regressionstest vor Fix fehlschlägt,
- derselbe Test nach Fix besteht,
- die vollständige Testsuite PASS ist,
- Creator-Sell niemals verloren gehen kann,
- ein gelatchter Creator-Sell bei `DECODE_GAP` einen expliziten, SPEC-konformen Recovery-/Retry-Pfad besitzt,
- keine unsichere/fiktive Curve als Fill-Basis verwendet wird,
- ein temporär nicht quotierbarer Exit nicht stillschweigend bis zum normalen Timeout verschleppt,
- kein neuer Hot-Path-I/O entsteht,
- Replay reproduzierbar bleibt.

---

## TODO P0.2 – Aktuelle `edge_probe_01.sqlite` als Bug-Evidenz sichern

Die aktuelle DB darf nicht versehentlich überschrieben werden, bevor P0.1 reproduziert wurde.

Zu sichern/dokumentieren:

- `data/edge_probe_01.sqlite`
- Run-ID
- Position `p1`
- Position `p2`
- relevante `market_events`
- `execution_attempts`
- `decisions`
- Fee-Snapshots um den Creator-Sell-Zeitpunkt
- ggf. Export/Report des Runs

Diese Aufnahme ist ein echter Mainnet-PAPER-Reproduktionsfall und wertvoller als ein nur synthetischer Test.

---

# PRIORITÄT P1 – DIREKT NACH P0

## TODO P1.1 – Live-PAPER-Status / Observability massiv verbessern

### Problem

Die aktuelle Statuszeile ist für Edge-Debugging zu blind.

Beispiel:

`fills=1 open=1 pnl=0`

Dabei war bereits ein kompletter erster Trade geschlossen und eine zweite Position offen.

`fills` bezeichnet aktuell Entry-Fills; Exit-Fills werden separat gezählt, aber nicht in der Statuszeile dargestellt.

Dadurch ist während des Tests nicht sichtbar:

- welcher Coin offen ist,
- ob er steigt oder fällt,
- wie viele Trades eintreffen,
- ob der Creator verkauft hat,
- ob ein Exit gelatcht wurde,
- ob ein SELL pending ist,
- wie weit das Target entfernt ist.

### Statuszeile soll mindestens enthalten

Für offene Position:

- `mint`
- `position_id`
- `state`
  - OPEN
  - EXIT_PENDING
  - UNRESOLVED
- Positionsalter
- Entry-Fills
- Exit-Fills
- Anzahl BUY-Events seit Entry
- Anzahl SELL-Events seit Entry
- Creator-Sell erkannt: ja/nein
- Zeitpunkt/Alter des Creator-Sells
- letzter Trade-Zeitpunkt
- aktuelle Reserve-/Curve-Situation
- korrekt berechnetes Preis-/Curve-Multiple gegenüber Entry
- aktueller Net-Liquidation-Value
- NLV-Multiple gegenüber Entry-Economic-Cost
- Target erreicht: ja/nein
- `latched_triggers`
- `origin_trigger`
- Exit-Grund
- Pending SELL-Attempt
- Attempt-Status
- Timeout-Restzeit
- ggf. aktuelle Fee-/Cost-Readiness

### Wichtig

Preis-/Curve-Multiple darf nicht durch getrennte `MAX()`-Werte verschiedener Events berechnet werden.

Reservenzähler und Nenner müssen aus demselben kanonischen State/Event stammen.

### Performance-Anforderung

Keine schwere zusätzliche Arbeit im kritischen BUY-Pfad.

Statuswerte möglichst aus bereits vorhandenem RAM-State/Background-State ableiten.

---

## TODO P1.2 – Nach P0-Fix neuer kurzer Live-PAPER-Sanity-Test

Noch kein 1.000-CREATE-Langlauf.

Ziel:

Mehrere vollständige Roundtrips der reparierten Engine prüfen.

Für jeden Trade kontrollieren:

- CREATE korrekt
- Entry korrekt
- Entry-Fill korrekt
- Creator-Sell oder Target korrekt erkannt
- Exit sofort korrekt ausgelöst
- Exit-Fill korrekt
- Gebühren vollständig
- Rent korrekt
- PnL korrekt
- keine UNKNOWN-/UNRESOLVED-Anomalie
- keine internen Fehler
- kein Coverage-Gap
- keine Queue-Overflows

Erst wenn dieser Sanity-Test sauber ist, lohnt sich weitere Edge-Statistik.

---

## TODO P1.3 – Feed-Race / reale Feed-Eingangslatenz messen

### Warum

Mit nur einem Public-RPC wissen wir nicht, wie spät wir einen CREATE tatsächlich sehen.

Die lokale `receive -> decision`-Latenz kann sehr klein sein und trotzdem kann unser Feed bereits 50, 100 oder 300 ms hinter einem schnelleren Provider liegen.

### Aufbau

Mindestens zwei unabhängige Feeds parallel:

- Helius
- `existing_rpc` Public RPC

Optional später weiterer unabhängiger Provider.

### Messung

Für denselben kanonischen CREATE:

- gleiche Mint
- gleiche Signatur
- gleiche Instruction
- `receive_mono_ns` je Feed

Dann messen:

- welcher Feed zuerst kam,
- Delta zwischen Feeds,
- p50,
- p95,
- p99,
- max,
- Anteil der CREATEs, bei denen Feed A/B gewinnt,
- Anteil der Events, die nur ein Feed liefert,
- Disconnect-/Reconnect-Effekte.

### Wichtig

Die Messung muss auf demselben Rechner und derselben Clock-Epoche erfolgen.

### Ziel

Reale Feed-Verzögerung später in das Latenz-/Edge-Modell übernehmen.

Nicht nur lokale Decode-Zeit messen.

---

## TODO P1.4 – Required-Feed-Semantik Helius vs. `existing_rpc` klären

README bezeichnet `helius_standard` als Pflichtfeed.

Aktuell kann D02 trotzdem PASS sein, obwohl:

`HELIUS_API_KEY` fehlt

und nur `existing_rpc` READY ist.

Das ist zumindest eine Semantik-/Dokumentationsfrage, die vor Freeze eindeutig geklärt werden muss:

Variante A:
Helius ist wirklich Pflicht -> Doctor/Config muss dies erzwingen.

Variante B:
Mindestens ein qualifizierter logsSubscribe-Feed reicht -> README/Config müssen entsprechend angepasst werden.

Nicht stillschweigend widersprüchliche Regeln behalten.

Für den späteren Feed-Race-Test sind zwei unabhängige Feeds ohnehin notwendig.

---

## TODO P1.5 – Realistische Execution-Latenz modellieren und Edge-Stress testen

### Aktueller Zustand

Baseline:

- `entry_delay_ms = 0`
- `exit_delay_ms = 0`
- Label: `ASSUMED_OPTIMISTIC_BASELINE`
- `zero_delay_run_can_qualify_edge = false`

Die reale Verzögerung bis die WebSocket-Nachricht unseren Mac erreicht, wirkt im Live-Lauf tatsächlich.

Nicht realistisch modelliert ist derzeit zusätzlich:

`Decision -> TX-Send -> RPC/Validator -> tatsächliches Landing`

### Vorhandene Stress-Szenarien

- 5 ms
- 25 ms
- 100 ms
- 250 ms
- 500 ms

### Vorgehen

Für denselben aufgezeichneten Datensatz:

1. 0-ms Baseline
2. 5 ms
3. 25 ms
4. 100 ms
5. 250 ms
6. 500 ms

Entry- und Exit-Verzögerung getrennt oder sauber als definierte Szenarien testen.

### Auswertung

Je Szenario:

- Anzahl Entries
- Anzahl Fills
- Slippage-Rejects
- Exit-Rejects
- Creator-Sell-Ergebnis
- Target-Ergebnis
- Netto-PnL
- EV pro Trade
- Profit Factor
- Drawdown
- Anteil negativer Trades
- offene/unresolved Positionen
- Sensitivität gegen Latenz

### Ziel

Bestimmen, ab welcher realistischen Latenz eine eventuell vorhandene Edge verschwindet.

Später synthetische Delays durch tatsächlich gemessene Feed-/Execution-Verteilungen ergänzen.

---

# PRIORITÄT P1 – EDGE-NACHWEIS

## TODO P1.6 – Alte positive Edge nur A/B testen, falls Originaldaten wiedergefunden werden

Die alte positive Rohaufnahme ist aktuell nicht im Projektbaum/ZIP-Bestand auffindbar.

Daher:

- keine synthetische Ersatz-Baseline erfinden,
- keine alten Resultate als reproduziert behaupten.

Falls die alte DB später gefunden wird:

1. exakte alte Engine-/Config-Version identifizieren,
2. alten Baseline-Lauf reproduzieren,
3. denselben Datensatz mit neuer Engine und identischen alten Annahmen laufen lassen,
4. Trade für Trade vergleichen,
5. danach Realismus einzeln zuschalten.

Delta-Attribution:

- alte vs. neue Fee-Pipeline
- Entry-Latenz
- Exit-Latenz
- Slippage
- Reject-/Failure-Modell
- Rent/ATA
- Token-2022-Kosten

Ziel:
Unterscheiden zwischen:
- echter Implementierungsregression
und
- Edge, die nur unter zu optimistischen Annahmen existierte.

Falls die alte Aufnahme nicht mehr existiert, diesen Punkt dokumentiert schließen und mit frischen aktuellen Daten weitermachen.

---

## TODO P1.7 – Aktuellen Edge-Datensatz aufbauen

Nach P0-Fix und Sanity-Test:

- neuer sauberer Live-PAPER-Lauf,
- aktuelle Mainnet-Daten,
- keine Strategieänderung während der Aufnahme,
- Config/SPEC/Code-Revision eindeutig gebunden,
- Kosten-Snapshots qualifiziert,
- Feed-Health dokumentiert,
- vollständige Replay-Fähigkeit.

Der Datensatz dient als Grundlage für die aktuelle Edge-Analyse.

---

## TODO P1.8 – ≥1.000 CREATE Pilot

Erst starten, wenn:

- P0 Creator-Sell-Bug geschlossen,
- vollständige Testsuite PASS,
- Doctor D01–D06 PASS,
- kurzer Live-PAPER-Sanity-Test sauber,
- keine bekannten P0/P1-Liveness-Fehler,
- aktuelle Kostenbasis qualifiziert.

Der Pilot soll mindestens etwa 1.000 CREATEs enthalten.

Auswertung unter anderem:

- CREATEs insgesamt
- actionable CREATEs
- Entries
- Fill-Rate
- Creator-Sell-Exits
- Target-Exits
- Timeout-Exits
- Rejects
- UNKNOWN
- UNRESOLVED
- Netto-PnL
- EV/Trade
- Profit Factor
- Verteilung der Haltedauer
- Latenzen
- Gebührenanteil
- Slippage
- Drawdown
- Coverage-/Feed-Probleme
- Token-2022/SPL-Profilverteilung

Kein voreiliger Profit-Claim.

---

## TODO P1.9 – Research Freeze / Kandidaten-Freeze

Nach dem Explorations-/Pilotdatensatz:

- Forschungsparameter festlegen,
- Kandidaten definieren,
- keine nachträgliche Anpassung an Holdout/Forward-Ergebnis,
- vollständige Config-/Dataset-/Code-Hashes festhalten.

Explorationsdaten dürfen nicht nachträglich als unabhängige Validierung verkauft werden.

---

## TODO P1.10 – Holdout-Test

Ein zuvor unangetasteter Holdout-Split.

Prüfen:

- gleiche Strategie
- gleiche gebundene Parameter
- keine manuelle Optimierung nach Sichtung
- Kosten-/Latency-Stress
- offene/unresolved Exposures

Holdout dient zur internen Auswahl/Validierung nach dem vorgesehenen Research-Prozess, nicht als globaler Profitnachweis.

---

## TODO P1.11 – Fresh-Forward-Test

Neue Daten nach Freeze aufnehmen.

Nur der vorregistrierte Kandidat wird verwendet.

Keine Parameteränderung nach Sichtung.

Erst hier kann bei bestandenen Bedingungen die Qualifikation in Richtung:

`POSITIVE_UNDER_MODEL`

gehen.

---

## TODO P1.12 – Stress / Bootstrap / Robustheit

Vor einer positiven Qualifikation:

- Fee-Multiplikatoren
- adverse price bps
- Exit-Reject-Rate
- reale bzw. gemessene Latenzverteilungen
- höhere Latenzen
- Feed-Ausfälle
- Fee-Snapshot-Ausfälle
- ggf. Bootstrap/Resampling der Trade-Ergebnisse
- Drawdown-/Tail-Risk-Auswertung

Ziel:

Nicht nur Mittelwert positiv, sondern verstehen, wie empfindlich das Ergebnis ist.

---

# PRIORITÄT P2 – TECHNISCHE HÄRTUNG VOR FREEZE

## TODO P2.1 – Portfolio-Kontinuität vollständig prüfen

Aus R8_1/R9 noch als offenes Thema bekannt.

Prüfen:

- Verhalten über kontrollierte Run-Neustarts
- Kampagnenweite Reentry-Sperre
- keine doppelte Position nach Restart
- kein Verlust reservierter/exponierter Zustände
- klare Behandlung unterbrochener Runs
- keine erfundene automatische Schließung offener Positionen

SPEC-konform dokumentieren und testen.

---

## TODO P2.2 – Purge-/Retention-Protokoll fertig definieren und testen

Aus Übergabe noch offen.

Prüfen:

- welche Daten wann gelöscht/evicted werden dürfen,
- aktive Positionen niemals versehentlich entfernen,
- Research-Horizon respektieren,
- Replay-Evidenz erhalten,
- kanonische Event-/Fee-/Decision-Provenienz nicht zerstören,
- DB-/RAM-Retention konsistent.

---

## TODO P2.3 – RAM-/Liveness-Grenzen weiter testen

Bekanntes offenes Thema.

Tests unter hoher Last:

- viele CREATEs
- viele parallele nicht gehandelte Mints
- starkes Trade-Aufkommen
- begrenzte Histories
- Dedup-History
- Signature-History
- Research Feature Jobs
- Writer Queue
- Fee-History
- Pending Attempts
- lange Runs

Prüfen:

- Memory bleibt bounded,
- aktive Positionen werden geschützt,
- keine Liveness-Verluste,
- keine O(n)-Scans im Hot Path,
- keine unbounded Listen/Maps.

---

## TODO P2.4 – Ressourcen-Cleanup vollständig prüfen

Aus Übergabe noch offen.

Prüfen:

- WebSockets
- Tasks
- ThreadPoolExecutor
- Writer-Thread
- SQLite-Verbindungen
- Scheduler-Timer
- Ctrl-C
- SIGTERM
- Exceptions
- Reconnect
- Doctor-Ende
- PAPER-Ende
- Replay-Ende

Kein Leaking von Threads, Connections oder offenen DB-Handles.

---

## TODO P2.5 – Custom PDA / `_on_curve` nochmals unabhängig gegen kanonische Solana-Implementierung validieren

Die aktuelle Implementierung ist bereits gegen aufgezeichnete Mainnet-Adressen getestet.

Vor Freeze zusätzliche unabhängige Absicherung:

- bekannte PDA-Testvektoren,
- Vergleich mit kanonischer Solana-Web3/SDK-Implementierung,
- ATAs/Pump-PDAs,
- SPL und Token-2022.

Ziel:
Kein stiller Fehler in eigener Ed25519/PDA-Logik.

---

## TODO P2.6 – Token-2022/V2 finaler Re-Review vor Freeze

Token-2022/V2 ist strukturell implementiert und lokal getestet.

Der letzte unabhängige Reviewer prüfte jedoch einen Stand vor einigen gebündelten Abschlusskorrekturen.

Daher vor Freeze:

- gezielter finaler SPEC/Test-Review,
- Token-2022-Profile,
- BUY/SELL Message-Metas,
- Fee-Scope,
- Rent,
- Pending Fill,
- Venue invalidation,
- Replay-Provenienz,
- Fail-closed-Verhalten.

Nicht nach jedem kleinen Patch; einmal gebündelt vor Freeze.

---

## TODO P2.7 – Doctor-/Readiness-Semantik dauerhaft konsistent halten

Aktueller Stand R8_2b:

- D01–D06 müssen PASS sein,
- dann `ECONOMIC_PAPER_READY`,
- fehlender D01/Feed -> `NOT_READY`,
- sonst höchstens `CAPTURE_READY`.

Vor Freeze Regression weiter absichern:

- kein späterer Refactor darf D04/D06 wieder aus dem finalen Gate entfernen,
- Report-Level, `summary.readiness` und `runs.readiness` müssen identisch bleiben,
- `runtime_readiness` getrennt dokumentieren.

---

## TODO P2.8 – Statusdokumentation bereinigen

Bekannte Dokumentationsreste korrigieren:

`docs/R8_2_DOCTOR_GATE_STATUS.md`

enthält noch die Aussage:

`noch nicht committed/gepusht`

obwohl der Fix inzwischen als R8_2b im Repository liegt.

Zusätzlich Versions-/Arbeitsnamen sauber halten:

- R8_2
- R8_2b
- alte Arbeitsbezeichnung R9

Keine funktionale Priorität, aber vor Freeze bereinigen.

---

## TODO P2.9 – Reproduzierbaren CI-/Testnachweis entscheiden

Aktuell sind die 130 Tests lokal dokumentiert.

Es existiert kein bestätigter GitHub-Actions-Lauf für diesen Stand.

Vor Freeze entscheiden:

Variante A:
Minimalen CI-Workflow hinzufügen und Tests bei Push/Freeze laufen lassen.

oder

Variante B:
Bewusst ohne CI bleiben, aber einen klar reproduzierbaren lokalen Freeze-Testnachweis mit Python-Version, Commit-Hash und kompletter Ausgabe archivieren.

Ziel:
Finaler Teststatus darf nicht nur aus einer Textbehauptung bestehen.

---

# PRIORITÄT P3 – FINALER FREEZE / QUALIFIKATION

## TODO P3.1 – Full Audit vor Freeze

Erst am größeren Meilenstein.

Rollen:

- CODER
- SPEC REVIEWER
- TEST REVIEWER
- ARCHITECTURE / LATENCY REVIEWER

Prüfen:

- vollständige SPEC 1.3.2
- keine versteckten Hardcodes
- keine Strategieabweichung
- Hot Path
- Async
- Netzwerk
- DB/I/O
- Timing
- Retry-/Liveness
- Replay
- Kosten
- Token-2022
- Reporting
- Research Gates

Reviewer gebündelt einsetzen, nicht nach jedem kleinen Patch.

---

## TODO P3.2 – Implementation Freeze

Wenn alle notwendigen Gates bestanden sind:

- Commit festlegen
- SPEC-Hash festlegen
- Config-Hashes festlegen
- Testnachweis archivieren
- Doctor-Nachweis archivieren
- Dataset-Hashes festlegen
- keine Änderungen während Holdout/Fresh-Forward

---

## TODO P3.3 – Qualifikationsregel strikt einhalten

Nie behaupten:

- garantierter Profit
- sichere Fills
- globale Erstkenntnis
- LIVE-Profitabilität allein aus PAPER

Zulässige spätere Qualifikation nur bei bestandenen Gates:

`POSITIVE_UNDER_MODEL`

und nur wenn:

- vorregistrierter Kandidat,
- Fresh Forward,
- Stress bestanden,
- keine ungeklärten Exposures,
- Kosten qualifiziert,
- Latenzmodell dokumentiert,
- Replay/Dataset integer.

---

# Aktuelle Reihenfolge ab jetzt

## Schritt 1
Aktuellen `edge_probe_01` sichern.

## Schritt 2
P0 Creator-Sell-Exit-Bug vollständig diagnostizieren.

## Schritt 3
Astra repariert den P0-Bug und ergänzt Regressionstests.

## Schritt 4
Komplette Testsuite ausführen.

## Schritt 5
Doctor D01–D06 erneut PASS.

## Schritt 6
Kurzer Live-PAPER-Sanity-Test mit mehreren vollständigen Roundtrips.

## Schritt 7
Observability verbessern, damit offene Positionen während des Tests sichtbar und verständlich sind.

## Schritt 8
Feed-Race Helius vs. Public RPC messen.

## Schritt 9
Aktuellen sauberen Edge-Datensatz aufnehmen.

## Schritt 10
0-ms Baseline und Latenz-Stress 5/25/100/250/500 ms vergleichen.

## Schritt 11
≥1.000 CREATE Pilot.

## Schritt 12
Research/Kandidaten-Freeze.

## Schritt 13
Holdout.

## Schritt 14
Fresh Forward.

## Schritt 15
Stress/Bootstrap/Robustheit.

## Schritt 16
Full Audit.

## Schritt 17
Implementation Freeze.

## Schritt 18
Nur bei bestandener Evidenz: `POSITIVE_UNDER_MODEL`.

---

# Was aktuell bereits erledigt ist

Damit diese TODO-Liste nicht alte abgeschlossene Punkte erneut als offen führt:

- SPEC 1.3.2 vorhanden und unverändert.
- PAPER-only / LIVE-Broadcast gesperrt.
- Token-2022 / Pump V2 implementiert.
- SPL-/Token-2022-Profiltrennung implementiert.
- V2 BUY/SELL Message-Templates implementiert.
- Fee-Profiles BUY/SELL getrennt.
- Rent 165/170 Byte getrennt.
- Replay-/SQL-Provenienz erweitert.
- Doctor-Gate D01–D06 korrigiert.
- 130 lokale Tests PASS vor dem aktuellen neu gefundenen Exit-Bug.
- echter Doctor D01–D06 PASS.
- `ECONOMIC_PAPER_READY` wurde erreicht.
- erster aktueller kompletter PAPER-Trade wurde geschlossen.
- zweiter Trade lieferte den aktuellen P0-Reproduktionsfall.
- ≥1.000-CREATE-Pilot wurde bewusst noch nicht gestartet.
- positiver Erwartungswert ist noch nicht nachgewiesen.

---

# Wichtigste Regel für die nächsten Schritte

Keine Optimierung der Strategie, solange P0-Bugs oder Messfehler offen sind.

Zuerst sicherstellen, dass exakt die bestehende Strategie korrekt ausgeführt und gemessen wird.

Erst danach bewerten, ob tatsächlich eine Edge vorhanden ist.
