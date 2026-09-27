# DarkBot BSC — Birth-Entry Architecture Contract

Status: **AUTHORITATIVE / NO ARCHITECTURE DRIFT**
Updated: 2026-09-27

## 1. Product objective

DarkBot BSC is designed to buy at the earliest technically usable moment of a new launch/token lifecycle — ideally from the first relevant pending transaction before canonical inclusion, not after a later candle/aggregated market signal.

The system objective is therefore:

1. see the relevant transaction as early as the BSC network exposes it;
2. recognize the launch/first actionable state immediately;
3. decide locally with no remote dependency;
4. build/sign immediately;
5. propagate the transaction back to the network as early as possible;
6. preserve exactly the same semantics in capture, replay, PAPER and LIVE.

The external lead objective is approximately:

```
EARLY_TX_SEEN - CANONICAL_REFERENCE <= -10 ms
```

where the real BSC peer topology exposes that lead.

No software can guarantee that the market always provides -10 ms. The architecture must guarantee that it does not deliberately add avoidable millisecond-scale software latency.

---

## 2. Mandatory components — MUST

These are required. They are not optional optimization ideas.

### 2.1 One own local BSC node

Run a pinned `bnb-chain/bsc` / `bsc-geth` node under our control on dedicated Linux hardware.

Reason:

- direct BSC P2P visibility;
- no provider queueing;
- no public WSS/JSON latency in the LIVE path;
- ability to instrument peer receive/decode/txpool stages;
- direct local transaction injection and peer propagation.

Public RPC/WSS may be used for secondary validation only, never as the primary LIVE signal path.

### 2.2 Minimal bsc-geth Fast-Lane patch

The pinned node gets only minimal hooks:

```
EARLY_TX_SEEN
TXPOOL_ACCEPTED
CANONICAL/REORG events
OWN_TX_ACCEPTED
FIRST_OUTBOUND_PEER_HANDOFF
```

No strategy, venue logic, economics or research logic goes into geth.

### 2.3 Shared-memory transport

Primary transport between geth and DarkBot:

```
geth -> mmap SPSC ring -> Rust
Rust -> mmap SPSC ring -> geth
```

No JSON, WebSocket, HTTP, RPC or Unix socket in the primary LIVE fast path.

### 2.4 One Rust runtime

All BSC runtime semantics live once in Rust:

- event decode;
- first-seen identity;
- sender/nonce replacement;
- canonical/reorg/orphan state;
- Flap decoder;
- Four.meme decoder;
- local market state;
- Strategy C;
- economics/quote;
- admission/risk;
- transaction build;
- secp256k1 sign;
- capture journal;
- replay;
- PAPER/shadow;
- LIVE.

There is no Python BSC trading engine.

There is no separate fast LIVE implementation.

### 2.5 Same code path in CAPTURE / REPLAY / PAPER / LIVE

Required:

```
same event schema
same Rust decoder
same Rust state
same Rust strategy
same Rust quote
same Rust builder
same Rust signer
```

Mode may change only external sources/sinks and key/config material.

PAPER still runs through build + sign. Only broadcast is suppressed.

### 2.6 Native binary capture

The primary 30-minute research capture is the exact binary event stream delivered to the Rust runtime.

```
native event
   +--> Rust runtime
   +--> immutable binary journal
```

Replay reads this exact binary journal and calls the same Rust event API.

SQLite/Parquet are derived offline products only.

### 2.7 Earliest actionable venue detection

For Flap and Four.meme, identify from verified on-chain evidence:

- deployment/manager addresses;
- relevant CREATE/launch selectors;
- BUY/SELL selectors;
- version/mode;
- token-address derivation if deterministic;
- initial buy/launch semantics;
- exact state needed to construct our entry.

Destination + selector filtering happens before full decode.

### 2.8 Pre-resident execution state

LIVE must not need a network roundtrip at decision time.

Keep resident in Rust:

- own account nonce;
- balances/reservations;
- current gas policy;
- current block/head context;
- venue state required for quote;
- relevant pending overlay/state;
- prebuilt transaction templates;
- warm signing context.

No synchronous `eth_call`, balance RPC, nonce RPC or metadata request in the hot path.

### 2.9 Dedicated target hardware

Final performance qualification uses dedicated Linux hardware.

Required to evaluate/freeze:

- dedicated physical Rust hot core;
- no competing SMT sibling;
- geth Fast-Lane work on separate core(s);
- recorder on separate core/process;
- CPU affinity;
- performance governor;
- IRQ/RSS/RPS/XPS placement;
- NUMA locality;
- no swap;
- prefault/mlock where justified;
- no unrelated noisy workloads.

