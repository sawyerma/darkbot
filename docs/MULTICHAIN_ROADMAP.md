# DarkBot — Current Multichain Roadmap

Status: **CURRENT HIGH-LEVEL OVERVIEW**
Updated: 2026-09-27

Detailed source of truth:
- Solana current production/research documents remain authoritative for the existing Pump.fun runtime.
- BSC scheduling: `docs/BSC_DUAL_VENUE_ROADMAP.md`
- BSC first feasibility gate: `docs/GATE0_TESTPLAN_SHORT.md`
- BSC birth-entry objective: `docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md`
- BSC production runtime after PASS: `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`

Older versions of this file are historical only and remain available through Git history.

## 0. Global execution invariant

For birth-sniping, DarkBot trades **directly on-chain**. Venue websites/frontends are never in the BUY path.

```
chain/pending signal -> local decision/build/sign -> inclusion adapter -> venue contract
```

No browser, page refresh, frontend trading API or manual action may gate execution. If required BUY inputs are available before first tradability, precompute/pre-submit through a proven ordered path as far as safely possible; otherwise react to the earliest valid chain evidence. Never wait for frontend publication.

## 1. Product shape

```
DarkBot
  |
  +-- Solana / Pump.fun
  |     existing runtime remains operational
  |
  +-- BSC
        Gate 0 first
        -> promote only a proven venue/path
        -> native Rust production runtime after PASS
```

Do not force both chains into one runtime before the new native path is independently proven.

## 2. Common economic objective

For a birth-sniping venue the relevant product question is:

> How early can a real successful BUY execute after the asset first becomes buyable?

Local CPU/decoder/sign/submit latency is supporting engineering evidence only.

## 3. Solana

Current Solana/Pump.fun remains the working baseline.

A native Solana rewrite is not started merely for symmetry.

Before any later migration:
- run the same Gate-0 first-BUY feasibility logic;
- prove the new native path materially improves the actual entry objective;
- preserve strategy/economic/replay semantics;
- migrate only after parity and forward validation.

## 4. BSC

BSC does not start with the full production architecture.

Current order:

```
Gate 0:
  Flap + Four evidence
  -> pre-T0 actionability
  -> minimal native build/sign/serialize/queue through T_send
  -> pending coverage
  -> inclusion race including ordered builder paths
  -> controlled real intended-token acquisition + first-external-BUY ordering validation
  -> PASS / ADAPT / STOP

After PASS:
  C0 production foundation
  C1 native ingress
  C2 state/reorg/replacement
  C3 local execution engineering gate
  D/E full promoted venue decoders
  F production capture
  G complete economics
  H Strategy C / EV
  I PAPER/shadow + final full-production hot-path requalification
  J production LIVE
  K measured escalation
```

## 5. BSC timing rule

Definitions:

```
T_pre = earliest actionable evidence before tradability
T0    = first execution point where the coin is buyable
T_buy = our successful BUY
```

Desired product target:

```
T_buy - T0 < 1 ms
```

Do not manufacture an intra-block millisecond value from ordinary BSC block timestamps.

For BSC the primary externally verifiable first-BUY evidence is transaction ordering:
- launch/activation transaction;
- our successful external BUY at the earliest valid following position, with intended-token acquisition evidence;
- number of successful competing BUYs between them.

## 6. No assumed BSC advantage

BSC pending visibility is a hypothesis to measure, not a guarantee.

Do not assume:
- every launch is public pending;
- fixed -10 ms pre-lead;
- token address is derivable before launch for every venue;
- a local node is the fastest inclusion path;
- a builder ACK equals execution;
- same block proves <1 ms.

## 7. Runtime ownership after BSC PASS

Production BSC:
- Go/bsc-geth: chain-client/native hooks only;
- Rust: runtime semantics, venue decode, state, economics, strategy, build/sign, inclusion, capture/replay/PAPER/LIVE;
- Python: offline analysis/reporting only.

No Python BSC trading engine.

## 8. Shared strategy relationship

Strategy C is not the Gate-0 feasibility test.

First prove that a venue/path can obtain the required early BUY.

Only afterward evaluate whether Strategy C has positive expectancy after:
- exact entry;
- complete protocol costs;
- gas/tips;
- failures/reverts;
- slippage;
- exit execution.

Latency feasibility and economic edge are separate gates.

## 9. Stop rule

If no tested venue/path can achieve sufficiently early real execution after credible ingress/inclusion testing, stop that birth-sniping branch instead of expanding architecture.

If one venue passes and another fails, promote only the passing venue first.
