# Unabhängiger SPEC REVIEW – 18.09.2026

**Ergebnis: nicht spec-konform; mehrere P1-Blocker.** Ausschließlich Read-only gegenüber Produktivcode. Keine Kommunikation mit anderen Reviewern, keine Reparaturen. Source of Truth: vollständig gelesene `SPEC_authoritative.json`; aktiver Runtime-Pfad `snapshot/darkbot/` geprüft. Historische Archivdateien sind keine aktive Laufzeit und wurden hier nicht vollständig semantisch neu auditiert; vollständige Testabdeckung ist Aufgabe des separaten TEST REVIEWER. Es wurde keine Netzwerkverbindung und kein Handel ausgelöst.

Originaldateipräfix für alle untenstehenden Dateireferenzen:
`/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/`
Die Zeilen entsprechen der unveränderten Audit-Kopie.

## Deterministische Gegenbeispiele

Ausgeführt mit System-`python3`, ohne neue Pakete:

```
PYTHONDONTWRITEBYTECODE=1 python3 audit_20260918/spec_checks/reproduce_spec_findings.py
PYTHONDONTWRITEBYTECODE=1 python3 audit_20260918/spec_checks/reproduce_research_collision.py
```

Beide Prozesse Exit 0: Hier bedeutet 0 **Fehler erfolgreich reproduziert**, ausdrücklich nicht Abnahme bestanden. Die Skripte enthalten Assertions auf das tatsächlich fehlerhafte Verhalten. Ergebnisse: `spec_checks/results.json`, `spec_checks/research_collision.json`. Insgesamt 12 reproduzierte Gegenbeispielgruppen.

## P1 – blockierend

### S01: Historisch unbekannter Fill wird mit künftigem Zustand rückdatiert

- **Code:** `darkbot/paper.py:667`, insbesondere 676–694.
- **Spec:** `shared_strategy_and_execution_contract.paper_realism[1,3,4,7]`; `fill_evidence_contract.future_annotation`; T20/T49/T51.
- **Befund:** `_reconcile_unknown` nimmt ausdrücklich den ersten State **nach** dem ursprünglich modellierten Fill, berechnet den Fill damit und übergibt anschließend wieder `handle.scheduled_fill_mono_ns` an die Buchung. Neue Reserven beweisen keine vergangene eigene Ausführung und schließen keinen historischen Creator-Sell-Gap.
- **Repro:** BUY +11 ms wegen Coverage-Gap zunächst UNRESOLVED. Reconnect +20 ms, Trade +21 ms. Ergebnis OPEN, `modeled_fill_mono_ns=1000011000000`, `proxy_receive_mono_ns=1000021000000`, `RECONCILED_NEXT_OBSERVED_STATE`. Quelle ist 10 ms jünger als rückdatierter Fill.
- **Auswirkung:** erfundene historische Fills, falsche Positionen/Timeout-Ursprünge und PnL; neue Daten ändern frühere Ausführungsbehauptungen.

### S02: Frischegrenze wird für OBSERVED-Quotes vollständig umgangen

- **Code:** `darkbot/execution.py:171`, 209, 306; `darkbot/paper.py:385`.
- **Spec:** `shared_strategy_and_execution_contract.paper_realism[4]`; `configuration_template.execution.quote_max_age_ms`; `reliability_and_resource_limits.position_safety`; T16/T32.
- **Befund:** Die Reserve-Altersprüfung gilt nur bei `coverage != OBSERVED`. `_quote_stale` misst zudem Alter des soeben erstellten Quote-Objekts statt Alter seines Reserve-Inputs.
- **Repro:** Ein CREATE, BUY, danach kein weiterer Marktstate; 590 Sekunden später wird TIMEOUT SELL als FILLED gebucht, `proxy_age_ns=590000000000`, obwohl `proxy_state_max_age_ms=250`. Kosten werden immerhin `MEASURED_STALE` markiert; das legitimiert keinen Fill aus veralteten Reserven.
- **Auswirkung:** unrealistische erfolgreiche Verkäufe nach langen Datenpausen.

### S03: PARTIAL-Erstbeobachtung verhindert spätere vollständige Handlungsfähigkeit

