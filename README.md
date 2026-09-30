# DarkBot

**Current project map — updated 2026-09-30**

DarkBot is no longer just the old Phase-1 PAPER backend. The repository now contains **three separate project lines**:

| Project | Purpose | Current state |
|---|---|---|
| **BSC / Birth Entry** | Buy new BSC assets directly on-chain as early as technically possible after launch | Active Gate-0 engineering; Rust latency/acquisition path under qualification |
| **Solana / Pump.fun** | Existing Pump.fun launch-sniper, capture/replay, strategies and economic research | Existing Python reference system; PAPER/capture/replay operational; LIVE broadcast still not the reference baseline |
| **Solana / Project 0 Liquidator** | Native Solana liquidation searcher for Project 0: state → exact health → economics → receivership → inclusion | Architecture/phase plan under review; production implementation not started yet |

High-level project map: [docs/MULTICHAIN_ROADMAP.md](docs/MULTICHAIN_ROADMAP.md)

---

# 1. BSC / Birth Entry

## What this project does

The BSC line is a **direct on-chain birth-entry system**.

The objective is not to wait for a website listing or frontend button. The intended path is:

~~~text
earliest usable pending/chain signal
→ decode launch
→ derive required BUY inputs locally
→ quote/build
→ sign
→ hand to the selected inclusion path
→ successful on-chain BUY
→ prove token acquisition and ordering versus competitors
~~~

Primary product evidence:

~~~text
T0 → successful BUY
+ intended token actually received
+ ordering versus competing BUYs
~~~

Local decode/build/sign/send latency is supporting engineering evidence, not proof of acquisition by itself.

## Current BSC status

According to the current measurement ledger:

- G0.2: PASS for the frozen historical Flap scope;
- G0.3: PASS for the frozen direct-BUY offline scope;
- G0.4: PARTIAL, real coverage qualification still open;
- G0.5: local Rust prequalification passed, final real transport qualification remains open;
- a real BSC **testnet** direct BUY/acquisition was proven;
- controlled Mainnet Gate G0.7 remains blocked until the preceding gates are satisfied.

## BSC phase plans / architecture

- **Active phase roadmap:** [docs/bsc/BSC_DUAL_VENUE_ROADMAP.md](docs/bsc/BSC_DUAL_VENUE_ROADMAP.md)
- **Gate-0 test plan:** [docs/bsc/GATE0_TESTPLAN_SHORT.md](docs/bsc/GATE0_TESTPLAN_SHORT.md)
- **Birth-entry architecture:** [docs/bsc/BSC_BIRTH_ENTRY_ARCHITECTURE.md](docs/bsc/BSC_BIRTH_ENTRY_ARCHITECTURE.md)
- **Production Rust runtime after Gate-0 PASS:** [docs/bsc/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md](docs/bsc/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md)
- **Economics gate contract:** [docs/bsc/BSC_ECONOMICS_GATE_CONTRACT.md](docs/bsc/BSC_ECONOMICS_GATE_CONTRACT.md)

## BSC measurements / evidence

- **Human-readable master measurement ledger:** [docs/bsc/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md](docs/bsc/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md)
- **G0.1 evidence freeze:** [docs/bsc/GATE0_G01_EVIDENCE_FREEZE_2026-09-27.md](docs/bsc/GATE0_G01_EVIDENCE_FREEZE_2026-09-27.md)
- **Raw/current Gate-0 evidence:** [gates/bsc/gate0/evidence/](gates/bsc/gate0/evidence/)
- **Rust Gate-0 harnesses:** [gates/bsc/gate0/src/](gates/bsc/gate0/src/)
- **Gate-0 verification tools:** [gates/bsc/gate0/tools/](gates/bsc/gate0/tools/)

The files under gates/bsc/gate0/ are current Gate-0 engineering/evidence code. They are **not automatically the final BSC production runtime**.

---

# 2. Solana / Pump.fun

## What this project does

This is the existing Solana system and the historical core of DarkBot.

