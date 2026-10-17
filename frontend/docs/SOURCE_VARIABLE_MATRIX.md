# DarkBot Frontend – Source-Variable-Matrix Project_0 / BSC

**Stand:** 17.10.2026. **Status:** **F0-VAR INITIAL EXTRACT (PARTIAL)** aus konkreten Rust-/CLI-Quellen und bestehender Code-Map. Noch **kein** vollständig verifizierter, kompilierter Feldcensus für jedes zukünftige UI-Feld. Die Pflicht zur vollständigen Inventur *vor* der Anbindung steht im [Masterplan](FRONTEND_PHASENPLAN.md), F0. Der Browser zeigt bis dahin UNKNOWN statt Scheinwerte.

## 0. Eigentum, Typen, Feldschema und Extraktionsverfahren

Ein UI-Feld wird erst als implementierbar freigegeben, wenn folgender Satz ausgefüllt und gegen HEAD geprüft ist:

~~~text
field_id | project_id | feature/panel | source_path | source_symbol |
source_commit_sha | native_type | wire_type | unit_and_scale |
default_origin | min_max_and_validator | read_or_command |
topic_or_command | snapshot_or_evidence_id | update_semantics |
provenance_and_quality | sensitivity | required_gate | verification_test
~~~

Jede Variable benötigt genau einen nativen fachlichen Owner. 'EXISTS_INTERNAL' ist nicht 'EXPOSED_TO_GATEWAY', 'QUALIFIED' oder 'AUTHORIZED'. Keine Source-Variable nur anhand eines Namens zur editierbaren React-Einstellung machen. Secrets dürfen nicht in Protobuf oder VITE-Umgebungsvariablen geraten. Versionierte Engine-Descriptors für Config statt abgeschriebener React-Defaults. Exakte Geldwerte: lamports/slots/passende positive USD-micro als unsigned 64-bit → TS BigInt; Wei/u128/u256 und große/negative USD-micro/PnL als kanonische Decimal-Strings mit verbindlicher Skala. Niemals JS-float-basierte Echtgeld-Entscheidungen.

## 1. Project_0 – belegte Source-Felder

**Sourcebasis:** [PROJECT_0_CODE_MAP.md](PROJECT_0_CODE_MAP.md) sowie native Sources unter 'project_0/native/crates' und 'project_0/operations/cli'. **Wichtiger veralteter Map-Satz:** Die ältere Map bezeichnet 'Project0Page.tsx' an einigen Stellen als Platzhalter; aktuell ist die **Graphite V0 React-Shell bereits implementiert**, jedoch ohne qualifizierten Control-Backendanschluss.

