# DarkBot Frontend – Verbindlicher Phasenplan v1.2

**Stand:** 2026-10-17. **Status:** SOLL / Implementierung offen. **Repo:** sawyerma/darkbot.
**Verifizierte Code-Basis:** Project_0 `0e0544d`; danach ist eine ausschließlich BSC-bezogene Änderung `ee94150` auf main erschienen. Für die Umsetzung jeweils aktuellen HEAD prüfen.

**Autorität:** [Project_0 PHASENPLAN](../../project_0/docs/PHASENPLAN.md), [STATUS](../../project_0/docs/STATUS.md), [MEASUREMENT_LEDGER](../../project_0/docs/MEASUREMENT_LEDGER.md). Dieser Frontend-Plan ergänzt FE-00…FE-07, C0-E1, PERSIST, JFI, SK-01…09 und alle FREE_ONLY-/LIVE-Gates; er ersetzt sie nicht.
**Verbindliche visuelle und finanzielle V5-Referenz:** [GRAPHITE_V5_UI_FINANCE_CONTRACT.md](GRAPHITE_V5_UI_FINANCE_CONTRACT.md) – vom Nutzer akzeptierte Graphite-V5-Oberfläche und gemeinsamer Portfolio-/Realized-PnL-Vertrag. **Keine andere V4-/Kachel-/Sidebar-Gestaltung umsetzen.**

**Nachschlagewerke:** [verifizierte Variablen](PROJECT_0_CODE_MAP.md) · [geplanter Control-/API-Vertrag](CONTROL_PROTOCOL.md) · **[verbindliche Boot-/Unbounded-/Backtest-/Optimizer-Spezifikation](STARTUP_BACKTEST_OPTIMIZATION.md)**. Die v1.1-Erweiterung ersetzt feste Laufzeit-Presets durch einen typisierten, frei konfigurierbaren Horizont.

## V0 Design- und Fleet-Spezifikation (2026-10-17)

**Abgenommenes Design als V0 festgelegt:** [Graphite V0](V0_GRAPHITE_FLEET.md) und [implementierbarer React-/Backend-/Speicher-/Refresh-Phasenplan](V0_FLEET_IMPLEMENTATION_PLAN.md). Die frühere Ein-Bot-UX ist hiermit überholt. `Liquidator` verwaltet später N Bot-Instanzen mit **je eigenen** Strategy Profiles, Capital, MinNet, Fees, Concurrency und qualifizierten Liquidationsklassen. Gemeinsamer Ingress/State plus globaler Arbiter, atomare Wallet-Reservation und per-bot Attribution; Captures/Shadow parallel und unfiltered, Offline-Replay ressourcenisoliert. Keine UI-Budgetänderung überschreibt LIVE-/FE-06-/JFI-Gates.

**Wallet-Lock:** Tabstruktur `Balances / Send / Auto-Sweep` und Reihenfolge aller KPI-Zeilen aus V0 sind fest. Bei Liquidator-/Capture-Änderungen **Wallets nicht neu gestalten**. `Overview` und `Wallets` sollen identische `PortfolioFinancialSnapshot`-Kennzahlen anzeigen. Echt persistiert werden Draft-/Profil- und Instanzdaten erst nach Rust-FLEET-03/05, nicht in React localStorage. Aktuell lediglich lokale zustandslose Frontend-Entwürfe.

## 0. Zielarchitektur und Entscheidung

