# V0 → Production: Project_0 Fleet control, source variables, persistence and refresh

**As-of code inspection:** 2026-10-17 `sawyerma/darkbot` on `main`. This is a detailed implementation checklist, **not** a claim all fields/paths exist. Sources: [code map](PROJECT_0_CODE_MAP.md), [frontend plan](FRONTEND_PHASENPLAN.md), [design freeze](V0_GRAPHITE_FLEET.md), [Project_0 STATUS](../../project_0/docs/STATUS.md) and [PHASENPLAN](../../project_0/docs/PHASENPLAN.md). No new Rust trading engine, SQLite/Redis runtime, internal JSON, paid data or LIVE permission.

## 1. Exact React ownership tree

```text
frontend/                                      one existing React/Vite application
├── docs/
│   ├── V0_GRAPHITE_FLEET.md                   visual and functionality freeze
│   ├── V0_FLEET_IMPLEMENTATION_PLAN.md       this source-backed work plan
│   ├── FRONTEND_PHASENPLAN.md                FEUI-00…10 gates
│   ├── PROJECT_0_CODE_MAP.md                 verified upstream symbols/units
│   ├── CONTROL_PROTOCOL.md                   proposed backend API/DTO semantics
│   └── WALLETS_TREASURY.md                   payments/ledger approval
└── src/
    ├── app/
    │   └── router.tsx                        EXISTING /project-0 route; do not duplicate
    ├── core/
    │   ├── api/{client.ts,endpoints.ts}       EXISTING read-only apiGet; no P0 POST today
    │   ├── realtime/{RealtimeClient.ts,
    │   │              protocol.ts}           EXISTING browser WS reconnection/gap
    │   └── types/system.ts                   EXISTING project IDs, NOT LIVE approval
    ├── features/project0/
    │   ├── Project0Page.tsx                   composition, tabs, NO execution rules
    │   ├── styles/graphite.css                scoped V0 layout/tokens (wallet frozen)
    │   ├── model/
    │   │   ├── contracts.ts                   view-only draft and SOLL backend DTOs
    │   │   └── format.ts                      BigInt-based exact display only
    │   ├── hooks/
    │   │   └── useFleetDrafts.ts              temporary React memory ONLY
    │   ├── components/
    │   │   └── PanelPrimitives.tsx            Section/DetailRow/TabStrip/UnknownMetric
    │   └── pages/
    │       ├── OverviewPanel.tsx              matching finance snapshot + latency
    │       ├── LiquidatorPanel.tsx            many drafts, one selection, edit/clone
    │       ├── CapturePanel.tsx               source/duration/marker/verify views
    │       ├── AnalyticsPanel.tsx             offline backtest and strategy draft transfer
    │       └── WalletsPanel.tsx               frozen V0 Balance/Send/Auto-Sweep
    ├── features/{bsc,solana}/                 unchanged / isolated
    └── shared/layout/                        existing DarkBot global navigation
```

**Next modules only once server contracts are built:** `core/api/project0.ts` (read-only P0 DTO mapper, write actions independently gated), `features/project0/api/{capabilities,portfolio,fleet,capture,reports}.ts`, `features/project0/hooks/{usePortfolio,useFleetSnapshot,useCaptureSnapshot,useRunList,useHealth,useControlEvents}.ts`, `features/project0/model/{runtimeDescriptors,fleet,treasury}.ts`. Keep data access out of pages; hooks manage server state, pages compose panels. Avoid a large monolithic `Project0Page` and reimplementing domain math in React.

## 2. Existing physical source and parameter mapping (verified)