It currently provides:

- Solana feed/chain semantics;
- Pump.fun decode and market-state handling;
- Pump.fun bonding-curve / quote behavior;
- Strategy A/B/C research;
- real market capture;
- deterministic replay;
- PAPER execution modeling;
- fee/rent/economic evidence handling;
- reporting and research workflows;
- the reference outputs later used for Rust parity.

The existing Python system remains the **reference oracle** during the native Solana migration. It should not be deleted merely because a Rust runtime is being built.

## Current Solana/Pump.fun state

The current repository still explicitly distinguishes PAPER from LIVE:

- PAPER uses real Mainnet observations but modeled fills;
- real Solana LIVE broadcast/landing is not proven by the old PAPER backend;
- historical positive scenarios were highly latency-sensitive;
- no old modeled result should be presented as proof of a real production edge;
- no blanket POSITIVE_UNDER_MODEL production release is established by the old status files.

## Solana/Pump.fun current code

- **Solana chain adapter:** [darkbot/chains/solana/](darkbot/chains/solana/)
- **Pump.fun venue adapter:** [darkbot/venues/pumpfun/](darkbot/venues/pumpfun/)
- **Runtime assembly:** [darkbot/runtime/](darkbot/runtime/)
- **Strategies:** [darkbot/strategies/](darkbot/strategies/)
- **Main Python package:** [darkbot/](darkbot/)
- **Configs:** [config/solana_pump/](config/solana_pump/)
- **Tests:** [tests/](tests/)

## Solana/Pump.fun phase/status documents

- **Current research + latency status:** [docs/solana_pump/RESEARCH_STATUS_2026-09-23.md](docs/solana_pump/RESEARCH_STATUS_2026-09-23.md)
- **Current historical master TODO:** [docs/solana_pump/MASTER_TODO_2026-09-20.md](docs/solana_pump/MASTER_TODO_2026-09-20.md)
- **Token-2022 / R9 implementation status:** [docs/solana_pump/R9_IMPLEMENTATION_STATUS.md](docs/solana_pump/R9_IMPLEMENTATION_STATUS.md)
- **Phase-A refactor / parity report:** [docs/bsc/phase_a/PHASE_A_REPORT.md](docs/bsc/phase_a/PHASE_A_REPORT.md)
- **Strategy C source of truth:** [docs/solana_pump/STRATEGY_C_SOURCE_OF_TRUTH.md](docs/solana_pump/STRATEGY_C_SOURCE_OF_TRUTH.md)

## Solana/Pump.fun measurements / evidence

- **Research/latency evidence summary:** [docs/solana_pump/RESEARCH_STATUS_2026-09-23.md](docs/solana_pump/RESEARCH_STATUS_2026-09-23.md)
- **Phase-A raw benchmark/parity material:** [docs/bsc/phase_a/](docs/bsc/phase_a/)
- **Current stored evidence:** [data/solana_pump/evidence/](data/solana_pump/evidence/)
- **Generated run reports:** [exports/solana_pump/](exports/solana_pump/)

### Naming warning: P0 is not Project 0

Files such as:

~~~text
docs/solana_pump/P0_IMPLEMENTATION_STATUS.md
docs/solana_pump/P0_DOCTOR_RESULT.json
docs/solana_pump/P0_SANITY_RESULT.json
docs/P0_...
~~~

belong to the older **Pump.fun Phase-0 / Doctor / exit-liveness work**.

They do **not** describe the new Project 0 liquidation protocol project below.

---

# 3. Solana / Project 0 Liquidator

## What this project does

This is the second Solana application and a different product from Pump.fun.

The objective is:

~~~text
Solana state update
→ identify affected Project 0 banks/accounts
→ reproduce exact protocol health
→ detect a real liquidation opportunity
→ prove it is economically worth attempting
→ build required refresh + receivership execution
→ sign
→ submit through Jito/RPC or another measured qualified path
→ reconcile landing
→ compare winner position
→ calculate realized net PnL
~~~

