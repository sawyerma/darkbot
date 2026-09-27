# DarkBot BSC Dual-Venue Roadmap — Native Single Runtime

Status: **ACTIVE AUTHORITATIVE ROADMAP**
Updated: 2026-09-27
Repository: `sawyerma/darkbot`

This file controls phase order/status. Detailed runtime invariants are in `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`.

## 1. Non-negotiable architecture decision

BSC testing and BSC LIVE must use the same Rust runtime implementation.

There is one normative BSC Rust core for:

```
venue decode -> state -> strategy -> economics/quote -> admission -> tx build -> sign
```

Capture, Replay, PAPER and LIVE are not separate trading implementations. Only external input/output adapters may differ.

Python remains research/reporting/reference infrastructure, not a second BSC trading engine.

The latency goal is to exploit native BSC pending visibility and pursue an effective pre-canonical advantage around **-10 ms**. Internal native processing is engineered with target <0.5 ms, warning >=1 ms and hard economic failure >3 ms.

## 2. Authoritative contracts

| Scope | Contract |
| --- | --- |
| Native runtime / capture-replay-live identity / latency | `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md` |
| Current/historical Phase-C technical evidence | `docs/BSC_IMPLEMENTATION_HANDOFF_CLAUDE.md` |
| Flap | `docs/BSC_PHASE_D_FLAP_READ_ONLY_CONTRACT.md` |
| Four.meme | `docs/BSC_PHASE_E_FOURMEME_READ_ONLY_CONTRACT.md` |
| Combined native 30-minute capture | `docs/BSC_PHASE_F_DUAL_CAPTURE_CONTRACT.md` |
| Economics | `docs/BSC_ECONOMICS_GATE_CONTRACT.md` |

If an older document conflicts with the Native Single-Runtime Contract for post-Phase-C BSC runtime architecture, the Native Single-Runtime Contract wins.

`docs/phase_a/**` remains frozen historical evidence.

## 3. Phase sequence

| Phase | Status | Scope |
| --- | --- | --- |
| A / M0-M3 | **IMPLEMENTED** | multichain seam, Solana/Pump wrappers, runtime/profile compatibility |
| B | **SKIPPED** | optional cleanup only |
| C / M4-M6 | **EXTERNAL HARDENED CANDIDATE; MERGE/ACCEPTANCE REQUIRED** | Python EVM/BSC evidence/canonical reference; retain as reference, not final BSC LIVE runtime |
| Native Runtime Gate / NR0 | **NEXT AFTER C ACCEPTANCE/MERGE** | bsc-geth native hook + binary wire + SPSC/shared-memory + Rust echo/build/sign/submit + exact binary capture/replay + T0-T9 benchmark |
| D / M7 | **BLOCKED** | Flap recognition/decoding implemented directly in the same Rust core; read-only semantics only |
| E / M8 | **BLOCKED** | Four.meme recognition/decoding in the same Rust core |
| F / M9 | **BLOCKED** | 30-minute combined BSC native capture using exact FastLaneEvent stream consumed by the Rust core |
| Economics Gate | **BLOCKED** | exact Rust economics/quote modules validated against chain/helper evidence; no remote quote in hot path |
| M10 | **BLOCKED** | Strategy C in Rust single core; no Python duplicate |
| M11 | **BLOCKED** | deterministic replay of Phase-F binary journal through the same Rust core |
| M12 | **BLOCKED** | PAPER/shadow through the same core including build/sign; only broadcast sink differs |
| M13-M15 | **BLOCKED** | native submit/inclusion, venue execution, ordered/backrun work where evidence supports it |
| M16 | **BLOCKED** | forward validation on the same Rust runtime and native input path |
| M17 | **BLOCKED** | GUI/control plane only; no trading semantics in GUI |

No phase may create a second BSC decision implementation.

## 4. Native Runtime Gate — mandatory before D

The first post-C engineering work is not Flap/Four.

