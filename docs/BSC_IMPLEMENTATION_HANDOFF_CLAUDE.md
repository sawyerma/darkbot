# DarkBot BSC — Current Implementation Handoff

Status: **CURRENT / EXECUTE ONLY FROM THIS ORDER**
Updated: 2026-09-27

## Source-of-truth order

1. `docs/GATE0_TESTPLAN_SHORT.md`
2. `docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md`
3. `docs/BSC_DUAL_VENUE_ROADMAP.md`
4. `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`
5. active phase-specific contract
6. `docs/BSC_PHASE_C_PYTHON_RETIREMENT.md` for retirement/history rules only

The Git history preserves older handoff versions. Do not recover old phase order from them.

## Current objective

The product target is:

> earliest technically possible successful BUY after a supported new coin becomes executable.

Do not substitute:
- pending lead;
- decode latency;
- sign latency;
- local geth acceptance;
- builder ACK;
- block/finality time

for actual BUY execution placement.

## Current timing model

```
T_pre = earliest usable pre-tradability evidence
T0    = first execution point where the coin is actually buyable
T_buy = our successful BUY execution
```

Desired product target:

```
T_buy - T0 < 1 ms
```

On ordinary BSC block/receipt data, do not invent a sub-ms intra-block value.

Required economic proof:

```
launch/activation = tx position N
our successful BUY = earliest technically valid position after N
successful competing BUYs between = 0
```

## Current work order

```
A       existing Solana/Phase-A seam stays

C-PY    retired / do not merge

G0.1    current Flap/Four evidence freeze
G0.2    prove token/address + BUY inputs before T0
G0.3    minimal Rust decode/build/sign local benchmark
G0.4    pending coverage / unseen-private launch measurement
G0.5    inclusion race: local P2P + current verified ordered builder paths
G0.6    prove ordered launch -> BUY validity
G0.7    controlled minimal-value Mainnet validation
G0.8    inspect actual execution ordering / competing BUYs / receipt / price
G0.9    PASS / ADAPT / STOP

Only after G0 PASS:
C0      production Rust foundation
C1      production native BSC ingress
C2      production state/reorg/replacement
C3      production local execution engineering gate
D       full Flap production decoder
E       full Four.meme production decoder
F       native capture / coverage / replay evidence
G       complete production economics
H       Strategy C + EV
I       PAPER/shadow
J       production LIVE using already-proven inclusion path
K       measured escalation only
```

## Gate-0 implementation rule

Gate 0 is deliberately smaller than production.

Allowed:
- minimal Rust harness;
- minimal current Flap/Four factual probe;
- exact first-BUY construction;
- minimal exact economics needed for validity;
- warmed nonce/gas/head/signer state;
- inclusion adapters required for the race;
- result logging outside the hot path.

Not required before PASS:
- full replay journal architecture;
- complete reorg/restart engine;
- Strategy C;
- full economics model;
- 30-minute capture;
- PAPER;
- GUI;
- multi-region;
- direct Rust devp2p;
- AF_XDP/DPDK.

## BSC inclusion rule

Builder/bundle testing is part of Gate 0, not a late research phase.

At execution time reverify current BSC PBS/builder APIs and compare under identical conditions.

Candidate classes:
- local bsc-geth/P2P;
- 48Club/Puissant ordered/backrun-target path if still supported;
- BlockRazor ordered/bundle path if still supported;
- another verified current path if justified.

Selection is based on successful first-BUY placement, not ACK latency.

## Venue rule

### Flap

Gate 0 must prove:
- current deployment/version;
- launch semantics;
- token-address derivation before T0 if claimed;
- exact first-BUY path.

### Four.meme

Gate 0 must prove:
- current manager/version;
- create/BUY semantics;
- whether token address and mandatory BUY inputs are knowable before T0.

Never infer pre-execution address availability merely from a later TokenCreate event.

## Local engineering target

For the controllable Gate-0 hot path:

```
<1 ms    target
1-3 ms   ADAPT only with a measured removable bottleneck
>3 ms    current local path FAIL
~10 ms+  STOP current local architecture
```

This local metric is not the purchase result.

## Stop rule

If both Four.meme and Flap return STOP after credible ingress and inclusion paths are tested, stop the BSC Birth-Sniping production roadmap.

If one venue passes, promote that venue first. Do not build the failing venue merely for symmetry.

## Repository cleanliness

Do not merge the retired Python Phase-C BSC/EVM runtime.

No duplicate Python/Rust BSC trading engine.
No duplicate decoder/strategy/quote implementation.
No old EVM SQLite runtime dependency.
No remote quote/RPC lookup on the production decision hot path.

## Reporting after every phase

Report:
- exact phase;
- files changed;
- assumptions verified;
- assumptions still unknown;
- commands/tests;
- raw latency samples where relevant;
- actual ordering evidence where relevant;
- PASS / ADAPT / STOP;
- exact commit SHA.

Do not continue automatically to the next phase without review.