- **Code:** `darkbot/market.py:389`–425.
- **Spec:** `core_design.winner_definition`; `core_design.dedup_and_order.partial_transaction`; T02.
- **Befund:** PARTIAL-Envelope setzt bereits `seen_keys` und `create_event_id`. Vollständige Zweitquelle wird danach vor jeder Nachvalidierung als DUPLICATE verworfen.
- **Repro:** CREATE mit zusätzlichem unbekanntem Pump-Event, anschließend vollständige Kopie derselben Signatur über zweiten Feed: PARTIAL/VALID und COMPLETE/DUPLICATE, **0 Positionen**, kein Entry aus vollständiger Quelle.
- **Auswirkung:** Ein langsamer vollständiger Feed kann das frühe unvollständige Signal nie ergänzen. Zusätzlich mutiert PARTIAL-Code Reservezustände exponierter Positionen, obwohl atomare Vollständigkeit fehlt.

### S04: Positive Qualifikation trotz fehlendem Forward-/Stress-Nachweis

- **Code:** `darkbot/reporting.py:275`–304; `darkbot/backtest.py:104`–132.
- **Spec:** `target_selection_and_validation.qualification_rule[0..6]`, Stages R6/R7; `required_cli_contract.commands[3]`.
- **Befund A:** `Reporter._qualify` kennt weder Holdout-/Forward-Status noch Policy-Freeze, Testverbrauch oder Stressresultate. 30 positive Blöcke + Nichtnull-Latenz reichen für POSITIVE_UNDER_MODEL auf einem beliebigen Lauf.
- **Repro A:** Direkter Aufruf mit 30 positiven Blöcken, vollständigen Kosten und ohne irgendeinen Forward-/Stressnachweis liefert POSITIVE_UNDER_MODEL.
- **Befund B:** `apply_stress_gate` prüft nur `ci_low > 0`; Status und offene Exposures der Stressgeschwister werden ignoriert. `variant_status` erhält überhaupt keine Coverage-/Kosten-/Internal-error-Gates.
- **Repro B:** Basis + Stressgeschwister `UNQUALIFIED_DATA_UNRESOLVED_EXPOSURE`, `unresolved=1`, aber positive CI-Untergrenze → Basis POSITIVE_UNDER_MODEL.
- **Weiterer Codebeleg:** Testfreigabe ist nur ein editierbares Boolean (backtest.py:165), wertet dann wieder alle Varianten aus; kein persistierter Freeze-/Einmalverbrauchsvertrag.

### S05: Research mehrerer Aufnahmen kollidiert in derselben run_id

- **Code:** `darkbot/backtest.py:81`, 85–88, 158–172; `darkbot/replay.py:100`; `darkbot/storage.py:273`–280.
- **Spec:** `database_specification.identity_scope`; `target_selection_and_validation.stages[R2,R4,R5]`.
- **Befund:** Variantenlauf-ID enthält Kampagne/Split/Variante, aber keine Quellaufnahme. Schleife über mehrere PAPER-Runs erzeugt dieselben IDs. `INSERT OR IGNORE` behält erste Run-Metadaten, Position-Upserts überschreiben Positionen mit Werten des zweiten Runs.
- **Repro:** source1/Mint A und source2/Mint B, gleiche Variante. Beide Ergebnisse heißen `research_camp_train_same`. DB enthält einen Research-Run mit `parent_run_id=source1`, dessen Position jetzt Mint B enthält.
- **Auswirkung:** Historie/Provenienz vermischt; Ergebnis nicht belastbar reproduzierbar. Zusätzlich werden pro Run eigene 60/20/20-Splits gebildet statt kampagnenweit zeitlich getrennter Session-/Tagesblöcke.

### S06: Dataset-Freeze bindet weder Gebühren noch Timing/Qualität vollständig

- **Code:** `darkbot/replay.py:60`–62; `darkbot/backtest.py:173`.
- **Spec:** `target_selection_and_validation.stages[R2]`; `database_specification.identity_scope`; `mandatory_fee_pipeline.gating.replay`.
- **Befund:** Hash enthält Raw-Payload-Hashliste, Run-ID und Decoder-/Coverage-Namen. Fee-Snapshots, arrival-Zeitwerte, Callback-Schedule, Gap-Intervalle, Run-Ende, Config-/Policy-Inhalt sind nicht gebunden.
- **Repro:** `normalized_fee_components` des historischen Protokoll-Snapshots verändert: neuer ReplayInput hat identischen Dataset-Hash.
- **Auswirkung:** Wirtschaftlich verschiedene Datensätze gelten als derselbe eingefrorene Input; Änderungen können unbemerkt Researchresultate verändern.

### S07: Zum Fill bereits bekannte neue Gebühren werden ignoriert

