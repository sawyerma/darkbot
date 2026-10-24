# Project_0 – geprüfte Code- und Variablenmatrix für das Frontend

**HINWEIS 2026-10-23:** Die Abschnitte 1–9 sind ein historischer Quell-Snapshot und enthalten ausdrücklich überholte IST-Aussagen (Project0Page „Platzhalter“, p0ctl „nicht umgesetzt“, nur ein P0-Topic). Die aktuelle Implementierungs-/Verkabelungsmatrix steht in [§10](#10-revalidierung-2026-10-23--aktueller-reactprotocolnative-abgleich), der Projektplan verweist auf sie. Bestehende F0-VAR-Rohinventare sind ebenfalls historisch und keine Freigabe.

Stand: 2026-10-17. Historische Quellenbasis Project_0 `0e0544d`; **heutige React-V0-Oberfläche existiert bereits**. Vor Implementierung aktuellen HEAD und [SOURCE_VARIABLE_MATRIX](SOURCE_VARIABLE_MATRIX.md) prüfen. **IST** = Symbol existiert, nicht zwingend QUALIFIED. **SOLL** = im [Frontend-Plan](FRONTEND_PHASENPLAN.md) vorgesehen. [Control-Protokoll](GATEWAY_WIRE_PROTOCOL_V1_3.md).

## 1. Frontend IST: bereits vorhandener Code

| Datei | Reale Implementierung | Lücke |
|---|---|---|
| [`src/app/router.tsx`](../src/app/router.tsx) | Route `/project-0` | Detailseiten fehlen |
| [`src/features/project0/Project0Page.tsx`](../src/features/project0/Project0Page.tsx) | Platzhaltertext | kein echter Controller |
| [`src/core/api/client.ts`](../src/core/api/client.ts) | `apiGet<T>`, `VITE_API_BASE_URL`, Default `/api/v1` | kein Control-POST-Adapter |
| [`src/core/api/endpoints.ts`](../src/core/api/endpoints.ts) | `status,overview,bsc,project0,solana`, `project0="/project0"` | konkrete Run-APIs fehlen |
| [`src/core/realtime/RealtimeClient.ts`](../src/core/realtime/RealtimeClient.ts) | WS `/ws/v1/events`, `connect/subscribe/onGap`, Reconnect | kein nachgewiesener Project0-Server |
| [`src/core/realtime/protocol.ts`](../src/core/realtime/protocol.ts) | `RealtimeEnvelope {seq,ts_ns,type,project?,payload}` | versionierter Rust-Vertrag fehlt |
| [`src/core/types/system.ts`](../src/core/types/system.ts) | `RuntimeMode=OFFLINE|SHADOW|LIVE|HALT` | LIVE-Enum ≠ LIVE-Freigabe |
| [`src/app/providers.tsx`](../src/app/providers.tsx) | QueryClientProvider, ThemeProvider | Project0-Hooks |
| [`vite.config.ts`](../vite.config.ts) | Proxy `/api`, `/ws` nach Port 8100, Vite 8080 | Proxy ≠ laufender Backendserver |
| [`.env.example`](../.env.example) | `VITE_API_BASE_URL,VITE_WS_URL,VITE_DEV_PORT,VITE_BACKEND_BASE,VITE_BACKEND_WS` | nur öffentliche Daten |
| [`package.json`](../package.json) | `build`, `lint`, `type-check`, `smoke`; React/Vite/Query/Recharts/Playwright | kein belegter Project0-E2E-PASS |

## 2. Operation: echte Einstiegspunkte

| Aufgabe | Exakter Owner | Frontend-Zuordnung |
|---|---|---|
| Run starten | [`project_0/scripts/test.sh`](../../project_0/scripts/test.sh) → [`capture.sh`](../../project_0/scripts/capture.sh) `econ4h_launcher()` | SOLL `p0ctl run start` |
| Kombinierte Runtime | [`orchestration/startup.rs`](../../project_0/native/crates/runtime/src/orchestration/startup.rs) `run_econ4h(),supervise()` | niemals zweite Engine |
| Config | [`solana/src/runtime_config.rs`](../../project_0/native/crates/solana/src/runtime_config.rs) `RuntimeConfig,Limits,Resolved::parse,descriptors()` | Backend-Defaults/Validation |
| Zeit | [`analytics/measurement.rs`](../../project_0/native/crates/runtime/src/analytics/measurement.rs) `MeasurementPlan::parse_duration`, `SNAPSHOT_COUNT=4` | Dauer, Viertel-Snapshots |
| Python CLI | [`operations/cli/measurement_contract.py`](../../project_0/operations/cli/measurement_contract.py) `MeasurementConfigV2,RunConfig,VerificationConfig,ReportQuery` | IST, JFI-Migration offen |
| Handoff | [`operations/cli/measurement_cli.py`](../../project_0/operations/cli/measurement_cli.py) `launch,prepare,finish` | Launcher, Verifier, Report |
| Chain-Check | [`scripts/verify.sh`](../../project_0/scripts/verify.sh), [`measurement_verify.py`](../../project_0/operations/verification/measurement_verify.py), `bin/p0-strict-verify.rs` | SOLL Verify Job |
| Report | [`scripts/report.sh`](../../project_0/scripts/report.sh), [`market_report.py`](../../project_0/operations/reporting/market_report.py) | native B7-Consumer fehlt |
| Replay | [`bin/p0-econ-replay.rs`](../../project_0/native/crates/runtime/src/bin/p0-econ-replay.rs) | exakte Rust-Economics |
| Preflight | [`storage/preflight.rs`](../../project_0/native/crates/runtime/src/storage/preflight.rs), [`bin/p0-capacity.rs`](../../project_0/native/crates/runtime/src/bin/p0-capacity.rs) | Code WIP, `capture.sh` verwendet noch Python-60GiB-Heuristik |
| Raw | [`market/rows.rs`](../../project_0/native/crates/runtime/src/market/rows.rs), [`analytics/journal.rs`](../../project_0/native/crates/runtime/src/analytics/journal.rs) | native `.p0j`-Reader erforderlich |

## 3. Exakte CLI-Flags (IST)

Quelle: `project_0/operations/cli/measurement_contract.py` Parser:

| Flag | Feld / Werte | Default | Steuerung |
|---|---|---|---|
| `--duration` | `RunConfig.duration`, Ganzzahl + `s/m/h`; 30s/60s/10m/1h/4h/24h/48h | required | Run |
| `--capture-source` | `RunConfig.capture_source` = `logs|transactions` | logs | Provider |
| `--chain-verify` (Alias `--verify,--mode`) | `VerificationConfig.mode` = `off|index|strict|crosscheck` | off | Post-Capture |
| `--report-types` | `ReportQuery.types`: all/none/Liste | alle 4 | nur Report |
| `--report-markers` | `ReportQuery.markers`: all/none/Liste | alle 6 | nur Report |
| `--report-related-events` | on/off | off | nur Report |
| `--report-related-types` | all/none/Liste | alle 4 | nur Report |
| `--report-view` | summary/liquidations/winners/competition/economics | summary | Report |
| `--report-provenance` | best/live/index/strict/shadow/all | best | Report |
| `--report-from`, `--report-to` | UTC | none | Report |
| `--report-min-net-usd` | finite number | none | **NICHT** `P0_MIN_PROFIT` |
| `--report-winners`, `--report-competition`, `--report-economics` | on/off | on | Darstellung |
| `--format` | table/json/csv | table | Legacy-Renderer, keine interne JSON-Erlaubnis |
| `--verify-rps`, `--verify-max-calls`, `--verify-max-seconds` | positive ints | 2 / 2000 / 300 | Provider-Budget |
| `--verify-max-bytes`, `--verify-max-disk-bytes` | bytes | 268435456 / 8589934592 | Verifier |
| `--verify-max-ram-bytes`, `--verify-min-free-bytes` | bytes | 2147483648 / 1073741824 | Verifier |
| `--max-pages`, `--timeout`, `--attempts` | ints | 50 / 15 / 3 | Verifier |
| `--resume-verification` | flag | false | nie Capture-Resume |
| `--verification-result` | finale Ergebnis-ID | none | reproduzierbarer Report |
| `--provider-qualification` | backend-only Pfad | none | FREE_ONLY Gate |
| `--profile`, `--config`, `--print-effective-config` | Rust operational config | standard | typed config |

### Sechs Marker, vier Klassen, vier verwandte Ereignisse

`MARKERS`: `lending_account_liquidate` (CLASSIC), `start_liquidation` und `end_liquidation` (RECEIVERSHIP), `start_deleverage` und `end_deleverage` (FORCED_DELEVERAGE), `lending_pool_handle_bankruptcy` (BANKRUPTCY_SETTLEMENT).

`KINDS`: `CLASSIC,RECEIVERSHIP,FORCED_DELEVERAGE,BANKRUPTCY_SETTLEMENT`.

`RELATED`: `drift_claim_bad_debt,purge_deleverage_balance,marginfi_account_keeper_close_order,marginfi_account_init_liq_record`. Niemals als profitables Liquidationsevent klassifizieren.

Produktive Decoder: `runtime/src/market_truth.rs`, `runtime/src/market/rows.rs`, `solana/src/chain.rs`. Historische A4-Parität nur im belegten Scope; Rohbeispiele CLASSIC/Deleverage/Bankruptcy/CPI/Legacy noch nicht vollständig qualifiziert.

## 4. Native Rust-Parameter/ENV – Quelle `runtime_config.rs::parameters!`

`Resolved::parse`: **profile < environment < config file < CLI**; `descriptors()` erzeugt für jedes Limit `name,field,type,unit,default,minimum,maximum,env,change="restart_required"`. Frontend darf keine unabhängigen Defaults implementieren.

| Rust-Limit | ENV | Default / Einheit |
|---|---|---|
| `rpc_rps,rpc_burst,rpc_concurrency,rpc_queue` | `P0_RPC_RPS,P0_RPC_BURST,P0_RPC_CONCURRENCY,P0_RPC_QUEUE` | 5 req/s, 1, 2, 64 |
| `rpc_timeout_ms,rpc_attempts,rpc_backoff_ms` | `P0_RPC_TIMEOUT_MS,P0_RPC_ATTEMPTS,P0_RPC_BACKOFF_MS` | 30000 ms,3,500 ms |
| `hotpath_rps,bootstrap_rps,history_rps,analytics_rps,simulation_rps` | `P0_HOTPATH_RPS,P0_BOOTSTRAP_RPS,P0_HISTORY_RPS,P0_ANALYTICS_RPS,P0_SIMULATION_RPS` | 5,5,2,2,2 req/s |
| `wss_connections,wss_subscriptions,wss_idle_timeout_ms,wss_recovery_ms` | `P0_WSS_CONNECTIONS,P0_WSS_SUBSCRIPTIONS,P0_WSS_IDLE_TIMEOUT_MS,P0_WSS_RECOVERY_MS` | 2,65536,60000ms,120000ms |
| `workers,ingress_queue,worker_queue` | `RAYON_NUM_THREADS,P0_INGRESS_BUFFER,P0_REMOTE_WORKER_BUFFER` | 1,65536,256 |
| `bootstrap_seconds,stream_ready_seconds,drain_seconds,market_drain_seconds` | `P0_ECON_BOOTSTRAP_SECS,P0_MARKET_READY_TIMEOUT_SECS,P0_ECON_DRAIN_SECS,P0_MARKET_DRAIN_TIMEOUT_SECS` | 600,600,180,120 s |
| `reconcile_seconds,gap_seconds,enrichment_seconds` | `P0_MARKET_RECONCILE_PHASE_MAX_SECS,P0_MARKET_GAP_PHASE_MAX_SECS,P0_MARKET_ENRICH_PHASE_MAX_SECS` | 14400,180,900 s |
| `verify_rps,verify_calls,verify_seconds` | `P0_VERIFY_RPS,P0_VERIFY_CALLS,P0_VERIFY_SECONDS` | 2 req/s,2000,300s |
| `verify_mib,verify_disk_mib,verify_ram_mib,verify_free_mib` | `P0_VERIFY_MIB,P0_VERIFY_DISK_MIB,P0_VERIFY_RAM_MIB,P0_VERIFY_FREE_MIB` | 256,8192,2048,1024 MiB |
| `market_capture_initial_mib,market_capture_max_mib,system_reserve_mib` | `P0_MARKET_CAPTURE_INITIAL_MIB,P0_MARKET_CAPTURE_MAX_MIB,P0_SYSTEM_RESERVE_MIB` | 64,512,4096 MiB |
| `market_writer_batch_frames,market_writer_flush_ms,market_writer_sync_ms,market_writer_stall_ms` | `P0_MARKET_WRITER_BATCH_FRAMES,P0_MARKET_WRITER_FLUSH_MS,P0_MARKET_WRITER_SYNC_MS,P0_MARKET_WRITER_STALL_MS` | 512 frames,1000ms,60000ms,30000ms |
| `market_disk_reserve_mib,capacity_safety_percent` | `P0_MARKET_DISK_RESERVE_MIB,P0_CAPACITY_SAFETY_PERCENT` | 2048 MiB,50% |

Weitere echte `RuntimeConfig`-Felder: `profile` (smoke/standard/performance/custom), `rpc_endpoints`, `wss_endpoints`, `fallback`, `adaptive`, `duration`, `verification`, `diagnostics`, `fe_mode`, `fe_targets`, `fe_group`. **Endpoint-Secrets nie im Browser ausgeben.** `P0_MIN_PROFIT` ist derzeit *kein* `Limits`-Parameter und darf nicht mit `--report-min-net-usd` verwechselt werden.

## 5. Journal-/Storage-/Economics-Typen

| Rust-Owner | Reale Typen / Feldnamen |
|---|---|
| `analytics/journal.rs` | `MAGIC=P0JRNL01`, `VERSION=1`, `Provenance {source,source_seq,slot,fork,rx_ns,unix_ns}` |
| `analytics/journal.rs::kind` | `MARKET_TRANSACTION=300`, `MARKET_LIQUIDATION=301`, `MARKET_COVERAGE=302`, `STRICT_SCOPE=310`, `STRICT_RPC_CACHE=311`, `STRICT_CHECKPOINT=312`, `STRICT_RESULT=313`, `CAPACITY_PREFLIGHT=320` |
| dito | `ECON_SHADOW_EVALUATION=400`, `ECON_SNAPSHOT=410`, `ECON_FINAL=411`, `ECON_OPPORTUNITY=412`, `ECON_CANONICAL=413` |
| `market/rows.rs` | `MarketTransaction{signature,slot,origin}`, `MarketLiquidation`, `MarketCoverage`, `WindowEvent`, `ReconciliationRow` |
| `solana/chain.rs` | `ChainTx,ChainMeta,ChainBlock,TxStatus::{Success,Failed,Unknown}`; Legacy/v0/v1/LUT/CPI |
| `market/core.rs` | `MarketStats.transactions_received,successful_transactions,failed_transactions,unknown_status_transactions,gap_count,transport_http_429,ingress_overflow_count,last_observed_slot` |
| `storage/writer.rs` | `StorageStatus {queue_bytes,queue_peak_bytes,admission_limit_bytes,max_bytes,free_budget_bytes,ingress_bytes_per_s,write_bytes_per_s,lag_frames,budget_exhaustion_s,disk_free_bytes,disk_reserve_bytes,disk_exhaustion_s}` |
| `storage/preflight.rs` | `CapacityReport {request,basis,write_bytes_per_s,environment,checks,verdict,start_allowed}`; WIP |
| `liquidation/economics.rs` | `CandidateInput,Evaluation::{Known,Unknown},FailedCostMatrix,actionable,t_profitable`; USD-micro, Gebühren, Rent, Funding, Capital |
| `analytics/econ.rs` | `CAPITAL_USD=[200,500,1000,2500,5000,10000]`; `SPEEDUP_MS=[1,5,10,25,50,100,200,500,1000]` |
| `analytics/econ_canonical.rs` | Market↔Shadow-Join, Capital, Competition, Race, Loss |
| `operations/reporting/market_report.py` | `market_gross_edge_usd`, `executable_shadow_net_edge_usd`, `realized_pnl_usd`, `unknown_costs` – Legacy Consumer noch JFI/B7 offen |

## 6. Erweiterung v1.1 – dynamische Dauer, Boot-Audit und Backtest (SOLL)

Vollständiger verbindlicher Vertrag: [Boot, unbounded capture und Klick-Optimierung](STARTUP_BACKTEST_OPTIMIZATION.md).

- **IST:** `MeasurementPlan::parse_duration` in `runtime/src/analytics/measurement.rs` und `runtime_config::duration_seconds` in `solana/src/runtime_config.rs` akzeptieren nur endliche Ganzzahlen mit `s/m/h`. `MeasurementPlan.duration_seconds:u64`, `SNAPSHOT_COUNT=4` und `snapshot_offsets_ns:[u64;4]` bilden endliche Messfenster. Kein tatsächliches Infinity im Code.
- **SOLL:** `RunHorizon::{Fixed,UntilStopped}`; `s/m/h/d`-Frontend-Eingabe und variable endliche Dauer, Rolling-Segment-/Checkpoint-/Preflight für unbounded; `Duration` niemals durch MaxInt vortäuschen.
- **IST:** `storage/preflight.rs::Request.duration_s:u64`, `CALIBRATION_MAX_S=600`, `CapacityReport.start_allowed`. **SOLL:** dynamischer Rolling-Runway und regelmäßiger Capacity-/Provider-Check.
- **IST:** `market/configuration.rs::MarketSource::coverage_verified=false` für logs/transactions. **SOLL:** vor jedem Run signierter/gebundener Boot-Audit mit genesis/owner/ProgramData/Bank-/Oracle-/PDA-/ABI-/WSS-Fingerprints, tatsächlicher Provider-Erreichbarkeit, Ressourcen- und Sicherheitsgates; dynamische Adressen aus Parent-State ableiten statt hardcoden.
- **IST:** `runtime/src/analytics/econ.rs::CAPITAL_USD` ist Default, `P0_ECON_CAPITAL_USD` akzeptiert bereits 1..32 positive u64-Werte. **SOLL:** freie kontrollierte Kapital-Szenarien, Offline-Backtest-/OptimizationJobs, serverseitige Replays und Baseline-Vergleiche.
- Alle neuen Typen `RunHorizon`, `BootAudit`, `BacktestJob`, `OptimizationJob` sind bislang nur **SOLL**, keine neuen Source-of-Truth-Defaults.

## 7. Noch NICHT umgesetzt / nicht nachgewiesen

`p0ctl`-Binary, Project_0-HTTP-Control-Server, Run-DTOs/API-Endpunkte, Strategieoptimierer, `StrategyProfile`, `OptimizationJob`, `LossAttribution` und Frontend-Komponenten = **SOLL**.

`MarketSource::coverage_verified()` ergibt für `logs` und `transactions` aktuell **false**; das Archiv-/Provider- und LUT-Source-Coverage-Gate ist offen. 4h-/5h-Versuche waren **INCOMPLETE**, C0-E1 Master NOT_QUALIFIED. SK-06-Launcher-Wiring und SK-07 Segmente offen, native B1/B7/JFI-Migration offen. FE-00 `NO_ECONOMIC_GO`; keine reale positive kumulierte Nettobilanz nachgewiesen. Jede neue UI-Variable braucht genau einen Rust-Owner, Einheit, Typ, Default, Validierung, Change-Klasse und Test.

## 8. Wallet-/Treasury-Symbole und Auszahlungsgrenzen

Details: **[Wallets & Treasury](WALLETS_TREASURY.md)**. **IST:** `config/wallet.public.env::P0_EXPECTED_PUBKEY,P0_WALLET_STATUS`; `runtime/src/execution/one_shot.rs::Approval,Intent,Current,Journal,WalletLock,Phase::{HandoffUnknown,HaltFinalized},Receipt`; `runtime/src/execution/one_shot/receipts.rs::FinalizedObservation,fetch_finalized,observe_finalized`; `runtime/src/execution/pending.rs::PendingSubmission,ReconcileRecord,ReconcileResult`; `liquidation/src/funding.rs::FundingClass,FundingContext`; `runtime/src/analytics/journal.rs` besitzt die kanonischen Journals. **Nicht IST:** WalletPortfolioSnapshot, TreasuryTransfer, Sweep-Policy, Manuell-Senden, Auto-Auszahlung, SOL-/SPL-Geldtransfer-UI. FE-04 nur PARTIAL_LOCAL, FE-07 NOT_RUN. Profit-Sweep erst nach finalisierter Reconciliation, Gebühren-/Tradingreserve und eigener Freigabe.

## 9. Graphite V5 UX-/Finanz-/Latenz-Mapping (IST vs SOLL)

Verbindliche Layout-/Felddefinitionen: [GRAPHITE_V5_UI_FINANCE_CONTRACT.md](GRAPHITE_V5_UI_FINANCE_CONTRACT.md). **IST:** `frontend/src/features/project0/Project0Page.tsx` bleibt Platzhalter, keine echten Tab-/Wallet-/Liquidator-React-Module. V5 ist eine lokale Bedienvorschau, kein existierendes Produkt. **IST:** `runtime/src/analytics/econ.rs::CAPITAL_USD` legacy-Defaults; `P0_ECON_CAPITAL_USD` akzeptiert bereits freie positive u64-Listen bis 32 Werte. **WICHTIGE GRENZE:** `runtime/src/analytics/econ_canonical.rs` iteriert für seinen `capital`-Report weiterhin `CAPITAL_USD`, statt alle frei konfigurierten Profile durchgängig zu übernehmen – vor realer V5-Optimize-Freigabe beseitigen und beweisen.

**IST-Quelle für teils vorhandene Walletgrundlagen:** `runtime/src/execution/one_shot.rs::Current.balances/WalletLock`, `runtime/src/execution/one_shot/receipts.rs::FinalizedObservation` mit `wallet_sol_delta_lamports`, `wallet_token_deltas` und `fee_lamports`. Letztere Datei erklärt ausdrücklich **keine Preis- oder Realized-PnL-Nachweise**. `operations/reporting/market_report.py::realized_pnl_usd` kann auf einen nachgewiesenen externen Market-Winner referenzieren und **ist nicht** die kumulierte eigene Portfolio-Trading-PnL.

**Neue SOLL-Variablen:** `PortfolioFinancialSnapshot.snapshot_id,portfolio_total_capital_usd_micro,portfolio_available_capital_usd_micro,portfolio_reserved_capital_usd_micro,realized_net_pnl_usd_micro,withdrawable_profit_usd_micro,transferred_to_treasury_usd_micro,wallet_asset_balances,as_of_slot,as_of_unix_ns,valuation_status,pnl_status,price_sources`. Keine dieser Variablen ist heute als produktive globale Rust-Portfolio-API nachgewiesen. `Overview` und `Wallets` teilen dieselbe Snapshotidentität und PnL-/Kapitalberechnung. Geldwerte verlustfrei in Rust, UI nur formatieren.

**Latenz IST:** `solana/src/ingress/mod.rs::t_rx_ns`, `market/core.rs::MarketStats.max_stream_silence_ms`/`last_observed_slot`, `storage/writer.rs::StorageStatus.queue_bytes/lag_frames`, `analytics/econ.rs::Distribution` mit p50/p99. **SOLL:** per-Provider RPC RTT-Histogramm, rolling decision-time p50/p95/p99, Stream-Age-Messwert und später submission stage latencies; p95 ist kein automatisch aus `P0_RPC_TIMEOUT_MS` ableitbarer Messwert.

---

## 10. Revalidierung 2026-10-23 – aktueller React/Protocol/Native-Abgleich

**Scope:** belegte GitHub-Quellen des gemeinsamen Repositories auf main am 2026-10-23; IST/SOLL nicht aus dem Design ableiten. **Normativ:** [Frontend Master](FRONTEND_PHASENPLAN.md), [Wire V1.3](GATEWAY_WIRE_PROTOCOL_V1_3.md), [F0-VAR](../../project_0/docs/F0_VAR_00_SCOPE.md), [Project_0 PHASENPLAN](../../project_0/docs/PHASENPLAN.md). Dieses Kapitel ist eine **fachliche Control-/UI-Zuordnung** und KEIN zweites Protobuf-, Config- oder Control-Protokoll. Die UI-Schlüssel unten sind Inventar-IDs, **keine neuen Gateway-Action-Namen oder Wire-Felder**. Source-HEAD erneut vor jedem Implementierungspaket prüfen.

### 10.1 Aktueller Implementierungsstand des gemeinsamen Systems

| Schicht | Tatsächlich vorhanden | Verbindliche Grenze |
|---|---|---|
| React Shell | Project0Page mit Overview, Liquidator, Capture, Analytics, Wallets; vorhandene Graphite-V0-Komponenten, scoped CSS, Health-Drawer | F1/V7-Design bleibt; **kein** neuer Reiter, zweites Frontend oder neues Wallet-Layout |
| Native binary states | protocol Project0 SystemStatus und CaptureState; topicTable.ts/gateway topics.rs: project0.status (10) und project0.capture.state (13) **available** | UI liest beide via project0Adapter.ts/project0ViewAdapter.ts/store. Topic-Enum allein beweist keine verfügbare Fachprojektion |
| Weitere P0-Topic-IDs | finance.summary (11), liquidator.state (12), wallets.state (14), analytics.jobs (15), execution.events (20), capture.events (21) | **RESERVED/UNAVAILABLE**: kein nicht-leeres payload_type in topicTable. Fehlende Finance nicht zu 0 rechnen |
| Gateway Capture Control | frontend/core/realtime/captureControl.ts; gateway/control.rs; HTTP GET /api/v1/project0/process, POST /api/v1/command, Lookup, CSRF/Receipts; action project0.capture.start/stop | Start hat **keine frei gewählten Parameter**: gateway startet bestehendes ops/darkbotctl **capture start-configured**. Das ist nicht der CLI-Vorschau-Run |
| Project0 CapturePanel | Duration-Presets, Source, Observation, Verify, Marker, CLI-Preview, Check & start / Stop | Die zwei Panel-Buttons erzeugen **nur lokale Notice**, kein Command. Bestehenden captureControl wiederverwenden, nicht zweiten HTTP-Client/Command bauen |
| Native CLI | runtime/src/bin/p0ctl.rs: doctor, preflight, run start/status/stop, verify, report, export, qualify, post-capture | **IST, kein Phantom**. run start benötigt --duration, gegebenenfalls NetworkAudit/Mainnet-Approval. **index/crosscheck werden in run start durch CHAIN_VERIFY_*_BLOCKED verweigert**; off/strict sind dort zulässig (strict nach Provider-/Audit-Gates) |
| Geyser/SourceControl | solana/src/ingress/{geyser,geyser_grpc,sources}.rs: BinaryGeyserSource, ProviderConfig, SourceUnit, OperatingMode OFF/OBSERVE/ACTIVE, Qualification, SourceArbiter | Adapter-Offline-Code vorhanden; Owner-Wiring und tatsächliches p0ctl-Source-Toggling sind derzeit noch Entwicklungsarbeit. Keine Quelle wegen GUI-Checkbox als qualifiziert behandeln |
| Finanz-/Safety-Code | runtime/src/execution/one_shot.rs + admission.rs, settlement.rs, reservations; liquidation health/economics/funding; FE-01/02/04 Codepakete | Kein nachgewiesener globaler finalisierter PortfolioFinancialSnapshot, kein Ledger-to-UI Finance-Topic und keine Treasury-/LIVE-Authorization. Ein Codeobjekt ist kein UI-Button |

### 10.2 Exakte Bestandszuordnung für die fünf vorhandenen Reiter

**Statuswerte:** DISPLAY_WIRED = existierender Binary-Topic-Read; COMMAND_EXISTING_OTHER_UI = vorhandener Commandpfad, aber noch nicht im P0-Reiter angeschlossen; COMMAND_WIRED = derselbe Commandpfad im P0-Reiter angeschlossen; DRAFT_LOCAL = Browser-State ohne Runtime-Wirkung; RESERVED = nur Topic/Phase/Backend-Entwurf; DISABLED = UI ausdrücklich nicht ausführbar; PARTIAL = vorhandener Rust-Owner, aber fehlender eindeutiger Publisher/Qualifikationsbeweis.

| Inventar-ID / vorhandener Slot | Aktuelle React-Datei / Variable | Native Source / bestehender Wire- oder CLI-Vertrag | IST / Korrektur (keine Doppel-UI) |
|---|---|---|---|
| OV-01 System Health / Phase / Stream freshness | OverviewPanel, HealthSection; capture.transport/phase, topicStatus | CaptureState(13), status_tap.rs → reporter → gateway → project0ViewAdapter | **DISPLAY_WIRED**; stale/detached klar markieren |
| OV-02 RPC p95, Decision p95 | OverviewPanel UnknownMetric, HealthSection | künftige native rolling-latency distributions/OwnerMetrics; P6-MT | **RESERVED**; kein aus Timeout oder max errechneter p95 |
| OV-03 Total capital, Realized net PnL | OverviewPanel; UNCONNECTED_MONITOR.finance | topic project0.finance.summary(11) + native finalisiertes PortfolioFinancialSnapshot erst nach FE-04/07/P8 | **RESERVED**, kein Markt-Winner-PnL als eigener Gewinn |
| OV-04 Liquidator status / Open Liquidator | OverviewPanel.goto("liquidator") | künftiges liquidator.state(12), FE-04/Gates; Execution aus SystemStatus(10) live/submission_authorized | Navigation **IST**; Execution **DISPLAY_WIRED** (Paket 1 `6d0cac99`, ohne LIVE-Topic „unknown“); aktive Instanz **UNVERIFIED** |
| OV-05 Capture status / Open Capture | OverviewPanel.goto("capture"), CaptureView | CaptureState(13) | **DISPLAY_WIRED**; kein Start/Stop aus Navigation |
| OV-06 Analytics / Open Analytics | OverviewPanel.goto("analytics") | künftiges analytics.jobs(15), Rust Report/Replay | Navigation **IST**; Jobs **RESERVED** |
| LIQ-01 Add / Duplicate / Remove / Select bot | LiquidatorPanel, useFleetDrafts::{create,duplicate,remove,select} | nur useState-Drafts; F8 spätere Runtime-Profile | **DRAFT_LOCAL**. Keine persistierte Botinstanz; Fleet später |
| LIQ-02 Name / Origin | LiquidatorPanel.d.name/d.origin | LiquidatorDraft; spätere typed Profile-Owner | **DRAFT_LOCAL**, Origin-Select ist disabled |
| LIQ-03 Classic / Receivership | LiquidatorPanel.enabledStrategies | Native Liquidation-Klassen/Planner und F8 Profile-Entwurf | **DRAFT_LOCAL**, keine nativ selektierte LIVE-Strategie. VIER Marktklassen/SIX Reportmarker nicht damit verwechseln |
| LIQ-04 Capital limit / Min net / Fee / Max in-flight | LiquidatorPanel capitalUsd/minimumNetUsd/maximumFeeUsd/maximumInFlight | runtime_config::descriptors, economics, Safety/Limits, global WalletReservations; später Profile-/CAS | **DRAFT_LOCAL**; React-Zahlen sind NICHT Engine-Defaults oder reserviertes Kapital |
| LIQ-05 Validate draft | LiquidatorPanel→validateDraft | Browser-Eingabeprüfung | **DRAFT_LOCAL**; kein Boot-/Simulation-/Health-/Economic-PASS |
| LIQ-06 Start selected bot / Start fleet | LiquidatorPanel disabled | FE-01…FE-06, F8 native Scheduler/Approval/OneShot | **DISABLED**, KEIN GUI-LIVE vor separater Freigabe |
| CAP-01 Run phase / duration / stream / counts / verdict | CapturePanel.RunningCapture | CaptureState(13)::run/phase/transport/market/assessment/writer | **DISPLAY_WIRED** (nur bei echtem Publisher), Verdict live ist noch keine Strict-Verifikation |
| CAP-02 Duration preset / custom / Until stopped | CapturePanel preset, amount, unit; CLI-Preview | p0ctl run start --duration; native runtime_config::duration_seconds; managed desktop configured capture | **DRAFT_LOCAL**. UI-"custom" d aktuell nicht automatisch native gültig; "Until stopped" disabled. Native Config/Controller-Effekt separat abnehmen |
| CAP-03 Logs/Transactions Source | CapturePanel.source | p0ctl --capture-source logs/transactions; MarketSource | **DRAFT_LOCAL**; Source-Coverage weiter unqualifiziert |
| CAP-04 Market / Econ4h / Shadow | CapturePanel.observation | Market Capture / run_econ4h / Shadow Owner in Rust; p0ctl run start usage | **DRAFT_LOCAL**; kein --mode in der Vorschau, `run start` = Market + Shadow (andere Auswahl → Vorschau verweigert), Desktop-Start Market-only (Paket 1 `6d0cac99`) |
| CAP-05 Check & start / Stop | CapturePanel → captureControl.ts (captureButtons, captureCommand) | bestehende Action project0.capture.start/stop, /api/v1/project0/process, ops/darkbotctl start-configured | **COMMAND_WIRED** (Paket 1 `6d0cac99`): ein Client, Freigabe nur nach launchd-Zustand, Stop-Bestätigung je Run (auch BSC-Readiness-Stopp), Receipt; Panel-Parameter NOT APPLIED |
| CAP-06 Verification off/index/strict/crosscheck | CapturePanel.verify | p0ctl run start blockiert index/crosscheck; CLI/post-capture separate Parser/Verifier | **DRAFT_LOCAL**; index/crosscheck DISABLED mit CHAIN_VERIFY_*_BLOCKED (Paket 1 `6d0cac99`) |
| CAP-07 Sechs Report-Marker / All / None | CapturePanel.markers, REPORT_MARKERS | operations/cli/measurement_contract.py::ReportQuery.markers, sechs gepinnte Marker | **DRAFT_LOCAL / report-only**; keine Rohcapture- oder Trading-Filter |
| CAP-08 Vier Liquidationsklassen, Related-Types, Provenance, Cost/Time Filters | Noch kein vollständiger Panel-Control | ReportQuery.types/related_*/view/provenance/from/to/min_net; p0ctl report/verify | **FEHLENDE UI-SURFACES innerhalb Capture/Analytics**, keine zweite Engine; alle vier Klassen und vier Related-Kategorien getrennt |
| CAP-09 Boot doctor, Preflight, Capacity, FREE_ONLY/provider quota | HealthSection teilweise read-only | p0ctl doctor/preflight; runtime storage/preflight.rs, provider.rs, ProviderEvidence | **PARTIAL/RESERVED**: Native Gates existieren, qualifizierte GUI-Projektion/Commandfreigabe fehlen |
| CAP-10 Source modes und Provider Readiness | Noch keine reale P0 Quellenleiste | solana/ingress/sources.rs::SourceControl/OperatingMode/Qualification; S1-02/S1-08/K-02/K-02b, zukünftige Source-DTOs | **RESERVED**: OFF/OBSERVE/ACTIVE; WSS vs Geyser(PublicNode provider) vs Deshred vs LocalShred getrennt, ACTIVE != LIVE |
| AN-01 Report / Backtest / Optimize tabs | AnalyticsPanel.tab | native p0ctl report, verify, qualify, export; p0-econ-replay.rs; OptimizationJob SOLL | Tabs **IST**; Ergebnisdaten ohne verfügbare analytics.jobs Projektion **UNCONNECTED** |
| AN-02 Recorded run selector / Run backtest | AnalyticsPanel disabled | binäre .p0j Run-/Report-IDs, p0ctl export/replay; analytics.jobs(15) RESERVED | **DISABLED**; Replay darf keine heutige Chain-Gewinnbeweiskette ersetzen |
| AN-03 Market, Shadow edge, Realized PnL | AnalyticsPanel values "—" | Marktbericht/Economics, FE-04-Settlement, Finance-Owner getrennt | **RESERVED**, Market-Winner-Economics != eigener realisierter Net PnL |
| AN-04 Capital / Minimum net / Optimize / Add as bot draft | AnalyticsPanel input + createDraft | Analytics nur Browser-Draft; später native Optimizer/Runs | Draft-Erstellung **IST**, Optimizer **DISABLED**; niemals als qualifizierte Strategie |
| WAL-01 Balances: sieben feste Zeilen | WalletsPanel.financialRows(finance) | Wallets(14) + Finance(11) RESERVED, finalisierte Account-/Price-/PnL-Owner | **DESIGN FROZEN/UNCONNECTED**. Gleiche künftige finance_snapshot_id wie Overview |
| WAL-02 Send: Ziel/Asset/Adresse/Menge/Review | WalletsPanel.send | FEUI-10B, Wallet-Treasury-Policy, native separate Approval erst nach Gate | **UI PREVIEW ONLY**; kein Transfer-Handler/Signer |
| WAL-03 Auto-Sweep: threshold/share/preview | WalletsPanel.auto | FEUI-10C, finale verfügbare Gewinne, eigener Policy-Owner | **UI PREVIEW ONLY**; nicht als automatisch aktive Regel behandeln |
| HLTH-01 Gateway, Stream, Transport, Writer, Market verdict, Counter, AllValues | HealthSection + AllValues | verfügbare Status(10) / Capture(13)-Protobuf; system.operational, capture run/transport/market/writer | **DISPLAY_WIRED** soweit Publisher sendet; übrige Health-/Boot-/Fee-/Provider-Zeilen klar NOT_COLLECTED statt Null; AllValues maskiert absoluten run_dir (Paket 1 `6d0cac99`) |

### 10.3 Neue Engine-Funktionen, die derzeit in den Reitern fehlen

**Kein zweites Protocol erfinden:** folgende Ergänzungen bleiben an bestehende Phasen/Owner und die reservierten Project0-Topics gebunden:

1. **Quellenkontrolle:** SourceUnit WsRpcControl, BinaryGeyser pro Provider (PublicNode nur Kandidat), Deshred und LocalShred (NOT_IMPLEMENTED); pro Einheit requested/effective, Qualification, Capabilities, Health, gap_open, reconnects, observed T_rx und Run-/Config-Generation; nach Runtime-Integration terminal zuerst und später **im bestehenden Capture/Health-Bereich**, kein neuer Hauptreiter. Spekulative Quellen niemals kanonischer State.
2. **Boot-/Run-Steuerung:** p0ctl doctor/preflight/run start/status/stop/verify/report/qualify/post-capture. Bestehende Gateway-Capture-Commands verwenden; freie Dauer/Quelle/Verify/Budgets benötigen erst typisierte gültige Command-Params und Engine-Bestätigung. Es darf keinen generischen Shell-/RPC-Befehl aus React geben.
3. **FE-Qualifikation:** FE-00…FE-07, First-Profit NO_ECONOMIC_GO, ProgramData/Bank/Oracle/Fees/Funding/Intent/Approval/Unknown/HALT und Ergebnisprovenienz als **read-only** unter Liquidator/Health/Analytics; Freigabe-/Submit-Schalter bleiben gesperrt.
4. **PERSIST/JFI/Kapazität:** .p0j Writer queue, lag, disk/ram, CapacityReport, segment/root/coverage, strict verifier/qualifier, Ergebnis- und Quelle-Identität; keine Browser-Hotpath-Journale oder JSON-in-bytes.
5. **P6-MT/Race:** Owner-Wakeup p50/p95/p99, state-to-detect/admit, Source lead/fork/gap, simulation/route/landing nur nach echter Sample-/Provenienz-Qualifikation. Keine UI-Erfindung von Latenzen, Winning oder Quoten.
6. **Wallet-/Finanzstatus:** FE-04-Reservations, finalized wallet deltas, net realized i128, UNKNOWN holds, policy-reserve, available funds – **erst bei validiertem globalen Finanzsnapshot**, keine aktuelle Bestätigung aus lokalen Datentypen ableiten.

### 10.4 Migrations- und Abnahmereihenfolge

- **F0-P0UI-1 SOURCE-TRUTH:** F0_VAR_RUST_FIELDS.tsv/F0_VAR_GUI_SURFACES.tsv/F0_VAR_ENV_REFERENCES.tsv/F0_VAR_PUSH_ANCHORS.tsv aus vorhandenem Inventurskript neu erzeugen; die F0_VAR_MANIFEST.json pinnt den jeweils erzeugten Quell-HEAD (2026-10-24: `9dbb6607`) und ist Lexer-Evidence; die semantische Zuordnung mit Statusklassen steht in [F0_VAR_01_MATRIX](../../project_0/docs/F0_VAR_01_MATRIX.md) (Paket 0). Alle gegenwärtigen Controls/Labels gegen HEAD, native Besitzer/Typen/Units, Qualität, Schutzgates und bestehenden Topic-/Command-Vertrag einzeln zuordnen. Kein neuer unabhängiger TS-Defaults-Katalog.
- **F0-P0UI-2 EXISTING-CAPTURE:** bereits vorhandene Capture-Prozesskontrolle mit CapturePanel UI **einmal** zusammenführen; Optionen, die Gateway-Start derzeit nicht akzeptiert, bleiben klar "DRAFT / NOT APPLIED". Besonders index/crosscheck und die nicht bewiesene --mode-CLI-Semantik nicht als startfähige Optionen anzeigen.
- **F0-P0UI-3 NATIVE-STATUS:** nur wirklich vorhandene Publisher und validierte Protobuf-Payloads freischalten, projektgebundene Wallets/Analytics/Liquidator Topics sonst UNAVAILABLE belassen. Jede neue Payload braucht engine→reporter→gateway→frontend und Rust/TS-Goldens plus Quality-/Generation-/Gap-Tests.
- **F0-P0UI-4 QUELLEN/GATES:** S1-02 Owner-Integration, sichere Mode-Wechsel und A23, später authentisierte Mode-Commands/Status; UI OFF/OBSERVE/ACTIVE erst nach qualifiziertem Native-Control, LIVE separat.
- **F0-P0UI-5 DESIGN/TEST:** exakt fünf Graphite-Tabs und Wallets-Layout, Pixel-/A11y-Regression. Playwright "Check & start sendet keinen Fake-Start", bestehender command receipt/reopen/stop-flow, Invalid Params, 429/DETACHED, Epoch/Schema-Konflikt, NO_ECONOMIC_GO und Keine-Vermischung mit BSC. Testresultate nur bei echter Ausführung PASS.

**Design-Leitlinie:** Heute vorhandene UI-Komponenten wiederverwenden, Inhalte innerhalb der fünf Tabs verfeinern. Fehlende Backendfelder sind offen/UNKNOWN/DISABLED mit fachlichem Grund, nicht als fertige Toggle-Logik oder Nullwert. Die globalen Overview-Capture-Controls sind eine **weitere UI-Position desselben bestehenden Actions**, keine zweite Command-Implementierung.
