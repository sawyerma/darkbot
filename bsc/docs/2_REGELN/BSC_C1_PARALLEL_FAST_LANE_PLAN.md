# DarkBot BSC — C1 Parallel Fast Lane / M4 Development Architecture

**Status:** normative C1 implementation plan, 2026-10-06  

**Integration checkpoint 2026-10-08 (REV2):** C1.5–C1.10 candidate is integrated
in the project and compiled with local regression checks. This is not a new
phase acceptance. Historical C1.1–C1.4a results remain recorded; changed
interfaces need the reruns listed in the [qualification runbook](BSC_C1_5_TO_C1_10_QUALIFICATION_RUNBOOK.md).
Acceptance order remains C1.5 → C1.6 → C1.7 → C1.8 → C1.9 → C1.9b → C1.10;
C2 stays blocked until C1.10. See the [REV2 integration report](../3_BEWEISE/BSC_C1_REV2_INTEGRATION_REPORT_20261008.md).
Actual successful acquisition remains **T0 → T_buy < 1,000,000 ns**, with the
original producer evidence and full 15-second capture required by the masterplan.

**Basis:** `main` source checkpoint `26725a8607435dd15c138231d9cfdd7d84d0f609`; pinned bsc-geth v1.7.8 at `cdb7548b5baacfdae92f9f63437d6456411665f3`.  
**Governing documents:** `BSC_PRODUCTION_BUILD_MASTER_PLAN.md`, `BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`, `BSC_BIRTH_ENTRY_ARCHITECTURE.md`.  
**Authorization:** C1 construction only. Production LIVE remains blocked.  
**Product target:** actual successful acquisition with strict `T_buy - T0 < 1,000,000 ns`; still NOT YET PROVEN.  
**Local target:** complete actionable ingress → actual selected transport handoff <1 ms, optimize toward <0.5 ms.

This document integrates the reviewed K1–K9 corrections plus three final clarifications: (A) distinguish packet-decode timing from the actionable ingress boundary after required basic validation, (B) treat M4 topology only as an initial scheduling hint rather than a hard S/P qualification threshold, and (C) keep Testnet hook/arrival-latency evidence separate from Mainnet coverage qualification.

## 1. Phase boundary

C1 owns only the native client/Fast-Lane boundary:

- pinned bsc-geth source/build;
- earliest useful complete-TX ingress;
- txpool-acceptance provenance;
- canonical/reorg provenance hooks;
- local signed-TX outbound;
- mmap/SPSC wire, ownership, ordering, backpressure, restart/shutdown/corruption;
- native hook→publish→Rust receive measurement;
- local-geth outbound-handoff measurement.

C1 does **not** create a second chain-state engine. C2 remains the sole pending/canonical state/reconciler owner. C3 remains the integrated geth→Rust→venue/state/economics/admission→build/sign→adapter qualification. Parallel 48Club/BlockRazor runner execution belongs to a later Unit-5 Runner Rev4 under C3, not to C1.

## 2. Current defect that C1 must remove

Wire V1 exposes one geth→Rust `ingress` SPSC ring to multiple producer goroutines:

| Record | Current producer |
| --- | --- |
| `EARLY_TX_SEEN` | each ETH peer handler; multiple handlers may run concurrently |
| `TXPOOL_ACCEPTED` | `txBroadcastLoop`; one goroutine |
| `CANONICAL_HEAD`, `REORG` | chain insertion under `chainmu` |
| `OUTBOUND_PEER_HANDOFF` | current single outbound loop |

The V1 `ring.publish` reads `write_sequence`, writes one slot and stores `write+1` without multi-producer arbitration. Two peer handlers may therefore select the same sequence/slot. V1 is not a valid multi-producer ring and concurrent-peer V1 latency is not an accepted baseline.

V1 integrity also covers only the payload; a mixed/torn header+payload record can evade that payload-only CRC. C1.1 must reproduce the ownership defect before V2 replaces it.