Build a minimal venue-neutral latency proof:

```
bsc-geth NewTxsEvent
 -> FastLaneEvent
 -> SPSC/shared memory
 -> Rust runner
 -> deterministic dummy build/sign
 -> binary submit ring
 -> local bsc-geth txpool
```

In parallel:

```
FastLaneEvent -> immutable binary capture journal
binary journal -> same Rust Engine::process_event
```

Gate requirements:

- no WSS/JSON/HTTP/RPC in T0-T8;
- same binary event schema in LIVE and replay;
- bounded queues/state;
- target T0-T8 <0.5 ms on dedicated target hardware;
- all >=1 ms samples analyzed;
- any reproducible >3 ms runtime path is a blocker;
- T9/P2P handoff measured where possible;
- output includes p50/p95/p99/p99.9/p99.99/max and exact outlier counts.

Only after this gate passes may D start.

## 5. Phase D — Flap

Protocol discovery/evidence freeze may use Python/tools, but the normative decoder is Rust.

Implement in `darkbot-core`:

- fixed address/version table from verified evidence;
- cheap destination + 4-byte selector recognition;
- pending launch decode;
- canonical CREATE/BUY/SELL/migration factual decode;
- explicit unknown/unsupported version handling;
- deterministic fixtures.

No economics, strategy or execution in D.

The same Flap decoder is used by native capture, replay, PAPER and LIVE. No Python Flap trading decoder is allowed.

## 6. Phase E — Four.meme

Same rule as D.

Implement the normative Four.meme decoder in the same Rust core with verified version/mode evidence.

No economics, Strategy C or execution yet.

## 7. Phase F — exact native 30-minute capture

The primary BSC research capture is no longer a Python/WSS approximation.

Required source:

```
local bsc-geth native tx event
 -> versioned FastLaneEvent
 -> same Rust core
 -> binary journal fan-out
```

Both venues run in one Rust core and one clock domain.

Primary evidence artifact:

```
immutable binary event journal + manifest
```

Derived SQLite/Parquet may be generated asynchronously for analysis.

The binary journal must be directly replayable through the same Rust event API with no Python transformation.

Phase F measures at least:

- T_NATIVE_PENDING;
- T_CANONICAL_REFERENCE;
- native lead distribution;
- effective decode latency;
- Flap/Four coverage;
- version/mode distribution;
- quality gaps/reorgs;
- exact sequence provenance.

Primary latency objective:

```
T_NATIVE_PENDING - T_CANONICAL_REFERENCE <= -10 ms
```

Report actual values even if the objective is missed.

## 8. Economics Gate

Economics is validated after real venue evidence exists.

Normative executable quote/economic math lives in Rust and is reused unchanged by Replay/PAPER/LIVE.

Python may analyze or cross-check results, but may not implement an alternative decision quote engine.

No synchronous `eth_call`/HTTP/helper request in the decision hot path.

## 9. Strategy C, Replay, PAPER, LIVE

Strategy C is implemented once in Rust.

### Replay

Phase-F binary events -> same Rust core.

### PAPER

Native/live or captured event -> same Rust core -> same tx build/sign -> non-broadcast sink.

### LIVE

Native event -> same Rust core -> same tx build/sign -> local bsc-geth submit sink.

If Replay/PAPER/LIVE would produce different decisions for the same event/state/config snapshot, that is a correctness bug.

## 10. What remains from Python Phase C

Keep it.

It provides:

- adversarial EVM/reorg/restart evidence;
- canonical/reference behavior;
- provenance/storage model;
- fixtures/oracles;
- research DB/report tooling.

Do not keep optimizing Python as the final BSC LIVE path.

## 11. STOP rules

Every phase ends with independent review.

Do not automatically proceed from:

- Native Gate -> D;
- D -> E;
- E -> F;
- F -> Economics;
- Economics -> Strategy;
- PAPER -> LIVE.

Latency regressions are correctness/release failures for this project.
