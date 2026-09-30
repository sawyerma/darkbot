# DarkBot

DarkBot contains three separate trading/automation projects with shared control-plane infrastructure and selected shared Solana primitives.

**Ownership rule:** protocol logic, project-specific configuration, evidence, gates, tests, tools and outputs stay inside the owning project area. Shared code is limited to genuinely reusable infrastructure.

## Projects

| Project | Purpose | Documentation | Code |
|---|---|---|---|
| **BSC / Birth Entry** | Earliest technically possible direct on-chain BUY after supported BSC launch | [docs/bsc/README.md](docs/bsc/README.md) | [darkbot/bsc/](darkbot/bsc/) |
| **Solana / Pump.fun** | Existing Pump.fun capture/replay/PAPER/strategy system and reference implementation | [docs/solana_pump/README.md](docs/solana_pump/README.md) | [darkbot/solana_pump/](darkbot/solana_pump/) |
| **Solana / Project 0** | Native liquidation searcher: exact state/health → economics → receivership → inclusion → realized PnL | [docs/project_0/README.md](docs/project_0/README.md) | [darkbot/project_0/](darkbot/project_0/) |

Cross-project overview: [docs/MULTICHAIN_ROADMAP.md](docs/MULTICHAIN_ROADMAP.md)

## Repository structure

~~~text
darkbot/
├── app/                    shared control-plane/application registry
├── common/                 genuinely project-independent infrastructure
├── runtime/                shared runtime/profile assembly
├── chains/                 shared chain contracts/registry
├── venues/                 shared venue contracts/registry
├── solana/                 shared native Solana primitives
├── bsc/                    BSC / Birth Entry
├── solana_pump/            Solana / Pump.fun
└── project_0/              Solana / Project 0

config/
data/
docs/
exports/
gates/
migrations/
tests/
tools/
└── each split into bsc / solana_pump / project_0 where project-owned
~~~

Runtime ownership contract: [darkbot/README.md](darkbot/README.md)  
Documentation index: [docs/README.md](docs/README.md)  
Tests index: [tests/README.md](tests/README.md)  
Tools index: [tools/README.md](tools/README.md)

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

No frontend/browser BUY dependency.

## Authoritative documents

- [BSC project index](docs/bsc/README.md)
- [Active dual-venue roadmap](docs/bsc/BSC_DUAL_VENUE_ROADMAP.md)
- [Gate-0 test plan](docs/bsc/GATE0_TESTPLAN_SHORT.md)
- [Birth-entry architecture](docs/bsc/BSC_BIRTH_ENTRY_ARCHITECTURE.md)
- [Native single-runtime contract](docs/bsc/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md)
- [Economics gate contract](docs/bsc/BSC_ECONOMICS_GATE_CONTRACT.md)
- [Measurement/evidence ledger](docs/bsc/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md)

## Code / evidence

- [Application boundary](darkbot/bsc/)
- [Gate-0 Rust workspace](gates/bsc/gate0/)
- [Gate-0 evidence](gates/bsc/gate0/evidence/)
- [Tests](tests/bsc/)
- [Tools](tools/bsc/)

---

# 2. Solana / Pump.fun

## Goal

Existing reference system for real market capture, deterministic replay, PAPER execution modeling, exact Pump.fun semantics, Strategy A/B/C, fee/rent evidence and research/reporting.

## Authoritative navigation

- [Pump.fun project index](docs/solana_pump/README.md)
- [Research + latency status](docs/solana_pump/RESEARCH_STATUS_2026-09-23.md)
- [Master TODO](docs/solana_pump/MASTER_TODO_2026-09-20.md)
- [R9 / Token-2022 implementation status](docs/solana_pump/R9_IMPLEMENTATION_STATUS.md)
- [Strategy C source of truth](docs/solana_pump/STRATEGY_C_SOURCE_OF_TRUTH.md)
- [SPEC.json](docs/solana_pump/SPEC.json)
- [Test handbook](docs/solana_pump/TEST_HANDBUCH.md)

## Code / evidence

- [Pump.fun implementation](darkbot/solana_pump/)
- [Strategies](darkbot/solana_pump/strategies/)
- [Pump.fun venue adapter](darkbot/solana_pump/venue/)
- [Shared Solana adapter](darkbot/solana/)
- [Data](data/solana_pump/)
- [Exports](exports/solana_pump/)
- [Tests + fixtures](tests/solana_pump/)
- [Tools](tools/solana_pump/)

Historical backend/reference material is retained under [docs/solana_pump/legacy_backend/](docs/solana_pump/legacy_backend/) and historical output archives under [exports/solana_pump/legacy/](exports/solana_pump/legacy/).

Files named `P0_*` inside Pump.fun history refer to historical **Pump.fun Phase-0**, not Project 0.

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

## Authoritative documents

- [Project 0 project index](docs/project_0/README.md)
- [PHASENPLAN v4 — authoritative](docs/project_0/PHASENPLAN.md)
- [Measurement ledger](docs/project_0/MEASUREMENT_LEDGER.md)

## Phase sequence

~~~text
S0 → S1 → S2 → S3 → S4
│
├─ parallel: P0 + P1A + P1B + C0A + H0
│
└→ Gate A
   → P2 → P3 → P4A → P5 → T → P6
   → 0.1.12 deployment: HALT + requalification + C0B
   → P7 → P4B → P8 → P9 → K
~~~

Strategic default: **Long-Tail first, Race later**, until real competition evidence supports escalation.

## Code / evidence

- [Project 0 application boundary](darkbot/project_0/)
- [Shared native Solana boundary](darkbot/solana/)
- [Project 0 gates](gates/project_0/)
- [Tests](tests/project_0/)
- [Tools](tools/project_0/)
- [Config](config/project_0/)
- [Data](data/project_0/)
- [Exports](exports/project_0/)

No temporary Python LIVE hotpath and no duplicate Solana stack.

---

# 4. Shared ownership rules

- BSC business/protocol logic → `darkbot/bsc/`
- Pump.fun business/protocol logic → `darkbot/solana_pump/`
- Project 0 business/protocol logic → `darkbot/project_0/`
- Solana primitives genuinely shared by Pump.fun and Project 0 → `darkbot/solana/`
- truly project-independent infrastructure → `darkbot/common/`
- shared registries/orchestration may select components but must not contain project business logic
- one production implementation per concern; no shadow duplicate cores

See [darkbot/README.md](darkbot/README.md) for the runtime ownership contract.
