# DarkBot — Phase F / M9 Native Dual-Venue 30-Minute Capture Contract

Status: **BLOCKED UNTIL PHASE D + E ARE ACCEPTED**
Milestone: **M9**
Primary deliverable: **immutable native binary event journal + manifest**
End rule: **STOP after Phase-F review; Economics Gate next**

Normative parent: `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`.

## 1. Purpose

Phase F captures the exact native BSC event stream used by the Rust runtime for Flap and Four.meme.

This is not a Python/WSS approximation of what LIVE would have seen.

The stored 30-minute dataset must be directly replayable into the exact same Rust event API later used for Strategy, PAPER and LIVE.

## 2. Mandatory topology

```
BSC P2P
   |
local bsc-geth
   |
native new-tx/canonical hooks
   |
FastLaneEvent / CanonicalEvent
   |
   +------------------> SAME Rust core -> Flap + Four factual decode
   |
   +--> bounded recorder ring -> immutable binary journal
```

Both consumers receive the same versioned event object/sequence/timestamp.

No Python transformation is allowed before the primary journal.

WSS/RPC providers may run only as secondary comparison/recovery evidence and must have separate source ids. They are not the latency source of truth.

## 3. Capture duration

Primary clean target:

**30 continuous wall-clock minutes of valid native capture.**

A failed/overflowed run is retained as diagnostic evidence and repeated.

No silent loss.

## 4. Primary artifacts

Required:

```
market_capture_bsc_native_30m.bin
market_capture_bsc_native_30m.manifest.json
```

Optional derived artifacts:

```
market_capture_bsc_30m.sqlite
market_capture_bsc_30m.parquet
```

The binary journal is immutable source evidence. Derived DBs never replace it as replay input.

Manifest includes at minimum:

- journal SHA-256;
- wire-format version/hash;
- Rust core commit/hash;
- bsc-geth commit/patch identity;
- chain id;
- start/end UTC and monotonic clock metadata;
- sequence range/count;
- config hashes;
- Flap/Four evidence/version hashes;
- queue/overflow counts;
- reconnect/recovery intervals;
- canonical/reorg quality;
- recorder completeness;
- target-host identity relevant to latency.

## 5. Exact event requirements

Record enough information for deterministic future Rust replay without RPC reconstruction of the pending input:

- sequence id;
- native first-seen timestamp;
- source;
- raw signed tx bytes or lossless equivalent;
- hash/from/nonce/to/value/gas/calldata;
- native geth event timestamp;
- canonical/head/block/receipt/log evidence needed by shared state;
- quality/reorg transitions.

Never replace original first-seen with enrichment/persistence time.

## 6. Two venues, one runtime

Flap and Four.meme decoders are the Phase-D/E Rust modules.

Do not run:

- Python Flap decoder for capture;
- Python Four decoder for capture;
- separate venue feeds;
- separate clocks;
- separate BSC state engines.

One event may be inspected by both recognizers; raw chain evidence is stored once.

## 7. -10 ms lead objective

For every canonical launch/trade where comparison is valid record:

```
raw_native_lead = T_NATIVE_PENDING - T_CANONICAL_REFERENCE
```

Primary objective:

```
raw_native_lead <= -10 ms
```

Also record where available:

```
T0 native geth event
T1 ring publish
T2 Rust receive
T3 decode
TC canonical reference
```

Later execution phases add T4-T9.

Report per venue and combined:

- min;
- p1/p5 where useful for worst lead;
- p50;
- p95;
- p99;
- max;
- count with lead <= -10 ms;
- count between -10 and -3 ms;
- count worse than -3 ms;
- missing/invalid timing evidence.

Do not redefine the reference if the result is disappointing.

## 8. Native runtime latency during capture

Even though Phase F is read-only with respect to strategy/execution, measure the actual native ingest/decode prefix:

- T0->T1;
- T1->T2;
- T2->Flap/Four recognition;
- matched full decode;
- recorder fanout cost.

Report p50/p95/p99/p99.9/p99.99/max and exact counts >=0.5 ms, >=1 ms and >3 ms.

Any reproducible >3 ms code path is a blocker.

## 9. Replay proof before Phase-F acceptance

Before accepting the dataset:

1. close/hash the binary journal;
2. replay it through the same Rust event API;
3. reproduce the same factual venue decode/state sequence;
4. prove sequence/event counts;
5. prove no Python normalization is required;
6. compare derived summaries against the live capture summary.

This is a mandatory capture->replay identity test.

## 10. Canonical census

Pending visibility never defines launch count.

Build independent canonical census for each venue from the same shared canonical event stream.

Report:

- canonical launches;
- pending-covered launches;
- never seen pending;
- pending after inclusion;
- gap affected;
- unsupported version/mode;
- reverted;
- orphaned/reorg affected.

## 11. Quality gate

Not clean if any unresolved:

- wrong chain;
- journal corruption/hash mismatch;
- sequence discontinuity not explicitly represented;
- silent queue loss;
- timestamp/source ambiguity;
- native/canonical state uncertainty contaminating claimed clean interval;
- decoder ambiguity for evidence claimed supported;
- binary replay mismatch.

Capture recorder failure invalidates research evidence but must never be allowed to stall a future LIVE trading hotpath.

## 12. Out of scope

No Strategy C optimization, no EV ranking, no economics assumptions, no real broadcast.

Phase F does not yet need a production trade decision, but it must already use the same Rust event/decode/state modules that later phases extend.

## 13. Acceptance

Phase F is complete only when:

- one clean 30-minute native binary journal exists;
- Flap/Four factual decode is accepted;
- native lead is measured honestly against canonical reference;
- the -10 ms objective is quantified;
- capture->Rust replay identity passes;
- quality/integrity manifest passes;
- no second trading/capture semantic implementation exists.

Then **STOP**. Economics Gate is next.
