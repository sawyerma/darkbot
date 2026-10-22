# DarkBot BSC — C1 Status + Fable Implementation Handoff (C1.5–C1.10)

**Date:** 2026-10-08  
**Status checkpoint before this documentation-only handoff:** `353c936bd973b032cc28d1ec6fa226acef50a3b9`  
**Pinned bsc-geth:** `cdb7548b5baacfdae92f9f63437d6456411665f3` (bnb-chain/bsc v1.7.8)  
**Normative plan:** `bsc/docs/BSC_C1_PARALLEL_FAST_LANE_PLAN.md`  
**Governing documents:** `BSC_PRODUCTION_BUILD_MASTER_PLAN.md`, `BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`, `BSC_BIRTH_ENTRY_ARCHITECTURE.md`

## 1. Current accepted status

The following C1 gates are accepted and must be treated as frozen inputs unless a later change explicitly reopens and requalifies them:

| Gate | Status | Accepted evidence / note |
| --- | --- | --- |
| C1.1 | PASS / COMPLETE | V1 ownership defect reproduced; concurrent-peer V1 is not a production baseline |
| C1.2 | PASS / COMPLETE | Wire V2 accepted |
| C1.3 | PASS / COMPLETE | lifecycle accepted; acceptance commit `c804bf74bb6a868a8610b79a100e3729038ec82d` |
| C1.4 | PASS / COMPLETE | geth ingress accepted; acceptance commit `cefd94659b7bc318ddd02a737b87f8be39e71857` |
| C1.4a | PASS / COMPLETE | `GREEN_STRONG_HEADROOM`; evidence commit `353c936bd973b032cc28d1ec6fa226acef50a3b9` |
| C1.5–C1.10 | INTEGRATED / COMPILED / NOT YET QUALIFIED | REV2 fixes and local regression checks completed; ordered gate acceptance remains open |

Authoritative C1.4a evidence:

- 4 simultaneous independent peer lanes;
- bounded one-outstanding-per-lane load model;
- explicit 32-record warmup per peer, retained in raw evidence;
- 128 warmup samples + 1000 qualification samples;
- qualification sequences 32..281 per lane;
- `T_ingress -> L2`: p50 5 us, p95 10 us, p99 14 us, p99.9 16 us, max 19 us;
- 1000/1000 qualification samples below 200 us;
- integrity PASS, zero failures;
- queue depth max = 1;
- classification = `GREEN_STRONG_HEADROOM`.

Authoritative files:

- `bsc/native/evidence/c1/c1-4a-early-latency-20261008.json`
- `bsc/native/evidence/c1/c1-4a-early-latency-20261008.csv`
- `bsc/native/evidence/c1/c1-4a-early-latency-20261008-go-snapshot.json`
- `bsc/native/evidence/c1/c1-4a-early-latency-20261008-lane-stats.csv`

These C1.4a results are an ingress→Rust feasibility result only. They do not prove acquisition latency, Mainnet ingress coverage, provider ordering, inclusion quality, or LIVE readiness.

REV2 implementation, executed commands, defects fixed and remaining gate work:
[BSC_C1_REV2_INTEGRATION_REPORT_20261008.md](../3_BEWEISE/BSC_C1_REV2_INTEGRATION_REPORT_20261008.md).
The short devp2p transport run is a regression smoke test, not C1.9 acceptance.

## 2. Fable assignment

Implement **C1.5 through C1.10 as one coherent implementation tranche** against the supplied source archive. Do not stop after each work package merely to produce separate ZIPs. The result should be one internally consistent candidate that contains the code, tests, measurement tooling, evidence schemas/templates and freeze tooling required for all remaining C1 work packages.

However, implementation and acceptance remain separate:

> Fable implements C1.5–C1.10 together. Local integration/qualification is still performed sequentially: C1.5, then C1.6, then C1.7, then C1.8, then C1.9, then C1.9b, then C1.10.