| Fach-/UI-Feld | Belegter Owner / Source | Typ/Einheit, Gate |
|---|---|---|
| Parameterdescriptoren | 'solana/src/runtime_config.rs::{RuntimeConfig,Limits,Resolved::parse,descriptors}' | Name/Typ/Einheit/Default/Min/Max/ENV/change; echtes Schema nur aus Engine |
| RPC-Quote | 'P0_RPC_RPS,P0_RPC_BURST,P0_RPC_CONCURRENCY,P0_RPC_QUEUE' | req/s / count, Backendlimits |
| RPC-Timeout/Retry | 'P0_RPC_TIMEOUT_MS,P0_RPC_ATTEMPTS,P0_RPC_BACKOFF_MS' | ms / count |
| Subsystem-Quotas | 'P0_HOTPATH_RPS,P0_BOOTSTRAP_RPS,P0_HISTORY_RPS,P0_ANALYTICS_RPS,P0_SIMULATION_RPS' | req/s; UI ist kein zweiter Quota-Owner |
| WSS | 'P0_WSS_CONNECTIONS,P0_WSS_SUBSCRIPTIONS,P0_WSS_IDLE_TIMEOUT_MS,P0_WSS_RECOVERY_MS' | count / ms; Sanitized Read-Status |
| Workerruntime | 'RAYON_NUM_THREADS,P0_INGRESS_BUFFER,P0_REMOTE_WORKER_BUFFER' | worker/count; **nicht Botanzahl** |
| Run-Lifecycle | 'P0_ECON_BOOTSTRAP_SECS,P0_MARKET_READY_TIMEOUT_SECS,P0_ECON_DRAIN_SECS,P0_MARKET_DRAIN_TIMEOUT_SECS' | seconds, Backend Owner |
| Reconcile/Gaps | 'P0_MARKET_RECONCILE_PHASE_MAX_SECS,P0_MARKET_GAP_PHASE_MAX_SECS,P0_MARKET_ENRICH_PHASE_MAX_SECS' | seconds |
| Strict-Budgets | 'P0_VERIFY_RPS,P0_VERIFY_CALLS,P0_VERIFY_SECONDS,P0_VERIFY_MIB,P0_VERIFY_DISK_MIB,P0_VERIFY_RAM_MIB,P0_VERIFY_FREE_MIB' | req/s,count,s,MiB |
| Storage | 'P0_MARKET_CAPTURE_INITIAL_MIB,P0_MARKET_CAPTURE_MAX_MIB,P0_SYSTEM_RESERVE_MIB,P0_MARKET_WRITER_BATCH_FRAMES,P0_MARKET_WRITER_FLUSH_MS,P0_MARKET_WRITER_SYNC_MS,P0_MARKET_WRITER_STALL_MS,P0_MARKET_DISK_RESERVE_MIB,P0_CAPACITY_SAFETY_PERCENT' | MiB/frames/ms/% |
| Capture-Dauer | 'runtime/src/analytics/measurement.rs::MeasurementPlan::parse_duration', '--duration' | IST endlich s/m/h, SOLL days/UntilStopped nach nativer Freigabe |
| Capturequelle | 'operations/cli/measurement_contract.py::RunConfig.capture_source', '--capture-source' | 'logs|transactions'; Qualität gesondert |
| Verify | 'VerificationConfig.mode', '--chain-verify' | 'off|index|strict|crosscheck'; Quality und Proof pro Run |
| Markerauswahl | 'ReportQuery.markers', '--report-markers' | sechs Marker; **REPORT ONLY**, nie Capture-Filter |
| Klassenfilter | 'ReportQuery.types', '--report-types' | vier Klassen; Anzeige, keine Capture-Löschung |
| Related-Filter | '--report-related-events, --report-related-types' | vier getrennte Related Types |
| Report Scope | '--report-view, --report-provenance, --report-from, --report-to, --report-min-net-usd, --verification-result' | Renderer/Query; '-report-min-net-usd' ist **nicht** 'P0_MIN_PROFIT' |
| Verifier-Budget | '--verify-rps, --verify-max-calls, --verify-max-seconds, --verify-max-bytes, --resume-verification' | bounded strict, kein Capture-Restart |
| MarketStatus | 'runtime/src/market/core.rs::MarketStats' | 'gap_count,transport_http_429,ingress_overflow_count,last_observed_slot,max_stream_silence_ms' |
| WriterStatus | 'runtime/src/storage/writer.rs::StorageStatus' | 'queue_bytes,lag_frames,write_bytes_per_s,disk_free_bytes,disk_exhaustion_s,budget_exhaustion_s' |
| CapacityGate | 'runtime/src/storage/preflight.rs::CapacityReport' | 'basis,write_bytes_per_s,verdict,start_allowed'; aktueller produktiver Qualifikationsstatus separat |
| Journal-Provenance | 'runtime/src/analytics/journal.rs::Provenance' | 'source,source_seq,slot,fork,rx_ns,unix_ns', internes '.p0j' |
| Chain-Status | 'solana/src/chain.rs::{ChainTx,ChainMeta,ChainBlock,TxStatus}' | slot/signature/failed/unknown, Legacy/v0/v1, LUT/CPI |
| Economics | 'liquidation/src/economics.rs::{CandidateInput,Evaluation,actionable}' | fees/funding/rent/UNKNOWN, Native Engine Owner |
| Kapital-Szenario | 'runtime/src/analytics/econ.rs', 'P0_ECON_CAPITAL_USD' | 1–32 positive u64 Szenario-Werte; **nicht** reales Walletbudget |
| OneShot/WalletLock | 'runtime/src/execution/one_shot.rs' | 'Intent,WalletLock,Phase,Journal'; nur teilweise produktiv |
| Finalisierte Receipts | 'runtime/src/execution/one_shot/receipts.rs::FinalizedObservation' | 'wallet_sol_delta_lamports,wallet_token_deltas,fee_lamports'; kein eigener PnL-Beweis |
| PendingReconcile | 'runtime/src/execution/pending.rs::{PendingSubmission,ReconcileRecord}' | idempotent + UNKNOWN, Qualität noch zu binden |
| Öffentliche P0 Wallet-Identität | 'project_0/config/wallet.public.env::{P0_EXPECTED_PUBKEY,P0_WALLET_STATUS}' | Public, kein Private Key |

