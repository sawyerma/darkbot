# Project_0 – Boot-Audit, frei wählbare Aufnahme, Backtest und Optimierung

**Verbindlicher SOLL-Vertrag v1.0 (2026-10-17).** Ergänzt [Frontend Master-Phasenplan V1.3](FRONTEND_PHASENPLAN.md), [reale Code-/Variablenmatrix](PROJECT_0_CODE_MAP.md) und [Control-Protokoll](GATEWAY_WIRE_PROTOCOL_V1_3.md). Die angeführten neuen Typen/Kommandos sind **nicht implementiert**. Projektweite Autorität: [PHASENPLAN](../../project_0/docs/PHASENPLAN.md), [STATUS](../../project_0/docs/STATUS.md), [MEASUREMENT_LEDGER](../../project_0/docs/MEASUREMENT_LEDGER.md). FREE_ONLY, JFI, PERSIST, FE und LIVE-Sperren bleiben unverändert.

## 1. End-to-End-Produktablauf

`Öffnen → System starten → Boot-Audit → READY oder BLOCKED → Run-Konfiguration → Preflight → Erfassen → Segment Seal → Verify → Backtest → Report → Optimieren → Baseline-Vergleich → Offline-Profil speichern`.

Jeder Abschnitt hat eine eigene serverseitige Job-ID, Typ-/Versionsidentität, Status, Ergebnis-/Fehlercode und nachvollziehbare Journal-Evidence. Ein Klick startet keinen versteckten nachfolgenden Mainnet-/Paid-/LIVE-Schritt. Browser kann schließen; Run- und Offline-Jobs bleiben unter dem Backend-Supervisor erhalten. Optimieren kann wiederholt auf denselben Daten erfolgen, ohne neue Marktaufnahme.

### User-Interface

1. **Boot & Diagnose:** einmaliger Button `System prüfen`, nach erfolgreicher Prüfung `System startbereit`; jede Prüfung mit PASS/WARN/BLOCKED/UNKNOWN, belegter Soll-/Ist-Identität, Slot/Zeit/Providerklasse und Ursache, `Erneut prüfen`, keine pauschale grüne Ampel bei kritischem UNKNOWN.
2. **Aufnahme:** Modus `Feste Dauer` mit frei eintippbarem positivem Wert und Einheit `s/m/h/d`, alternativ `Bis manuell gestoppt`. Die vorhandenen Werte 30s/60s/10m/1h/4h/24h/48h sind **nur Presets**. Quelle `logs|transactions`, Qualifikationsgrad, Profil, effective config und Ressourcenbudget sichtbar. Startknopf nur nach bestandenem Gate.
3. **Live-Monitoring:** beobachtete/abgedeckte Zeit, ingest, Slot, WSS/429/Gaps, sechs Marker, Storage Bytes/s, Segment-Status, RAM/SSD, Provider-/Strict-Budget, Shadow/UNKNOWN, aktuelle Readiness. `Pause` ist **nur UI-Anzeigepause**, nicht verlustloses Pausieren einer Live-Chain.
4. **Runs & Daten:** `Rohdaten öffnen`, `Verifizieren`, `Auswerten`, `Backtest starten`, `Optimieren`, `Profile vergleichen`, `Report exportieren`. Jeder Button benennt Scope (Offline/Netzwerk) und geschätztes zulässiges Job-Budget; API-Result ist nur eine Job-ID, Erfolg erst nach Status/Evidence.
5. **Analyseansicht:** Evidence-Qualität + Umfang, Run-/Protokoll-/Hash-Identitäten, Timeline und Gaps, Marker/4 Klassen, Funding/Capital, Brutto-/Shadow-/Realized-Ökonomie, Race/Winner-Labels, Verlustgründe, Drilldown zur signierten Chain-/Journalquelle.
6. **Optimizer:** Parameterraum auswählbar, aber Eigentümer ist Rust. Sweep-Budget, Kapital-/Funding-/Fee-/Exit-/Threshold-Szenarien, Baseline und Holdout wählen. Ergebnis mit Ranking, Unsicherheitsintervallen, UNKNOWNs, Robustheit, Drawdown (nur wenn echte Loss-/Exposure-Basis verfügbar), Profit/Kapital und einer qualifizierungsfähigen Profilempfehlung. Ein Klick `Profil übernehmen` bedeutet nur Offline/Shadow-Kandidat – **kein Echtgeld-Go**.