## 3. Target concurrency model

The rule is:

> Parallelize independent work; keep one owner for each causally ordered state transition; never introduce a shared blocking hot-path resource merely to increase thread count.

Target data flow:

```text
geth peer handler 0 ──> peer SPSC lane 0 ─┐
geth peer handler 1 ──> peer SPSC lane 1 ─┤
geth peer handler 2 ──> peer SPSC lane 2 ─┤
...                                       ├──> Rust HOT0
geth peer handler N ──> peer SPSC lane N ─┘      poll/decode/state/econ/admission/build/sign
                                                   │
                                                   ├──> local-geth egress SPSC  [C1]
                                                   ├──> 48Club transport         [current Unit 5 sequential;
                                                   └──> BlockRazor transport      parallel Rev4 only in C3]
```

Each lane has exactly one producer and one consumer. No global ready bitmap is written by every producer. No common MPSC head is introduced into `EARLY_TX_SEEN`.

## 4. Wire V2 layout

Wire V2 is one mmap arena, not one file per hot peer:

```text
fastlane-v2.mmap
  SuperHeader
  LaneDirectory
  Peer lane 0..N
  Txpool lane
  Chain lane
  Outbound-evidence lane(s)
  Rust→geth egress lane
```

Each lane is an independent bounded SPSC ring.

### 4.1 Cacheline ownership

The wire uses a fixed 128-byte alignment/stride contract for V2. The runtime still records the host cache-line value for evidence, but V2 layout does not change dynamically per host.

Each lane header separates hot ownership:

| Cache region | Writer | Fields |
| --- | --- | --- |
| producer-owned | producer | `write_sequence`, `dropped_full`, `oversize`, producer status |
| consumer-owned | consumer | `read_sequence`, `corrupt_records`, `drained_epoch` |
| lifecycle | allocator/recovery only | `generation`, `lane_epoch`, owner binding, FREE/BOUND/CLOSING, `final_write_sequence`, shutdown |

Slot stride is a multiple of 128 bytes. Producer and consumer sequence counters must not share the same hot cacheline.

### 4.2 Ordering

Ordering is guaranteed only within one lane. Cross-lane processing order is not inferred from ring position.

Every observed event carries a compatible monotonic timestamp and `clock_id`. Ties/clock uncertainty remain explicit. Canonical/reorg evidence must never be reordered by pretending that different SPSC indices form a global sequence.

## 5. Lane ownership and lifecycle

### Peer lanes

One active ETH peer handler owns one peer lane exclusively for its connection lifetime.

```text
CONNECT
  allocate FREE lane outside hot path
  lane_epoch += 1
  bind peer identity
  state = BOUND

RUN
  peer handler is sole producer

DISCONNECT
  producer stops
  set final_write_sequence
  state = CLOSING

DRAIN
  Rust consumes to final_write_sequence or quarantines
  Rust sets drained_epoch = lane_epoch

REUSE
  allocator may set FREE only after drained_epoch == lane_epoch
```

Peer connect/disconnect allocation may use a mutex because it is outside the transaction hot path.

The configured lane pool has a fixed qualified upper bound plus explicit churn reserve. Requesting more peers than the qualified bound fails startup. Runtime exhaustion is never silent: the peer may continue without Fast Lane only if a GAP/health signal makes the missing Fast-Lane coverage explicit; it cannot be counted as covered C1 ingress.

Rust restart invalidates the generation and quarantines old lanes.

### Other producer classes

- `TXPOOL_ACCEPTED`: dedicated Txpool SPSC, sole producer `txBroadcastLoop`.
- `CANONICAL_HEAD` + `REORG`: dedicated Chain SPSC. C1.8 must prove every patched publisher is serialized by the pinned client's chain-write ownership before accepting this as one producer.
- `OUTBOUND_PEER_HANDOFF`: if local-geth becomes parallel per peer, each peer worker gets its own evidence SPSC, or each reports via its own SPSC to one evidence publisher preserving the worker's original handoff timestamp.
- `OUTBOUND_SIGNED_TX`: Rust→geth egress remains one SPSC produced by HOT0.