| UI / source of truth | Actual code symbol / env / CLI | Present or missing | Required adaptation |
|---|---|---|---|
| Duration | `solana/src/runtime_config.rs::duration_seconds`, `runtime/src/analytics/measurement.rs::MeasurementPlan::parse_duration`; `P0_MEASUREMENT_DURATION`, `P0_ECON_DURATION_SECS`, `P0_MARKET_DURATION_SECS` | EXISTS; positive integer s/m/h only | typed `RunHorizon::Fixed/UntilStopped`, support days/checkpoints; backend parser and preflight; UI shortcuts not constraints |
| Measurement modes | `runtime/src/orchestration/startup.rs` / `P0_MODE=market|econ4h|shadow` | EXISTS in runtime, not arbitrary LIVE combinations | launch contracts and same ingress for parallel fleet |
| Capture source | `operations/cli/measurement_contract.py::RunConfig.capture_source`, `--capture-source logs|transactions` | EXISTS; transaction feed not FREE_ONLY qualified | provider binding and honest errors; no fabricated full coverage |
| Chain verification | `VerificationConfig.mode`, `--chain-verify off|index|strict|crosscheck` | EXISTS, not all provider paths qualified | async verify job with proof/attempt/result IDs; index != strict |
| Six onchain marker flags | `MARKERS`; `--report-markers all|none|list` and `KINDS`, `RELATED` | EXISTS for REPORT rendering | UI checked set → ReportQuery only; never filter capture or executable classes |
| Economics capital scenarios | `runtime/src/analytics/econ.rs::P0_ECON_CAPITAL_USD` supports 1…32 positive u64 entries, default `CAPITAL_USD` | EXISTS partly; `econ_canonical.rs` still enumerates default buckets in report branch | eliminate fixed bucket duplication and support arbitrary positive validated amounts; distinguish from real wallet allocation |
| Minimum actionable profit | `liquidation/src/economics.rs::rank/actionable(min_profit:i128)`; `P0_MIN_PROFIT` set separately by launcher | EXISTS in different owners | native typed strategy parameter with exact USD-micro units, signer/fee/risk gate; no direct unvalidated web amount |
| Fee limit | `liquidation/src/economics.rs::CandidateInput` fee components + fee quoting | COST MODEL PARTIAL, no approved fleet fee cap DTO | define per-bot fee budget and global cost arbitration in Rust |
| Bot concurrency | `solana/src/runtime_config.rs::workers=1`, `worker_queue=256`, `ingress_queue=65536`; `RAYON_NUM_THREADS`, `P0_REMOTE_WORKER_BUFFER`, `P0_INGRESS_BUFFER` | EXISTING GLOBAL runtime resources; per-bot in-flight parameter **MISSING** | separately typed bot cap, global capped arbiter, contention/latency tests; never map worker count to bot count |
| RPC capacity | `RuntimeConfig::Limits`, `P0_RPC_RPS=5`, `P0_RPC_CONCURRENCY=2`, `P0_WSS_CONNECTIONS=2`, `P0_HISTORY_RPS=2` | EXISTS | shared quota allocation across bots/capture/verification, frontend descriptors read-only |
| Live status / gaps | `runtime/src/market/core.rs::MarketStats`: `gap_count,transport_http_429,last_observed_slot,max_stream_silence_ms,ingress_overflow_count` | EXISTS internal | typed bounded StatusSnapshot, freshness and provenance via backend |
| Writer status | `runtime/src/storage/writer.rs::StorageStatus`: `queue_bytes,lag_frames,write_bytes_per_s,disk_free_bytes,disk_exhaustion_s` | EXISTS internal | no UI writer polling from hotpath; periodic coalesced immutable read snapshot |
| RPC / decision p95 | `solana/src/ingress::t_rx_ns`; `runtime/src/analytics/econ.rs::Distribution` | partial timing support, no verified rolling per-provider RPC p95 API | typed native quantile histograms with window/sample count, provider id and monotonic timing |
| Bot-level PnL/latency | `one_shot/receipts.rs::FinalizedObservation`, `execution/pending.rs`, shared economics | PARTIAL financial primitives; bot attribution/persistent execution records **MISSING** | bind receipt ↔ bot_id/strategy_version ↔ wallet reservation, no double booking |
| Financial fields | `one_shot.rs::WalletLock`, `receipts.rs`, [finance contract](GRAPHITE_V5_UI_FINANCE_CONTRACT.md) | `PortfolioFinancialSnapshot` MISSING; FE-07 NOT_RUN | all-time/as-of proof, wallet universe, liabilities, fees, USD quotes; same snapshot on Overview/Wallets |
| Wallet payment | `config/wallet.public.env::P0_EXPECTED_PUBKEY` and partial FE-04 journal | Public identity only; actual treasury sender MISSING | destination whitelist + transfer intent, explicit approvals, transfer checked, finalized balance receipts |
| Dynamic profile | no `FleetStrategyProfile`/fleet scheduler in Rust; `useFleetDrafts` only UI state | MISSING | native typed immutable profile/config + qualification / activation; project_0 control server endpoints |
| Boot audit | existing `storage/preflight.rs::CapacityReport`; provider/ProgramData gate partial | no unified `p0ctl doctor` implementation | verify genesis/ProgramData/owners/Bank/Oracle/Mint/Wallet and WSS/RPC/disk; critical failures block |
| LIVE permission | `project_0/docs/STATUS.md`: FE-06 NOT_AUTHORIZED, `P0_MODE=live` refused | **DISABLED** | keep disabled regardless of profile preview; separately authorized future FE-06 |