**Sechs echte Marginfi-Instruktionsmarker:** 'lending_account_liquidate' = CLASSIC; 'start_liquidation/end_liquidation' = RECEIVERSHIP; 'start_deleverage/end_deleverage' = FORCED_DELEVERAGE; 'lending_pool_handle_bankruptcy' = BANKRUPTCY_SETTLEMENT. Vier Related ohne profitable-Liquidation-Gleichsetzung: 'drift_claim_bad_debt', 'purge_deleverage_balance', 'marginfi_account_keeper_close_order', 'marginfi_account_init_liq_record'.

**P0-Felder derzeit nicht als fertige Control-/Snapshot-Schnittstelle belegt (SOLL_MISSING):** 'PortfolioFinancialSnapshot', 'finance_snapshot_id', 'realized_net_pnl_usd_micro', 'total_capital_usd_micro', 'config_version', 'profile_version', 'pending_commands[]', 'recent_command_results[]', Rolling-RPC-/Decision-p95 mit Fenster und Samplezahl, native persistente Fleetstrategien, Auto-Sweep-/Treasury-Sender. Der in der UI vorhandene 'useFleetDrafts' ist **nur React-Memory**. Kein P0-Bot-Schwarm und keine auto-startbaren Profile.

## 2. BSC – konkrete Rust- und Schemafelder

**Sourcebasis:** 'bsc/engine/crates/execution/src/acceptance_guard.rs', 'bsc/engine/crates/record/src/schema/{g07,g08}.rs', 'bsc/engine/crates/execution/src/bin/g07_mainnet_self_launch_buy/main.rs', 'bsc/engine/crates/execution/src/bin/g07_pre_live_readiness.rs', 'bsc/engine/crates/qualification/src/bin/g08_mainnet_verify.rs', 'bsc/engine/crates/trading/src/submission_policy/plan.rs', 'bsc/engine/crates/journal/src/' und Status/Evidence.

