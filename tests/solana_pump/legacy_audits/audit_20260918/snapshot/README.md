# DarkBot Phase-1 PAPER Backend (SPEC 1.3.2)

Ein Prozess, ein `StateOwner`, RAM-Zustand, begrenzte Queues, ein SQLite-Writer-Thread. PAPER heißt: reale Mainnet-Ereignisse,
imaginäres Kapital, modellierte Fills mit Evidenz `OBSERVED_STATE_PROXY`. **LIVE-Senden ist technisch gesperrt** (`execution.LiveBroadcastForbidden`, T22).
Verbindliche Spezifikation: `docs/SPEC.json` (Kopie von `DARKBOT_PHASE1_IMPLEMENTATION_SPEC_v1_3_2.json`).

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
```
python3 -m darkbot doctor   --config config/phase1.json                                   # D01–D06, Readiness, Secrets redigiert
python3 -m darkbot paper    --config config/phase1.json --db data/phase1.sqlite --campaign phase1_001   # Live-PAPER, Ctrl-C = kontrolliertes Ende
python3 -m darkbot report   --db data/phase1.sqlite --campaign phase1_001 --out exports/phase1_001
python3 -m darkbot research --db data/phase1.sqlite --campaign phase1_001 --config config/research.json --out exports/research_001
python3 -m unittest discover -s tests -t .                                                # T01–T52 (+ D01/D04/D06 offline)
```
Exitcodes: 0 ok, 2 Konfigurationsfehler, 3 Laufzeitfehler, 4 doctor NOT_READY.

## Dateibaum (normativ, `simple_project_structure`)
`darkbot/{__init__,__main__,main,cli,config,domain,clock,capture,market,strategy,execution,paper,fees,storage,replay,backtest,reporting}.py`,
`config/phase1.json`, `config/research.json`, `migrations/0001_phase1_spec_1_3_2.sql` (Legacy-Migration unverändert unter `migrations/legacy/`),
`tests/`, `data/`, `exports/`, `docs/SPEC.json`, `docs/legacy_backend/` (nicht lauffähiger Legacy-Upload als Beleg).

## Betriebsverhalten
- Hot Path pro Nachricht: normalize → atomares Envelope anwenden → Exposure-Ticks → Admission → Decision → atomare Reservierung → Adapter-Enqueue → Telemetrie `put_nowait`. Keine DB, kein Hashing, kein Print, kein `await` (T37).
- Gebühren werden im Hintergrund beschafft (Global-Account, getFeeForMessage, Rent, Priority-Perzentil) und als unveränderliche Snapshots gespeichert. `NULL` heißt UNBEKANNT, nie 0. Ohne vollständige, frische Snapshots: keine wirtschaftlichen Entries (`UNQUALIFIED_COSTS`), Aufnahme/Latenzbenchmark läuft weiter. Exits werden mit der zuletzt gültigen Regel weiter bearbeitet, dann als `*_STALE` markiert (Lauf nicht qualifizierbar).
- Statuszeile alle `storage.report_interval_ms`; Ctrl-C → keine neuen Entries, Feeds stoppen, Recorder flushen, Run `INTERRUPTED`, offene Positionen als Exposure/zensiert.
- Neustart: vorher laufende Runs werden `INTERRUPTED`, neue Clock-Epoch; Positionen werden nie automatisch geschlossen (T23).

## Bekannte Grenzen / Annahmen (siehe Abschlussbericht)
- `CreateEvent`-Tail-Layout ist offline nicht verifizierbar. Nur das gepinnte Layout `CREATE_EVENT_V1_NO_TAIL` (impliziertes SOL) ist handelbar; unbekannte Tails ⇒ `QUOTE_UNKNOWN_LAYOUT_UNVERIFIED` (observe-only). `doctor` (D03) meldet beobachtete Layouts; Pinning erfolgt versioniert in `config/phase1.json → protocol.create_event_tail_layouts`.
- Global-Account-Adresse/Layout sind aus der öffentlichen IDL gepinnt, aber offline unverifiziert (`verification_status`); `doctor` validiert Owner/Länge/Plausibilität, sonst `UNQUALIFIED_COSTS`.
- Fee-Rundung `CEIL_BPS` und Curve-Rundung `FLOOR_TOKENS_CEIL_SOL_INPUT` sind versionierte Annahmen aus dem Legacy-Decoder.
- Keine Zahl im Report behauptet globale Erstkenntnis, sicheren Fill oder Profit; positive Ergebnisse sind höchstens `POSITIVE_UNDER_MODEL`.
