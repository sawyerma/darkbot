# Project 0 Control – Graphite V5 UI + Financial KPI Contract

**Status: binding SOLL design and acceptance contract; not an implemented API or validated portfolio ledger.**
**Version:** UI-V5.1, 2026-10-17. **Scope:** existing `frontend/` and Project_0 Rust runtime; no second engine/frontend.
**Authority:** [FRONTEND_PHASENPLAN.md](FRONTEND_PHASENPLAN.md), [PROJECT_0_CODE_MAP.md](PROJECT_0_CODE_MAP.md), [CONTROL_PROTOCOL.md](GATEWAY_WIRE_PROTOCOL_V1_3.md), [WALLETS_TREASURY.md](WALLETS_TREASURY.md), [Project_0 STATUS](../../project_0/docs/STATUS.md) and [PHASENPLAN](../../project_0/docs/PHASENPLAN.md).

## 1. Accepted visual reference: Graphite V5 (not V4)

The user accepted the graphite V5 concept. Preserve this appearance when converting `frontend/src/features/project0/Project0Page.tsx` from a placeholder into the real page:

- **Dark neutral macOS-like palette, not navy or saturated blue.** V5 reference tokens: page `#1b1b1b`; single centered graphite surface `#303030`; recessed input/command surface `#222222`/`#262626`; separators `#464646`; primary text `#f0f0f0`; secondary `#b2b2b2`; muted `#858585`. Prefer existing global theme tokens where equivalent; this list is a **visual target**, not an invitation to fork a second theme.
- **One continuous content panel**, rounded corners, generous but compact spacing, horizontal **pill** navigation, light selected pill, no additional sidebar. Sections separated by thin lines, compact rows and simple labels. No large colored metric cards, gradients, multicolored badges, pictographic feature grids, fake switches or excessive boxes. Include accessible keyboard focus and responsive/wrapping tab layout.
- Visual priority: typography and alignment > decorative containers. Critical warnings visible as quiet high-contrast text and explicit labels; do not use color alone.
- **Exactly five first-level Project_0 tabs, English UI text:** `Overview`, `Liquidator`, `Capture`, `Analytics`, `Wallets`.
- **`System Health` is a clickable top status** with a compact expandable diagnostics sheet; **not** a sixth top-level tab and not a giant boot-check list on Overview.
- `Analytics` subnavigation: `Report`, `Backtest`, `Optimize` (profile comparison within Optimize). `Wallets` subnavigation: `Balances`, `Send`, `Auto-Sweep`. Backend/CLI retain precise Rust/source names, independent of UI copy.
- The shared DarkBot navigation (`Overview / BSC / Project 0 / Solana`) may wrap the Project_0 five-tab surface. Do not add a competing second top-level app/navigation system.
- Fixed monetary scenarios are **not** UI options: capital and profit thresholds are free positive decimal inputs with **backend-owned** validation, currency, precision, exposure/asset and cap rules. Existing `CAPITAL_USD` constants are legacy analysis defaults, not a UX constraint.

### Content contract per tab (strict disclosure)

| Tab | Above the fold / primary action | Expand / advanced only |
|---|---|---|
| **Overview** | Current **RPC latency p95**, **decision latency p95**, **stream freshness**, **Realized net PnL USD**, **Total capital USD**, current Liquidator/Capture state, concise System Health link | p50/p99, 429, gaps, queues, detailed dependency checks, capital allocation breakdown |
| **Liquidator** | Explicit execution gate / start or stop after separate LIVE authorization, available working capital, pending TX, realized PnL, submission latency, active profile | Fee, Jito, route, funding, wallet-holds, risk/policy/drain/signer proofs. Current LIVE-disabled state visibly locked |
| **Capture** | Free positive duration with unit s/m/h/d or `Until stopped`; optional presets; source logs/transactions; `market`/combined market+shadow/shadow; Check & Start, Stop | Chain verification `off/index/strict/crosscheck` (postcapture, qualified), six independently switchable **report-only** instruction markers, four classes, provider/capacity details |
| **Analytics** | Select immutable run/segment, Coverage, Report, Backtest, Optimize and profile compare, known gross/shadow/realized values separated | Funding/capital/fees/timing sweep, confidence intervals/holdout, row drilldown with Chain- and Journal-proof |
| **Wallets** | **Total capital**, **Available capital**, **Reserved capital**, **Realized net PnL**, **Withdrawable profit**, **Transferred to Treasury** in same snapshot with token-level balances | Solana account/Mint/ATA proof, deposit/withdrawal history, send preview, whitelist/policy, Auto-Sweep limits and finality receipts |