## 2. BOOT-AUDIT vor jedem Start: alles Relevante prüfen

Der Audit ist ein verbindlicher Backend-Gate, kein optionaler UI-Selbsttest. Er besitzt `AuditSpec` (verbindliche Erwartungen), `AuditResult` (Ist-Nachweise), `CheckResult {id,criticality,state,expected_digest?,observed_digest?,source,slot?,observed_at,reason}` und einen gültigkeitsbegrenzten `ReadinessToken`, der an **Konfiguration, Binärdatei, Programmintegrität, Provider, Cluster und Run-Vorhaben** gebunden ist. Token wird bei Start erneut geprüft, bei Materialänderung ungültig. Diese Typen sind SOLL.

**Prüfmatrix (read-only, soweit technisch überprüfbar):**

| ID | Komponente | Konkrete Prüfung | Bei Fehler |
|---|---|---|---|
| BOOT-01 | Software/Schema | Binary-Hash, HEAD-/Konfigurationshash, Journal-/Decoder-/Runtime-/Control-Protokollversion, erwartete ABI/Discriminators | BLOCKED bei inkompatibler kritischer Version |
| BOOT-02 | Host/Prozesse | PID/Single-Owner-Lock, keine Stale-Writer-/Doppelstarts, Zeitbasis, RAM-/Disk-Reserve, Writer-Probe, Journal Seal-/Recovery-Status | BLOCKED oder kontrollierte Recovery |
| BOOT-03 | Cluster | RPC `getGenesisHash`, `getVersion`, Slot/Commitment/Health, erwarteter Cluster, aktuelle/finalisierte Referenz, Zeitstempel | Falscher Cluster oder widersprüchliche Antworten = BLOCKED |
| BOOT-04 | WSS/Provider | DNS/TLS/WSS Connect, Subscription ACK, Appframe-/Slot-Liveness, HTTP RPC Read, 403/429, RPS/Archive/FREE_ONLY, Quota/Timebudget | Quelle nicht verfügbar/Scope unbekannt = BLOCKED für erforderlichen Capture |
| BOOT-05 | Programmidentität | Aktuelle Marginfi-/einbezogene Programm-IDs, Besitzer, ProgramData-/UpgradeableLoader-Metadaten, ausführbarer Zustand, Bytecode-/Deployment-Hash **sofern nachweisbar** gegen gepinnte Erwartung | `PROGRAM_IDENTITY_MISMATCH` / `PROGRAM_VERSION_UNKNOWN`, kein stiller Re-Pin |
| BOOT-06 | Fachlicher State | Marginfi Group/Bank/Mint/Oracle/Pyth/Token-Program/Adapter/Public-Address-Beziehungen, Owner-/DataLength-/Discriminator-/Schema-/Authority-Regeln, Gültigkeit über gemeinsam konsistenten Slot | unzulässige/veränderte Rolle = QUARANTINED/BLOCKED |
| BOOT-07 | Dynamische Adressen | Erwartete konstante Program-IDs unterscheiden von **dynamischen** PDAs/Router-Pools/Banklisten/Oracle-Accounts; dynamische Werte aus kanonischem Parent-State neu ableiten, dann Beziehung & Owner & Seeds prüfen. Keine blind hardcodierte Adresse; keine automatische Akzeptanz neuer Programm-ID | mismatch/invalide Seeds/Besitzer = BLOCKED |
| BOOT-08 | Markt-/Preisquellen | Oracle-Publisher/Feed-Identität, Datenalter, Confidence-/Staleness-Gates, Bank-/Quote-Mapping; Quotes als flüchtige Werte, nicht pinnen | veraltete/inkonsistente Oracle-/Quote-Daten = BLOCKED für abhängige Strategie |
| BOOT-09 | Wallet/Safety | nur öffentliche Wallet-Adresse, gültige Tokenkonten/ATA-Konfiguration, Fee-Reserve, Zugriff auf benötigte Read-only-Accounts, `P0_LIVE_ENABLED=false`, `P0_SUBMISSION_ENABLED=false`; Private Keys niemals über Browser | fehlende ID oder unerwartete Erlaubnis = BLOCKED |
| BOOT-10 | Mess-/Proofumfang | Sechs Marker, vier Klassen, Programfilter/LUT-Scope, Native-Decoder-Parität, Archive-Verifizierbarkeit, erwartete slotweise Coverage | kein falsches FULLY_VERIFIED; schwache Quellen ausdrücklich als PROVISIONAL |
| BOOT-11 | Wirtschaftlichkeit/Execution Readiness | Route-/Funding-/LUT-/Fee-Budget-/Simulationsvoraussetzungen je jeweiligem **Modus**, für Observe keine LIVE-Signer-Prüfung erforderlich | abhängig vom Modus BLOCKED/NOT_APPLICABLE |
| BOOT-12 | Endgültiger Vorstartvergleich | Nach BOOT-01…11 kurz vor Admission alle kritischen Hash-/Cluster-/Programm-/Config-Identitäten erneut gegen Token binden, START nur bei aktueller Qualifikation | TOCTOU = BLOCKED |