- **Ein Rust-Core.** `p0-longtail` bleibt die einzige fachliche Project_0-Runtime in den vorhandenen vier Crates `common, solana, liquidation, runtime`. State, Decoder, Health, Economics, Winner, Funding, Routing, Signing und Reconciliation niemals im Browser neu implementieren.
- **Eine zentrale Steuerung.** Neues kleines `p0ctl` innerhalb der bestehenden `runtime`-Crate für `doctor/preflight/run start/status/stop/logs/verify/report`, später `optimize/profiles`. Terminal funktioniert unabhängig von React; optionaler lokaler `p0ctl serve`-Supervisor stellt dieselbe typisierte Control-Boundary bereit.
- **Ein vorhandenes Frontend.** `frontend/src/features/project0/Project0Page.tsx` ist derzeit Platzhalter. Bestehendes React-/Vite-/Router-/API-/Realtime-Grundgerüst erweitern, kein zweites temporäres Frontend.
- **Eine Evidence-Wahrheit.** Aktive Rohaufnahme ungefiltert in die versionierten binären `.p0j`-Journale. Nachgeschalteter Verifier, Replay, Renderer und Optimierer konsumieren dieselben versiegelten Daten. Intern ausschließlich native/typisierte/binäre Übergaben gemäß JFI; Browser-Wire-Format nicht als stillschweigende zusätzliche JSON-Ausnahme interpretieren.
- **Keine Redis-Pflicht.** Solange ein lokaler Host mit einem kanonischen State Owner genügt, bestehende bounded Channels und Journale verwenden. Optionaler SQLite-Indexer SK-08 nur offline, nicht im aktiven Trading-/Capture-/Verifier-Pfad.
- **Sicherheitsgrenzen.** Observe-only, FREE_ONLY und Live-/Submission-Sperren lassen sich durch GUI, Profile, API oder Optimierer niemals umgehen. Keine Mainnet-/Paid-/Echtgeldaktionen werden durch dieses Dokument autorisiert.

### Was den ersten 4h-Test blockieren darf

Die notwendige **Terminal-Control-/Lifecycle-Basis**, Kapazitäts-/Providerqualifikation, SK-07-Segmentierung, sechs Marker, Raw-Capture, JFI-/Verifier-/Recovery-Abnahme. **Nicht** erforderlich als Vorbedingung: fertiges React-Dashboard, ML, ein KI-Agent oder die optionale Offline-SQLite-Abfrage.

## 1. Verbindliche Frontend-Struktur

**IST:** `src/app/router.tsx` hat `/project-0`; `src/core/api/client.ts` bietet `apiGet<T>`; `src/core/realtime/RealtimeClient.ts` hat WS-Client/Sequence-Gap-Erkennung; `Project0Page.tsx` ist nur Platzhalter. `vite.config.ts` proxyt Entwicklungsanfragen auf `localhost:8100`, beweist aber keinen vorhandenen Backend-Control-Server.

**SOLL – Dateien erst anlegen, wenn die entsprechende Phase implementiert wird:**

~~~text
frontend/
  docs/
    FRONTEND_PHASENPLAN.md
    PROJECT_0_CODE_MAP.md
    CONTROL_PROTOCOL.md
  src/
    app/{router.tsx,providers.tsx}
    core/api/{client.ts,endpoints.ts,project0.ts}
    core/realtime/{RealtimeClient.ts,protocol.ts,...}
    core/types/{system.ts,project0.ts}
    features/project0/
      Project0Page.tsx
      pages/{ControlCenter,MarketExplorer,Economics,ProfitabilityLab,Profiles}.tsx
      components/{RunStatus,PreflightPanel,StoragePanel,MarkerFilter,...}.tsx
      hooks/{useRunSnapshot,useRunEvents,useReportQuery,...}.ts
    features/{bsc,solana}/
    shared/{layout,ui}/
project_0/native/crates/runtime/src/bin/p0ctl.rs    # NEU, keine zweite Engine
~~~

## 2. Phasen und Tickets

**Statusdefinition:** `OPEN` fehlt; `CODE` existiert, nicht abgenommen; `LOCAL_PASS` Tests/Scope bestanden; `BLOCKED` externe Voraussetzung fehlt; `NOT_RUN` nicht ausgeführt; `QUALIFIED` alle vorgesehenen Evidence-Gates erfüllt.

### FEUI-00 – Reale Basis/Parameterinventur

- `origin/main`, lokale Änderungen, existierende TS-/Rust-/Python-/Shell-Dateien, aktuellen Project_0 STATUS und Gate-Zustände prüfen.
- [CODE_MAP](PROJECT_0_CODE_MAP.md) mit echten Symbolen/Feldern/Defaults/Einheiten/CLI-Namen gegen Rust aktualisieren.
- Frontend-`type-check`, `lint`, `build`, `smoke` und vorhandene Tests nur mit realen Voraussetzungen durchführen; keine grünen APIs erfinden.
- **Abnahme:** vollständiges IST-/SOLL-Delta, exakter Source-Hash, belegte lokale Tests, keine pauschalen PASS-Behauptungen.