- **Code:** `darkbot/paper.py:519`–524; Attempt-Basis wird bei Submit eingefroren (367/480).
- **Spec:** `mandatory_fee_pipeline.startup_and_refresh[0..2]`, `database_contract.immutability`, `gating.replay`; sizing contract und T27/T30.
- **Befund:** `_on_outcome` berechnet den gesamten tatsächlich modellierten Swap mit `meta['basis']` vom Submit. Neue Protokollgebühr zwischen Submit und Fill wird nie angewandt.
- **Repro:** Bei Submit 100 bps, +5 ms neuer gültiger Snapshot 1000 bps, Fill +11 ms: FILLED zu 983700 Lamports Protokollgebühr mit alter Fee-ID. Korrekte neue Gebühren würden die konfigurierte 300-bps-Maxkosten-Grenze überschreiten.
- **Auswirkung:** Fills, die unter gültiger Gebührenregel scheitern müssten, und falsches PnL. Netzwerkgebühr kann submissionsgebunden sein; das rechtfertigt nicht eingefrorene Protokollgebühren.

### S08: Nicht unterstütztes Tokenprogramm wird handelbar

- **Code:** `darkbot/market.py:190`–215; `darkbot/strategy.py:32`; `darkbot/execution.py:159`; `darkbot/fees.py:196`.
- **Spec:** `mandatory_fee_pipeline.startup_and_refresh[1,6]`; T25/T29; `ultra_low_latency_contract.minimal_safety_checks[0]`.
- **Befund:** Tokenprogramm wird dekodiert/gespeichert, jedoch weder Admission noch QuoteEngine blockieren Token-2022 oder unbekannte Programme. Rent verwendet stets die konfigurierte SPL-Accountgröße.
- **Repro:** Verifiziertes Testlayout mit WSOL-Quote und Token-2022; Config `token_2022_supported=false`; trotzdem Position OPEN und regulärer SPL-Rent.
- **Auswirkung:** Angeblich unterstützte Fills/Kosten auf ausdrücklich unimplementiertem Accounttyp.

### S09: Konfigurierbare Kosten-/Stressparameter sind teilweise wirkungslos

- **Code:** `darkbot/execution.py:191`, 229; `darkbot/config.py:87`,105,115; `darkbot/backtest.py:62`.
- **Spec:** `instruction_to_implementing_ai.working_rules[1,3]`; `mandatory_fee_pipeline.gating.unknown_not_zero`; `target_selection_and_validation.stress_design`; `configuration_template.research.execution_stress`.
- **Befund:** `irrecoverable_account_cost_lamports` wird akzeptiert, aber beide Quotes setzen unabhängig davon `irrecoverable_cost_lamports=0`. `conservative_exit_reject_rate` wird akzeptiert/berichtet, jedoch nirgends ausgeführt. Separate `entry_delay_ms` und `exit_delay_ms`-Listen werden nicht zur Szenariobildung benutzt; ausschließlich `joint_delay_scenarios`.
- **Repro Delay:** Tatsächliche Variantenpaare sind nur (0,0),(5,5),(25,25),(100,100),(250,250),(500,500), kein einziger isolierter Entry-/Exit-Test trotz separater Arrays.
- **Auswirkung:** Fehlkosten-/Failure-Stress fehlt, deklarierte Config überschätzt tatsächlich geprüfte Robustheit.

### S10: Zeitliche Research-Splits schützen Abhängigkeiten/ungeklärte Positionen nicht

- **Code:** `darkbot/backtest.py:40`–56, 90–94, 158–166.
- **Spec:** `target_selection_and_validation.stages[R4]`; `timeout_and_observation_contract.split_embargo`; `target_selection_and_validation.uncertainty`.
- **Befund (statisch):** Split anhand Anzahl CREATEs pro Run, nicht ganzer Session-/Tagesblöcke. Purge prüft ausschließlich Startzeit innerhalb fester Embargobreite. Tatsächlich weit länger offene/UNRESOLVED-Exposures werden nicht zum vollständigen grenzübergreifenden Purging verwendet. Block-IDs aus monotonic/HOUR sind keine begründeten unabhängigen Session-/Tagesblöcke.
- **Auswirkung:** abhängige Fälle über Train/Validation/Test hinweg, unberechtigte Unabhängigkeit und potenziell zu enge CI.

## P2 – erhebliche Spec-Lücken

### S11: Feature-Aufnahme selektiert Entries und verändert historische Sicht