The product goal is **not** an arbitrary “1 ms BUY” target.

The **primary objective is maximum sustainable realized net profit**. The important end results are:

~~~text
REALIZED NET PNL
CAPTURE RATE
WINNER POSITION
CAPITAL EFFICIENCY
PROVIDER / INFRASTRUCTURE COST
~~~

Paid portals/APIs are not assumed production dependencies. A recurring paid provider is retained only if measured incremental profit exceeds subscription, usage and dependency cost.

## Planned native architecture

The target is a shared native Solana production core that can later serve both Pump.fun and Project 0 while keeping the application/protocol logic separate.

Core responsibilities include:

- Yellowstone/gRPC ingress;
- canonical/fork-aware Solana state;
- RAM state and reconciliation;
- blockhash / transaction metadata;
- transaction construction;
- signing;
- submission adapters;
- telemetry/replay/lifecycle.

Project 0 adds:

- protocol/account/bank/oracle decoding;
- integration state;
- reverse indices;
- exact health and liquidation eligibility;
- opportunity/economics;
- refresh planning;
- receivership transaction/bundle planning;
- shadow-race and competition analysis;
- controlled LIVE and production reconciliation.

## Project 0 phase plan

- **Current draft phase plan:** [docs/project_0/PHASENPLAN.md](docs/project_0/PHASENPLAN.md)

Current phase order:

~~~text
S0  native workspace / common extraction
S1  Solana ingress
S2  canonical/fork-aware state
S3  execution core
S4  Pump.fun parity

parallel:
P1A current-mainnet Project 0 freeze
P1B upcoming-release freeze
H0  historical Project 0 evidence

then:
P2 state + reverse indices
P3 exact health + eligibility
P4A deterministic economics/ranking
P5 refresh/routing/receivership planner
P6 full hotpath + shadow race
P7 economics/competition/provider qualification
P4B probabilistic ranking/tip/stopping policy
P8 controlled LIVE
P9 production LIVE
K  measured escalation only
~~~

The authoritative Project 0 architecture and phase plan is frozen in **docs/project_0/PHASENPLAN.md**.

## Project 0 measurements / evidence

- **Project 0 measurement ledger:** [docs/project_0/MEASUREMENT_LEDGER.md](docs/project_0/MEASUREMENT_LEDGER.md)

At the moment this ledger intentionally contains **N/A**, not invented values, because Project 0 production implementation/measurement has not started.

---

# 4. Repository ownership

~~~text
darkbot/
  existing Solana/Pump.fun Python reference runtime

gates/bsc/gate0/
  current BSC Gate-0 Rust harnesses, tools and evidence

docs/
  README.md
  MULTICHAIN_ROADMAP.md
  bsc/
  solana_pump/
  project_0/

config/
  existing Pump.fun/PAPER/research configuration

data/ + exports/
  existing Pump.fun capture/evidence/report outputs

native/
  NOT PRESENT YET
  future production Rust workspace after workspace freeze
~~~

The future native workspace must not create separate incompatible BSC and Solana frameworks. Only truly generic infrastructure should be shared; chain/protocol semantics remain separate.

---

# 5. Engineering rules shared across projects

- direct chain execution; no website/browser dependency in a production trade path;
- fail closed on unknown protocol/layout/state;
- no database in a latency-critical decision path;
- no unmeasured latency claims;
- ACK != landed;
- PAPER/model result != real LIVE execution result;
- liquidatable != profitable;
- one production business-logic implementation per project;
- benchmark/gate/shadow code does not implement a second strategy or second protocol engine;
- infrastructure escalation only after a measured bottleneck;
- every meaningful benchmark/result links to raw evidence and the exact source commit.

---

# 6. Legacy material

The repository intentionally still contains older specs, audits, snapshots, test artifacts and historical reports. They are evidence/history unless a current project document explicitly names them as authoritative.

For current work, start from the three project sections above rather than assuming the old root README, old Phase-1 SPEC or an older audit directory defines the whole repository.
