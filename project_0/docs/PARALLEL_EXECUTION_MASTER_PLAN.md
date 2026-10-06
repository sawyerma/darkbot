# Project 0 — Parallel Execution Master Plan

**Status:** ARCHITECTURE PLAN — IMPLEMENTATION NOT STARTED  
**Date:** 2026-10-06  
**Baseline:** `main` at clean pre-parallel Project-0 FIX-139 content  
**Rule:** Project 0 changes are made on `main` only. No feature/hardening branches.

---

# 0. Why this plan exists

FIX-138/139 established that the serial candidate execution pipeline is materially slower than exact Health.

Measured evidence before any multithreading implementation:

```text
Health average             ≈ 3.39 µs
Pipeline average           ≈ 1.57 ms
Pipeline max               ≈ 2.654 s
```

The next architecture step must therefore remove remote-I/O and independent candidate work from the serial detection drain **without weakening canonical-state correctness**.

This document is the binding design before implementation.

No implementation change is allowed until this plan is reviewed against:

- `PHASENPLAN.md`
- `AUDIT_HANDOFF_2026-10-05.md`
- `MEASUREMENT_LEDGER.md`
- current `main`
- all existing FIX-134/135/137/138/139 invariants

---

# 1. Non-negotiable architecture

Project 0 does **not** use a “more threads everywhere” design.

The rule is:

```text
parallelize only work that is independent
+
keep causally ordered canonical state under one owner
+
never let a worker mutate canonical state
+
bound every queue and remote-I/O class
+
measure each stage before increasing concurrency
```

Target topology:

```text
                        ┌──────────────────────────┐
WSS / Snapshot / RPC -> │ CANONICAL STATE OWNER    │
                        │                          │
                        │ apply ordering           │
                        │ fork/root                │
                        │ recovery                 │
                        │ revision assignment      │
                        │ reverse index            │
                        │ candidate admission      │
                        └────────────┬─────────────┘
                                     │ immutable work
                                     ▼
                         CPU PREPARATION STAGE
                         bounded / parallel
                                     │
                                     ▼
                         EXECUTION ADMISSION
                         per-account single-flight
                                     │
                    ┌────────────────┼────────────────┐
                    ▼                ▼                ▼
                worker 0         worker 1         worker N
                    │                │                │
                    ├─ planning      ├─ planning      ├─ planning
                    ├─ build         ├─ build         ├─ build
                    ├─ fee RPC       ├─ fee RPC       ├─ fee RPC
                    └─ simulation    └─ simulation    └─ simulation
                    │                │                │
                    └────────────────┼────────────────┘
                                     ▼
                              RESULT QUEUE
                                     │
                                     ▼
                        CURRENT-STATE VALIDATION
                        generation/revision/hash
                                     │
                                     ▼
                          DETERMINISTIC COMMIT
                                     │
                                     ▼
                         Shadow stop / LIVE gate
```

---

# 2. Canonical State owner

Exactly one owner mutates:

- `StateCore`
- fork/root state
- Recovery state
- ProgramData upgrade latch
- reverse indices
- causal input revisions
- candidate cache
- admission ownership
- deterministic ledger/watch/safety commit

No worker receives:

```text
&mut StateCore
Mutex<StateCore>
RwLock<StateCore>
Arc<Mutex<StateCore>>
```

Remote work must never hold a canonical-state lock because no such lock should exist.

---

# 3. Candidate single-flight contract

Per Marginfi account:

```text
max 1 execution job in flight
+
max 1 newest pending causal revision
```

If revisions arrive while revision N is executing:

```text
N executing
N+1 arrives
N+2 arrives
N+3 arrives
```

state becomes:

```text
inflight = N
pending  = N+3
```

N+1 and N+2 do not each create full remote work.

When N finishes:

- if N is still current: commit;
- if canonical state changed: discard N;
- if a newer pending revision exists: launch exactly the newest pending revision.

---

# 4. Work identity

Every asynchronous candidate job carries:

```text
account
execution generation
causal input revision
trigger slot
evaluation slot
T_rx
canonical fingerprint
snapshot identity
```

Every returned result carries the same identity.

A result may be committed only if:

```text
generation == current generation
account still owns this in-flight identity
canonical fingerprint still matches
current causal revision <= result causal revision
state remains tradable
no explicitly newer pending account revision supersedes it
```

Otherwise it is stale.

---

# 5. Recovery is a hard generation barrier

Recovery increments:

```text
execution_generation
```

All asynchronous work from an older generation is invalid.

Recovery must clear or invalidate:

- candidate ready queues
- candidate pending maps
- execution ownership
- newest-pending map
- projection caches
- projection pending maps
- integration probe work
- oracle projection work
- prepared candidate caches tied to old state

An old worker result may never manufacture new work after Recovery.

---

# 6. CPU architecture on Apple M-series

Do not hard-code a specific M4 core count.

At startup inspect available CPU topology.

For current Project-0 work, useful CPU acceleration applies to:

- parallel exact Health sweeps
- immutable candidate preparation
- dependency-key derivation
- fingerprinting
- transaction construction/validation where independent
- deterministic local calculations

A dedicated bounded CPU pool is used.

Initial qualification rule:

```text
CPU preparation threads
= measured topology-derived value
```

Do **not** blindly use every core for every stage.

The State/Ingress owner must retain scheduling headroom.

Performance-core / efficiency-core policy is measured, not guessed:

1. baseline with OS scheduler;
2. record CPU utilization and stage latency;
3. only add Darwin QoS classes if they measurably improve tail latency;
4. no false hard-core-pinning claim on macOS.

---

# 7. GPU / Neural Engine

GPU and Neural Engine are excluded from the online liquidation hotpath unless profiling later proves a suitable workload.

Current hotpath consists primarily of:

- branch-heavy account decoding
- fixed/integer protocol math
- hashing
- transaction construction
- signing
- network waits

These are not currently justified GPU/ANE workloads.

GPU/ANE remain candidates only for:

- offline replay
- large research sweeps
- statistics
- ML/research workloads

No Metal/CoreML dependency is introduced into the LIVE decision path without a measured proof.

---

# 8. Network architecture

Network speed is obtained from persistent transport and concurrency, not from creating arbitrary threads.

## 8.1 Jupiter

Required:

- one reusable HTTP client / connection pool per endpoint class;
- persistent keep-alive/TLS reuse;
- no DNS/connect/TLS setup in the candidate hotpath;
- bounded concurrent requests;
- no OS-thread creation per quote batch in final architecture.

The initial implementation should use a persistent bounded Jupiter execution stage.

## 8.2 Solana fee RPC

`getFeeForMessage` must not block the canonical owner.

Fee requests run concurrently through a bounded remote-I/O stage.

## 8.3 Simulation

`simulateTransaction` / projection simulations must not block the canonical owner.

Simulation concurrency is bounded separately from CPU concurrency.

## 8.4 WSS

WSS remains Control/Long-Tail ingress.

WSS session parsing remains isolated from execution work.

No network optimization may weaken:

- Gap halt
- StreamReady barrier
- Recovery
- subscription completeness

## 8.5 NIC / kernel bypass

On macOS with HTTPS/WSS RPC providers, do not implement fake “direct NIC” optimization.

Meaningful local network controls are:

- wired Ethernet for measurements;
- no VPN for benchmark runs;
- persistent connections;
- no DNS/TLS setup in hotpath;
- TCP_NODELAY where supported/default;
- provider geography;
- bounded connection pools;
- zero ingress overflow.

AF_XDP/DPDK/RSS/IRQ-affinity belong only to a future Linux/node architecture if measurements justify them.

---

# 9. Snapshot design

Worker execution must use immutable input.

Do not copy the entire canonical state per candidate.

Target design:

```text
generation-scoped immutable shared state
+
candidate-specific account data
+
small projection overlay where needed
```

The exact representation is selected only after measuring copy/hash cost.

First implementation may use a safe owned minimal snapshot, but it must have telemetry for:

- accounts copied per candidate
- bytes copied
- snapshot-build latency
- shared-data duplication

If copying becomes material, migrate to shared immutable `Arc` records or generation snapshots.

---

# 10. One preparation implementation

There must not be two subtly different implementations for serial and parallel preparation.

Create one pure/read-only preparation function for semantics shared by both:

```text
exact Health
dependency discovery
candidate key set
canonical fingerprint
snapshot construction
local eligibility
```

Projection-specific logic wraps that function rather than reimplementing it.

This is mandatory before parallel preparation is considered complete.

---

# 11. Persistent executors only

Do not create OS threads per candidate or per Jupiter quote batch.

Persistent executors:

```text
CPU prep pool
candidate execution pool
Jupiter I/O pool
fee-RPC pool
simulation pool
projection pool where measurement requires >1
```

Each has:

- fixed/bounded concurrency
- bounded queue
- queue-depth telemetry
- submit/completion counters
- timeout/error counters

No unbounded `spawn`.

---

# 12. Backpressure

Backpressure must occur before expensive work.

If downstream execution capacity is exhausted:

- do not repeatedly rebuild Health/snapshot/fingerprint;
- retain newest candidate revision;
- do not spin;
- do not silently drop;
- keep ingress processing available.

Queue-full telemetry is a qualification failure if sustained.

---

# 13. Deterministic result commit

Worker completion order is nondeterministic.

Business-state effects are not.

Returned worker results are classified on the owner thread:

```text
ACCEPT
DROP_OLD_GENERATION
DROP_FOREIGN_INFLIGHT
RETRY_NEWEST_PENDING
RECHECK_CURRENT_STATE
```

Only ACCEPT reaches:

- opportunity record
- ledger
- winner watch
- future Safety reservation
- future send arbitration

---

# 14. LIVE remains disabled

During all parallelism implementation:

```text
P0_MODE=live
→ startup rejection
```

Parallelization must never be tested first with value-bearing execution.

