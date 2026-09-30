# DarkBot

DarkBot contains three separate trading/automation projects with a shared control plane and selected shared infrastructure.

The repository is deliberately organized by project. **Protocol, strategy, evidence, gates, configuration, tests and outputs must stay inside the owning project area.**

## Projects

| Project | Purpose | Primary navigation |
|---|---|---|
| **BSC / Birth Entry** | Earliest technically possible direct on-chain BUY after supported BSC launch | [BSC docs](docs/bsc/README.md) |
| **Solana / Pump.fun** | Existing Pump.fun capture/replay/PAPER/strategy system and reference implementation | [Pump.fun docs](docs/solana_pump/README.md) |
| **Solana / Project 0** | Native liquidation searcher: exact state/health → economics → receivership → inclusion | [Project 0 docs](docs/project_0/README.md) |

Cross-project overview: [docs/MULTICHAIN_ROADMAP.md](docs/MULTICHAIN_ROADMAP.md)

---

# Repository structure

~~~text
darkbot/
├── __main__.py             shared package entry point
├── app/                    shared control-plane / app registry
├── common/                 only genuinely project-independent code
├── runtime/                shared profile/runtime assembly
├── chains/                 shared chain contracts/registry
├── venues/                 shared venue contracts/registry
├── solana/                 shared native Solana primitives
├── bsc/                    BSC / Birth Entry application
├── solana_pump/            Solana / Pump.fun application
└── project_0/              Solana / Project 0 application

docs/
├── README.md
├── MULTICHAIN_ROADMAP.md
├── bsc/
├── solana_pump/
└── project_0/

gates/
├── README.md
├── bsc/
│   └── gate0/
├── solana_pump/
└── project_0/

config/
├── bsc/
├── solana_pump/
└── project_0/

data/
├── bsc/
├── solana_pump/
└── project_0/

exports/
├── bsc/
├── solana_pump/
└── project_0/

migrations/
├── bsc/
├── solana_pump/
└── project_0/

tests/
├── common/                 cross-project registry/runtime seam tests
├── bsc/
├── solana_pump/            includes Pump.fun helpers + fixtures
└── project_0/

tools/
├── bsc/
├── solana_pump/
└── project_0/

~~~

Runtime ownership rules: [darkbot/README.md](darkbot/README.md)

Documentation root: [docs/README.md](docs/README.md)  
Gates root: [gates/README.md](gates/README.md)  
Tests root: [tests/README.md](tests/README.md)  
Tools root: [tools/README.md](tools/README.md)

---

# 1. BSC / Birth Entry

## Goal

~~~text
earliest usable pending/chain evidence
→ derive exact launch/BUY inputs locally
→ build + sign direct contract transaction/bundle
→ qualified inclusion path
→ successful on-chain BUY
→ prove token acquisition and ordering
~~~

No website/browser/frontend BUY dependency.

## Phase plan / architecture

- **Active BSC phase roadmap:** [docs/bsc/BSC_DUAL_VENUE_ROADMAP.md](docs/bsc/BSC_DUAL_VENUE_ROADMAP.md)
- **Gate-0 test plan:** [docs/bsc/GATE0_TESTPLAN_SHORT.md](docs/bsc/GATE0_TESTPLAN_SHORT.md)
- **Birth-entry architecture:** [docs/bsc/BSC_BIRTH_ENTRY_ARCHITECTURE.md](docs/bsc/BSC_BIRTH_ENTRY_ARCHITECTURE.md)
- **Final native runtime contract after Gate-0 PASS:** [docs/bsc/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md](docs/bsc/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md)
- **Economics gate:** [docs/bsc/BSC_ECONOMICS_GATE_CONTRACT.md](docs/bsc/BSC_ECONOMICS_GATE_CONTRACT.md)

## Measurements / evidence

- **Master measurement ledger:** [docs/bsc/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md](docs/bsc/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md)
- **Gate-0 evidence:** [gates/bsc/gate0/evidence/](gates/bsc/gate0/evidence/)
- **Additional BSC docs/evidence:** [docs/bsc/](docs/bsc/)

## Gate code

- **Gate-0 Rust workspace:** [gates/bsc/gate0/](gates/bsc/gate0/)
- **Gate-0 Rust source:** [gates/bsc/gate0/src/](gates/bsc/gate0/src/)
- **Gate-0 tools:** [gates/bsc/gate0/tools/](gates/bsc/gate0/tools/)

## Application code

- **BSC application boundary:** [darkbot/bsc/](darkbot/bsc/)
- **Shared application registry:** [darkbot/app/](darkbot/app/)

## Project assets

- [config/bsc/](config/bsc/)
- [data/bsc/](data/bsc/)
- [exports/bsc/](exports/bsc/)
- [migrations/bsc/](migrations/bsc/)
- [tests/bsc/](tests/bsc/)
- [tools/bsc/](tools/bsc/)

---

# 2. Solana / Pump.fun

## Goal

Existing Pump.fun reference system for:

- real market capture;
- deterministic replay;
- PAPER execution modeling;
- exact Pump.fun market/quote semantics;
- Strategy A/B/C;
- fee/rent/economic evidence;
- research and reporting.

The current Python implementation remains the reference system while native shared Solana infrastructure is developed.

## Current phase / status

- **Research + latency status:** [docs/solana_pump/RESEARCH_STATUS_2026-09-23.md](docs/solana_pump/RESEARCH_STATUS_2026-09-23.md)
- **Current master TODO / phase work:** [docs/solana_pump/MASTER_TODO_2026-09-20.md](docs/solana_pump/MASTER_TODO_2026-09-20.md)
- **R9 / Token-2022 implementation status:** [docs/solana_pump/R9_IMPLEMENTATION_STATUS.md](docs/solana_pump/R9_IMPLEMENTATION_STATUS.md)
- **Strategy C source of truth:** [docs/solana_pump/STRATEGY_C_SOURCE_OF_TRUTH.md](docs/solana_pump/STRATEGY_C_SOURCE_OF_TRUTH.md)
- **Pump.fun documentation index:** [docs/solana_pump/README.md](docs/solana_pump/README.md)

