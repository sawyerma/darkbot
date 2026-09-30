> Historische erste Aufgabenfassung. Ersetzt durch `STRATEGY_B_SOURCE_OF_TRUTH.md`, insbesondere Quellenfreigabe und $10-Budgetsemantik. Nicht als aktuelle Implementierungsanweisung verwenden.

# DARKBOT – VERBINDLICHE AUFGABE: STRATEGY B ZUSÄTZLICH ZU STRATEGY A IMPLEMENTIEREN

## 0. WICHTIG: SOURCE-OF-TRUTH-REIHENFOLGE

Für diese Aufgabe gilt folgende Priorität:

### 1. Diese Anweisung
Diese Anweisung definiert verbindlich, **was Strategy B tun soll**.

### 2. Aktueller DarkBot-Repository-Stand + aktuelle SPEC
Diese definieren verbindlich:
- Architektur
- Market-/Feed-System
- Execution
- Fees
- Token-2022
- Replay
- SQLite
- Reporting
- Latenzmessung
- Safety/Fail-closed-Verhalten

### 3. Historische Dateien
Zusätzlich erhältst du:

- `backend.zip`
- `lean_v1_frozen(1).zip`

Diese Dateien sind **nur READ-ONLY historische Referenz**.

Sie dürfen verwendet werden, um alte Begriffe und Semantik zu verstehen, insbesondere:

- wie BUYs gezählt wurden,
- wie Unique Buyers gezählt wurden,
- wie Observation Windows funktionierten,
- wie `stop_net_multiple=0` verstanden wurde,
- welche Events nicht in die Filterzählung gehörten,
- historische Strategy-/Backtest-Semantik.

Die ZIPs dürfen diese Anweisung **nicht eigenmächtig überschreiben**.

Wenn eine historische Datei einem hier ausdrücklich festgelegten Strategy-B-Parameter widerspricht:

1. nichts stillschweigend ändern,
2. den Konflikt dokumentieren,
3. Strategy B trotzdem gemäß dieser Anweisung implementieren,
4. den historischen Unterschied separat nennen.

---

# 1. GRUNDZIEL

Der aktuelle DarkBot soll anschließend zwei unterschiedliche Strategy-Policies auf derselben modernen Engine ausführen können:

```text
STRATEGY A
= bestehende aktuelle DarkBot-Strategie

STRATEGY B
= historische 1s / 5 BUY / 5 Unique Buyer Strategie
```

Wir bauen **keinen zweiten Bot**.

Wir bauen:

```text
                    MODERNER DARKBOT
                         │
              Market / Feed / State
                         │
             ┌───────────┴───────────┐
             │                       │
        STRATEGY A              STRATEGY B
             │                       │
             └───────────┬───────────┘
                         │
                gleiche Execution
                         │
             gleiche Fees / Quotes
                         │
                 gleiche SQLite
                         │
                   gleicher Replay
```

Ziel ist später ein echter A/B-Test:

```text
gleiche Mainnet-Daten
gleiche Kosten
gleiche Execution
gleiche Feeds
gleiche Latenzannahme
gleiche Portfolio-Bedingungen

nur Strategy A vs. Strategy B unterschiedlich
```

---

# 2. STRATEGY A DARF NICHT VERÄNDERT WERDEN

Die bestehende Strategy A bleibt funktional exakt wie sie aktuell ist.

Aktueller Kern:

```text
observation_ms = 0
filters = []
entry_trigger = FIRST_ACTIONABLE_CREATE

target_net_multiple = 1.5

ordinary_stop_enabled = false

creator_sell_exit_enabled = true

strategy_timeout_ms = 590000
```

Strategy A kann einen eindeutigen Identifier erhalten, z. B.:

```text
FAST_CREATOR_HOLD_V1
```

Aber:

**Keine Verhaltensänderung an Strategy A.**

Alle bestehenden Tests müssen nach dem Refactor weiterhin bestehen.

---

# 3. STRATEGY B MUSS EINE EIGENE STRATEGY-KLASSE / DATEI SEIN

