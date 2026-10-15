# Runtime modularization plan: `main.rs` / `run_shadow()` and `market_observer.rs`

Status 2026-10-15: **plan only, nothing executed** (except the `cfg(not(crate_health))` removal of R0, done by the
mock-ban cleanup). Basis: `d234913` plus the staged iterations 30–32. Goal: smaller, owned
modules with byte-for-byte unchanged behaviour. No new crate, no new thread, no new trading function, no LIVE path.

## 1. Fixed constraints

- **Strict pins:** `market_verification.rs`, `market_reconcile.rs`, `market_truth.rs`, `market_gap.rs` are hashed into strict
  verification specs and stay byte-identical. They reach parent items through `super::` (for example `RpcFetcher::new` in
  `market_gap.rs` and `market_reconcile.rs`); every such item must stay reachable at the same path through re-exports in
  `market_observer.rs`.
- **Self-scan test** `market_mode_has_no_signing_simulation_or_submission_path` reads `market_observer.rs` and `main.rs` with
  `include_str!`: forbidden tokens must stay absent and in `main.rs` the order `fn run()` → `market_observer::run_from_env` →
  `config::require("P0_RPC_URL")` must hold. Before any split the scan is extended to every new module file.
- **Public paths stay:** `darkbot_runner::market_observer::{MarketConfig, run_embedded, run_from_env, ...}`, used by `main.rs`,
  `econ.rs` and the tests, keep their paths through `pub use`.
- **One owner thread:** `run_shadow` keeps its single state owner. Extraction moves code and groups locals into structs; it
  adds no `Arc`/`Mutex`, no channel and no reordering of the loop.
- **Behaviour parity per step:** `cargo check` and runtime tests for `crate_health` and `crate_health,txv1`, differential Clippy
  (no new warning), rustfmt on changed files, `git diff --color-moved=zebra --color-moved-ws=allow-indentation-change` showing
  the moved block as moved (logic lines unchanged), test count per crate unchanged or higher, strict-pin hashes unchanged.

## 2. `run_shadow()` today (`native/crates/runtime/src/main.rs`, lines ~130–3390, one function)

| Section | Lines | Content (fix tags) | State owner after split |
|---|---|---|---|
| A Startup | 132–296 | env, ProgramData pin [FIX-131], funded payer [FIX-98], group bank bootstrap [FIX-78], required/optional dependency keys, SOL banks | `Startup` (immutable result) |
| B Ingress session | 297–351 | `spawn_ingress`, subscription sets, restart on dependency change [FIX-34] | `IngressSessions` |
| C State and recovery | 352–480 | snapshot RPC client, `RecoveryDriver`, `fetch`/`drain` closures [FIX-96] | `StateCore` + `RecoveryDriver` (owner loop) |
| D Pipeline components | 482–623 | blockhash thread, `Safety`/`Allowlist`, `ObserveOnly` submitter, routes, builder, resources, freshness, min-profit, integration-probe config, `RemoteWorkers` [FIX-108] | `ShadowPipeline` |
| E Loop state | 624–925 | projection cache, detect queue, oracle detect + coalescing, integration probe, ledger, winner tracker, output files, reverse/threshold index [FIX-133], sweep batching [R127c], candidate cache [R127d, FIX-134/135], measurement counters [FIX-138], fairness [R126b], MT0 timers | grouped structs (see 3) |
| F1 Econ hooks, operational window | 944–1026 | C0-E1 shadow tap, operational window | owner loop |
| F2 Remote results | 1027–1398 | winner, reconcile, oracle-price projection, projection results | `remote_apply` |
| F3 Dispatch | 1399–1438 | due winner watches, reconcile jobs (bounded, non-blocking) | `remote_apply` |
| F4 Sweep flush | 1439–1863 | bank-coalesced exact Health fan-out, candidate cache decisions | `sweep` |
| F5 Ingress events | 1864–2715 | classification, dependency updates, recovery outcome (upgrade re-check, startup scan, probe), index maintenance, bank-config dependency expansion [FIX-34/35], oracle/integration triggers | `events` |
| F6 Detection consumer | 2716–3236 | integration projection [FIX-111], simulation clock [FIX-119], Health → pipeline, ledger/watchlist, reconciliation [FIX-18/23] | `detect` |
| F7 Telemetry | 3237–3339 | periodic stats and summary lines | `telemetry` |
| G Shutdown | 3340–3390 | final ledger day, flush, error classification | `run_shadow` |

Data flow (unchanged): WSS ingress → bounded `ingress` channel → event handling (state, indices) → sweep / oracle triggers →
detect queue → Health → pipeline (`ObserveOnly`, never submits) → ledger, records, winner watches → bounded remote workers →
results applied at the top of the next loop iteration.

## 3. Target modules (binary-private, `src/longtail/` like the existing `accounts.rs` and `candidates.rs`)

