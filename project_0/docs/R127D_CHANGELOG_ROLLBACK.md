# R127d — Candidate Cache / Re-Arm Layer

Date: 2026-10-05

## Why this change exists

R127c successfully removed the catastrophic per-dependency fanout before the pending queue:

- max_pending fell from ~78k to 6,279;
- ingress remained lossless;
- batch account dedupe reached 39.6%;
- 91.0% of exact Health checks were dropped as healthy before downstream work.

The remaining bottleneck was different:

- 1,113,854 Accounts were still forwarded into Projection/Sizing/Economics in the 10-minute run;
- most were persistent liquidatable/error Accounts seen repeatedly across many 5 ms batches;
- final pending remained 6,154, so R127c was PARTIAL rather than PASS.

R127d therefore adds the P2 Candidate Cache/re-arm layer required by PHASENPLAN instead of
changing the batch architecture again.

## Frozen rollback point before R127d

Exact pre-R127d tree:

- commit: `dde69699a9943e376ca4343d454f8e3177ee9b36`
- archive branch: `archive/r127c-batch-partial-20261005`

This tree contains the complete R127c batching implementation and its documentation but no
R127d candidate cache.

## R127d implementation commits

### `1047896a63e2a2f3c63d733b8893dfd307057f02`

Adds the Candidate Cache to `darkbot-runner/src/main.rs`.

Behavior:

1. Exact Marginfi Health is still recomputed on every affected R127c batch.
2. Healthy Accounts never enter the downstream candidate queue.
3. First transition into `Liquidatable` or `Error` is forwarded immediately.
4. A real canonical Bank write force-rearms all Accounts using that Bank.
5. Persistent unchanged Liquidatable Accounts are periodically re-evaluated downstream after a
   bounded slot interval rather than every 5 ms batch.
6. Persistent Error Accounts use a shorter bounded retry interval.
7. A pending-queue soft cap prevents periodic repeat work from recreating the old backlog.
8. Transition/forced work bypasses the repeat soft cap; it is never silently suppressed.
9. Cache is cleared on recovery/reconciliation rebuild.

Defaults:

- `P0_CANDIDATE_RECHECK_SLOTS=64`
- `P0_CANDIDATE_ERROR_RECHECK_SLOTS=16`
- `P0_CANDIDATE_REPEAT_SOFT_CAP=1024`

The cache does **not** replace Health. It only controls repeated admission into the expensive
Projection/Sizing/Economics pipeline after exact Health has already been recomputed.

### `a319e0a13296aa9c6aa57d169d15fe8e42fdc818`

Extends `run_r127_qualify.py` with Candidate Cache evidence and gates.

New fields:

- `candidate_cache_entries`
- `candidate_cache_suppressed`
- `candidate_cache_suppression_ratio`
- `candidate_cache_softcap_suppressed`
- `candidate_cache_transition_forwards`
- `candidate_cache_periodic_forwards`
- `candidate_cache_forced_forwards`

Qualification now requires:
- candidate cache populated;
- repeated candidates actually suppressed;
- transition or forced-forward behavior observed;
- existing batch/ingress/pending gates still hold.

### `82ea2f75d6ef3ddd227a907ed92667dee54105ed`

Stores the completed R127c Mainnet Shadow PARTIAL result as structured evidence:
`project_0/docs/evidence/R127c_20261005T161806Z.json`.

## Runtime ordering after R127d

```text
canonical ingress
→ StateCore update
→ affected Banks dirty
→ <=64 events OR <=5 ms batch
→ Bank→Account union
→ Account dedupe
→ exact canonical Marginfi Health in parallel
→ Healthy: DROP
→ Liquidatable/Error:
     Candidate Cache / re-arm decision
     ├─ transition: forward now
     ├─ Bank invalidation: forward now
     ├─ periodic due + capacity: forward
     └─ unchanged/recent: suppress repeat
→ pending candidate queue
→ Projection
→ Sizing
→ Economics
→ Build/Sign/Sim
```

## Why this is not a blind cooldown

A blind wall-clock cooldown would risk hiding state transitions. R127d instead:

- recalculates exact Health on every affected batch;
- forwards state-class transitions immediately;
- force-rearms on canonical Bank writes;
- uses Solana slot-based periodic rechecks for persistent candidates;
- uses shorter rechecks for fail-closed errors;
- keeps direct MarginfiAccount writes on the existing direct detection path.

The periodic recheck exists because route/fee/economic conditions may change even when the
Marginfi Health class remains Liquidatable.

## Rollback

### Inspect/reproduce R127c without Candidate Cache

```bash
git fetch origin
git switch --track origin/archive/r127c-batch-partial-20261005
```

### Revert only R127d runtime/qualifier changes on main

Revert in reverse order while normally keeping evidence/docs:

```bash
git revert a319e0a13296aa9c6aa57d169d15fe8e42fdc818
git revert 1047896a63e2a2f3c63d733b8893dfd307057f02
```

The R127c evidence commit `82ea2f75...` should normally remain because it records the measured
reason for adding R127d.

## Expected R127d qualification effect

Compared with R127c:

- `sweep_checks` may remain large because exact Health is intentionally still recomputed;
- `sweep_forwarded` should fall sharply;
- `candidate_cache_suppressed` should be large and non-zero;
- `max_pending` should fall materially below R127c's 6,279;
- `pending_final` should drain below 1,000, ideally near zero;
- ingress overflow and sink overflow must remain zero.

R127d must not be promoted on cache suppression alone. A PASS still requires lossless ingress,
active batching, bounded/down-draining pending work, and observed transition/forced forwarding.