Strategy B darf nicht einfach als zusätzlicher großer `if/elif`-Block in Strategy A eingebaut werden.

Gewünscht ist eine klare Trennung.

Beispiel:

```text
darkbot/
    strategies/
        base.py
        strategy_a_fast_creator.py
        strategy_b_legacy_5buy.py
```

Die exakten Namen können an das Projekt angepasst werden.

Aber zwingend:

```text
Strategy A = eigene Klasse / eigenes Modul

Strategy B = eigene Klasse / eigenes Modul
```

Gemeinsame Logik darf abstrahiert werden.

Nicht duplizieren:

- QuoteEngine
- Fees
- Execution
- PaperExecutionAdapter
- MarketReducer
- Decoder
- Replay
- Storage
- Reporting

---

# 4. KEINE ALTE INFRASTRUKTUR PORTIEREN

Aus `backend.zip` und `lean_v1_frozen(1).zip` darf ausdrücklich NICHT übernommen werden:

- alte Feed-Architektur
- alte WebSocket-Implementierung
- alte Datenbankstruktur
- alte SQLite-Writer
- alte Execution Engine
- alte Fee-Formeln
- alte Quote-Berechnung
- alte Slippage-Implementierung
- alte Portfolio-Engine
- alte Replay-Engine
- alte Async-/Thread-Struktur
- alte Decoder
- alte Pump-Programmlogik
- alte Klassenarchitektur

Wir übernehmen ausschließlich:

> **die Strategy-Policy**

und setzen sie auf die heutige Engine.

---

# 5. STRATEGY B – VERBINDLICHE POLICY

Strategy-ID:

```text
LEGACY_5BUY_5UNIQUE_V1
```

Die Strategy-B-Regeln sind:

```text
Trade Size              = 10 USD

Observation Window      = 1000 ms

Minimum BUY Events      = 5

Minimum Unique Buyers   = 5

Target Net Multiple     = 1.05

Ordinary Stop           = AUS

Stop Net Multiple       = 0

Maximum Hold Time       = 60000 ms

Creator-Sell Exit       = AUS
```

Diese Werte sind für diese Strategy-B-Version verbindlich.

---

# 6. ENTRY-ABLAUF STRATEGY B

Nach einem gültigen, actionable Pump CREATE:

```text
CREATE
  ↓
T0
  ↓
1000 ms beobachten
  ↓
BUY Events zählen
  ↓
Unique Buyer zählen
  ↓
nach exakt 1000 ms entscheiden
```

Entry genau dann, wenn:

```text
buy_count >= 5

UND

unique_buyers >= 5
```

Andernfalls:

```text
SKIP
```

---

# 7. WICHTIG: CREATE-/INITIAL-BUY NICHT ALS FILTER-BUY ZÄHLEN

Der initiale Creator-BUY bzw. der im CREATE-Kontext enthaltene Initial-Buy darf:

```text
NICHT zu buy_count zählen
```

und:

```text
NICHT zu unique_buyers zählen
```

Für die 5/5-Bedingung zählen nur die nach dem relevanten CREATE beobachteten gültigen BUY-Events innerhalb des Observation Windows.

Beispiel:

```text
CREATE + Creator Initial Buy
→ zählt 0 / 0

Buyer A BUY
→ 1 BUY / 1 Unique

Buyer B BUY
→ 2 / 2

Buyer C BUY
→ 3 / 3

Buyer D BUY
→ 4 / 4

Buyer E BUY
→ 5 / 5

=> QUALIFIED
```

---

# 8. UNIQUE-BUYER-SEMANITIK

Unique Buyer wird über den tatsächlichen Actor/PublicKey des gültigen BUY-Events bestimmt.

Ein Wallet zählt innerhalb des Observation Windows nur einmal.

Beispiel:

```text
Wallet A BUY
Wallet A BUY
Wallet A BUY
Wallet A BUY
Wallet A BUY
```

Ergebnis:

```text
buy_count = 5
unique_buyers = 1
```

Damit:

```text
KEIN ENTRY
```

---

