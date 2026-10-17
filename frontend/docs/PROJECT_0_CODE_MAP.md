# Project_0 – geprüfte Code- und Variablenmatrix für das Frontend

Stand: 2026-10-17. Quellenbasis Project_0 `0e0544d`; bei Umsetzung aktuellen HEAD prüfen. **IST** = Symbol existiert, nicht zwingend QUALIFIED. **SOLL** = im [Frontend-Plan](FRONTEND_PHASENPLAN.md) vorgesehen. [Control-Protokoll](CONTROL_PROTOCOL.md).

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