- **Code:** `darkbot/paper.py:317`–327 vs 378; 715–735; `darkbot/storage.py:282`; `darkbot/config.py:27`.
- **Spec:** `legacy_feature_contract.compute_schedule`, `preservation`, `new_version.other_definitions`; `database_specification.tables.feature_snapshots.rule`.
- **Repro:** Zwei gültige CREATEs, zweiter wegen SLOT_LIMIT übersprungen. Nach allen Featuredeadlines existieren Features ausschließlich für ersten Mint. Forschung verliert gerade kapitalbedingt ausgeschlossene Launches.
- **Weitere belegte Abweichungen:** Für alle früheren Cutoffs wird Coverage-Zustand zum späteren Rechenzeitpunkt übernommen (`coverage = st.coverage_state.value`), V1-Gap basiert ebenfalls auf aktuellem `any(feed_ready)` statt historischer V1-Sicht. Legacy-Researchbetrag wird durch aktuellen Strategy-Entrybetrag ersetzt. Persistenz serialisiert Feature-JSON alphabetisch; reproduzierte Schlüsselanfänge `buy_count,buy_volume_sol,buyer_volume_concentration` statt verbindlich `buy_count,sell_count,buy_volume_sol`.

### S12: Fremdwährungsreserven stehen weiterhin in SOL-Spalten

- **Code:** `darkbot/paper.py:269`–279; `darkbot/market.py:266`–272.
- **Spec:** `quote_asset_contract.storage`.
- **Repro:** Explizit fremde Quote → UNSUPPORTED_QUOTE, korrekt kein Entry; `market_events.virtual_sol_reserves=30000000000` trotzdem gesetzt statt NULL.
- **Auswirkung:** DB-Consumer/Analyse können fremde Quote-Basiseinheiten als SOL interpretieren; Raw-Aufbewahrung allein genügt nicht zur Feldtrennung.

### S13: Statistik lässt gebührenpflichtige Entryfehler aus CI heraus und beschreibt Auslastung falsch

- **Code:** `darkbot/reporting.py:131`, 125–126, 146, 194–198.
- **Spec:** `economic_analysis.accounting.expectancy_per_attempt`; `economic_analysis.required_report_fields`; `portfolio_evaluation_contract.daily_pnl`.
- **Befund (statisch):** Bootstrap filtert `open_mono_ns`; gescheiterte Entries haben keinen erfolgreichen Fill und fallen damit aus dem angeblichen Intent-PnL-CI. Punktmittel enthält sie hingegen. `capital_utilization_fraction` ist Summe Positionsdauer / Runzeit ohne Kapitalgewichtung oder Slotnormalisierung, kann bei 10 parallelen Positionen bis 10 ergeben. Tages-PnL wird ausdrücklich ausschließlich dem Starttag zugeordnet. Schlusskurs-Drawdown ersetzt keinen Equityverlauf mit offenen Beständen.

### S14: Quote-/Kosten-/Fill-Provenienz unvollständig

- **Code:** `darkbot/paper.py:358`, 473, 519–539; `darkbot/execution.py:122`; `darkbot/fees.py:58`–68, 125; `darkbot/paper.py:525`.
- **Spec:** `mandatory_fee_pipeline.database_contract`; `mandatory_fee_pipeline.startup_and_refresh[0,3]`; `fill_evidence_contract.proxy_fields`, `future_annotation`.
- **Befund (statisch):** CostBasis enthält vier Snapshot-IDs, Intent/Attempt/Quote referenzieren nur Protocol-ID. Tatsächliche Fill-Quote wird nicht als eigener Snapshot persistiert (Attempt behält Decision-Quote-ID). Network-Fee-Probe besteht nur aus ComputeBudget-Instruktionen, nicht konkret konfigurierter BUY/SELL-Nachricht. Global-Decoder überspringt die ersten 8 Bytes ohne Account-Discriminatorprüfung, Owner/Plausibilität genügen nach einem `VERIFIED*`-Label. `next_observed_event_id_after_modeled_fill` wird nur beim Callback bestimmt und bei erst später eintreffendem Folgeevent nicht ergänzt; NULL ohne finale Audit-Zensierungskennzeichnung.

### S15: Coverage, processed-Konflikte und Migration-Nachprüfung bleiben unvollständig

