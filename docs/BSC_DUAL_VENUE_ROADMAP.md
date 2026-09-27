# DarkBot Solana + BSC Roadmap — Clean Native BSC, Maximum-Performance Runtime

Status: **ACTIVE AUTHORITATIVE ROADMAP**
Updated: 2026-09-27

Normative architecture:
- `docs/GATE0_TESTPLAN_SHORT.md` — kurzer vollständiger Ausführungsplan für Gate 0
- `docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md` — final MUST/OPTIONAL/FORBIDDEN birth-entry architecture
- `docs/BSC_PHASE_C_PYTHON_RETIREMENT.md`
- `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`

## 1. Core decision

Do not merge the Pass-6 Python Phase-C BSC/EVM runtime.

Start native BSC work from the clean current `main`, which already contains the useful Phase-A multichain seam and the working Solana/Pump.fun system.

BSC runtime semantics are Rust-only.

Go/bsc-geth provides only native chain hooks.

Python is offline analysis/control only for BSC.

## 2. Final target

```
                         DARKBOT
                            |
              +-------------+-------------+
              |                           |
          Solana adapter               BSC adapter
              |                           |
          Pump.fun                  Flap / Four.meme
              \                           /
               +--------- Rust Core ------+
                          |
             strategy/economics/risk
               build/sign/replay
                PAPER/LIVE/capture
                          |
                    native execution

Python: offline analysis/reporting only
```

During migration, existing Solana Python runtime remains frozen and operational. It is not mixed into the new BSC runtime.

## 3. Phase sequence

| Phase | Status | Deliverable |
| --- | --- | --- |
| A | **DONE / KEEP** | Existing Solana/Pump.fun + Phase-A multichain seam |
| B | **SKIPPED** | Optional cleanup; no need |
| C-PY | **RETIRED / DO NOT MERGE** | Pass-6 Python BSC/EVM prototype kept only as external evidence |
| C0 | **BLOCKED BY GATE 0** | Clean Rust workspace + wire/event model + native binary capture/replay skeleton |
| C1 | **BLOCKED** | Minimal bsc-geth early/accepted ingress hooks + shared-memory SPSC Fast Lane |
| C2 | **BLOCKED** | Rust canonical/reorg/nonce/replacement state + native capture/replay parity |
| C3 | **BLOCKED** | Native signed-tx return path + geth admission/broadcast + T-1..T9 latency gate |
| D | **BLOCKED** | Flap factual decoder in same Rust core |
| E | **BLOCKED** | Four.meme factual decoder in same Rust core |
| F | **BLOCKED** | Real 30-minute native dual-venue capture, exact same runtime event stream |
| G | **BLOCKED** | Rust economics/quote gate from real evidence |
| H | **BLOCKED** | Strategy C Rust single-source + deterministic replay/EV |
| I | **BLOCKED** | Rust PAPER/shadow, same decode/state/strategy/quote/build/sign |
| J | **BLOCKED** | LIVE native execution, inclusion and peer-propagation validation |
| K | **BLOCKED** | Latency escalation only where measured: direct Rust devp2p / AF_XDP etc. |
| S0-S3 | **FUTURE** | Optional controlled Solana convergence into the proven Rust core |

Every phase STOPs for independent review.


## Gate 0 — Birth-to-BUY latency definition

This gate is mandatory before further low-latency work on either chain.

Test objects:
- Solana / Pump.fun
- BSC / Four.meme
- BSC / Flap

Use one identical timing model:

```
T_pre = earliest usable indication that the coin is about to become tradable
T0    = first instant the coin is actually tradable/buyable
T_buy = instant our BUY has actually executed successfully
```

Lead metric:

```
pre_lead = T_pre - T0
```

Target: as negative as possible. About -10 ms is a desired lead, not an assumed fact.

Primary decision metric:

```
buy_delay = T_buy - T0
```

Target:

```
buy_delay << 1 ms
```

Gate classification:
- <1 ms: PASS
- 1-3 ms: continue only if a concrete measured bottleneck is removable
- >3 ms: current path FAIL; change infrastructure and retest
- ~10 ms or more: STOP that venue/chain for Birth Sniping

Also record:
- was this the first technically possible buy: YES/NO
- number of successful BUYs ahead of us
- distance from first-tradable point to our BUY
- achieved entry price versus first executable price
- revert/drop/failure outcome

Do not use as the primary metric:
- block time
- finality time
- local decode time alone
- signing time alone
- submit acknowledgement alone
- EARLY_TX_SEEN versus canonical block time

Venue selection:
- do not preselect Four.meme or Flap
- choose the venue with the lowest measured `T_buy - T0`
- if latency is similar, prefer fewer successful competing BUYs ahead

Project order:
- Gate 0 comes before further BSC buildout
- Gate 0 comes before any Solana low-latency rebuild
- only PASS branches proceed

## 4. Phase C0 — clean native foundation

Create no Python BSC runtime.

Implement:

- Rust workspace;
- versioned binary `FastLaneEvent` and canonical-event schemas;
- fixed-width/lossless EVM integer/address/hash primitives;
- immutable binary journal;
- journal manifest/hash;
- Rust replay reader feeding exactly the same `Engine::process_event` API as LIVE will use;
- monotonic sequence/order model;
- bounded queues and explicit overflow events;
- native latency sample format.

Port Phase-C adversarial semantic fixtures into Rust unit/property tests where applicable.

Acceptance:

- binary event -> journal -> replay is byte/semantic deterministic;
- no SQLite required for replay;
- no JSON on native runtime path;
- no Python dependency in the Rust runner.

STOP.

## 5. Phase C1 — earliest native BSC ingress

Patch a pinned bsc-geth version minimally.

Add:

- `EARLY_TX_SEEN` hook as early as practical after peer transaction-message decode;
- `TXPOOL_ACCEPTED` hook after local txpool acceptance;
- canonical/head hook required by Rust state;
- shared-memory SPSC producer;
- exact local timestamps and sequence correlation.

Rust consumer:

- dedicated busy-poll thread;
- address/selector prefilter;
- no venue semantics yet.

Measure:

```
peer decode -> Fast Lane publish -> Rust visible
```

In addition, instrument the ingress so later -10 ms work is evidence-driven without changing runtime semantics:

- distinguish full-transaction receipt from hash-only announcement/request/response paths;
- record the source peer id for first-seen evidence;
- record per-peer first-seen deltas, duplicates, invalids and disconnects;
- retain peer-message type and timing in the binary evidence stream;
- do not yet add peer ranking, multi-region fan-in or a second ingress implementation.

Acceptance is measurement-based on dedicated Linux target hardware.

Simplicity rule: C1 remains one local bsc-geth, one Fast Lane and one Rust consumer. Instrumentation is allowed; topology expansion is not.

STOP.

## 6. Phase C2 — Rust EVM/BSC state

Port only the semantics actually required by correct BSC runtime:

- tx identity;
- first-seen;
- sender+nonce candidate competition;
- replacement state;
- included/reverted;
- canonical/orphan/uncertain;
- bounded ancestry/reorg;
- restart snapshot/recovery required by runtime;
- exact provenance/gap semantics.

Do not port the old Python class/file architecture.

Use the Pass-6 counterexamples as Rust tests.

Capture/replay must produce identical state transitions.

STOP.

## 7. Phase C3 — native submit and complete latency proof

Add second SPSC shared-memory ring:

```
Rust signed tx -> geth
```

Implement:

- Rust tx representation/builder skeleton;
- warm secp256k1 signer;
- deterministic dummy/fixture transaction build+sign;
- geth native admission;
- timestamp of acceptance;
- timestamp of first outbound peer handoff where technically measurable.

No venue trade logic yet.

Mandatory timing:

```
T-1 EARLY_TX_SEEN
T0  TXPOOL_ACCEPTED source tx
T1  ring publish
T2  Rust receive
T5  built
T6  signed
T7  submit
T8  local geth accepted ours
T9  outbound peer handoff
```

Gate:

- engineering target T-1 -> T8 <0.5 ms;
- stretch <0.25 ms;
- every >=1 ms sample investigated;
- reproducible >3 ms runtime path blocks D;
- report full tails, never p99-only.

STOP.

## 8. Phase D — Flap Rust decoder

Use `docs/BSC_PHASE_D_FLAP_READ_ONLY_CONTRACT.md`.

Important:

- one Rust implementation;
- exact FastLaneEvent input;
- same decoder later in F/H/I/J;
- no Python runtime decoder.

STOP.

## 9. Phase E — Four.meme Rust decoder

Use `docs/BSC_PHASE_E_FOURMEME_READ_ONLY_CONTRACT.md`.

Same single-runtime rule.

STOP.

## 10. Phase F — 30-minute native capture

Use `docs/BSC_PHASE_F_DUAL_CAPTURE_CONTRACT.md`.

Primary dataset is the exact binary native event journal consumed by the Rust core.

Measure real market lead:

```
early_lead     = T-1 - TC
validated_lead = T0  - TC
```

Primary objective:

```
early_lead <= -10 ms
```

Report the truth if the network does not provide it.

The exact journal is replayed through the same Rust core before Phase-F acceptance.

Phase F also reports where lead is gained/lost:

- earliest peer by relevant launch transaction;
- full-tx vs hash-announcement arrival path;
- peer-to-peer first-seen spread for the same transaction;
- geth peer-decode -> EARLY_TX_SEEN cost;
- EARLY_TX_SEEN -> Rust-visible cost.

If the single-node topology already reaches the -10 ms objective with sufficient stability, no topology expansion is added.

If it does not, the deficit becomes measured input to Phase K. Do not solve it inside Phase F by inventing a second capture/live architecture.

STOP.

## 11. Phase G — economics

Executable quote/cost semantics live once in Rust.

Validate against real captured state, receipts and authoritative contract/helper calls offline.

LIVE decision path has:

- local state;
- local integer quote math;
- no eth_call;
- no RPC;
- no HTTP.

STOP.

## 12. Phase H — Strategy C + EV replay

