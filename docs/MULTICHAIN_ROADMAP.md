# DarkBot — Current Project Map

Status: **CURRENT HIGH-LEVEL OVERVIEW**  
Updated: 2026-09-30

DarkBot currently contains three distinct engineering lines. They share selected principles and later selected Rust infrastructure, but they are **not the same strategy or protocol**.

~~~text
DarkBot
├── BSC / Birth Entry
│   └── earliest technically possible on-chain BUY after launch
├── Solana / Pump.fun
│   └── existing launch-sniper / strategy / capture / replay reference system
└── Solana / Project 0 Liquidator
    └── liquidation detection, exact health, receivership execution and competition
~~~

The root [README](../README.md) is the navigation entry point.

## 1. BSC / Birth Entry

Goal:

~~~text
earliest actionable pending/chain signal
→ local decode/quote/build/sign
→ qualified inclusion path
→ successful on-chain BUY
→ acquisition/ordering evidence
~~~

Current work is Gate 0. The decisive product result is the actual BUY/acquisition position, not a local benchmark alone.

Authoritative documents:

- [BSC_DUAL_VENUE_ROADMAP.md](BSC_DUAL_VENUE_ROADMAP.md)
- [GATE0_TESTPLAN_SHORT.md](GATE0_TESTPLAN_SHORT.md)
- [BSC_BIRTH_ENTRY_ARCHITECTURE.md](BSC_BIRTH_ENTRY_ARCHITECTURE.md)
- [BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md](BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md)
- [GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md](GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md)

Code/evidence:

- [../gate0/](../gate0/)
- [../gate0/evidence/](../gate0/evidence/)

## 2. Solana / Pump.fun

Goal:

- preserve and validate the existing Pump.fun semantics;
- capture/replay real market data;
- evaluate strategies and economics without pretending PAPER fills are LIVE fills;
- retain the current Python implementation as the reference system while native Solana work is introduced.

Current authoritative status documents:

- [RESEARCH_STATUS_2026-09-23.md](RESEARCH_STATUS_2026-09-23.md)
- [MASTER_TODO_2026-09-20.md](MASTER_TODO_2026-09-20.md)
- [R9_IMPLEMENTATION_STATUS.md](R9_IMPLEMENTATION_STATUS.md)
- [phase_a/PHASE_A_REPORT.md](phase_a/PHASE_A_REPORT.md)

Current code:

- [../darkbot/chains/solana/](../darkbot/chains/solana/)
- [../darkbot/venues/pumpfun/](../darkbot/venues/pumpfun/)
- [../darkbot/runtime/](../darkbot/runtime/)
- [../darkbot/strategies/](../darkbot/strategies/)

Important: legacy files named P0_* refer to the old **Pump.fun Phase-0/Doctor work**, not to the Project 0 protocol.

## 3. Solana / Project 0 Liquidator

Goal:

~~~text
native Solana ingress/state
→ exact Project 0 protocol/risk state
→ exact liquidation eligibility
→ economics
→ receivership execution planning
→ Jito/RPC submission
→ landing/winner/PnL reconciliation
~~~

Current phase plan:

- [../docs_project_0/PHASENPLAN.md](../docs_project_0/PHASENPLAN.md)

Measurement ledger:

- [../docs_project_0/MEASUREMENT_LEDGER.md](../docs_project_0/MEASUREMENT_LEDGER.md)

Status:

- architecture/phase plan frozen in dedicated Project 0 docs folder;
- no Project 0 production implementation in the repository yet;
- no Project 0 measurement may be claimed before evidence is written to the ledger.

## 4. Shared architecture rule

Do not force BSC and Solana into one fake chain abstraction.

Share only components that are genuinely generic and proven useful, for example:

- timing/telemetry;
- lifecycle/shutdown;
- bounded channels;
- journal/replay concepts;
- measurement discipline;
- fail-closed handling.

Chain and protocol semantics remain separate.

## 5. Source-of-truth rule

For each project use its own phase plan and evidence ledger. Historical documents remain useful evidence, but they do not override a newer project-specific source of truth.
