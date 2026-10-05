# R127c — Batched Long-Tail Admission: Change Log and Rollback

Date: 2026-10-05

## Purpose

R127c replaces the R127 per-dependency account fanout with a bounded, bank-level burst
admission path.

The change exists because the completed R127 Mainnet Shadow qualification proved that the
previous ordering was wrong:

- 96,825 dependency updates;
- 15,106,838 pending-queue coalesces;
- max pending 78,453;
- final pending 77,230;
- ingress remained lossless;
- exact canonical health rejected 92.1% of checked accounts as healthy.

The problem was therefore not missing dependency coverage or WSS throughput. The problem was
that account fanout happened once for every dependency update and only afterwards was work
deduplicated/filtered.

R127c moves deduplication and exact-health admission in front of the pending queue.

## Frozen rollback point

The exact pre-R127c runtime/qualifier state is:

- commit: `bdcaa41c2c41eeefdacad08e7a8285a7f1921b79`
- archive branch: `archive/r127b-pre-batch-20261005`

The archive branch was created specifically so the pre-batch implementation remains directly
addressable even if `main` advances.

Do not delete or repoint this branch while R127c is under qualification.

## R127c code-change sequence

The runtime change from the frozen rollback point to the first complete R127c implementation is
six commits:

1. `609ca943a4be70ac993dde1af400159dd4d932d4`
   - adds Rayon to `darkbot-runner`;
   - purpose: CPU-parallel exact Marginfi health evaluation for unique accounts in a batch.

2. `077a764566a48adae547a9591b31be9b97671cdf`
   - removes per-dependency account fanout from the default Long-Tail path;
   - dependency and Bank writes mark Banks dirty only;
   - flush expands the dirty-Bank union to Accounts once;
   - Accounts are deduplicated before Health;
   - canonical exact Marginfi Maintenance Health is evaluated in parallel;
   - healthy Accounts are dropped before the pending queue;
   - only liquidatable Accounts and fail-closed health errors are forwarded;
   - adds batch/fanout telemetry.

3. `abcf32376387d86960dcb414536f56be2a0e09e3`
   - makes batching real rather than opportunistic;
   - flush condition is bounded by either:
     - `P0_SWEEP_BATCH_MAX_EVENTS` (default 64), or
     - `P0_SWEEP_BATCH_MAX_AGE_NS` (default 5,000,000 ns = 5 ms);
   - while a batch is open, ingress polling uses a 1 ms timeout so a quiet stream cannot turn
     the 5 ms batch bound into the old 250 ms idle wait.

4. `fcd262e74e545e5f8510b1a4de0455b53878b581`
   - removes the per-candidate `protocol_luts: active=... addresses=...` terminal log;
   - no LUT behavior or transaction construction semantics changed.

5. `28474060ce44a3a483c6be6f905044293739d623`
   - updates `run_r127_qualify.py` for the batch architecture;
   - reports batch count, raw Bank→Account references, unique Accounts, dedupe ratio, forwarded
     Accounts, max unique batch and max dirty Banks;
   - requires dependency updates and batch execution;
   - max pending >10,000 or final pending >1,000 cannot be called a clean PASS.

6. `69b10360475872e0c4c987e81e3ece0acf2d1a2b`
   - adds the one-time startup debtor sweep;
   - after Recovery/ReverseIndex rebuild, every current Bank in the configured group is marked
     dirty exactly once;
   - the same R127c batch/parallel-health path performs this startup scan;
   - prevents an Account that is already liquidatable at startup from waiting indefinitely for a
     later Account/Oracle write.

GitHub compare for `bdcaa41c...69b10360` shows the runtime implementation changed only:

- `project_0/native/crates/darkbot-runner/Cargo.toml`
- `project_0/native/crates/darkbot-runner/src/main.rs`
- `project_0/native/crates/darkbot-runner/src/builder.rs`
- `project_0/scripts/run_r127_qualify.py`

## Runtime semantics before and after

### Before R127c

```text
dependency update
→ dependency → Banks
→ Banks → Accounts
→ enqueue/coalesce Accounts
→ exact Health
→ healthy drop
→ pipeline
```

This ordering created millions of duplicate fanout references before the healthy filter could
help.

### R127c

