# Solana / Project 0 Liquidator

Project 0 is the native Solana liquidation-searcher project.

## Goal

~~~text
native Solana ingress/state
→ exact Project 0 protocol state
→ exact health/liquidation eligibility
→ deterministic economics
→ receivership planning
→ Jito/RPC submission
→ landing/winner/PnL reconciliation
~~~

Primary objective: **maximize sustainable realized net profit**.

## Authoritative documents

1. [PHASENPLAN.md](PHASENPLAN.md) — **ARCHITECTURE FREEZE — AUTHORITATIVE — FROZEN v4**. This is the source of truth for architecture, ordering, gates and development rules.
2. [MEASUREMENT_LEDGER.md](MEASUREMENT_LEDGER.md) — measurements, evidence and gate results.

## Frozen architecture principles

- exactly one final production code path;
- no prototype → rewrite → production cycle;
- Race and Long-Tail are both included from the initial design;
- Rust for LIVE correctness/state/race latency/deterministic execution/TX generation;
- Python only for source analysis, census, historical extraction, reports, research and diagnostics;
- reuse before reimplementation;
- fail closed on unknown protocol/layout/oracle/integration/economics/state;
- paid providers only after measured positive incremental net value;
- no own node merely for prestige/latency without an economic case.

## Frozen phase model

~~~text
TRACK A — NATIVE CORE
S0 Native Foundation
→ S1 Solana Ingress
→ S2 Canonical State
→ S3 Execution Core
→ S4 Core Parity + Performance Baseline

TRACK B — PROJECT-0 EVIDENCE / QUALIFICATION
P0  Reference / reuse / licenses
P1A Current Mainnet Freeze
P1B Upcoming 0.1.12 Freeze
C0  Market + Account Census
H0  Historical Liquidations

→ MERGE GATE
→ P2 → P3 → P4A → P5 → T → P6 → P7 → P4B → P8 → P9 → K
~~~

There is no Long-Tail-only gate and no Race-only gate.

## Receivership freeze

For the qualified `mrgn-0.1.12-rc1` source:

~~~text
seized    = pre_asset_value_equity - post_asset_value_equity
repaid    = pre_liability_value_equity - post_liability_value_equity
extracted = max(0, seized - repaid)

seized <= repaid * (1 + allowed_premium)
pre_health - post_health <= extracted

if pre-equity is solvent:
    post-equity must remain solvent
~~~

Maintenance snapshots use **SPOT**; Equity snapshots use **EMA**.

The naive shortcut `liability_value_equity - asset_value_equity` is explicitly not used as the seizure limit.

## Reuse / licensing freeze

- `marginfi-v2`: Apache-2.0; direct crate use is preferred when technically compatible.
- `solana-dex-superagg`: MIT; classified as **REMOTE_AGGREGATOR_CLIENT**, not local routing.
- `eva01`: reference implementation; code copying remains blocked until its own license/freigabe is clear.
- `eva01` exposes `MIN_PROFIT`; current template value is `1` USD and `liquidator.rs` rejects `liquidator_profit <= min_profit`.

## Current immediate work

Start **P0 + C0 immediately**.

The full frozen plan also allows/requests P1A, P1B, H0 and S0 to proceed in parallel where practical.

## Code and project assets

- [Application boundary](../../darkbot/project_0/)
- [Shared native Solana code](../../darkbot/solana/)
- [Config](../../config/project_0/)
- [Data](../../data/project_0/)
- [Exports](../../exports/project_0/)
- [Gates](../../gates/project_0/)
- [Migrations](../../migrations/project_0/)
- [Tests](../../tests/project_0/)
- [Tools](../../tools/project_0/)

Historical Pump.fun files named `P0_*` do **not** belong to Project 0; they remain under [../solana_pump/](../solana_pump/).
