# Gate 0 — BSC/Solana Birth-to-BUY Feasibility Gate

Status: **AUTHORITATIVE / FIRST HARD GATE**
Updated: 2026-09-27

## 1. Product question

Before any further low-latency production build, answer one question:

> When a new coin becomes buyable for the first time, can DarkBot obtain the earliest technically possible successful BUY?

Test objects:
- Solana / Pump.fun
- BSC / Four.meme
- BSC / Flap

Gate 0 is a minimal feasibility harness, not the production bot.

## 2. Authoritative timing definitions

```
T_pre   = earliest usable evidence before the coin becomes buyable
T0      = first execution point at which the coin is actually buyable
T_ready = our exact BUY transaction/bundle is fully built and signed
T_send  = our exact BUY transaction/bundle is handed to the tested inclusion path
T_buy   = our BUY actually executes successfully
```

Desired product target:

```
T_buy - T0 < 1 ms
```

Important measurement rule:

Standard BSC receipts/blocks expose transaction ordering, not a trusted sub-millisecond execution timestamp for each transaction. Therefore Gate 0 MUST NOT invent a numeric `T_buy-T0` value from ordinary block timestamps.

For BSC the hard economic proof is therefore:

```
launch/activation transaction = position N
our successful BUY            = earliest valid position after N
successful competing BUYs between launch and ours = 0
```

If a trusted builder/validator trace later exposes intra-block execution timestamps, record the direct `T_buy-T0` value as additional evidence.

## 3. Pre-lead

```
pre_lead = T_pre - T0
```

The desired design direction is negative lead, historically around `-10 ms` or better.

This is **not an assumed fact** and is **not a Gate-0 PASS criterion by itself**.

Do not derive an authoritative millisecond pre-lead from ordinary canonical block observation. A numeric pre-lead is valid only when the reference clock actually represents the first executable point with sufficient timing fidelity.

## 4. What Gate 0 must prove on BSC

For Four.meme and Flap separately:

1. current Mainnet contracts/versions/selectors are frozen from verified evidence;
2. the launch transaction can be recognized before or at first executability;
3. determine whether the future token address and all mandatory BUY inputs can be derived before `T0`;
4. exact initial BUY calldata/value/gas/fee requirements can be prepared without a synchronous hot-path RPC;
5. build + sign stays comfortably below the 1 ms local engineering budget;
6. measure how often the launch is actually visible pending before execution;
7. compare real inclusion paths, including ordinary local-node/P2P and current ordered-builder/bundle paths;
8. perform controlled minimal-value Mainnet validation only after dry qualification;
9. verify actual on-chain ordering and successful receipt;
10. classify PASS / ADAPT / STOP.

## 5. BSC inclusion paths are part of Gate 0

Do not postpone builder/bundle testing to a late LIVE phase.

BSC uses proposer-builder separation and builders accept searcher transactions/bundles. Current official/reference evidence must be rechecked at execution time.

Minimum candidate comparison:

- own local bsc-geth / P2P submission;
- 48Club/Puissant ordered or backrun-target bundle path where currently supported;
- BlockRazor ordered/bundle path where currently supported;
- another current BSC builder only if independently verified.

48Club currently documents that when a bundle is submitted with `backrunTarget`, the target transaction is automatically prepended ahead of the submitted bundle transactions. This is directly relevant to testing:

```
[target launch]
[our BUY]
```

Source snapshot:
- BNB Chain PBS/MEV overview: https://docs.bnbchain.org/bnb-smart-chain/validator/mev/overview/
- BNB Chain user guide: https://docs.bnbchain.org/bnb-smart-chain/validator/mev/user-guide/
- 48Club Auction Feed / backrunTarget: https://docs.48.club/puissant-builder/auction-transaction-feed
- 48Club Send Bundle: https://docs.48.club/puissant-builder/send-bundle
- BlockRazor support/docs index: https://www.blockrazor.io/support/

No builder is assumed fastest. Gate 0 measures the result.

## 6. Venue-specific minimum proof

### Flap

Freeze current BNB Portal/version and launch/buy semantics.

Current public documentation snapshot lists BNB Portal v5.8.6:
https://docs.flap.sh/flap/developers/deployed-contract-addresses