**Source path abbreviations:** `runtime/src/*` means `project_0/native/crates/runtime/src/*`; `solana/src/*` means `project_0/native/crates/solana/src/*`.

## 3. How state is stored TODAY vs future (no false persistence)

| State class | Current actual persistence | V0 front-end behavior | Production owner / required change |
|---|---|---|---|
| Active Run/Owner, raw market, chain proof, economics | `project_0/data/` via existing run store / binary `.p0j` append-only JournalSink; legacy JSON/JSONL views are offline rendering, migration JFI still open | NEVER creates records or fake run IDs | Rust run store + typed journal and snapshot projection |
| CLI configuration | `RuntimeConfig::Resolved::parse` from profile < environment < config file < CLI; existing files `project_0/config/profiles/*.json` and old Python `measurement_contract` manifests | Not changed by React | native typed profile/config provenance; replace operational JSON through approved JFI plan |
| Bot strategy drafts | NO native `BotProfile` persistent storage | `useState` in `useFleetDrafts`, discarded on page unmount/reload; **not saved** | versioned immutable typed `FleetStrategyProfile`, journaled revisions with IDs, hashes, audit and atomic commit; no localStorage as authority |
| Running bot instances | one existing market/econ/one-shot architecture, NO qualified multi-bot scheduler | Always `0 · not authorized` | runtime arbiter with distinct bot IDs, execution admission, locks and per-bot status |
| Wallet balances and finance | account state + FE-04 partial finalize/receipt journal; no `PortfolioFinancialSnapshot` | Always `—` / unknown; no calculated fake balance | read-only account + positions/asset quotes + liability reconciliation with immutable snapshot ID |
| Treasury destinations, policies, transfers | `wallet.public.env` identity only, treasury automation absent | Inputs are temporary, no transfer authorization | typed per-wallet destination and policy records, standalone approval, finalized receipts, safety gates |
| UI current tab / report filter selection | No backend reader/writer | ephemeral React state, reset on page reload | optional non-authoritative URL tab query; report selection versioned by server for reproducibility |

**Fleet strategy data model (SOLL; do not assume exists):** `FleetStrategyProfile {profile_id,version,hash,kind,parameters,enabled_classes,allowed_mode,qualification_ids}`; `BotInstance {bot_id,profile_id,instance_generation,status,capital_limit_usd_micro,min_net_usd_micro,max_fee_usd_micro,max_in_flight,source_capture_run_ids}`; `FleetState {fleet_id,owner_id,instances,shared_admission_policy,versions}`; `Reservation {intent_id,bot_id,wallet_id,asset_mint,atomic_amount,expires,proof}`. All money atomics use integers, not JS floats. A bot may run different validated strategies but only after its class' instruction semantics and LIVE gates qualify.

**No unbounded duplicate streams:** one shared ingress/canonical state fan-out → N strategy evaluations → one global candidate dedupe + admission/tx arbiter → one FE-04 reservation and signer/submit owner. Multiple bots evaluating same candidate must not spend the same funds or race each other accidentally. Capture remains unfiltered alongside eventual qualified execution and Shadow; offline analyzer reads sealed segments with bounded resource budget.

