# DarkBot — Solana Native + Project 0 Liquidator Roadmap

Status: **DRAFT / REVIEW PENDING**  
Updated: 2026-09-30

> This file is the current working phase map for the new native Solana core and the Project 0 liquidation searcher. It is **not yet the final implementation freeze**. Astra/Fable review findings must be resolved before tickets are treated as authoritative.

## 0. Objective

Build one native Solana production core in Rust and use it for two Solana applications:

1. existing Pump.fun semantics / later native migration;
2. the new **Project 0 Liquidator**.

Project 0 is a liquidation searcher, not a birth-sniping strategy. Its product objective is:

~~~text
detect a real liquidatable account early
→ reproduce exact protocol health
→ prove the liquidation is economically worth attempting
→ build a valid receivership execution
→ submit through a qualified Solana inclusion path
→ reconcile landing and realized net PnL
~~~

Primary product evidence:

~~~text
REALIZED NET PNL
+ CAPTURE RATE
+ WINNER POSITION
~~~

Local latency is a technical input, not the final product result.

## 1. Hard invariants

- Solana LIVE hot path is Rust.
- Python does not make LIVE trade decisions.
- No synchronous RPC in the decision hot path.
- No database writes in the decision hot path.
- No full account scan per oracle update.
- No simplified Project 0 health formula.
- Official fixed-point/risk semantics are preserved.
- Unknown protocol/account/oracle/integration types fail closed.
- Feed gaps or unreconciled state halt execution.
- liquidatable != profitable.
- submission ACK / bundle ID != landed.
- gate/benchmark code must not become a second trading engine.
- production components are built once and reused.

## 2. Current repository state

As of 2026-09-30:

- the existing Solana/Pump.fun reference runtime is Python under ../darkbot/;
- the current Rust work is the BSC Gate-0 harness under ../gate0/;
- there is **no native/ workspace yet**;
- there is **no Project 0 production code yet**;
- generic Rust pieces must therefore be extracted or built during S0, not assumed to already exist.

The exact Rust workspace layout must remain compatible with the BSC production contract in BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md. Do not create a second competing native workspace.

## 3. Phase map

~~~text
TRACK A — Native Solana Core

S0  Workspace + real common-component extraction
 ↓
S1  Native Solana ingress
 ↓
S2  Canonical/fork-aware Solana state
 ↓
S3  Native Solana execution core + transaction-format qualification
 ↓
S4  Pump.fun parity against the current Python reference

TRACK B — Project 0 evidence/protocol, in parallel where possible

P1A Current-mainnet source freeze
P1B Upcoming-release source freeze
H0  Historical liquidation/composition evidence

Merge after S4 / protocol freeze:

P2  Project 0 RAM state + reverse indices + timed health rechecks
 ↓
P3  Exact health + liquidation eligibility
 ↓
P4  Opportunity + economics
 ↓
P5  Refresh/routing/receivership execution planner
 ↓
P6  Full hot path + shadow race
 ↓
P7  Economic / competition qualification
 ↓
P8  Controlled LIVE
 ↓
P9  Production LIVE
 ↓
K   Measured escalation only
~~~

## 4. S0 — Workspace + Native Foundation

Purpose:

- inspect existing BSC Rust Gate-0 code;
- extract only genuinely chain-generic components;
- create the production Rust workspace without conflicting with the already defined BSC native target;
- freeze timing, telemetry, lifecycle, replay/journal and bounded-channel contracts.

Do **not** describe existing Gate-0 binaries as a finished common runtime.

Required timing vocabulary for Solana:

~~~text
T_rx
T_detect
T_ready
T_sign
T_handoff
T_observed_landed
~~~

Landing evidence also records:

~~~text
signature
landed_slot
tx_index_in_slot where derivable
confirmation status
bundle/submission status
~~~

## 5. S1 — Native Solana Ingress

Baseline provider: QuickNode Yellowstone/gRPC unless measurement later proves a better path is required.