**BSC-Lehre:** Eine geänderte Adresse ist nicht automatisch gleich eine Sicherheitsverletzung; entscheidend sind Eigentümer, Seeds, Rolle, On-chain Herkunft, erwartete Bytecodes/Programmversions-Pins und freigegebener Cluster. **Adressklassen getrennt behandeln:** (a) stabile Chain-/Programmidentität fail-closed bei Drift; (b) dynamische PDAs/Bank-/Pool-/Oracle-Beziehungen aus authentischem Zustand ableiten und unabhängig validieren; (c) vergängliche Blockhashes/Leader/Quotes nach Freshness-Gates erneuern. Kein „alles grün“, weil ein HTTP-200 kam.

**Geltungsbereich:** BOOT-Prüfungen, die externe Mainnet-Reads benötigen, werden nur nach ausdrücklicher Netzwerk-/Providerfreigabe ausgeführt. Offline-Dry-Boot prüft lokale Assets/Identitäten und kennzeichnet externe Checks `NOT_RUN` – **kein gültiges Network-READY**. Kritische technische Voraussetzung, die über freien Provider nicht überprüfbar ist, wird nicht still als PASS klassifiziert. Verbindliches Audit-Manifest mit exakten Quellen/Hashes und wiederkehrende Checks im Dauerlauf.

## 3. Frei wählbare Laufzeit einschließlich „unbegrenzt“

**IST-Code:** [`MeasurementPlan::parse_duration`](../../project_0/native/crates/runtime/src/analytics/measurement.rs) akzeptiert strikt `integer + s/m/h`, `MeasurementPlan {duration_seconds:u64,snapshot_offsets_ns:[u64;4],budgets}`, `SNAPSHOT_COUNT=4`. [`runtime_config::duration_seconds`](../../project_0/native/crates/solana/src/runtime_config.rs) akzeptiert ebenso nur endliche `s/m/h`. [`storage/preflight.rs`](../../project_0/native/crates/runtime/src/storage/preflight.rs) berechnet Kapazität aus `Request.duration_s:u64` und erlaubt ohne Langlaufbasis höchstens `CALIBRATION_MAX_S=600`. Ein „infinity“-String ist deshalb **heute ungültig**.

