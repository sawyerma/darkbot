# DarkBot — Phase F / M9 Native Dual-Venue Capture Contract

Status: **BLOCKED UNTIL PROMOTED VENUE DECODERS ARE ACCEPTED**
Milestone: **M9**
End rule: **STOP after Phase-F review**

Normative parents:
- `docs/GATE0_TESTPLAN_SHORT.md`
- `../1_PLAN/PHASENPLAN.md`
- `docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`

## 1. Purpose

Phase F is a production-quality native market-evidence capture.

It does **not** decide first-BUY feasibility; Gate 0 already did that.

Phase F quantifies:
- pending coverage;
- source/peer timing;
- venue versions/modes;
- canonical outcomes;
- data-quality gaps;
- production ingest/decode latency;
- capture→replay identity.

## 2. Topology

```
BSC P2P
 -> pinned local bsc-geth
 -> native event hooks
 -> same Rust core
      -> promoted Flap/Four decoder(s)
      -> bounded native recorder
```

No Python transformation before the primary binary journal.

Secondary WSS/RPC evidence may be recorded under separate source ids but is not the latency source of truth.

## 3. Duration

Primary target: 30 continuous minutes of clean native capture.

A failed/overflowed run is retained as diagnostic evidence and repeated.

## 4. Artifacts

Required:

```
market_capture_bsc_native_30m.bin
market_capture_bsc_native_30m.manifest.json
```

Optional derived SQLite/Parquet never replace the binary journal.

Manifest includes:
- journal hash;
- wire/core/geth versions;
- chain id;
- start/end and clock metadata;
- sequence/count;
- config/evidence hashes;
- queue/overflow counts;
- canonical/reorg quality;
- host metadata relevant to latency.

## 5. Event evidence

Record enough for replay without reconstructing pending input from RPC:

- sequence id;
- native first-seen timestamp;
- source/peer/message type where available;
- raw signed tx bytes or lossless equivalent;
- hash/from/nonce/to/value/gas/calldata;
- txpool/canonical/head evidence;
- receipt/log evidence;
- reorg/quality transitions.

Never overwrite original first-seen with enrichment time.

## 6. One runtime

Promoted venue decoders inspect the same native event object.

No:
- Python venue capture decoder;
- separate venue feeds;
- separate clocks;
- separate BSC state engines.

## 7. Pending-coverage and pre-lead diagnostics

For each canonical launch report:

- seen as complete pending tx before inclusion: YES/NO;
- earliest source peer/path;
- full-TX vs hash-announcement path;
- first-seen spread across peers;
- token/address actionable before execution: YES/NO/UNKNOWN;
- venue/mode/version;
- reverted/replaced/orphaned state.

A canonical-observation lead may be reported as a **diagnostic** if timestamp semantics are clear:

```
diagnostic_pending_vs_canonical_observation =
    T_NATIVE_PENDING - T_CANONICAL_OBSERVATION
```

It is **not**:
- the authoritative `T_pre-T0`;
- proof the coin was buyable at the canonical observation timestamp;
- a Gate-0 PASS condition.

There is no Phase-F requirement that this value be `<= -10 ms`.

## 8. Native local latency

Measure:
- peer decode→native event;
- native event→ring publish;
- ring→Rust receive;
- recognition;
- matched full decode;
- recorder fanout.

Report p50/p95/p99/p99.9/p99.99/max plus counts >=0.5/1/3 ms.

Reproducible >3 ms local code paths block production until explained/corrected.

## 9. Replay proof

Before acceptance:

1. close/hash journal;
2. replay through same Rust API;
3. reproduce factual venue/state sequence;
4. prove sequence/event counts;
5. prove no Python normalization is required;
6. compare derived summaries.

## 10. Canonical census

Report per venue:
- canonical launches;
- pending-covered launches;
- never-seen-pending;
- pending first observed after inclusion;
- unsupported version/mode;
- reverted;
- orphan/reorg affected;
- actionable pre-execution address/input availability.

This measures how representative Gate-0 pending assumptions are over a larger sample.

## 11. Quality gate

Not clean with unresolved:
- wrong chain;
- journal corruption;
- silent sequence loss;
- queue loss;
- timestamp/source ambiguity;
- canonical uncertainty contaminating a claimed clean interval;
- decoder ambiguity;
- replay mismatch.

Recorder failure invalidates research evidence but must not stall future LIVE hotpath.

## 12. Out of scope

No Strategy C optimization, EV ranking or first-time real execution proof.

Phase F may compare already Gate-0-validated inclusion metadata offline, but it does not need to send trades.

## 13. Acceptance

Complete only when:
- one clean 30-minute binary journal exists;
- promoted decoder(s) are factual and current;
- pending coverage is quantified honestly;
- pre-execution actionability is quantified;
- native ingest/decode tails are measured;
- capture→replay identity passes;
- quality manifest passes.

Then STOP. Full Economics Gate is next.
