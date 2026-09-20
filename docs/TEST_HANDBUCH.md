# DarkBot – Test-Handbuch

Kurzreferenz für Entwicklung, Tests und PAPER-Läufe.

## 1. Repository aktualisieren

```bash
git pull
```

Aktuellen Commit prüfen:

```bash
git rev-parse --short HEAD
```

## 2. Vollständige Testsuite

```bash
python3 -m unittest discover -s tests -p 'test_*.py' -v
```

Aktueller Sollstand: **180 Tests, OK**.

Nach jeder Code-/Config-Änderung zuerst die komplette Suite ausführen.

## 3. Doctor / Readiness

```bash
python3 -m darkbot doctor --config config/strategy_a_creator_exit_usd_slots.json
```

Optional mit eigener DB:

```bash
python3 -m darkbot doctor \
  --config config/strategy_a_creator_exit_usd_slots.json \
  --db data/doctor.sqlite
```

## 4. Aktuelle Strategy A – Creator Exit / USD Slots

Config:

```text
config/strategy_a_creator_exit_usd_slots.json
```

Kernlogik:

- CREATE -> sofort BUY
- keine künstliche Observation-Zeit
- kein fixes Take-Profit
- Creator-Sell -> EXIT
- Positions-Timeout: 590 s
- 10 USD Gesamtbudget pro Slot
- 1 Slot = 10 USD Startkapital
- 10 Slots = 100 USD Startkapital
- PAPER only; LIVE-Senden gesperrt

### PAPER-Test starten

`--max-duration-minutes` ist **Pflicht** und beendet den gesamten PAPER-Lauf automatisch.

Beispiel: 10 Slots, 15 Minuten:

```bash
python3 -m darkbot paper \
  --config config/strategy_a_creator_exit_usd_slots.json \
  --db data/strategy_a_creator_exit_10slots_clean_15m.sqlite \
  --campaign strategy_a_creator_exit_10slots_clean_15m \
  --max-open-positions 10 \
  --max-duration-minutes 15
```

Andere Laufzeit:

```text
--max-duration-minutes 5
--max-duration-minutes 15
--max-duration-minutes 30
--max-duration-minutes 60
```

Andere Slotzahl:

```text
--max-open-positions 1
--max-open-positions 2
--max-open-positions 10
```

Bei der USD-Slot-Config gilt:

```text
Gesamtstartkapital = max_open_positions × 10 USD
```

Für einen neuen unabhängigen Test immer **neue DB und neue Campaign** verwenden.

## 5. Lauf vorzeitig kontrolliert stoppen

```text
Ctrl+C
```

Das beendet den Run kontrolliert und erzeugt weiterhin die Abschlussartefakte.

## 6. Statuszeile lesen

Wichtige Felder:

- `creates` = erkannte CREATEs
- `actionable` = wirtschaftlich handelbare CREATEs
- `intents` = erzeugte Entry-Intents
- `fills` = modellierte Entry-Fills
- `open` = offene Positionen
- `unresolved` = Positionen mit ungeklärtem Exposure/Exit
- `pnl` = realisierter PnL in Lamports
- `cash` = freies PAPER-Kapital
- `gap=False` = keine aktuelle kanonische Coverage-Lücke
- `err=0` = keine internen Fehler

Für einen sauberen Test auf `ECONOMIC_PAPER_READY`, `err=0`, keine Recorder-Overflows und nachvollziehbare `unresolved` achten.

## 7. Report erzeugen

Nach PAPER wird automatisch unter

```text
exports/<campaign>/<run_id>/
```

erzeugt:

- `report.json`
- `summary.md`
- `positions.csv`
- `latency_by_feed.csv`

Manuell erneut erzeugen:

```bash
python3 -m darkbot report \
  --db data/<datei>.sqlite \
  --campaign <campaign> \
  --out exports/<ordner>
```

## 8. Strategy B

Config:

```text
config/strategy_b.json
```

Strategy B bleibt die Legacy-Vergleichsstrategie:

- 1 s Observation
- mindestens 5 BUYs
- mindestens 5 Unique Buyers
- 10 USD Swap-Budget
- Netto-Target 1.05x
- 60 s Timeout
- kein Creator-Sell-Exit

## 9. A/B Replay

```bash
python3 -m darkbot ab-replay \
  --source-db data/<source>.sqlite \
  --source-run-id <run_id> \
  --output-db data/<neue_ab_db>.sqlite \
  --out exports/<ab_ordner> \
  --config-a config/phase1.json \
  --config-b config/strategy_b.json \
  --research-config config/research_ab.json
```

Die Output-DB muss neu und getrennt von der Source-DB sein.

## 10. Research

```bash
python3 -m darkbot research \
  --db data/<datei>.sqlite \
  --campaign <campaign> \
  --config config/research.json \
  --runtime-config config/phase1.json \
  --out exports/<research_ordner>
```

## 11. Wichtige Configs

- `config/strategy_a_creator_exit_usd_slots.json` – aktuelle Creator-Exit-/USD-Slot-PAPER-Variante
- `config/phase1.json` – stabile Strategy-A-Baseline für Regression/Research
- `config/strategy_b.json` – Legacy 5-BUY/5-Unique-Vergleichsstrategie
- `config/research.json` – Research-Parameter
- `config/research_ab.json` – A/B-Replay-Matrix

## 12. Sauberer Test – Mindestcheck

Vor Auswertung prüfen:

1. komplette Testsuite = OK
2. neue DB + neue Campaign
3. gewünschte Slotzahl explizit gesetzt
4. `--max-duration-minutes` gesetzt
5. keine Recorder-Overflows
6. `err=0`
7. Coverage-Gaps und `unresolved` separat prüfen
8. Cash-Reconciliation prüfen
9. erst danach EV/PnL auswerten

