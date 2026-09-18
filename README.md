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
Exitcodes: 0 ok, 2 Konfigurationsfehler, 3 Laufzeitfehler, 4 NOT_READY (`doctor`, oder `paper` ohne startbaren Feed). Nach jedem `paper`-Lauf (auch Ctrl-C) werden report.json/summary.md/positions.csv/latency_by_feed.csv automatisch unter `exports/<campaign>/<run_id>/` erzeugt und in `reports` persistiert.

## Dateibaum (normativ, `simple_project_structure`)
`darkbot/{__init__,__main__,main,cli,config,domain,clock,capture,market,strategy,execution,paper,fees,storage,replay,backtest,reporting}.py`,
`config/phase1.json`, `config/research.json`, `migrations/0001_phase1_spec_1_3_2.sql` (Legacy-Migration unverändert unter `migrations/legacy/`),
`tests/`, `data/`, `exports/`, `docs/SPEC.json`, `docs/legacy_backend/` (nicht lauffähiger Legacy-Upload als Beleg).

## Betriebsverhalten
- Hot Path pro Nachricht: normalize → atomares Envelope anwenden → Exposure-Ticks → Admission → Decision → atomare Reservierung → Adapter-Enqueue → Telemetrie `put_nowait`. Keine DB, kein Hashing, kein Print, kein `await` (T37).
- Gebühren werden im Hintergrund beschafft (Global-Account, getFeeForMessage, Rent, Priority-Perzentil) und als unveränderliche Snapshots gespeichert. `NULL` heißt UNBEKANNT, nie 0. Ohne vollständige, frische Snapshots: keine wirtschaftlichen Entries (`UNQUALIFIED_COSTS`), Aufnahme/Latenzbenchmark läuft weiter. Auch Exits werden nie auf veralteter Kostenbasis gefüllt: der Exit-Intent bleibt geloggt/latched, bis ein frischer Snapshot vorliegt. Beim modellierten Fill gilt die Fee-Regel des zu diesem Zeitpunkt gültigen Snapshots (nicht die Submit-Basis). Ein Attempt, dessen modellierter Fill in eine Coverage-Lücke fällt, bleibt UNKNOWN (Kosten NULL, Position UNRESOLVED) und wird nie mit späteren Zuständen „reconciled“.
- Statuszeile alle `storage.report_interval_ms`; Ctrl-C → keine neuen Entries, Feeds stoppen, Recorder flushen, Run `INTERRUPTED`, offene Positionen als Exposure/zensiert.
- Neustart: vorher laufende Runs werden `INTERRUPTED`, neue Clock-Epoch; Positionen werden nie automatisch geschlossen (T23).

## Bekannte Grenzen / Annahmen (siehe Abschlussbericht)
- `CreateEvent`-Tail-Layout ist offline nicht verifizierbar. Nur das gepinnte Layout `CREATE_EVENT_V1_NO_TAIL` (impliziertes SOL) ist handelbar; unbekannte Tails ⇒ `QUOTE_UNKNOWN_LAYOUT_UNVERIFIED` (observe-only). `doctor` (D03) meldet beobachtete Layouts; Pinning erfolgt versioniert in `config/phase1.json → protocol.create_event_tail_layouts`.
- Global-Account: Owner **und** Anchor-Discriminator (`sha256("account:Global")[:8]`) werden immer geprüft; Adresse/Layout sind aus der öffentlichen IDL gepinnt, aber offline unverifiziert (`verification_status`): Protokollgebühren bleiben `LAYOUT_UNVERIFIED` (kein wirtschaftlicher PAPER), bis das Layout nach Prüfung gegen die offizielle IDL als `VERIFIED_*` markiert ist; `doctor` D05 zeigt die dekodierten Kandidatenwerte.
- Die offizielle Pump-IDL enthält laut Audit zusätzliche Felder (u. a. `quote_mint`, `quote_amount`, `virtual_quote_reserves`, Gebührenfelder) in CreateEvent/TradeEvent/CompleteEvent. Der Decoder verarbeitet gepinnte Prefixe; Tails werden nur über versionierte, als `VERIFIED_*` markierte Layouts wirtschaftlich genutzt (`protocol.events.<Event>.tail_layouts`, `protocol.create_event_tail_layouts`). Der mitgelieferte TradeEvent-Tail-Kandidat ist UNVERIFIED und dient nur als Evidenz. Ohne diese Pinnings: Aufnahme ja, Handel nein.
- Reservefrische ist strikt: Reserven älter als `execution.quote_max_age_ms` (250 ms) sind nie Quote- oder Fill-Basis, auch nicht bei lückenloser Coverage; Exits bleiben dann geloggt/latched bis zum nächsten beobachteten Trade (Timeout auf inaktiven Tokens ⇒ zensiert statt fiktiv verkauft). Modellierte Fills werden UNKNOWN, wenn im Fenster (Proxy → Fill) ein Konflikt, eine Mint-Decode-Lücke (unbekanntes Pump-Event im Tx der Mint), ein nicht zuordenbarer undecodierbarer Frame, eine kanonische Lücke oder eine unbekannte Fee-Regel liegt.
- Wiedereinstiegsschutz ist kampagnenweit (`positions` aller PAPER-Runs der Kampagne werden beim Start geladen). Nicht persistierbare Datensätze (dauerhaft fehlendes FK-Ziel) landen begrenzt in `<db>.quarantine.jsonl`, der Run wird DEGRADED; keine Endlosschleife.
- Replay wendet die aufgezeichneten Feed-Health-Übergänge (Quality-Events) zeitlich verzahnt mit Timern über denselben `on_feed_health`-Pfad an wie PAPER; Roh-Payloads werden beim Laden gegen ihre gespeicherten Hashes geprüft (Manipulation ⇒ `DATASET_INTEGRITY_VIOLATION`); der Dataset-Hash enthält Health-Übergänge, Fee-Inhalte, Decoder-Code-Hash und Protokoll-Pinning.
- Research läuft in zwei Stufen: `test_split_unlocked=false` erzeugt bei positivem Validierungskandidaten ein unveränderliches `POLICY_FREEZE`; `test_split_unlocked=true` ohne Freeze wird abgelehnt. Stress-Siblings zählen nur je eindeutigem Szenario und je Aufnahme; INSUFFICIENT/UNQUALIFIED-Siblings und unbekannte Exposure blockieren.
- Replay/Research: Modellzeit endet am aufgezeichneten Run-Ende (`runs.ended_mono_ns`); offene Positionen werden zensiert, nie mit altem Kurs geschlossen. Research-Läufe sind pro Aufnahme+Dataset-Hash eindeutig; `POSITIVE_UNDER_MODEL` nur auf dem entsperrten Forward-Split, für den vorregistrierten Validierungskandidaten, mit bestandenen Stress-Varianten (Fee ×, adverse bps, Reject-Rate) und ohne ungeklärte Exposure.
- Fee-Rundung `CEIL_BPS` und Curve-Rundung `FLOOR_TOKENS_CEIL_SOL_INPUT` sind versionierte Annahmen aus dem Legacy-Decoder.
- Keine Zahl im Report behauptet globale Erstkenntnis, sicheren Fill oder Profit; positive Ergebnisse sind höchstens `POSITIVE_UNDER_MODEL`.