| Fach-/UI-Feld | Belegtes Symbol | Typ, Ownership |
|---|---|---|
| Vertragsfreigabe | 'acceptance_guard.rs::FROZEN_APPROVED_V4_SHA256' | IST 'UNARMED': UI muss BLOCKED anzeigen |
| Signierte Identitäten | 'LiveGuardContext.{launcher_address,buyer_address,chain_id,portal_address}' | EVM public addresses/u64, Gate-gebunden |
| Signed Intent | 'LiveGuardContext.{launch_value_wei,buy_value_wei,gas_price_wei,launch_gas_limit,buy_gas_limit,direct_builder_tip_wei,spent_budget_wei,reserved_attempt_wei,attempt_number}' | u128 Wei/u64 Gas/Count, reine Anzeige bis Gate |
| Contract Status | 'ContractSnapshot.{approved,execution_ready,attempt_id,max_attempts}' | bool/ID/count; **keine** React-Freigabe |
| Contract Amount-Caps | 'ContractSnapshot.{max_buy_value_per_attempt,max_launch_value_per_attempt,max_total_per_attempt,absolute_budget_wei}' | u128 Wei → decimal string |
| Contract Gas-Caps | 'ContractSnapshot.{max_launch_gas_limit,max_buy_gas_limit,max_gas_price_wei,max_direct_builder_tip_wei}' | gas count/Wei |
| DRY / Builder / Witness binding | 'ContractSnapshot.{dry_evidence,preflight_evidence,builder_policy_evidence,producer_witness_source}' | 'InputHashes' provenance, Evidence Owner |
| Portal Drift | 'record/src/schema/g07.rs::{SUPERSEDES_CONTRACT_SHA256,PORTAL_UPGRADE_RECORD_SHA256,PORTAL_UPGRADE_RECORD_PATH}' | hash/path ID; Dashboard nur verifizierte Drift |
| V4 Contract fields | 'record/src/schema/g07.rs::contract' | APPROVED, EXECUTION_READY, ATTEMPT_ID, DRY/PREFLIGHT/BUILDER/WITNESS SHA256, CHAIN_ID, PORTAL, MAX_ATTEMPTS, MAX_* |
| G0.7 Evidence | 'record/src/schema/g07.rs::evidence' | ATTEMPT_ID, STATE_DIR, LAUNCH_TX_HASH, BUY_TX_HASH, TOKEN, GAS/BUDGET, CAPTURE_STATUS, JOURNAL, CLOCK_METADATA; nur echte run-Evidence |
| G0.6a Preflight | 'execution/src/bin/g06a_preflight_record.rs' | launcher/buyer, chain read, gas/Wei caps, Provider qualifiziert separat |
| G0.7 Readiness | 'execution/src/bin/g07_pre_live_readiness.rs' | version/Guard/Clock/Boot/RPC/Policy/Witness; kein UI-Send ohne Freigabe |
| Builder policy | 'trading/src/submission_policy/plan.rs::{EnvelopeOptions,ProviderValidity,BudgetEnvelope,PlanCosts}' | no_merge/position_first/block bounds/Costs; immer Gate-gebunden |
| G0.8 Receipt identity | 'record/src/schema/g08.rs::result' | LAUNCH_TX_HASH, BUY_TX_HASH, LAUNCH_STATUS, BUY_STATUS, launch/buy block/index |
| G0.8 Canonical & Acquisition | 'g08.rs::result' | ORDERED, SAME_BLOCK, ADJACENT, CANONICAL, ACQUISITION_EVIDENCE_COMPLETE, BUYER_TOKEN_BALANCE_AT_BUY_BLOCK |
| G0.8 Cost | 'g08.rs::result' | LAUNCH_GAS_COST_WEI, BUY_GAS_COST_WEI, BUY_VALUE_WEI, BUYER_TOTAL_DEBIT_WEI, EXPERIMENT_TOTAL_COST_WEI, DIRECT_BUILDER_TIP_WEI |
| Producer Timing | 'g08.rs::result::T0_TO_BUY', 'qualification/src/bin/g08_producer_timing.rs' | **nur** mit originalem Witness, nicht aus BSC-Blocktimestamp ableiten |
| G0.8 Inputs | 'qualification/src/bin/g08_mainnet_verify.rs' | 'DARKBOT_G08_G07_EVIDENCE,DARKBOT_G08_CHAIN_JOURNAL,DARKBOT_G08_EVIDENCE_PATH' |
| Wallet public | 'bsc/config/wallet.public.env::{BSC_EXPECTED_ADDRESS,BSC_WALLET_STATUS}' | public addr/status, kein Treasury-API-Beweis |
| Native BSC Journal | 'bsc/engine/crates/journal/src/{format,recorder,replay}.rs' | nicht P0-.p0j, eigener Evidence-Owner |
| BSC IPC | 'bsc/engine/crates/geth_ipc/src/protocol_v2/records.rs::OwnedRecordV2' | seq/lane_epoch/clock/timestamp, nicht Browser-Cursor |

**Runner ENV aus tatsächlichen BSC-Binaries (IST, **nicht** automatisch freigegebene UI-Eingaben):**

- Gate-/Evidenzidentität: 'DARKBOT_G07_ATTEMPT_ID', 'DARKBOT_G07_ATTEMPT_NUMBER', 'DARKBOT_G07_DRY_EVIDENCE', 'DARKBOT_G07_PREFLIGHT_EVIDENCE', 'DARKBOT_G07_BUILDER_POLICY_EVIDENCE', 'DARKBOT_G07_PRODUCER_WITNESS_CONTRACT', 'DARKBOT_G07_EVIDENCE_PATH', 'DARKBOT_G07_CAPTURE_JOURNAL', 'DARKBOT_G07_CLOCK_ID', 'DARKBOT_G07_BOOT_ID', 'DARKBOT_G07_SPENT_BUDGET_WEI'.
- Value-sensitive / nicht automatisch GUI-steuerbar: 'DARKBOT_G07_MAINNET_SEND', 'DARKBOT_G07_SALT', 'DARKBOT_G07_PROBE_SEND', 'DARKBOT_G07_PROBE_APPROVED_PLAN_SHA256'. Freigabe an exakten genehmigten Contract gebunden.
- **SECRET_NEVER_BROWSER:** 'DARKBOT_G07_BUYER_PRIVATE_KEY', 'DARKBOT_G07_LAUNCHER_PRIVATE_KEY', 'DARKBOT_G07_BLOCKRAZOR_AUTHORIZATION', credentialed 'BSC_MAINNET_RPC_HTTPS' / 'DARKBOT_G06A_ARCHIVE_RPC_HTTPS'. Browser nur bereinigten Quellstatus, nie Endpunkt mit Secret.
- Verifier-Scope: 'DARKBOT_G08_PRODUCER_WITNESS', 'DARKBOT_G08_TIMING_EVIDENCE_PATH' sowie oben genannte G08-Quellen. Path-Inputs bleiben auf dem Backend.