No fake numerical data in operational UI: missing results rendered as `—` + `Not connected` / `Not measured` / `UNKNOWN` / `NOT QUALIFIED`; never a fabricated `0.00`.

## 2. Financial domain definitions – one authority, two presentations

**Critical:** `Overview` and `Wallets` must display **the same source-bound fields** (same read timestamp, provider Slot/commitment, wallet universe, valuation convention, FX quote age, proof status, rounding). Display reuse is permitted; duplicate React financial calculations are forbidden.

**Named proposed DTO:** `PortfolioFinancialSnapshot` (SOLL; not in present Rust code).

| Proposed field / UI title | Exact meaning / semantics | Current source and readiness |
|---|---|---|
| `portfolio_total_capital_usd_micro` / **Total capital · USD** | **Project_0 managed portfolio net equity** = provably owned and nonduplicated wallet SOL/SPL/active on-protocol receivable and seized inventory valued as-of snapshot **minus** outstanding liabilities. Exclude external personal Phantom/MetaMask treasury destinations unless expressly included as separately managed accounts. Include marked-to-market holdings only as valuation, never as realized PnL. | **SOLL** portfolio aggregation; `one_shot::Current.balances`, `receipts::FinalizedObservation` and Chain/Bank/Oracle state are partial building blocks, not an implemented consolidated total |
| `portfolio_available_capital_usd_micro` / **Available capital · USD** | Spendable/free capital for **new liquidation opportunities** after fee/rent/funding holds, active orders, liabilities, policy reserves and safety buffers. Token-specific fundability preserved; a USD number alone cannot guarantee route liquidity. | **SOLL** owner aggregation + reserve ledger; `one_shot.rs::Current` and `liquidation/funding.rs` partially inform it |
| `portfolio_reserved_capital_usd_micro` / **Reserved capital · USD** | Value of committed funds across transactions, existing in-flight liquidation and treasury sweep intents and fee/rent holds; each hold has an owner/intent and no double count. | **SOLL**, FE-04 one-shot `WalletLock`/holds exist partially; P8-RES/P8-INV gates still matter |
| `realized_net_pnl_usd_micro` / **Realized net PnL · USD** | **Our own cumulative finalized, fully reconciled closed-trade net PnL**, explicitly windowed/all-time, incorporating actual trading revenues, realized disposal/exits, tx fees, tips, rent effects, settlement losses, as documented by exact ledger deltas and priced at attributable event-time USD valuation. Exclude unrealized mark-to-market changes, hypothetical/Shadow/Gross edge, deposits, external transfers, treasury sweeps. | **SOLL** fully reconciled FE-07 financial ledger. `operations/reporting/market_report.py::realized_pnl_usd` currently concerns report/market winner evidence, **not** automatically our owned-wallet accumulated realized net PnL. `receipts.rs` explicitly states it contains no price or realized-PnL claims |
| `withdrawable_profit_usd_micro` / **Withdrawable profit · USD** | Finalized *unswept* realized profit available for transfer, **limited by exact per-asset spendable balance and trading reserves**. Display USD estimate only with qualified price/FX quote. | **SOLL** `TreasuryAvailability` from FEUI-10B; not the same as Realized net PnL or onchain balance |
| `transferred_to_treasury_usd_micro` / **Transferred to Treasury · USD** | Finalized historical sweep flows converted using proven transfer-time rates; **capital relocation, not a negative realized trading PnL**. | **SOLL** sweep intent and receipt ledger FEUI-10B/C |
| `wallet_asset_balances` / **Asset balances** | For each wallet + chain + mint/Token program: verified exact atomic balance, token decimals, priced value and slot; liabilities/positions separately reconciled; never double count owned balance and protocol claim. | **PARTIAL IST** wallet public identity, finalized wallet deltas, per-account balance views; SOLL consolidated holdings and quote service |
| `valuation_status`, `pnl_status`, `as_of_slot`, `as_of_unix_ns`, `price_sources` / **Data quality** | Source provenance, last complete successful reconciliation and price/FX freshness; `KNOWN/STALE/UNKNOWN/NOT_PROVEN` are separate. Unsafe price, missing liability or incomplete wallet universe invalidates **Total capital** estimate (may show separately known partials). | **SOLL** versioned snapshot+quality fields |