Prove from current ABI/on-chain evidence whether token address derivation is deterministic before `T0`, then build the exact first BUY path.

### Four.meme

Freeze current TokenManager2 create/trade surface.

Current public integration documentation states that new classic launches use `TokenManager2.createToken` and emit `TokenCreate` containing the token address:
https://github.com/four-meme-community/fourmeme-docs/blob/main/docs/create-guide.md

Do **not** assume that the future token address is derivable from the pending create transaction. Gate 0 must prove this from current contract behavior or mark it unavailable.

If the address cannot be known before execution, quantify the earliest valid alternative and its effect on first-BUY placement.

## 7. Minimal Gate-0 harness

Gate 0 may use a small native Rust harness:

```
gate0/
  Cargo.toml
  src/
    main.rs
    clock.rs
    result.rs
    bsc_ingress.rs
    bsc_inclusion.rs
    flap_probe.rs
    fourmeme_probe.rs
    solana_pump_probe.rs
```

It may contain only what is required to answer feasibility:

- monotonic timestamps;
- launch recognition;
- minimal verified venue decode;
- token/address derivation where proven;
- minimal exact initial quote/fee requirements;
- nonce/gas/head state required for a valid BSC BUY;
- transaction build/sign;
- candidate inclusion adapters;
- result recording after the hot path.

Do not add Strategy C, research DB architecture, full reorg engine, full capture framework or GUI before Gate-0 PASS.

## 8. Gate-0 stages

### G0.1 Evidence freeze
Freeze current Flap/Four contracts, versions, selectors, launch and BUY semantics.

### G0.2 Pre-computability proof
For each venue prove whether token address + all BUY inputs are available before `T0`.

### G0.3 Local hotpath proof
Benchmark:

```
T_pre -> venue decode -> BUY build -> sign -> T_ready
```

Target: comfortably below 1 ms on target-like hardware.

This is an engineering metric only; it does not prove a real first BUY.

### G0.4 Pending coverage
Measure what percentage of real launches is seen with complete actionable data before canonical execution.

Record private/unseen launches explicitly. Do not assume one local node sees every launch.

### G0.5 Inclusion-path race
Compare candidate submission/builder paths under the same test conditions.

Measure acknowledgement as diagnostic data only.

### G0.6 Sequential validity
Before value-bearing validation, prove that:

```
launch -> BUY
```

is a valid ordered execution for the exact venue/version/mode.

### G0.7 Controlled Mainnet validation
Only after dry qualification, use minimal user-authorized test value.

No unattended strategy trading is needed for Gate 0.

### G0.8 Economic ordering proof
For each successful validation record:

- launch tx hash/index;
- our BUY tx hash/index;
- same block YES/NO;
- successful competing BUYs between launch and ours;
- entry price versus earliest observable successful BUY price;
- revert/drop/failure;
- inclusion path used.

### G0.9 Decision

```
PASS:
- exact BUY can be prepared fast enough;
- real validation reaches the first technically possible execution position with competitive repeatability.

ADAPT:
- failure is attributable to one measured, realistically removable bottleneck.

STOP:
- the venue/chain cannot repeatedly reach an economically early enough first-BUY position after credible ingress/inclusion paths are tested.
```

## 9. Local latency classification

For the controllable local reaction path, not as a substitute for actual execution:

- <1 ms: target PASS
- 1–3 ms: ADAPT only with a concrete removable bottleneck
- >3 ms: current local path FAIL
- ~10 ms or worse: STOP that local architecture for Birth Sniping

## 10. Not valid as the primary Gate-0 success metric

Do not use any of these alone:

- block time;
- finality;
- canonical block timestamp;
- local decode time;
- signing time;
- local geth acceptance;
- RPC/builder acknowledgement;
- `EARLY_TX_SEEN - canonical observation`;
- a simulated fill.

## 11. Project rule

Gate 0 comes before:

- full BSC C0/C1/C2/C3 production build;
- full Flap/Four production decoder phases;
- 30-minute research capture;
- Strategy C BSC integration;
- PAPER;
- production LIVE;
- optional kernel/NIC/multi-region optimization.

Only a venue/chain that passes Gate 0 proceeds.
