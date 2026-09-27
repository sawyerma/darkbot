# DarkBot Solana + BSC Roadmap — Gate-0 First

Status: **ACTIVE AUTHORITATIVE ROADMAP**
Updated: 2026-09-27

Normative documents, highest priority first:
1. `docs/GATE0_TESTPLAN_SHORT.md` — feasibility gate; decides whether BSC/Solana low-latency work proceeds
2. `docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md` — BSC birth-entry objective and evidence rules
3. `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md` — production runtime contract after Gate-0 PASS
4. phase-specific BSC contracts
5. `docs/BSC_PHASE_C_PYTHON_RETIREMENT.md` — retired Python BSC prototype policy

If an older document conflicts with Gate 0, Gate 0 wins.

## 0. NON-NEGOTIABLE EXECUTION INVARIANT — DIRECT ON-CHAIN ONLY

DarkBot **NEVER** uses the Four.meme or Flap website, browser UI, frontend trading flow, frontend API, page refresh, or manual interaction as part of BUY execution.

The BUY path is always:

```
earliest verified chain/pending evidence
-> venue decode / derive required BUY inputs
-> local quote/input calculation
-> construct exact contract transaction or ordered bundle
-> local secp256k1 signing
-> measured inclusion adapter
-> direct BSC execution against the verified venue contract
```

For Four.meme this means direct interaction with the verified current TokenManager/TokenManager2 trade surface; for Flap it means direct interaction with the verified current Flap/BNB Portal contract surface. Exact addresses, versions, selectors and modes must be frozen from current evidence before use.

A builder/bundle provider or local bsc-geth is an **inclusion transport**, not a trading frontend. It may carry the already-constructed signed transaction/bundle to block execution, but it never supplies strategy, quote, BUY logic or readiness.

Website/frontend state is **never** a readiness signal. DarkBot does not wait for a page to list, refresh, enable a button or expose a frontend API response.

The only external facts relevant to earliest valid execution are the verified on-chain/pending facts required by the current contract semantics. Internal strategy/admission/risk rules may still decline a BUY.

If token address and all mandatory BUY inputs are derivable before `T0`, DarkBot precomputes as much as possible before `T0` and may hand an ordered launch->BUY bundle to a proven inclusion path before the launch executes. If any mandatory input is not derivable before `T0`, DarkBot reacts at the earliest chain-visible point at which it becomes derivable. In both cases the execution remains direct on-chain.

Frontend/UI/API latency is therefore outside the BUY latency budget and outside the production dependency graph.

## 1. Core objective

The BSC product succeeds only if it can obtain the earliest technically possible successful BUY after a new launch becomes executable.

Fast local code, pending lead, geth acceptance or builder acknowledgement are not substitutes for actual execution ordering.

## 2. Current state

- Existing Solana/Pump.fun runtime remains operational.
- Python Phase-C BSC/EVM prototype remains retired / do not merge.
- No full BSC production runtime is built before Gate-0 feasibility is proven.
- Gate 0 may use a deliberately small native Rust measurement harness.

## 3. Phase sequence

| Phase | Status | Deliverable |
| --- | --- | --- |
| A | **DONE / KEEP** | Existing Solana/Pump.fun + Phase-A multichain seam |
| C-PY | **RETIRED** | Old Python BSC/EVM prototype retained only as engineering evidence |
| G0.1 | **NEXT** | Freeze current Flap/Four Mainnet contracts, versions, launch/BUY semantics |
| G0.2 | **BLOCKED** | Build BUY-field pre-computability matrix and freeze one exact PRIMARY_GATE0_SCOPE |
| G0.3 | **BLOCKED** | Prove minimal exact launch→BUY semantic validity for PRIMARY_GATE0_SCOPE before performance tuning |
| G0.4 | **BLOCKED** | Start bounded pending/actionable coverage in parallel as soon as launch evidence is frozen |
| G0.5 | **BLOCKED** | Benchmark/optimize complete local path through T_send after semantic BUY validity is proven |
| G0.6 | **BLOCKED** | Race full ingress×inclusion combinations plus concurrent-launch/nonce correctness |
| G0.7 | **BLOCKED** | Controlled minimal-value Mainnet validation |
| G0.8 | **BLOCKED** | Prove actual token acquisition, ordering, competing external BUYs, costs and entry price |
| G0.9 | **BLOCKED** | Classify external builder dependency: NONE / BENEFICIAL / REQUIRED / UNKNOWN |
| G0.10 | **BLOCKED** | Own-builder feasibility: validator/proposer reach, operating cost, bid competitiveness and independence from third-party providers |
| G0.11 | **BLOCKED** | PASS / ADAPT / STOP decision per venue |
| C0 | **BLOCKED BY G0 PASS** | Production Rust wire/event/journal/replay foundation |
| C1 | **BLOCKED** | Production bsc-geth Fast Lane + SPSC ingress |
| C2 | **BLOCKED** | Production pending/canonical/reorg/nonce/replacement state |
| C3 | **BLOCKED** | Production local build/sign/geth path benchmark; engineering proof only |
| D | **BLOCKED** | Full production Flap Rust decoder |
| E | **BLOCKED** | Full production Four.meme Rust decoder |
| F | **BLOCKED** | Native dual-venue capture and pending-coverage/market evidence |
| G | **BLOCKED** | Full production economics/quote verification |
| H | **BLOCKED** | Strategy C Rust + deterministic replay/EV |
| I | **BLOCKED** | PAPER/shadow plus full production hot-path requalification before LIVE |
| J | **BLOCKED** | Production LIVE enablement using Gate-0-proven inclusion adapters |
| KB | **BLOCKED / OPTIONAL AFTER PASS** | Own BSC builder track if external builder access is materially beneficial or required |
| K | **BLOCKED** | Only measured performance escalation |
| S0-S3 | **FUTURE** | Optional later native Solana convergence |

