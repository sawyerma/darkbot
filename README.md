# DarkBot Phase-1 PAPER Backend (SPEC 1.3.2)

Ein Prozess, ein `StateOwner`, RAM-Zustand, begrenzte Queues, ein SQLite-Writer-Thread. PAPER heißt: reale Mainnet-Ereignisse,
imaginäres Kapital, modellierte Fills mit Evidenz `OBSERVED_STATE_PROXY`. **LIVE-Senden ist technisch gesperrt** (`execution.LiveBroadcastForbidden`, T22).
Verbindliche Spezifikation: `docs/SPEC.json` (Kopie von `DARKBOT_PHASE1_IMPLEMENTATION_SPEC_v1_3_2.json`).

**Praxis-/Test-Kurzreferenz:** [docs/TEST_HANDBUCH.md](docs/TEST_HANDBUCH.md)

## Voraussetzungen
- Python ≥ 3.12 (entwickelt/getestet mit CPython 3.12.3). Keine venv/Container. Installation nur über denselben Interpreter:
  `python3 -m pip install -r requirements.txt` (einzige Drittabhängigkeit: `websockets`; nur für `doctor`/`paper` nötig; Tests, Replay, Report, Research laufen ohne).
- Umgebungsvariablen (nur Namen, keine Werte in Dateien):
  - `HELIUS_API_KEY` – Feed `helius_standard` (Pflicht) und HTTP-RPC-Fallback für Gebühren-Snapshots
  - `EXISTING_RPC_WSS_URL` – optionaler zweiter unabhängiger logsSubscribe-Feed (Provider/Region dokumentieren)
  - `EXISTING_RPC_HTTP_URL` – optionaler HTTP-RPC für getAccountInfo/getFeeForMessage/getMinimumBalanceForRentExemption/getRecentPrioritizationFees
  - `PUMPPORTAL_API_KEY` – optional; nur `subscribeNewToken` + `subscribeMigration` auf genau einer Verbindung, keine kostenpflichtigen Subscriptions
- Ausreichend Speicherplatz (`doctor` prüft ≥ 512 MB frei) und schreibbares `data/`.

## Befehle

```bash
# komplette Testsuite
python3 -m unittest discover -s tests -p 'test_*.py' -v

# Doctor / Readiness für aktuelle Strategy A
python3 -m darkbot doctor --config config/strategy_a_creator_exit_usd_slots.json

# Aktuelle Strategy A: 10 USD pro Slot, Creator-Exit, kein fixes TP
# --max-duration-minutes ist für PAPER verpflichtend.
python3 -m darkbot paper \
  --config config/strategy_a_creator_exit_usd_slots.json \
  --db data/strategy_a_creator_exit_10slots_15m.sqlite \
  --campaign strategy_a_creator_exit_10slots_15m \
  --max-open-positions 10 \
  --max-duration-minutes 15

# Strategy C: altes Backend als Policy (hier Profil PRE_PUMP_01; alle sechs Profile: config/strategy_c_*.json)
python3 -m darkbot paper \
  --config config/strategy_c_pre_pump_01.json \
  --db data/strategy_c_pre_pump_01_15m.sqlite \
  --campaign strategy_c_pre_pump_01_15m \
  --max-open-positions 1 \
  --max-duration-minutes 15

# Strategie-freier 15-Minuten-Market-Capture
python3 -m darkbot capture \
  --config config/strategy_a_creator_exit_usd_slots.json \
  --db data/market_capture_15m.sqlite \
  --campaign market_capture_15m \
  --max-duration-minutes 15

# Offline Edge-Analyse (keine Trades, reine Auswertung)
python3 -m darkbot edge-analysis \
  --source data/market_capture_15m.sqlite \
  --out exports/market_capture_15m_edge

# Report
python3 -m darkbot report \
  --db data/strategy_a_creator_exit_10slots_15m.sqlite \
  --campaign strategy_a_creator_exit_10slots_15m \
  --out exports/strategy_a_creator_exit_10slots_15m

# Research / Baseline
python3 -m darkbot research \
  --db data/phase1.sqlite \
  --campaign phase1_001 \
  --config config/research.json \
  --runtime-config config/phase1.json \
  --out exports/research_001
```

Exitcodes: 0 ok, 2 Konfigurationsfehler, 3 Laufzeitfehler, 4 NOT_READY. Jeder PAPER-Lauf benötigt `--max-duration-minutes > 0`; nach Ablauf stoppt der gesamte Run kontrolliert automatisch. `Ctrl+C` bleibt als kontrollierter manueller Stop möglich. Nach jedem PAPER-Lauf werden `report.json`, `summary.md`, `positions.csv` und `latency_by_feed.csv` automatisch unter `exports/<campaign>/<run_id>/` erzeugt.

