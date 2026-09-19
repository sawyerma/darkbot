# DarkBot Strategy B – Source of Truth

**Status:** Verbindliche Implementierungsanweisung  
**Base Commit:** `367c97cae79bc6d3517917786fdc0b00b9b812fb` (`R8_2c`)

## 1. Priorität der Quellen

Für diese Aufgabe gilt folgende Reihenfolge:

1. **Diese Datei ist die verbindliche Source of Truth für Strategy B.**
2. Der aktuelle DarkBot-Stand `R8_2c` und die aktuelle SPEC definieren die technische Architektur.
3. `lean_v1_frozen(1).zip` darf ausschließlich **READ-ONLY** als historische Semantikreferenz verwendet werden.
4. `backend.zip` ist **NICHT Bestandteil dieses Auftrags** und darf nicht verwendet werden.

Wenn `lean_v1_frozen(1).zip` einem hier ausdrücklich definierten Strategy-B-Parameter widerspricht, gilt **diese Datei**. Den Unterschied nur dokumentieren, nicht Strategy B verändern.

## 2. Ziel

Der aktuelle DarkBot soll zwei getrennte Strategy-Policies auf derselben modernen Engine unterstützen:

- **Strategy A:** bestehende aktuelle DarkBot-Strategie
- **Strategy B:** `LEGACY_5BUY_5UNIQUE_V1`

Keine zweite Engine und kein alter Bot.

Beide Strategien verwenden dieselbe aktuelle Infrastruktur:

- Capture / Feeds
- Helius / existing_rpc / optionale Feeds
- Normalizer
- MarketReducer
- Canonical Dedup
- Coverage
- Venue Guard
- Token-2022 / Pump V2
- Fee Snapshots
- QuoteEngine
- Slippage
- Rent
- Priority Fees
- PAPER Execution
- Replay
- SQLite
- Reporting
- Latency Measurement

Nur die Strategy-Policy unterscheidet sich.

## 3. Strategy A

Strategy A bleibt funktional unverändert gegenüber `R8_2c`.

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

Die R8_2c-P0-Fixes für Exit-Liveness/Decode-Gap/Replay müssen vollständig erhalten bleiben.

Strategy A darf durch den Multi-Strategy-Umbau nicht funktional verändert werden.

## 4. Architektur der Strategien

Strategy A und Strategy B müssen getrennte Strategy-Klassen/Module erhalten.

Beispiel:

```text
darkbot/
  strategies/
    base.py
    strategy_a_fast_creator.py
    strategy_b_legacy_5buy.py
```

Die genaue Dateistruktur darf sinnvoll an den bestehenden Code angepasst werden.

Zwingend:

- Strategy A eigene Klasse / eigenes Modul
- Strategy B eigene Klasse / eigenes Modul
- kleine gemeinsame Strategy-Schnittstelle
- keine duplizierte Market-, Execution-, Fee-, Replay- oder Storage-Engine

Keine große `if strategy == ...`-Mischlogik quer durch `paper.py`.

## 5. Strategy B – verbindliche Policy

Strategy-ID:

```text
LEGACY_5BUY_5UNIQUE_V1
```

Verbindliche Parameter:

```text
trade_size_usd          = 10
observation_ms          = 1000
min_buy_count           = 5
min_unique_buyers       = 5
target_net_multiple     = 1.05
ordinary_stop_enabled   = false
stop_net_multiple       = 0
strategy_timeout_ms     = 60000
creator_sell_exit       = false
timeout_clock_origin    = SUCCESSFUL_ENTRY_FILL
```

Diese Werte werden **nicht aus alten Dateien abgeleitet**, sondern sind für diese Strategy-Version ausdrücklich vorgegeben.

## 6. $10-Sizing – exakte Semantik

Für Strategy B gilt:

```text
$10 Swap-Budget; Network Fees / Priority Fees / Rent zusätzlich.
```

Das bedeutet:

```text
trade_size_usd = 10
→ $10 werden zum Entry-Zeitpunkt reproduzierbar in SOL/Lamports umgerechnet
→ dieser Betrag ist das Swap-/Curve-Budget
```

Innerhalb des Swap-Budgets wirken die für den Swap geltenden Pump-/Creator-Gebühren entsprechend dem aktuellen Quote-/Execution-Modell.

