# Project 0 Control V0 — frozen Graphite V7 fleet design

**Status:** V0 design acceptance / React feature shell. **Captured:** 2026-10-17. **Do not redesign Wallets or introduce new tiles, blue/navy styling, sidebar or secondary app.** [Project_0 plan](../../project_0/docs/PHASENPLAN.md) · [Frontend phases](FRONTEND_PHASENPLAN.md) · [source map](PROJECT_0_CODE_MAP.md) · [V0 gap contract](V0_FLEET_IMPLEMENTATION_PLAN.md).

## V0 layout and immutability
The approved Graphite V7 preview (the conversation's `project0_graphite_v7_fleet.html`) is the visual authority. V0 is now a native React feature under `frontend/src/features/project0/`, following the same graphite theme. The reference has five pill tabs: **Overview / Liquidator / Capture / Analytics / Wallets**. Main page: single central 800px graphite surface `#303030` on dark backdrop `#1b1b1b`; `#464646` separators, selected dark pill, native macOS typographic sizing, compact lines. CSS scoped to `.p0-graphite`; do not alter global/BSC themes.

**Immutable Wallets UX:** exactly **Balances / Send / Auto-Sweep** with the same straightforward line rows. Balances has **Total capital, Realized net PnL, On-chain balance, Available capital, Reserved capital, Withdrawable profit, Transferred to Treasury** in that order; `Overview` repeats Realized net PnL and Total Capital with the same future `PortfolioFinancialSnapshot.snapshotId`. Display `— USD` when no finality/valuation/ledger evidence, not fake `0 USD`. `Send` and `Auto-Sweep` remain non-executing until independently authorized. Wallets is not to be reworked when updating Liquidator.

**Liquidator Fleet:** list on left, editor on right; Add / Duplicate / Remove, name, origin, Classic and Receivership (draft-capable only; other classes not promised executable), **free numeric** capital, min net, fee, max in-flight, status LOCKED. Many drafts can coexist; no fake active bots, no value-bearing submission. Each later LIVE instance needs one exact typed strategy identity, profile hash, budget and common arbitration. Capture remains a separate concurrently runnable observation lane.

**Capture:** free duration / Until stopped, logs|transactions, market|econ4h|shadow, Chain verification `off|index|strict|crosscheck`, all six report-only markers. Raw capture unfiltered. `Analytics` is Report/Backtest/Optimize with a button to create an **unqualified draft**, not an approved strategy. UI operations must never confuse editing a draft with running a bot.

## V0 source, verification and boundaries

React component entry: [Project0Page](../src/features/project0/Project0Page.tsx) / graphit [CSS](../src/features/project0/styles/graphite.css) / independent panel components [pages](../src/features/project0/pages/) / [model contracts](../src/features/project0/model/contracts.ts). Frontend does **not** start, submit, verify, persist to Rust or obtain actual balances in V0. Existing REST `apiGet<T>` and `RealtimeClient` remain shared integration primitives but are **not connected to a Project_0 Control backend**.

The V7 single-file HTML in the conversation is a **reference/prototype only**; its inline imperative DOM event handlers and short-lived local bot array are intentionally **not imported into the React product**. Its structure, layout and wallet hierarchy are reproduced via React state, scoped CSS and independently testable pages. Pin source hash/visual evidence in the acceptance review; don't rewrite existing `frontend/src/features/bsc/` or `solana/`.

**V0 code delivery:** React shell, isolated presentation types and in-memory drafts are implemented; source-to-API binding, backend durability, true refresh, native Rust scheduler and React E2E visual comparison remain OPEN until tested. Do not call V0 production-ready.