Do not mark a later gate PASS merely because its code exists. Do not mark C1 frozen before every earlier exit criterion has passed locally.

## 3. Non-negotiable architecture constraints

Preserve all accepted C1.1–C1.4a semantics unless a change is strictly required by C1.5–C1.10. If an accepted boundary or wire semantic must change, explicitly identify the reopened gate and the exact requalification required.

Mandatory invariants:

- one active peer handler = one V2 peer SPSC lane = sole producer for that connection lifetime;
- no global producer-written ready bitmap;
- no common MPSC head for `EARLY_TX_SEEN`;
- no new shared blocking hot-path mutex;
- `T_packet_decoded` remains distinct from actionable `T_ingress`;
- `T_ingress` remains after required pinned-geth basic validation;
- geth prefilter remains a conservative address+selector superset, never economic/admission authority;
- Rust remains authoritative for factual venue decode, state/economics and admission;
- Wire V2 integrity stays enabled; do not remove CRC/integrity to improve benchmarks;
- no JSON/WSS/filesystem/DB/DNS/connect/handshake/synchronous RPC/HTTP/log-wait in the production hot path;
- no per-BUY thread/goroutine creation;
- no C2 canonical/pending state implementation in this tranche;
- no C3 parallel-builder Runner Rev4 implementation in this tranche;
- do not modify `project_0/`;
- do not introduce a second BSC implementation path alongside the accepted native path.

## 4. C1.5 — Rust HOT0 / zero-copy

Implement the §10 / C1.5 contract completely.

Required behavior:

1. Replace the generic receive-path payload heap copy (`to_vec().into_boxed_slice()` or equivalent) with a borrowed-slot guard.
2. Acquire the committed sequence, validate the exact Wire V2 header/payload integrity contract, expose a `BorrowedRecord<'lane>`-style view, decode/decide directly from mmap bytes, and advance `read_sequence` only when the guard is released/consumed.
3. The guard must never escape HOT0 or outlive the lane/slot.
4. Producer slot reuse must remain impossible before the consumer releases/advances the read sequence.
5. For an admitted candidate, copy only required immutable launch/BUY material once into a bounded preallocated `IntentSlot`; do not retain borrowed mmap references in downstream adapters.
6. Implement local active-lane polling with fair round-robin beginning after the last successful lane; one noisy peer must not starve others.
7. Cross-lane ordering must remain timestamp/provenance based; never infer a global order from per-lane ring sequence.
8. Deduplicate duplicate launch transactions observed on multiple peer lanes using bounded Core state/identity semantics without turning the peer lanes into a shared MPSC structure.
9. HOT0 must not add filesystem, DB, network reads, sleeps, blocking recorder waits, lazy initialization, or avoidable allocation.

Required tests/evidence tooling:

- lifetime/soundness tests;
- slot cannot be reused before guard release;
- slot can be reused immediately after release;
- corruption/integrity still fails closed;
- wrap/full/backpressure behavior preserved;
- multi-lane fairness/starvation tests;
- duplicate-across-peers tests;
- allocation measurement proving no generic receive-path heap copy/allocation in the qualified common path;
- exact semantic parity with the accepted C1.4 record content.

Do not claim C1.5 PASS in the implementation package; provide the tests and exact local commands required to qualify it.

## 5. C1.6 — M4 development profile

Implement topology/profile evidence and macOS scheduling support for permanent latency-critical threads.

Required:

- reproducible host/topology evidence capture;
- record relevant Apple performance levels/cache/topology information that can be obtained reliably;
- macOS QoS support for permanent HOT0/geth critical threads where technically appropriate;
- retain benchmarkable Profile S / Profile P concepts from the normative plan;
- topology chooses only an initial profile/hint; measured latency decides;
- no hard-coded claim that a particular P-core count qualifies a profile;
- no claim that macOS/M4 qualification equals the final dedicated Linux C3 host.