**SOLL ohne bestehende Verhalten zu zerstören:**
- `RunHorizon::Fixed {duration_seconds:u64}` oder `RunHorizon::UntilStopped {checkpoint_seconds:u64,segment_policy,minimum_runway_seconds:u64}`. Kein `u64::MAX` als Pseudo-Unendlich, keine negative/Null-Dauer, keine NaN/Overflow-Werte.
- Zeit-UI: positive Ganzzahl und `s/m/h/d` (Tage serverseitig sicher in Sekunden überführt), oder `Bis Stop`; Schnellwahlen lediglich Eingabehilfen. Für beliebige reale Dauer gelten qualifizierte Ressourcen-/Providerbudgets und maximal sicher repräsentierbarer Zeithorizont. Nicht jedes `u64`-Intervall tatsächlich betrieblich qualifiziert.
- **Fixed:** Bestehender vierteiliger `MeasurementPlan` bleibt für alte Runs/Evidence versioniert lesbar; neue endliche Runs verwenden denselben aktiven Capture-Owner und dieselben exakten Clock-/Snapshot-Regeln, statt eine zweite Zählweise anzulegen.
- **UntilStopped:** rollierende, klar definierte Zeit-/Slotfenster mit kontinuierlichen Window-/Hour- und optional 4-Quadranten-Aggregaten; je Fenster eigene Checkpoint-/Segment-/Verifier-Scopes. Nach Stop finale unvollständige Teilfenster ausweisen. Kein behauptetes `FINAL_48H`, bevor 48h wirklich lückenlos nachgewiesen sind.
- **Datenhaltung:** SK-07 hash-/sequenzverkettete `.p0j`-Segmente, Crash-/Powerloss-Recovery, nie stille Rotation oder Überschreiben ungeprüfter Rohdaten. Unbegrenzter Lauf benötigt begrenzte RAM- und Event-Queues, realistische permanente Writer-Kapazität, definierte Disk-/Provider-Haltkriterien, Retention-/Exportpolicy. **Keine automatische Löschung ungesicherter Rohbeweise** für „endlosen Betrieb“.
- **Preflight:** für Fixed projizierte gesamte Disk-/Verifier-/Provider-Budgets; für UntilStopped *qualifizierte rollierende Mindest-Runway* und periodischer Re-Preflight (aktuelle Datenrate, Speicherreserve, Worker-/Slot-Lag, API-Rate). `UntilStopped` = kein planmäßiger Zeitstopp, **nicht** garantierter Betrieb ohne Speicher, Netzwerk und Quota.
- **Stop/Halt:** manuell STOP → Drain/Seal/Checkpoints; physikalisch unrettbarer Disk-/RAM-/Providerfehler führt zu klarer `ABORTED_INCOMPLETE` und Safety-HALT. Für laufende Analysen nur unveränderliche versiegelte Segmentversionen bereitstellen.

## 4. Ein-Klick-Auswertung / Backtest – reproduzierbare Jobpipeline

**A. Run auswählen:** Listen-/Kalenderansicht mit Run-ID, UTC, Dauer, Scope, Capture-/Segment-Status, real abgedeckte Sekunden, Providerklasse, Program-/Code-Fingerprint, Rohhash, sechs Marker, unaufgelöste Gaps/429. Benutzer sieht sofort, welche Daten verlässlich sind.

**B. „Analysieren“:** Verbindlicher `AnalysisJob` (SOLL) mit `run_ids,segment_root_hashes,query,verification_result_id?,schema/decoder/economics_version,resource_budget,job_id`. Backend liest versiegelte Journale, erstellt bereits fachlich berechnete aggregierte Sichten, ohne neuerliche Live-Aufnahme. Status/Progress/Cancel/Retry via Job-API, niemals CPU-lastige Schleife im Browser.

