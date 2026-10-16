# C0 — Market and account census (2026-10-16)

Every number below is **historical** (program 0.1.11, October 2026 runs) and is not a statement about today's Mainnet.
A current census needs a Mainnet read-only Shadow start, which needs separate permission.

## C0-01 — Account census (historical PASS)

Full-program `getProgramAccounts`, slot 452723812 (2026-10-02, ledger "C0-01 Mainnet census"):

| Type | Accounts |
|---|---|
| MarginfiAccount | 165,597 |
| LiquidationRecord | 6,330 |
| Bank | 437 |
| MarginfiGroup | 162 |
| FeeState / StakedSettings | 1 / 1 |
| Unknown discriminator | 214 = 204 `BankMetadata`, 1 `Order`, 9 unidentified (`184662bf3a907b9e`, not in the liquidation core) |

Target group `4qp6Fx6t…` (slot 452727389): 165,123 accounts; 82,680 with liabilities, 82,443 lending-only, 4,662 empty;
on-chain indexer flags: 61 `was_liquidatable`, 60 `was_underwater`, 5,114 ever liquidated, 9,954 active in 30 days.

## C0-02 — Health classification

**Implemented (2026-10-16):** `darkbot_project0::census` classifies every liability account of the configured group with
the pinned protocol Health into the PHASENPLAN classes and sums equity assets/liabilities per class:

| Class | Rule |
|---|---|
| healthy | Health OK, not liquidatable |
| premium_driven | healthy, but projected borrow premium erodes maintenance health (0 under 0.1.11: no borrow premium) |
| race_now | liquidatable and net-positive now at the untagged premium (≤ max(FeeState fee, 5 %)) |
| long_tail | liquidatable, not net-positive now, net-positive at `t_profitable` if tagged now (0.1.12 ramp; buckets ≤ 1 h / 24 h / 7 d) |
| uneconomic | liquidatable, not net-positive at any premium up to the asset-surplus cap |
| bad_debt | unweighted assets < liabilities (bankruptcy path) |
| unsupported | Health fails closed, by reason: `UnknownLayout` (incl. 0.1.12-resized banks), `Unsupported` oracle/setup, `Stale`, `MissingAccount`, `Crate` |

It runs once at the first Shadow READY when `P0_C0_CENSUS=1`; `P0_C0_REALIZABLE_BPS`, `P0_C0_FIXED_COST_USD_MICRO` and
`P0_C0_MIN_PROFIT_USD_MICRO` are mandatory (no default cost model) and are logged with the result as one `c0_census:`
line. Tests: `census::tests` 2/2 PASS.

**Current run: NOT_RUN** (Mainnet read-only permission).

Historical indicators (4 h run `c0e1_4h_20261008T222326Z`, slot 454677228–454730767): reverse index 82,782 accounts over
204 group banks; `unsupported_bank_oracle_layouts=0`; integration accounts (R110/R111): Kamino 194–195, JupLend 43–44,
Drift 0.

## C0-03 — Race map (historical, incomplete)

Same 4 h run, 24,569 Shadow opportunities, all with unresolved economics; known executable net edge 0:

| Rejection | Opportunities |
|---|---|
| MissingAta | 10,655 |
| Routing | 8,911 |
| Bankruptcy (bad debt) | 4,612 |
| Unknown | 269 |
| Health | 91 |
| Sizing | 29 |
| StateRace | 2 |

The market side observed 0 liquidations in 1,682 program transactions (81 truncated logs) → `MARKET_DATA_INCOMPLETE`,
no winner, latency or competitor evidence. Distance to threshold, oracle/bank exposure, TX complexity and v0/v1 class per
account are not measured as a census.

## C0-04 — Long-tail / `t_profitable` (model only)

`economics::{premium_bps, time_for_premium, t_profitable}` implement the 0.1.12 premium ramp; the census applies them per
account (`long_tail` class and buckets). Tagging exists only from 0.1.12 (P1B), so long-tail is potential, not executable,
on the 0.1.11 program. No current account data has been evaluated.

## Gate C0: NOT_PROVEN

No reliable data shows an economically relevant opportunity class: the only economic run has known executable edge 0
(FE-00 `NO_ECONOMIC_GO`), the race side has no observed liquidations, long-tail and premium-driven need 0.1.12. Unblock:
one Mainnet read-only Shadow start with `P0_C0_CENSUS=1` and a measured cost model (fee evidence R124/FIX129), then decide
GO per class from the `c0_census:` line.
