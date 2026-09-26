# DarkBot — Phase F / M9 Combined Real BSC Capture Contract

Status: **FUTURE CONTRACT / BLOCKED UNTIL PHASE D AND E ARE ACCEPTED**  
Milestone: **M9**  
Scope: **one real read-only BSC capture with Flap.sh + Four.meme simultaneously**  
Primary deliverable: **clean evidence dataset, not a strategy result**

---

## 1. Purpose

Phase F is the first real combined BSC measurement.

It runs the accepted Flap and Four.meme read-only decoders on the same BSC ingest and records enough pending + canonical evidence to answer:

- how many real launches occur on each venue;
- how often launches are seen pending before canonical inclusion;
- how early each provider sees them;
- whether token/asset identity is known before inclusion;
- which protocol versions/modes are actually active;
- what canonical trades/migrations occur;
- whether the captured evidence is clean enough to build economics afterward.

Phase F does not attempt to prove positive EV.

---

## 2. Preconditions

Do not start the primary capture until:

1. Phase C hardening is accepted;
2. restart/backfill/durability/reorg/first-seen/provider-provenance are proven;
3. Phase D Flap decoder is accepted;
4. Phase E Four.meme decoder is accepted;
5. all current Mainnet contract/version evidence is frozen;
6. full regression suite and compileall are green;
7. capture provider configuration contains no committed secrets;
8. BSC endpoint identity checks pass.

---

## 3. One-ingest architecture

Required topology:

```
BSC provider(s)
   |
   +--> pending
   +--> canonical heads/blocks/tx/logs/receipts
             |
             v
        one BSC chain state
             |
             v
       one shared dispatcher
          /        \
       Flap        Four
          \        /
           v      v
          Recorder/Writer
             |
             v
      one BSC capture DB
```

Do not run a separate full BSC capture process per venue for the primary experiment.

Both venues must share the same local clock/timestamp origin and canonical state.

---

## 4. Provider model

For every provider used, freeze/configure:

- provider_id;
- WSS endpoint via environment/config reference;
- HTTP endpoint via environment/config reference;
- pending capability detected at runtime;
- chain-id verification result;
- connection/reconnect history;
- first-seen observations;
- RPC source provenance for enrichment/block/receipt calls;
- quality gaps/overflow/reconnect intervals.

No credentials or private endpoints are written into source, reports or captured raw public artifacts unless explicitly safe to share.

Provider A evidence must never be mislabeled as Provider B.

---

## 5. Capture duration

Primary clean capture target:

**30 continuous wall-clock minutes of valid capture.**

The run may internally reconnect and canonically backfill, but:

- missed pending visibility remains an explicit pending gap;
- critical silent-loss/overflow is forbidden;
- any interval with unresolved canonical certainty is not counted as clean until recovered;
- if the primary 30-minute interval contains a critical data-integrity failure, retain it as a diagnostic run and repeat the clean primary capture.

Do not delete failed/diagnostic runs.

---

## 6. Dataset separation

Primary BSC output is separate from Solana:

```
data/market_capture_bsc_30m.sqlite
```

A separate Solana capture, when run for its own parity purpose, remains:

```
data/market_capture_solana_30m.sqlite
```

Never merge chain rows or use one database as the other's replay source.

If filenames need collision-safe timestamps in practice, preserve a stable manifest pointing to the exact primary dataset.

---

## 7. Capture manifest

Freeze a machine-readable manifest containing at minimum:

- capture id;
- start/end UTC;
- duration;
- code/source snapshot identity available in the execution environment;
- config hashes;
- migration/schema version;
- BSC chain id;
- provider ids/capabilities;
- Flap decoder version/ABI hashes;
- Four decoder version/ABI hashes;
- supported contract addresses/version identities;
- database SHA-256 after close;
- row counts by evidence type;
- quality-event counts;
- reconnect counts;
- pending gap intervals;
- canonical gap/backfill intervals;
- reorg/uncertain counts;
- queue overflow counts;
- writer failure/quarantine counts.

Do not infer missing provenance after the run.

---

## 8. Independent canonical denominator

Pending visibility must never define launch count.

For each venue compute canonical launch census from canonical decoded evidence:

`canonical_launches_total`

Then calculate pending coverage against that denominator:

`canonical launches that had valid pre-inclusion pending evidence / canonical_launches_total`

Also report:

- pending evidence arrived after inclusion;
- never seen pending;
- pending gap affected;
- ambiguous/unsupported version;
- orphaned-only evidence.

Do this separately for Flap and Four.meme.

---

## 9. Lead-time metrics

For every canonical launch with valid pending evidence record:

- earliest pending first_seen across providers;
- first_seen per provider;
- canonical block/tx observation time;
- block timestamp separately;
- inclusion block number/hash;
- tx index;
- local pending -> canonical observation delta;
- whether token/asset id was already known pending;
- enrichment delay where provider was hash-only.