| Module | Content | Inputs → outputs |
|---|---|---|
| `longtail/startup.rs` | A + C setup, returns `Startup { program, group, payer, dep keys, sol banks, program_data_hash, rpc clients, recovery driver }` | env, `Clock` → `Startup` |
| `longtail/ingress.rs` | B: `IngressSessions { subscribed, required, optional, session }`, `respawn(...)` | subscription changes → new session |
| `longtail/setup.rs` | D: `ShadowPipeline { pipeline parts, remote workers, limits }` | `Startup` → `ShadowPipeline` |
| `longtail/state.rs` | E grouped: `ProjectionState`, `OracleDetect`, `IntegrationProbe`, `SweepState`, `CandidateCache` (config + counters), `Fairness`, `Telemetry` (FIX-138, health sweep, MT0 owner timer) | owned by `run_shadow` |
| `longtail/remote_apply.rs` | F2 + F3 as `apply_results(&mut ...)` and `dispatch(&mut ...)` | worker results → state, jobs → bounded queues |
| `longtail/sweep.rs` | F4 `flush(&mut SweepState, &mut CandidateCache, ...)` | dirty banks → detect queue |
| `longtail/events.rs` | F5 `on_event(...)` incl. recovery outcome handling | `IngressEvent` → state, triggers |
| `longtail/detect.rs` | F6 `on_detect(...)` | detect item → pipeline, ledger, watches |
| `longtail/telemetry.rs` | F7 formatting and emission, exact strings | counters → stderr/summary lines |
| `main.rs` | `main`, `run` (mode dispatch), `run_econ4h`, `run_shadow` as orchestrator calling the above in today's order, shutdown | — |

## 4. Steps for `run_shadow` (each one separately reviewable)

| Step | Change | Extra parity check |
|---|---|---|
| R0 | Parity tooling: telemetry format golden tests (exact line formats of F7), self-scan extended to `longtail/*.rs`, moved-line check script. (Done already: the never-compiled `cfg(not(feature = "crate_health"))` branches, `NoPlans` and `NoHealth` were removed by the mock-ban cleanup, see `delete/README.md`.) | new tests fail if a format string changes; `cargo check` with and without features |
| R1 | Group F7 counters and E counters into `Telemetry`/`CandidateCache` structs (field renames only) | golden formats unchanged |
| R2 | Extract F7 telemetry emission | golden formats |
| R3 | Extract A/C/D setup into `startup.rs`/`setup.rs` | startup error strings unchanged (unit tests on fail-closed paths: ProgramData owner, unfunded payer, invalid bounds) |
| R4 | Extract F2/F3 remote apply and dispatch | existing worker/tracker tests, no RPC on the owner thread (grep check) |
| R5 | Extract F4 sweep | candidate-cache and fairness tests (`main_tests.rs`) |
| R6 | Extract F6 detection consumer | pipeline/ledger tests, `ObserveOnly` still the only submitter (grep check) |
| R7 | Extract F5 event handling (largest, most coupled, last) | recovery/upgrade-latch tests, ingress-before-detect fairness |

## 5. `market_observer.rs` today (2 822 lines) and target

| Section | Lines | Target file (`#[path]`, re-exported from `market_observer.rs`) |
|---|---|---|
| Configuration, `MarketConfig`, `market_ws_config`, `redact_url` | 79–348 | `market_config.rs` |
| UTC formatting | 349–391 | `market_time.rs` |
| `LiquidationReceiverEvent` decoder | 392–544 | `market_decode.rs` |
| Transaction enrichment, `verify_tx_identity` | 545–760 | `market_enrich.rs` |
| Output sink `MarketSink`, `JsonlSink` | 761–819 | `market_sink.rs` |
| Observer core `ObserverCore`, stats, truncation qualification | 820–2096 | `market_core.rs` (later optional split of the 1 080-line `impl`) |
| Summary, report, `finalize` | 2097–2249 | `market_summary.rs` |
| `RpcFetcher` (used by strict-pinned gap/reconcile) | 2250–2319 | `market_fetch.rs`, re-exported at the old path |
| Production run: banner, `finish_capture`, `run`, `run_embedded` | 2320–2820 | `market_run.rs` |

State owners: `MarketConfig` immutable; `ObserverCore` owns statistics, truncation and deferred liquidations;
`run_embedded` owns sink, WSS session and the econ tap; `RpcFetcher` instances belong to the post-capture enrichment, gap and
reconcile phases. Steps O1 (facade with re-export list, no move), O2 config, O3 time + decoder, O4 enrichment, O5 sink,
O6 summary, O7 fetcher, O8 run, O9 core; each with the parity checks of section 1, the whole `market_observer_tests.rs` passing
unchanged, and the strict-pin hashes verified.

## 6. Out of scope

Performance changes, new queues or threads, changed limits, any LIVE or submission path, renaming Cargo packages.
A representative performance baseline (hot path, queues, Health, economics, scheduling under mainnet load) is
`NOT_MEASURED` offline; the runtime counters (`pipeline_timing_snapshot`, MT0, candidate-cache and sweep counters) only
produce values during a capture and need a separately authorized FREE_ONLY run.