The implementation must degrade explicitly/safely when a topology/QoS feature is unavailable. No hidden fallback may alter correctness.

Required exit tooling: reproducible host/profile report plus tests for configuration/profile selection and failure/fallback behavior.

## 6. C1.7 — local-geth outbound

Implement/qualify persistent local peer-send ownership.

Required architecture:

```text
Rust egress SPSC
-> geth outbound coordinator
-> pre-created persistent selected peer worker A
-> pre-created persistent selected peer worker B
-> ...
```

Requirements:

- no goroutine/thread creation per BUY;
- peer worker ownership is persistent and explicit;
- slow/failing peer A cannot head-of-line block first handoff to peer B;
- peer selection/maintenance occurs outside the BUY dispatch path where possible;
- eliminate unmeasured per-record `allNonEVNPeers()`-style selection/copy/lock from the qualified path;
- each worker preserves its original handoff timestamp;
- evidence transport is ownership-safe (per-worker SPSC or equivalent architecture already permitted by the normative plan);
- measure first selected-peer handoff and all selected-peer handoffs separately;
- explicit failed/unknown peer-send state;
- do not reinterpret local geth acceptance/handoff as a successful BUY.

Tests must cover multiple selected peers, slow peer, failing peer, partial handoff, worker lifecycle, no per-BUY worker creation, and evidence ordering/provenance.

## 7. C1.8 — concurrency and state-safety stress

Build the stress/qualification suite for all of the following:

- simultaneous independent peer producers;
- lane ownership/reuse/exhaustion;
- disconnect/reconnect/churn;
- producer restart;
- consumer restart;
- generation mismatch;
- wraparound/full/no-overwrite;
- torn/corrupt metadata;
- corrupt payload;
- oversize records;
- consumer stall;
- high-TX burst;
- canonical/reorg while TX load is active;
- recorder/background load;
- local-geth slow/failing peers;
- actual single-producer ownership of every Chain-lane publisher in the pinned geth, including the relevant `chainmu`/chain-write ownership proof.

The earlier C1.4a artificial burst diagnosis is not a substitute for C1.8. C1.8 must intentionally stress queueing, saturation and concurrency and prove invariants under that load.

Exit criterion: no ownership/state invariant violation. Any silent loss, overwrite, unaccounted gap, lifecycle reuse race, corruption acceptance or false single-producer assumption is a failure.

## 8. C1.9 — native latency qualification tooling

Implement the final C1 native latency measurement tooling around the integrated C1.5–C1.8 candidate.

Preserve compatible raw timestamps for:

```text
T_packet_decoded
T_ingress
L1 = mmap/SPSC Release commit
L2 = Rust Acquire/receive boundary
local-geth first selected-peer handoff
local-geth every selected-peer handoff
```

Requirements:

- use compatible clock domains and state them explicitly;
- retain raw samples;
- report sample count, min, mean, p50, p95, p99, p99.9 and p99.99 only where statistically supported, plus max;
- report misses, queue depth, drops, oversize, corruption, HoL and allocation/lock observations;
- preserve exact source commit/build/config/topology hashes;
- never synthesize end-to-end latency by adding component percentiles;
- never substitute p99 for max;
- measurement instrumentation must not silently redefine L0/L1/L2;
- separate warmup from the qualification window if warmup is used, with all warmup raw evidence retained and the qualification window fixed before measurement.

C1.9 implementation may provide harnesses and schema/templates, but Fable must not fabricate measured PASS evidence. Real evidence is generated during local qualification.

## 9. C1.9b — hook/path early qualification tooling

Implement controlled Testnet hook/path qualification tooling that can measure, per source/peer:

```text
known controlled launch handoff
-> packet decode
-> T_ingress
```

Requirements:

- preserve raw attributable timestamps and source identity;
- distinguish broadcast vs pooled path when applicable;
- characterize hook/path lateness and misses;
- never turn Testnet hook/path results into a Mainnet coverage claim;
- explicitly keep `MAINNET_INGRESS_COVERAGE = NOT_PROVEN` unless a separate Mainnet-qualified capture proves otherwise.

