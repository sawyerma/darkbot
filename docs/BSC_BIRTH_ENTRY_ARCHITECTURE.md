# DarkBot BSC — Birth-Entry Architecture Contract

Status: **AUTHORITATIVE / NO ARCHITECTURE DRIFT**
Updated: 2026-09-27

Normative feasibility gate:
- `docs/GATE0_TESTPLAN_SHORT.md`

## 1. Product objective

The product objective is not "submit quickly" and not "see pending early".

The objective is:

> obtain the earliest technically possible successful BUY once a new Flap/Four.meme coin becomes executable.

Authoritative definitions:

```
T_pre = earliest usable pre-tradability evidence
T0    = first execution point where the coin is actually buyable
T_buy = our successful BUY execution
```

Desired product target:

```
T_buy - T0 < 1 ms
```

On standard BSC chain data, sub-millisecond intra-block execution time is not directly observable per transaction. Therefore the required economic proof is actual execution ordering:

```
launch/activation at tx position N
our successful BUY at earliest valid position after N
successful competing BUYs between launch and ours = 0
```

A millisecond `T_buy-T0` value may only be claimed when a trusted execution trace supplies timestamps with sufficient fidelity.

## 2. What BSC may provide

BSC pending visibility can provide useful pre-execution evidence, but it is not guaranteed for every launch.

A launch may arrive through public P2P, a private RPC/builder path, or another path that our local node does not see before inclusion.

Therefore:

- an own local node is a primary ingress candidate, not a claim of complete visibility;
- pending coverage is measured per venue;
- no fixed `-10 ms` lead is assumed;
- a negative pre-lead is useful only when it is based on a valid timing reference.

The historical `-10 ms` value is retained only as a desired pre-computation direction, not as the product acceptance metric.

## 3. Mandatory Gate-0 architecture

Before the production runtime is built, Gate 0 must contain only enough native code to answer feasibility.

### 3.1 Current venue evidence

Freeze current Mainnet facts for Flap and Four.meme:

- deployed contract/manager address;
- implementation/version;
- launch selector;
- BUY selector;
- relevant modes;
- exact data needed to construct the first valid BUY;
- token-address derivation proof if deterministic;
- current provenance/date.

Unknown => unsupported. Never guess.

### 3.2 Earliest actionable ingress

Primary candidate:

```
BSC peer transaction
-> complete signed transaction decoded
-> Gate-0 Rust probe
```

A pinned local `bsc-geth` with an early full-transaction hook is preferred for controlled measurement.

Also measure whether relevant launches are absent from local pending visibility.

### 3.3 Minimal native hot path

```
T_pre
-> cheap destination/selector match
-> minimal verified venue decode
-> token/address derivation if proven
-> initial quote/input calculation
-> transaction template patch
-> secp256k1 sign
-> inclusion adapter
```

No DB, filesystem, DNS, metadata request, synchronous `eth_call`, HTTP quote or lazy initialization in this path.

### 3.4 Pre-resident state

Before the event arrives keep ready:

- own nonce state;
- balance/reservation needed by test;
- gas policy;
- head/chain context needed by the transaction;
- verified contract constants;
- transaction template/static calldata;
- warm secp256k1 signer;
- warmed inclusion connections.

Missing required verified state => do not send; never block on a remote fetch.

### 3.5 Inclusion is part of the feasibility gate

BSC PBS/builders are part of the real execution market and must not be postponed until late production LIVE.

Gate 0 compares at least:

- local geth/P2P path;
- current verified ordered-builder/backrun path(s).

Current evidence snapshot:
- BNB Chain PBS: https://docs.bnbchain.org/bnb-smart-chain/validator/mev/overview/
- BNB Chain builder selection guidance: https://docs.bnbchain.org/bnb-smart-chain/validator/mev/user-guide/
- 48Club target-prepend behavior: https://docs.48.club/puissant-builder/auction-transaction-feed
- BlockRazor current support/docs: https://www.blockrazor.io/support/

A builder is never selected on marketing claims. Actual first-BUY ordering decides.

External builder access is not assumed to be permanent. If Gate 0 classifies external builder access as BENEFICIAL or REQUIRED, the roadmap activates an own-builder option.

That option is accepted only if validator/proposer reach, practical coverage, operating cost and measured inclusion quality are competitive enough. The ability to run builder software alone is not sufficient.

## 4. Gate-0 latency evidence

Record local timestamps such as:

```
T_pre
T_decode_done
T_build_done
T_sign_done
T_send
```

Local engineering target:

```
T_pre -> T_send < 1 ms target
1-3 ms = investigate/adapt
>3 ms = fail current local path
~10 ms+ = stop current local architecture
```

This local timing is diagnostic and controllable. It is not the same as real BUY execution.

## 5. Gate-0 economic evidence

For every controlled real validation record:

```
launch_tx_hash
launch_tx_index
buy_tx_hash
buy_tx_index
receipt_status
same_block
successful_buys_between
entry_price
inclusion_path
```

Primary economic success:

```
successful BUY
+ earliest technically valid execution position
+ no successful competing BUY between launch and ours
```

## 6. Production architecture after Gate-0 PASS

Only after PASS build the full native runtime:

```
one pinned bsc-geth
+ native Fast-Lane hooks
+ Rust runtime
+ native recorder/replay
+ venue modules
+ economics
+ strategy
+ PAPER/LIVE
+ measured inclusion adapters
```

Go/bsc-geth owns chain-client duties only.

Rust owns:

- native event model;
- pending/replacement/canonical state;
- Flap/Four factual decode;
- economics;
- strategy;
- build/sign;
- capture/replay/PAPER/LIVE;
- inclusion selection and telemetry.

Python is offline analysis/reporting only.

## 7. What is NOT required before Gate-0 PASS

Do not prebuild:

- complete reorg/restart state machine;
- full binary research journal;
- 30-minute capture infrastructure;
- Strategy C;
- EV sweeps;
- PAPER architecture;
- GUI;
- multi-region sensors;
- custom Rust devp2p;
- AF_XDP/XDP/DPDK;
- hardware timestamping;
- full sell/exit engine.

They do not answer the first feasibility question.

## 8. Production escalation after PASS

After Gate 0, add complexity only for a measured bottleneck:

1. peer-path optimization;
2. builder/inclusion path optimization;
3. regional sensors if geography dominates;
4. direct Rust devp2p if geth ingress dominates;
5. kernel/NIC specialization if local kernel/NIC tails dominate.

The exact same strategy/economics/build semantics remain single-source.

## 9. Explicit forbidden claims

Do not claim any of the following without measurement:

- "BSC always gives -10 ms";
- "local pending sees every launch";
- "local geth acceptance means we bought";
- "builder ACK means we bought";
- "same block means <1 ms";
- "canonical block timestamp gives exact T0";
- "Four.meme token address is knowable before execution" unless current contract evidence proves it;
- "one builder is fastest" before the same-condition race.

## 10. Definition of success

BSC proceeds beyond Gate 0 only when at least one venue demonstrates:

- enough pre-execution information to build a valid earliest BUY;
- local build/sign/send path within the required engineering budget;
- a credible inclusion mechanism;
- controlled Mainnet evidence of the earliest technically possible successful BUY placement with acceptable repeatability.

Otherwise the venue is ADAPT or STOP.