### 2.10 Full latency instrumentation

Mandatory timestamps:

```
T-1  peer message decoded / EARLY_TX_SEEN
T0   source tx accepted locally
T1   inbound ring publish
T2   Rust receive
T3   venue decode complete
T4   strategy + quote complete
T5   own tx built
T6   own tx signed
T7   outbound ring publish
T8   own tx accepted by local geth
T9   first outbound peer handoff
TC   canonical reference
```

Report full distributions and maxima.

---

## 3. Hard engineering gates — MUST PASS

Internal target:

```
T-1 -> T8 < 0.5 ms
stretch   < 0.25 ms
>= 1 ms   investigate / gate failure until explained
> 3 ms reproducible path = RELEASE BLOCKER
```

External target:

```
T-1 - TC <= -10 ms
```

Report actual results even when the target is missed.

Do not redefine TC to improve the number.

---

## 4. Things that are NOT mandatory initially

These are explicitly not prerequisites for C0-C3/D/E/F:

- multiple BSC nodes;
- multiple geographic regions;
- own Rust devp2p client;
- validator node;
- sentry-node status;
- AF_XDP/XDP/DPDK;
- multiple broadcast nodes;
- direct validator relationships;
- custom NIC hardware timestamping;
- PREEMPT_RT;
- microservices;
- event bus;
- distributed database.

Do not build them preemptively.

---

## 5. Measurement-gated escalation

Only add complexity when the current simple topology proves where the missing lead is.

### 5.1 If peer quality is the bottleneck

Measure real first-seen by peer for the same transaction.

Then improve peer topology.

Rank peers by relevant full-transaction arrival, not ping alone.

### 5.2 If geography is the bottleneck

Add a small number of regional native sensors.

They contain no strategy.

They emit the same binary event contract.

Production uses one deterministic earliest-valid-event arbitration rule.

### 5.3 If bsc-geth ingress is the bottleneck

Add a dedicated Rust devp2p observer feeding the same Rust core.

Do not rewrite strategy/state/economics.

### 5.4 If outbound propagation is the bottleneck

Measure T8 -> T9.

Then evaluate peer topology and, if proven beneficial, broadcast the exact same signed raw transaction through multiple controlled BSC nodes.

### 5.5 If kernel/NIC latency is the bottleneck

Only then evaluate:

- busy polling;
- dedicated NIC queues;
- IRQ isolation;
- AF_XDP/XDP;
- hardware timestamping;
- further real-time kernel tuning.

---

## 6. Explicit forbidden architecture

Do not introduce:

- Python BSC runtime;
- public WSS as primary LIVE feed;
- JSON-RPC in LIVE decision/send path;
- separate replay/live decoder;
- separate PAPER strategy;
- separate quote implementation;
- separate transaction builder;
- synchronous remote quote;
- ORM/event bus/microservice framework;
- unbounded hot-path queues;
- old Phase-C EVM SQLite capture schema as runtime source of truth;
- old Pass-6 Python BSC/EVM implementation.

---

## 7. Phase order

```
A      existing Solana remains working
C-PY   retired; do not merge

C0     Rust event/wire/journal/replay foundation
C1     local bsc-geth early ingress + SPSC
C2     Rust BSC state/reorg/replacement
C3     Rust build/sign/native submit + T-1..T9 gate

D      Flap Rust decoder
E      Four.meme Rust decoder
F      30-min exact native capture

G      Rust economics/quote
H      Rust Strategy C + EV replay
I      Rust PAPER/shadow
J      Rust LIVE

K      only measured escalation
S0-S3  optional later Solana migration to the same Rust runtime
```

No phase may change the fundamental runtime architecture without explicit evidence that this contract is technically impossible.

---

## 8. Definition of done for the BSC architecture

The architecture is considered complete when:

- one own local pinned BSC node feeds the native event path;
- Rust receives earliest usable transaction evidence without JSON/WSS/RPC;
- the exact same Rust core runs replay/PAPER/LIVE;
- 30-minute binary captures replay deterministically;
- Flap/Four birth/launch detection is proven from real Mainnet evidence;
- quote/build/sign require no synchronous network call;
- T-1..T9 are measured;
- local runtime meets its latency release gates;
- real BSC lead relative to TC is known;
- additional topology is added only if measurements prove it is required.

This contract is the architectural stopping point. Future work optimizes measured bottlenecks without redesigning the trading core.
