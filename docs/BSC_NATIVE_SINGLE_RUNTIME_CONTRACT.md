# DarkBot BSC — Native Single-Runtime Contract

Status: **NORMATIVE / MANDATORY BEFORE PHASE D**
Updated: 2026-09-27
Scope: BSC runtime architecture from native pending observation through replay/PAPER/LIVE execution.

## 1. Non-negotiable objective

DarkBot BSC has one runtime implementation for latency-relevant behavior.

There is no Python trading engine and no separate test trading engine.

The normative runtime is Rust. The exact same Rust code must execute venue recognition/decoding, state transitions, strategy, local economics/quote, admission, transaction construction, signing and latency accounting in replay, PAPER and LIVE.

Mode may change only external sources/sinks and credentials. Mode must not select a different decoder, strategy, quote model, transaction builder or execution semantics.

Python remains allowed for offline analysis, SQLite/Parquet reporting, plots, parameter search, evidence inspection and as historical Phase-C reference material. Python must never become a second source of truth for BSC trading decisions.

## 2. Latency objective

The project is economically latency-critical.

Measured market objective:

```
T_NATIVE_PENDING - T_CANONICAL_REFERENCE <= -10 ms
```

Meaning: pursue at least 10 ms pre-canonical lead where the BSC network/venue makes it physically available.

The runtime must preserve as much of that lead as technically possible.

Native internal objective:

```
T0 = bsc-geth native new-tx event
T8 = signed transaction accepted by local bsc-geth txpool

engineering target: T0 -> T8 < 0.5 ms
stretch target:     T0 -> T8 < 0.25 ms
warning:            T0 -> T8 >= 1.0 ms
economic hard fail: T0 -> T8 > 3.0 ms
```

The 3 ms value is not a percentile comfort target. Any reproducible runtime path above it is a release blocker because the measured strategy edge can disappear at millisecond scale.

Percentiles are still reported, but p50/p99 must never be used to hide maxima/outliers.

## 3. Mandatory topology

```
BSC P2P
   |
   v
local bsc-geth
   |
   | native txpool/new-tx hook
   v
binary FastLaneEvent
   |
   v
bounded SPSC/shared-memory ring
   |
   v
+--------------------------------------+
|            SAME RUST CORE            |
| recognize/decode                     |
| state/replacement                    |
| strategy                             |
| economics/quote                      |
| admission/risk                       |
| tx build                             |
| secp256k1 sign                       |
+------------------+-------------------+
                   |
        same signed-tx object
                   |
        +----------+----------+
        |          |          |
      REPLAY     PAPER       LIVE
       sink       sink        sink
        |          |          |
     result      no-send   binary submit ring
                              |
                              v
                         local bsc-geth
                              |
                              v
                            BSC P2P
```

No WSS, JSON-RPC, HTTP, DNS, filesystem or SQLite call is allowed in the latency-critical LIVE path.

A secondary WSS/RPC path may exist only for evidence comparison, canonical recovery or validation outside the trading hot path. It can never be silently substituted for the native latency source.

## 4. Exact single-core rule

The following must exist exactly once in production semantics:

- Flap recognizer and decoder;
- Four.meme recognizer and decoder;
- replacement/nonce state used by decisions;
- Strategy C decision logic;
- economics/quote math;
- admission/risk rules;
- transaction builder;
- signer integration;
- execution-intent model.

Replay, PAPER and LIVE must call the same Rust functions/crates.

Forbidden:

- Python decoder used for research while Rust decoder is used for LIVE;
- Python Strategy C plus Rust Strategy C;
- special "fast live" strategy implementation;
- simplified replay quote model;
- PAPER-only builder;
- test-only semantics that skip build/sign;
- different venue state machines by mode.

## 5. Allowed boundary differences

Only boundary adapters may differ.

### Input

LIVE:
`bsc-geth native event -> FastLaneEvent`

REPLAY:
`captured FastLaneEvent bytes -> same Rust Engine::process_event`

PAPER/shadow:
live native event or captured event -> same Rust Engine::process_event.

### Output

LIVE:
signed RLP -> local bsc-geth binary submit hook.

PAPER:
the exact signed output object goes to a non-broadcast sink.

REPLAY:
the exact built/signed output object is recorded/computed but not broadcast.

Signing code is identical. Key material may differ by mode for security; that is configuration, not a separate signing implementation.

### Clock

LIVE uses the monotonic production clock.
Replay injects recorded event timestamps through the same clock interface.

Clock source may differ. State/strategy semantics may not.

## 6. Capture/replay identity

The primary 30-minute BSC capture must record the exact ordered binary FastLaneEvent stream that is delivered to the Rust core.

Each event has at minimum:

- format_version;
- monotonic sequence id;
- chain id;
- source id;
- native first-seen timestamp;
- tx hash;
- raw signed transaction bytes or lossless equivalent native fields;
- destination;
- nonce;
- value;
- gas fields;
- calldata;
- canonical/head evidence events required for deterministic state reconstruction;
- quality flags.

The recorder is a fan-out from the same event boundary:

```
                  +--> Rust core
FastLaneEvent ----+
                  +--> bounded capture ring -> recorder
```

