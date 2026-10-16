# PERSIST / FE-01 local follow-up — 2026-10-16

Base: `9047c7b45988b0c8798b710bb97c183d0a063c7f`. This is a new, **uncommitted local** follow-up to production `4a27a1c` and its documentation synchronization. Overall **PERSIST PARTIAL**, **FE-01 OPEN**, **FE-04 PARTIAL_LOCAL**. No current economic GO, Mainnet/LIVE or submission claim.

## Implemented and verified

- Strict batch evidence now uses the existing `JournalSink` and `market.p0j`. The compatibility `JsonlSink` name exists solely for the pinned verifier: its final batch `flush` seals the binary journal and derives the three legacy JSONL views. There are no parallel primary JSONL writes. Subsequent appends fail; failed export/sync remains failed on retry. Final manifest completeness concerns recorder output, not chain coverage: the unchanged strict reconciliation gate still decides qualification.
- `market_verification.rs`, `market_reconcile.rs`, `market_truth.rs`, `market_gap.rs`, historical fixtures/evidence and spec/checkpoint schemas are unchanged. Exact hashes are recorded in the qualification file. New built verifier binaries naturally receive their existing automatic binary digest; old proof identities are neither overwritten nor silently reused across code changes. Byte parity and the existing interrupted/resumed strict-batch tests pass.
- Deterministic per-writer sync errors (`Other`, `StorageFull`) are injected at the I/O boundary only in test builds. Frames are actually appended before the sync error. No durable acknowledgment, SEND callback, retry or complete seal succeeds after failure. Reopening retained unacknowledged frames preserves holds and UNKNOWN. This proves error-path behavior, **not a physical power-loss or fault-injecting-filesystem experiment** (NOT_RUN).
- FE-01 binary replay now requires an ordered config/read/result/manifest archive, a complete seal and exact logical read count. Whole-file and consumed-frame budgets include config/result/manifest; the call limit applies before accumulating reads. Unsealed, incomplete, reordered, corrupt, over-budget and ambiguous archives fail closed. Saved-state parity with the historical JSON reader is retained; replay remains HISTORICAL_NOT_CURRENT and never qualifies funding or enables LIVE.
- Three capture regression fixtures now instantiate `JournalSink`, matching the existing production capture entrypoint. Existing assertions for liquidation rows, errors, coverage and reports were retained. This fixes the three failures in the first full run; the complete final suites pass.

## Validation

- Rust workspace `crate_health`: **421 PASS**, 0 failed, 2 pre-existing ignored.
- Rust workspace `crate_health,txv1`: **423 PASS**, 0 failed, 2 pre-existing ignored.
- Featureless check, both release builds, Python **107 PASS**, verifier self-test **29 PASS** and census self-test **PASS**. Full logs and command metadata: [qualification.json](../evidence/PERSIST_FOLLOWUP_20261016/qualification.json).
- Clippy exits 0 in both feature configurations; warning inventories and comparison with the prior recorded local baseline are included. No claim of a warning-free baseline. The only post-suite edit removed an unused import; final checks, both release builds and all-target Clippy were repeated.
- Four build jobs and four test threads maximum, sequential suites. No new benchmark: prior synthetic results remain historical; current Mainnet volume/throughput/p50/p95/p99 are **NOT_MEASURED**.

## Remaining gates

FE-01 needs separately authorized current deployment/source/state and funding-class evidence under qualified FREE_ONLY provider limits. FE-04 still needs production-owned Intent/Current integration, terminal approval lifecycle, exit and complete loss/PnL accounting. Physical filesystem/power-loss qualification and realistic load measurements remain open. Existing user-owned 5h replay is not run here. No structural reform, BSC edit, staging, commit or push.
