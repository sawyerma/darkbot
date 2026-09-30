# DarkBot BSC — Maximum-Performance Native Single-Runtime Contract

Status: **NORMATIVE AFTER GATE-0 PASS**
Updated: 2026-09-27

Higher-priority feasibility documents:
1. `docs/GATE0_TESTPLAN_SHORT.md`
2. `docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md`
3. `docs/BSC_DUAL_VENUE_ROADMAP.md`

This production contract becomes active only after at least one BSC venue passes Gate 0.

## 0. Production invariant — no trading frontend

Production BUY execution is direct on-chain only:

```
native event
-> Rust venue/economics/admission
-> local transaction/bundle construction
-> local signing
-> Gate-0-proven inclusion adapter
-> venue contract execution
```

Four.meme/Flap websites, browser automation, frontend trading APIs and UI state are forbidden production BUY dependencies.

No component may wait for "listed on website", page refresh, frontend button enablement or frontend API publication when the verified chain/pending data required for a valid BUY is already available.

Local bsc-geth/P2P and external/own builders are transport/inclusion adapters only. They receive DarkBot's already-determined transaction/bundle intent; they are not quote, strategy, venue or readiness authorities.

## 1. Objective

Build one native BSC runtime that preserves the Gate-0-proven first-BUY capability while adding correctness, replayability, strategy and operational safety.

The product objective remains:

> earliest technically possible successful BUY after a supported launch becomes executable.

Fast local processing is necessary but is not itself proof of purchase.

## 2. Ownership

### Go / pinned bsc-geth

Own only chain-client duties:

- BSC devp2p/network protocol;
- canonical chain import;
- txpool validation/admission;
- peer management;
- minimal timestamped ingress hooks;
- minimal local signed-transaction injection/broadcast hook.

No venue strategy/economics/research logic in geth.

### Rust / DarkBot

Own all runtime semantics:

- native event model;
- pending identity;
- sender/nonce competition/replacement;
- canonical/reorg/orphan state;
- Flap/Four factual decode;
- market/runtime state;
- economics/local quote math;
- Strategy C;
- admission/risk;
- transaction construction;
- secp256k1 signing;
- inclusion adapter selection;
- latency telemetry;
- binary capture journal;
- replay;
- PAPER/shadow;
- LIVE.

### Python

Offline analysis/control only:

- EV/statistical analysis;
- parameter sweeps;
- plots/reports;
- SQLite/Parquet derivation;
- notebooks.

Python never determines a BSC runtime trade.

## 3. Ingress

Use native event classes such as:

### EARLY_TX_SEEN

Timestamp a complete relevant signed transaction as early as safely practical after peer-message decode.

Purpose:

- preserve actionable pending evidence;
- cheap destination/selector filtering;
- precompute deterministic launch/BUY material before txpool acceptance where valid.

### TXPOOL_ACCEPTED

Timestamp local txpool acceptance of the source transaction.

Purpose:

- local validation evidence;
- diagnostics for pending lifecycle.

Neither event proves that the token is already buyable.

Pending visibility is incomplete by nature; record launches not seen pending.

## 4. Production egress/inclusion

The production runtime supports more than one inclusion adapter behind one exact signed transaction/bundle intent.

Candidate adapters may include:

- local bsc-geth/P2P;
- Gate-0-proven ordered builder/backrun path;
- another currently verified builder path.

A local geth acceptance or builder ACK is **not** a successful BUY.

The production-selected inclusion adapter must have passed/repassed actual ordering/inclusion validation.

## 5. Shared-memory Fast Lane

Default local IPC:

```
geth -> mmap SPSC -> Rust
Rust -> mmap SPSC -> local geth adapter
```

Requirements:

- one producer / one consumer per ring;
- fixed-capacity/preallocated structures;
- sequence numbers;
- no common-path locks;
- no silent overwrite;
- explicit overflow evidence;
- no JSON wire format.

Builder adapters may use their required external transport; this does not change Rust decision/build semantics.

## 6. Same Rust core in all modes

These semantics exist once:

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

Replay/PAPER/LIVE reuse them.

Mode-specific differences are limited to external source/sink, credentials and test/live key material.

Forbidden:

- replay-only decoder;
- PAPER-only quote;
- LIVE-only strategy;
- alternate fast signer;
- Python decision oracle.

## 7. Native capture/replay

Production evidence is an immutable versioned binary journal.

Required:

- append-only;
- sequence preserving;
- checksummed/hash-manifested;
- exact native timestamps;
- raw signed tx bytes or lossless fields;
- canonical/reorg events;
- explicit gaps/overflow.

SQLite/Parquet are derived offline only.

## 8. Production state

Rust owns:

- tx identity/first-seen;
- sender+nonce candidates;
- replacement resolution;
- included/reverted;
- canonical/orphan/uncertain;
- bounded reorg ancestry;
- restart/recovery state needed for operational parity;
- explicit provenance/gaps.

Relevant retired Python Phase-C counterexamples become Rust tests, not runtime dependencies.

## 9. Hot-path rules

Production common path:

```
native event
-> cheap venue reject
-> verified fixed decode
-> state/decision/quote
-> build/sign
-> selected inclusion adapter
```

