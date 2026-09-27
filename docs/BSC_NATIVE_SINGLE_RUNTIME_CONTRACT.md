# DarkBot BSC — Maximum-Performance Native Single-Runtime Contract

Status: **NORMATIVE**
Updated: 2026-09-27
Parent cleanup: `docs/BSC_PHASE_C_PYTHON_RETIREMENT.md`

## 1. Objective

Build BSC as a clean native runtime with no Python in the latency-critical or runtime-semantic path.

The same Rust runtime must be used for:

```
CAPTURE
REPLAY
PAPER/SHADOW
LIVE
```

There is no alternate test engine, no Python trading engine and no "fast live" implementation.

Only external sources/sinks, credentials and clock adapters may differ.

Zero latency cannot be physically guaranteed because network, CPU, kernel and peer scheduling are external realities. Instead this contract removes avoidable software latency and defines hard release gates. A build that misses those gates does not ship.

## 2. Final ownership

### Go / bsc-geth

Go owns only what should remain inside the chain client:

- BSC devp2p/network protocol;
- canonical chain import;
- txpool validation/admission;
- peer management;
- minimal timestamped Fast-Lane hooks;
- minimal signed-transaction injection/broadcast hook.

Do not put strategy, venue logic, economics or research code into geth.

### Rust / DarkBot runtime

Rust owns all BSC runtime semantics:

- native event model;
- pending identity;
- sender/nonce replacement competition;
- canonical/reorg/orphan state;
- Flap recognition/decoding;
- Four.meme recognition/decoding;
- runtime feature/state extraction;
- Strategy C;
- economics/local quote math;
- admission/risk;
- transaction construction;
- secp256k1 signing;
- execution intent;
- latency telemetry;
- binary capture journal;
- replay;
- PAPER/shadow;
- LIVE runner;
- native output/result records.

### Python

Python is offline/control analysis only:

- parameter sweeps;
- statistical EV analysis;
- plots;
- reports;
- SQLite/Parquet generation;
- notebooks;
- human-facing research utilities.

Python output may configure Rust, but Python may not make a BSC runtime trade decision.

## 3. Ingress: capture as early as technically useful

Use two native BSC event classes.

### EARLY_TX_SEEN

Timestamp and export the transaction as early as safely practical after peer-message transaction decoding.

Purpose:

- preserve maximum market lead;
- perform destination/selector filtering immediately;
- predecode venue fields;
- precompute any deterministic state/transaction material possible before txpool acceptance.

### TXPOOL_ACCEPTED

Emit when the local node has accepted/validated the transaction into the relevant txpool path.

Purpose:

- authoritative local acceptance;
- final gate for any policy that requires validated pending evidence.

Both event classes enter the same versioned binary wire format and are captured/replayed.

This allows early speculative computation without silently treating unvalidated network spam as authoritative state.

## 4. Egress

Rust outputs a fully signed canonical transaction representation through a second shared-memory SPSC ring.

The geth hook:

1. consumes signed bytes;
2. performs required local chain-client validation/admission;
3. injects into txpool;
4. triggers normal/optimized peer propagation;
5. emits acceptance and first-outbound timing evidence.

No JSON-RPC/HTTP/WSS is used for LIVE submission.

## 5. Shared-memory Fast Lane

Primary inter-process transport:

```
Go producer -> mmap SPSC ring -> Rust consumer
Rust producer -> mmap SPSC ring -> Go consumer
```

Requirements:

- one producer / one consumer per ring;
- cache-line aligned producer/consumer indices;
- fixed-capacity descriptors;
- preallocated payload slab for variable transaction bytes;
- sequence numbers;
- no locks on common path;
- no allocation on the ring common path after initialization;
- explicit full/overflow state;
- no silent overwrite;
- busy-poll option on dedicated cores;
- Unix socket only as debug/control fallback, never primary LIVE path.

Do not use JSON as the wire format.

## 6. Same Rust core in all modes

The following Rust functions/modules exist once:

```
decode_event
update_chain_state
recognize_venue
decode_venue
update_market_state
strategy_decide
quote
admission
build_transaction
sign_transaction
```

Replay/PAPER/LIVE call the same implementations.

Forbidden:

- replay-only decoder;
- paper-only quote;
- live-only strategy;
- alternate fast signer;
- Python decision oracle in production;
- mode-specific state transition behavior.

PAPER runs through build + sign. Only broadcast is replaced by a non-broadcast sink.

## 7. Capture format

Primary BSC research evidence is an immutable versioned binary journal.

The event is fanned out at the exact same boundary used by the Rust core:

```
native event
   +--> Rust core
   +--> bounded Rust/native recorder queue
```

The primary recorder implementation is native/Rust, not Python.

Required properties:

- append-only;
- sequence-preserving;
- checksummed;
- hash-addressed manifest;
- wire-format version;
- exact timestamps;
- raw tx bytes/lossless fields;
- canonical/reorg events;
- explicit gap/overflow markers.

SQLite/Parquet are generated later and are never the canonical replay source.

## 8. Canonical/reorg runtime is Rust

Do not keep canonical/reorg/replacement semantics in Python.

Rust must own:

- block/head identity needed by runtime;
- included/reverted outcome;
- canonical/orphan/uncertain state;
- sender+nonce candidate sets;
- replacement resolution;
- restart/recovery state required by LIVE/PAPER/replay equivalence.

Phase-C Python semantics are ported as fixtures and adversarial tests.

## 9. Hot-thread rules

The trading thread is deliberately simple:

```
busy-poll event
-> cheap address/selector reject
-> fixed decode
-> state
-> decision/quote
-> build/sign
-> submit ring
```

Common path rules:

- no filesystem;
- no DB;
- no DNS;
- no RPC;
- no HTTP;
- no WSS;
- no JSON;
- no logging formatting;
- no blocking locks;
- no unbounded collections;
- no async task scheduling on the hot thread;
- no dynamic ABI reflection;
- no metadata lookup;
- no heap allocation after warmup where technically avoidable;
- no lazy initialization;
- no page faults expected after warmup/prefault;
- no synchronous external quote/helper call.

Use fixed verified ABI offsets and integer arithmetic.

## 10. Build/profile optimization

Release builds must benchmark at least:

- Rust `opt-level=3`;
- LTO;
- codegen-units=1 for latency build;
- `panic=abort`;
- target CPU tuned for production hardware where deployment policy allows;
- PGO after representative native capture exists.

Do not accept compiler flags on belief alone; retain only settings that improve measured tails.

The signer context is pre-created and warm.

## 11. Host isolation

Production latency tests and LIVE use dedicated Linux hardware.

Evaluate and freeze:

- isolated physical hot core;
- no competing SMT sibling;
- fixed CPU affinity;
- Go fast-lane producer on a separate dedicated core;
- Rust hot thread on its own core;
- writer/analysis on other cores;
- NIC IRQ/RSS/RPS/XPS away from the Rust hot core;
- performance CPU governor;
- disabled swap;
- memory prefault/`mlockall` where justified;
- NUMA-local memory/NIC placement;
- real-time scheduling after validation;
- no noisy colocated workloads.

A shared VPS/macOS result cannot establish the final latency gate.

## 12. Timing model

Capture at least:

```
T-1 peer message decoded / EARLY_TX_SEEN
T0  local txpool accepted
T1  Fast Lane publish
T2  Rust receive
T3  venue decode complete
T4  strategy + quote complete
T5  transaction built
T6  signing complete
T7  submit ring publish
T8  local geth accepts our signed tx
T9  first outbound peer handoff/write
TC  canonical reference observation
```

Report:

```
early_lead         = T-1 - TC
validated_lead     = T0  - TC
internal_from_early= T8  - T-1
internal_validated = T8  - T0
submit_lead        = T8  - TC
peer_lead          = T9  - TC
```

## 13. Targets

External market objective:

```
early_lead <= -10 ms
```

where the BSC peer topology actually provides it.

Internal engineering target:

```
EARLY_TX_SEEN -> local acceptance of our signed tx  < 0.5 ms
stretch                                         < 0.25 ms
>= 1.0 ms                                       investigate/fail gate
> 3.0 ms reproducible runtime path              release blocker
```

For individual components use design budgets, not guarantees:

- Go hook -> Rust visibility: tens of microseconds target;
- Rust decode/state/strategy/quote: low hundreds of microseconds or less;
- build/sign: low hundreds of microseconds or less;
- Rust -> geth acceptance: tens/low hundreds of microseconds target.

Every stage reports p50/p95/p99/p99.9/p99.99/max plus exact counts above 0.5/1/3 ms.

No percentile may hide a bad maximum.

## 14. Optional maximum-speed escalation

The core is designed so ingress can be replaced without changing strategy semantics.

### Level 1 — native geth Fast Lane

Default.

Use earliest practical geth peer/txpool hooks + shared memory.

### Level 2 — dedicated Rust devp2p observer

Only if measurement proves geth ingress itself consumes material lead.

A Rust BSC devp2p observer may receive the same peer transaction stream earlier and feed the exact same Rust event/core API.

It must not create a second strategy/runtime.

### Level 3 — kernel/NIC specialization

Only if kernel receive jitter is proven to be the remaining economic bottleneck:

- dedicated NIC queue;
- busy polling;
- AF_XDP/XDP or equivalent measured approach;
- hardware timestamping where useful.

Do not add DPDK/AF_XDP complexity without evidence. The architecture permits it without changing the Rust core.

## 15. Rust workspace

Target:

```
native/
  Cargo.toml
  crates/
    darkbot-wire/
    darkbot-fastlane/
    darkbot-core/
      event/
      chain_state/
      venues/
      economics/
      strategy/
      tx/
      signer/
    darkbot-capture/
    darkbot-runner/
      replay
      paper
      live
```

Keep packages responsibility-based and small. Do not create a framework for hypothetical chains.

## 16. Solana relationship

Existing Solana/Pump.fun remains frozen and working while BSC native work is proven.

Do not rewrite Solana merely to make the BSC migration look uniform.

However, the Rust core is designed chain-neutral enough that a later controlled Solana convergence can reuse:

- strategy interfaces;
- execution intent;
- portfolio/risk concepts;
- capture/replay mode model;
- latency telemetry.

After BSC is stable, Solana may be migrated phase-by-phase into the same Rust runtime with strict replay/PAPER parity. Only after that parity passes may old Python trading runtime be removed.

The final desired product can therefore become:

```
one Rust runtime
  + Solana native adapter
  + BSC native adapter
  + Pump.fun / Flap / Four.meme venue modules
Python offline analysis only
```

without risking the currently proven Solana system during BSC development.

## 17. Acceptance principle

Architecture is accepted only by measurement.

No claim such as "Rust is fast" is evidence.

Each phase must produce raw latency samples, target-host metadata, correctness tests and capture/replay parity.

If the -10 ms market lead does not exist in real BSC evidence, report it. Software cannot manufacture network lead that the market does not expose.