Every phase STOPs for independent review.

## 4. Gate 0 is not C0

Gate 0 deliberately does **not** require the complete production architecture.

Gate-0 ordering rule: prove the smallest exact BUY semantics first, collect ingress coverage in parallel, and only then invest in fine local latency optimization.

It needs only enough code to answer:

```
Can we know enough before/at first tradability
to construct the exact BUY,
and can that BUY occupy the earliest valid execution position?
```

Do not require full journal/replay/reorg/Strategy C/PAPER before answering this.

## 5. Gate-0 measurement model

```
T_pre = earliest actionable pre-tradability evidence
T0    = first execution point where the coin is buyable
T_buy = our successful BUY execution
```

Desired product target:

```
T_buy - T0 < 1 ms
```

For ordinary BSC chain evidence, do not fabricate a sub-ms `T_buy-T0` number.

Economic PASS evidence is actual transaction ordering:

```
launch position N
our successful BUY = earliest valid position after N
successful competing BUYs between = 0
```

Local engineering target remains <1 ms for the controllable hot path.

## 6. Gate-0 inclusion race is mandatory

Do not wait until J/K to investigate builders.

BSC production uses PBS/builders, and Gate 0 must compare the real available inclusion paths before a large implementation is justified.

Minimum candidates at the time of test:

- local bsc-geth/P2P;
- 48Club/Puissant ordered/backrun-target path if still supported;
- BlockRazor ordered/bundle path if still supported.

Reverify current APIs immediately before the test.

The winning path is selected from real execution placement, not ACK latency.

## 7. Gate-0 venue rules

### Flap

Before full Phase D, Gate 0 is allowed to implement a **minimal factual Flap probe** sufficient for:
- launch recognition;
- BUY-field preparation matrix;
- token-address derivation proof;
- exact first-BUY construction;
- semantic launch→BUY validity;
- ingress coverage measurement for the selected scope.

This is not the full production decoder.

### Four.meme

Before full Phase E, Gate 0 is allowed to implement a **minimal factual Four probe** sufficient for the same feasibility questions.

Do not assume pre-execution token-address derivation. Prove it or record that it is unavailable.

## 8. C0 — production foundation after PASS

Only after at least one BSC venue passes Gate 0:

- create clean Rust workspace;
- versioned native event schema;
- immutable binary journal;
- deterministic replay API;
- bounded queues/overflow evidence;
- exact integer/address/hash primitives;
- native latency records.

STOP.

## 9. C1 — production earliest ingress

Implement minimal pinned bsc-geth hooks:

- earliest practical complete transaction event;
- txpool acceptance;
- canonical/reorg events;
- source peer/message provenance;
- shared-memory SPSC into Rust.

Measure peer decode → Rust visibility.

Pending coverage remains a quality metric; do not assume completeness.

STOP.

## 10. C2 — production BSC state

Implement only runtime semantics needed for correctness:

- tx identity/first-seen;
- sender+nonce competition/replacement;
- included/reverted;
- canonical/orphan/uncertain;
- bounded reorg ancestry;
- restart/recovery;
- explicit gaps/provenance.

Port relevant Python Phase-C counterexamples as Rust tests.

STOP.

## 11. C3 — production local execution engineering gate

C3 no longer pretends to prove a real BUY.

It proves only the production local path:

```
native event
-> Rust
-> decode/state/decision/quote
-> build/sign
-> selected local submission adapter
```

Target local controllable latency: <0.5 ms where achievable; >=1 ms investigated; reproducible >3 ms blocks release until explained.

Actual economic BUY feasibility was already proven in Gate 0.

STOP.

## 12. D/E — full production venue decoders

D = full Flap decoder.
E = full Four.meme decoder.

Gate-0 minimal probes are not independent production engines.

Production decoder requirements:

- one Rust source of truth;
- verified current deployment/version;
- pending + canonical factual decode;
- unknown modes fail closed;
- same functions in capture/replay/PAPER/LIVE;
- no remote fetch in hot path.

STOP after each.

## 13. F — native market evidence

Run the production dual-venue capture.

Primary purpose:

- pending coverage;
- complete/full-tx arrival paths;
- peer first-seen spread;
- venue version/mode census;
- canonical outcomes;
- quality gaps;
- replay identity;
- production ingest/decode tails.

Canonical timing may be retained as diagnostic data, but **`EARLY_TX_SEEN - canonical observation <= -10 ms` is not a product PASS gate**.

The useful pre-lead question is whether actionable evidence arrives before first executability often enough to support the Gate-0-proven path.

STOP.

## 14. G — full economics

Implement complete executable quote/cost semantics once in Rust.

Gate 0 already contains only the minimal exact economics needed to make its test BUY valid.

Phase G adds full supported-mode economics:

- protocol fees/taxes;
- exact quote/output math;
- gas/revert/replacement costs;
- sell/approval path;
- migration/graduation economics.

No synchronous external quote in the live hot path.

STOP.

## 15. H — Strategy C / EV

Only now add Strategy C to the BSC production runtime.

Use same:
- state;
- venue decode;
- economics;
- builder;
- signer.

Positive expectancy after complete costs is a separate question from Gate-0 feasibility.

STOP.

## 16. I — PAPER/shadow + final production hot-path requalification

Run the same production semantics through build/sign in PAPER/shadow, but distinguish two boundaries:

```
production event
-> full production venue decoder
-> production state
-> production economics / quote
-> production strategy / admission
-> production transaction build
-> production signer
-> production serialization
-> PAPER test-sink handoff = T_shadow_sink
```

`T_shadow_sink` is useful for isolating local production-code cost but is **not** the real transport boundary and must never be reported as `T_send`.

Before broad LIVE is enabled, run a controlled transport revalidation using the actual Gate-0-proven production adapter:

```
production event
-> same full production path
-> actual adapter queue/IPC
-> actual defined transport handoff = T_send
```

The final production program must still satisfy the frozen local latency requirement using the same adapter-specific timing boundary defined in Gate 0 **G0.5**. A fast Gate-0 prototype, C3 result or PAPER test sink cannot waive the real transport revalidation.

Also rerun the required concurrent-launch/nonce stress cases against the final production runtime.

STOP.

## 17. J — production LIVE

J is production enablement, not the first time inclusion is tested. Its first controlled step revalidates the actual production transport boundary before broader LIVE is permitted.

Use only inclusion adapters already validated in Gate 0 and revalidated for current service/API behavior.

Validate:
- actual intended-token acquisition by the intended recipient;
- ordering against external competing BUYs;
- realized gas/tips/slippage and total debit/cost;
- replacement/reconciliation;
- replay/decision parity;
- continued compliance with the final production hot-path latency gate.

STOP.

## 18. KB — own-builder independence track

This phase is activated only if Gate 0 shows that an external builder materially improves or is required for the target inclusion quality.

External builders are not a permanent architectural dependency by default.

Evaluate whether DarkBot can operate its own BSC builder using current official builder interfaces/reference implementations.

The acceptance criteria are infrastructure-level:

- validator/proposer integration is technically available;
- sufficient proposer coverage can be reached;
- valid competitive block bids can be produced reliably;
- operating cost is known;
- maintenance complexity is known;
- third-party dependency is reduced without degrading the measured inclusion result.

Possible outcomes:

- **OWN_BUILDER_PASS** — own builder can replace the external provider for the required quality;
- **OWN_BUILDER_PARTIAL** — hybrid operation is justified;
- **OWN_BUILDER_FAIL** — retain the external builder only where its measured benefit justifies the dependency/cost.

Building or operating a validator is not required by this phase.

STOP.

## 19. K — measured escalation only

Escalate only when production measurements identify the bottleneck.

Possible order:

1. peer-path optimization;
2. inclusion/builder path optimization;
3. geographic sensors;
4. direct Rust devp2p;
5. kernel/NIC specialization.

Do not prebuild all levels.

## 20. Architecture invariants

Production remains:

```
pinned bsc-geth
+ native event path
+ one Rust runtime
+ one decision implementation
+ one build/sign implementation
+ one recorder/replay format
+ measured inclusion adapters
```

Forbidden:

- Python BSC trading runtime;
- duplicate strategy/decoder/quote implementations;
- public WSS as the sole primary production signal;
- remote quote/RPC lookup in the decision hot path;
- treating local geth acceptance or builder ACK as a BUY;
- claiming exact sub-ms on-chain execution from ordinary block timestamps.

## 21. Decision rule

If Gate 0 returns STOP for both Four.meme and Flap, the BSC Birth-Sniping production roadmap stops.

If one venue passes, production work proceeds only for the proven venue first. The second venue may remain ADAPT/STOP until separately proven.