### Strategie-freie Edge-Suche

Für neue Exit-/Timing-Hypothesen zuerst `capture` verwenden. Dieser Modus zeichnet Marktpfade ohne Strategy-Callbacks, Entry-/Exit-Entscheidungen oder simulierte Trades auf. `edge-analysis` wertet diese Aufnahme anschließend offline aus (Peak/Tief, Zeitpunkte, Target-Hits, Creator-Verhalten und kombinierte Exit-Regeln). Eine auf demselben Datensatz gefundene Regel gilt nur als In-Sample-Kandidat und muss auf einem neuen Capture validiert werden.

### Aktuelle Strategien

**Strategy A – `config/strategy_a_creator_exit_usd_slots.json`**
- CREATE -> sofort BUY
- keine Observation-Verzögerung
- 10 USD Gesamtbudget pro Slot
- Startkapital = `max_open_positions × 10 USD`
- kein fixes Take-Profit
- Creator-Sell -> EXIT
- Positions-Timeout 590 s
- PAPER only

**Strategy-A-Baseline – `config/phase1.json`**
- stabile Regression-/Research-Baseline
- FIXED_SOL-Sizing
- 1.5x-Target weiterhin aktiv
- nicht mit der aktuellen Creator-Exit-/USD-Slot-PAPER-Variante verwechseln

**Strategy B – `config/strategy_b.json`**
- 1 s Observation
- mindestens 5 BUYs und 5 Unique Buyers
- 10 USD Swap-Budget
- 1.05x Netto-Target
- 60 s Timeout
- kein Creator-Sell-Exit

**Strategy C – `config/strategy_c_*.json` (`LEGACY_BACKEND_FILTER_V1`)**

- komplette Policy des alten Backends (`old_system/backend`), siehe [docs/STRATEGY_C_SOURCE_OF_TRUTH.md](docs/STRATEGY_C_SOURCE_OF_TRUTH.md)
- `observation_ms` frei (alte Profile: 0 / 500 / 1000 ms), Feature-Snapshot mit exaktem Cutoff
- typisierte Filter `{feature, min, max}` über die 17 Legacy-Features; NULL = nicht bestanden
- Creator-Sell bis zum modellierten Entry-Fill -> Entry entfällt kostenfrei
- Exits: CREATOR_SELL > TARGET > STOP > TIMEOUT; Stop 0 löst bei Netto-Wert <= 0 trotzdem aus
- Swap-Budget in Lamports inkl. Pump-/Creator-Fee; Netzwerk/Rent zusätzlich
- Profile: `fast_creator_hold_2x`, `pre_pump_01`, `launch_sniper_0ms_15x`, `paper_explore_01`, `example`, `smoke_all`
- Latenz 800/800 ms (Sniper 0/0) und `quote_max_age_ms = 600000` wie im alten Backend (letzter beobachteter Zustand, beliebig alt); für einen A/B/C-Vergleich unter identischer Engine beides in der Config auf die A/B-Werte setzen

## Dateibaum (normativ, `simple_project_structure`)
`darkbot/{__init__,__main__,main,cli,config,domain,clock,capture,market,strategy,execution,paper,fees,storage,replay,backtest,reporting}.py`,
`config/phase1.json`, `config/research.json`, `migrations/0001_phase1_spec_1_3_2.sql` (Legacy-Migration unverändert unter `migrations/legacy/`),
`tests/`, `data/`, `exports/`, `docs/SPEC.json`, `docs/legacy_backend/` (nicht lauffähiger Legacy-Upload als Beleg).

## Betriebsverhalten
- Hot Path pro Nachricht: normalize → atomares Envelope anwenden → Exposure-Ticks → Admission → Decision → atomare Reservierung → Adapter-Enqueue → Telemetrie `put_nowait`. Keine DB, kein Hashing, kein Print, kein `await` (T37).
- Gebühren werden im Hintergrund beschafft (Global-Account, getFeeForMessage, Rent, Priority-Perzentil) und als unveränderliche Snapshots gespeichert. `NULL` heißt UNBEKANNT, nie 0. Ohne vollständige, frische Snapshots: keine wirtschaftlichen Entries (`UNQUALIFIED_COSTS`), Aufnahme/Latenzbenchmark läuft weiter. Auch Exits werden nie auf veralteter Kostenbasis gefüllt: der Exit-Intent bleibt geloggt/latched, bis ein frischer Snapshot vorliegt. Beim modellierten Fill gilt die Fee-Regel des zu diesem Zeitpunkt gültigen Snapshots (nicht die Submit-Basis). Ein Attempt, dessen modellierter Fill in eine Coverage-Lücke fällt, bleibt UNKNOWN (Kosten NULL, Position UNRESOLVED) und wird nie mit späteren Zuständen „reconciled“.
- Statuszeile alle `storage.report_interval_ms`. PAPER verlangt `--max-duration-minutes`; nach Ablauf stoppt der gesamte Run kontrolliert automatisch. Ctrl-C bleibt als manueller kontrollierter Stop möglich.
- Neustart: vorher laufende Runs werden `INTERRUPTED`, neue Clock-Epoch; Positionen werden nie automatisch geschlossen (T23).