## Measurements / evidence

- **Current research/latency evidence summary:** [docs/solana_pump/RESEARCH_STATUS_2026-09-23.md](docs/solana_pump/RESEARCH_STATUS_2026-09-23.md)
- **Stored evidence:** [data/solana_pump/](data/solana_pump/)
- **Generated reports/exports:** [exports/solana_pump/](exports/solana_pump/)

## Gates

- **Pump.fun gates area:** [gates/solana_pump/](gates/solana_pump/)

No separate new production gate implementation is assumed merely because this folder exists; current authoritative status remains in the documents above.

## Application code

- **Pump.fun implementation:** [darkbot/solana_pump/](darkbot/solana_pump/)
- **Strategies:** [darkbot/solana_pump/strategies/](darkbot/solana_pump/strategies/)
- **Current Pump.fun chain adapter:** [darkbot/solana_pump/chain/](darkbot/solana_pump/chain/)
- **Pump.fun venue adapter:** [darkbot/solana_pump/venue/](darkbot/solana_pump/venue/)

Historical top-level modules such as `darkbot/market.py`, `darkbot/execution.py`, `darkbot/paper.py` etc. are compatibility facades. New Pump.fun implementation work belongs under `darkbot/solana_pump/`.

## Project assets

- [config/solana_pump/](config/solana_pump/)
- [data/solana_pump/](data/solana_pump/)
- [exports/solana_pump/](exports/solana_pump/)
- [migrations/solana_pump/](migrations/solana_pump/)
- [tests/solana_pump/](tests/solana_pump/)
- [tools/solana_pump/](tools/solana_pump/)

Important naming rule: files named `P0_*` under Pump.fun documentation/tests refer to historical **Pump.fun Phase-0**, not the Project 0 liquidation protocol.

---

# 3. Solana / Project 0

## Goal

~~~text
native Solana ingress/state
→ exact Project 0 protocol state
→ exact health/liquidation eligibility
→ deterministic economics
→ valid receivership execution
→ qualified Jito/RPC inclusion
→ landing/winner reconciliation
→ realized net profit
~~~

Primary objective: **maximize sustainable realized net profit**.

## Authoritative phase plan

- **Frozen production phase plan:** [docs/project_0/PHASENPLAN.md](docs/project_0/PHASENPLAN.md)
- **Project 0 documentation index:** [docs/project_0/README.md](docs/project_0/README.md)

Core phase sequence:

~~~text
S0 → S1 → S2 → S3 → S4
                 │
P1A + P1B + H0 ──┘
                 ↓
P2 → P3 → P4A → P5 → P6 → P7 → P4B → P8 → P9 → K
~~~

## Measurements / evidence

- **Measurement ledger:** [docs/project_0/MEASUREMENT_LEDGER.md](docs/project_0/MEASUREMENT_LEDGER.md)

Unmeasured values remain N/A; no invented Project-0 latency, p(win), capture-rate or PnL values.

## Gates

- **Project 0 gates area:** [gates/project_0/](gates/project_0/)

Actual phase gates are defined in the authoritative [PHASENPLAN](docs/project_0/PHASENPLAN.md).

## Application code

- **Project 0 application boundary:** [darkbot/project_0/](darkbot/project_0/)
- **Shared native Solana boundary:** [darkbot/solana/](darkbot/solana/)
- **Shared application/control plane:** [darkbot/app/](darkbot/app/)

Project 0 production code is added directly into the final project/shared-Solana architecture; no temporary Python LIVE hotpath and no duplicate Solana stack.

## Project assets

- [config/project_0/](config/project_0/)
- [data/project_0/](data/project_0/)
- [exports/project_0/](exports/project_0/)
- [migrations/project_0/](migrations/project_0/)
- [tests/project_0/](tests/project_0/)
- [tools/project_0/](tools/project_0/)

---

# 4. Shared control plane and code ownership

The target application model is:

~~~text
                    shared frontend / control plane
                               │
                         darkbot.app
                               │
                    shared main / CLI facade
                               │
          ┌────────────────────┼────────────────────┐
          │                    │                    │
         BSC             Solana / Pump.fun      Project 0
          │                    │                    │
     darkbot/bsc/       darkbot/solana_pump/   darkbot/project_0/
                                                    │
                                          darkbot/solana/
                                     shared native Solana pieces
~~~

Rules:

- frontend/control-plane code does not contain protocol trading logic;
- BSC logic stays in `darkbot/bsc/`;
- Pump.fun logic stays in `darkbot/solana_pump/`;
- Project 0 logic stays in `darkbot/project_0/`;
- only Solana infrastructure genuinely shared by Pump.fun and Project 0 belongs in `darkbot/solana/`;
- only truly project-independent infrastructure belongs in `darkbot/common/`;
- one production business-logic implementation per project.

Detailed ownership contract: [darkbot/README.md](darkbot/README.md)

---

# 5. Old system / legacy material

Historical backend/source and exports are isolated under:

- [old_system/](old_system/)
- [old_system/backend/](old_system/backend/)
- [old_system/exports/](old_system/exports/)
- [old_system/tests/](old_system/tests/)

Older audit files, snapshots and historical evidence are intentionally retained for now.

They may be deleted later **only after checking that they are not referenced by current tests, evidence chains, source manifests or authoritative documentation**. Repository cleanup is a separate task from the structural split.
