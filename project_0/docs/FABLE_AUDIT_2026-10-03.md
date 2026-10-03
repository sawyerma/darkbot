# Project_0 — Fable Audit Verification

Date: 2026-10-03  
Scope: Fable review supplied after the first real Mainnet Shadow bootstrap probe.  
Rule: **No audit statement becomes an implementation task until independently verified.**

Status vocabulary:

- **VERIFIED** — current DarkBot code and pinned `marginfi-v2@mrgn-0.1.11-rc1` support the finding.
- **PARTIAL** — a real issue exists, but the claim/remedy is too broad or not yet fully proven.
- **FALSE** — contradicted by current code or pinned protocol source; must not be implemented.
- **UNPROVEN** — insufficient evidence; measurement/source verification required.

## 1. Executive state

Real Mainnet evidence before this audit:

- 716/716 WSS subscription ACKs → StreamReady.
- Two filtered snapshots completed with zero missing required dependencies.
- Reconcile → `Ready`.
- Real provider interruption → reconnect → new reconcile → `Ready`.
- 61 bootstrap probes queued from `was_liquidatable`.
- 61/61 passed Exact Health as currently liquidatable.
- downstream: 61/61 Skip; 54 `no feasible execution candidates`; 7 Kamino `ReserveStale`; 0 Build / 0 Sign / 0 Simulation.

Therefore the current blocker is **Execution planning before Build**, not bootstrap/recovery.

## 2. Verified audit register