No value-bearing Mainnet action is authorized by this tranche.

## 10. C1.10 — freeze tooling and completion package

Implement the machinery/report needed to freeze C1 only after C1.5–C1.9b have locally passed.

The final freeze package must bind:

- pinned geth base commit;
- final C1 patch/source hash;
- final geth binary hash;
- Wire V2 specification/hash;
- Rust build/binary hash;
- C1 configuration;
- topology/profile evidence;
- complete accepted test inventory;
- raw accepted measurement evidence and hashes;
- exact source/build commands;
- exact non-claims.

Do **not** emit `IMPLEMENTED_TESTED/FROZEN` merely because this tooling exists. The freeze command/report must refuse or remain incomplete when required acceptance inputs are missing.

Only after local C1.10 acceptance may work proceed to C2.

## 11. Required non-claims throughout

Every implementation report and later C1 freeze statement must preserve:

```text
T0_TO_T_BUY_LT_1MS = NOT_PROVEN
MAINNET_INGRESS_COVERAGE = NOT_PROVEN unless separately qualified
PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED unless separately qualified
LIVE = BLOCKED
```

Also preserve:

- C1.4a GREEN proves only measured geth→Wire-V2→Rust feasibility;
- local geth handoff is not acquisition;
- builder ACK is not acquisition;
- same block is not sub-ms proof;
- ordinary BSC block timestamps do not prove `T_buy-T0 < 1 ms`.

## 12. Scope discipline

Allowed primary scope:

- `bsc/native/**`
- `bsc/docs/**` only where required to document the C1.5–C1.10 candidate;
- pinned-geth overlay/source integration required by C1.7/C1.8/C1.9 tooling.

Forbidden scope without explicit necessity and explanation:

- `project_0/**`;
- unrelated Solana/Pump.fun code;
- unrelated BSC strategy/economics rewrites;
- C2/C3 implementation;
- secrets/credentials;
- LIVE/value-bearing configuration.

Every changed file must be listed with its C1 work-package justification.

## 13. Deliverable from Fable

Return **one ZIP** containing:

1. the complete C1.5–C1.10 candidate file tree relative to repository root;
2. patch(es) against the supplied archive base, preferably one ordered patch series or one complete diff;
3. `REPORT.md` with:
   - supplied base/archive identity;
   - changed-file inventory;
   - C1.5–C1.10 mapping per file;
   - architecture decisions;
   - all tests added/changed;
   - commands actually executed;
   - exact results;
   - anything not compiled/run;
   - known limitations;
   - gates that remain unqualified;
   - required non-claims;
4. checksums for every deliverable file/package;
5. no generated secrets, private keys, credentials, build caches, `.git`, target directories or unrelated project files.

If a local environment prevents compilation/testing, label the package `IMPLEMENTED / NOT COMPILED` or the exact narrower status. Never infer PASS from code review alone.

## 14. Local integration contract after Fable returns

The returned ZIP is not merged blindly.

Local procedure remains:

1. verify archive/checksums/base;
2. inspect scope/diff/secrets;
3. `git apply --check` or equivalent isolated comparison;
4. integrate only intended C1 files;
5. qualify **C1.5 first**;
6. only after C1.5 PASS qualify C1.6;
7. then C1.7;
8. then C1.8;
9. then C1.9;
10. then C1.9b;
11. only then C1.10 freeze;
12. only after C1.10 PASS/FROZEN may C2 begin.

A failure at any gate stops downstream acceptance even if later-gate code is already present.

## 15. Completion boundary

The requested Fable tranche is complete when the repository contains a coherent candidate implementation for C1.5–C1.10 plus the required tests/tooling and a truthful implementation report.

It is **not** complete merely because files were written, and it is **not C1 frozen** until the local sequential acceptance process succeeds.