**Important accounting invariants:**

1. `Total capital` is **not** `available capital`; `available + reserved` may **not** equal total capital because held collateral, liabilities, mark-to-market, unrealized positions and non-fundable tokens exist.
2. Portfolio equity is a **point-in-time valuation**; realized net PnL is a **flow over a defined window**. They cannot be substituted or added. A USD conversion requires explicit source, quote time, stale threshold, token decimals and lossless money representation.
3. Wallet top-ups or withdrawals, self-transfers, ATA creations, borrowing/repayment principal and treasury moves are **not strategy income/expense** except for their **actual fees**; sweep USD value can affect total *managed* capital, but not realized trading PnL.
4. `realized_net_pnl_usd_micro` must not be taken from a competitor's `winner_net_edge_usd`, a market-event field, a shadow edge estimate or a positive simulated opportunity. All own-trade closed-exit, fee, balance and inventory paths must reconcile to finalized chain receipts; else `UNKNOWN`/partial with explicit scope.
5. Multiple controlled wallets / token accounts / protocol margin accounts are grouped by **distinct ownership**, not blindly summed (no double-count of ATAs, internal transfers, protocol positions vs underlying wallet collateral). Liability/exposure valuations subtracted exactly once.
6. USD-micro/i128/u128 and lamports represented losslessly at the Rust and Browser boundary; no JS float as authoritative accumulator. UI can render fiat with precision policy but never silently transform `UNKNOWN` into zero.
7. All-time PnL needs a trustworthy **inception baseline**, ledger coverage and a defined `as_of`; otherwise `NO_LIFETIME_BASELINE` instead of pretending to know lifetime performance.
8. Each portfolio snapshot must be immutable/id-bound and **reused** across Overview, Liquidator and Wallets to prevent contradictory values during provider/reconcile lag.

### Global vs per-wallet display

- **Overview:** compact `Realized net PnL` and `Total capital` are first-class, above the fold. RPC p95, Decision p95 and Stream freshness stay visible; if width is tight use a compact secondary financial line, **not** new large colored tiles.
- **Liquidator:** available **trading** capital and realized net PnL (same snapshot), plus actual submission outcome/fee/race. This is not a generic exchange "Trading" tab.
- **Wallets / Balances:** one summary for **Total capital / Available / Reserved / Realized net PnL / Withdrawable / Transferred to Treasury**, followed by per-wallet and per-asset rows with token totals, USD values, pending/held amounts and as-of/provenance. No fictional balances when current wallet is unconnected.
- **Wallets / Send & Auto-Sweep:** use **withdrawable profit per asset** rather than portfolio total or cumulative PnL as the send amount. Manual/automated transfers require separate FEUI-10 authorization; none are enabled by the frontend plan.
- Cross-project BSC portfolios **not** included in Project_0 Total capital. If a future DarkBot global summary is needed, implement a separately provenance-qualified cross-chain portfolio aggregator.

## 3. Latency and concurrency: precise metric inventory

### User-visible latency