| ID | Verdict | Severity | Finding | Evidence / correction | Required action |
|---|---|---:|---|---|---|
| A1 | **VERIFIED** | Critical | Funding classes are incorrectly coupled to an ExactIn market route. | In `builder.rs::plans`, `let Some(mark) = market.clone() else { continue };` executes before the FundingClass match. Therefore NoSwapInventory / PreheldInventory are also discarded when route=None. | Decouple execution feasibility from exit-market valuation. ZERO_PREFUND requires an executable route; NoSwap/Preheld must not be mechanically discarded. Unknown realizable value must remain UNKNOWN, never 0. |
| A2 | **VERIFIED** | Critical coverage | Integration banks are excluded from ProtocolSizer. | `sizing.rs::supported_generic_bank` accepts only `ASSET_TAG_DEFAULT | ASSET_TAG_SOL`; integration positions are skipped intentionally until integration builders exist. | Keep generic path fail-closed, but implement explicit integration sizing/withdraw support rather than silently treating coverage as complete. |
| A3 | **VERIFIED** | Critical | Required integration refresh planner is missing. | Builder currently passes `pre_refreshes: vec![]`. Pinned `liquidate_start.rs::validate_instructions` explicitly permits Kamino RefreshReserve/RefreshReservesBatch/RefreshObligation, Drift UpdateSpotMarketCumulativeInterest and JupLend UpdateRate before StartLiquidation. Pinned receivership tests use Kamino refreshes before Start. | Build a deterministic pre-start refresh planner from active balances; no separate architecture. |
| A4 | **PARTIAL** | Critical integration | Freshness semantics block stale integration state, but “off-chain health can never evaluate integration banks” is too absolute. | Kamino freshness is slot-based; Drift/JupLend timestamp-based. Both Health(Client Clock) and Sizing use protocol oracle adapters. An integration account can be evaluated if its integration state is fresh. Faking the Clock backwards would bypass protocol freshness and is rejected as a production fix. | For integration support, model/project the **actual refresh result** or otherwise derive same-tx post-refresh values. For the first generic E2E, exclude integration-bank accounts. |
| A5 | **VERIFIED / FIX-88 IMPLEMENTED** | Critical correctness | Receivership eligibility lacked the protocol circuit-breaker gate. | Pinned `start_liquidation` calls both `any_balance_bank_is_cb_halted` and `run_cb_price_gate`; current off-chain candidate eligibility does not. | Add exact fail-closed CB eligibility before plan admission, using canonical RAM state and protocol semantics. |
| A6 | **VERIFIED** | Critical for simulation | Empty Shadow wallet cannot prove full Build/Simulation. | Current Shadow wallet has 0/328 optional ATAs in snapshot. Builder requires asset and liability ATAs. RPC simulation uses `sig_verify=true`; EndLiquidation can transfer a real flat SOL fee. | Define deterministic Shadow wallet prerequisites: required ATAs and measured minimum SOL for fees/rent only. Do **not** assume “0.2 SOL”; calculate the requirement. |
| A7 | **VERIFIED** | Critical runtime | Winner matching / reconciliation perform blocking RPC in the detection loop. | `main.rs` invokes winner search/reconcile in the same loop that consumes ingress. `find_winner` can paginate signature RPCs and fetch transactions. | Move evidence/reconciliation RPC work to bounded worker channel(s). No synchronous remote RPC in the decision/ingress loop. |
| B1 | **VERIFIED** | Critical | Refresh planner missing. | Same evidence as A3. | Implement after generic E2E correctness gates or as required by integration support. |
| B2 | **PARTIAL** | Critical integration | A projected integration view is likely required, but the exact design is not proven by Fable. | Same-tx refresh changes the values accepted by protocol freshness checks. A fake older Clock is not protocol-equivalent. | Design a `ProjectedView` only if it reproduces the actual refresh transition; otherwise fail closed. |
| B3 | **VERIFIED** | Critical coverage | Integration withdraw path missing. | Sizer excludes integration tags and generic builder always uses standard `LendingAccountWithdraw`. | Implement Kamino/Drift/JupLend-specific withdraw builders only from pinned source semantics. |
| B4 | **VERIFIED** | Critical for simulation | ATA / fee-payer strategy missing for current Shadow wallet. | Same evidence as A6. | Decide pre-created ATA vs idempotent ATA inside allowed transaction section; account rent/fees explicitly. |
| B5 | **VERIFIED CURRENTLY** | Medium | Jupiter `GET /swap/v2/build` endpoint needed verification. | Official Jupiter Swap API V2 documentation currently defines Router `GET /swap/v2/build` for raw instructions and ALTs with `inputMint/outputMint/amount/taker`. Our endpoint shape is valid as of 2026-10-03. Runtime auth/rate-limit behavior still requires evidence. | Keep endpoint; add runtime capability evidence and fail-closed auth/error classification. |
| B6 | **PARTIAL** | High | `maxAccounts=64` can consume too much transaction space. | Current Jupiter docs expose 1–64 and specifically recommend lowering maxAccounts to free space for custom instructions / 1232-byte limit. Fable's literal `64 - locked_accounts` formula is not sufficient. | Budget by actual compiled account set + serialized size; requote with lower maxAccounts when necessary. |
| B7 | **VERIFIED OPEN** | Medium | P3-02 threshold index is not implemented. | `health.rs::thresholds` explicitly returns Unsupported. | Remains open; not required for first account-update Long-Tail E2E. |
| C1 | **FALSE** | Dangerous false fix | “Remove health_cap; protocol has no health<=0 upper bound.” | Pinned `check_post_liquidation_condition_and_get_account_health` explicitly requires `account_health <= 0` and health improvement. Marginfi comments say post-maintenance health must remain <=0. | **Do not remove health_cap.** Preserve conservative headroom; add parity tests around the zero-health boundary. |
| C2 | **PARTIAL** | Medium | Ceil rounding can discard a near-full-liability candidate. | Current sizer drops when rounded `repay_amount > liab.amount`. Pinned protocol also has `repay_all`; simple clamping is not automatically equivalent. | Add exact edge-case tests. If fixing, rederive seize/premium/health from the final repay amount or use proven repay_all semantics. |
| C3 | **VERIFIED** | Correctness | Bankruptcy/no-collateral separation is valid for sampled records. | Equity requirement uses weight ONE for assets/liabilities. Sampled cases had `equity_assets_bits=0`, `bad_debt=true`. Production-group bankruptcy is not permissionless per pinned source. | Keep explicit Bankruptcy classification. Do not generalize sampled evidence to all 54 until counted. |
| C4 | **FALSE** | Methodology false alarm | Technical probes are counted as economic shadow wins. | Current ledger has separate `shadow_technical_ready`. Main watch/race path excludes `ShadowTechnicalReady`; `shadow_wins` only comes from the economic watch/race path. | No code fix required. Clarify documentation wording so the distinction cannot regress. |
| C5 | **VERIFIED** | Correct | Liability filter and indexer offset are valid for the pinned layout. | `MarginfiAccount` fields place `indexer_flags` at payload offset 2248; raw Anchor offset is 2256 = 8+2248. `is_lending_only` is the first IndexerFlags byte. Balance-mutating instructions sync indexer flags. | Keep filter; add compile-time/layout regression test instead of magic-number trust. |

