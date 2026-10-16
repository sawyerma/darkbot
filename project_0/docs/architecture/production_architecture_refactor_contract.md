# Project_0 – Verbindlicher Produktionsarchitektur- und Refactoring-Vertrag

**Stand:** 2026-10-15  
**Status:** Struktur am 2026-10-16 lokal implementiert und offline qualifiziert; [Nachweis und bewusste Abweichungen](runtime_modularization_20261016.md). Keine Mainnet-/LIVE- oder wirtschaftliche Abnahme.
**Scope:** exclusively `project_0/`. Other projects, especially `bsc/`, are out of scope.  
**Authority:** [PHASENPLAN.md](../PHASENPLAN.md) v4.5.2; [PHASENPLAN.json](../PHASENPLAN.json); [MEASUREMENT_LEDGER.md](../MEASUREMENT_LEDGER.md), [MEASUREMENT_LEDGER.json](../MEASUREMENT_LEDGER.json), immutable [evidence](../evidence/). This contract adds a structure/migration layer. It neither shortens nor changes any authoritative product or safety gate.

> **Aktueller Auftrag 2026-10-16:** Der Nutzer hat A0–A7 ausdrücklich freigegeben und die nachfolgend historisch dokumentierte A0-Stoppregel ersetzt. Der aktuelle Code-/Qualifikationsstand steht im [Abschlussnachweis](runtime_modularization_20261016.md).

> **Historische Anweisung vom 2026-10-15:** Read this document in full, then the current README, status, plan/ledger and the real code. Verify the LOCAL staged iterations 30–32 and subsequent path/keypair/feature/mock cleanup **before** any new architecture refactor. Remote GitHub cannot see staged local edits. **Begin with A0 only, report findings and STOP.** Never silently change, unstage or overwrite the user's existing working tree. No commit, push or branch without separate user instruction.

## 1. Non-negotiable product and architecture rules

1. One production Rust implementation for Long-Tail, Premium, Race, Shadow, Replay and eventually separately authorized LIVE; no second bot, trading kernel or parallel economic truth.
2. Three triggers into the same canonical pipeline: state/oracle updates (Race), premium/debt boundaries, liquidation-tag time (`t_profitable`). All converge on exact health → deterministic economics → ranking → receivership plan → build/sign → controlled handoff → landing/reconciliation.
3. Four crates only: `common`, `solana`, `liquidation`, `runtime`. No new crate/framework without a separately reviewed dependency/ownership necessity.
4. `solana/state` implements the *only* canonical chain/fork state. `runtime/orchestration` owns the coordination/event-ordering/lifecycle of that state, **not a second state**.
5. Rust performs every LIVE-/hot-path-relevant calculation. Python is for offline capture assistance, verification, reporting and operations. No Python/JSON/new RPC roundtrip required in the Race hot path.
6. `runtime` remains named `runtime`: its scope is orchestration + market observation + execution + analytics. Do **not** rename it `execution` as a crate. `common` remains `common`, not a mandatory cosmetic `foundation` rename.
7. The outer Rust Workspace may later be renamed `native/ → engine/`, but this is a **separate, optional, consumer-checked path migration**, not a prerequisite to FE-00–FE-07 and not a reason to change the Cargo package names.
8. Keep `operations/` (Python package) and `scripts/` (stable shell CLI) separate. Do **not** move them together under `tools/` or duplicate parsers and config.
9. No mock, fake, stub or demo implementation as production code or replacement for product tests. Per user request, audit pre-existing fake-cargo/mock test artifacts; remove unnecessary ones **only after verifying consumers and pinned contracts**. Dependencies such as `kamino-mocks`, `drift-mocks` and `juplend-mocks` must be classified by **actual protocol function**, not removed solely because of their names. Recorded real original chain data and independent crosschecks are not pretend production engines.
10. Safety remains fail-closed: `FREE_ONLY`, LIVE DISABLED, value-bearing submission DISABLED, no unauthorised Mainnet queries/cost, no automatic paid fallback, no accidental wallet keypair resolution change, no loss of verification identity.
11. Structure work has **no economic GO**. FE-00…FE-07 remain first priority; full v4.5.2 coverage remains mandatory: S1-RACE/Binary-Geyser, S3.R/Jito, prepared TX v0/v1, all funding classes/JIT exact-in, Tagger, C0/C0-E1/H0, P6-MT/M4, P7/P4B, P8-RES/INV, multi-asset inventory/rebalancing, K and frontend.

## 2. Human-readable target (no empty placeholder directories)

