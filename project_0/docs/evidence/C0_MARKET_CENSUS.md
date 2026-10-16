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

**Implemented (2026-10-16, revised by C0-FIX-01..05):** `darkbot_project0::census` classifies every liability account of
the configured group with the pinned protocol Health. Race/long-tail/premium classes are **model classes**
(`classes=MODEL_NOT_EXECUTABLE`): a stated potential model, never executable net edge — that stays with the existing
CandidateInput/`economics::evaluate` pipeline the sweep already feeds.

| Class | Rule |
|---|---|
| healthy | Health OK, not liquidatable; bucketed by distance to the liquidation threshold (maintenance headroom ≤ 1 / 5 / 10 / > 10 % of maintenance liabilities) |
| model_premium_driven | healthy, projected borrow premium erodes maintenance health (none under 0.1.11: premium decay is `Ok(0)` there) |
| model_race_now | liquidatable and model-net-positive now at the untagged premium (≤ max(FeeState fee, 5 %), capped by asset surplus) |
| model_long_tail | liquidatable, not now, model-net-positive at `t_profitable` if tagged now (0.1.12 ramp; buckets ≤ 1 h / 24 h / 7 d) |
| model_uneconomic | liquidatable, not model-net-positive at any premium up to the cap |
| bad_debt | unweighted assets < liabilities (bankruptcy path) |
| unsupported | fail-closed with reason: `UnknownLayout` (incl. 0.1.12-resized banks), `Unsupported`, `Stale`, `MissingAccount`, `Crate`, `Premium` (decay not determinable — never 0), `Facts` (banks/oracles/integrations/observation not readable) |

Per account (facts from canonical state, no RPC): active and liability banks, oracle setups, Kamino/Drift/JupLend,
e-mode banks, observation accounts (TX load; accounts above the `TxLimits` locked-account limit are counted), equity
assets/liabilities. Costly positions and `PREMIUM_ACTIVE` do not exist in 0.1.11 and are counted as not applicable.

How it runs (C0-FIX-01/03): no second scan. The R127 startup sweep, which already evaluates every debtor once in
parallel, builds one row per account for the armed batch against one state view and the **Clock sysvar** of the
canonical state (now a required dependency; every owner-side Health/threshold/projection/detect evaluation uses it,
never local or zero time). The owner moves the rows into a single-slot bounded queue; a worker classifies and
aggregates and returns one `c0_census: COMPLETE` line with slot, record slot, clock source, model, `owner_batch_ms` and
`worker_ms`. Coverage gap, full queue, missing clock or FeeState error → `c0_census: INCOMPLETE reason=…`, retried at the
next READY; only a complete census retires the request. Model inputs `P0_C0_REALIZABLE_BPS` (1..=10 000),
`P0_C0_FIXED_COST_USD_MICRO` (> 0), `P0_C0_MIN_PROFIT_USD_MICRO` (≥ 0) are mandatory and validated at startup; values
taken from historical fee evidence (R124/FIX129) must say so where the run is recorded.

Tests: `census::tests` 3/3, runner `census::tests` 2/2 (worker coverage gap, chain clock decoding), runner lib 270/270.

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
no winner, latency or competitor evidence. Distance to threshold, oracle setups, integrations, e-mode and TX load
(observation accounts vs. locked-account limit) per account are now census fields (C0-02); exact v0/v1 fit needs the
built transaction and stays with the pipeline. Current values NOT_RUN.

## C0-04 — Long-tail / `t_profitable` (model only)

`economics::{premium_bps, time_for_premium, t_profitable}` implement the 0.1.12 premium ramp; the census applies them per
account (`long_tail` class and buckets). Tagging exists only from 0.1.12 (P1B), so long-tail is potential, not executable,
on the 0.1.11 program. No current account data has been evaluated.

## Gate C0: NOT_PROVEN

No reliable data shows an economically relevant opportunity class: the only economic run has known executable edge 0
(FE-00 `NO_ECONOMIC_GO`), the race side has no observed liquidations, long-tail and premium-driven need 0.1.12. Unblock:
one Mainnet read-only Shadow start with `P0_C0_CENSUS=1` and a measured cost model (fee evidence R124/FIX129), then decide
GO per class from the `c0_census:` line.