**C. „Backtest starten“:** `BacktestJob`/historischer as-of Replay; Clock/State/Oracle/Dex-Quote/Health exakt zum damaligen Entscheidungszeitpunkt rekonstruieren, nur Daten verwenden, die damals zur jeweiligen Pipeline-Zeit verfügbar gewesen wären. Keine finale Blockinformation aus der Zukunft zur historischen Gewinner-Vorhersage. Die replayfähige Eventmenge und fehlende Inputs für jede Strategie ausdrücklich ausweisen. Drei getrennte Replay-Stufen:
1. `OBSERVED_MARKET`: reale bekannte Chain-Ereignisse, keine eigene gewonnene TX.
2. `COUNTERFACTUAL_SHADOW`: mit historischen Inputs wirtschaftlich *möglicherweise* ausführbare Chancen; Unbekanntes `UNPROVEN`.
3. `REALIZED_ONCHAIN`: ausschließlich echte eigene, finalisierte Signaturen + Wallet-/Fee-/Inventar-Reconciliation (erst nach gesonderter Echtgeldfreigabe).
Verifikationsstatus und Seen-at-/Available-at-Zeitpunkte gehören zum Ergebnis. History-Replay kann originale verpasste Live-Transportframes nicht wieder herstellen.

**D. „Bericht“:** UI mit sechs unterscheidbaren Bereichen: (1) Evidence-Zusammenfassung/Qualifikation, (2) Source-/Slot-/Gap-/429-Zeitachse mit Drilldown, (3) Marker-/Typenzahlen/Unique Liquidation Episodes/Start-End-Paare, (4) Market-Gross vs Shadow-executable vs Realized-Net, Fee/Rent/Tip/Exit und UNKNOWN, (5) Winner-/Race-/Timing-/Bottleneck-Attribution mit Confidence, (6) Kapital-/Funding-/Strategie-/Zeitsegment-Vergleich.

**E. „Optimieren“:** `OptimizationJob {baseline_profile,run_ids,holdout_plan,param_ranges,objective,constraints,iterations?,seed,proof_ids}` (SOLL); deterministische Sweeps via bestehende Rust-Economics, kein zweiter TypeScript-/Python-Simulator. Primitive Suchmethoden zuerst Grid/Random mit begrenztem Suchbudget; erst mit echten Winner-/Landing-Labels kalibrierte Statistik/BO/GBDT. Risikoorientierte Zielfunktion z.B. *konservativer, nachweisbar realisierbarer Net Edge pro gebundener Kapitaleinheit und Zeit* unter unbekannten Kosten-/Provider-/Liquiditätsgrenzen; keine Modelleffekte als realisierte Profite ausgeben.

**F. „Vergleichen und speichern“:** Tabelle und Kurven für Baseline vs Kandidaten (gleiche Runs, identische Provenienz) mit Kosten je Gelegenheit, Fundingscope, Race, Diversität nach Marktregime, zeitlich unabhängigen Holdouts, Bootstrap-CI, UNKNOWN, Chancequalität und Datenlücken. Reproduzierbarer Report-Hash/Profil-Diff. Speichern erzeugt **Offline/Shadow-Kandidatenprofil**, keine automatische Start-/LIVE-Freigabe.

### Darstellungsrichtlinien

- Übersichts-KPIs: `MARKET_EVENTS`, `UNIQUE_LIQUIDATION_EPISODES`, `ELIGIBLE_OPPORTUNITIES`, `EXECUTABLE_SHADOW_OPPORTUNITIES`, `GROSS_EDGE`, `KNOWN_SHADOW_NET_EDGE`, `REALIZED_NET_PNL`, `UNKNOWN_COUNT`, `COVERAGE`; alle mit Provenienzlabel, Bezugszeit, Einheit und Tooltips.
- Charts: Zeitachse (Events/Coverage/Feedgaps), Kapitalband→Net Edge, Gebühren/Slippage-Sensitivität, Zeitpunkt→gefundene/geblockte Chancen, stacked LossReasons, Profit-vs-Working-Capital; kein aggregiertes Prozent aus inhomogenen Bezugsgrößen.
- Tabellen: jede Liquidation/Signatur/Slot/Bank/Mint/Eventpositions-/Wallet-/CostEvidence per Klick öffnen. Bounded Pages, serverseitige native Abfrage, keine unbounded 48h-Payloads im Browser.
- Verfügbare Auswahl: Reportfilter für alle sechs Marker/vier Klassen/vier Related; separat Replay-/Optimizerparameter. **Anzeige-Filter dürfen Captured Raw, Verifier-Scope und PnL-Truth nicht beeinflussen.**
- Rohwerte und Kosten in lamports/u64/u128/i128 und USD-micro verlustfrei bis an die Darstellung weitergeben. JavaScript-Number darf keine exakten wirtschaftlichen Summen überschreiben.
- `NO_SAMPLE`, `UNKNOWN`, `INSUFFICIENT_SAMPLE`, `UNPROVEN` und `BLOCKED` sind sichtbare fachliche Zustände, **keine** als null oder grün ausgegebenen Werte.