```text
project_0/
├── README.md  .env.example  .gitignore
├── config/                         # profiles, examples, public configuration
├── engine/                         # TARGET name for current native/ Rust workspace
│   ├── Cargo.toml  Cargo.lock
│   └── crates/
│       ├── common/                 # clock, timing, channel, lifecycle, primitives
│       ├── solana/                 # Solana adapters, canonical chain state and submission
│       │   └── src/
│       │       ├── ingress/
│       │       │   ├── rpc_websocket/       # split of existing ws_rpc.rs, if useful
│       │       │   └── binary_geyser/       # future only after S1-RACE gate
│       │       ├── state/
│       │       ├── tx/
│       │       ├── submit/
│       │       ├── provider.rs
│       │       └── runtime_config.rs
│       ├── liquidation/            # protocol and binding economic decisions
│       │   └── src/
│       │       ├── protocol.rs  index.rs  schedule.rs  health.rs
│       │       ├── sizing.rs  economics.rs  funding.rs  planner.rs
│       │       ├── prepared.rs  receivership.rs  integration.rs
│       │       ├── projection.rs  safety.rs
│       │       ├── execution_policy.rs    # potential rename of execution.rs
│       │       ├── tagger.rs              # only when Phase T is implemented
│       │       └── optimizer.rs           # only when P4B is implemented
│       └── runtime/                # sole application orchestration / runner
│           └── src/
│               ├── main.rs         # thin entrypoint, no duplicate engine
│               ├── orchestration/
│               │   ├── startup.rs  state_owner.rs  triggers.rs
│               │   ├── candidates.rs  scheduling.rs  preparation.rs
│               │   └── recovery.rs
│               ├── execution/
│               │   ├── pipeline.rs  builder.rs  route.rs  pending.rs
│               │   ├── one_shot/         # existing journal/receipts/transport
│               │   ├── reservations/     # future P8-RES
│               │   ├── inventory/        # future P8-INV
│               │   └── accounting/       # finalized realized wallet/PnL
│               ├── market/
│               │   ├── observer.rs  decoding.rs  gap.rs
│               │   ├── reconcile.rs  verification.rs  truth.rs
│               ├── analytics/
│               │   ├── market_economics.rs
│               │   ├── competition.rs
│               │   └── measurement.rs
│               └── bin/
├── operations/                     # existing Python package / real offline operations
│   ├── cli/  capture/  verification/  reporting/  qualification/
├── scripts/                        # stable shell operator entrypoints
├── tests/                          # real production-code regressions and historical records
├── docs/                           # source-of-truth plans and immutable evidence
├── data/                           # local, non-versioned runs / cache
└── exports/                        # generated exports
```

**This is a desired responsibility map, NOT an instruction to create all listed paths.** The current checked-in workspace remains `project_0/native/`. Most of these nested paths are not implemented. Existing additional modules such as `lookup_tables`, `record`, `run_layout`, `shadow_ledger`, `remote_workers`, `one_shot` and tests MUST retain a justified owner; omission from the abbreviated tree never means deletion.

## 3. Strict responsibility boundaries

| Owner | Only authority / responsibility | Forbidden duplication |
|---|---|---|
| `common` | Clock, timing, channels, primitive telemetry, lifecycle, errors | Business, chain, fee, wallet and liquidation policy |
| `solana/ingress` | WebSocket/RPC and eventually binary Geyser adapters, subscriptions, provenance, receive timestamps and stream health | Health/economic decisions |
| `solana/state` | Single canonical Chain/Fork/Snapshot/Recovery implementation | Parallel second market or runtime chain truth |
| `solana/tx`, `solana/submit` | Solana transaction v0/v1, builder-/signing-related adapters, RPC/Jito submitters | Independently choose opportunities, wallet holds, market profit |
| `liquidation` | Marginfi protocol, indices, schedule/tag, exact health/sizing, deterministic economic evaluation, funding, prepared candidate, planner, execution policy, safety | Alternative market observation/second LIVE engine |
| `runtime/orchestration` | Central state owner and event ordering, generation/readset, trigger→candidate, worker/scheduler, singleflight, recovery | Second canonical chain-state or separate trading kernel |
| `runtime/execution` | Coordinate plans, routing/tx build, persisted one-shot safety/holds, simulation/handoff, landing and finalized accounting using `solana/submit` | Another Solana adapter, duplicated economics or free-standing LIVE arm |
| `runtime/market` | Independent read-only capture, raw event decode, gap handling, chain verification, strict canonical records | Execute trades, classify Shadow as chain truth, omit data because of report filters |
| `runtime/analytics` | C0-E1 analytical economics, competitions, observed market winners, result reporting | Make LIVE business-policy decisions or overwrite historical/protocol truth |
| `operations` + `scripts` | Python offline verification/reporting + shell launchers, one typed backend configuration contract | Second health/route/decoder/verification engine; another set of defaults |