# 9. DEDUP MUSS GELTEN

Ein identisches Chain-Event darf über mehrere Feeds niemals mehrfach gezählt werden.

Beispielsweise:

```text
Helius sieht BUY X
Public RPC sieht BUY X
```

muss bleiben:

```text
buy_count += 1
```

und nicht:

```text
buy_count += 2
```

Die bereits vorhandene kanonische Event-/Dedup-Infrastruktur des modernen DarkBot ist dafür zu verwenden.

Strategy B darf keine eigene Dedup-Engine implementieren.

---

# 10. EXAKTES OBSERVATION WINDOW

Observation:

```text
1000 ms
```

Start:

```text
actionable CREATE / für die Strategy definierter T0
```

Ende:

```text
T0 + 1000 ms
```

Nur Events innerhalb dieses Fensters zählen.

Ein BUY, der nach Abschluss des Fensters eintrifft, darf die bereits getroffene Entscheidung nicht rückwirkend verändern.

Der Timer muss über die existierende Clock-/Scheduler-Abstraktion laufen.

Damit müssen PAPER und Replay dieselbe Entscheidung erzeugen.

---

# 11. KEINE EXTERNEN ABFRAGEN FÜR DEN FILTER

Während der 1000 ms darf Strategy B ausschließlich vorhandenen RAM-State verwenden.

Benötigt werden im Kern nur:

```text
buy_count
unique_buyers
observation_timer
```

Keine:

- RPC-Abfrage
- REST-Abfrage
- Metadata-Abfrage
- HTTP-Abfrage
- DB-Leseoperation
- synchroner Report
- externe API

für die Strategy-Entscheidung.

---

# 12. NACH QUALIFIKATION ÜBERNIMMT DIE MODERNE EXECUTION ENGINE

Wenn 5/5 erreicht und das Observation Window abgeschlossen ist:

```text
Strategy B
→ ENTRY DECISION
```

Ab diesem Punkt gelten ausschließlich die aktuellen modernen DarkBot-Regeln.

Also:

- aktuelle QuoteEngine
- aktuelle Pump-Gebühren
- Creator Fees
- Network Fees
- Priority Fees
- Rent
- Slippage
- Token Accounts
- SPL
- Token-2022
- Pump V2
- Venue Guard
- Coverage
- Quote Freshness
- Execution Attempts
- PAPER Fill Evidence

Strategy B darf keine historischen alten Fill-Formeln verwenden.

---

# 13. POSITIONSGRÖSSE = 10 USD

Strategy B verwendet:

```text
10 USD pro Trade
```

Das ist ein wesentlicher Bestandteil dieser Strategy-Version.

Nicht einfach behaupten:

```text
98.370.000 Lamports = 10 USD
```

Der moderne DarkBot muss `$10` eindeutig und reproduzierbar in das benötigte SOL-Budget übersetzen.

Anforderung:

```text
trade_size_usd = 10
```

muss versioniert und reproduzierbar sein.

Wenn dafür eine SOL/USD-Evidenz benötigt wird:

- außerhalb des kritischen Hot Paths beschaffen,
- Quelle/Evidenz persistieren,
- Replay reproduzierbar machen.

Keine versteckte konstante USD→SOL-Umrechnung.

---

# 14. MAXIMALER TRADE-VERLUST

Der einzelne Trade riskiert den Einsatz:

```text
10 USD
```

Der Trade darf diesen Einsatz vollständig verlieren.

Es existiert ausdrücklich:

```text
KEIN 0.90x STOP
```

und kein anderer normaler enger Kurs-Stop.

Strategy-B-Konfiguration:

```text
ordinary_stop_enabled = false
stop_net_multiple = 0
```

Wenn der Coin:

```text
-20 %
-50 %
-90 %
```

steht, führt das alleine zu keinem normalen Stop-Exit.

Der maximale wirtschaftliche Trade-Verlust kann bis zum eingesetzten Kapital gehen.

Dieser Verlust muss korrekt in:

- Net PnL
- Average Loss
- Expectancy
- Drawdown

eingehen.

---