| Display | Required source measurement | Current evidence / next step |
|---|---|---|
| **RPC latency · p95 ms** | Per real RPC operation monotonic request→response duration, operation/provider class, sample count + window | **SOLL** typed backend histogram/quantile; existing RPC throttle/timeout config is not a measured RTT |
| **Decision latency · p95 ms** | `t_rx` → validated decision/economics-ready, same monotonic clock, typed stage and clear inclusion/exclusion of QUEUED/UNKNOWN | **PARTIAL IST** Ingress `t_rx_ns`/Shadow `timing`; need runtime rolling quantile emitter |
| **Stream freshness · ms** | Now on valid monotonic clock minus last *valid* progressing application/slot frame; staleness/gaps/clock validity | **PARTIAL IST** `MarketStats.max_stream_silence_ms`, `last_observed_slot`, ingress `t_rx_ns`; fresh/current ms derived backend |
| **Submission latency · p95 ms** | Tx build/sign/transport ACK and accepted/landed/finalized stages **separately**; do not call HTTP ack landing | **SOLL** qualified source-specific metric, only after value-bearing execution authorization |
| **Writer queue / lag** | `StorageStatus.queue_bytes,lag_frames,write_bytes_per_s,ingress_bytes_per_s,disk_exhaustion_s` | **IST** bounded writer telemetry; only expose valid current samples |
| **WSS health / 429 / Gaps** | `MarketStats.transport_http_429,gap_count,transport_reconnect_attempts,last_observed_slot` | **IST** actual counters; interpretation requires coverage/validity status |

**Histogram contract:** every p50/p95/p99 includes unit, sample count, observation window, data source, freshness, and unavailable/insufficient-sample handling. Different clocks, unverifiable latency, missing `T_rx` and simulated timestamps are not valid production timing samples.

### Concurrent Liquidator + Capture + Shadow + Offline Analytics

**TARGET, not current LIVE capability.** A single canonical chain ingestion/state owner fans out to isolated consumers (execution priority > bounded capture/journal > Shadow > offline job), each with explicit CPU, RAM, RPC/WSS, disk IO budgets/backpressure; no duplicated inconsistent State Owner and no signer in the Read-only analyzer.

- Capture/Shadow must be able to record **unfiltered** raw data during later authorized liquidation execution, without degrading its measured decision/submission latency beyond qualified thresholds.
- Analyst can run historical backtest/optimizer from **sealed segment roots** while later segments are still recording, with resource isolation and stop/pause/cancel priority; no unbounded concurrent jobs.
- Queue saturation/429/disk-pressure remains visible and fails appropriately; "parallel" alone does not certify complete evidence or money safety.
- Distinct runtime mode flag for `market`, `econ4h`, `shadow` **already exists**; a single combined financially authorized operation remains **SOLL**, subject to FE-06 safety approvals. `P0_MODE=live` currently refused by runtime.

## 4. Cross-code owner map (IST vs SOLL)

| UI requirement | Exact current paths / symbols | Implement in |
|---|---|---|
| V5 Project0 page | `frontend/src/features/project0/Project0Page.tsx` placeholder, `frontend/src/app/router.tsx`, `shared/layout/Navigation.tsx` | FEUI-03: five-tab graphite layout with no duplicate global nav |
| Realized net PnL | `project_0/native/crates/runtime/src/execution/one_shot/receipts.rs::FinalizedObservation`; `project_0/operations/reporting/market_report.py::realized_pnl_usd` is **not** cumulative owned-wallet PnL | FEUI-10A/FE-07 finalized finance engine, no synthetic balances |
| Capital/reservations | `project_0/native/crates/runtime/src/execution/one_shot.rs::Current.balances`, `WalletLock`; `project_0/native/crates/liquidation/src/funding.rs::capital_required` | FEUI-10A and P8-RES/P8-INV qualified portfolio snapshot |
| Hypothetical capital | `project_0/native/crates/runtime/src/analytics/econ.rs::CAPITAL_USD`, `P0_ECON_CAPITAL_USD`; canonical `analytics/econ_canonical.rs` currently also uses fixed `CAPITAL_USD` in a separate report branch | FEUI-07; backend refactor to make sweeps truly free-valued; no UI fixed buckets |
| Latency/coverage | `solana/src/ingress/mod.rs::t_rx_ns`, `runtime/src/market/core.rs::MarketStats`, `runtime/src/analytics/econ.rs::Distribution`, `runtime/src/storage/writer.rs::StorageStatus` | FEUI-03/06 native rolling latency telemetry and UI |
| Capture/verify | `runtime/src/orchestration/startup.rs` modes `market,econ4h,shadow`; `operations/cli/measurement_contract.py::VerificationConfig.mode` | FEUI-01/02/04, no assumption of LIVE authorization |
| Wallet views/sweeps | `project_0/config/wallet.public.env`, `runtime/src/execution/one_shot/receipts.rs`, `frontend/docs/WALLETS_TREASURY.md` | FEUI-10A/B/C with separate per-asset authorized sender |