Zusätzlich zum $10-Swap-Budget werden reale Infrastrukturkosten verbucht, insbesondere:

- Solana Base Fee
- Priority Fee
- ggf. Account-/ATA-Rent

Nicht zulässig:

```text
$10 gesamtes Entry-Cash-Budget inklusive Network Fees und Rent
```

Nicht einfach `98.370.000 Lamports = $10` hardcoden.

Die USD→SOL-Evidenz muss versioniert und reproduzierbar sein. Keine erfundene feste Umrechnung.

## 7. Entry-Logik

Nach einem gültigen actionable Pump CREATE:

```text
T0 = Strategy-B Observation Start
```

Dann exakt:

```text
1000 ms beobachten
```

Währenddessen pro Mint ausschließlich im RAM zählen:

```text
buy_count
unique_buyers
```

Nach Ablauf der 1000 ms:

```text
if buy_count >= 5 and unique_buyers >= 5:
    ENTRY
else:
    SKIP
```

Die Entscheidung erfolgt am Ende des Observation Windows.

## 8. Initial Creator Buy zählt NICHT

Der im CREATE-Kontext enthaltene Initial Buy des Creators zählt ausdrücklich nicht:

```text
nicht zu buy_count
nicht zu unique_buyers
```

Beispiel:

```text
CREATE + Creator Initial Buy -> 0 BUY / 0 Unique
Buyer A BUY                -> 1 / 1
Buyer B BUY                -> 2 / 2
Buyer C BUY                -> 3 / 3
Buyer D BUY                -> 4 / 4
Buyer E BUY                -> 5 / 5
=> QUALIFIED
```

## 9. Unique Buyers

Unique Buyer wird über den tatsächlichen Actor/PublicKey eines gültigen BUY-Events bestimmt.

Ein Wallet zählt im Observation Window nur einmal.

Beispiel:

```text
Wallet A kauft 5-mal
buy_count = 5
unique_buyers = 1
=> KEIN ENTRY
```

## 10. Dedup

Dasselbe Chain-Event darf über mehrere Feeds nur einmal gezählt werden.

Beispiel:

```text
Helius sieht BUY X
existing_rpc sieht BUY X
=> buy_count +1, nicht +2
```

Die bestehende kanonische Dedup-Infrastruktur verwenden. Keine eigene Strategy-B-Dedup-Engine.

## 11. Observation Window

Nur BUY-Events innerhalb des definierten 1000-ms-Fensters zählen.

Ein Event nach Observation-Ende darf eine bereits getroffene Entscheidung nicht rückwirkend verändern.

Timer ausschließlich über die bestehende Clock-/Scheduler-Abstraktion, damit PAPER und Replay deterministisch identisch arbeiten.

## 12. Kein externer I/O für den Strategy-B-Filter

Während des Observation Windows keine:

- RPC-Abfragen
- REST-Abfragen
- Metadata-Abfragen
- HTTP-Abfragen
- DB-Reads
- synchronen Reports

Für die Filterentscheidung genügen RAM-State und Timer.

## 13. Moderne Execution bleibt maßgeblich

Nach der ENTRY-Decision übernimmt ausschließlich die aktuelle DarkBot-Engine:

- QuoteEngine
- aktuelle Pump Fees
- Creator Fees
- Network Fees
- Priority Fees
- Rent
- Slippage
- SPL / Token-2022
- Pump V2
- Venue Guard
- Coverage
- Quote Freshness
- PAPER Fill Model
- Execution Attempts

Keine historischen Legacy-Fill-/Fee-/Execution-Formeln übernehmen.

## 14. Target

Strategy B:

```text
target_net_multiple = 1.05
```

Das bedeutet **NETTO**.

Nicht einfacher Chartpreis +5 %.

Sinngemäß:

```text
Net Liquidation Value >= 1.05 × vollständige wirtschaftliche Entry-Kosten
```

unter dem aktuellen modernen Kostenmodell.

## 15. Stop / Risiko

Strategy B hat keinen gewöhnlichen engen Stop:

```text
ordinary_stop_enabled = false
stop_net_multiple = 0
```

Kein 0.90x-Stop.

Der $10-Swap-Einsatz darf vollständig verloren gehen. Zusätzlich angefallene reale Execution-/Infrastrukturkosten werden separat korrekt verbucht.

