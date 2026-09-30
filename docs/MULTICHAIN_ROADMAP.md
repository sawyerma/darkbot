# DarkBot — Current Project Map

Status: **CURRENT HIGH-LEVEL OVERVIEW**  
Updated: 2026-09-30

DarkBot contains three separate engineering lines. They share selected infrastructure, but not protocol semantics or strategy logic.

~~~text
DarkBot
├── BSC / Birth Entry
│   └── earliest technically possible on-chain BUY after launch
├── Solana / Pump.fun
│   └── capture / replay / PAPER / strategy reference system
└── Solana / Project 0
    └── exact liquidation state / health / economics / receivership / competition
~~~

Root navigation: [../README.md](../README.md)

## 1. BSC / Birth Entry

Goal:

~~~text
pending/chain evidence
→ local decode/build/sign
→ qualified submission
→ successful BUY
→ acquisition/ordering evidence
~~~

Key links:

- [BSC project index](bsc/README.md)
- [Active roadmap](bsc/BSC_DUAL_VENUE_ROADMAP.md)
- [Gate-0 plan](bsc/GATE0_TESTPLAN_SHORT.md)
- [Measurement ledger](bsc/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md)
- [Gate-0 Rust workspace](../gates/bsc/gate0/)
- [Gate-0 evidence](../gates/bsc/gate0/evidence/)

## 2. Solana / Pump.fun

Purpose:

- preserve exact Pump.fun semantics;
- capture/replay real market data;
- evaluate strategies and economics;
- keep the current implementation as the semantic reference while shared Solana primitives are extracted.

Key links:

- [Pump.fun project index](solana_pump/README.md)
- [Research status](solana_pump/RESEARCH_STATUS_2026-09-23.md)
- [Master TODO](solana_pump/MASTER_TODO_2026-09-20.md)
- [Strategy C source of truth](solana_pump/STRATEGY_C_SOURCE_OF_TRUTH.md)
- [Pump.fun code](../darkbot/solana_pump/)
- [Shared Solana code](../darkbot/solana/)
- [Tests + fixtures](../tests/solana_pump/)

## 3. Solana / Project 0

Goal:

~~~text
native Solana state
→ exact Project-0 state + health
→ deterministic economics
→ receivership planning
→ submission
→ landing/winner/PnL reconciliation
~~~

Key links:

- [Project 0 project index](project_0/README.md)
- [PHASENPLAN v4](project_0/PHASENPLAN.md)
- [Measurement ledger](project_0/MEASUREMENT_LEDGER.md)
- [Project 0 code](../darkbot/project_0/)
- [Shared Solana code](../darkbot/solana/)

Authoritative high-level order:

~~~text
S0 → S1 → S2 → S3 → S4
parallel P0 + P1A + P1B + C0A + H0
→ Gate A
→ P2 → P3 → P4A → P5 → T → P6
→ 0.1.12 HALT/requalification + C0B
→ P7 → P4B → P8 → P9 → K
~~~

## 4. Shared architecture rule

Share only what is truly generic:

- timing/telemetry;
- lifecycle/shutdown;
- bounded channels;
- chain/venue contracts;
- shared native Solana primitives;
- measurement discipline;
- fail-closed infrastructure.

Protocol semantics remain project-owned.

## 5. Source-of-truth rule

Each project owns its own authoritative roadmap/spec/evidence. Historical documents are evidence only and do not override newer project-specific source-of-truth documents.
