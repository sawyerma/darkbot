# DarkBot BSC — Birth-Entry Architecture Contract

Status: **AUTHORITATIVE / NO ARCHITECTURE DRIFT**
Updated: 2026-09-27

Normative feasibility gate:
- `docs/bsc/GATE0_TESTPLAN_SHORT.md`

## 0. Direct-chain invariant

DarkBot's BUY execution is **contract-native and direct on-chain**.

```
chain/pending evidence
-> local verified venue decode
-> local BUY construction
-> local signing
-> inclusion adapter
-> BSC contract execution
```

The Four.meme/Flap website, browser, UI, frontend trading workflow and frontend API are not part of this path and must never become runtime BUY dependencies.

Frontend state does not determine whether a token is buyable. Readiness is determined only from verified contract semantics and the chain/pending facts needed to construct a valid transaction, plus DarkBot's own admission/risk decision.

A local node or builder is only an inclusion mechanism. The economic action remains the signed transaction executing directly against the verified venue contract.

Where an ordered launch->BUY bundle is supported and proven, DarkBot may prepare and submit that bundle before `T0`; the BUY still executes only in the valid position after activation. Where some mandatory BUY input becomes known only at/after activation, DarkBot reacts at the earliest chain-visible point. There is never a reason to wait for website listing, page refresh or an enabled frontend BUY button.

## 1. Product objective

The product objective is not "submit quickly" and not "see pending early".

The objective is:

> obtain the earliest technically possible successful external BUY once a new Flap/Four.meme coin becomes executable.

If the launch transaction itself contains a creator/presale/initial BUY, that launch-internal acquisition is not an externally overtakeable BUY. The target is then the first technically possible external successful BUY after the launch-internal state transition.

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

Freeze current Mainnet facts for Flap, Four.Meme Classic and Four.Meme OpenFour, then select one exact `PRIMARY_GATE0_SCOPE = venue family + deployment/version + launch mode + BUY selector/mode + quote mode` for the first complete proof:

- deployed contract/manager/core/registry address as applicable;
- implementation/version;
- launch selector;
- BUY selector;
- relevant modes;
- exact data needed to construct the first valid BUY;
- token-address derivation proof if deterministic;
- current provenance/date.

Four Classic and OpenFour are distinct protocol families; do not reuse one family’s decoder, address-derivation or BUY assumptions for the other without proof.

Unknown => unsupported. Never guess.

### 3.2 Earliest actionable ingress

Ingress is not assumed to be a single source. Compare complete candidate sources early enough to falsify weak coverage.

Primary candidate:

```
BSC peer transaction
-> complete signed transaction decoded
-> Gate-0 Rust probe
```

A pinned local `bsc-geth` with an early full-transaction hook is preferred for controlled measurement.

Also measure whether relevant launches are absent from local pending visibility. Where independently verified, compare alternate early ingress such as the current 48Club Auction Feed if it exposes enough attributable simulated logs/state for the selected scope. Early arrival alone is not enough; actionable field completeness and real coverage decide.

### 3.3 Preparation matrix and minimal native path

Before performance tuning, create the Gate-0 BUY-field preparation matrix for the selected scope. For every signature-bound or BUY-validity field record source, earliest availability, precomputable portion, hot-path residual and invalidation condition. Static bytes/state are prepared before the actionable signal; pre-signing is allowed only when all signature-bound fields are final and remain valid.

Then prove the smallest exact semantic path first:

```
selected launch mode/state
-> exact BUY inputs
-> exact transaction
-> valid launch -> BUY semantics
-> expected acquisition
```

Only after this correctness proof optimize the local path:

```
T_pre
-> cheap destination/selector match
-> minimal verified venue decode
-> patch/finalize only proven dynamic BUY fields
-> local quote only if unavoidable
-> transaction finalize
-> secp256k1 sign
-> serialization/adapter queue
-> inclusion-adapter handoff
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

### 3.5 Ingress × inclusion is part of the feasibility gate

BSC PBS/builders are part of the real execution market and must not be postponed until late production LIVE.

Gate 0 compares complete `ingress source × inclusion path` combinations. At minimum compare local geth/P2P ingress and any independently verified alternate early ingress that exposes sufficient BUY facts, against local geth/P2P submission and current verified ordered-builder/backrun path(s). Actual successful acquisition/order result ranks the combination.

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
SUCCESSFUL_BUY = intended token actually acquired by intended recipient
               + positive received token amount
               + attributable debit/cost evidence
               + verified earliest technically valid external execution position
               + no successful competing external BUY between launch and ours
```

Receipt success alone is insufficient. Launch-internal creator/presale/initial purchases are recorded separately and do not count as externally overtakeable competitors.

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

BSC proceeds beyond Gate 0 only when one exact `PRIMARY_GATE0_SCOPE` demonstrates:

- enough pre-execution information to build a valid earliest BUY;
- local build/sign/send path within the required engineering budget;
- a credible inclusion mechanism;
- controlled Mainnet evidence of the earliest technically possible successful external BUY placement with repeatability measured against the numeric Gate-0 acceptance contract frozen before value-bearing trials.

PASS applies only to that exact venue/deployment/version/launch mode/BUY mode. Untested scopes remain unproven. Otherwise the scope is ADAPT or STOP.