Ein Kursrückgang auf 0.90x / 0.50x / 0.10x löst allein keinen normalen Stop aus.

## 16. Timeout

Strategy B:

```text
strategy_timeout_ms = 60000
timeout_clock_origin = SUCCESSFUL_ENTRY_FILL
```

Also:

```text
erfolgreicher Entry Fill
→ maximal 60 Sekunden halten
→ wenn Target nicht erreicht: TIMEOUT EXIT
```

## 17. Creator Sell

Strategy B besitzt **keinen Creator-Sell-Exit**:

```text
creator_sell_exit = false
```

Ein Creator-Sell darf Strategy B nicht automatisch schließen.

Strategy B darf nicht mit Strategy A vermischt werden.

Normale Strategy-B-Ausgänge:

```text
TARGET
TIMEOUT
```

Gemeinsame zwingende Engine-/Safety-Zustände bleiben natürlich gültig.

## 18. Latency

Die historische Referenz:

```text
entry_delay_ms = 400
exit_delay_ms = 400
```

ist **kein Bestandteil der Strategy-B-Klasse**.

Sie gehört in das gemeinsame Latency-Modell.

Mindestens als Research-Szenario:

```text
LEGACY_REFERENCE_LATENCY
entry_delay_ms = 400
exit_delay_ms = 400
```

Danach zusätzlich z. B.:

```text
250 / 250 ms
100 / 100 ms
25 / 25 ms
5 / 5 ms
0 / 0 ms
später reale gemessene Verteilung
```

## 19. Parallelität

Mehrere neue Mints müssen gleichzeitig beobachtet werden können.

Eine offene Position darf die Observation anderer Mints nicht verhindern.

Strategy B darf `max_open_positions=1` nicht hardcoden.

Portfolio-Limit bleibt gemeinsamer Research-/Portfolio-Parameter.

Mindestens testbar:

```text
max_open_positions = 1
max_open_positions = 10
```

## 20. Config-Auswahl

Strategy muss versioniert über Config auswählbar sein.

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
    "strategy_timeout_ms": 60000,
    "timeout_clock_origin": "SUCCESSFUL_ENTRY_FILL"
  }
}
```

Keine versteckten Strategy-Parameter in Python.

## 21. A/B-Replay

Ein identischer aufgezeichneter Dataset-Run muss mit beiden Strategien replaybar sein:

```text
Dataset X
├── Strategy A
└── Strategy B
```

Identisch halten:

- Raw Events
- Feed-Timestamps
- Decoder
- Fee Snapshots
- QuoteEngine
- Slippage
- Rent
- Portfolio-Limit
- Latency Scenario
- Token-2022-Regeln

Nur die Strategy-Policy unterscheidet sich.

## 22. Historische positive Referenz

Als historische Referenz wird dokumentiert:

```text
Trade Size:          $10 Swap-Budget
Observation:         1 Sekunde
Filter:              >=5 BUYs
                     >=5 unterschiedliche Käufer