## 5. Konkreter Source-/Var-Anschluss

| Geplante Kontrolle | Reale Quelle | Änderung |
|---|---|---|
| beliebige endliche Dauer | `runtime/src/analytics/measurement.rs::MeasurementPlan::parse_duration`, `solana/src/runtime_config.rs::duration_seconds`, `P0_MEASUREMENT_DURATION` | `s/m/h/d` sicher versioniert; beide Parser konsolidieren |
| nicht terminiert | `MeasurementPlan.duration_seconds`, `snapshot_offsets_ns[4]`, `P0_ECON_DURATION_SECS`, `P0_MARKET_DURATION_SECS` | neuer `RunHorizon::UntilStopped`, rolling planner; bislang **nicht vorhanden** |
| Storage-/Runway | `runtime/src/storage/preflight.rs::Request{duration_s,verify_calls,...}`, `CAPACITY_PREFLIGHT` | neuer rolling Preflight ohne 48h-/unbounded-Scheingröße |
| Boot-Programm-Pins | `runtime/src/market/configuration.rs::MarketSource`, `solana/src/chain.rs`, `runtime/src/market_truth.rs` | typisierter Identity-/ABI-/Account-Audit mit Source- und Slot-Proof; `coverage_verified=false` bleibt ehrlich |
| Betriebskonfiguration | `solana/src/runtime_config.rs::RuntimeConfig/Limits/Resolved::parse/descriptors` | typisierte, tatsächlich geroutete Config; keine konkurrierenden UI-Defaults |
| Provider-/Storage-Status | `market/core.rs::MarketStats`, `storage/writer.rs::StorageStatus`, `storage/preflight.rs::CapacityReport` | Control Snapshots/Deltas und Stale-Watchdog |
| Run/Proof-IDs | `operations/capture/run_store.py`, `operations/verification/measurement_verify.py`, `analytics/journal.rs` | immutable Job-/Result-/Capture-Identitäten |
| Rohmarker/-Klassen | `operations/cli/measurement_contract.py::MARKERS/KINDS/RELATED`, `market/rows.rs` | ausschließlich Viewfilter |
| Offline-Econ | `liquidation/src/economics.rs::CandidateInput/Evaluation/FailedCostMatrix`, `runtime/src/analytics/econ_canonical.rs` | exakte Rust-Replay-/Sweep-/Report-Consumers |
| Kapital-Szenarien | `runtime/src/analytics/econ.rs::CAPITAL_USD`, `P0_ECON_CAPITAL_USD` (bereits per CSV positive u64 Werte, max 32) | frei begrenzte validerte Kapitalvektoren statt nur feste Knöpfe |
| Replay/Optimizer | `runtime/src/bin/p0-econ-replay.rs`, `market_report.py` (historischer Legacy-Consumer) | neue native Offline-Job-/Query-Schicht, keine JSON-Schattenpipeline |

## 6. Neue SOLL-Commands und Control-Endpunkte

~~~bash
p0ctl doctor --scope offline
p0ctl doctor --scope network --profile standard    # nur autorisierter read-only Provider-Check
p0ctl run preflight --duration 75m --capture-source logs
p0ctl run start --duration 75m --profile standard
p0ctl run start --until-stopped --profile standard   # rollierend, erst nach Gate
p0ctl run status --run RUN_ID
p0ctl run stop --run RUN_ID
p0ctl runs list
p0ctl verify --run RUN_ID --mode strict
p0ctl analyze --run RUN_ID --segment-root ROOT_HASH
p0ctl backtest --run RUN_ID --baseline PROFILE_ID
p0ctl optimize --run RUN_ID --baseline PROFILE_ID --search PROFILE_SPACE_ID
p0ctl profiles compare --baseline A --candidate B
~~~

