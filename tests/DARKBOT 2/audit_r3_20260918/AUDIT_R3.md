# DarkBot r3 – erneute unabhängige Prüfung

Stand: 18.09.2026. Ergebnis: **78 vorhandene Tests bestanden; vollständige Spec-Konformität und die Behauptung „alle 17 Gruppen behoben“ sind nicht bestätigt. Mehrere blockierende Gegenbeispiele bestehen fort.**

## Prüfgrundlage und Grenzen

- Original: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`.
- Tests auf unveränderter Produktivcode-Kopie: `audit_r3_20260918/snapshot` (69 Quelldateien im Manifest, ohne `.DS_Store`/Bytecode).
- `docs/SPEC.json` ist bytegleich mit der verbindlichen Roadmap `DARKBOT_PHASE1_IMPLEMENTATION_SPEC_v1_3_2.json`.
- Kein Produktivcode verändert. Audit-Skripte, temporäre Datenbanken und Berichte liegen ausschließlich im Audit-Verzeichnis.
- Drei unabhängige Read-only-Subagenten: `r3_spec_reviewer`, `r3_test_reviewer`, `r3_architecture_reviewer`. Kein Austausch der Findings zwischen Reviewern. Alle drei hinterließen reproduzierbare Skripte/Ergebnisse, wurden aber vor abschließendem Review-Bericht durch das Nutzungslimit unterbrochen. Daher **kein vollständiges dreifaches Review-Sign-off**. Der Orchestrator hat die abgelegten Skripte gelesen, erneut ausgeführt und die Ergebnisse konsolidiert.
- Kein CODER eingesetzt: Auftrag ist weiterhin Prüfung und Fehlerliste vor gemeinsamer Überarbeitung.

## Terminalergebnis

| Prüfung | Ergebnis |
|---|---|
| `python3 -m unittest discover -s tests -t . -v` | 78 Tests, 1,271 s, OK; ResourceWarnings wegen nicht geschlossener Test-SQLite-Verbindungen |
| `doctor` ohne Feed-Zugangsdaten | Exit 4, NOT_READY; D01/D02/D03/D05/D06 FAIL, D04 Fixture PASS |
| `paper` ohne Feed-Zugangsdaten | Exit 4, NOT_READY; automatische report.json, summary.md, positions.csv, latency_by_feed.csv vorhanden |
| `report` auf dieser Diagnoseaufnahme | Exit 0 |
| `research --max-variants 1` auf dieser Diagnoseaufnahme | Exit 0; nur CLI-Smoke-Test, kein wirtschaftlicher Nachweis |
| Drei unabhängige Gegenbeispiel-Skripte | Erneut erfolgreich ausgeführt; reproduzieren die unten genannten Fehler |

Python: lokales CPython 3.14.3. `websockets` fehlt; HELIUS_API_KEY, EXISTING_RPC_WSS_URL, EXISTING_RPC_HTTP_URL und PUMPPORTAL_API_KEY waren nicht verfügbar. Kein Mainnet-Pilot, keine 1000 CREATEs, kein Forward-Test. D04 PASS bezieht sich auf synthetische Fixtures. Die öffentlichen IDL-Dokumente konnten dagegen ohne Keys gelesen werden.

Die Gegenbeispiel-Skripte protokollieren tatsächliches Fehlverhalten und enden bei erfolgreicher Reproduktion mit Exit 0. Dieser Exitcode bedeutet nicht, dass der Bot die betreffende Anforderung erfüllt. Sie wurden nicht in die 78 vorhandenen Tests eingerechnet.

## Status der ursprünglichen 17 Gruppen

„Kernfall behoben“ ist eine begrenzte Aussage über den ursprünglichen Fall, keine Abnahme aller angrenzenden Anforderungen.

| ID | Status r3 | Beleg / verbleibender Fehler |
|---|---|---|
| S01 | Kernfall behoben | Die Future-State-Reconciliation ist entfernt. UNKNOWN wird nicht mehr durch den alten Pfad nachträglich zum früheren Fill. Andere Fälle unbekannter Kosten bleiben fehlerhaft, siehe S07. |
| S02 | Teilweise behoben, **blockierend** | Veraltete Gebühren werden beim Exit geblockt. Mit frischen Gebühren werden aber weiterhin **590 Sekunden alte Reserven** gefüllt, obwohl `proxy_state_max_age_ms=250`. |
| S03 | Teilweise behoben | COMPLETE nach PARTIAL erzeugt nun einen Entry. Persistiert bleibt jedoch der erste PARTIAL-Datensatz: `actionable_mono_ns=NULL`, falscher `winner_feed_id`. |
| S04 | Weiterhin **blockierend** | Einzellauf-Report vergibt kein POSITIVE mehr. Research kann aber ohne gefrorenen Kandidaten und mit unzureichenden/fremden Stress-Siblings POSITIVE vergeben. |
| S05 | Ursprüngliche run_id-Kollision behoben | Source-Run und Dataset-Scope trennen die Replay-Runs; identischer persistierter Rerun wird abgelehnt. Die Research-Statistik und Stress-Zuordnung sind dadurch noch nicht kampagnenweit korrekt. |
| S06 | Teilweise behoben | Timing, Snapshot-Hash-Felder, Gaps und Run-Ende aufgenommen. Geladene Inhalte werden nicht gegen diese Hashes validiert; geändertes Decoder-Layout bei gleichem Versionsstring ändert den Dataset-Hash nicht. |
| S07 | Teilweise behoben, **blockierend** | Fill verwendet neue gültige Gebühren. Bei inzwischen ungültiger Gebührenbasis wird jedoch ein definitiver bezahlter Fehlversuch statt UNKNOWN verbucht; außerdem inkonsistente Attempt-Kosten. |
| S08 | Ursprünglicher Token-2022-Fall behoben | Der neue Regressionstest bestätigt Ablehnung bei deaktivierter Unterstützung. Das ist kein Nachweis vollständiger Unterstützung aktueller Mainnet-Layouts. |
| S09 | Teilweise behoben, **blockierend im Stressmodell** | Kostenwert, Reject-Szenarien und einseitige Delays vorhanden. Stress wird nach der Slippage-Prüfung angewendet und kann einen Fill über `max_input` erzeugen; Stresswerte fehlen in den per-Variant Runtime-/Cost-/Quote-Hashes. |
| S10 | Teilweise behoben | Ganze Zeitblöcke werden innerhalb eines Runs getrennt. Jeder Quellrun wird aber separat 60/20/20 geteilt; kein kampagnenweiter chronologischer Split. Ungeklärte Exposure bestimmt die tatsächliche Purge-Grenze nicht. |
| S11 | Teilweise behoben | SKIP-Snapshots und V2-Cutoff-Gap korrigiert. V1 verwendet weiterhin spätere Feed-Gesundheit; abgeschnittene Trade-Historie wird als `quality_status=OK` ausgegeben; Feature-Key-Reihenfolge bleibt alphabetisch. |
| S12 | Teilweise behoben, **blockierend** | Fremd-Quote-CREATE und seine gewöhnlichen Folge-Trades werden geblockt. Explizit fremder Trade-Tail nach SOL-CREATE setzt nur Unsicherheit, überschreibt trotzdem SOL-Reserven und kann einen gefüllten Exit auslösen. |
| S13 | Teilweise behoben | Reporter berücksichtigt fehlgeschlagene Entries im Bootstrap. Utilization kann schon bei einem Slot `5.0` betragen; Research gruppiert fehlgeschlagene Entries weiterhin über `open_mono_ns or 0`; daily_pnl bleibt Run-Starttag. |
| S14 | Teilweise behoben | Neue Snapshot-IDs/Fill-Quote vorhanden. Zahlen und referenzierte IDs widersprechen sich nach Fee-Wechsel; Global-Discriminator wird nicht geprüft; Fee-Probe ist nur ComputeBudget statt konkreter BUY/SELL-Nachricht; zukünftiges Audit-Event/CENSORED nicht vollständig nachgeführt. |
| S15 | Weiterhin **blockierend** | Konflikte/Reorgs setzen `quote_uncertain`, aber Pending-Fills und Exits ignorieren es. Decode-Lücken führen nur zu einer Summary mit `canonical_gap=0`. Migration-Hint-Nachprüfung und Creator-Sell-Livezähler bleiben unvollständig. |
| S16 | Teilweise behoben | Mehrere Limits in JSON verschoben. Weitere operative Konstanten bleiben in Python; RPC-Budget/Liveness-Limits weiterhin nicht implementiert. Historische Positionsdaten bleiben unbegrenzt im RAM und werden vor neuen Entries durchlaufen. |
| S17 | Kernfall behoben | Automatische vier Artefakte plus Reports-Persistierung bestätigt; Interrupted-Zweig durch vorhandenen Mock-Test abgedeckt, No-Feed-Zweig tatsächlich im Terminal ausgeführt. |
| CLI | Ursprünglicher Fall behoben | Tatsächlicher No-Feed-Aufruf liefert Exit 4 statt 0. |

## Konkrete offene Findings mit Reproduktion

### P1 – veraltete Reserven werden ausgeführt (S02)

`darkbot/execution.py:180`, `:218`, `:317`: Die Altersgrenze wird nur bei `coverage != OBSERVED` geprüft. `_quote_stale` in `paper.py:417` misst lediglich das Alter der neu erzeugten Quote.

Repro `stale_reserve_timeout` / `old_reserves`: Entry öffnen, Gebühren frisch halten, 590 Sekunden keine neue Reserve beobachten. Ergebnis: SELL `FILLED`, Position CLOSED, `proxy_age_ns=590000000000`, konfiguriert `250000000`. Der neue S02-Test stellt nur den Gebühren-Refresh ab und prüft diesen Reservefall nicht.

Spec: `reliability_and_resource_limits.position_safety`, `shared_strategy_and_execution_contract.paper_realism`, konfigurierte Reserve-/Quote-Frische.

### P1 – Konflikt/Reorg invalidiert Pending-Fill nicht (S15)

`market.py:426`, `:445` markieren Unsicherheit; Prüfung nur beim neuen Entry in `paper.py:332`, nicht in `_on_outcome` ab `paper.py:540` oder dem Exit-Pfad.

Repros `conflict_pending`, `reorg_before_fill`: CREATE, dann widersprüchliche Kopie bzw. FAILED_TX vor dem geplanten Fill. Ergebnis jeweils `quote_uncertain=true`, trotzdem BUY `FILLED`, Position OPEN. Der Reorg-Fall verwendet weiter `proxy_signature=sigC`, Gap-Flag bleibt false.

Spec: `core_design.dedup_and_order.out_of_order/reorgs`, `provider_coverage_contract.definition/hard_gate`.

### P1 – Decode-Lücke wird als vollständige Abdeckung behandelt (S15)

`paper.py:263` zählt Fehler, `paper.py:755` schreibt nur `DECODE_GAP_SUMMARY` mit `canonical_gap=False`.

Repro `decode_gap_before_fill`: Einzige abdeckende Quelle liefert zwischen Submit und Fill ein unbekanntes Pump-Event. Ergebnis: `decode_errors=1`, Coverage OBSERVED, Gap-Flag false, Fill erfolgreich. Widerspricht dem Abdeckungsnachweis; eine offene WebSocket-Verbindung reicht nicht.

Spec: `provider_coverage_contract.definition/scope/hard_gate`.

### P1 – Fremd-Quote-Trade wird mit SOL bewertet (S12)

`market.py:498` setzt bei fremdem Tail `quote_uncertain`, läuft aber weiter zu `_set_state` unter dem alten SOL-Status. Folgepfade ignorieren Unsicherheit.

Repro `foreign_trade`: Nach SOL-CREATE kommt ein verifiziert dekodierter Trade-Tail mit fremdem quote_mint. Ergebnis: 99.000.000.000 in `virtual_sol_reserves`, Speicherung in SOL-Spalte, danach SELL `FILLED`.

Spec: `quote_asset_contract`, `mandatory_fee_pipeline.database_contract.component_rule`.

### P1 – Research-Qualifikation weiterhin umgehbar (S04/S10)

`backtest.py:160` prüft Kandidat nur, wenn ungleich None. `run_research` übergibt None ohne positiven Auswahlstatus (`:215`). Das Stress-Gate (`:163–169`) gruppiert nicht nach Quellrun/Dataset und prüft positive CI plus Exposure, aber weder ausreichende Blockzahl noch eindeutige vollständige Szenariomenge. Ein bloßes Zählen kann Duplikate verschiedener Aufnahmen akzeptieren.

Repros: `gate_no_candidate_and_insufficient_stress` ergibt POSITIVE trotz fehlendem Kandidaten und Stressstatus INSUFFICIENT_DATA mit 2 Blöcken. `gate_duplicate_stress_across_recordings` ergibt POSITIVE, obwohl dieselbe Stressdimension nur über mehrere Aufnahmen dupliziert wird. `gate_unknown_entry_exposure` zeigt außerdem, dass unbekannte Entry-Kosten als 0 die vorgelagerte Exposure-Prüfung passieren (Zwischenstatus PENDING_STRESS, noch kein alleiniger Beleg finaler Freigabe).

Der Test-Unlock ist weiterhin ein editierbarer Bool; kein persistierter verbindlicher Freeze mit einmaliger Test-Auswertung und danach neuem Forward-Datensatz. `run_research:198–211` teilt jede Aufnahme einzeln: ein Testfenster der früheren Aufnahme kann zeitlich vor Training der späteren Aufnahme liegen. Die neue Blockfunktion behebt dies nicht.

Spec: `target_selection_and_validation` R2/R4/R6/R7 und qualification_rule; `timeout_and_observation_contract.split_embargo`.

### P1 – Kostenverlust im Pending-Status wird als sicherer Fehlversuch gebucht (S07)

`execution.py:309–324`: Nach ungültiger Fee-Basis wird InvalidQuote in CURVE_UNAVAILABLE mit alten Submit-Netzwerkgebühren umgewandelt. `paper.py:559` markiert APPLIED_AT_MODELED_FILL, statt die Kosten-/Ausführungsunsicherheit zu erhalten.

Repro `invalid_fee_pending`: Fee-Snapshot wird vor Fill RPC_ERROR. Ergebnis: ENTRY_FAILED, 7.000 Lamports belastet, Cost-Evidence MEASURED, keine mögliche Exposure mehr. Fehlende Modellinformation beweist keine tatsächlich fehlgeschlagene Ausführung.

Spec: `mandatory_fee_pipeline.gating`, `shared_strategy_and_execution_contract.paper_realism`, UNKNOWN- und Exposure-Regeln.

### P1 – Stress kann die Ausführungsgrenze überschreiten (S09)

`execution.py:275–304`: Slippage-Prüfung erfolgt vor `_apply_stress`. Nach Erhöhung des Kaufdebets wird nicht erneut geprüft.

Repro `stress_exceeds_slippage_limit`: `max_input=102638274`, gestresstes Debit `109613691`, trotzdem FILLED (300 bps Slippage, 1000 bps separat konfigurierter Stress). Der Regressionstest prüft nur Erzeugung von Stressvarianten und Zufallsverteilung, nicht deren Ausführungssemantik.

Spec: `mandatory_fee_pipeline.no_double_counting` (Slippage als Ausführungsgrenze), gemeinsames Execution-Modell und Research-Stress.

### P1 – kampagnenweiter Wiedereinstiegsschutz fehlt nach Neustart (neu N1)

`paper.py:138` beginnt mit leerem `submitted_mints`; `main.py:open_storage/run` lädt keine kampagnenweite Intent-Sperre zurück.

Repro `campaign_reentry_after_restart`: Zwei StateOwner-/Run-Instanzen mit derselben SQLite-Datei, Kampagne `camp`, gleichem Mint: zwei OPEN-Positionen und zwei eingereichte BUYs. Der zweite Run erhält erneut das volle konfigurierte Startkapital, während die frühere Exposure in der DB verbleibt. Unabhängige Kampagnen dürfen getrennt sein; diese Reproduktion verwendet ausdrücklich dieselbe Kampagne.

Spec: `configuration_template.strategy.entry_reentry_policy=ONE_SUBMITTED_ENTRY_PER_MINT_PER_CAMPAIGN`; Portfolio-/Restart-Regeln.

### P1 – Writer kann beim Herunterfahren endlos FK-Batches wiederholen (neu N2)

`storage.py:204–221`, `:254`: Ein Batch mit dauerhaft fehlendem FK-Ziel bleibt als held-Batch erhalten. Bei Stop und leerer Queue fehlt terminale Fehlerbehandlung/Abbruch; die Schleife wartet dann nicht mehr. `stop()` prüft nach join nicht, ob der Thread noch lebt.

Repro `writer_orphan_fk_shutdown`: Ein Kinddatensatz ohne eintreffendes Elternobjekt, Stop. Nach 50-ms-Join lebt der Writer weiter, über 11.000 Wiederholungen, `disk_error=false`, kein Fehlercallback, Queue leer. Die exakte Wiederholungszahl ist laufzeitabhängig. Die Anwendung kann danach mit noch aktivem Writer weiterarbeiten bzw. die Verbindung schließen. Ein verlorener Elternrecord bei Backpressure ist ein relevanter Auslöser.

Spec: `storage.on_disk_error`, kontrolliertes Flush/Shutdown, `reliability_and_resource_limits.position_safety`.

### P2 – gespeicherter PARTIAL→COMPLETE-Upgrade fehlt (S03)

`market.py:453–462` korrigiert RAM. `storage.py:273` verwendet INSERT OR IGNORE für `market_events`, weshalb die spätere vollständige Beobachtung das ursprüngliche Event nicht aktualisiert.

Repro `partial_persisted`: 1 Position, aber CREATE `actionable_mono_ns=NULL`, `winner_feed_id=existing_rpc` vom partiellen Feed. D03, Feed-Race-Latenzen und Research-Metadaten widersprechen dem tatsächlichen Entry.

Spec: `core_design.winner_definition`, Provider-Beobachtungen und persistente actionable-/winner-Timestamps.

### P2 – Kostenprovenienz widerspricht den gebuchten Beträgen (S07/S14)

`paper.py:556–571` aktualisiert Snapshot-IDs und einige Protokollkosten, nicht die alten Base-/Priority-/Network-/Rent-Felder des Attempts.

Repro `fee_provenance`: neue Network-Snapshot-ID, Attempt 7.000 Lamports, Fill-Quote und Position 11.000. Zweites Repro mit 20.000 bestätigt denselben Fehler. Reports summieren Attempt-Spalten und können deshalb falsche Gebührenaufteilungen liefern.

Spec: `mandatory_fee_pipeline.database_contract.attempt_cost_fields/immutability/report_additions`.

### P2 – Dataset-/Variant-Provenienz noch unvollständig (S06/S09)

`replay.py:61–69` vertraut gespeicherten `raw_payload_hash`/`snapshot_hash`, liest aber tatsächlich separate veränderbare Inhalte. Keine Integritätsvalidierung. `cfg.protocol.decoder_version` wird einbezogen, nicht das vollständige angewandte Decoder-Layout. `variant_config` in `backtest.py:102` enthält Stressparameter nicht.

Repros: Fee-Content von 100 auf 1000 bps geändert → Loader verwendet 1000, Dataset-Hash bleibt gleich. Decoder-Pinning geändert → Runtime-Hash verschieden, Dataset-Hash identisch. Unterschiedliche Fee-/Adverse-/Reject-Stressvarianten → identische Runtime-, Cost- und Quote-Hashes. Ein Content-/Hash-Mismatch muss mindestens erkannt und abgelehnt werden; gültige Decoder-Korrekturen brauchen neue Dataset-Versionen.

Spec: R2/R3, `mandatory_fee_pipeline.database_contract`, `legacy_feature_contract.comparability`.

### P2 – Feature-Historie und Reports bleiben fehlerhaft (S11/S13)

- `paper.py:729`: V1 nutzt aktuellen Feedzustand für frühere Cutoffs. Repro: 1-ms-Cutoff, Disconnect erst bei 1,2 s → V1 gap=true, V2 gap=false.
- `market.py:519`: Trade-Log stoppt bei konfigurierter Kapazität. Repro mit Limit 2 und drei Trades: 1-s-Snapshot buy_count=2, Qualität trotzdem OK. Fehlende Daten müssen als solche ausgewiesen werden.
- `storage.py:280` serialisiert Feature-Dicts alphabetisch, entgegen festgelegter Reihenfolge.
- `reporting.py:129–151`: Utilization teilt Haltedauer durch Zeitraum zwischen erstem und letztem Marktframe, nicht tatsächliches Run-Fenster/Kapitalbelegung. Repro ergibt 5.0 schon mit einem Slot. Mit mehreren Positionen fehlt zusätzliche Normalisierung.
- `backtest.py:126`: fehlgeschlagene Entries landen alle im Block 0 statt ihrem Decision-Zeitblock.
- `reporting.py:198`: daily_pnl bleibt ausschließlich dem Starttag zugeordnet; im Nutzerbericht selbst als offen genannt.

Spec: `legacy_feature_contract.compute_schedule/missing_inputs/preservation`, Economic-Report-Pflichtwerte und R4/uncertainty.

### P2 – RAM/Hot Path und operative Limits (S16, neu N3)

`paper.py:767` schützt alle `position_by_mint`-Einträge inklusive abgeschlossener Positionen. `occupied_slots` (`:208`) läuft für jeden neuen Entry über die gesamte Positionshistorie. Adapter behält alte Submissions. Repro: max_tracked_mints=1, drei abgeschlossene Positionen, nach Eviction weiterhin drei Tracker und sechs Adapter-Submissions.

Weitere statisch belegte operative Hardcodes: Fee-History 4096/2048 (`paper.py:166`), Writer-Telemetrie 10000/5000 und 50-ms-Locked-Wait/30-s-Shutdown (`storage.py`), Research-10%-Auswahlregel (`backtest.py:181`). `rpc_http.max_requests_per_minute`, `liveness_check_interval_ms`, `position_state_stale_ms` werden nicht als aktive Budgets/Wächter durchgesetzt. Der neue Test verbietet nur vier konkrete Textfragmente und beweist daher keine Abwesenheit von Hardcodes.

Vor BUY befindet sich außerdem das neue Tail-Prefix-Hashing in `market.py:184`. Seine Kosten wurden hier nicht profiliert; keine quantitative Latenzbehauptung. Primär relevant ist das mit der Historie wachsende Durchlaufen der Positionsdaten.

Spec: `minimal_environment_contract.no_hidden_parameters`, `reliability_and_resource_limits.bounded_memory/free_tier_budget`, `core_design.hot_path/outside_hot_path`.

### P2 – Global-Verifikation, doctor und aktuelle IDL (S14/S15/IDL)

- `fees.py:125`: überspringt 8 Bytes ohne Discriminator-Prüfung. Repro `wrong_global_discriminator`: Payload mit `WRONGDIS`, korrektem Owner und plausiblen Feldern wird nach VERIFIED-Layoutkennzeichnung VALID/MEASURED. Der Default UNVERIFIED schützt aktuell vor Handel, behebt aber nicht die fehlende Prüfung nach Pinning.
- `fees.py:58`: Fee-Probe enthält nur ComputeBudget-Instruktionen. Die Spec fordert Validierung der konkreten konfigurierten BUY-/SELL-Message-Form.
- `main.py:276`: D04 fragt SELL `actor=creator`, aber SELL-Events speichern `creator=NULL`. Repro: Runtime erkennt Creator-Sell, D04-SQL zählt 0.
- PumpPortal-Hints sperren die Venue; erforderliche asynchrone On-chain-Nachprüfung ist weiterhin nicht implementiert.
- `next_observed_event_id_after_modeled_fill` wird beim Callback gesetzt, spätere Ereignisse aktualisieren es nicht; das explizite NULL+CENSORED-Audit fehlt.

Die [offizielle öffentliche Pump-IDL](https://raw.githubusercontent.com/pump-fun/pump-public-docs/main/idl/pump.json) wurde gelesen. Der aktuelle CreateEvent-Tail enthält unter anderem Token-Programm, Modus-Flags, Quote-Mint und weitere Werte. TradeEvent enthält deutlich mehr Felder als der mitgelieferte Gebühren-Tail-Kandidat, einschließlich variabler Shareholder-Liste und Quote-Reserven. Das bestätigt die offene Decoder-Verifikation; bloßes Umbenennen auf VERIFIED reicht nicht. Öffentliche IDL-Prüfung braucht keine privaten Keys. Tatsächliche Mainnet-Samples/Accountwerte bleiben separat zu verifizieren.

## Evidenzdateien und Wiederholung

Aus dem Arbeitsverzeichnis `/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2`:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 audit_r3_20260918/spec_checks/reproduce.py
PYTHONDONTWRITEBYTECODE=1 python3 audit_r3_20260918/test_checks/reproduce_execution.py
PYTHONDONTWRITEBYTECODE=1 python3 audit_r3_20260918/architecture_checks/reproduce_architecture.py
PYTHONDONTWRITEBYTECODE=1 python3 audit_r3_20260918/orchestrator_checks/reproduce.py
```