**BSC SOLL_MISSING für Frontend:** read-only 'bsc.status' und 'bsc.launch.state' Adapter, 'config_version/plan_version/pending_commands[]/recent_command_results[]', BSC-spezifische Portfolio-/Finance-Projektion, qualifizierter Gateway-Journal-Cursor, direkte Wallettransfer-Commands. NICHT implementiert durch den bloßen Rust-G0.7-Runner. **G0.7/G0.8 NOT_EXECUTED, LIVE BLOCKED, V4 UNARMED, v5.25 Requalification offen.**

## 3. Neue Common-Projektionen – explizit SOLL, nicht aus Rust erfunden

| Projekt/Topic | Vorgesehene Anzeige/Command-Felder | Owner |
|---|---|---|
| 'overview.aggregates' | project_id,label,route,health,readiness,execution_authorization,running_instances,active_jobs,warnings,as_of | Gateway read-only Aggregation, native Source-Proof |
| 'project0.status' | config_version,pending_commands[],recent_command_results[],source_quality,storage,transport,rpc/decision_latency_series | Project0 Backend + Gateway quality overlay |
| 'project0.finance.summary' | finance_snapshot_id,realized_net_pnl_usd_micro,total_capital_usd_micro,valuation_quality,pnl_quality | P0 finalisiertes Portfolio, SOLL |
| 'project0.liquidator.state' | native profile_version, bot_id/phase/limits/fees; React Drafts != active | P0 Runtime, vor F8 nur EIN Bot |
| 'bsc.status' | G0.6a/G0.7/G0.8 status, portal drift, builder status, config_version, pending/recent commands | BSC native status adapter |
| 'bsc.launch.state' | plan_version, signed parameters, live gate, attempt identity | BSC acceptance guard, no generic UI send |
| EVENT | topic,journal_epoch,durable_cursor,event_id,run/source/proof | native Journal, not gateway seq |
| Command | action,project_id,request_id,resource_id,expected_profile_version/expected_config_version,capability_version | Engine idempotency/authorization |

## 4. F0-VAR harte Vorprüfungen

1. Aktuellen Git-HEAD und alle Änderungen festhalten. Jedes in Graphite/BSC vorgesehene UI-Feld an Source-Symbol und Einheiten binden; 'SOLL_MISSING' mit Ticket referenzieren.
2. P0 'Resolved::descriptors()' und CLI-Eingänge gegen aktuelle Rust/Python-Version testen: Feld-Typ/Default/Min/Max/ENV, Prioritäten, restart_required. Six marker/four kinds/related per tatsächlichem Parser.
3. BSC Guard, DBREC-v4, G0.7-Inputs/G0.8-Resultate, Portal-Upgrade-Records aus genau derselben Git-Version lesen; Version/Witness/Builder-/DRY-Gates in Read-only Projektion berücksichtigen.
4. Sensitivity/Authorization inventarisieren: PUBLIC_READ / SANITIZED_READ / CONTROL_AFTER_GATE / SECRET_NEVER_BROWSER. Kein Browser-Schalter für BSC frozen Gas/Buy-Betrag; keine selbstständig kopierten P0-Defaults.
5. Native/Protobuf-Typen und Skalen festlegen; Negative PnL/Wei-Strings und Unsicherheit/Finalität Golden testen. Kein falscher 0-Wert.
6. Field-source-machine-readable Manifest + Validator als **F0-Lieferobjekt**: Source-SHA, Owner, Gate, Unit, Schema, Tests. Fehlende oder widersprüchliche Daten blockieren ihren Adapter, **nicht** die anderen Projekte.
7. Verbotene Frontend-Wallet-/Private-Key-/Secret-Aktionen in Negativtests nachweisen.

**F0-VAR-Abnahme:** Datenfelder/Commands und schreibgeschützte Quellen sind vollständig bezogen auf den definierten UI-Scope inventarisiert; offene native Adapter erhalten eigene Tickets. Die **hier** vorliegende Matrix ist ein initialer, quellenbasierter Startstand – **nicht** die behauptete vollständige Codegen-Schnittstelle.