```text
canonical ingress
→ apply update to StateCore immediately
→ dependency/Bank write marks affected Banks dirty
→ collect <=64 events OR <=5 ms
→ union dirty Banks
→ expand Bank→Account references once
→ deduplicate Accounts
→ exact canonical Marginfi Health in parallel
→ healthy: DROP before pending queue
→ liquidatable: enqueue existing pipeline
→ health stale/error: enqueue fail-closed existing refresh/projection path
→ Projection → Sizing → Economics → Build/Sim
```

No approximate health model was introduced. The admission decision continues to use the pinned
official Marginfi health implementation over canonical RAM state.

## New/default controls

- `P0_USE_THRESHOLD_TRIGGER=false`
  - remains the default;
  - the old ThresholdIndex path is retained only as a legacy/experimental comparison path.

- `P0_SWEEP_BATCH_MAX_EVENTS=64`
  - maximum dependency/Bank updates accumulated before a forced batch flush.

- `P0_SWEEP_BATCH_MAX_AGE_NS=5000000`
  - maximum age of an open batch; default 5 ms.

- `P0_INGRESS_BURST_BEFORE_DETECT=64`
  - existing ingress/detection fairness limit remains active.

## New R127c telemetry

The `oracle_trigger_stats` line additionally contains:

- `sweep_batches`
- `sweep_dirty_banks`
- `sweep_account_refs`
- `sweep_unique_accounts`
- `sweep_forwarded`
- `sweep_max_unique_batch`
- `sweep_max_dirty_banks`

Interpretation:

- `sweep_account_refs`: raw Account references obtained while expanding all dirty Banks;
- `sweep_unique_accounts`: Accounts after cross-Bank deduplication;
- `batch_dedupe_ratio = 1 - sweep_unique_accounts / sweep_account_refs`;
- `sweep_forwarded`: only liquidatable plus fail-closed health-error Accounts admitted to the
  expensive downstream pipeline;
- `pending` / `max_pending`: now measure downstream candidates, not the full healthy debtor
  population.

## Test status

User-reported local tests after the complete R127c implementation:

- workspace tests with `crate_health txv1`: PASS;
- workspace tests with `crate_health`: PASS;
- visible final unit group: 30 passed, 0 failed;
- all shown Doc-tests: PASS; zero Doc-test cases is not a failure.

The release binary still emits one compiler warning:

```text
value assigned to `detect` is never read
```

This warning is not a compile/test failure and is unrelated to the R127c batch semantics. It is
left unchanged while the active qualification run is using this exact runtime code.

## Current qualification

The user started:

```text
python3 project_0/scripts/run_r127_qualify.py
```

Current run prefix:

```text
r127_20261005T160040Z
```

This run must be evaluated using the R127c batch metrics above. Do not promote R127c based only
on compilation or on low ingress backlog.

## Rollback procedures

### Safest inspection/reproduction rollback

Do not alter `main`. Check out the frozen branch:

```bash
git fetch origin
git switch --track origin/archive/r127b-pre-batch-20261005
```

This reproduces the exact pre-R127c tree at `bdcaa41c`.

### Revert R127c runtime changes on main

If R127c must be removed from `main` while preserving history, revert the six implementation
commits in reverse chronological order:

```bash
git revert 69b10360475872e0c4c987e81e3ece0acf2d1a2b
git revert 28474060ce44a3a483c6be6f905044293739d623
git revert fcd262e74e545e5f8510b1a4de0455b53878b581
git revert abcf32376387d86960dcb414536f56be2a0e09e3
git revert 077a764566a48adae547a9591b31be9b97671cdf
git revert 609ca943a4be70ac993dde1af400159dd4d932d4
```

The evidence and ledger commits should normally remain: they document why the experiment was
performed and what was measured. If an exact historical tree is required instead, use the frozen
archive branch/commit rather than deleting evidence.

## Promotion / rejection rule

R127c is not considered qualified until its Mainnet Shadow report shows all of the following:

- dependency updates > 0;
- `sweep_batches > 0`;
- batch Account deduplication is actually exercised;
- no ingress overflow / no WSS sink-overflow;
- ingress backlog remains bounded;
- `max_pending` is materially below the R127/R126b ~78k ceiling;
- final pending queue drains to a small bounded level;
- healthy Accounts dominate prefilter drops rather than entering the downstream queue.

If those conditions fail, use this document and the frozen branch to compare or roll back without
reconstructing the change history from chat.
