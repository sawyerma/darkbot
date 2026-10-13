# FE-04 persistent single-shot hold — offline subpackage (2026-10-13)

**IMPLEMENTED / TESTED foundation; FE-04 overall PARTIAL, NOT LIVE READY.**

`darkbot_runner::one_shot` supplies a versioned approval/intent contract and per-wallet OS lock + atomic, fsynced durable journal. Intent pins approval/code/build/program/ProgramData/funding/target/signatures/wire hash/expiry and exact SOL/SPL holds. Current execution facts are supplied by the production owner, not inferred from historical reports. The journal rechecks limits/freshness/fork-generation/upgrade/balance/simulation facts before a callback; UNKNOWN is persisted before possible handoff. No internal resend. Restart consumes even a pre-handoff reservation; ambiguous/torn writes halt. Only complete finalized signature membership, fee, expected wallet-asset deltas and exit evidence can close holds. Completed one-shot stays halted and cannot be rearmed by a new profile.

Existing production Safety fix: `record_outcome(None)` now retains outstanding capital/position holds and latches UNKNOWN. Later complete accounting can release the hold but cannot clear HALT. This is connected to the existing Pipeline reconciliation through its existing Safety call.

Six journal regressions: simulated crash/reopen before/after send, timeout after possible acceptance, double trigger, changed wire/budgets/auth, stale/gap/fork/upgrade/wallet, bundle-only simulation requirement, missing/duplicate bundle members, missing finalized status/fees/token deltas/exit and corrupted/torn journal. Plus one existing Safety regression. No real signed transaction was sent.

Workspace tests before the last added bundle-membership fixture: txv1 **346 PASS / 0 FAIL / 1 ignored**; health **344 PASS / 0 FAIL / 1 ignored**. Final journal subset including the new test: **6 PASS** in each feature set. Both full release builds PASS (production code unchanged by the final test addition). Existing warnings retained. Changed-region formatting and whitespace PASS. Final differential Clippy in adjacent JSON; baseline is FE00 compiled Rust at 28f6d53. Python support scripts: 59 PASS.

**Explicit remaining integration, no fake FE-04 PASS:** journal is exported but the normal LIVE entrypoint remains hard blocked. The terminal authorization lifecycle, production-owned assembly of the current proof/intent immediately at handoff, coupling to the actual adapter over bound serialized transactions, and chain-backed complete wallet/exit receipts are not connected yet. This subpackage does not itself fetch/verify chain receipts or calculate realized PnL; supplying a struct is not a Mainnet proof. FE-06/07 actual pilot/realized profit remain NOT_RUN. No live authorizing profile was created.

The bounded FE-01 read-only observation is prepared separately; actual current protocol parity and candidate profitability remain blocked until authorized chain checks. Five-hour replay remains user-owned.