## 4. Refresh/update pipeline and error behavior (SOLL)

```text
backend journal/.p0j -> native typed read projection
   -> GET /api/v1/project0/capabilities  [boot + version handshake]
   -> GET /api/v1/project0/status        [stable snapshot ID]
   -> GET /api/v1/project0/fleet         [typed fleet and per-bot status]
   -> GET /api/v1/project0/portfolio     [same finance snapshot Overview+Wallets]
   -> GET /api/v1/project0/runs          [paged immutable run manifests]
   -> GET /api/v1/project0/reports       [query/results with verification IDs]
   -> server-authenticated /ws/v1/events [coalesced, bounded delta events]
   -> UI query cache keyed by (project, run/profile/snapshot ID, backend schema)
```

**Existing client behavior:** `apiGet<T>` uses `VITE_API_BASE_URL=/api/v1`, JSON browser transport; Vite proxies dev requests to 8100 but this is NOT a server. `RealtimeClient` connects to `/ws/v1/events`, parses envelopes `{seq,ts_ns,type,project?,payload}`, exponential reconnect ≤ 5 s and emits gap notifications; on gap it **does not** automatically reload REST state. `useRealtimeStatus` reports connection only. JFI permits only explicit external wire boundaries; finalize a versioned authorized Browser Wire contract before implementing server.

**Required synchronization:**
1. On mount verify capabilities/schema/hash, fetch initial snapshots. Backend reports independently: `UNKNOWN`, `BLOCKED`, `PARTIAL`, `ACTIVE`; UI must not synthesize readiness.
2. A backend push includes `stream_generation`, `seq`, `snapshot_id`, `entity_id`, `entity_version`, `as_of_slot`, `observed_at`. Discard stale and cross-project deltas. Reconnect/sequence gap/version mismatch → `STALE` badge + abort outstanding stale merge → full REST resync → resume only from matching stream generation.
3. Coalesce status events at backend (suggested dashboard telemetry cadence ≤ 1 Hz UI, configurable, **not** fixed as operational requirement). Use normal request/response for editor actions, no per-event hook rerender and no one WS per bot.
4. Controlled query invalidation: Overview/Wallets share single `portfolio` query; Run status and Fleet status separately keyed; reports require immutable `verification_result_id` and `query_hash`. Invalidate on acknowledged backend version, not after merely clicking a button.
5. Browser background/suspended tabs use reconnect + snapshot, no missing `t_rx` fabrication. Provider/HTTP429/queue/disk faults label stale/incomplete. Server snapshots preserve exact u64/i128 USD-micro via strings/BigInt-safe boundaries.
6. UI draft edits do not update running bot: `EditDraft → Validate → SaveImmutableProfile → Qualify → PreparePlan → separate authorized Activate` (with optimistic compare-and-swap revision/ETag). Never send arbitrary UI params to hotpath or mutate active profile without a durable transition.
7. Multi-client/multi-user edit conflicts: backend CAS on `profile_version`; idempotency token per POST; lifecycle stable across browser close. Dedicated `RunStop` and `FleetBotStop` scopes (not global kill).

## 5. Backend work packages / phase mapping