Shadow must still build/sign/simulate the exact intended transaction path, then stop before submission.

---

# 15. Required telemetry

## Canonical owner

```text
ingress_backlog
ingress_overflow
state_apply latency
candidate admission rate
candidate pending
candidate coalesced
```

## CPU preparation

```text
prep_jobs
prep_completed
prep_queue_depth/max
prep_latency avg/p50/p95/p99/max
snapshot_bytes
snapshot_accounts
```

## Execution

```text
exec_dispatched
exec_completed
exec_inflight
exec_max_inflight
exec_pending_latest
exec_stale
exec_queue_depth/max
exec_latency avg/p50/p95/p99/max
```

## Remote stages

Separate timings and concurrency for:

```text
Jupiter
fee RPC
simulation
projection
```

No optimization decision is made from aggregate pipeline timing alone once stage telemetry exists.

---

# 16. Required tests before Mainnet Shadow

Mandatory regression tests:

1. two different accounts execute concurrently;
2. one account never owns two concurrent execution jobs;
3. newest pending revision replaces older pending revisions;
4. old-generation result cannot clear newer in-flight ownership;
5. Recovery invalidates all old execution/projection work;
6. changed fingerprint cannot commit;
7. changed causal revision cannot commit;
8. queue-full never silently loses a candidate;
9. backpressure does not busy-loop expensive preparation;
10. serial and parallel preparation produce identical semantics;
11. different worker completion orders produce identical committed outcome;
12. worker panic/disconnect fails closed;
13. shared HTTP client preserves deterministic route-result mapping;
14. Shadow path cannot submit;
15. existing FIX-134/135/137 regressions remain PASS.

---

# 17. Implementation sequence

No large all-at-once patch.

## MT0 — Freeze baseline

- baseline = FIX-139 Project-0 content on current `main`;
- full tests + Clippy;
- record exact baseline commit;
- no architecture change.

## MT1 — Extract pure candidate preparation

- no threads yet;
- remove duplicated preparation semantics;
- regression parity tests;
- output must be identical to current serial path.

## MT2 — Define work/result identity and stale policy

- generation/revision/fingerprint types;
- pure stale-result action;
- Recovery invalidation tests;
- still no concurrent execution.

## MT3 — Persistent CPU preparation pool

- bounded pool;
- immutable state reads only;
- telemetry;
- compare serial vs parallel output byte-for-byte/field-for-field.

## MT4 — Cross-candidate execution pool

- per-account single-flight;
- newest pending;
- no network pooling changes yet;
- prove concurrent accounts / same-account exclusion.

## MT5 — Persistent Jupiter I/O stage

- shared persistent HTTP client;
- no per-call threads;
- bounded concurrency;
- deterministic result ordering.

## MT6 — Fee RPC + simulation concurrency

- separate bounded capacities;
- no canonical owner blocking;
- exact signed transaction preserved.

## MT7 — Projection generation hardening

- candidate projection and oracle projection generation-bound;
- bounded concurrency only if measurements show serial projection backlog.

## MT8 — M4 scheduling qualification

Measure configurations:

```text
CPU prep threads
execution workers
Jupiter concurrency
fee concurrency
simulation concurrency
```

Select configuration from throughput + p99/max latency + queue drain, not CPU utilization alone.

## MT9 — 3-minute Mainnet Shadow

PASS requires:

```text
ingress_overflow = 0
sink_overflow = 0
no stale commit
no queue-loss
no sustained downstream queue-full
pending trend drains / does not stabilize at old backlog
```

## MT10 — 10-minute stability qualification

Only after MT9 PASS.

Then and only then resume:

- executable-floor economics;
- winner fixtures;
- Drift;
- historical replay;
- competitive Shadow;
- controlled LIVE.

---

# 18. Git discipline

Project 0 uses `main` only.

Before every write:

1. fetch current `main` SHA;
2. inspect changed paths;
3. verify unrelated BSC files are untouched;
4. make one logically bounded commit;
5. update `main` with expected-SHA lease;
6. if lease fails, stop and inspect;
7. never force-reset `main`;
8. never create Project-0 feature branches unless the user explicitly changes this policy.

Each commit must be independently revertible.

---

# 19. Stop conditions

Stop implementation immediately if:

- a change requires weakening canonical-state correctness;
- a queue becomes unbounded;
- a worker needs mutable canonical state;
- stale-result identity is ambiguous;
- a performance patch duplicates protocol semantics;
- BSC files would be touched;
- `main` lease changed unexpectedly;
- tests reveal semantic divergence.

Correctness takes priority over throughput.

---

# 20. Success definition

Parallelization is complete only when all are true:

```text
canonical state remains single-owner
all independent expensive work is moved off owner
CPU preparation is bounded and measured
remote I/O is persistent and bounded
per-account single-flight is proven
Recovery is a hard generation barrier
no stale result commits
no candidate loss
no sustained queue backlog
Shadow parity holds
LIVE still disabled
```

Only after those proofs is Project 0 allowed to continue to economics/competitive/live qualification.