Build:

- Rust Yellowstone client;
- account/slot/transaction subscriptions required by supported applications;
- receive timestamp at ingress;
- reconnect and stream-health handling;
- gap detection;
- traffic/filter metrics.

No arbitrary absolute microsecond pass threshold. Establish a real baseline and protect it against unexplained regressions.

## 6. S2 — Canonical/Fork-Aware State

Maintain warm state in RAM:

- account versions;
- slot/write_version;
- slot parent/status/root provenance;
- fork/reconciliation policy;
- blockhash cache;
- token/mint metadata;
- address-table or transaction-format metadata as needed;
- snapshot bootstrap and reconnect recovery.

Bootstrap must avoid a snapshot/delta race:

~~~text
start stream / buffer updates
→ obtain snapshot with context slot S
→ install snapshot
→ apply buffered canonical updates newer than S
→ enable trading
~~~

If a gap cannot be reconciled from the supported replay window:

~~~text
HALT
→ fresh snapshot
→ reconcile
→ explicit recovery
~~~

## 7. S3 — Native Solana Execution Core

Build and qualify:

- Solana transaction construction;
- signing;
- serialization;
- compute-budget policy;
- Jito adapter;
- RPC adapter;
- landing reconciliation.

Do not freeze transaction format prematurely. Qualify the currently supported Solana transaction formats for Project 0 account-heavy execution and choose by evidence.

## 8. S4 — Pump.fun Parity

Use the existing Solana/Pump.fun system as the reference oracle.

Compare identical captures through:

~~~text
Python reference
vs.
Rust native
~~~

Compare:

- decode;
- market state;
- bonding-curve values;
- quotes;
- decision-relevant values;
- timing stages.

S4 answers: **Have we migrated Solana semantics correctly?**

It is not a Project 0 economics gate.

## 9. P1A / P1B — Project 0 Source Freeze

Maintain two explicit manifests while an announced upgrade is not yet deployed:

### P1A — Current Mainnet

Freeze the actually deployed program and supported state:

- Program ID;
- ProgramData / upgrade identity;
- deployed source/release evidence;
- IDL hash;
- account layouts/sizes;
- bank/oracle/integration variants;
- FeeState;
- liquidation/receivership instructions.

Only P1A may authorize LIVE trading.

### P1B — Upcoming Release

Track the upcoming protocol source independently and prepare parity/tests before deployment.

A program upgrade causes:

~~~text
PROGRAM_UPGRADE
→ execution HALT
→ deployed version identify
→ layout/config/resize checks
→ parity suite
→ promote new manifest only after PASS
~~~

## 10. H0 — Historical Project 0 Evidence

Read-only evidence track. It must not implement a second searcher.

For each liquidation collect where derivable:

- signature / slot / transaction position;
- liquidatee / receiver;
- asset/debt pair;
- seized / repaid;
- fees/tips where provable;
- account and bank composition;
- costly/integration position count;
- oracle setup types;
- external refresh programs/instructions;
- transaction size / compute;
- single transaction vs bundle;
- observed winner concentration and repeat receivers.

This evidence informs P5 transaction feasibility and later P7 competition analysis.

## 11. P2 — State + Reverse Indices

Warm Project 0 state in RAM:

~~~text
Accounts
Banks
Oracles
FeeState
Integration state
Group config
Token metadata
Blockhash / transaction metadata
~~~

At minimum:

~~~text
Oracle → Banks
Bank → Accounts
Account → Banks
Integration → Banks
~~~

No global account scan per price update.

If protocol debt/health can change over elapsed time without a new oracle/account update, schedule deterministic timed rechecks instead of relying only on stream events.

## 12. P3 — Exact Health + Eligibility

Reproduce the pinned official Project 0 risk semantics exactly.

Do not use one oversimplified global price rule. Freeze behavior per calculation purpose, including:

- liquidation eligibility;
- maintenance health;
- initial health;
- equity;
- pre/post liquidation checks;
- premium/debt projection;
- bad-debt determination;
- e-mode/integration handling;
- oracle confidence/freshness.

Correctness precedes optimization.

Any unexplained healthy/liquidatable classification difference is STOP.

## 13. P4 — Opportunity + Economics

Convert liquidatable into worth attempting.

One deterministic Rust source of truth for:

- repay/seize selection;
- liquidation amount;
- protocol fees;
- swap/routing cost;
- slippage;
- priority fee;
- Jito tip;
- token-account/rent effects;
- failed-attempt costs;
- realized net PnL.

## 14. P5 — Refresh / Routing / Receivership Planner

A candidate must become a valid executable plan.

Planner must support the actually required protocol shape, including external reserve/market refreshes where applicable.

Conceptually:

~~~text
RefreshPlan
→ Start liquidation
→ required withdrawals/operations
→ swap / repay
→ End liquidation
~~~

Execution form is selected by evidence:

~~~text
SingleTransaction
or
AtomicBundle
~~~

Measure separately:

- route latency;
- build latency;
- transaction size;
- account count;
- compute units;
- route validity;
- quote quality;
- LUT/transaction-format usage;
- bundle feasibility.

Unsupported active integration paths fail closed.

## 15. P6 — Full Hot Path + Shadow Race

Measure the complete production code path:

~~~text
stream update
→ RAM update
→ reverse index
→ exact health
→ candidate/economics
→ routing
→ receivership plan/build
→ sign
→ submission handoff
→ observed landing
~~~

Report per stage:

~~~text
p50 / p95 / p99 / max / sample count
~~~

Loss reasons are classified, not guessed.

## 16. P7 — Economic / Competition Qualification

Run historical and forward shadow evidence long enough to support a decision.

For every actual liquidation answer:

- did we observe it?
- when did we classify it?
- was it profitable under our exact model?
- when was our execution ready?
- who won?
- where did we lose?

Loss classes:

~~~text
INGRESS
STATE
HEALTH
ROUTING
BUILD
SUBMISSION
TIP
STATE_RACE
ECONOMIC_SKIP
UNKNOWN
~~~

## 17. P8 — Controlled LIVE

Only after correctness, state, receivership, shadow and economics gates pass.

Required safety gates include:

- dedicated hot wallet;
- small capital;
- max opportunity exposure;
- max daily loss;
- max failed attempts;
- stale-state halt;
- feed-gap halt;
- program-upgrade halt;
- unknown-version halt.

Every execution reconciles planned vs submitted vs landed vs actual token deltas vs fees vs realized PnL.

## 18. P9 — Production LIVE

Operate the qualified searcher and monitor:

- feed/state quality;
- latency distributions;
- route quality;
- submission/landing success;
- capture rate;
- failed attempts;
- realized net PnL;
- competition;
- program/config changes.

## 19. K — Measured Escalation Only

Add infrastructure only for a demonstrated bottleneck.

Examples:

~~~text
INGRESS bottleneck
→ dedicated provider / own Geyser / network path investigation

ROUTING bottleneck
→ local/precomputed routing investigation

SUBMISSION bottleneck
→ Jito region/tip/inclusion path investigation

CPU bottleneck
→ profiling / allocation / affinity / sharding
~~~

No own node, Geyser, special network, custom kernel, NIC tuning, massive sharding or multi-region architecture merely because it could be faster.

## 20. Measurement ledger

All Project 0 measurements belong in:

- [SOLANA_PROJECT0_MEASUREMENT_LEDGER.md](SOLANA_PROJECT0_MEASUREMENT_LEDGER.md)

Raw evidence should later live under a dedicated Project 0 evidence directory once implementation begins.

## 21. Naming warning

Existing files named P0_* in this repository belong to the **older Pump.fun Phase-0/Doctor work**. They are not documentation for the new **Project 0 Liquidator**.