- **FLEET-00:** accept V0 visual + code structure and component tests; no change to Wallets when touching Liquidator.
- **FLEET-01:** Rust-native `p0ctl` and boot audit, feature-independent status/preflight/versioned DTOs; Python shell handoff migration per existing JFI, not duplicate CLI.
- **FLEET-02:** canonical one-state shared ingress fan-out with bounded priority lanes for capture, shadow, multiple class evaluations; per-bot identity/latency/fees/winner and resource budget.
- **FLEET-03:** typed immutable strategy profiles and draft/revision/provenance; preserve manual/Analytics origin; free capital/min net/fee/in-flight; input descriptors from `runtime_config.rs`, not hardcoded React domain values.
- **FLEET-04:** single global arbiter to enforce duplicate candidate suppression, wallet atomic reservation, global risk/capital/fee budget, serial/parallel sequencing and fair/prioritized scheduling. FE-04/P8 locks cannot be bypassed by independent bot worker.
- **FLEET-05:** native journal persistence + typed fleet control events; recovery/NO-LIVE guards; read-only projections; API auth/CSRF/origin/loopback binding and idempotent jobs; JFI-approved browser wire.
- **FLEET-06:** backtest/optimization uses true as-of historical replay + existing Rust economics, creates immutable candidate profile in DRAFT; benchmark multi-strategy portfolio not only individual net gain, include common cash reserve, correlation, interference and p95/p99 impact. No simulated winner auto-activation.
- **FLEET-07:** Wallet PortfolioFinancialSnapshot and bot-attributed finalized earnings, Total Capital and Realized Net PnL same source across Overview/Wallets; separately authorized manual/auto treasury after FE-07 and P8/funding gates.
- **FLEET-08:** integration/observability: up to N qualified bots, Capture + Shadow + offline replay concurrently; decision/submit/capture latency p50/p95/p99, queue and RPC pressure, provider recovery, crash and per-wallet double-reservation tests. Measure against baseline with identical load and run evidence.
- **FLEET-09:** LIVE remains LOCKED until Project_0 FE-06 separately authorizes exact executable code/wallet/strategy/caps. Creating a fleet does not grant new LIVE mode or bypass first-profit and pilot sequence.

**Ordering:** V0 React shell/design/reference → backend audit + single owner + 4h qualification → bounded multi-bot Shadow fleet and native persistence → UI read-only snapshots and control contract → offline optimizer profile promotion → strictly separate signed LIVE and treasury approval. The 4h raw proof must not wait for full fleet execution.

## 6. Required acceptance tests

- V0 visual reference freeze for all 5 tabs, especially Wallet balances rows/subtabs unchanged.
- Add, duplicate, remove, select, edit drafts: independent settings and **zero** changes to actual runtime/network/wallets; browser reload discards drafts until real Rust profile store exists.
- Report marker selection across 6 markers/4 classes changes only view query, raw .p0j identity invariant; liquidator supports executable classes only after qualification.
- Free positive decimal capital and thresholds; exact money units in backend, no floating JS authority or static `CAPITAL_USD` limits.
- Duplicate bot same signature/slot/transaction fails global arbitration; two bots reserve overlapping SOL/SPL funding atomically without double-spend; crash/restart UNKNOWN never resend.
- 1/N/large N fleet + concurrent unfiltered capture/Shadow/optimizer must not degrade qualified p95/p99 or exceed RPC/WSS, CPU/RAM/SSD/queue budgets.
- Stable snapshot_id for same Total Capital and Realized Net PnL in Overview and Wallets; missing USD valuation, own-wallet FE-07 or liabilities → UNKNOWN, no example profit.
- WS drop, reconnect, seq gap, slow browser, stale backend version, failed/partial API response require REST resync, without affecting engine; explicit health status.
- No unapproved LIVE, Paid Provider, signing, transfer or auto sweep exposed by V0 or by draft editing. Backtest cannot update active profile.
- Tests/certification: source-linked Rust unit + integration, browser typecheck/lint/build, Playwright interactions, no Mainnet in CI. Report exact PASS/FAIL/NOT_RUN evidence, not guesses.

## 7. Source file pointers

`frontend/src/features/project0/Project0Page.tsx`; `frontend/src/core/api/client.ts`; `frontend/src/core/realtime/{RealtimeClient.ts,protocol.ts}`; `project_0/native/crates/solana/src/runtime_config.rs`; `project_0/native/crates/runtime/src/{orchestration/startup.rs,analytics/measurement.rs,analytics/econ.rs,analytics/econ_canonical.rs,analytics/journal.rs,market/core.rs,storage/writer.rs,execution/one_shot.rs,execution/one_shot/receipts.rs}`; `project_0/native/crates/liquidation/src/{economics.rs,funding.rs}`; `project_0/operations/cli/measurement_contract.py`; `project_0/config/wallet.public.env`; `project_0/docs/{PHASENPLAN.md,STATUS.md,MEASUREMENT_LEDGER.md}`.

**No private keys or real wallet IDs embedded.** Old `wallet.public.env` is only a public identity, not evidence of available funds.