Forbidden on the latency-critical decision path:

- filesystem/DB;
- DNS;
- blocking RPC/HTTP/WSS lookup;
- metadata lookup;
- dynamic ABI download/reflection;
- JSON persistence;
- synchronous logging;
- blocking locks;
- lazy initialization;
- avoidable heap allocation/page faults;
- synchronous external quote/helper call.

Required state is prepared before the event. Missing/stale required state => fail closed.

## 10. Build/sign preparation

Benchmark and retain only beneficial settings:

- release `opt-level=3`;
- LTO;
- codegen-units=1 where beneficial;
- `panic=abort`;
- target CPU tuning where deployment permits;
- PGO only after representative production capture exists.

Signer context is warm before trading.

## 11. Target host

Final production latency qualification uses dedicated Linux hardware.

Evaluate:

- isolated hot physical core;
- no competing SMT sibling;
- fixed affinity;
- geth fast-lane work on separate core(s);
- recorder/analysis off hot core;
- NIC IRQ/RSS/RPS/XPS placement;
- performance governor;
- no swap;
- prefault/mlock where justified;
- NUMA/NIC locality.

A shared VPS/macOS benchmark cannot establish the production latency gate.

## 12. Timing model

Use names that do not confuse local engineering time with actual economic execution:

```
L0 = EARLY_TX_SEEN
L1 = Fast Lane publish
L2 = Rust receive
L3 = venue decode complete
L4 = decision + local quote complete
L5 = transaction/bundle built
L6 = signed
L7 = inclusion-adapter handoff
L8 = local geth acceptance, if local path
L9 = first outbound peer handoff, if measurable

C0 = source launch canonical inclusion/order evidence
B0 = our BUY canonical inclusion/order evidence
```

Report local durations:

```
local_ready = L6 - L0
local_send  = L7 - L0
```

`local_send` is the authoritative local BUY-path latency boundary. Any post-sign serialization, queueing or adapter preparation performed before L7 is part of the measured latency and may not be excluded.

Record canonical ordering:

```
launch_tx_index
buy_tx_index
successful_competing_buys_between
same_block
receipt_status
```

Do **not** calculate an authoritative sub-ms `T_buy-T0` from ordinary block timestamps.

## 13. Engineering targets

Local production target:

```
EARLY_TX_SEEN -> inclusion-adapter handoff < 0.5 ms preferred
< 1.0 ms                               required target band
1-3 ms                                 investigate/adapt
> 3 ms reproducible                    release blocker
```

These are local engineering targets.

Economic success remains the actual execution placement proven by Gate 0 and revalidated in LIVE.

Historical `-10 ms` pre-lead is not a production PASS gate. Negative actionable lead may still be measured as diagnostic evidence when the timing reference is valid.

## 14. Inclusion-path invariant

Do not hide inclusion behind a late optional optimization phase.

The production runtime must expose the Gate-0-proven inclusion mechanism from the beginning.

If an ordered builder/backrun path won Gate 0, it is a production requirement, not a Phase-K experiment.

Local geth/P2P may remain as:
- baseline;
- fallback only if its semantics are explicitly safe;
- diagnostic comparison.

## 15. Optional escalation after PASS

Only for measured remaining bottlenecks:

### Level 1 — peer-path optimization
Rank peers by relevant full-transaction first-seen performance, not ping.

### Level 2 — inclusion/builder optimization
Optimize or replace the inclusion provider only from measured execution-placement evidence.

### Level 3 — geographic sensors
Only if geography dominates ingress. Sensors contain no strategy/economics.

### Level 4 — direct Rust devp2p
Only if geth ingress itself consumes material lead.

### Level 5 — kernel/NIC specialization
Only if local kernel/NIC tails dominate:
- dedicated NIC queues;
- busy polling;
- IRQ isolation;
- AF_XDP/XDP if measured beneficial;
- hardware timestamping where useful.

No escalation is kept without measured benefit.

## 16. Rust workspace target

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
      inclusion/
    darkbot-capture/
    darkbot-runner/
      replay/
      paper/
      live/
```

Do not create a generic framework for hypothetical chains.

## 17. Solana relationship

Existing Solana/Pump.fun remains working while BSC production is proven.

A later Solana convergence may reuse the Rust core only after strict parity/forward validation.

Do not rewrite Solana merely for architecture symmetry.

## 18. Acceptance principle

Production architecture is accepted only by measurement and correctness evidence.

No statement such as:
- "Rust is fast";
- "geth accepted it";
- "builder acknowledged it";
- "same block";
- "we had -10 ms canonical lead"

is sufficient proof of the first BUY.

The decisive economic evidence is actual acquisition of the intended token by the intended recipient, with positive received amount and attributable debit/cost, at the earliest technically valid external BUY position relative to the launch and competing external BUYs. Receipt success alone is insufficient.

Before production LIVE, the final D/E/G/H-complete runtime must requalify the full `event -> decoder/state/economics/strategy -> build/sign -> serialization/adapter handoff` path against the same local latency boundary used by Gate 0, including concurrent-launch/nonce stress.
