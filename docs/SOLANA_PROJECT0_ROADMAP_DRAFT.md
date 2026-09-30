# DarkBot — Solana Native + Project 0 Liquidator Roadmap

Status: **DRAFT / REVIEW PENDING**  
Updated: 2026-09-30

> This file is the current working phase map for the new native Solana core and the Project 0 liquidation searcher. It is not yet the final implementation freeze. Astra/Fable review findings must be resolved before tickets are treated as authoritative.

# PRIMARY OBJECTIVE

**Maximize sustainable realized net profit.**

Latency, capture rate, routing quality, inclusion performance, infrastructure choice and provider choice are subordinate optimization variables.

The system is not optimized for:
- lowest local latency at any cost;
- maximum liquidation count;
- maximum provider count;
- maximum infrastructure complexity;
- architectural elegance without economic benefit.

The production objective is:

~~~text
maximize:
  realized liquidation revenue
- protocol fees
- swap/DEX fees
- slippage
- priority fees
- Jito tips
- failed-attempt costs
- infrastructure costs
- paid API / SaaS / portal costs
- avoidable operational costs
~~~

## COST-INDEPENDENCE RULE

**No external paid data/API/SaaS/portal service is a permanent production dependency unless measured evidence shows that it increases sustainable net profit by more than its full cost and dependency risk.**

Every paid provider must have:

- explicit purpose;
- monthly and usage cost;
- measured incremental benefit;
- measured impact on capture/landing/profit;
- local/free fallback where technically possible;
- removal criterion;
- no hidden ownership of protocol/business logic.

Preferred production direction:

~~~text
own/local state
→ local Rust health/economics
→ local or free routing path where valid
→ local signing/building
→ direct Jito / RPC submission
~~~

Paid QuickNode/Helius/Jupiter/other services may be used temporarily for bootstrap, comparison or qualification, but must not become permanent merely because they were convenient during development.

A paid service survives only if:

~~~text
incremental sustainable net profit
>
subscription + usage + operational/dependency cost
~~~

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
+ CAPITAL EFFICIENCY
+ PROVIDER / INFRA COST
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
- paid-provider dependence is not accepted without measured economic justification.
- no probability model may use invented p(win) values.

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
S1  Native Solana ingress + provider-independence foundation
 ↓
S2  Canonical/fork-aware Solana state
 ↓
S3  Native Solana execution core + tx-format / submission capability qualification
 ↓
S4  Pump.fun parity against the current Python reference

TRACK B — Project 0 evidence/protocol, in parallel where possible

P1A Current-mainnet source freeze
P1B Upcoming-release source freeze
H0  Historical liquidation/composition/competition evidence

Merge after S4 / protocol freeze:

P2  Project 0 RAM state + reverse indices + timed health rechecks
 ↓
P3  Exact health + liquidation eligibility
 ↓
P4A Deterministic opportunity economics/ranking
 ↓
P5  Refresh/routing/receivership execution planner
 ↓
P6  Full hot path + shadow race
 ↓
P7  Economic / competition qualification + p(win|opportunity,tip)
 ↓
P4B Probabilistic ranking / tip / stopping policy
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
- freeze timing, telemetry, lifecycle, replay/journal and bounded-channel contracts;
- keep protocol/business logic independent from external provider SDKs.

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

## 5. S1 — Native Solana Ingress + Provider Independence

A temporary bootstrap provider is allowed. A permanent paid-provider dependency is not assumed.

Target interfaces must support:

~~~text
IngressSource
├── Yellowstone / Geyser provider
├── own-node Geyser / Yellowstone
└── other source only if measured and qualified
~~~

Build:

- Rust Yellowstone client;
- account/slot/transaction subscriptions required by supported applications;
- receive timestamp at ingress;
- reconnect and stream-health handling;
- replay/from-slot recovery where supported;
- gap detection;
- traffic/filter metrics;
- structured transaction-version detection including v1 Message.config;
- capability tests for the exact provider/plugin/proto path;
- provider cost/coverage/latency measurements.

The target architecture must allow replacing QuickNode/Helius without rewriting state, health, strategy or execution logic.

No arbitrary absolute microsecond pass threshold. Establish a real baseline and protect it against unexplained regressions.

### S1 provider-cost evidence

For every provider path record:

~~~text
monthly_cost
usage_cost
coverage
gaps
reconnect behavior
latency distribution
v1 fidelity
operational burden
incremental opportunity capture
~~~

Paid provider becomes permanent only after later economic evidence shows positive incremental net profit.

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

State is owned locally. Protocol calculations must not depend on synchronous portal lookups.

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

## 7. S3 — Native Solana Execution Core + Capability Qualification

Build and qualify:

- Solana transaction construction;
- signing;
- serialization;
- transaction-version aware resource configuration;
- Jito adapter;
- RPC adapter;
- landing reconciliation.

Do not freeze transaction format prematurely. Qualify current deployed transaction formats by measured execution behavior.

Current production qualification must measure:

~~~text
transaction_version
serialized_bytes
account_count
compute_limit
loaded_account_data_limit
priority_fee_model
LUT usage
build/sign latency
submission path
landed/rejected/expired
failure reason
~~~