**No hardcoded wallet balances, wallet identities, financial defaults, FX quotes, full-portfolio capital, duration, profit thresholds or optimizer "sweet spots" in the UI.** The names in this document are proposed DTO fields to be implemented in Rust and versioned; they are not proof that the current bot publishes these fields.

## 5. UI-V5 acceptance gates

- **V5-G01 Visual:** full five-tab graphite/pill/single-surface layout; no V4 navy cards or sidebar; keyboard/touch mobile width tested; English labels.
- **V5-G02 Contract:** every UI value maps to verified Rust-source field/typed owner or is explicitly `SOLL`; provider/latency/financial unknowns render `—` and a reason, not a fabricated zero.
- **V5-G03 Overview/Wallet parity:** same `snapshot_id` → identical `realized_net_pnl_usd_micro`, `portfolio_total_capital_usd_micro`, status, USD units, date/slot and code-verified rounding across both tabs.
- **V5-G04 Finance:** ownership/liabilities, token decimals and as-of price proofs; self-transfers/sweeps do not change realized PnL; deposits not classified as profit; competitor winners and shadow do not fund Wallet payouts; finality and fee proof required.
- **V5-G05 Missing scope:** any unresolved wallet, oracle, price, liability, unclosed inventory, unknown receipts or missing inception baseline gives `UNKNOWN/PARTIAL`; no unsafe total capital / withdrawable "0" representation.
- **V5-G06 Timing:** RPC/Decision/Stream metrics include p95 source/count/timewindow and null reason; p50/p99/details only on Health; no guessed send latency before LIVE.
- **V5-G07 Parallelism:** concurrent execution + capture + shadow + replay cannot bypass FE-06 authority; bounded queues and prioritized resources; no capture overflow affecting execution safety and no false full-coverage claims.
- **V5-G08 Capital controls:** arbitrary positive capital/min-net values validated by backend precision/ranges; UI may present presets but never restrict to fixed hardcoded `CAPITAL_USD` buckets.
- **V5-G09 Money controls:** `Wallets → Send / Auto-Sweep` uses `withdrawable` by exact asset and explicit separately scoped approval; portfolio Total capital and lifetime PnL are never spend permission.
- **V5-G10 Regression:** storybook/static visual regression against accepted graphite V5, plus TypeScript/React tests, Rust typed financial snapshot tests, offline paired UI/CLI replay; no paid data/LIVE tests in CI.

## 6. Build-order impact

- **Before C0-E1 4h:** no requirement to implement full wallet accounting / V5 UI. Still close FEUI-01/02 and existing SK/JFI/Provider/6-marker/Capacity gates.
- **First UI read-only milestone:** V5 shell with Health + latency availability + status + existing capture metadata and Wallet placeholders / real proved balances. Render statuses honestly.
- **Second UI data milestone:** binding `PortfolioFinancialSnapshot` and FE-07 ledger/accounting qualified; Overview and Wallets display the same **Realized net PnL USD** and **Total capital USD** values.
- **After explicit LIVE/treasury approval:** enable the separate liquidation control and payout features under existing qualified runtime; never enable by just attaching a frontend.

_This document is deliberately precise about what does not yet exist. Implementation must not promote proposed names or example colors into current operational capability._