Do not substitute enrichment-completion time for original first_seen.

Do not use block timestamp as a replacement for local observation timestamps.

---

## 10. Canonical venue measurements

Per venue collect/report:

- canonical CREATE count;
- BUY count;
- SELL count;
- migration/liquidity/graduation count where supported;
- version/mode distribution;
- unsupported/unknown mode/version count;
- reverted tx count;
- reorg/orphan count;
- canonical transaction ordering/indices;
- launch-to-first-canonical-trade timing where evidence supports it.

This is descriptive evidence only.

No profitability ranking in Phase F.

---

## 11. Raw evidence and integrity

Requirements:

- chain transaction evidence stored once;
- provider observations separate;
- venue mappings reference chain evidence;
- exact calldata/log/receipt fields required for later reconstruction retained;
- exact integer values;
- canonicality transitions retained;
- raw-evidence integrity hashes/references available;
- database close/flush confirmed before final SHA/count manifest.

High-water must mean durable completed evidence, not merely RAM enqueue.

---

## 12. Quality gate

The primary capture is not accepted as clean if any of these remain unresolved:

- wrong chain id;
- silent queue loss;
- critical writer failure;
- high-water beyond durable persistence;
- unresolved canonical gap;
- unresolved canonical uncertainty;
- unknown provider provenance for evidence used in latency analysis;
- corrupted/raw evidence mismatch;
- decoder version ambiguity for evidence claimed supported.

Pending disconnect gaps may exist only if they are explicitly recorded. A gap-affected interval is excluded/segmented from claims that require continuous mempool visibility.

---

## 13. Reporting

Produce one human-readable and one machine-readable summary.

At minimum per venue/provider:

- canonical launch count;
- pending coverage numerator/denominator;
- clean observation count;
- first-seen lead-time min/p50/p95/p99/max;
- hash-only enrichment delay distribution;
- provider comparison;
- version/mode counts;
- reverted/orphaned/unknown counts;
- quality gaps;
- dispatcher/decode performance;
- storage/queue performance.

Also report combined BSC system quality.

---

## 14. No tuning during capture

Before the primary run freeze:

- code;
- decoder evidence/version snapshots;
- config;
- provider set;
- capture duration;
- report definitions.

Do not change filters/decoders mid-run to improve statistics.

If a bug is found:

1. stop;
2. preserve the invalid/diagnostic dataset;
3. fix + retest;
4. start a new clean run.

---

## 15. Explicitly out of scope

Phase F does not implement:

- Strategy C multichain;
- entry/exit filters;
- replay trading;
- positive EV ranking;
- quote engines;
- fee/tax economics;
- execution plans;
- signing;
- broadcast;
- builder/relay;
- PAPER/LIVE.

It may record factual mode/tax configuration fields only when already decoded from verified chain evidence; economic interpretation waits for the Economics Gate.

---

## 16. Performance

During the real capture measure at minimum:

- receive -> parse;
- receive -> full tx available;
- receive -> dispatcher complete;
- venue recognizer/decode;
- dispatcher -> recorder enqueue;
- writer queue wait/persist completion;
- queue depths;
- overflows;
- RPC/enrichment latency.

Report min/p50/p95/p99/max/sample count.

The capture path must remain bounded.

Do not claim the final 3 ms LIVE socket-handoff target.

---

## 17. Simplicity/code-quality gate

Phase F should add only the minimum user-facing/config/report wiring needed to run the already-built capture pipeline.

Do not add:

- another BSC engine;
- venue-specific feed processes;
- another database framework;
- ORM;
- event bus;
- multiprocessing merely for architecture;
- hardcoded credentials/endpoints;
- venv/vendor folders;
- speculative execution code.

---

## 18. Test gate before real capture

Required deterministic tests:

- combined Flap+Four dispatcher;
- one tx inspected by both recognizers without duplicated chain evidence;
- provider provenance;
- restart from persisted high-water;
- reconnect/backfill;
- pending-gap accounting;
- reorg/orphan/uncertain;
- durable writer/high-water;
- queue overflow fail-closed;
- exact report denominators;
- manifest/database integrity;
- orderly shutdown/flush.

Then complete suite + compileall.

---

## 19. Acceptance

Phase F is complete only when:

- one clean primary combined 30-minute BSC dataset exists;
- manifest/integrity checks pass;
- independent canonical census exists for both venues;
- pending coverage and lead times are correctly measured;
- provider provenance/gaps are explicit;
- protocol version/mode distribution is known for observed launches;
- no critical quality failure contaminates the primary clean interval;
- full regression suite remains green.

Then:

**STOP. Analyze the dataset. Do not start Strategy C.**

The next gate is the separate BSC Economics Contract.