### FEUI-01 – Zentrale `p0ctl`-Steuerung und Lifecycle (VOR 4h)

- Bestehende `scripts/test.sh`, `capture.sh`, `verify.sh`, `report.sh`, `orchestration/startup.rs`, `runtime_config.rs`, `MeasurementPlan` wiederverwenden.
- Neue `p0ctl`-Kommandos: `doctor`, `preflight`, `run start`, `run status`, `run stop`, `runs list`, `logs`, `verify`, `report`, später `optimize`.
- Ein aktiver Run, einmaliges Run-ID-/Manifest-Commit vor Capture, exklusiver Writer-/Owner-Lock, PID und Startidentität, Kindprozess-Reaping, sicherer Shutdown mit Admission-Close, Drain, Sync, Seal, Exit-/Crash-Evidence.
- **Zwingender Start-Audit vor jedem neuen externen Run:** Offline-Selftest plus ausdrücklich erlaubte read-only Chain-/Provider-Probes (Solana-Genesis-/RPC-/WSS-Identität, Marginfi-/Deployment-/ProgramData-Fingerprint sofern abrufbar, Bank-/Group-/Oracle-/Mint-/Owner-Beziehungen, Program-ID/Discriminator-/ABI-Pins, erreichbare Free-Provider und Quotas, Wallet-Public-Identity/Permissions, Host-Storage/Writer/Memory, Binär-/Konfigurations-Hashes). Erwartete Identitäten gegen aktuelle Quelle prüfen; Drift, fehlende/umgebogene Adresse und falscher Cluster führen zu `IDENTITY_MISMATCH/QUARANTINED/BLOCKED` statt stiller neuer Adresse. Keine selbsttätige Re-Pin-/Signer-/LIVE-Freigabe. Pro Check reale Quelle, Slot, Timestamp und Resultat journalen, periodisch erneut validieren. Details in [STARTUP_BACKTEST_OPTIMIZATION](STARTUP_BACKTEST_OPTIMIZATION.md).
- WSS-ACK **und** verwertbaren Slot-/Appframe-Fortschritt als Readiness-Bedingung. Keinen zurückliegenden Live-Empfang durch historischen Replay „reparieren`.
- `p0ctl` darf ohne GUI mehrere Stunden laufen; Browser-/HTTP-/WS-Abstürze beeinflussen weder Ingress noch Engine.
- **Abnahme:** Doppelstart/Doppelwriter verweigert, Stop idempotent, SIGINT/SIGTERM/Crash/Restart fail-closed, exakt eine effektive Runtime-Konfiguration, kein LIVE per Control.

### FEUI-02 – Capacity-/Provider-Preflight, Storage und 4h-Gate (VOR 4h)

- Bestehende `storage/preflight.rs` / `p0-capacity.rs` (WIP) in den Launcher integrieren und alte pauschale Python-60GiB-Heuristik ablösen.
- `StorageStatus`, `StorageMetrics`, `CapacityReport`: gemessene durchschnittliche/Burst-Raten, nachhaltige Writer-Leistung, SSD Reserve, RAM/Swap/RSS, Queue, Stall, CPU und geschätzte Erschöpfungszeit.
- **Freie Laufzeit:** `RunHorizon::Fixed(duration_seconds)` für beliebige positiv validierte Sekunden-/Minuten-/Stunden-/Tagesdauern und `RunHorizon::UntilStopped` ohne künstliches Enddatum. Die bisherigen 30s/60s/10m/1h/4h/24h/48h sind ausschließlich UI-Schnellwahlen; der native `MeasurementPlan` hat bislang **nur** endliche s/m/h-Dauern. Infinite darf **nicht** als `u64::MAX`-Sekunden oder Platzhalter-48h implementiert werden. Details/Gates in [STARTUP_BACKTEST_OPTIMIZATION](STARTUP_BACKTEST_OPTIMIZATION.md).
- 30s/60s/10m-Kalibrierung von beliebigen längeren Fixed-/UntilStopped-Läufen trennen. Fehlende Basis: `CAPACITY_UNVERIFIED` statt stilles Startsollen. Für `UntilStopped` Start nur bei qualifizierter rollierender Mindest-Runway und laufendem Auto-Recheck; kein unbegrenztes SSD-/Provider-/RAM-Versprechen.
- Provider-Status/quotas/HTTP429, `getBlock`-Archiv, Slot-/LUT-Coverage, WSS-Liveness und Strict-Call-/Walltimebudget vor Start prüfen.
- SK-07 hashverkettete `.p0j`-Segmente, Crash-/Truncation-/Reader-Recovery; SK-03 noch offene RAM-Collections in Econ-/Winner-Worker berücksichtigen.
- **Abnahme:** 600s gezielter technischer Smoke, Fehlerfälle (Disk-full, 429, WSS-gap, Overload), Provider-/Kapazitätsbeweis; 4h erst nach Userfreigabe.

### FEUI-03 – Graphite V5 Project 0 Control (fünf Reiter, kein zweites Dashboard)

- **Freies Laufzeit-Eingabefeld mit Einheit** (`s/m/h/d`, später kompatibel erweitern) und separatem explizitem Modus **`Bis manuell gestoppt`**. `30s/60s/10m/1h/4h/24h/48h` nur vorbesetzende Shortcuts; `RunHorizon` als Backend-Owner, keine statisch unbegrenzte Scheinzahl. Bestehendes `MeasurementPlan::parse_duration` validiert nur endliche s/m/h-Dauern und wird versioniert migriert; UI darf nicht eigenständig Grenzen/Snapshot-Zeitpunkte berechnen.
- Quelle `logs|transactions`, Verification `off|index|strict` (vorhandenes `crosscheck` sofern separat qualifiziert), Profile `smoke|standard|performance|custom`.
- Sämtliche konfigurierbaren Rust-`Limits` anhand `runtime_config.rs::descriptors()` anzeigen: Einheit, Default, Min/Max, ENV/Quelle, `restart_required`. Keine kopierten Werte als UI-Defaults.
- **V5-UI bindend:** neutrales Graphitgrau, eine zentrale Fläche, Pill-Navigation mit exakt `Overview / Liquidator / Capture / Analytics / Wallets`, schmale Trennlinien statt blauer Kacheln, keine zusätzliche Sidebar. `System Health` in der Kopfzeile als aufklappbarer Diagnostikbereich; englische Beschriftungen. Optische Referenz und Tokens siehe [GRAPHITE_V5_UI_FINANCE_CONTRACT.md](GRAPHITE_V5_UI_FINANCE_CONTRACT.md).
- **Overview-Pflichtfelder:** RPC-Latenz p95 (ms), Entscheidungs-Latenz p95 (ms), Stream-Freshness (ms), **Realized net PnL · USD**, **Total capital · USD** sowie Liquidator-/Capture-Status; `—` mit Quelle/Grund, wenn noch nicht nachgewiesen. Realized Net PnL und Total Capital sind dieselben Snapshotwerte wie unter Wallets, keine getrennten Frontend-Berechnungen.
- Statusachsen: Run-ID, Walltime vs tatsächlich covered, READY/WSS/Slots, Gaps/429, Writer-Lag, RAM/SSD, Verifier-Cursor, Shadow-/Economics-UNKNOWN, Safety.
- UI-Snapshot + begrenzte WS-Deltas; bei Sequence-Gap erneut REST-Snapshot laden, bis dahin `STALE` zeigen. Browser-Neustart darf keinen Run stoppen.
- **Abnahme:** identische Werte/Fehler/Profil-Hash bei Terminal-/Browserstart, kein Doppelstart, kein Slow-Client-Effekt auf Engine.

### FEUI-04 – Vollständige 6-Marker-/4-Klassen-Auswertung und Raw Explorer

- Die **sechs** tatsächlichen Marginfi-Instruktionsmarker `lending_account_liquidate`, `start_liquidation`, `end_liquidation`, `start_deleverage`, `end_deleverage`, `lending_pool_handle_bankruptcy` einzeln/alle/keine im **Report** schaltbar machen.
- Die **vier** fachlichen Klassen `CLASSIC`, `RECEIVERSHIP`, `FORCED_DELEVERAGE`, `BANKRUPTCY_SETTLEMENT` getrennt auswählbar; Start-/Endpaarung ohne Doppelerfolg.
- Die vier verwandten, **nicht als Liquidation zählenden** Instruktionen `drift_claim_bad_debt`, `purge_deleverage_balance`, `marginfi_account_keeper_close_order`, `marginfi_account_init_liq_record` separat.
- Reale Flags `--report-types`, `--report-markers`, `--report-related-events/types`, `--report-view`, `--report-provenance`, `--report-from/to`, `--report-min-net-usd`, `--report-winners/competition/economics`. Alle sind Ansicht, **nicht** Capture/Verifier/Trading-Filter.
- Aus `.p0j` paginierte RawRecord-Details: Signature, Slot, T_rx, TX-Status SUCCESS/FAILED/UNKNOWN, Top-level/CPI-Position, LUT, Blocktime, Fee/Token-Deltas, Actor, Marker/Pairing, Source/Proof, Event-/Journal-Hash. Keine unbounded Browserdaten.
- **Abnahme:** Rust-/Python-A4-Parität im belegten Scope, 6 Marker einzeln, 4 Klassen/4 Related, failed/truncated/Legacy/v0/v1/CPI, identischer Raw-Hash trotz jeder Filterkombination. Fehlende historische Rohbeispiele ausdrücklich BLOCKED.

### FEUI-05 – Run-Historie, Verifikation und Provenienz

- Unveränderte `capture_run_id`, stabile `verification_spec_id`, pro Versuch neue `verification_attempt_id`, unveränderliche finale `verification_result_id`, gebundene `report_query_identity`.
- `off` = NOT_RUN; `index` maximal INDEX_VERIFIED; `strict` nur bei vollständigem finalized Proof CANONICAL_VERIFIED; 429/Pruning = HISTORICAL_INCOMPLETE mit Resume-Cursor.
- LIVE/INDEX/STRICT/SHADOW_ONLY unterscheiden. Ein historisch vervollständigter Scan ersetzt nicht verlorenes originales `T_rx`.
- **Abnahme:** Verify/Resume/Report-Nachweise, Hash-Mismatch/ältere Schemaversion fehlschlagen, keine „vollständig“-Anzeige bei nur vorläufigem Result.

### FEUI-06 – Economics/Competition/Race/Verlustursachen und native Latenz-Telemetrie

- Existierende Rust-`liquidation/economics.rs`, `analytics/econ_canonical.rs`, `market/winner.rs`, `winner_tracker.rs` wiederverwenden.
- `market_gross_edge_usd` ≠ `executable_shadow_net_edge_usd` ≠ `realized_pnl_usd`. On-chain Fees, Priority Fee, Tip, Rent, Swap/Exit/Restinventar und `unknown_costs` mit Provenienz.
- Ursachen je Signature+Slot+Instruction/Event: Ingress, State-Staleness, Health, Funding, Routing/Exit, TX-Größe, Simulation, Fee-Budget, Race, Landing oder UNKNOWN. Pro Episode ein primärer Verlustgrund; Sekundärgründe getrennt, keine doppelte Verlust-/Fee-Zählung.
- Kapitalbänder `200/500/1000/2500/5000/10000 USD`; gemessene Speedup-Sensitivität nur Szenario, kein bewiesener Winner.
- Für RPC p50/p95/p99, RX→Decision p50/p95/p99, Stream Age sowie spätere Build/Sign/Submit-/Finalized-Latenz die *gemessenen* monotonen Zeitpunkte, Samples, Messfenster, Backend-Quantile und Status offenlegen. Vorhanden: `ingress::t_rx_ns`, `MarketStats.max_stream_silence_ms`, `StorageStatus.queue_bytes/lag_frames`, `econ.rs::Distribution`; vollständige rolling Live-Histogramme bleiben **SOLL**, keine Dummy-Latenzen. Quelle siehe [GRAPHITE_V5_UI_FINANCE_CONTRACT.md](GRAPHITE_V5_UI_FINANCE_CONTRACT.md).
- **Abnahme:** exakte u64/u128/i128-/USD-micro-/bps-/ns-Einheiten, keine JS-Float-Entscheidung, keine Scheingenauigkeit bei nicht vorhandenem Edge oder Latenz. p95 + Samplezahl + Fenster + Fehlerstatus nachvollziehbar.

### FEUI-07 – Profitability Lab V1: reproduzierbare Offline-Optimierung

- **Ein-Klick-Kette:** `Capture-Run auswählen → Evidence-Qualität anzeigen → Check/Verify → Backtest (zeitkonsistenter Replay) → automatisch Report → Optimieren → Baseline/Kandidat vergleichen → versioniertes Offline-Profil exportieren`. Auf abgeschlossenen `Run`s oder unveränderlichen **versiegelten Segment-Snapshots** eines laufenden `UntilStopped`-Runs; keine erneute Raw-Aufnahme nötig, kein Browser-Compute. Detailvertrag [STARTUP_BACKTEST_OPTIMIZATION](STARTUP_BACKTEST_OPTIMIZATION.md).
- Ausschließlich mit versiegelten realen `.p0j`-Runs und bestehender Rust-Economics: deterministische Parameter-Sweeps für Kapital, Mindest-Nettoedge, Funding, Route/Exit, Gebühren/Tip, Candidate-Admission und Timing-Sensitivität.
- Baseline und Kandidat auf identischem historischem Datensatz/Code-/Decoder-/Protokoll-/Profil-Hash vergleichen, UNKNOWN statt erfundener Gegenfakten.
- Zeitblock-/Cluster-Bootstrap, Walk-forward und separater Holdout; mögliche Marktregime, Unsicherheitsintervalle, Nicht-Realisierbares und `INSUFFICIENT_SAMPLE` anzeigen. Eine einzelne 4h-Probe garantiert keine Profitsteigerung.
- **Abnahme:** bitidentisches Replay bei gleichen Inputs; keine zukünftigen Quotes/Slotfacts in historischen Entscheidungen, keine Änderung laufender Strategieparameter.

### FEUI-08 – Strategy Profiles und adaptive Optimierung (nach FEUI-07)

- Versionierte Profile und explizit gebundene Replay-/Holdout-Evidence, Cost-/Provider-/Risk-Limits; Approval und atomische Profilaktivierung nach getrennten Gates.
- Erst mit echten ausreichend gelabelten Landing-/Winner-Daten: logistische Kalibrierung, Gradient Boosting, Quantile, Bayesian Optimization, Drift/Change-Point. Least Squares nur für sinnvolle Teil-Baselines. KI/LLM erklärt Befunde, entscheidet nicht selbst über Geld.
- **Abnahme:** schlechte Out-of-sample-/UNKNOWN-Gates verwerfen Parameter; kein stilles LIVE- oder Paid-Schalten aus dem Optimierer.

### FEUI-09 – Security, Contracttests, CI/Deployment

- Control-API lokal/Loopback, Auth, Origin/CSRF/WS-Upgrades, rate-limits, idempotente Start-/Stop-Jobs, Projekttrennung, keine Keys/Signaturdaten in Vite/HTML/Logs, keine Shell-Interpolation.
- Echte Contract-Parität Frontend↔Rust↔CLI, Build/Typecheck/Lint/Playwright, Crash/Reconnect/slow-client, JFI-konforme interne Protokolle, Sicherheits- und Failure-Tests.
- Optional macOS `launchd`, aber kein zusätzlicher Redis- oder dauerhafter Paid-Dienst notwendig.
- **Abnahme:** UI-Disconnect beeinflusst keinen Run; aus UI ist keine Geldtransaktion möglich; CI benötigt keine privaten Providerkeys.


### FEUI-10 – Wallets & Treasury (neuer verbindlicher Produktbereich)

Verbindlicher Detailvertrag: **[Wallets & Treasury](WALLETS_TREASURY.md)**. Wallets, Public Keys, SOL/SPL/Token-Balances, Geld-/Trading-Reservations, verifizierte Gebühren-/Profitwerte und Ziel-Adressen für Phantom-/MetaMask-Solana im gemeinsamen Dashboard anzeigen. Quelle: `project_0/config/wallet.public.env`, `execution/one_shot.rs` (WalletLock, Hold, HandoffUnknown), `receipts.rs` (finalized wallet deltas), `pending.rs`, `liquidation/funding.rs`, `analytics/journal.rs`. Das ist **nicht** schon ein Treasury-Sender.

- **FEUI-10A READ_ONLY:** alle konfigurierten Wallets mit Projektdomäne/Chain/Mint/Tokenkonto/Balance/Spendable/Reservations, bekanntem finalized Netprofit und UNKNOWN getrennt. Walletsource/-Status nie als echten Kontostand interpretieren. Konsolidiertes versioniertes `PortfolioFinancialSnapshot` (SOLL) mit **Total capital, Available capital, Reserved capital, Realized net PnL, Withdrawable profit, Transferred to Treasury** in USD und per Asset; alle Werte zugleich in `Wallets` und die beiden Pflicht-KPIs `Total capital` + `Realized net PnL` auf `Overview` mit **derselben snapshot_id**. Portfolio-/PnL-Nachweis erst nach echter Wallet-/Price-/Liability-/FE-07-Reconciliation.
- **FEUI-10B MANUAL:** separate Treasury-Preview/Intent/Signer-/Receiver-/Mint-/Fee-/Reserve-/Amount-Gates, ausdrücklich autorisierte manuelle SOL-/SPL-Transfers zu allowlist-basierten **Solana**-Empfangsadressen von Phantom oder MetaMask, finalisierte Receipts, crashsichere Holds/UNKNOWN/keine Duplikate.
- **FEUI-10C AUTO:** frei einstellbarer Mindestbetrag `X` **neu realisierter, noch nicht abgeschöpfter** Profit, Überweisungsanteil/-Betrag, Fee-/Reserve-/Transfer-/Zeitraumgrenzen, Cooldown/Expiry, Empfänger-/Asset-/Chain-Allowlist, versionierte explizit freigegebene Standing-Policy und Kill Switch. Niemals komplette Wallet-Guthaben oder Shadow-Szenarien als auszahlbare Profite behandeln.
- Eine MetaMask-`0x`-EVM-Adresse ist **kein** direktes Solana-Auszahlungsziel; kein automatisches Bridging ohne neuen gesonderten Sicherheitsvertrag. Wallet-Transfers sind **value-bearing** und bleiben bis nach FE-04/P8, abgeschlossener echter FE-07-PnL-Reconciliation und einer **gesonderten Treasury-Geldfreigabe** gesperrt. Sie blockieren **nicht** den nächsten 4h-Beobachtungstest.
- **Abnahme W-G01…08** laut Walletvertrag: Public Identity, Profit/Balance/Spendable, Reservierungs-Atomizität, sichere Einmalüberweisung, idempotente Gewinnschwelle/Restart/UNKNOWN, UI, Geldberechtigung, JFI-/BSC-Isolation.

## 3. Erweiterter, verpflichtender Betriebs- und Analyseablauf v1.1

Die ausführliche Spezifikation steht in [STARTUP_BACKTEST_OPTIMIZATION.md](STARTUP_BACKTEST_OPTIMIZATION.md). Die folgenden Gates sind **verbindliche Ergänzungen** zu FEUI-01/02/03/05/06/07/08/09 und dürfen nicht als gesonderte, nachrangige Fantasiephase behandelt werden:

| Gate | Implementierungspflicht | Positive Abnahme |
|---|---|---|
| **F-G10 BOOT INTEGRITY** | Vor jedem Capture alle kritischen Build-/Chain-/Program-/Bank-/Oracle-/Provider-/Host-/Safety-Abhängigkeiten mit erwarteter Identität prüfen; Hardfail bei Mismatch, Unknown für unprüfbare optionale Checks | Reproduzierbare Audit-Evidence, kein Start trotz Address-/Bytecode-/ProgramData-Drift |
| **F-G11 UNBOUNDED** | Versioniertes `RunHorizon::Fixed` oder `UntilStopped`, rollierende Fenster, Journalsegmente, Retention/Storage/Provider-Watchdogs, graceful stop | 30s/custom 75m/custom 7d/unbounded, ns-Overflow verhindert, 4 Quartale für finite Bestandsläufe kompatibel |
| **F-G12 ANALYSIS PIPELINE** | Paginierter Run-Browser; as-of historischen Replay auf gespeicherten Beweisen; Verify/Backtest/Report/Optimize als getrennte serverseitige Jobs, alle mit festen Daten-/Versionsidentitäten | Klickkette vollständig, idempotent, Close/Reopen des Browsers ohne Daten-/Jobverlust |
| **F-G13 PROOF-/PNL DISPLAY** | Deckung und UNKNOWN getrennt von Gross-Edge, Shadow-Edge, Race-Sensitivität und **tatsächlich realisiertem** PnL; grafische Drilldowns auf signierte Evidence | Kein hypothetischer Gewinn in Realized-KPI, keine erfundene Win-Wahrscheinlichkeit |
| **F-G15 GRAPHITE-V5 + FINANCE** | Verbindliche fünf englische Reiter/Graphit-Pill-UI, Health als Headerdetail; p95 samt Quelle, Snapshotidentität für `Overview` und `Wallets`; Portfolio-Nettovermögen/Working-Capital/Reservations/realized PnL/Sweeps fachlich separat | Visual-/Contracttests aus [V5-Vertrag](GRAPHITE_V5_UI_FINANCE_CONTRACT.md), kein falscher Echtgeld-/PNL-Wert |
| **F-G14 OPTIMIZE** | Konfiguriertes Budget/Parameterraum, exakter Backend-Sweep, Time-Block-Holdout, Confidence/UNKNOWN, Baseline-Vergleich, profile candidate only | Derselbe Input → gleiches Ranking/Report-Hash, kein Lookahead, kein automatisches LIVE-Profil |

**Wichtig:** Ein Boot-Audit stellt **Identität/Erreichbarkeit zum Prüfzeitpunkt** fest, nicht zukünftige Quotenfreiheit oder garantiert lückenlose WSS-Lieferung. Langläufe überwachen diese Eigenschaften erneut und wechseln bei kritischer Drift auf `DEGRADED`/Fail-Closed, ohne Daten zu verwerfen oder unbemerkt andere Gegenparteien anzusprechen. `UntilStopped` bedeutet **keine planmäßige Zeitabschaltung**, jedoch zwingend endliche RAM-/Storage-/Provider-Sicherheitsbudgets, Segmentrotation, automatische Kapazitätsprüfungen und transparenten Stop/Halt bei nicht behebbarem Fehler.

## 4. Umsetzungsgates und Reihenfolge

| Priorität | Tickets | Blockiert nächsten 4h? |
|---|---|---|
| 1 | FEUI-00 / schlankes FEUI-01 | JA – sicherer CLI-Lifecycle |
| 2 | FEUI-02 + **nur Backendteil FEUI-04** + SK-/JFI-/Verifier-Gates | JA – sechs Marker, Binär-Raw, Capacity und Provider |
| 3 | Qualifizierter kurzer Smoke, gesondert freigegebener 4h-Run | JA – echte Coverage und Proof, nicht UI |
| 4 | FEUI-03/04/05 React-Anbindung | NEIN – parallel/anschließend |
| 5 | FEUI-06/07 Profitability Lab | NEIN – offline auf bereits gespeicherten Daten |
| 6 | FEUI-08/09 finale Produkt-/Optimizer-Gates | NEIN für 4h; vor späterer adaptiver Betriebsfreigabe JA |

**Abnahmekriterien:** (1) single Owner und idempotenter Stop; (2) gleiche CLI-/UI-Parameter/Units/Defaults; (3) sechs Marker + vier Klassen + vier Related, Hash-invariante Filter; (4) SK-06/07 + unfiltered Journal und Provider/Strict; (5) STATUS/UNKNOWN/Provenienz korrekt; (6) exakte Backend-Economics; (7) Offline-Holdout ohne Zukunftsinformation; (8) keine LIVE-/Paid-/Submission-Umgehung; (9) reproduzierbare Tests und Ledger.

## 5. Claude-Ausführungsregel

Vor jeder Phase aktuellen HEAD, `STATUS.md`, `PHASENPLAN.md`, `MEASUREMENT_LEDGER.md`, die vier Frontend-Dokumente einschließlich `STARTUP_BACKTEST_OPTIMIZATION.md` und die tatsächlichen Dateien lesen. Nur getestete vollständig abgeschlossene Tickets als LOCAL_PASS markieren. Nie eine neue Klasse/Variable/REST-Route als bereits implementiert behandeln, solange sie nur im SOLL-Plan steht. Kein Code unter `bsc/` oder fremden Projektteilen ändern. Keine unautorisierten mehrstündigen Mainnet-/Paid-/LIVE-/Submission-Aktionen. FE-First-Profit-Priorität bleibt separat bindend.