Target:              1.05x netto
Normaler Stop:       keiner
Max Hold:            60 Sekunden
Entry Delay:         400 ms
Exit Delay:          400 ms
Laufdauer:           ca. 897,65 s / 14:57 min
qualifizierte Trades: 21
Netto-PnL:           +$9,55
Ø Ergebnis:          ca. +$0,455/Trade
```

Diese historische Referenz ist **kein reproduzierter aktueller Profitnachweis**, weil die ursprüngliche Rohaufnahme dieses positiven Laufs nicht vorliegt.

Der neue A/B-Test verwendet deshalb dieselben neuen Mainnet-Aufnahmen und identischen technischen Bedingungen für Strategy A und B.

## 23. Pflicht-Tests Strategy B

### B01
4 BUY / 4 Unique → SKIP

### B02
5 BUY / 4 Unique → SKIP

### B03
5 BUY / 5 Unique → ENTRY

### B04
10 BUY desselben Wallets → SKIP

### B05
Initial Creator Buy zählt weder zu BUY noch Unique Buyer.

### B06
Dasselbe Chain-Event über zwei Feeds zählt einmal.

### B07
BUY nach Observation-Ende ändert Entscheidung nicht rückwirkend.

### B08
PAPER und Replay erzeugen bei identischen Events dieselbe Strategy-B-Decision.

### B09
NLV = 1.049x → HOLD

### B10
NLV >= 1.05x → TARGET EXIT

### B11
NLV 0.90 / 0.50 / 0.10 → kein normaler Stop

### B12
Creator verkauft → kein Strategy-B Creator-Sell-Exit

### B13
60 s nach erfolgreichem Entry Fill ohne Target → TIMEOUT EXIT

### B14
Mehrere Mints gleichzeitig beobachten.

### B15
Offene Position A blockiert Observation von Mint B nicht.

### B16
Bei Portfolio-Limit >1 mehrere Positionen parallel möglich.

### B17
Alle bestehenden Strategy-A-Tests bleiben PASS.

### B18
A und B erzeugen getrennte `strategy_id` und `strategy_config_hash`.

## 24. Reporting

Jeder Run speichert mindestens:

```text
strategy_id
strategy_config_hash
```

Strategy B zusätzlich:

```text
observation_start
observation_end
buy_count
unique_buyers
qualified
```

A/B-Auswertung mindestens:

- CREATEs
- qualifizierte Setups
- Entries
- Entry Fills
- Exit Fills
- geschlossene Trades
- Winrate
- Net PnL
- PnL/Trade
- Average Winner
- Average Loser
- EV/Trade
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
- Entry-/Exit-Latenzen

## 25. Bestehender P0-Fix

Der R8_2c-Fix für `EXIT_LATCHED_DECODE_GAP` und zugehörige Replay-/Recovery-Korrekturen bleiben Teil der gemeinsamen Engine.

Der Strategy-Umbau darf diese Fixes weder entfernen noch umgehen.

## 26. Nicht erlaubt

Nicht:

- Strategy A durch B ersetzen
- A und B vermischen
- `backend.zip` verwenden
- alte Infrastruktur portieren
- Creator-Sell-Exit aus A nach B übernehmen
- 0.90x Stop einbauen
- Initial Creator Buy zu den 5 BUYs zählen
- ein Wallet mehrfach als mehrere Unique Buyers zählen
- historische Fee-/Execution-Engine übernehmen
- historische SQLite-/Feed-/Replay-Architektur übernehmen
- 400 ms in Strategy B hardcoden
- `98.370.000 Lamports = $10` hardcoden
- `max_open_positions=1` in Strategy B hardcoden
- LIVE Broadcast aktivieren

## 27. Definition of Done

Fertig erst, wenn:

- Arbeitsbasis R8_2c ist
- Strategy A funktional unverändert bleibt
- Strategy B separat implementiert ist
- beide eigene Strategy-Klassen/Module besitzen
- beide dieselbe moderne DarkBot-Engine verwenden
- Strategy B exakt 1000 ms beobachtet
- Initial Creator Buy nicht gezählt wird
- >=5 BUY und >=5 Unique Buyer korrekt funktionieren
- Dedup korrekt bleibt
- $10 als Swap-Budget, Fees/Rent zusätzlich, korrekt umgesetzt ist
- Target 1.05x netto funktioniert
- kein normaler Stop existiert
- Creator-Sell bei Strategy B deaktiviert ist
- 60-s-Timeout ab erfolgreichem Entry Fill funktioniert
- Legacy-Referenzlatenz 400/400 ms als gemeinsames Szenario verfügbar ist
- parallele Mint-Beobachtung funktioniert
- mehrere Positionen bei geeignetem Portfolio-Limit möglich sind
- Config A/B eindeutig auswählt
- PAPER und Replay beide Strategien unterstützen
- identischer Dataset-Run A/B-vergleichbar ist
- Strategy-ID und Config-Hash gespeichert werden
- alle bisherigen Strategy-A-Tests PASS sind
- alle neuen Strategy-B-Tests PASS sind
- moderne Hot-Path-Architektur erhalten bleibt
- keine LIVE-Execution aktiviert wird

## Endziel

Nicht den alten Bot wiederherstellen.

Nur die historische Strategy-Policy:

```text
$10 Swap-Budget
+ 1 s Observation
+ >=5 BUYs
+ >=5 Unique Buyers
+ 1.05x Netto-Target
+ kein normaler Stop
+ voller $10-Swap-Einsatz darf verloren gehen
+ 60 s Max Hold
```

auf der heutigen R8_2c-DarkBot-Infrastruktur ausführen und danach Strategy A und Strategy B unter identischen technischen Bedingungen vergleichen.