Implement Strategy C once in Rust.

Run the Phase-F binary journal directly through:

```
same state
same venue decoder
same economics
same strategy
same builder
same signer
```

Research sweeps change config/parameters, not implementation.

Python may consume Rust result files for statistics only.

Positive expectancy must be shown after complete costs before PAPER progression.

STOP.

## 13. Phase I — PAPER/shadow

Run native live event source or exact journal through the same Rust core.

PAPER executes through signed transaction creation.

Only final broadcast sink is suppressed.

Compare shadow decisions against deterministic replay.

Latency is measured end-to-end exactly as LIVE except external broadcast.

STOP.

## 14. Phase J — LIVE

Enable native submit sink.

No Python component may be on:

```
event -> decision -> quote -> build -> sign -> submit
```

Validate:

- local acceptance;
- peer handoff;
- inclusion;
- replacement behavior;
- realized fees/slippage;
- decision/replay parity;
- actual submit lead relative to canonical reference.

LIVE release fails on unresolved deterministic/reproducible >3 ms native runtime paths.

STOP.

## 15. Phase K — measured -10 ms escalation ladder only

Phase K exists only if Phase F/J measurements show the -10 ms objective or the internal latency budget is being lost outside the Rust core.

Do not enable all items by default. Apply one level, measure, keep it only if it provides a reproducible economic latency benefit.

### K1 — peer-path optimization

Use the C1/F evidence to improve the existing single-node topology:

- prefer peers that actually deliver relevant full transactions earliest, not merely peers with low ping;
- distinguish full-TX delivery from hash-announcement/request paths;
- maintain bounded per-peer first-seen/duplicate/invalid/disconnect statistics;
- evaluate static/trusted peer topology only where operationally valid;
- measure geth receive/decode/validation delay before changing anything.

No strategy/runtime fork.

### K2 — geographically separate native sensors

Only if peer geography is proven to dominate first-seen timing.

Run the same pinned native sensor/wire protocol in a small number of regions and compare identical transaction hashes.

Requirements:

- same event format and timestamp semantics;
- no venue/strategy logic duplicated in regional sensors;
- source id and clock-quality metadata always recorded;
- a single explicit earliest-valid-event arbitration rule used identically in capture/replay/PAPER/LIVE if this topology becomes production;
- do not create separate regional strategy engines unless a later dedicated design proves this necessary and preserves deterministic trade identity.

The purpose is to discover whether network topology can recover additional milliseconds toward -10 ms.

### K3 — direct Rust devp2p ingress

Only if measurement proves bsc-geth ingress itself consumes material lead.

A dedicated Rust BSC devp2p observer may feed the exact same Rust event/core API earlier.

The existing Rust strategy/state/economics/build/sign path remains unchanged.

### K4 — propagation optimization

If ingress and Rust processing are already fast but outbound propagation dominates:

- measure local geth acceptance -> first peer handoff;
- evaluate peer selection/topology;
- where safe and deterministic, fan the exact same signed raw transaction to multiple controlled local/remote BSC broadcast nodes;
- never generate different strategy decisions or different transaction payloads per broadcast node.

### K5 — kernel/NIC specialization

Only if kernel/NIC jitter is proven to be the remaining bottleneck:

- dedicated NIC queue;
- IRQ/RSS/RPS/XPS isolation;
- busy polling / SO_BUSY_POLL where measured;
- AF_XDP/XDP only where it improves tails;
- hardware timestamping where useful;
- NUMA/NIC locality.

Do not add DPDK/AF_XDP complexity without measured need.

### K simplicity gate

At every level:

- one Rust core semantics;
- one binary event contract;
- one decision implementation;
- one build/sign implementation;
- bounded state;
- no generic event bus;
- no microservices framework;
- no second database/runtime architecture;
- no optimization kept without benchmark evidence.

The simplest level that meets the economic target wins.

## 16. Solana convergence S0-S3

This is future work after BSC Rust runtime is proven.

Goal: final one-runtime product without risking existing Solana.

### S0
Native Solana event adapter feeding the same Rust event/core interfaces.

### S1
Pump.fun decode + Strategy parity against frozen existing captures/results.

### S2
Replay/PAPER parity, then native send path.

### S3
Only after full parity and forward validation, retire the old Python Solana trading runtime.

Python then remains offline analysis only for both chains.

## 17. Simplicity invariant

"Maximum performance" does not mean maximum architecture.

Default production shape remains:

```
one pinned bsc-geth
+ two SPSC rings
+ one Rust runtime
+ one native recorder
```

Additional nodes/regions/devp2p/kernel specialization are optional adapters behind the same event contract and are activated only by measured deficit.

Do not prebuild K2-K5 while C0-C3/D/E/F are still sufficient.

## 18. Repository cleanliness rule

The production repository must never contain both the retired Python BSC runtime and the native Rust BSC runtime.

No old Phase-C fallback.

No compatibility bridge.

No old EVM SQLite schema as mandatory runtime state.

No second decision engine.

The clean source of truth is the Rust runtime plus pinned native chain adapters.