Current deployed runtime account-lock limits must be read from the target cluster/runtime capability and must not be treated as a permanent protocol constant.

### JITO / TX-V1 CAPABILITY GATE

Do not infer tx-v1 support from validator version alone.

For the exact production path prove:

1. ordinary RPC submission accepts and lands v1;
2. Jito sendTransaction accepts and lands v1;
3. Jito bundle path accepts v1;
4. >1232-byte v1 survives the full submission/bundle path;
5. landed transaction preserves the intended v1 Message.config semantics;
6. failure/fallback behavior to v0 is deterministic;
7. no hidden truncation or version degradation occurs.

No v1 production default until this gate passes.

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

Maintain two explicit manifests while an announced upgrade is not yet deployed.

### P1A — Current Mainnet

Freeze the actually deployed program and supported state:

- Program ID;
- ProgramData / upgrade identity;
- deployed source/release evidence;
- IDL hash;
- account layouts/sizes;
- bank/oracle/integration variants;
- FeeState;
- liquidation/receivership instructions;
- refresh allowlist;
- swap/integration allowlist;
- flat liquidation fee semantics.

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

All Solana historical readers must support the transaction versions present on the target chain, including v1 where deployed.

For each liquidation collect where derivable:

- signature / slot / transaction position;
- liquidatee / receiver;
- asset/debt pair;
- seized / repaid;
- protocol/flat fees;
- priority fees/tips where provable;
- transaction version;
- serialized size;
- account count;
- LUT usage;
- compute units;
- submission path where derivable;
- account and bank composition;
- costly/integration position count;
- oracle setup types;
- external refresh programs/instructions;
- swap program/route;
- single transaction vs bundle;
- tagging age/premium when derivable;
- observed winner concentration and repeat receivers.

H0 provides evidence for:

- transaction feasibility;
- real competitor behavior;
- tip behavior where observable;
- timing after liquidation tagging;
- later p(win | opportunity, tip) estimation.

H0 does **not** invent win probabilities.

## 11. P2 — State + Reverse Indices + Premium Boundary Scheduler

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

If protocol debt/health can change over elapsed time without a new oracle/account update, schedule deterministic timed rechecks.

Required concept:

~~~text
next_health_boundary
~~~

The scheduler uses protocol/chain time semantics and exact premium state. It is not a generic polling loop.

## 12. P3 — Exact Health + Eligibility

Reproduce the pinned official Project 0 risk semantics exactly.

Freeze the full calculation matrix rather than one simplified price rule:

- liquidation eligibility;
- RequirementType;
- OraclePriceType;
- HealthPriceMode;
- PriceBias;
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

## 13. P4A — Deterministic Opportunity Economics + Ranking

P4A runs **before** Shadow has produced a trustworthy win-probability model.

It must not invent:

~~~text
p(win)
p(land)
competitor response
optimal waiting time
~~~

P4A ranks only from deterministic/currently observable economic terms.

Required economics include:

- repay/seize selection;
- liquidation amount;
- current allowed premium;
- Project 0 protocol fees;
- Project 0 flat SOL liquidation fee;
- swap/DEX fees;
- routing cost;
- slippage;
- priority fee;
- Jito tip;
- token-account/rent effects;
- required capital;
- capital efficiency;
- path-dependent failed-attempt cost;
- deterministic net profit if successful.

### Path-dependent failed-attempt cost

Do not model one global failed-attempt constant.

Use:

~~~text
failed_attempt_cost(
  submission_path,
  outcome_class,
  fee/tip semantics
)
~~~

At minimum distinguish:

~~~text
RPC not landed
RPC landed success
RPC landed revert/fail
Jito single-tx path
Jito bundle not landed
Jito bundle landed success
Jito bundle landed/reverted
~~~

Exact fee/tip behavior must be measured/verified in P5/P6; do not assume a failed Jito path is always free.

### P4A V1 execution rule

Before competition/win probabilities are proven:

> **Act when deterministic net profit at the current allowed premium is positive and all correctness/execution gates pass.**

Do not delay merely to wait for a larger tagged premium.

Tagging/optimal waiting is deferred to P4B after H0/Shadow evidence exists.

## 14. P5 — Refresh / Routing / Receivership Planner

A candidate must become a valid executable plan.

Protocol source decides what can execute inside receivership.

Pin from the source manifest:

- refresh allowlist;
- integration allowlist;
- swap-program allowlist;
- StartLiquidation/EndLiquidation ordering;
- flat fee semantics.

The receivership lifecycle is one transaction:

~~~text
[allowed prerequisite/refresh instructions where valid]
→ StartLiquidation
→ allowed withdrawals / integrations / swap / repay
→ EndLiquidation
~~~

EndLiquidation must satisfy the protocol's same-transaction/end-position introspection rules.

A Jito bundle may only wrap prerequisite transaction(s) plus the final receivership transaction where protocol semantics allow. It must not split StartLiquidation and EndLiquidation across different transactions.

### RouteProvider order

Production route choice should prefer avoiding paid APIs:

~~~text
1. local/precomputed route where protocol-allowed and technically correct
2. free/public allowed routing path
3. optional paid routing provider only after measured economic justification
~~~

A local route is only valid if its executable swap program is allowed by the pinned Project 0 source.

Measure separately:

- route latency;
- route correctness;
- build latency;
- transaction size;
- account count;
- compute units;
- quote quality;
- LUT/transaction-format usage;
- single-tx/bundle feasibility;
- failed-attempt fee/tip behavior by submission path.

## 15. P6 — Full Hot Path + Shadow Race

Measure the complete production code path:

~~~text
stream update
→ RAM update
→ reverse index / timed boundary
→ exact health
→ P4A deterministic ranking
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

For submission/landing also record:

~~~text
transaction_version
serialized_bytes
submission_path
single_tx / bundle
leader/client type where reliably derivable
landed / rejected / expired
failure reason
actual fees
actual tips
~~~

Loss reasons are classified, not guessed.

## 16. P7 — Economic / Competition Qualification

Run historical and forward shadow evidence long enough to support economic decisions.

For every actual liquidation answer:

- did we observe it?
- when did we classify it?
- was it deterministically profitable?
- when was our execution ready?
- who won?
- at what tag age/current premium?
- what transaction version/path did the winner use?
- where did we lose?
- what would each qualified provider/path have cost?

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

Only here may a probabilistic model be trained/estimated, for example:

~~~text
p(win | opportunity features, current premium, tag age, tip, path)
~~~

Any probability used later must be empirical and versioned against a frozen dataset/forward window.

P7 also performs provider economics:

~~~text
incremental opportunities/profit captured by provider
-
subscription/usage cost
-
operational/dependency cost
~~~

Paid provider is rejected when this incremental value is not positive and material.

## 17. P4B — Probabilistic Ranking / Tip / Optimal-Stopping Policy

P4B is activated only after P7 provides sufficient evidence.

Possible later ranking objective:

~~~text
expected_net_profit
=
p(win | opportunity, tip, path, tag_age)
× deterministic_profit_if_won
- expected_failed_attempt_cost
- marginal infrastructure/provider cost
~~~

Only at this stage may the searcher:

- rank by empirical win probability;
- optimize Jito tip;
- model competition;
- decide whether to wait for a larger tagging premium;
- solve the tagged-account act-now-vs-wait problem;
- allocate capital across simultaneous opportunities using expected value.

No probabilistic policy enters LIVE without out-of-sample/shadow validation.

## 18. P8 — Controlled LIVE

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

Every execution reconciles:

~~~text
planned
vs submitted
vs landed
vs token deltas
vs fees/tips
vs realized net PnL
~~~

## 19. P9 — Production LIVE

Operate the qualified searcher and monitor:

- realized net PnL;
- capital efficiency;
- feed/state quality;
- latency distributions;
- route quality;
- submission/landing success;
- capture rate;
- failed-attempt costs by path;
- competition;
- program/config changes;
- provider spend;
- infrastructure spend;
- incremental profit attributable to each paid provider.

A paid service is periodically re-qualified. If its measured incremental profit no longer exceeds its full cost, remove it.

## 20. K — Measured Escalation Only

Add infrastructure only for a demonstrated bottleneck or positive economic case.

Examples:

~~~text
INGRESS bottleneck
→ compare provider vs own Geyser/node

ROUTING bottleneck
→ local/precomputed routing investigation

SUBMISSION bottleneck
→ Jito region/tip/inclusion path investigation

CPU bottleneck
→ profiling / allocation / affinity / sharding
~~~

### Own-node rule

An own Solana node/Geyser path is **not automatically cheaper**. It replaces SaaS cost with:

- hardware/bare-metal;
- storage;
- bandwidth;
- operations;
- redundancy.

Promote own-node operation when:

~~~text
net economic value of own node
>
best qualified provider path
~~~

No own node, Geyser, special network, custom kernel, NIC tuning, massive sharding or multi-region architecture merely because it could be faster.

## 21. Provider / Portal Policy

The default target is **zero unnecessary recurring portal subscriptions**.

Do not architect production as:

~~~text
QuickNode paid
+ Helius paid
+ Jupiter paid
+ second paid RPC
+ historical-data SaaS
+ monitoring SaaS
+ other portals
~~~

unless each service separately earns its place economically.

Desired ownership:

~~~text
on-chain state
→ local/native state store
→ local health engine
→ local economics/ranking
→ local build/sign
→ qualified routing
→ direct Jito/RPC inclusion
~~~

Historical H0 data may use a one-time paid backfill if necessary, because that is not automatically a permanent production dependency. Its cost must still be recorded.

## 22. Measurement ledger

All Project 0 measurements belong in:

- [SOLANA_PROJECT0_MEASUREMENT_LEDGER.md](SOLANA_PROJECT0_MEASUREMENT_LEDGER.md)

Raw evidence should later live under a dedicated Project 0 evidence directory once implementation begins.

## 23. Naming warning

Existing files named P0_* in this repository belong to the **older Pump.fun Phase-0/Doctor work**. They are not documentation for the new **Project 0 Liquidator**.