**Critical nuances (do not flatten them):**

- Chain-State implementation belongs to `solana/state`; `orchestration/state_owner` only coordinates it. One owner, no mutable double copy.
- Current `winner.rs` performs real chain winner/reconciliation checking (belongs conceptually near `market/verification`); `winner_tracker.rs` owns pending verification coordination (orchestration); statistical win probability/competition belongs to `analytics`. Pinned `market_truth.rs` and `market_reconcile.rs` currently depend on winner helpers, so keep their actual paths and compatible API until a fully qualified move.
- Hot-path measurement lives at the real event boundary and **does not perform blocking file I/O**; `analytics/measurement` only aggregates results. `solana/submit` actually interfaces with the chain; `runtime/execution` controls legal sequence/intent, not a second transport.
- `liquidation/economics.rs` = binding executable trade economics; `runtime/econ.rs` = C0-E1 market/economic observation; `runtime/econ_canonical.rs` needs exact dependency/pin review; `runtime/accounting` = finalized fees/token/exit/realized net PnL. No double economic decision.
- Current `liquidation/execution.rs` concerns quarantine, backoff, freshness, simulation and fallback rules; a later rename to `execution_policy.rs` is descriptive but **does not authorize changing semantics**.
- `runtime/one_shot.rs` and `one_shot/{receipts,transport}.rs` are mandatory FE-04/P8 boundaries. Persisted UNKNOWN/SUBMITTED states and wallet-/SOL-/fee-/tip-/rent holds must survive crash/restart. Concurrent LIVE requires global atomic reservations, not parallel spending of the same assets.
- The six on-chain instruction markers represent **four liquidation classes**, not six independently counted wins. Report marker/type filters never filter original capture, decoder, verifier or Trading admission. Four related NON-LIQUIDATION instructions are separately classified. Shadow evidence is NOT finalized chain evidence.
- Preserve `capture_run_id`, `verification_spec_id`, `verification_attempt_id` / result ID, `report_query_identity` and all V1/V2 reports, strict chain paths, bookmarks, and historical run IDs.

## 4. Exact Phase coverage (PHASENPLAN v4.5.2)

| Phase | Design ownership |
|---|---|
| P0 / P1A / P1B | Product source/program/IDL pins, protocol, integration, version-aware decoding |
| S0 | `common` foundation primitives |
| S1-LT / S1-RACE | `solana/ingress`: RPC WebSocket long-tail, future binary Geyser on qualified feed |
| S2 / P2 | `solana/state`, `liquidation/index,schedule,prepared` and orchestration |
| S3 / S3.R / S4 | `solana/tx,submit`, runtime execution; RPC/Jito/gRPC later; versioned parity measurements |
| P3 | `liquidation/health, sizing, integration, projection` |
| P4A / P4A-JIT | `liquidation/economics,funding` and routing/execution classes |
| P5 | Protocol-valid receivership planner, RouteProvider, TX build; prepared candidates |
| T | Tagging and timed premium/tag scheduling, same candidate decision pipeline |
| C0 / C0-E1 / H0 | `market` / `analytics` / Python `operations`; independent Capture/Index/Strict/Report |
| P6-LT / P6-RACE / P6-MT | `orchestration`, bounded processing, M4 and genuine performance/safety evidence |
| FE-00…FE-07 / P8-LT | One current executable class + authenticated one-shot, persistent hold, reconciliation, realized net PnL |
| P8-RES / INV / P9 | Atomic multi-asset capital reservations, inventory risk/rebalancing, repeated controlled runtime |
| P7 / P4B | Measured winner labels + probability, business optimizer via existing Rust decision path |
| K / Frontend | Measured incremental optimization of SAME core; versioned shared CLI/frontend API |

NO capability is waived by profit-first. A planned module/location is not equivalent to a supported, compiled, qualified or profitable capability.

## 5. Implementation order – strict per-step acceptance