## Bekannte Grenzen / Annahmen

Aktueller Prüfstand: [R8_2 – Doctor-Gate bestanden, Edge-A/B offen](docs/R8_2_DOCTOR_GATE_STATUS.md). Token-2022-Implementierung: [R8_2/R9-Arbeitsbericht](docs/R9_IMPLEMENTATION_STATUS.md). Historischer Stand: [R6/R7-Auditstatus](docs/R6_R7_AUDIT_STATUS.md).
- Die aktuellen offiziellen Create-/Trade-/Complete-/Migration-Layouts sind mit IDL-Hash gepinnt und durch aufgezeichnete Mainnet-Fixtures ergänzt. Unbekannte Layouts bleiben observe-only. Token-2022 unterstützt das kausal belegte Pump-CreateV2-Profil (MetadataPointer/TokenMetadata, ImmutableOwner-ATA mit 170 Byte). Andere Extensions und unbekannte Programme bleiben gesperrt. V2-BUY/SELL, programmbezogene Rent-/Fee-Snapshots und die Grenzen der Prüfung sind in `docs/R9_IMPLEMENTATION_STATUS.md` dokumentiert.
- Global und FeeConfig werden mit Owner-/Discriminator-/Layoutprüfung gelesen. Die korrigierte Global-Adresse und dynamische SOL-FeeConfig-Tiers sind implementiert; der neue 60-Sekunden-Doctor hat D01–D06 gleichzeitig bestanden; der alte Edge-A/B-Nachweis steht noch aus (siehe Gate-Nachtrag).
- Nicht unterstützte Gebührenmodi sind nicht handelbar. Verifizierte Eventlayouts allein garantieren keine wirtschaftliche Readiness; vollständige aktuelle Kosten-Snapshots bleiben Pflicht.
- Reservefrische ist strikt: Reserven älter als `execution.quote_max_age_ms` (250 ms) sind nie Quote- oder Fill-Basis, auch nicht bei lückenloser Coverage; Exits bleiben dann geloggt/latched bis zum nächsten beobachteten Trade (Timeout auf inaktiven Tokens ⇒ zensiert statt fiktiv verkauft). Modellierte Fills werden UNKNOWN, wenn im Fenster (Proxy → Fill) ein Konflikt, eine Mint-Decode-Lücke (unbekanntes Pump-Event im Tx der Mint), ein nicht zuordenbarer undecodierbarer Frame, eine kanonische Lücke oder eine unbekannte Fee-Regel liegt.
- Gebuehren-Gate: `--require-economic-within-s N` setzt die Startfrist bis zur ersten wirtschaftlichen Readiness (Kapital initialisiert, beobachteter unterstuetzter CurveState, BUY- und SELL-`fee_terms` gueltig). `--max-economic-outage-s M` setzt die maximal tolerierte zusammenhaengende spaetere Unterbrechung; wird M weggelassen, gilt aus Kompatibilitaetsgruenden N auch fuer spaetere Ausfaelle. Eine Erholung setzt den Ausfalltimer zurueck, getrennte kurze RPC-Ausfaelle akkumulieren nicht. Bei Ueberschreitung stoppt PAPER/CAPTURE mit Exit 4 (`FEE_EVIDENCE_NOT_ECONOMIC`, inklusive `phase`, Komponentenstatus und `not_economic_for_s`). Die Statuszeile zeigt `fees=prot:…,prio:…,netw:…,rent:…` (VALID / STALE / MISSING / UNKNOWN_EVIDENCE / RPC-Status). Eine Aufnahme mit Gate-Verletzung wird nicht als saubere wirtschaftliche Replay-Datenbasis akzeptiert. Diagnose einer vorhandenen DB: `python3 fee_evidence_check.py --db <db> --run-id <run> --config <profil>` (read-only; Rohbestand inklusive Evidence, BUY/SELL-Zeitleiste, Refresh-Abstaende, echter RAM-Replay der Config; Exit 0 = PASS nur bei Fills und 0x `UNQUALIFIED_COSTS`).
- Wiedereinstiegsschutz ist kampagnenweit (`positions` aller PAPER-Runs der Kampagne werden beim Start geladen). Nicht persistierbare Datensätze (dauerhaft fehlendes FK-Ziel) landen begrenzt in `<db>.quarantine.jsonl`, der Run wird DEGRADED; keine Endlosschleife.
- Replay wendet die aufgezeichneten Feed-Health-Übergänge (Quality-Events) zeitlich verzahnt mit Timern über denselben `on_feed_health`-Pfad an wie PAPER; Roh-Payloads werden beim Laden gegen ihre gespeicherten Hashes geprüft (Manipulation ⇒ `DATASET_INTEGRITY_VIOLATION`); der Dataset-Hash enthält Health-Übergänge, Fee-Inhalte, Decoder-Code-Hash und Protokoll-Pinning.
- Research trennt Kandidaten-Freeze, einmaligen Holdout und spätere Forward-Aufnahmen. Runtime-/Researchparameter bleiben gebunden. `forward_test=true` wählt den Forward-Pfad; `--source-run-id` kann für jede neue Aufnahme wiederholt werden.
- Replay/Research: Modellzeit endet am aufgezeichneten Run-Ende (`runs.ended_mono_ns`); offene Positionen werden zensiert, nie mit altem Kurs geschlossen. Research-Läufe sind pro Aufnahme+Dataset-Hash eindeutig; `POSITIVE_UNDER_MODEL` nur auf dem entsperrten Forward-Split, für den vorregistrierten Validierungskandidaten, mit bestandenen Stress-Varianten (Fee ×, adverse bps, Reject-Rate) und ohne ungeklärte Exposure.
- Fee- und Curve-Rundung sind in der Runtime-Konfiguration versioniert; die BUY-Kosten berücksichtigen die offizielle Floor-plus-one-Regel. Legacy-Featureformeln bleiben davon getrennt.
- Keine Zahl im Report behauptet globale Erstkenntnis, sicheren Fill oder Profit; positive Ergebnisse sind höchstens `POSITIVE_UNDER_MODEL`.
# Audit-Ergaenzung: Replay-Evidenz und Strategy C