# 15. TARGET = 1.05x NETTO

Target:

```text
target_net_multiple = 1.05
```

Wichtig:

Das ist ausdrücklich kein einfacher Chartpreis von +5 %.

Entscheidend ist der moderne wirtschaftliche Netto-Liquidationswert.

Sinngemäß:

```text
NET LIQUIDATION VALUE
>=
1.05 × vollständige wirtschaftliche Entry-Kosten
```

Unter Berücksichtigung der aktuellen Engine:

- Pump Fees
- Creator Fee
- Network Fee
- Priority Fee
- Rent-/Account-Semantik
- Slippage
- Curve Impact
- weitere tatsächlich modellierte Kosten

Erst dann:

```text
TARGET EXIT
```

---

# 16. MAX HOLD = 60 SEKUNDEN

Strategy B verwendet:

```text
strategy_timeout_ms = 60000
```

also:

```text
maximale Haltedauer = 60 Sekunden
```

Für die neue Implementierung wird diese Haltedauer ab:

```text
SUCCESSFUL_ENTRY_FILL
```

gemessen.

Damit bedeutet:

```text
BUY erfolgreich gefüllt
→ maximal 60 Sekunden Position halten
```

Wenn bis dahin das 1.05x-Netto-Target nicht erreicht wurde:

```text
TIMEOUT EXIT
```

unter den gültigen Sicherheits-/Execution-Regeln der modernen Engine.

---

# 17. KEIN CREATOR-SELL-EXIT IN STRATEGY B

Sehr wichtig.

Strategy A besitzt:

```text
creator_sell_exit_enabled = true
```

Strategy B nicht.

Für Strategy B gilt:

```text
creator_sell_exit_enabled = false
```

Ein Creator-Sell darf bei Strategy B nicht automatisch einen Exit auslösen.

Strategy B wird nicht mit Strategy A vermischt.

Strategy-B-Ausgänge sind für diese Version:

```text
TARGET
TIMEOUT
```

sowie zwingende gemeinsame Engine-/Safety-Zustände, falls der Markt technisch nicht mehr sicher handelbar ist.

Aber:

```text
CREATOR_SELL
```

ist **kein Strategy-B-Exit-Trigger**.

---

# 18. LEGACY-LATENZREFERENZ = 400 ms / 400 ms

Der historische positive Test wurde mit folgender Ausführungsannahme ausgewertet:

```text
Entry Execution Delay = 400 ms
Exit Execution Delay  = 400 ms
```

Diese Werte sind **nicht Bestandteil der Strategy-B-Klasse**.

Sie gehören in das bestehende gemeinsame Latency-Modell.

Für die historische Referenz muss ein Szenario existieren:

```text
LEGACY_REFERENCE_LATENCY

entry_delay_ms = 400
exit_delay_ms  = 400
```

Damit kann die neue Engine zunächst die historische Strategie unter derselben Latenzannahme testen.

Danach zusätzlich:

```text
250 / 250 ms
100 / 100 ms
25 / 25 ms
5 / 5 ms
0 / 0 ms
reale gemessene Verteilung
```

testen.

So wird getrennt:

```text
Strategy-Effekt
vs.
Latenz-Effekt
```

---

# 19. DIRECT PUMP EXECUTION

Strategy B soll auf dem aktuell unterstützten direkten Pump-Bonding-Curve-Pfad der modernen Engine ausgeführt werden.

Kein DEX-Aggregator als Ersatz.

Kein historischer Executor.

Die heutige Pump-/V2-/Token-2022-Execution-Infrastruktur bleibt maßgeblich.

---

# 20. PARALLELE BEOBACHTUNG IST ZWINGEND

Mehrere neue Mints müssen gleichzeitig beobachtet werden können.

Beispiel:

```text
Coin A Observation läuft
Coin B CREATE kommt
Coin B Observation startet ebenfalls
Coin C CREATE kommt
Coin C Observation startet ebenfalls
```

Coin A darf Coin B/C nicht blockieren.

Strategy B darf niemals als serieller Prozess implementiert werden:

```text
Coin A komplett beenden
→ erst danach Coin B anschauen
```

Das wäre falsch.

---

# 21. PARALLELE POSITIONEN MÜSSEN UNTERSTÜTZT WERDEN

Die historische Strategie/Hochrechnung setzt voraus, dass mehrere Trades parallel verwaltet werden können.

Deshalb darf Strategy B nicht intern:

```text
max_open_positions = 1
```

festlegen.

Portfolio-Limit bleibt gemeinsamer Parameter der modernen Engine.

Mindestens Research-Szenarien:

```text
max_open_positions = 1
max_open_positions = 10
```

Die Strategy-Klasse selbst darf diesen Wert nicht hardcoden.

---

# 22. STRATEGY A UND B MÜSSEN DIESELBE ENGINE BENUTZEN

Beide müssen exakt dieselben Komponenten nutzen:

```text
Capture
Helius
existing_rpc
PumpPortal optional
Normalizer
MarketReducer
Canonical Event Identity
Dedup
Coverage
Venue Guard
Fee Cache
QuoteEngine
Execution Engine
PaperExecutionAdapter
Portfolio
SQLite Recorder
ReplayClock
Reporter
Research
```

Es darf nicht entstehen:

```text
old_engine_for_B
new_engine_for_A
```

Das wäre ausdrücklich falsch.

---

# 23. CONFIG-SELEKTION

Strategy muss eindeutig über versionierte Config auswählbar sein.

Beispiel A:

```json
{
  "strategy": {
    "strategy_id": "FAST_CREATOR_HOLD_V1"
  }
}
```

Beispiel B:

```json
{
  "strategy": {
    "strategy_id": "LEGACY_5BUY_5UNIQUE_V1",
    "trade_size_usd": "10",
    "observation_ms": 1000,
    "min_buy_count": 5,
    "min_unique_buyers": 5,
    "target_net_multiple": "1.05",
    "ordinary_stop_enabled": false,
    "stop_net_multiple": "0",
    "creator_sell_exit_enabled": false,
    "strategy_timeout_ms": 60000
  }
}
```

Keine versteckten Strategy-Parameter in Python.

---

# 24. A/B-REPLAY MUSS MÖGLICH SEIN

Ein und derselbe aufgezeichnete Datensatz muss mindestens so getestet werden können:

```text
DATASET X
│
├── Replay mit Strategy A
│
└── Replay mit Strategy B
```

Dabei identisch halten:

- Dataset
- Raw Events
- Feed-Timestamps
- Decoder
- Fee Snapshots
- Kostenmodell
- QuoteEngine
- Slippage
- Portfolio-Limit
- Latency Scenario
- Token-2022-Regeln

Nur:

```text
Strategy Policy
```

ändert sich.

---

# 25. HISTORISCHE POSITIVE REFERENZ

Folgender historische Wert wird als:

```text
LEGACY_POSITIVE_REFERENCE
```

geführt:

```text
Trade Size:
$10

Observation:
1 Sekunde

Filter:
>= 5 BUYs
>= 5 unterschiedliche Käufer

Target:
1.05x NETTO

Normaler Stop:
keiner

Trade-Risiko:
bis zum vollständigen $10-Einsatz

Max Hold:
60 Sekunden

Entry Execution Delay:
400 ms

Exit Execution Delay:
400 ms

historischer Lauf:
ca. 897,65 Sekunden / 14:57 Minuten

qualifizierte Trades:
21

Netto-PnL:
+$9,55

durchschnittlich:
ca. +$0,455 pro Trade
```

Diese historische Referenz ist:

**kein Beweis heutiger Profitabilität.**

Sie dient als Referenz dafür, welche Strategy Policy wir mit der heutigen Engine neu testen wollen.

---

# 26. STRATEGY-B TESTS – ZWINGEND

## B01

```text
4 BUYs
4 Unique Buyers
```

Erwartung:

```text
SKIP
```

---

## B02

```text
5 BUYs
4 Unique Buyers
```

Erwartung:

```text
SKIP
```

---

