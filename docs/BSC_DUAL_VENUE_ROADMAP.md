# DarkBot Solana + BSC Roadmap — Clean Native BSC, Maximum-Performance Runtime

Status: **ACTIVE AUTHORITATIVE ROADMAP**
Updated: 2026-09-27

Normative architecture:
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
| C0 | **NEXT** | Clean Rust workspace + wire/event model + native binary capture/replay skeleton |
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

Acceptance is measurement-based on dedicated Linux target hardware.

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

## 15. Phase K — only if more speed is measurably available

Do not prematurely add complexity.

If profiling shows geth ingress materially consumes lead:

- add a dedicated Rust devp2p observer feeding the exact same Rust event API.

If kernel/NIC jitter then dominates:

- dedicated NIC queue;
- busy-poll/socket tuning;
- AF_XDP/XDP where measured beneficial;
- hardware timestamps if useful.

No strategy fork is permitted at any level.

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

## 17. Repository cleanliness rule

The production repository must never contain both the retired Python BSC runtime and the native Rust BSC runtime.

No old Phase-C fallback.

No compatibility bridge.

No old EVM SQLite schema as mandatory runtime state.

No second decision engine.

The clean source of truth is the Rust runtime plus pinned native chain adapters.