No new record type may be added to a lane unless its producer ownership is identical.

## 6. Exact ingress boundaries

C1 records two distinct early boundaries. They must never be conflated.

### T_packet_decoded

`T_packet_decoded` is captured once immediately after a complete Transactions/PooledTransactions packet has been decoded into transaction objects.

It is diagnostic. It may precede the pinned client's required basic transaction validation and therefore is **not automatically actionable `T_ingress`**.

### T_ingress / L0

`T_ingress` is the earliest locally observed **actionable** boundary after the required pinned-geth basic validation for the selected hook has succeeded, and before Fast-Lane serialization/publication.

The existing `handleTransactions` performs relevant basic validation/duplicate checks. C1 may move the hook earlier only if the resulting per-TX boundary is proven not to authorize a transaction that the selected geth validity contract would reject.

C1 records:

```text
T_packet_decoded
T_ingress
L1 = SPSC Release commit
L2 = Rust Acquire observes record
```

This permits separate reporting of:

```text
packet decode → actionable validation
T_ingress → L1
L1 → L2
T_ingress → L2
```

No optimization may call `T_packet_decoded` `T_ingress` merely to produce a smaller number.

## 7. geth producer path

After basic validation establishes `T_ingress`, the peer handler performs the cheapest safe conservative filter before publication:

1. destination matches the frozen supported venue-address superset;
2. selector matches the frozen supported launch-selector superset;
3. only then serialize and publish the candidate into that peer's SPSC lane.

The geth filter is never venue/economic/admission authority. Rust remains authoritative. The geth filter is a conservative superset and its generation is bound to startup configuration.

Coverage protection: the full txpool-evidence path remains available in the background so missed qualifying launches can be detected. Any false negative in the geth prefilter is a qualification failure.

Direct serialization into the mmap slot is optional optimization work **after** the filtered path is measured. It is not required to make V2 correct. If implemented, it must produce the exact canonical transaction bytes and use fixed bounded storage; oversize fails closed with evidence.

## 8. Rust HOT0 consumer

One permanent Rust thread owns the causal candidate path:

```text
poll active peer lanes
→ check chain evidence before admission
→ select earliest actionable candidate
→ minimal factual venue decode
→ resident-state/economics lookup
→ Unit-4 admission
→ finalize BUY
→ sign once
→ emit immutable submission descriptors
```

HOT0 has no:

- filesystem/DB access;
- DNS/connect/handshake;
- synchronous RPC/HTTP reads;
- writer/logger waits;
- sleeps;
- reconciliation wait;
- blocking recorder path.

A thread pool is intentionally not inserted into decode→state→admission→build→sign because those steps are causally dependent for one candidate and a queue hop/cache transfer would add tail latency.

## 9. Multi-lane polling

Rust keeps the active-lane list locally. There is no global producer-written ready bitmap.

Round-robin starts after the last successful lane to prevent one noisy peer from starving others. Producer/consumer sequence values may be cached locally and reloaded only when the local cached relation requires it, while preserving Acquire/Release correctness.

Before an economic admission, HOT0 services relevant Chain-lane evidence so a launch already known canonical/reorged is not treated as pending merely because the peer lanes and chain lane no longer share one physical queue. This does not make HOT0 a second reconciler; C2 still owns coherent chain state and recovery.

Duplicate launch transactions received from multiple peers are deduplicated by hash/identity in the bounded Core path; duplicate slots are released immediately.

## 10. Zero-copy receive and bounded intent copy

The production Rust receive path replaces the V1 `payload.to_vec().into_boxed_slice()` allocation with a borrowed slot guard:

```text
Acquire committed_sequence
→ validate header + payload integrity
→ expose BorrowedRecord<'lane>
→ decode/decision directly on mmap bytes
→ for an admitted candidate copy required immutable bytes once into a preallocated IntentSlot
→ release guard
→ read_sequence Release
```

The guard never leaves HOT0. The producer may reuse the slot immediately after `read_sequence` advances.

The preallocated `IntentSlot` contains, as required by the selected mode:

- intent/generation/policy identity;
- nonce;
- BUY hash + signed BUY bytes + length;
- launch hash + exact raw launch bytes + length for public full-launch ordered-bundle modes;
- adapter permit mask.

All adapters reference this immutable slot. No second nonce and no second economic BUY are created for redundant submission.

## 11. Integrity

Wire V2 changes integrity from V1 payload-only IEEE CRC to a versioned V2 CRC32C contract covering the exact serialized metadata fields plus payload.

The CRC byte domain is explicitly specified; implementation-language struct padding is never included implicitly. `committed_sequence` and the CRC field itself are excluded according to the frozen byte-domain specification.

Go and Rust must share golden vectors.

Implementation:

- startup-selected accelerated CRC32C path where supported;
- deterministic table fallback;
- no per-record CPU-feature discovery;
- no disabling integrity to improve benchmark numbers.

The transaction hash remains semantic identity evidence, not a replacement for the wire corruption check.

## 12. M4 development scheduling

macOS/M4 is the development and C1 diagnostic host, not the final C3 production qualification host.

At startup, the runtime records available Apple performance levels and relevant cache/topology data. These observations influence the **initial** thread profile only. They are not hard acceptance thresholds.

### Latency-critical ownership

Priority order:

1. Rust HOT0.
2. geth peer ingress/validation responsible for `T_ingress`.
3. local-geth egress coordinator/peer send workers when enabled.
4. builder transport workers only when Unit-5 Runner Rev4 is introduced in C3.
5. ACK/response/maintenance.
6. capture/logging/reconciliation/background.

macOS QoS may be applied to permanent latency-critical OS threads, but is an advisory scheduling mechanism, not a claim of hard P-core pinning.

### Profile selection

Two benchmarkable scheduling profiles are retained:

- **Profile S:** current qualified Unit-5 builder execution remains sequential; only C1 local-geth work is parallelized where C1 specifies it.
- **Profile P:** parallel builder workers, introduced only by Unit-5 Runner Rev4 in C3.

Core count does **not** decide qualification by itself. A smaller M4 may still benefit from Profile P and a larger M4 may not. Hardware topology selects a reasonable initial experiment; measured max/tail and full-path results decide the retained profile.

No per-BUY thread/goroutine creation is allowed in either profile.

## 13. GPU / Neural Engine

GPU and ANE are excluded from `T_ingress → T_send`.

The production hot work is one small transaction, bounded decoding/state lookup, one quote/admission and one signature. Dispatch, synchronization and memory-transfer overhead would create a new latency boundary rather than remove one.

GPU/ANE remain allowed for offline replay, research sweeps, statistics and capture analysis.

## 14. Local-geth outbound

C1 replaces the current sequential peer send bottleneck with persistent pre-created peer send ownership only if the C1 measurement shows that fan-out is required for the selected local-geth scope.

Target model:

```text
Rust egress SPSC
→ geth outbound coordinator
→ preselected persistent peer worker A
→ preselected persistent peer worker B
→ ...
```

No goroutine is created per BUY. Slow peer A cannot delay first handoff to B.

Peer-list maintenance is moved outside the BUY dispatch. The current per-record `allNonEVNPeers()` selection/copy/lock must not remain an unmeasured hot-path dependency in the qualified path.

Measure separately:

- Rust egress commit;
- first selected peer transport handoff;
- every selected peer transport handoff;
- failed/unknown peer sends.

Each parallel worker publishes evidence through ownership-safe SPSC evidence transport with its original worker timestamp.