## B03

```text
5 BUYs
5 Unique Buyers
```

Erwartung:

```text
ENTRY
```

---

## B04

```text
10 BUYs
alle vom selben Wallet
```

Erwartung:

```text
SKIP
```

---

## B05

CREATE-/Initial-Buy des Creators:

```text
darf buy_count nicht erhöhen
darf unique_buyers nicht erhöhen
```

---

## B06

Dasselbe BUY-Event über zwei Feeds:

```text
muss nur einmal gezählt werden
```

---

## B07

BUY nach Ende des Observation Windows:

```text
darf vorherige Entscheidung nicht ändern
```

---

## B08

PAPER und Replay:

```text
müssen auf identischen Events
identische Strategy-B-Decision erzeugen
```

---

## B09

Net Liquidation Value:

```text
1.049x
```

Erwartung:

```text
HOLD
```

---

## B10

Net Liquidation Value:

```text
>= 1.05x
```

Erwartung:

```text
TARGET EXIT
```

---

## B11

Position fällt auf:

```text
0.90x
0.50x
0.10x
```

Erwartung:

```text
KEIN gewöhnlicher Stop
```

---

## B12

Creator verkauft:

Erwartung für Strategy B:

```text
KEIN CREATOR_SELL Strategy Exit
```

---

## B13

60 Sekunden nach erfolgreichem Entry Fill ohne Target:

```text
TIMEOUT EXIT
```

---

## B14

Mehrere Mints:

```text
gleichzeitig beobachten
```

muss funktionieren.

---

## B15

Position A ist offen.

Mint B erscheint.

Erwartung:

```text
Observation von B läuft trotzdem.
```

---

## B16

Bei Portfolio-Limit >1:

```text
mehrere Strategy-B-Positionen parallel möglich.
```

---

## B17

Strategy-A-Regression:

Alle bestehenden Strategy-A-Tests weiterhin PASS.

---

## B18

Strategy Selection:

A und B müssen unterschiedliche:

```text
strategy_id
strategy_config_hash
```

erzeugen.

---

# 27. REPORTING

Jeder Run muss eindeutig speichern:

```text
strategy_id
strategy_config_hash
```

Für Strategy B zusätzlich mindestens:

```text
observation_start
observation_end
buy_count
unique_buyers
qualified yes/no
```

A/B-Auswertung mindestens:

- CREATEs
- qualifizierte CREATEs
- Entries
- Fills
- geschlossene Trades
- Winrate
- Netto-PnL
- PnL pro Trade
- Average Winner
- Average Loser
- EV pro Trade
- Profit Factor
- Max Drawdown
- Kapitalbindung
- Haltedauer
- Target Exits
- Timeout Exits
- UNKNOWN
- UNRESOLVED
- Fees
- Rent
- Slippage
- Entry-Latenz
- Exit-Latenz

---

# 28. P0 `EXIT_LATCHED_DECODE_GAP` BLEIBT SEPARATER BUG

Im aktuellen System existiert bereits der reale P0-Fall:

```text
CREATOR_SELL
→ Exit korrekt gelatcht
→ EXIT_LATCHED_DECODE_GAP
→ kein SELL-Attempt
```

Dieser gemeinsame Engine-/Liveness-Bug muss separat behoben werden.

Strategy B darf ihn:

- nicht kaschieren,
- nicht umgehen,
- nicht durch unsichere Fills „lösen“.

Die gemeinsame neue Execution-/Coverage-Engine bleibt sicherheitsrelevant.

---

# 29. IMPLEMENTIERUNGSREIHENFOLGE

Arbeite in dieser Reihenfolge:

1. Aktuellen DarkBot vollständig lesen.
2. Aktuelle SPEC vollständig lesen.
3. `backend.zip` und `lean_v1_frozen(1).zip` nur als historische Strategy-Semantik-Referenz lesen.
4. Keine alte Infrastruktur übernehmen.
5. Bestehende Strategy-A-Logik identifizieren.
6. Minimale gemeinsame Strategy-Schnittstelle definieren.
7. Strategy A in eigene Klasse/Datei verschieben.
8. Verhalten von Strategy A unverändert lassen.
9. Gesamtsuite ausführen.
10. Strategy B als eigene Klasse/Datei implementieren.
11. Strategy-B-Config implementieren.
12. Strategy-B-Tests B01–B18 implementieren.
13. Gesamtsuite ausführen.
14. PAPER Strategy Selection testen.
15. Replay Strategy Selection testen.
16. Gleichen Dataset-Run A/B ermöglichen.
17. Reporting um Strategy-ID/Hash und Strategy-B-Filterdaten erweitern.
18. Hot-Path-/Architecture-Review.
19. Keine LIVE-Ausführung aktivieren.

---

# 30. WAS NICHT GETAN WERDEN DARF

Nicht:

```text
Strategy A durch B ersetzen
```

Nicht:

```text
A und B zu einer Mischstrategie machen
```

Nicht:

```text
Creator-Sell-Exit aus A in B übernehmen
```

Nicht:

```text
0.90x Stop einbauen
```

Nicht:

```text
Initial Creator BUY zu den 5 BUYs zählen
```

Nicht:

```text
gleiche Wallet fünfmal als fünf Unique Buyers zählen
```

Nicht:

```text
alte Fee-/Execution-Engine übernehmen
```

Nicht:

```text
alte SQLite-Architektur übernehmen
```

Nicht:

```text
alte Feed-Architektur übernehmen
```

Nicht:

```text
400 ms direkt in Strategy B hardcoden
```

Nicht:

```text
98.37m Lamports einfach mit $10 gleichsetzen
```

Nicht:

```text
max_open_positions=1 in Strategy B hardcoden
```

Nicht:

```text
LIVE Broadcast ermöglichen
```

---

# 31. DEFINITION OF DONE

Die Aufgabe ist erst fertig, wenn:

- Strategy A unverändert vorhanden ist.
- Strategy B separat vorhanden ist.
- beide eigene Strategy-Klassen/Module haben.
- beide dieselbe moderne Engine verwenden.
- Strategy B exakt 1000 ms beobachtet.
- Initial Creator Buy nicht gezählt wird.
- Strategy B mindestens 5 BUYs verlangt.
- Strategy B mindestens 5 unterschiedliche Buyer verlangt.
- Deduplizierung korrekt bleibt.
- Strategy B mit $10 Trade Size arbeitet.
- Target exakt 1.05x NETTO ist.
- kein normaler Stop vorhanden ist.
- `stop_net_multiple=0` korrekt behandelt wird.
- Creator-Sell Strategy Exit bei B deaktiviert ist.
- Max Hold 60 s ab erfolgreichem Entry Fill funktioniert.
- Legacy-Referenzlatenz 400/400 ms als gemeinsames Latency-Szenario verfügbar ist.
- parallel mehrere Mints beobachtet werden.
- mehrere Positionen bei entsprechendem Portfolio-Limit möglich sind.
- Config A/B eindeutig auswählen kann.
- PAPER und Replay beide Strategien unterstützen.
- derselbe Datensatz mit A und B vergleichbar ist.
- Strategy-ID und Config-Hash gespeichert werden.
- bestehende Strategy-A-Tests weiterhin PASS sind.
- alle neuen Strategy-B-Tests PASS sind.
- keine alte Infrastruktur portiert wurde.
- der moderne Hot Path nicht unnötig verschlechtert wurde.
- keine LIVE-Execution aktiviert wurde.

# ENDZIEL

Wir bauen **nicht den alten Bot wieder auf**.

Wir übernehmen nur die alte Strategy-Policy:

```text
$10
+
1 s Observation
+
>=5 BUYs
+
>=5 Unique Buyers
+
1.05x Netto-Target
+
kein normaler Stop
+
voller $10-Einsatz darf verloren gehen
+
60 s Max Hold
```

und führen sie auf der heutigen DarkBot-Infrastruktur aus.

Danach können wir Strategy A und Strategy B unter denselben technischen Bedingungen fair gegeneinander testen.