- `strategy.creator_sold_before_fill = "TX_LANDS"` aktiviert fuer Strategy C die normale
  Auswertung eines bereits eingereichten BUY trotz Creator-Sell vor modelliertem Fill.
  Kein erzwungener Fill: Kosten, Coverage, Reservefrische und Slippage gelten weiterhin.
  Ohne Feld bleibt `LEGACY_FREE_CANCEL` zur historischen Reproduktion erhalten. Dieser Modus
  ist diagnostisch und kann keinen positiven wirtschaftlichen Nachweis liefern.
- Reports zeigen kostenlose Creator-Stornos separat als `entry_cancelled_free_of_cost`,
  ihren Grund und den Modus in `model_assumptions`; sie sind keine Pre-Submit-SKIPs.
- Replay prueft alle von Fee-Snapshots referenzierten `COST_MODEL`- und `FRESHNESS_POLICY`-
  Snapshots gegen Typ, Inhalt und Hash. `max_age_ns` wird ausschliesslich aus der beim Source-
  Run referenzierten Freshness-Policy rekonstruiert, nicht aus der spaeter gewaehlten Replay-
  Config. Fehlende/falsche Source-Policies sind `DATASET_INTEGRITY_VIOLATION`. Referenzen,
  Inhalte und daraus abgeleitete TTL sind an `REPLAY_DATASET_V4_SOURCE_FRESHNESS_POLICY`
  gebunden; neue Dataset-Hashes sind eine neue Datenversion.
- Voruebergehende Transport-/Request-Budget-Fehler duerfen passende, noch frische Kosten-
  Evidenz bis zu ihrer urspruenglichen TTL erhalten. Der FeeResolver wiederholt retrybare
  Transport-/Request-Budget-Fehler mit getrenntem Backoff fuer Protocol, Rent und das gekoppelte
  Priority/Network-Paar nach derselben konfigurierten Connectivity-Backoff-Policy wie die Live-Feeds (`connectivity.reconnect_initial_ms` bis `connectivity.reconnect_max_ms`, exponentiell), ohne TTL zu verlaengern. Erfolg
  setzt den jeweiligen Backoff zurueck; nicht-retrybare Layout-/Profilfehler bleiben auf normalem
  Refresh. Rohfehler bleiben gespeichert. Netzwerkgebuehr und Priority bleiben ein
  zusammengehoeriges Paar. Live, Replay und Pending-Fills verwenden denselben Auswahlpfad zum
  jeweiligen Wissenszeitpunkt.
- PAPER `--max-open-positions` verwendet wie Replay/Research Kapital pro konfiguriertem Slot;
  USD-Slot-Profile werden weiterhin ueber FX initialisiert.