## 15. Builder transports and phase boundary

The already qualified builder connections remain warm, non-blocking and `TCP_NODELAY` with DNS/connect/TLS handshake outside the dispatch path.

C1 does **not** rewrite frozen Unit-5 builder dispatch.

Parallel 48Club + BlockRazor transport execution requires Unit-5 Runner Rev4 and belongs to C3. Its future contract is:

```text
HOT0:
  begin_send once
  mint/consume adapter permits according to Unit 4/5 contract
  publish descriptors to one permanent worker per adapter

worker:
  owns one transport exclusively
  frame/prepare
  final deadline check
  first socket write
  flush to the qualified boundary
  return state + original timestamps
```

Identical signed BUY bytes/nonce are used. One adapter never waits for another's ACK. UNKNOWN and no-blind-retry semantics remain per adapter.

This later Rev4 must requalify Q5B binding and the affected Unit-5 runner tests before C3 can use Profile P.

## 16. Canonical/reorg safety

C1 performance work must not alter:

- chain mutation;
- common-ancestor determination;
- tx lookup mutation;
- head state;
- canonical ordering;
- reorg ordering.

C1 only publishes evidence at reviewed hooks. C1.8 must prove the claimed single producer for every Chain-lane hook in the pinned geth, including all relevant insertion/canonicalization paths.

C2 remains the only Rust owner that turns those observations into coherent pending/canonical state and recovery decisions.

## 17. Network policy

On the development host:

- use wired Ethernet for latency qualification where practical;
- no VPN on the measured path;
- no DNS, connect, handshake or lazy initialization in the hot path;
- warm builder sockets;
- retain `TCP_NODELAY`;
- do not blindly enlarge socket buffers;
- `WouldBlock` after permit is UNKNOWN, never a blind retry.

Kernel/NIC specialization is not added merely because it exists. C3 on the dedicated Linux target may benchmark CPU isolation, IRQ affinity, RSS/RPS/XPS, busy polling, `SO_BUSY_POLL`, AF_XDP or similar only if normal-socket measurements identify kernel/NIC tail as a material bottleneck.

## 18. Measurement contract

C1 preserves raw samples and reports at least:

```text
T_packet_decoded
T_ingress
L1 = mmap/SPSC commit
L2 = Rust receive
local-geth first-peer handoff
local-geth all-selected-peer handoffs
```

Break down `T_ingress → L1` into filter, serialization, integrity and commit where instrumentation can do so without distorting the path.

Report sample count, min, mean, p50, p95, p99, p99.9, p99.99 only when sample support is meaningful, max, misses, queue depth, drops, oversize, corruption, HoL and measured allocations/locks.

C1 never accepts a p99 in place of max.

### Mandatory early architecture feasibility gate (C1.4a)

C1.4a is a **mandatory stop gate immediately after C1.4 and before C1.5 implementation proceeds**. Its purpose is to determine early whether the selected native ingress architecture is operating in a latency range that can still plausibly support the strict complete-path target. It is not deferred to C1.9.

Measure the actually integrated C1.4 path, with the pinned geth build and Wire V2 active:

```text
T_packet_decoded → T_ingress
T_ingress → L1
L1 → L2
T_ingress → L2
```

C1.4a must preserve raw per-sample timestamps and report sample count, min, mean, p50, p95, p99, p99.9 where supported, max, deadline-band counts, queue depth, drops, oversize, corruption, active peer-lane count, source/build/config hashes and the exact clock domain. The repeatable local harness must exercise the real C1.4 producer path with simultaneous independent peer producers; it may use a controlled local transaction fixture and the existing Rust Wire-V2 consumer. It must not substitute V1, a mock queue, a userspace timestamp taken after publication, or a synthetic arithmetic sum of component percentiles.

The authoritative early decision metric is **max observed `T_ingress → L2`**, with all samples retained:

| max `T_ingress → L2` | C1.4a classification | Required action |
| ---: | --- | --- |
| < 200,000 ns | `GREEN_STRONG_HEADROOM` | continue to C1.5 |
| 200,000..499,999 ns | `GREEN_FEASIBLE` | continue to C1.5; retain breakdown for optimization |
| 500,000..999,999 ns | `AMBER_LOW_HEADROOM` | do not proceed routinely; profile/filter/serialization/wire/consumer handoff and obtain reviewed disposition before C1.5 |
| >= 1,000,000 ns | `RED_ARCHITECTURE_FAIL` | stop C1 progression; the ingress→Rust segment alone consumes the complete local target and must be corrected/requalified |

Any dropped qualifying record, silent lane-exhaustion coverage loss, corruption, timestamp-domain incompatibility or unaccounted measurement gap makes C1.4a `FAIL` regardless of the numerical latency.

The classification is a **feasibility result only**. Even `GREEN_STRONG_HEADROOM` does not prove `T0 → T_buy < 1 ms`, Mainnet ingress advantage, provider ordering, inclusion or acquisition. It only establishes that the measured native ingress→Rust boundary leaves quantified local headroom for later C1/C2/C3 work. C1.9 remains the full native latency qualification after C1.5–C1.8 are integrated.

Required evidence artifact:

`bsc/native/evidence/c1/c1-4a-early-latency-<date>.json`

It must contain the raw-sample reference(s), exact command(s), host/build/config identity, threshold counts, max, classification and the standard C1 non-claims.

### Hook/path-early qualification

Controlled Testnet launches may measure:

```text
known launch handoff
→ packet decode
→ T_ingress per peer/source
```

and compare available ingress sources. This qualifies the selected hook/path latency and can falsify a late hook.

It does **not** prove Mainnet peer coverage, private-flow visibility or Mainnet arrival advantage. Mainnet coverage remains a later production capture/coverage responsibility and cannot be inherited from Testnet.

## 19. C1 work packages and stop gates

### C1.1 — V1 baseline correctness

- finish compile/test of source checkpoint `26725a86`;
- add a concurrent-producer regression that demonstrates the V1 ownership defect;
- use race/concurrency instrumentation where meaningful;
- do not accept concurrent-peer V1 latency as production baseline.

**Exit:** defect reproduced and historical V1 status recorded; no LIVE.

### C1.2 — Wire V2

Implement:

- one mmap arena + LaneDirectory;
- independent SPSC lane headers;
- 128-byte ownership separation;
- 128-byte slot stride;
- explicit byte-domain CRC32C over metadata+payload;
- wire version/generation/bounds/endianness/framing.

**Exit:** Go/Rust golden vectors, wrap/backpressure/corruption tests PASS.

### C1.3 — Lane lifecycle

Implement peer lane bind/close/drain/reuse, lane epochs, generation invalidation, bounded churn reserve, exhaustion/GAP visibility and restart quarantine.

**Exit:** lifecycle/churn/restart tests PASS.

### C1.4 — geth ingress producer

Implement:

- `T_packet_decoded`;
- correct actionable `T_ingress` after required basic validation;
- conservative address+selector prefilter;
- one peer producer → one lane;
- false-negative detection path through background evidence;
- direct-slot serialization only if measurement later justifies it.

**Exit:** no invalid transaction authorized by moving the hook; filter false-negative suite PASS.

### C1.4a — early architecture feasibility gate

Immediately after C1.4, before C1.5 changes the Rust receive path, run the mandatory §18 early-feasibility measurement against the real integrated C1.4 geth→Wire-V2→Rust boundary.

Required:

- simultaneous independent peer producers on their own V2 lanes;
- preserved raw samples for `T_packet_decoded`, `T_ingress`, L1 and L2;
- exact source/build/config/clock identity;
- counts for drops, oversize, corruption, queue pressure and uncovered lane exhaustion;
- max-based classification using the §18 thresholds;
- committed C1.4a evidence artifact.