- **Code:** `darkbot/market.py:389`–402, 497–504; `darkbot/paper.py:193`–215; `darkbot/main.py:291`.
- **Spec:** `core_design.dedup_and_order.out_of_order/reorgs`; `provider_coverage_contract.definition/scope`; `migration_contract.signals[2]`; T15/T48/T52.
- **Befund (statisch):** Konflikt gleicher Event-ID wird ohne Payload-/Reservevergleich als DUPLICATE behandelt. Unterschiedliche Transaktionen im gleichen Slot überschreiben schlicht nach Empfang. `quote_uncertain` wird nie gesetzt/geprüft. Failed-TX ignoriert frühere processed-Zustände statt sie als invalidiert zu kennzeichnen. Feed-Coverage beruht im Wesentlichen auf ACK/health und deklariertem `provides`, Decode-Lücken erzeugen keinen historischen Gap. PumpPortal-Migrationshinweis setzt Sperre korrekt, die vorgeschriebene asynchrone Nachprüfung fehlt.
- **D04-Zähler:** SELL-Events persistieren keinen Creator (`ev.creator` ist bei Trades None); SQL `event_type='SELL' AND actor=creator` zählt deshalb reguläre Creator-Sells nicht.

### S16: Betriebs-Hardcodes und ungenutzte Limits

- **Code:** `darkbot/backtest.py:141` (10%-Nähe bei Target-Auswahl), `darkbot/paper.py:152` (20k/10k Telemetrieretention), `darkbot/storage.py:142` (256 Headroom), 206 (30s Shutdown), 219/222 (batch*20), 250 (100 Retries/50ms), `darkbot/main.py:241` (512 MiB Disk-Gate), `darkbot/fees.py:238` (200ms Poll-Untergrenze).
- **Spec:** `minimal_environment_contract.no_hidden_parameters`; `instruction_to_implementing_ai.working_rules[1]`.
- **Befund:** Das sind operative Research-/Speicher-/Retry-/Zeitlimits, keine Borsh-/Einheitenkonstanten; nicht JSON-versioniert. `rpc_http.max_requests_per_minute` wird nur validiert, aber nirgends budgetiert. Liveness-/Positions-Staleness-Config hat keinen aktiven Watchdog-Pfad.

### S17: Vollständige automatische Delivery fehlt

- **Code:** `darkbot/main.py:118`–141; `darkbot/cli.py:57`–87.
- **Spec:** `economic_analysis.automatic_outputs.after_run/on_interrupt`; `complete_delivery_contract.first_paper_delivery/reports`.
- **Befund (statisch):** PAPER-Lifecycle schließt DB und gibt Statusobjekt zurück, erzeugt aber keine `report.json`, `summary.md`, `positions.csv`, `latency_by_feed.csv` automatisch. Diese entstehen nur im separaten Report-Befehl, der standardmäßig nur den letzten PAPER-Run einer Kampagne auswertet.

## Externe Evidenz – getrennt von Codefehlern

- Kein aktueller Mainnet-Lauf / keine ≥1000 CREATEs / kein unberührter Forward-Test wurde hier ausgeführt. Das sind offene Step-5/6-Gates, keine bestandenen Prüfungen.
- Pinning tatsächlicher aktueller offizieller IDL, Create/create_v2/Tokenprogramm-Fixtures, Global-Layout/Rundung, reale Kosten und Feed-Verfügbarkeit konnten offline nicht belegt werden. Fail-closed für unverifiziertes Global-Layout ist korrekt und bleibt erforderlich.
- Die vorhandenen Fixtures sind synthetisch durch denselben Encoder/Decoder erzeugt; sie ersetzen keine unabhängige verifizierte Protokollevidenz.
- Positive Aspekte: LIVE-Broadcast-Sperre vorhanden; gemeinsamer StateOwner für PAPER/Replay/Research; kein normaler Stop; Retry-Backoff grundsätzlich begrenzt; Raw-FK-Schema und nicht splittbare RecordGroup vorhanden; Run-Ende wird für Replay berücksichtigt. Diese Aspekte neutralisieren die oben reproduzierten Gegenbeispiele nicht.

## Empfehlung für gemeinsame Priorisierung

Zuerst S01/S02/S03 (Kausalität/Quote/Dedup), dann S04–S10 (Kosten/Qualifikation/Research), anschließend S11–S17 (Datenvertrag/Betrieb/Delivery). Nach Reparaturen erneute unabhängige Review-Runde und Gegenbeispiele als **korrektheitsorientierte** Regressionstests invertieren. Dieser Audit hat nichts repariert.