- `test_checks/baseline.txt`: vollständiger 78-Test-Lauf.
- `spec_checks/results.json`: Research-/Statistik-/Freeze-/Frische-Gegenbeispiele.
- `test_checks/execution_results.json`: Ausführung, Gebühren, Fremd-Quote und Persistenz.
- `architecture_checks/results.json`: Reorg, Decode-Coverage, Features, Retention und Writer-Shutdown.
- `orchestrator_checks/counterexamples.json`: Discriminator, Creator-Sell-Zählung und kampagnenweiter Neustartschutz; der zusätzliche Versuch zu unbekannten Quote-Trades in DB-SOL-Spalten war negativ und wird ausdrücklich nicht als Fehler behauptet.
- `orchestrator_checks/cli_results.json`: tatsächliche CLI-Exitcodes und Artefakte.
- `source_manifest.json`: SHA256 je ursprünglicher Datei.

## Nächste Arbeitsrunde

Zuerst P1-Ausführungs-/Datenintegrität: Reservefrische, Konflikt/Reorg/Decode-Coverage, Fremd-Quote und unbekannte Pending-Kosten. Danach Writer-Shutdown/Kampagnen-Neustart, dann Research-Freeze/Gates/Stress. Die neuen Gegenbeispiele müssen echte Regressionstests werden, anschließend vollständige Suite plus erneute unabhängige Reviews. Bis dahin keine technische oder wirtschaftliche Abnahme.