1. **A0 – LOCAL BASELINE REVIEW; STOP AFTER THIS STEP.** Inspect `git status -sb`, `git log -1`, `git diff --cached -- project_0/`, `git diff -- project_0/` and existing staged Iterations 30–32 + follow-up keypair/path/mock/feature fixes. Compare pinned evidence, existing exports and flags. Treat the local staged state as data, not disposable leftovers. Report PASS/FAIL/NOT_RUN and exact SHA. Do not modify runtime or other files yet.
2. **A1 – full active-file/consumer matrix:** For every active Rust/Python/Shell and interface, identify current path, target responsibility, owner, importers, build membership, feature gate, include-str path, fixed release-binary location, identity pins, whether it can stay where it is and the minimal proof. Real production linkage, not mere existence of a file.
3. **A2 – reproducible baseline:** Golden output/transaction bytes and actual historical chain-derived Replay/Resume, feature builds, Python/CLI, Rust warnings, safety, measurements. p50/p95/p99 latency and workload baselines only if genuinely measured; otherwise `NOT_MEASURED`.
4. **A3 – `run_shadow()` extraction:** Untangle the ~3260-line function into minimal domain functions preserving canonical state owner, event/slot order, generation/readset, queue bounds, fairness, timestamps, singleflight and all fail-closed behavior. Do not add network waits/locks in the hot path. Each extraction separately diff/replay/regression checked; rollback/stop on divergence.
5. **A4 – Market and Winner:** Decompose `market_observer.rs` only at genuine decode/capture/verification/analytics boundaries. Avoid touching pinned `market_gap.rs`, `market_reconcile.rs`, `market_truth.rs` until ABI/hash/reader-proof review. Keep original raw capture, index/strict verifier, immutable proof IDs and separate Market/Shadow provenance.
6. **A5 – WebSocket and Execution:** Only if useful, split `ws_rpc.rs` into connection/subscription/notification units, preserve WebSocket ACK/StreamReady, Gap, recovery and source stamps. Separate genuine TX submission adapter from orchestration; keep one-shot/UNKNOWN and fee/handoff guards.
7. **A6 – Naming and optional root move:** Do NOT rename crate `runtime`. Rename inner concepts only where true boundary exists (e.g. `execution_policy.rs`, `analytics/market_economics.rs`). Keep `operations` and `scripts`. Consider `native/ → engine/` ONLY in a separate fully qualified path migration: `include_str!`, cargo workspace/toolchain, Cargo package names, Python release paths, scripts, documented/hashed evidence, user-CLI paths, historical resume. No automatic move if a pinned path blocks it.
8. **A7 – independent final proof and architecture freeze:** Both specified Rust feature configurations, default workspace handling, cargo metadata/locked builds, Python offline tests, read-only CLI, historical Replay V1/V2/Strict, complete safety negative cases, rustfmt changed files, Clippy differential, measured performance vs baseline or `NOT_MEASURED`. Stop at open fail. Leave changes uncommitted/unpushed for explicit user review.

**Never perform the next group after a FAIL.** Report proposed changes, expected effect, exact regression/test evidence and ownership after each group. Do not run live-chain tests, paid RPC, monetary transactions, or create a dummy/fake external executable to claim an actual end-to-end pass.

## 6. Safety, data and Git guardrails

- Preserve all original raw and pinned evidence, immutable hashes, V1/V2 data, stable run/verification/report identities, historical readers and strict-verification semantics. No editing original evidence or auto-moving ~75 GB local runs.
- Preserve explicit chain state freshness/reconciliation, fee/route/health UNKNOWN rejection, no stale candidate, no duplicated signed intent, no unapproved LIVE/Paid fallback. Shadow must execute identical decision code up to explicit send boundary.
- No new Crates, microservices, double engines, unnecessary dirs, staged fake progress or blanket mocking.
- No changes to `bsc/**`, `prestudy/**`, `frontend/**` or other projects. No `git clean`, `git reset --hard`, `git add -A`, whole-repo stash/pull that risks local work, or automatic commit/push/branch.
- A test marked `ignored` is not executed; running it manually does not retroactively eliminate prior ignore counts. Static structure/formats are not economic or Mainnet certificates. Any missing benchmark = `NOT_MEASURED`.
- Temporary `delete/` is a review quarantine, not a deployed code path or final target. Only the user authorizes final removal.

## 7. Final deliverables to user

- Compact current-vs-target tree; full active-file/consumer/owner matrix.
- Exact minimal code-change sequence and the smaller areas explicitly **left unchanged**.
- Correctness results for canonical state, concurrent candidates, WebSocket gaps, P4A exact economics, JIT, transaction bytes, wallet holds, read-only capture/strict proof, winner provenance and V1/V2 replay.
- Before/after test, warning and performance results with PASS / FAIL / NOT_RUN / NOT_MEASURED and original evidence and hashes.
- Clear list of deferred features, open external provider constraints and remaining FE product gates.
- Git worktree/index state, with no cross-project changes or unapproved push.

**Initial execution instruction: A0 ONLY, then stop for review. Do not infer approval for A1–A7 from this document.**