The recorder is never allowed to alter the event before the Rust core sees it.

A capture overflow invalidates the affected research interval explicitly. It must not block LIVE execution.

Replay must feed the recorded events back into the same Rust event API in the same order. No Python normalization layer may sit between capture bytes and the Rust replay engine.

## 7. Binary capture is primary evidence

For BSC latency/strategy replay the primary artifact is a versioned append-only binary event journal plus manifest.

SQLite/Parquet are derived analysis products.

The original binary journal is immutable and hash-addressed.

This prevents a slow analysis schema from becoming the trading/replay source of truth.

## 8. Rust workspace — keep it small

Prefer a small workspace, not a framework:

```
native/
  Cargo.toml
  crates/
    darkbot-core/
      src/
        event.rs
        state.rs
        venues/
          flap.rs
          fourmeme.rs
        economics/
        strategy/
        tx.rs
        signer.rs
    darkbot-fastlane/
      src/
        wire.rs
        ring.rs
    darkbot-runner/
      src/
        main.rs
        replay.rs
        paper.rs
        live.rs
```

The runner modes are boundary composition only. They may not duplicate core logic.

The bsc-geth hook remains a minimal isolated patch/package on the Go side.

## 9. Hot-path engineering rules

The hot thread must be designed for determinism:

- preallocated/fixed-capacity structures where practical;
- no heap allocation on the common decision path after warm-up where practical;
- no unbounded collections;
- no mutex/condvar on the SPSC common path;
- no async runtime scheduling on the critical decision thread unless measurement proves it superior;
- no logging formatting in the hot path;
- no database access;
- no synchronous RPC;
- no ABI reflection/general-purpose dynamic decoder when fixed verified layouts suffice;
- destination + selector first;
- all required quote/strategy state resident in memory;
- warm secp256k1 context;
- persistent native submit path;
- explicit overflow/fail-closed semantics.

## 10. Target host

Latency acceptance must be measured on dedicated Linux target hardware, not inferred from a shared development host.

Target configuration should evaluate:

- dedicated physical core for Rust hot thread;
- no competing SMT sibling;
- CPU affinity/isolation;
- performance governor;
- IRQ/RPS/XPS placement away from hot core;
- no swap;
- prefault/mlock where justified;
- recorder/writer on other cores/processes;
- suitable real-time scheduling only after controlled validation;
- local bsc-geth on the same host or demonstrably lower-latency topology.

Do not claim an absolute latency property from macOS or a shared VPS benchmark.

## 11. Mandatory timestamps

Use one monotonic clock domain on the host where possible.

At minimum:

```
T0 native geth new-tx event
T1 FastLaneEvent published
T2 Rust event consumed
T3 venue decode complete
T4 strategy/quote decision complete
T5 transaction built
T6 signature complete
T7 submit ring published
T8 local geth txpool accepted
T9 first outbound P2P write/hand-off when measurable
TC canonical reference observation
```

Report both:

`internal_latency = T8 - T0`

and:

`effective_submit_lead = T8 - TC`

The project objective is to drive effective submit lead toward or beyond **-10 ms** where native BSC visibility allows it.

## 12. Native Runtime Gate before Phase D

Before Flap/Four production decoding begins, build only enough native infrastructure to prove the architecture:

1. local bsc-geth native pending hook;
2. versioned FastLaneEvent wire format;
3. bounded SPSC/shared-memory transport;
4. Rust runner consuming events;
5. binary submit path back to local bsc-geth;
6. dummy deterministic build/sign/submit round-trip benchmark;
7. binary capture + replay of the same events;
8. latency timestamps T0-T9.

No venue strategy is required for this gate.

Acceptance target:

- correct lossless event identity;
- replay reads the exact same wire format as LIVE;
- bounded state/queues;
- no JSON/WSS/RPC in T0-T8;
- target T0-T8 < 0.5 ms under controlled load;
- every >1 ms sample investigated;
- any reproducible >3 ms code/runtime path blocks progression;
- target-host results reported with p50/p95/p99/p99.9/p99.99/max and exact >1 ms/>3 ms counts.

## 13. Compatibility with Phase C

Phase C remains useful and is not discarded.

Its Python implementation is retained as:

- evidence/canonical correctness reference;
- historical test oracle;
- capture/reorg/provenance research;
- offline comparison during migration.

It does not remain the BSC LIVE trading runtime.

Where Rust reproduces a Phase-C semantic rule, cross-language fixtures must prove the Rust result. After the Rust core becomes normative, future BSC runtime behavior is defined by Rust plus frozen fixtures/contracts.

## 14. Release rule

No BSC LIVE release is accepted unless:

- the same Rust core has passed capture replay, PAPER/shadow and LIVE composition tests;
- the exact code hashes/config/economics/strategy snapshot are bound to the run;
- capture/replay parity is proven;
- no mode-specific trading implementation exists;
- the measured native lead and T0-T8 latency are reported honestly;
- the hard economic latency ceiling is respected on target hardware.

If the market no longer offers sufficient pre-canonical lead, report that result. Do not hide it by changing the latency reference.
