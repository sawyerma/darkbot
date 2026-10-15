# Wallet journal lock release — technical finding (2026-10-15)

Status: **PENDING_INDEPENDENT_REVIEW**. This fix is separate from the run-layout restructuring.
Committed by the user as `6ec4483` (unlock in `Drop for Journal`) and `218710b` (`WalletLock` guard for every release path).

## Symptom

An independent test run failed in
`one_shot::tests::exact_adapter_bytes_ack_timeout_and_rejection_never_rearm`
(`native/crates/darkbot-runner/src/one_shot.rs`): after `drop(j)` the next `Journal::open` for the same wallet returned
`WALLET_ALREADY_LOCKED`. The failure was sporadic and only appeared in full test runs.

## Root cause

`Journal::open` takes an exclusive, non-blocking `flock` on `<wallet>.lock` through `File::try_lock`. A `flock` belongs to
the open file description, not to the file descriptor. Closing the descriptor releases the lock only when no other
reference to that description exists.

Other tests in the same test process start child processes concurrently: `git` in `econ::source_commit` and
`measurement::persist_manifest`, `ps` and `df` in `market_verification`. A child created while a journal is open holds
a reference to every open file description until its `exec` closes the close-on-exec descriptors. If the journal is
dropped inside that window, closing its descriptor does not release the lock. An immediate reopen then fails.

Ruled out: no shared temporary directories (each test uses `p0-shot-<pid>-<counter>` and a fresh wallet), no second
journal on the same wallet, no macOS-specific lock semantics beyond `flock` itself, no difference in the submission
reopen path.

## Reproduction

| Check | Result |
|---|---|
| Standalone program: open, `try_lock`, drop, reopen, while 4 threads spawn `/usr/bin/true` | drop only: 410–474 reopen failures per 20,000 rounds; explicit `unlock()` before drop: 0 of 60,000 |
| Deterministic test `dropped_journal_releases_wallet_while_lock_descriptor_is_shared` (a cloned descriptor stands in for the child) | fails on the reopen without the fix, passes with it |
| Error-path test `failed_open_releases_wallet_while_processes_are_spawned` (failed open with an interrupted-write marker, 4 spawner threads, 2,000 opens) | with unlock disabled: 199 of 2,000 opens wrongly reported `WALLET_ALREADY_LOCKED`; with the fix: 0 |

## Fix

The lock file is wrapped in `WalletLock`, whose `Drop` calls `unlock()` (`LOCK_UN`) before the descriptor is closed.
`LOCK_UN` acts on the open file description, so the wallet is free as soon as its owner releases it, whatever a
concurrently spawned child still references. Because the wrapper owns the lock from the moment `try_lock` succeeds,
the failed-open paths (`JOURNAL_INTERRUPTED_WRITE_HALT`, `JOURNAL_SIZE_LIMIT`, `JOURNAL_READ`, `JOURNAL_CORRUPT`,
`JOURNAL_WALLET_MISMATCH`, a failed re-persist) release it the same way.

Commit `6ec4483` added the explicit unlock in `Drop for Journal`. Commit `218710b` moves it into `WalletLock` so the
error paths are covered too, and adds the error-path regression test.

## Invariants kept

- A wallet is never held by two journals at once: `try_lock` is unchanged and still fails closed with
  `WALLET_ALREADY_LOCKED`. The unlock runs only when the owning journal or the failed open releases the wallet.
- Unknown submission state is never re-armed: the `Reserved -> HandoffUnknown` transition and every hold/receipt rule are
  unchanged.
- Persistent holds and journals are untouched: the change concerns only the advisory lock file.
- No test removed, ignored or filtered.
- `one_shot` has no production caller outside its module today; the fix also covers any future caller that spawns processes.

## Validation

Offline, `--locked`, default test parallelism, on the final code (`218710b`):

| Check | `crate_health` | `crate_health,txv1` |
|---|---|---|
| Isolated `exact_adapter_bytes_ack_timeout_and_rejection_never_rearm`, 200 runs | 0 failed | 0 failed |
| Both new lock tests, 50 runs | 0 failed | 0 failed |
| Whole `one_shot` module, 50 runs | 0 failed | 0 failed |
| Whole `darkbot-runner` lib, 10 runs | 0 failed | 0 failed |
| Full workspace test matrix | **377 passed, 0 failed, 1 ignored** | **379 passed, 0 failed, 1 ignored** |
| Release build | PASS | PASS |

The previous baseline of 375/377 rises by the two new tests. Differential Clippy (`--all-targets`, `crate_health,txv1`)
against `docs/evidence/OPERATIONS_CONFIG_20261014/clippy-diagnostics.json`: 38 → 38 unique warnings, none added.
`one_shot.rs` passes `rustfmt --check`. Before the error-path change, the `Drop for Journal` version (`6ec4483`) also
passed 25 of 25 full lib runs and 14 of 14 lock-heavy module runs under `crate_health`. Logs are local only, not
versioned.