**Exit:** `GREEN_STRONG_HEADROOM` or `GREEN_FEASIBLE` with zero integrity/coverage failures. `AMBER_LOW_HEADROOM` requires explicit reviewed optimization/disposition before C1.5. `RED_ARCHITECTURE_FAIL` blocks C1 progression until corrected and requalified.

### C1.5 — Rust HOT0 / zero-copy

Implement BorrowedRecord/guard, no heap copy on generic receive, bounded one-time copy of admitted launch bytes into preallocated IntentSlot, local active-lane polling and explicit cross-lane timing semantics.

**Exit:** lifetime/soundness tests, allocation measurement and exact-core semantics PASS.

### C1.6 — M4 development profile

Implement topology evidence capture and macOS QoS support for permanent threads. Retain S/P as benchmarkable profiles; do not hard-code qualification to a P-core count.

**Exit:** reproducible host/profile evidence; no claim that macOS equals Linux C3.

### C1.7 — local-geth outbound

Implement/qualify persistent local peer-send ownership without per-BUY goroutine creation, ownership-safe per-worker evidence and precomputed peer selection where applicable.

**Exit:** first/all selected peer handoff evidence, failure/slow-peer tests PASS.

### C1.8 — concurrency and state-safety stress

Test at least:

- simultaneous peer producers;
- lane ownership/reuse/exhaustion;
- disconnect/reconnect/churn;
- producer/consumer restart;
- generation mismatch;
- wraparound/full/no-overwrite;
- torn/corrupt metadata and payload;
- oversize;
- consumer stall;
- high TX burst;
- canonical/reorg under TX load;
- recorder/background load;
- local-geth slow/failing peers;
- every Chain-lane publisher's actual `chainmu`/single-producer ownership.

**Exit:** no violated ownership/state invariant.

### C1.9 — native latency qualification

Measure `T_packet_decoded → T_ingress → L1 → L2` and local-geth handoffs with compatible clocks and preserved raw samples.

**Exit:** C1 latency report with max/tail/failure denominators and exact source/build/config hashes.
The outbound evidence must be `production_devp2p_selected_peer_handoff`, traversing
actual `Peer.SendRawTransaction → p2p.Send → devp2p/RLPx`, and preserve every
worker's timestamp. `loopback_microbenchmark` and `regression_smoke` cannot
qualify C1.9. C1.9b characterizes ingress, not this outbound boundary.

### C1.9b — hook/path early qualification

Use controlled Testnet launch/handoff evidence to measure selected hook/path lateness per source.

**Exit:** hook/path latency characterized. Explicitly label Mainnet coverage/arrival advantage UNKNOWN unless separately measured on Mainnet.

### C1.10 — freeze

Freeze:

- geth base commit;
- exact final DarkBot commit and clean working-tree state;
- final C1 patch hash or deterministic complete C1 source-tree hash, checked against HEAD;
- final geth binary hash;
- Wire V2 specification/hash;
- Rust build hash;
- C1 configuration/topology profile;
- all accepted tests;
- raw measurement evidence;
- exact non-claims.

Only after C1.10 may execution proceed to C2.

## 20. Required C1 completion statement

C1 can be marked `IMPLEMENTED_TESTED/FROZEN` only when C1.1–C1.10 exits are satisfied, including the mandatory C1.4a early-feasibility gate and C1.9b hook/path qualification.

The C1 completion statement must still say:

```text
T0_TO_T_BUY_LT_1MS = NOT_PROVEN
MAINNET_INGRESS_COVERAGE = NOT_PROVEN unless separately qualified
PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED unless separately qualified
LIVE = BLOCKED
```

C2 then implements canonical/pending state and controlled recovery. C3 later integrates the full local execution chain on the dedicated Linux target, including any Unit-5 Runner Rev4 parallel-builder profile, and enforces the strict complete-local <1 ms requirement with optimization toward <0.5 ms.