**SOLL browser-control** nach JFI-konformem Transportabgleich: `capabilities` inklusive RunHorizon-Schema, `/project0/diagnostics` und `/project0/boot-audits`, `/project0/preflight`, `/project0/runs`, `/project0/runs/:id/segments`, `/project0/runs/:id/reports`, `/project0/backtest-jobs`, `/project0/optimization-jobs`, `/project0/profiles`. Kontrollierende POSTs brauchen Auth, Useraktion, Scope-/Capacity-/Provider-Gates, Idempotency-Keys; keine öffentlichen Signer- oder LIVE-Routen.

## 7. Abnahmen – Muss-Tests

1. **Boot-Identität:** falscher GenesisHash/ProgramOwner/ProgramData-Pin, driftende statische Programm-ID, falsch abgeleitete dynamische PDA/Oracle-Zuordnung, falsche Decoder-Version, inkompatibler Journalcodec, doppelte Prozesse oder fehlende private Providerrechte → BLOCKED mit exaktem Code und Evidence; authentisch veränderte erlaubte dynamische Adressen nur nach Owner-/Seed-/Parent-State-Beweis.
2. **Boot-TOCTOU:** zwischen Audit und Start geänderte Identität/Version/Provider-Lage invalidiert Token und verbietet Run; im Lauf entdeckter kritischer Drift = Quarantäne/HALT ohne heimliche neue Quelle.
3. **Duration:** `30s`, `75m`, `7d`, `48h`, Overflow/0/negativ/ungültige Einheiten und `UntilStopped`; bestehende vier Snapshot-Marker bei alten endlichen Runs bitkompatibel; kein pseudo-unendliches u64.
4. **Unbounded Run:** segmentübergreifend kontinuierliches Replay/Verifier, Crash/Restart, fehlender RAM/SSD/Provider, dauerhafte Writerstalls, voller Datenträger, zweiter Writer, Stop/Drain/Seal; keine ungesicherte automatische Löschung.
5. **Clickchain:** Run→Analyze→Backtest→Report→Optimize→Compare führt ausschließlich autorisierte Backendjobs aus; Client-Neustart/WS-Gap ändert Jobs nicht; Outputs unveränderlich versioniert.
6. **Backtest-Zeitkonsistenz:** Zukunfts-Oracle, Block-/TX-Landing, spätere Quotes und Winner-Evidenz dürfen keine frühere Entscheidung beeinflussen; nicht rekonstruierbare Inputs=UNKNOWN/UNPROVEN.
7. **Optimizer:** dieselben Run-/Profil-/Version-/Seed-/Holdout-IDs → gleicher Result-Hash; OOS-Misserfolg/UNKNOWN markiert, keine historischen Einzelgewinne zu prognostizierten realisierten Gewinnen.
8. **Unfiltered:** alle 6 Marker, vier fachlichen Klassen, vier Related-Events im Report selektierbar ohne Capture-Hash/Verifizierer-/Economics-Drift.
9. **Sicherheit:** UI kann weder Paid-Fallback noch LIVE/SEND noch Keypair-Werte aktivieren; Grenzen/Rollen und C0-E1-/FE-Phasenplangates unverändert.

**Implementierungspriorität:** Boot-Audit und sichere Control-CLI unmittelbar; endliche freie Laufzeiten ohne Hindernis vor dem qualifizierten 4h-Test. `UntilStopped` wird strukturell jetzt geplant, aber durch SK-07-/Provider-/Storage-Gates aktiviert. Backtest/Optimierung auf denselben gespeicherten Daten danach. Kein neues Redis, kein neues Frontend, kein paralleler Economics-Algorithmus.