## 3. Additional verified findings

### D1 — The 54 “no feasible” causes are not fully classified

Only sampled records were shown with `bad_debt=true` and `equity_assets_bits=0`. It is **UNPROVEN** that all 54 are bankruptcy/no-collateral.

Action: instrument planning rejection by stage:
`no_assets | no_liabilities | unsupported_bank | stale_integration | no_route | funding | missing_ata | cb_gate | sizing_rounding | other`.

### D2 — Possible WSOL valuation-bank selection ambiguity — UNPROVEN

`find_bank_by_mint` picks the first group Bank matching the native SOL mint. If multiple WSOL-mint banks exist and an integration bank is selected, execution-cost valuation can inherit integration freshness errors.

Action: census all same-mint WSOL banks and qualify the SOL valuation bank deterministically before changing code.

### D3 — Jupiter route endpoint is current; transaction-space budgeting is still incomplete

Current official docs confirm `/swap/v2/build` returns raw instructions and lookup tables for custom transactions. Current docs also recommend reducing `maxAccounts` when custom instructions need space.

Action: runtime test and dynamic transaction-size budget; do not replace the endpoint based on the Fable review.

## 4. Fix order

The order is correctness-first and minimizes simultaneous changes.

### Gate R0 — Audit / compiler baseline
1. Freeze this audit register.
2. Build/test current FIX-85/FIX-86 tree.
3. No further code change until compiler baseline is green.

### Gate R1 — Generic-path correctness
1. A5 circuit-breaker eligibility.
2. D1 structured rejection diagnostics.
3. Restrict the **first E2E qualification cohort** to fully supported generic P0 banks; integration accounts remain visible but explicitly unsupported/fail-closed.
4. C5 layout regression test.

Goal: know exactly why every candidate is rejected.

### Gate R2 — Shadow transaction prerequisites
1. A6 ATA/fee-payer strategy.
2. Measure exact SOL/rent requirement; no arbitrary funding amount.
3. Verify that the same transaction can Build → Sign → Simulate without trading capital.

### Gate R3 — Funding / market decoupling
1. A1 remove the accidental route dependency for NoSwap/Preheld execution feasibility.
2. Preserve UNKNOWN economics where exit value is unavailable.
3. Do not turn UNKNOWN into zero or an economic win.
4. Re-test ZERO_PREFUND separately with executable route.

### Gate R4 — Hotpath isolation
1. A7 move winner matching to bounded evidence worker.
2. Move live reconciliation to bounded reconciliation worker.
3. Detection loop must perform no synchronous remote RPC.

This is mandatory before long-duration Shadow/production qualification.

### Gate R5 — First generic Mainnet Shadow E2E
Target evidence:
`liquidatable → exact sizing → funding → build → sign → simulate → ShadowTechnicalReady|ShadowTechnicalSimFailed`.

No Integration-bank support is required for this gate.

### Gate R6 — Integration completeness
1. A3/B1 refresh planner.
2. A4/B2 exact projected post-refresh semantics.
3. A2/B3 Kamino/Drift/JupLend sizing + specific withdraw builders.
4. Integration-specific simulation matrix.

### Gate R7 — Edge/economics hardening
1. C2 rounding/repay_all edge.
2. B6 dynamic Jupiter account/size budgeting.
3. FailedCostMatrix measurement.
4. v1 final wire/submission qualification.
5. P3-02 thresholds.

## 5. Non-negotiable guardrails

- C1 is rejected: do not remove the zero-health boundary.
- C4 is rejected: do not merge technical and economic Shadow wins.
- Never fake freshness by simply moving Clock backwards.
- Never map unknown route/value/cost to zero.
- Do not enable LIVE during this audit/fix sequence.
- Every fix gets: pinned-source reference → unit/integration test → local build/test → Mainnet Shadow evidence → ledger update.


## 6. Remediation status

- **R0 PASS** — compiler/workspace baseline green after FIX-87.
- **R1-A5 FIX-88 IMPLEMENTED; build/test reported green** — circuit-breaker eligibility is now in the shared pipeline before planning.
- **R1 diagnostics FIX-89 IMPLEMENTED / VERIFY** — typed sizing/routing/ATA/integration rejection evidence added.
- A1 remains deliberately open until R3; diagnostics must expose it rather than silently changing economics/execution semantics.
