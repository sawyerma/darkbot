## P2 continuation — 2026-10-12

P2 transport/provider implementation is **LOCAL PASS**, with 120 s / 8-attempt
outage recovery, advancing-slot readiness/liveness, coalesced gap intervals,
shared bounded provider admission/429 cooldown and early incomplete shutdown.
Existing state/recovery/economics/ObserveOnly paths reused. No Mainnet measurement
or submission performed. P4–P6 and provider qualification remain OPEN.

Evidence, exact commands, known limits and consumer inventory:
[`docs/evidence/C0E1_P2_20261012/README.md`](../../evidence/C0E1_P2_20261012/README.md).
Current resume status: `MEASUREMENT_LEDGER.md` (P2 entry at the top).

---

## P3 — UTC historical index verification, 2026-10-11

**P3 IMPLEMENTED / LOCAL TECHNICAL PASS. Real five-hour historical qualification: OPEN (`RPC_RATE_LIMIT`), not a zero-liquidation result. Stop after this package; next P2.**

Implementation extends `scripts/audit_market_quick.py`; entry point:

```bash
./project_0/scripts/verify.sh --run market5h_20261011T010758Z --mode index
# Bounded historical qualification / resume (same persisted cursor and raw responses):
./project_0/scripts/verify.sh --run market5h_20261011T010758Z --mode index --max-seconds 60 --max-pages 8 --max-calls 100 --attempts 2
```

- P1 manifest + CAPTURE journal supplies exact UTC nanosecond boundaries. Legacy reports supply start + requested duration; actual end is retained separately. Last observed slot never truncates the requested historical window. No five-hour constants in production code.
- Finalized `getSignaturesForAddress` pages continue through short pages until an entirely timed page precedes start. Dedupe, conflicting duplicates, reverse slot order, cursor stalls, null blockTime and history/page-budget exhaustion are explicit. `getTransaction` fetches all eligible indexed TX, including failed ones and unknown-time rows; no liquidation prefilter. Only those two RPC methods allowed; no full-block downloads.
- Existing independent base58/discriminator/frame decoder extended with pinned 0.1.11 event payload layouts, known instruction/event surface, outer/CPI execution positions, loaded addresses, versions legacy/0/1, exact instruction target/actor binding, start/end separation, caught-CPI rollback/truncation/malformed/unknown handling. Event position, target, actor, amounts, meta.fee, fee payer and raw token/lamport balances retained; missing prices/costs stay unknown. Runtime never invokes the bot decoder to establish external truth.
- SQLite disk-backed, bounded import and signature state; atomic per-page cursor/checkpoint, cached original RPC responses, exclusive writer lock, immutable run/window/decoder identity and opaque provider binding. Successful responses survive interruption before DB checkpoint; already completed TX/pages are not fetched again. Local input size/mtime guards prevent silently mixing altered capture files. Per-invocation rate/call/time/response-byte limits and bounded HTTP-429 Retry-After/backoff. No endpoint or credential logging.
- Comparison by signature/slot/event position, type, target/actor and comparable amounts. Live records, independently decoded live payload and canonical records remain separate. A missing record inside a documented outage differs from a missing liquidation record with an existing live TX; merely observed slot intervals do not establish universal provider coverage. Legacy non-receivership live detection was not always materialized, therefore payload presence is **DETECTION_UNPROVEN**, not a invented bot detection/miss claim.
- Shadow join consumes existing materialized `canonical_market_shadow.jsonl` and `opportunity_economics.jsonl`, preserving available economics/capital/race evidence. `SHADOW_MATCH` requires event identity; absence means no evidence in those reports, **not** proof no raw Shadow evaluation existed. This verifier does not re-run simulation, re-price old assets or infer historical live execution speed/profit.
- Outputs under `<run>/verification/index_v1/`: `checkpoint.sqlite3`, immutable page/transaction response files, per-attempt summaries, `comparison.jsonl`, `bot_only.jsonl`, `summary.json`, `report.txt`. Finalized index coverage can produce only `INDEX_VERIFIED`; `canonical_verified=false`, `c0_e1_master_pass=false`, LIVE disabled. Cross-provider verification is not claimed; strict remains its existing separate qualification. Provider/decoder identity changes deliberately reject mixing persisted evidence and require an explicit migration/new evidence namespace.

Qualification actually executed:

```bash
python3 -m unittest discover -s project_0/scripts -p test_audit_market_quick.py -v
./project_0/scripts/verify.sh --self-test
python3 project_0/scripts/test_measurement_plan.py
python3 -m py_compile project_0/scripts/audit_market_quick.py project_0/scripts/test_audit_market_quick.py
bash -n project_0/scripts/verify.sh
git diff --check
```

**26 tests PASS / 0 FAIL**, including six existing real Mainnet receivership TX fixtures from `docs/evidence/AUDIT_20261005`; classic/deleverage/bankruptcy tested with synthetic payloads derived from the exact pinned event schema (not claimed as real Mainnet positives). Includes fractional P1 UTC, legacy planned-vs-actual end, duplicate/short pages, checkpoint interruption/resume without repeat fetch, unavailable TX then resume, 429 Retry-After and finite retry/secret-free errors, unsupported versions, CPI/LUT-only program references, failed/foreign/truncated/malformed events, amount/type/slot mismatch, Shadow identity and unknown economics. CLI self-test repeats the same 26 tests. Existing P1 CLI/release-plan integration **4 PASS**. No Rust source changed; no redundant Rust rebuild/workspace tests claimed.

**Real preserved five-hour run:** automatically found `project_0/data/econ4h/market5h_20261011T010758Z`; derived `2026-10-11T01:08:38Z` through `06:08:38Z` from its report. Imported **181 live TX**, full local coverage file and materialized Shadow reports. Last slot evidence at **01:42:38Z**, unresolved GAP starts there, **15,960 seconds** without later slot evidence. Four total RPC attempts (including failures) returned **two pages / 2,000 persisted signatures**, **504,600 response bytes**, then bounded retry stopped with **RPC_RATE_LIMIT**. Start not yet crossed; **no historical getTransaction evaluation completed**. Historical liquidation counts remain **UNKNOWN**; zeros in observed counters do not prove absence. Cursor, original responses and local-file offsets remain available for the resume command above. No repeated quota probes or new capture performed.

Evidence: `project_0/docs/evidence/C0E1_P3_20261011/` (tests, source/fixture hashes, real partial summary, resume proof, exact command). Existing raw runs untouched; only new verifier-owned output created inside the old run. Initial P1 base was `a097a59`; concurrent local commit `bc5a18a` checkpointed this implementation's unfinished prefix while BSC work progressed. Those commits were preserved; final P3 staging contains Project_0 files only.

**Next:** report P3 commit/push and stop. P2 WebSocket/provider stability remains next, P4 strict partition/resume remains OPEN. No long run, provider purchase, LIVE or submission authorized by this result.

---

## P1 — Dynamic MeasurementPlan, 2026-10-11

**P1 IMPLEMENTED / LOCAL TECHNICAL PASS. No Mainnet capture performed. P3 next; P2/P4–P6 remain OPEN.**

Authorized baseline: local `8f4e9d21dd7da5d9754fb14193ebac6268508b5e` fast-forwarded to remote `504353d88e1c45e1ed695eb7eb47fa516ac3e516` after status/fetch/delta checks. The accepted remote delta was only `bsc/tools/g07_pre_live_readiness.sh`; no local BSC work was edited or staged. Existing untracked run data was preserved.

Implemented:
- `project_0/scripts/test.sh --duration 10m|1h|5h|24h`; also whole-number `s/m/h` durations resolving to 1..86400 seconds. No second snapshot time parameter required.
- Single typed Rust `MeasurementPlan`, persisted as `measurement_plan.json` by the offline `p0-measurement-plan` compiler, then loaded by the runner. Four exact nanosecond quarter boundaries, including 1s and 61s, shared by market checkpoints, live/chain attribution, analytics and reports. One `SNAPSHOT_COUNT` source. Existing live `(previous_end,end]` and chain `[start,end)` conventions preserved explicitly.
- Bootstrap and StreamReady precede actual capture; planned cutoff is start + full duration. Separate bootstrap, stream-ready, Shadow drain, market drain, quiescence, strict, gap and enrichment budgets. Owner watchdog sums phase budgets rather than limiting the whole process to capture duration. Removed implicit short/auto wall-timebox; explicit obsolete `quick` fails with an explanation.
- Exclusive `measurement_manifest.json` before producers start: plan/schema, fixed monotonic/UTC clock origin, Git HEAD, SHA256 source/binary identities, sanitized provider class, numeric risk settings and observe-only permissions. Append-only, synchronized `measurement_phases.jsonl`: bootstrap, readiness, exact capture window/cutoff, drain, verification, owner state and final reports. Exact UTC nanosecond fields retain fractional boundaries. Report time comes from the shared Clock, not a later wall-clock sample.
- Legacy `run_econ4h.sh`, consistent `P0_ECON_DURATION_SECS`/`P0_ECON_SNAPSHOT_SECS`, existing report names `hour_01..04.json` and historical run files remain supported. Conflicting time overrides fail closed; old fractional-incompatible snapshot settings must be unset. New additive `measurement_plan`, `capture_window` and offset-ns metadata define the meaning of snapshot filenames. No trading/economics formulas or submission gates changed.

Local qualification (commands run from repository root):

```bash
cargo test --locked --manifest-path project_0/native/Cargo.toml -p darkbot-runner --features crate_health,txv1
cargo test --locked --manifest-path project_0/native/Cargo.toml -p darkbot-runner --features crate_health
cargo test --locked --manifest-path project_0/native/Cargo.toml -p darkbot-runner --features crate_health,txv1 measurement
cargo test --locked --manifest-path project_0/native/Cargo.toml -p darkbot-runner --features crate_health,txv1 runtime_worker_writes_four_snapshots_then_identical_final
cargo build --release --locked --manifest-path project_0/native/Cargo.toml --features crate_health,txv1 --bin p0-longtail --bin p0-measurement-plan
python3 project_0/scripts/test_measurement_plan.py
bash -n project_0/scripts/test.sh project_0/scripts/run_econ4h.sh project_0/scripts/run_gap_reconnect_smoke.sh
cargo clippy --locked --manifest-path project_0/native/Cargo.toml -p darkbot-runner --features crate_health,txv1 --all-targets --message-format=json
```

- Each full runner configuration: **192 passed, 0 failed, 1 existing ignored** (`winner::tests::f8_probe_sdk_encoded_transaction_decoding`). Final txv1 suite rerun after fractional UTC/report journal changes. Health suite preceded those presentation/journal changes; no claim of a subsequent whole-workspace run.
- Additional focused measurement regressions **7 passed** and real four-second automatic snapshot worker **1 passed**. Invalid/overflow/Unicode durations, legacy conflict rejection, exclusive manifest persistence and fractional boundary attribution covered.
- Release build **PASS**; offline CLI/launcher tests **4 passed**. Launcher test uses the real release plan compiler, dummy signer, mocked Cargo/storage and a fake capture executable: verifies forwarding, persisted plan, matching storage duration, safe flags and no quick timebox. It is not a Mainnet or real Shadow startup smoke.
- Format **PASS for changed Rust files and the changed `run_econ4h` function in main.rs**; shell syntax and `git diff --check` PASS. Whole-workspace formatting was not claimed or applied. Clippy completed successfully with existing diagnostics; no warning-free workspace or full differential-Clippy claim.
- Evidence and retained logs: `project_0/docs/evidence/C0E1_P1_20261011/`.

**Limits / exact next step:** P1 proves time planning and local integration, not uninterrupted 5h/24h market coverage or economic profitability. Legacy strict verification is retained; `--verify index` and the automatic historical UTC verifier belong to **P3** and are not exposed as working here. Existing strict 100,000-slot limit and streaming/resume are **P4 OPEN**, including 24h strict qualification. Provider 429/reconnect stability is **P2 OPEN**. No new long Mainnet run, LIVE, submission, provider purchase or evidence deletion. Stop after P1 commit/push status; next user-directed package is P3 on the existing auditor.

---

> Historical isolated implementation report. Its statements about the production repository being untouched apply to package creation. Subsequent local integration is recorded in MEASUREMENT_LEDGER.md and project_0/evidence/C0_E1_local_integration_20261008/.

# C0-E1 Implementation Report

## Verified continuation — 2026-10-10 19:57 Europe/Berlin (WIP, not final qualification)

Latest observed source checkpoint: GitHub WIP branch `project0/c0e1-startup-wip-20261010` at `c2ba995e647296d1c3d3b0a134d2f30ff8db1d64` (before documentation commits), separated from `main` `3fdf30b`. Six Astra-modified Rust files implement compressed getProgramAccounts snapshots (`base64+zstd`), true Shadow-READY startup barrier before market observation, first-ready timestamp, no technical PASS with zero in-window evaluations, and cancellable fail-closed recovery. Two **local, not yet pushed** test-only fixture fixes: remove ambiguous `.into()` from two snapshot mock-RPC tests and inject one synthetic Shadow evaluation in the four-report worker fixture. No production implementation was edited by these fixes.

User-reported validations after both fixes: `darkbot-solana state::` 19 PASS/0 FAIL (20 filtered); full `darkbot-runner --features crate_health` exit0 and `--features "crate_health txv1"` exit0; release binary build exit0; Clippy all-targets txv1 exit0 (three warnings in bin, differential not established). Before second fixture fix, Runner had 173 PASS/1 FAIL/1 ignored; the updated isolated test subsequently passed. These are static/local validations, not full Mainnet qualification.

Active real 60-second **capture** (not 60-second walltime) controlled market WSS interruption smoke `c0e1_gap_smoke_20261010T195016Z`:
- Shadow startup attempt1 snapshot A failed decoding the RPC response body at `liability_gpa`. Attempt2 Snapshot A: slot455376771, 83,516 total, 82,810 liabilities, 204 banks, 494 required, 8 optional, 0 missing required, 28,425ms. B: slot455376902, same counts, 29,561ms.
- Root catch-up from Some(455376992) to target455377037 succeeded in12,546ms. Log has `recovery: Ready, run_state=Ready` **before** `C0_E1_WARMUP_READY; starting market stream and measurement window`: the startup sequencing worked in this observed trial.
- During subsequent observation the `ws_rpc` source logged a connection reset without closing handshake and automatic attempt2 reconnect, two subscription ACKs and StreamReady. That is reconnect evidence; the supplied excerpt does not yet prove the proxy-induced interruption marker, post-capture finalized all-slot backfill, `GAP_RECOVERED`, zero unresolved slots, or `CONTROLLED_GAP_SMOKE_PASS`.
- Intermediate Shadow ingress latest sent/received29978/29978 overflow0; health sweeps 1,111,897, liquidatable classifications97,536, health errors4,504, winner requests dispatched265 and results8, resolved0. Intermediate, not the final economics sample, and NOT observed market liquidations / profit.
- Existing qualification proxy interrupts the **market** WSS only. A separate intentional interruption of Shadow's WSS remains an unproven case.

**Gate stays PENDING.** Inspect after test completion: `project_0/data/econ4h/c0e1_gap_smoke_20261010T195016Z/runtime.log`, `gap_proxy.log`, `market/coverage.jsonl`, `market/summary.json`, `economics_final.json`, `hour_01..04.json`, plus exit code. Only an actual full-slot finalized reconciliation with 0 unresolved slots, gap recovery, real Shadow evaluations, clean shutdown and 0 submissions can support the complete short smoke. No 4h re-run until then. LIVE and submission disabled. Local evidence not automatically included in the GitHub documentation commit. See top of `MEASUREMENT_LEDGER.md` for full handoff.

---


## Transport completion — 2026-10-10 (current local candidate)

This section supersedes older package-only status for the current working tree.
Base checkpoint: `2c03b03ad166b0c92589da295c3ff5284a51e2db` (user-pushed).
Evidence: `docs/evidence/C0E1_TRANSPORT_FINISH_20261010/`.

Confirmed defects repaired:
- Test proxy had advertised a local listener before proving upstream availability.
  Readiness now requires the exact endpoint's successful WebSocket handshake,
  both market subscription ACKs and a real slot. A failed preflight exits2 without
  a readiness file; the launcher never starts the expensive combined measurement.
- Proxy diagnostics now include safe IO kind/OS code and HTTP status, never the
  library's URL-bearing error text. Existing tool supports a short direct probe
  and controlled proxy disconnect/reconnect probe; it stays outside normal ingress.
- A retry of a prefetched failed block could extend the read-ahead batch. Retries
  now request only that slot, without more speculative traffic. All reconciliation
  request starts use the same5/s pacer;5 bounded attempts, exponential transient
  backoff1/2/4/8 seconds, total phase deadline unchanged.
- Deployment HTTP failures formerly collapsed to DEPLOYMENT_RPC. Three bounded
  attempts retain safe timeout/connect/rate-limit/decode categories. An unknown
  hash or incomplete deployment evidence still prevents qualification.
- The combined owner formerly discarded Shadow's returned failure. It now logs
  a typed, secret-free failure category before setting the existing failed flag.

No trading, health, sizing, sign/submit, candidate choice, market identity, or
financial valuation semantics changed in this follow-up. The existing final
join continues consuming production cost evidence; missing terms remain null.

Historical diagnosis limitations: the IO-only log from T131358Z does not prove a
specific endpoint/TLS/provider root cause. Its independent deployment HTTP failure
also lost detail; Shadow's original returned error was not retained. The barrier
failure follows Shadow's early exit, and is not evidence of a barrier deadlock.
Current direct HTTP and WSS checks, then the short proxy reconnect, passed.

The supplied terminal transcript reports300/302 workspace tests PASS,0FAIL,
1ignored each. It does not capture independent shell exit codes for every command.
All39 files reported by its global fmt check already differ from rustfmt at the
pinned b7f5ed3 baseline. No broad formatting cleanup was performed. Edited modules
are checked separately; whole-workspace fmt must not be called PASS.

Qualification status remains PARTIAL until the current combined600s smoke and
final matrix are complete. No4h measurement is started by the agent. Commands,
exits and initial failed regression assertion are preserved, not overwritten.


## Before any source changes

Read and confirmed package SOURCE_COMMIT.txt: **57bef334e60764f614712785438a95edb7638352**.
Marginfi pin: **33c67987a661f59c6683a65485ebb4f0ef030b77**.
ZIP SHA256: e829116253cd14da0236e31b0b10dd469becb52dc5d309406a2758600634b850.
All 275 supplied checksums verified. FABLE_TASK.md read in full; README, Project README,
PHASENPLAN and baseline build status read. Work uses the exported repo only; no production
repository reads/writes or integration. Baseline logs are not candidate evidence.

Understood non-claims: Shadow is not realized PnL, guaranteed capture, true competitor send time,
private-relay deadline, future daily/monthly income, P8 LIVE qualification, or verified provider
source coverage. UNKNOWN stays UNKNOWN. Same-slot ambiguity is never resolved by transaction index.
Market truth remains independent. No submission permitted. No dependency/pin changes planned.

Status: **IMPLEMENTED / COMPILED / LOCAL TESTS PASS / MAINNET NOT QUALIFIED**.

The user explicitly shortened this turn to implementation, basic local checks and delivery;
the user will perform further qualification to conserve credits. No 600-second Mainnet smoke,
four-hour observation or production integration was performed. No push, no production commit,
no LIVE submission, no reads of production secrets. All source came from the supplied ZIP.


## Architecture and changed scope

- `econ.rs`: combined-mode configuration, ObserveOnly submission barrier, shared Clock handle,
  bounded typed evidence queue, analytics worker, correlation, capital/race/stage/bottleneck
  aggregates, atomic snapshots, automatic final report and explicit non-claims.
- `econ_tests.rs`: 22 new deterministic/isolated tests, including the existing Pipeline's actual
  Build/Sign/Simulation call flow with counting submitter and a real four-boundary worker timer.
- `record.rs` and `pipeline.rs`: retain CandidateInput/Evaluation data for every evaluated plan
  and the exact selected plan, rather than reconstructing economics offline. Missing fields
  remain null. Absolute timing/slot points and simulation outcome become evidence. Bundle
  signature timing records completion of all constituent signatures. Trigger slot is retained
  independently of the projection's simulation slot.
- `market_observer.rs`: optional typed taps on the existing successful decoder/aggregator,
  slot first-seen events and fixed-size counter snapshots; same core and same r6/F8 truncation
  finishing path. No added RPC in on_logs or ingress capture. Configurable caps only in econ4h
  explicitly fail qualification on retention overflow. Standalone market mode retains its flow.
- `main.rs`: runs existing Shadow owner and embedded observer with the same Clock, stops new
  admissions at the market cutoff, drains bounded winner evidence, publishes runtime status,
  and coordinates final analytics. No new state owner or independent trading algorithm.
- `run_econ4h.sh`: existing owner-only .env/external-keypair/public-identity validation pattern,
  hard disabled LIVE/submission, release build, detailed file logs and compact terminal summary.
- README/PHASENPLAN: entry point, schema, non-claims and outstanding qualification gates.

No dependency or Cargo.lock change. No unrelated project changes. Complete changed-file list:

- `project_0/README.md`
- `project_0/docs/PHASENPLAN.md`
- `project_0/scripts/run_econ4h.sh`
- `project_0/native/crates/darkbot-runner/src/lib.rs`
- `project_0/native/crates/darkbot-runner/src/econ_tests.rs`
- `project_0/native/crates/darkbot-runner/src/market_observer.rs`
- `project_0/native/crates/darkbot-runner/src/econ.rs`
- `project_0/native/crates/darkbot-runner/src/main.rs`
- `project_0/native/crates/darkbot-runner/src/pipeline.rs`
- `project_0/native/crates/darkbot-runner/src/record.rs`

## Economics, capital and UNKNOWN semantics

No second economics engine: records copy the production CandidateInput and Evaluation::Known.
Gross realizable value, repayment, protocol fee, flat SOL fee, swap fees/impact, exact-message
transaction fee, path-specific tip, rent/token cost, failure reserve, working capital and
profit/capital are retained. Candidate choice remains the existing pipeline's ranking.

Reserve = known transaction fee + actual path tip + known rent + flat SOL fee + protocol fee +
failed-attempt reserve. Working capital is separate. Total scenario budget is the conservative
sum; checked arithmetic returns UNKNOWN on absent terms/overflow. ZERO_PREFUND retains its fee
reserve. Economics Unknown cannot become a known executable or fundable opportunity. Existing
zero-capital profit/capital sentinel is retained in evidence but excluded from finite ratio
percentiles and counted as zero-denominator evidence. No universal rent or fee constants added.

Default buckets: 200/500/1000/2500/5000/10000 USD; configurable. Counts, edge, per-hour edge,
PRE/SAME/POST/unresolved subdivisions, unknown counts and capital/efficiency distributions are
emitted per bucket. These are independent opportunity eligibility scenarios, not a proof of
simultaneous capital allocation or realized return.

## Market / winner / clock contract

C0-MO remains the market truth; Shadow selection cannot remove a market liquidation.
One Clock is constructed in run_econ4h and copied to both existing ingress paths and the owner.
The timestamp of delayed WinnerTracker RPC completion is never a competitor race timestamp.
Unique F8 identity is correlated by target + signature + receiver + slot against a live decoded
successful market event. Mismatch disqualifies timing. Ambiguous winners retain all F8 candidates;
available transaction indices are evidence only. No-sign/missing winner/missing live match stay
explicit. Signed local margins and speedups are computed in i128/u64 ns, with ms convenience
fields. True send/relay deadlines remain UNKNOWN. Stage attribution uses the current pipeline's
existing marks; its T_state means eligibility/state check, not a separately measured canonical
apply timestamp. Missing or inverted intervals remain null.

Full raw evaluation evidence is streamed to shadow_evaluations.jsonl. Bounded retained records
are coalesced for opportunity accounting: earliest executable sign per target/authoritative
winner; unresolved targets use latest per target/hour. Repeated evaluations cannot multiply
one winner's edge. Matched rows are assigned to the observed winner's hour, unmatched rows to
trigger receive hour. Both decisions and candidate costs remain in audit rows. The existing
WinnerTracker relevance policy is unchanged, so technical-only/unsupported candidates may have
no authoritative winner. They remain unresolved, never assigned a guessed competitor.

## Output schema and automatic lifecycle

hour_01.json ... hour_04.json and economics_final.json contain identity, window,
technical_qualification, market, shadow, matching, economics, capital, competition, race_timing,
bottlenecks, unknowns, non_claims and separate economic sample/edge/competition statuses.
Metric sections have window_delta and cumulative. Source identity resolves only from a clean
real Git checkout; archive/dirty builds say UNKNOWN. Marginfi pin is retained. Source coverage
is always false and live_ready is always false.

Live hourly files are provisional. After bounded drain/enrichment they are atomically replaced
with reconciled evidence; hour 4 and final are identical. `capture_counters` are receive-time
interval deltas, even if processing crossed the boundary. Original r6 checkpoints remain
visible; their deferred RPC/truncation qualification is labelled pending in hours 1–3, with
post-capture resolution recorded in the final r6 summary/resolution window. This avoids changing
r6's prohibition on RPC during logs capture.

Default observation 14400s / 3600s; smoke 600s / 150s. Four equal intervals required. Clock cutoff
stops new Shadow admission and C0-MO capture. Market drain/enrichment and bounded winner lookup
may extend process wall time. Any still-unresolved winner at its bounded deadline remains
unresolved and its pending count is reported. Queues and retained records are bounded; drops
and retention overflow fail qualification. Market incomplete, early Shadow termination, upgrade
halt, missing snapshots, writer errors or attempted submission cannot yield PASS. Missing
sample is distinct from technical failure. No Ctrl+C needed on normal completion.

## Validation actually performed

Environment: local Apple/macOS host; rustc 1.93.1 (Homebrew), cargo 1.93.1 (Homebrew).
All commands below ran against the returned candidate (debug builds, not release).

### build_health_final

Working directory: `/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo/project_0/native`

```bash
cargo build --locked --features crate_health
```

Exit code: 0.

### build_txv1_final

Working directory: `/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo/project_0/native`

```bash
cargo build --locked --features 'crate_health txv1'
```

Exit code: 0.

### tests_health_final

Working directory: `/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo/project_0/native`

```bash
cargo test --locked --workspace --features crate_health
```

Exit code: 0. Counts: {'passed': 242, 'failed': 0, 'ignored': 1}.

### tests_txv1_final

Working directory: `/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo/project_0/native`

```bash
cargo test --locked --workspace --features 'crate_health txv1'
```

Exit code: 0. Counts: {'passed': 244, 'failed': 0, 'ignored': 1}.

### format_new_modules

Working directory: `/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo`

```bash
rustfmt --edition 2021 --config skip_children=true --check project_0/native/crates/darkbot-runner/src/econ.rs project_0/native/crates/darkbot-runner/src/econ_tests.rs project_0/native/crates/darkbot-runner/src/record.rs
```

Exit code: 0.

### script_syntax

Working directory: `/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo`

```bash
bash -n project_0/scripts/run_econ4h.sh
```

Exit code: 0.

Existing ignored test in both feature sets: `winner::tests::f8_probe_sdk_encoded_transaction_decoding`.
It is not counted as PASS. Existing six historical audit fixtures, r6 truncated-log qualification,
failed-transaction exclusion, loaded-address parsing and F8 ambiguity regressions all ran within
the workspace suites. Final counts include 22 new econ tests.

The compiled p0-longtail binary was also invoked with controlled non-network startup settings:
P0_MODE=live; P0_MODE=econ4h + P0_LIVE_ENABLED=true; and P0_MODE=econ4h +
P0_SUBMISSION_ENABLED=true. Each returned exit 2 before any network/keypair setup.

Patch qualification (in disposable copies, outside production):

```bash
git diff --check
git diff --binary --no-ext-diff > 0001-C0-E1-4h-economic-race-harness.patch
git apply --check /absolute/path/0001-C0-E1-4h-economic-race-harness.patch
git apply /absolute/path/0001-C0-E1-4h-economic-race-harness.patch
```

PASS on an independent pristine, checksum-verified export of declared base
57bef334e60764f614712785438a95edb7638352. Every applied file was byte-compared and permission-mode
compared with end_state. The package contains no Git history; no fabricated Git commit identity
was used as proof. Integration must still independently check its actual checkout HEAD equals
that exact commit. Package checksums establish its supplied source identity, not remote history.

Early compiler checks encountered two implementation mistakes (an accidental extra MarketConfig
field and a closure lifetime); both were fixed before candidate gates. Intermediate check/test
commands and logs are included below; supplied baseline PASS logs were not reused as candidate PASS.

## Tests added

- `config_requires_four_snapshots_and_live_disabled`
- `shared_clock_preserves_origin_across_producers`
- `economics_input_and_profit_capital_preserved`
- `unknown_costs_and_economics_remain_null`
- `zero_prefund_does_not_erase_fees_and_unknown_budget_is_not_fundable`
- `every_default_capital_boundary_includes_reserve`
- `unique_winner_slot_classes_and_signed_margins`
- `identity_mismatch_never_yields_timing`
- `absent_sign_winner_market_and_ambiguous_identity_fail_closed`
- `small_sample_percentiles_and_missing_intervals`
- `strong_join_unmatched_diagnostics_and_speedup_math`
- `winner_without_live_observation_is_visible_and_not_race_qualified`
- `simulation_bottleneck_and_unknown_edge_not_fabricated`
- `all_loss_reasons_are_retained_exactly`
- `bounded_queue_and_retention_overflow_fail_qualification`
- `early_end_missing_snapshot_upgrade_and_writer_failures_are_incomplete`
- `four_boundaries_delta_cumulative_and_final_reconcile`
- `atomic_writer_only_exposes_whole_json`
- `counters_use_event_receive_hour_even_if_processed_after_boundary`
- `actual_shadow_pipeline_builds_signs_simulates_and_never_submits`
- `runtime_worker_writes_four_snapshots_then_identical_final`
- `unknown_capital_does_not_remove_market_match`

## Not run / handoff gates

- Candidate release build: NOT RUN here; the runner script performs it before a live observation.
- Full candidate-only formatting gate across all legacy modified files: NOT RUN. rustfmt check
  passed on econ.rs, econ_tests.rs and record.rs; legacy files keep their existing style.
- Differential Clippy vs exact base, both feature sets: NOT RUN, left for integration.
- 600-second combined Mainnet smoke: NOT RUN.
- 14400-second observation: NOT RUN.
- Runtime performance/queue-capacity adequacy under Mainnet volume: NOT PROVEN.
- Source coverage, actual realized PnL, P8/LIVE: NOT QUALIFIED; LIVE remains blocked.

No claim that green unit tests establish production economic competitiveness. The return is a
compiled implementation candidate for the user's independent review and integration workflow.
The default bounded retention may need tuning after the smoke; overflow explicitly invalidates
qualification rather than silently dropping evidence. Existing Shadow scheduling, freshness,
inventory/reservation limitations and remote request budgets were not optimized in this task.

## Safety startup command results

```json
[
  {
    "mode": "live",
    "extra": {},
    "exit_code": 2,
    "stdout": "",
    "stderr": "p0-longtail: live mode requires P8-LT qualification in MEASUREMENT_LEDGER; not enabled\n"
  },
  {
    "mode": "econ4h",
    "extra": {
      "P0_LIVE_ENABLED": "true"
    },
    "exit_code": 2,
    "stdout": "",
    "stderr": "p0-longtail: P0_LIVE_ENABLED must be false in econ4h\n"
  },
  {
    "mode": "econ4h",
    "extra": {
      "P0_SUBMISSION_ENABLED": "true"
    },
    "exit_code": 2,
    "stdout": "",
    "stderr": "p0-longtail: P0_SUBMISSION_ENABLED must be false in econ4h\n"
  }
]
```

## Exact final gate logs

### build_health_final

```text
   Compiling darkbot-runner v0.1.0 (/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo/project_0/native/crates/darkbot-runner)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.42s

```

### build_txv1_final

```text
   Compiling darkbot-runner v0.1.0 (/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo/project_0/native/crates/darkbot-runner)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.41s

```

### tests_health_final

```text
   Compiling darkbot-runner v0.1.0 (/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo/project_0/native/crates/darkbot-runner)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.15s
     Running unittests src/lib.rs (target/debug/deps/darkbot_common-84f89d61913e24bf)

running 26 tests
test config::tests::missing_is_error_not_default ... ok
test channel::tests::disconnect_is_reported ... ok
test channel::tests::overflow_is_explicit_and_returns_element ... ok
test error::tests::display_carries_reason_and_detail ... ok
test config::tests::parsed_or_uses_explicit_default_and_rejects_garbage ... ok
test histogram::tests::empty_is_none_not_zero ... ok
test histogram::tests::index_and_upper_are_consistent ... ok
test histogram::tests::merge_keeps_extremes ... ok
test journal::tests::append_and_replay_in_order ... ok
test journal::tests::gap_is_rejected ... ok
test histogram::tests::percentiles_are_conservative_and_bounded ... ok
test lifecycle::tests::bootstrapping_is_not_tradable ... ok
test lifecycle::tests::halted_keeps_first_reason_and_needs_explicit_recovery ... ok
test lifecycle::tests::shutdown_flag ... ok
test mt0::tests::error_classification_separates_429_timeout_and_other ... ok
test mt0::tests::histogram_is_deterministic_bounded_and_conservative ... ok
test mt0::tests::long_running_section_is_visible_before_it_ends ... ok
test mt0::tests::remote_classes_and_outcomes_are_separate ... ok
test timing::tests::clock_is_monotonic ... ok
test timing::tests::stage_names_are_ledger_vocabulary ... ok
test timing::tests::timeline_intervals_and_no_overwrite ... ok
test timing::tests::timeline_rejects_negative_interval ... ok
test mt0::tests::owner_sections_sum_to_elapsed_time ... ok
test mt0::tests::trigger_mapping_and_health_counters_count_actual_calls ... ok
test mt0::tests::unmeasured_metrics_are_marked_not_zero_and_snapshot_is_bounded ... ok
test channel::tests::pop_timeout_returns_none_on_empty ... ok

test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running unittests src/lib.rs (target/debug/deps/darkbot_project0-e2c2a76fe79d3b4a)

running 60 tests
test economics::tests::exact_rpc_fee_qualifies_only_rpc_scenarios ... ok
test economics::tests::funding_class_is_not_a_hardcoded_preference ... ok
test economics::tests::equal_net_prefers_capital_efficiency ... ok
test economics::tests::premium_curve_matches_release_notes ... ok
test economics::tests::time_for_premium_inverts_curve ... ok
test economics::tests::seize_bound_from_source ... ok
test economics::tests::ranking_by_net_then_capital_efficiency ... ok
test economics::tests::t_profitable_solver ... ok
test economics::tests::unknown_cost_is_not_zero ... ok
test economics::tests::unknown_realizable_value_is_unknown ... ok
test execution::tests::freshness_route_fee ... ok
test execution::tests::gates ... ok
test execution::tests::backoff_grows_and_drops ... ok
test execution::tests::quarantine_expires ... ok
test funding::tests::emits_all_feasible_candidates_instead_of_picking_one ... ok
test funding::tests::inventory_classes_do_not_require_market_route ... ok
test funding::tests::jit_is_only_shortfall_fallback ... ok
test funding::tests::no_route_no_inventory_means_no_candidate ... ok
test health::threshold_search_tests::first_true_boundary_expands_above_current ... ok
test health::threshold_search_tests::first_true_boundary_finds_exact_integer_crossing ... ok
test health::threshold_search_tests::first_true_boundary_propagates_fail_closed_error ... ok
test index::tests::reverse_index_fanout_and_replacement ... ok
test index::tests::threshold_crossings_only_wake_newly_endangered_accounts ... ok
test index::tests::threshold_lookup_both_sides ... ok
test planner::tests::bundle_rules ... ok
test integration::tests::pinned_refresh_discriminators_match_expected ... ok
test planner::tests::end_must_be_last_without_exception ... ok
test planner::tests::no_swap_plan_rejects_swap_and_requires_repay ... ok
test planner::tests::non_allowlisted_swap_and_paid_route ... ok
test planner::tests::valid_swap_plan ... ok
test prepared::tests::exact_key_or_rebuild ... ok
test projection::tests::delayed_evaluation_uses_exact_simulation_clock_for_both_venues ... ok
test projection::tests::empty_and_zero_slot_projection_fail_closed ... ok
test projection::tests::negative_simulation_timestamp_fails_closed ... ok
test projection::tests::non_projected_account_falls_through_to_base ... ok
test projection::tests::projected_account_overrides_base_without_mutating_it ... ok
test projection::tests::writable_refresh_accounts_are_canonical_projection_targets ... ok
test protocol::tests::classify_and_layouts ... ok
test receivership::builders::repay_all_tests::partial_repay_does_not_append_health_observation ... ok
test receivership::builders::repay_all_tests::repay_all_carries_observation_accounts ... ok
test receivership::builders::repay_all_tests::classic_withdraw_does_not_append_mint ... ok
test receivership::builders::repay_all_tests::token_2022_repay_includes_mint_before_repay_all_observation ... ok
test safety::tests::limits_and_halts ... ok
test protocol::tests::pdas_are_deterministic ... ok
test receivership::builders::repay_all_tests::token_2022_withdraw_includes_mint_before_observation ... ok
test safety::tests::live_disabled_and_day_roll ... ok
test safety::tests::preview_does_not_reserve_capital ... ok
test schedule::tests::premium_boundary_math ... ok
test schedule::tests::scheduler_orders_and_invalidates ... ok
test sizing::tests::closeout_threshold_matches_protocol_units ... ok
test sizing::tests::drift_sizing_roundtrip_uses_scaled_balance_decimals ... ok
test sizing::tests::headroom_is_below_boundary ... ok
test sizing::tests::integer_conversions_are_conservative ... ok
test sizing::tests::near_full_rounding_uses_source_backed_repay_all ... ok
test sizing::tests::normal_and_closeout_bounds_are_distinct ... ok
test sizing::tests::ordinary_partial_repay_semantics_are_unchanged ... ok
test sizing::tests::r121_pair_heals_below_maximum_premium ... ok
test sizing::tests::repay_all_matches_protocol_opposite_dust_boundary ... ok
test sizing::tests::repay_all_never_clamps_a_target_above_full_transfer ... ok
test sizing::tests::rounded_health_keeps_normal_boundary_and_closeout_exception ... ok

test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (target/debug/deps/darkbot_runner-034bbfa47ebe952b)

running 114 tests
test builder::p0::fix100_tests::liquidation_record_link_controls_init ... ok
test builder::p0::fix100_tests::p1c_quote_profitable_floor_unprofitable_is_not_actionable ... ok
test builder::p0::fix100_tests::repay_all_removes_closed_bank_only_from_end_layout ... ok
test builder::p0::fix100_tests::non_budget_errors_are_not_retried ... ok
test econ::tests::all_loss_reasons_are_retained_exactly ... ok
test builder::p0::fix100_tests::p1c_zero_prefund_feasibility_uses_floor_not_quote ... ok
test builder::p0::fix100_tests::missing_floor_production_path_is_unknown_and_never_actionable ... ok
test econ::tests::absent_sign_winner_market_and_ambiguous_identity_fail_closed ... ok
test econ::tests::economics_input_and_profit_capital_preserved ... ok
test econ::tests::config_requires_four_snapshots_and_live_disabled ... ok
test econ::tests::every_default_capital_boundary_includes_reserve ... ok
test econ::tests::identity_mismatch_never_yields_timing ... ok
test builder::p0::fix100_tests::route_budget_counts_payer_and_resource_accounts_too ... ok
test builder::p0::fix100_tests::route_requotes_until_actual_serialized_transaction_fits ... ok
test builder::p0::fix100_tests::impossible_route_exhausts_bounded_budget_and_fails_closed ... ok
test econ::tests::shared_clock_preserves_origin_across_producers ... ok
test econ::tests::unique_winner_slot_classes_and_signed_margins ... ok
test econ::tests::unknown_capital_does_not_remove_market_match ... ok
test econ::tests::small_sample_percentiles_and_missing_intervals ... ok
test econ::tests::actual_shadow_pipeline_builds_signs_simulates_and_never_submits ... ok
test econ::tests::counters_use_event_receive_hour_even_if_processed_after_boundary ... ok
test econ::tests::early_end_missing_snapshot_upgrade_and_writer_failures_are_incomplete ... ok
test econ::tests::unknown_costs_and_economics_remain_null ... ok
test econ::tests::zero_prefund_does_not_erase_fees_and_unknown_budget_is_not_fundable ... ok
test fixture::run::tests::all_close_fixture_omits_both_closed_banks_from_end ... ok
test lookup_tables::tests::extension_slot_only_exposes_old_prefix ... ok
test econ::tests::strong_join_unmatched_diagnostics_and_speedup_math ... ok
test econ::tests::simulation_bottleneck_and_unknown_edge_not_fabricated ... ok
test lookup_tables::tests::invalid_and_deactivating_tables_are_excluded ... ok
test lookup_tables::tests::tables_reduce_compiled_size_and_merge_preserves_canonical_indices ... ok
test market_observer::tests::data_from_another_program_is_never_counted ... ok
test market_observer::tests::decodes_valid_receiver_event ... ok
test market_observer::tests::duplicate_signature_counts_once ... ok
test market_observer::tests::decoder_error_in_successful_transaction_blocks_valid ... ok
test market_observer::tests::events_before_observation_are_not_counted ... ok
test market_observer::tests::explicit_ws_url_overrides_helius_derived_url ... ok
test market_observer::tests::cutoff_counts_queued_events_received_before_and_rejects_events_after ... ok
test market_observer::tests::gap_before_stream_ready_is_recorded_but_not_counted ... ok
test market_observer::tests::failed_transaction_is_recorded_but_not_a_liquidation ... ok
test market_observer::tests::enrichment_budget_identity_and_queue_limits_are_explicit ... ok
test market_observer::tests::ingress_overflow_makes_data_incomplete ... ok
test market_observer::tests::failed_transaction_does_not_create_liquidation ... ok
test market_observer::tests::market_config_needs_no_keypair_and_validates_source_and_commitment ... ok
test market_observer::tests::quantiles_are_nearest_rank ... ok
test market_observer::tests::market_ws_config_enables_heartbeat_and_shadow_default_does_not ... ok
test market_observer::tests::short_non_finite_or_undecodable_marginfi_data_is_a_decoder_error ... ok
test market_observer::tests::slot_telemetry_frames_measure_stream_silence ... ok
test market_observer::tests::transactions_with_helius_key_uses_enhanced_mainnet_endpoint ... ok
test market_observer::tests::truncated_logs_are_flagged ... ok
test market_observer::tests::non_truncated_existing_behavior_unchanged ... ok
test market_observer::tests::truncated_fetch_failure_or_wrong_transaction_fail_closed ... ok
test market_observer::tests::truncated_logs_with_no_end_liquidation_can_be_qualified ... ok
test market_observer::tests::truncated_logs_with_exact_instruction_event_parity_can_be_qualified ... ok
test market_observer::tests::truncated_logs_missing_liquidation_event_fail_closed ... ok
test market_observer::tests::truncated_missing_loaded_addresses_fail_closed ... ok
test market_observer::tests::two_events_in_one_transaction_keep_distinct_indices ... ok
test market_observer::tests::truncated_v0_loaded_addresses_are_resolved ... ok
test econ::tests::bounded_queue_and_retention_overflow_fail_qualification ... ok
test market_observer::tests::logs_capture_never_fetches_and_enrichment_runs_after_capture ... ok
test market_observer::tests::truncated_logs_wrong_target_or_receiver_fail_closed ... ok
test market_observer::tests::market_mode_has_no_signing_simulation_or_submission_path ... ok
test market_observer::tests::six_real_audit_liquidations_decode_enrich_and_verify_identity ... ok
test market_observer::tests::utc_formatting ... ok
test econ::tests::winner_without_live_observation_is_visible_and_not_race_qualified ... ok
test market_observer::tests::wrong_discriminator_is_not_a_liquidation_event ... ok
test market_observer::tests::unverified_sources_never_yield_market_data_valid ... ok
test market_observer::tests::v0_loaded_addresses_resolve_receiver_and_lookups_without_loaded_are_unknown ... ok
test pending::tests::landed_success_and_revert_use_meta_fee ... ok
test pending::tests::absence_and_missing_metadata_never_invent_fee ... ok
test pipeline::mt0_tests::local_fee_estimate_reproduces_fix129_observations ... ok
test pending::tests::tip_requires_our_matching_landed_transfer_and_partial_bundle_is_unknown ... ok
test remote_workers::tests::winner_watch_keeps_retry_deadline ... ok
test record::tests::health_json_preserves_full_protocol_fixed_point_range ... ok
test route::tests::p1c_parses_quote_and_executable_floor_separately ... ok
test market_observer::tests::verdicts_by_source_and_without_liquidations ... ok
test route::tests::parses_instruction_and_alt_without_rpc ... ok
test pending::tests::recorded_mainnet_success_and_revert_fees_are_preserved ... ok
test shadow_ledger::tests::aggregates_and_rolls ... ok
test winner::tests::f8_probe_sdk_encoded_transaction_decoding ... ignored
test winner::tests::f8_same_slot_hits_are_ambiguous_never_guessed ... ok
test winner::tests::f8_failed_liquidation_is_not_a_winner ... ok
test winner::tests::f8_legacy_successful_winner ... ok
test winner::tests::f8_single_earliest_hit_is_unique_winner ... ok
test route::tests::strips_only_exact_verified_prewarmed_ata_setup ... ok
test winner::tests::f8_transaction_identity_is_fail_closed ... ok
test winner::tests::outcomes ... ok
test winner::tests::f8_winner_serializes_ambiguity_explicitly ... ok
test winner::tests::f8_v0_winner_without_relevant_lut_address ... ok
test winner::tests::f8_v0_winner_with_receiver_from_lut ... ok
test winner::tests::f8_fail_closed_shapes ... ok
test winner::tests::f8_unrelated_successful_tx_is_not_a_winner ... ok
test market_observer::tests::six_real_audit_liquidations_aggregate_into_market_summary ... ok
test winner::tests::f8_shared_instruction_truth_matches_six_audit_winners ... ok
test winner_tracker::tests::capacity_overflow_is_counted_not_silent ... ok
test market_observer::tests::enrichment_init_failure_still_produces_summary_and_report ... ok
test market_observer::tests::finalize_writes_summary_report_and_unenriched_rows ... ok
test winner::tests::f8_six_real_audit_winners_resolve_6_of_6 ... ok
test winner_tracker::tests::config_validation ... ok
test winner_tracker::tests::due_after_initial_delay_and_never_dispatched_twice ... ok
test winner_tracker::tests::full_worker_queue_stops_round_and_backs_off ... ok
test winner_tracker::tests::not_liquidatable_skip_creates_no_watch ... ok
test winner_tracker::tests::ok_none_and_err_reschedule_then_expire ... ok
test winner_tracker::tests::older_inflight_completion_cannot_resolve_or_remove_newer_revision ... ok
test winner_tracker::tests::one_watch_per_account_and_dedupe ... ok
test winner_tracker::tests::partial_dispatch_until_first_full ... ok
test econ::tests::atomic_writer_only_exposes_whole_json ... ok
test winner_tracker::tests::resolved_uses_coalesced_earliest_readiness ... ok
test winner_tracker::tests::result_for_untracked_account_is_ignored ... ok
test winner_tracker::tests::send_error_propagates_and_leaves_nothing_in_flight ... ok
test winner_tracker::tests::stale_none_or_error_never_reschedules_the_older_revision ... ok
test winner_tracker::tests::undispatched_watch_expires_without_worker ... ok
test market_observer::tests::logs_without_rpc_url_still_reaches_finalize_and_report ... ok
test econ::tests::four_boundaries_delta_cumulative_and_final_reconcile ... ok
test econ::tests::runtime_worker_writes_four_snapshots_then_identical_final ... ok

test result: ok. 113 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 4.37s

     Running unittests src/main.rs (target/debug/deps/p0_longtail-a855168c28090410)

running 8 tests
test integration_probe_tests::candidate_cache_forwards_transition_force_revision_and_bounded_periodic_recheck ... ok
test integration_probe_tests::candidate_singleflight_coalesces_latest_work_without_duplicate_queue_token ... ok
test integration_probe_tests::only_one_target_is_dispatched_per_idle_turn ... ok
test integration_probe_tests::oracle_detect_returns_latest_coalesced_work_once ... ok
test integration_probe_tests::target_waits_unsigned_until_worker_is_idle ... ok
test integration_probe_tests::oracle_detect_skips_stale_order_entries ... ok
test integration_probe_tests::candidate_fairness_services_all_above_soft_cap_without_starvation ... ok
test integration_probe_tests::candidate_fairness_repeat_admission_remains_bounded ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

     Running unittests src/bin/p0-prewarm-atas.rs (target/debug/deps/p0_prewarm_atas-cec59557071e3864)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/p0-probe-costs.rs (target/debug/deps/p0_probe_costs-cb3962abb7691ef8)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (target/debug/deps/darkbot_solana-d4730ac71a081c1e)

running 35 tests
test ingress::tests::config_wins_over_version_field ... ok
test ingress::tests::version_field_and_legacy ... ok
test ingress::ws_rpc::tests::account_notification_needs_known_subscription ... ok
test ingress::tests::bytes_prefix ... ok
test ingress::ws_rpc::tests::c0mo_transaction_subscription_requests_successful_and_failed ... ok
test ingress::ws_rpc::tests::parses_slot_and_root ... ok
test ingress::ws_rpc::tests::shadow_config_keeps_heartbeat_disabled_and_wait_unchanged ... ok
test ingress::ws_rpc::tests::transport_ping_pong_never_counts_as_stream_activity ... ok
test state::blockhash::tests::staleness ... ok
test ingress::ws_rpc::tests::c0mo_logs_and_transaction_notifications_keep_raw_payload ... ok
test ingress::ws_rpc::tests::c0mo_market_subscriptions_are_opt_in ... ok
test ingress::ws_rpc::tests::parses_program_notification ... ok
test ingress::ws_rpc::tests::subscribe_requests_cover_all_kinds ... ok
test state::fork::tests::root_backwards_is_ignored ... ok
test state::fork::tests::detects_orphaned_branch ... ok
test state::recovery::tests::dirty_reconcile_stays_halted ... ok
test state::recovery::tests::gap_to_ready_full_loop ... ok
test state::recovery::tests::missing_required_dependency_blocks_ready ... ok
test state::recovery::tests::snapshot_b_missing_required_blocks_ready ... ok
test state::tests::bootstrap_buffers_then_applies_only_newer_than_snapshot ... ok
test state::tests::gap_halts_and_reconcile_recovers ... ok
test state::tests::ordering_rules ... ok
test state::tests::reconcile_mismatch_halts ... ok
test state::tests::reconcile_mixed_slot_batches_detects_stale_local ... ok
test state::tests::reconcile_owner_mismatch_halts ... ok
test state::tests::root_policy_rejects_below_root ... ok
test state::tests::slot_events_track_highest ... ok
test submit::tests::path_names_match_ledger ... ok
test tx::tests::locked_accounts_are_deduplicated ... ok
test tx::tests::priority_fee_uses_requested_limit_not_simulated_consumption ... ok
test tx::tests::v1_effective_list_has_no_compute_budget_instructions ... ok
test tx::tests::v1_is_unavailable_without_feature ... ok
test tx::tests::v1_priority_fee_is_total_lamports ... ok
test tx::tests::v1_priority_fee_rounds_up_to_total_lamports ... ok
test tx::tests::too_many_accounts_is_rejected_before_compile ... ok

test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests darkbot_common

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests darkbot_project0

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests darkbot_runner

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests darkbot_solana

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s


```

### tests_txv1_final

```text
   Compiling darkbot-runner v0.1.0 (/Users/sawyer_ma/Documents/ChatGPT/C0E1_57bef33_implementation/DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33/repo/project_0/native/crates/darkbot-runner)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.15s
     Running unittests src/lib.rs (target/debug/deps/darkbot_common-84f89d61913e24bf)

running 26 tests
test config::tests::missing_is_error_not_default ... ok
test channel::tests::disconnect_is_reported ... ok
test config::tests::parsed_or_uses_explicit_default_and_rejects_garbage ... ok
test channel::tests::overflow_is_explicit_and_returns_element ... ok
test histogram::tests::empty_is_none_not_zero ... ok
test histogram::tests::index_and_upper_are_consistent ... ok
test histogram::tests::merge_keeps_extremes ... ok
test error::tests::display_carries_reason_and_detail ... ok
test journal::tests::append_and_replay_in_order ... ok
test histogram::tests::percentiles_are_conservative_and_bounded ... ok
test journal::tests::gap_is_rejected ... ok
test lifecycle::tests::bootstrapping_is_not_tradable ... ok
test lifecycle::tests::halted_keeps_first_reason_and_needs_explicit_recovery ... ok
test lifecycle::tests::shutdown_flag ... ok
test mt0::tests::error_classification_separates_429_timeout_and_other ... ok
test mt0::tests::histogram_is_deterministic_bounded_and_conservative ... ok
test mt0::tests::long_running_section_is_visible_before_it_ends ... ok
test mt0::tests::remote_classes_and_outcomes_are_separate ... ok
test timing::tests::clock_is_monotonic ... ok
test timing::tests::stage_names_are_ledger_vocabulary ... ok
test timing::tests::timeline_intervals_and_no_overwrite ... ok
test timing::tests::timeline_rejects_negative_interval ... ok
test mt0::tests::owner_sections_sum_to_elapsed_time ... ok
test mt0::tests::trigger_mapping_and_health_counters_count_actual_calls ... ok
test mt0::tests::unmeasured_metrics_are_marked_not_zero_and_snapshot_is_bounded ... ok
test channel::tests::pop_timeout_returns_none_on_empty ... ok

test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running unittests src/lib.rs (target/debug/deps/darkbot_project0-83cd6f816b9b98ca)

running 60 tests
test economics::tests::funding_class_is_not_a_hardcoded_preference ... ok
test economics::tests::premium_curve_matches_release_notes ... ok
test economics::tests::exact_rpc_fee_qualifies_only_rpc_scenarios ... ok
test economics::tests::equal_net_prefers_capital_efficiency ... ok
test economics::tests::t_profitable_solver ... ok
test economics::tests::time_for_premium_inverts_curve ... ok
test economics::tests::unknown_cost_is_not_zero ... ok
test economics::tests::seize_bound_from_source ... ok
test economics::tests::unknown_realizable_value_is_unknown ... ok
test economics::tests::ranking_by_net_then_capital_efficiency ... ok
test execution::tests::freshness_route_fee ... ok
test execution::tests::backoff_grows_and_drops ... ok
test execution::tests::gates ... ok
test execution::tests::quarantine_expires ... ok
test funding::tests::emits_all_feasible_candidates_instead_of_picking_one ... ok
test funding::tests::inventory_classes_do_not_require_market_route ... ok
test funding::tests::jit_is_only_shortfall_fallback ... ok
test funding::tests::no_route_no_inventory_means_no_candidate ... ok
test health::threshold_search_tests::first_true_boundary_expands_above_current ... ok
test health::threshold_search_tests::first_true_boundary_finds_exact_integer_crossing ... ok
test health::threshold_search_tests::first_true_boundary_propagates_fail_closed_error ... ok
test index::tests::reverse_index_fanout_and_replacement ... ok
test index::tests::threshold_crossings_only_wake_newly_endangered_accounts ... ok
test planner::tests::bundle_rules ... ok
test index::tests::threshold_lookup_both_sides ... ok
test integration::tests::pinned_refresh_discriminators_match_expected ... ok
test planner::tests::end_must_be_last_without_exception ... ok
test planner::tests::no_swap_plan_rejects_swap_and_requires_repay ... ok
test planner::tests::non_allowlisted_swap_and_paid_route ... ok
test planner::tests::valid_swap_plan ... ok
test prepared::tests::exact_key_or_rebuild ... ok
test projection::tests::delayed_evaluation_uses_exact_simulation_clock_for_both_venues ... ok
test projection::tests::empty_and_zero_slot_projection_fail_closed ... ok
test projection::tests::negative_simulation_timestamp_fails_closed ... ok
test projection::tests::non_projected_account_falls_through_to_base ... ok
test projection::tests::projected_account_overrides_base_without_mutating_it ... ok
test projection::tests::writable_refresh_accounts_are_canonical_projection_targets ... ok
test protocol::tests::classify_and_layouts ... ok
test receivership::builders::repay_all_tests::partial_repay_does_not_append_health_observation ... ok
test receivership::builders::repay_all_tests::repay_all_carries_observation_accounts ... ok
test receivership::builders::repay_all_tests::classic_withdraw_does_not_append_mint ... ok
test receivership::builders::repay_all_tests::token_2022_repay_includes_mint_before_repay_all_observation ... ok
test safety::tests::limits_and_halts ... ok
test protocol::tests::pdas_are_deterministic ... ok
test safety::tests::live_disabled_and_day_roll ... ok
test safety::tests::preview_does_not_reserve_capital ... ok
test receivership::builders::repay_all_tests::token_2022_withdraw_includes_mint_before_observation ... ok
test schedule::tests::premium_boundary_math ... ok
test schedule::tests::scheduler_orders_and_invalidates ... ok
test sizing::tests::closeout_threshold_matches_protocol_units ... ok
test sizing::tests::drift_sizing_roundtrip_uses_scaled_balance_decimals ... ok
test sizing::tests::headroom_is_below_boundary ... ok
test sizing::tests::integer_conversions_are_conservative ... ok
test sizing::tests::near_full_rounding_uses_source_backed_repay_all ... ok
test sizing::tests::normal_and_closeout_bounds_are_distinct ... ok
test sizing::tests::ordinary_partial_repay_semantics_are_unchanged ... ok
test sizing::tests::r121_pair_heals_below_maximum_premium ... ok
test sizing::tests::repay_all_matches_protocol_opposite_dust_boundary ... ok
test sizing::tests::repay_all_never_clamps_a_target_above_full_transfer ... ok
test sizing::tests::rounded_health_keeps_normal_boundary_and_closeout_exception ... ok

test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (target/debug/deps/darkbot_runner-6830e0bba0ad474c)

running 114 tests
test builder::p0::fix100_tests::p1c_quote_profitable_floor_unprofitable_is_not_actionable ... ok
test builder::p0::fix100_tests::p1c_zero_prefund_feasibility_uses_floor_not_quote ... ok
test builder::p0::fix100_tests::liquidation_record_link_controls_init ... ok
test builder::p0::fix100_tests::missing_floor_production_path_is_unknown_and_never_actionable ... ok
test builder::p0::fix100_tests::non_budget_errors_are_not_retried ... ok
test builder::p0::fix100_tests::repay_all_removes_closed_bank_only_from_end_layout ... ok
test econ::tests::all_loss_reasons_are_retained_exactly ... ok
test econ::tests::absent_sign_winner_market_and_ambiguous_identity_fail_closed ... ok
test econ::tests::config_requires_four_snapshots_and_live_disabled ... ok
test econ::tests::economics_input_and_profit_capital_preserved ... ok
test econ::tests::every_default_capital_boundary_includes_reserve ... ok
test econ::tests::identity_mismatch_never_yields_timing ... ok
test builder::p0::fix100_tests::route_budget_counts_payer_and_resource_accounts_too ... ok
test builder::p0::fix100_tests::route_requotes_until_actual_serialized_transaction_fits ... ok
test builder::p0::fix100_tests::impossible_route_exhausts_bounded_budget_and_fails_closed ... ok
test econ::tests::shared_clock_preserves_origin_across_producers ... ok
test econ::tests::unique_winner_slot_classes_and_signed_margins ... ok
test econ::tests::unknown_capital_does_not_remove_market_match ... ok
test econ::tests::small_sample_percentiles_and_missing_intervals ... ok
test econ::tests::unknown_costs_and_economics_remain_null ... ok
test econ::tests::actual_shadow_pipeline_builds_signs_simulates_and_never_submits ... ok
test econ::tests::counters_use_event_receive_hour_even_if_processed_after_boundary ... ok
test econ::tests::early_end_missing_snapshot_upgrade_and_writer_failures_are_incomplete ... ok
test econ::tests::zero_prefund_does_not_erase_fees_and_unknown_budget_is_not_fundable ... ok
test fixture::run::tests::all_close_fixture_omits_both_closed_banks_from_end ... ok
test lookup_tables::tests::extension_slot_only_exposes_old_prefix ... ok
test econ::tests::strong_join_unmatched_diagnostics_and_speedup_math ... ok
test econ::tests::simulation_bottleneck_and_unknown_edge_not_fabricated ... ok
test lookup_tables::tests::invalid_and_deactivating_tables_are_excluded ... ok
test lookup_tables::tests::tables_reduce_compiled_size_and_merge_preserves_canonical_indices ... ok
test market_observer::tests::data_from_another_program_is_never_counted ... ok
test market_observer::tests::decodes_valid_receiver_event ... ok
test market_observer::tests::duplicate_signature_counts_once ... ok
test market_observer::tests::events_before_observation_are_not_counted ... ok
test market_observer::tests::cutoff_counts_queued_events_received_before_and_rejects_events_after ... ok
test market_observer::tests::explicit_ws_url_overrides_helius_derived_url ... ok
test market_observer::tests::decoder_error_in_successful_transaction_blocks_valid ... ok
test market_observer::tests::gap_before_stream_ready_is_recorded_but_not_counted ... ok
test market_observer::tests::enrichment_budget_identity_and_queue_limits_are_explicit ... ok
test market_observer::tests::failed_transaction_is_recorded_but_not_a_liquidation ... ok
test market_observer::tests::failed_transaction_does_not_create_liquidation ... ok
test market_observer::tests::ingress_overflow_makes_data_incomplete ... ok
test market_observer::tests::market_config_needs_no_keypair_and_validates_source_and_commitment ... ok
test market_observer::tests::market_ws_config_enables_heartbeat_and_shadow_default_does_not ... ok
test econ::tests::bounded_queue_and_retention_overflow_fail_qualification ... ok
test market_observer::tests::quantiles_are_nearest_rank ... ok
test market_observer::tests::short_non_finite_or_undecodable_marginfi_data_is_a_decoder_error ... ok
test market_observer::tests::slot_telemetry_frames_measure_stream_silence ... ok
test market_observer::tests::transactions_with_helius_key_uses_enhanced_mainnet_endpoint ... ok
test market_observer::tests::truncated_logs_are_flagged ... ok
test market_observer::tests::non_truncated_existing_behavior_unchanged ... ok
test market_observer::tests::truncated_fetch_failure_or_wrong_transaction_fail_closed ... ok
test market_observer::tests::truncated_logs_with_no_end_liquidation_can_be_qualified ... ok
test market_observer::tests::truncated_logs_missing_liquidation_event_fail_closed ... ok
test market_observer::tests::truncated_logs_with_exact_instruction_event_parity_can_be_qualified ... ok
test market_observer::tests::two_events_in_one_transaction_keep_distinct_indices ... ok
test market_observer::tests::unverified_sources_never_yield_market_data_valid ... ok
test market_observer::tests::utc_formatting ... ok
test market_observer::tests::truncated_v0_loaded_addresses_are_resolved ... ok
test market_observer::tests::truncated_missing_loaded_addresses_fail_closed ... ok
test market_observer::tests::wrong_discriminator_is_not_a_liquidation_event ... ok
test market_observer::tests::v0_loaded_addresses_resolve_receiver_and_lookups_without_loaded_are_unknown ... ok
test pending::tests::absence_and_missing_metadata_never_invent_fee ... ok
test market_observer::tests::logs_capture_never_fetches_and_enrichment_runs_after_capture ... ok
test pending::tests::landed_success_and_revert_use_meta_fee ... ok
test pipeline::mt0_tests::local_fee_estimate_reproduces_fix129_observations ... ok
test market_observer::tests::truncated_logs_wrong_target_or_receiver_fail_closed ... ok
test remote_workers::tests::winner_watch_keeps_retry_deadline ... ok
test record::tests::health_json_preserves_full_protocol_fixed_point_range ... ok
test pending::tests::tip_requires_our_matching_landed_transfer_and_partial_bundle_is_unknown ... ok
test route::tests::p1c_parses_quote_and_executable_floor_separately ... ok
test market_observer::tests::verdicts_by_source_and_without_liquidations ... ok
test route::tests::parses_instruction_and_alt_without_rpc ... ok
test pending::tests::recorded_mainnet_success_and_revert_fees_are_preserved ... ok
test market_observer::tests::market_mode_has_no_signing_simulation_or_submission_path ... ok
test econ::tests::winner_without_live_observation_is_visible_and_not_race_qualified ... ok
test shadow_ledger::tests::aggregates_and_rolls ... ok
test winner::tests::f8_probe_sdk_encoded_transaction_decoding ... ignored
test winner::tests::f8_failed_liquidation_is_not_a_winner ... ok
test winner::tests::f8_same_slot_hits_are_ambiguous_never_guessed ... ok
test winner::tests::f8_legacy_successful_winner ... ok
test route::tests::strips_only_exact_verified_prewarmed_ata_setup ... ok
test winner::tests::f8_single_earliest_hit_is_unique_winner ... ok
test winner::tests::f8_transaction_identity_is_fail_closed ... ok
test winner::tests::f8_winner_serializes_ambiguity_explicitly ... ok
test winner::tests::outcomes ... ok
test winner_tracker::tests::config_validation ... ok
test winner::tests::f8_v0_winner_without_relevant_lut_address ... ok
test winner_tracker::tests::capacity_overflow_is_counted_not_silent ... ok
test winner::tests::f8_fail_closed_shapes ... ok
test winner_tracker::tests::due_after_initial_delay_and_never_dispatched_twice ... ok
test winner::tests::f8_v0_winner_with_receiver_from_lut ... ok
test winner_tracker::tests::not_liquidatable_skip_creates_no_watch ... ok
test winner_tracker::tests::full_worker_queue_stops_round_and_backs_off ... ok
test winner::tests::f8_unrelated_successful_tx_is_not_a_winner ... ok
test winner_tracker::tests::ok_none_and_err_reschedule_then_expire ... ok
test winner_tracker::tests::older_inflight_completion_cannot_resolve_or_remove_newer_revision ... ok
test winner_tracker::tests::one_watch_per_account_and_dedupe ... ok
test winner_tracker::tests::partial_dispatch_until_first_full ... ok
test winner_tracker::tests::resolved_uses_coalesced_earliest_readiness ... ok
test winner_tracker::tests::result_for_untracked_account_is_ignored ... ok
test winner_tracker::tests::send_error_propagates_and_leaves_nothing_in_flight ... ok
test winner_tracker::tests::stale_none_or_error_never_reschedules_the_older_revision ... ok
test winner_tracker::tests::undispatched_watch_expires_without_worker ... ok
test market_observer::tests::six_real_audit_liquidations_decode_enrich_and_verify_identity ... ok
test winner::tests::f8_shared_instruction_truth_matches_six_audit_winners ... ok
test winner::tests::f8_six_real_audit_winners_resolve_6_of_6 ... ok
test market_observer::tests::enrichment_init_failure_still_produces_summary_and_report ... ok
test market_observer::tests::six_real_audit_liquidations_aggregate_into_market_summary ... ok
test market_observer::tests::finalize_writes_summary_report_and_unenriched_rows ... ok
test market_observer::tests::logs_without_rpc_url_still_reaches_finalize_and_report ... ok
test econ::tests::atomic_writer_only_exposes_whole_json ... ok
test econ::tests::four_boundaries_delta_cumulative_and_final_reconcile ... ok
test econ::tests::runtime_worker_writes_four_snapshots_then_identical_final ... ok

test result: ok. 113 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 4.39s

     Running unittests src/main.rs (target/debug/deps/p0_longtail-27149dfa1f50697b)

running 8 tests
test integration_probe_tests::candidate_cache_forwards_transition_force_revision_and_bounded_periodic_recheck ... ok
test integration_probe_tests::only_one_target_is_dispatched_per_idle_turn ... ok
test integration_probe_tests::candidate_singleflight_coalesces_latest_work_without_duplicate_queue_token ... ok
test integration_probe_tests::oracle_detect_skips_stale_order_entries ... ok
test integration_probe_tests::oracle_detect_returns_latest_coalesced_work_once ... ok
test integration_probe_tests::target_waits_unsigned_until_worker_is_idle ... ok
test integration_probe_tests::candidate_fairness_services_all_above_soft_cap_without_starvation ... ok
test integration_probe_tests::candidate_fairness_repeat_admission_remains_bounded ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

     Running unittests src/bin/p0-prewarm-atas.rs (target/debug/deps/p0_prewarm_atas-854cbe7fc7bbe1ff)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/p0-probe-costs.rs (target/debug/deps/p0_probe_costs-91e8b1f0725e7886)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (target/debug/deps/darkbot_solana-2e51925ab56bbd68)

running 37 tests
test ingress::tests::bytes_prefix ... ok
test ingress::tests::config_wins_over_version_field ... ok
test ingress::tests::version_field_and_legacy ... ok
test ingress::ws_rpc::tests::parses_slot_and_root ... ok
test ingress::ws_rpc::tests::account_notification_needs_known_subscription ... ok
test ingress::ws_rpc::tests::c0mo_logs_and_transaction_notifications_keep_raw_payload ... ok
test ingress::ws_rpc::tests::subscribe_requests_cover_all_kinds ... ok
test ingress::ws_rpc::tests::c0mo_market_subscriptions_are_opt_in ... ok
test ingress::ws_rpc::tests::c0mo_transaction_subscription_requests_successful_and_failed ... ok
test state::blockhash::tests::staleness ... ok
test ingress::ws_rpc::tests::transport_ping_pong_never_counts_as_stream_activity ... ok
test state::fork::tests::detects_orphaned_branch ... ok
test ingress::ws_rpc::tests::shadow_config_keeps_heartbeat_disabled_and_wait_unchanged ... ok
test state::fork::tests::root_backwards_is_ignored ... ok
test ingress::ws_rpc::tests::parses_program_notification ... ok
test state::recovery::tests::dirty_reconcile_stays_halted ... ok
test state::recovery::tests::gap_to_ready_full_loop ... ok
test state::recovery::tests::missing_required_dependency_blocks_ready ... ok
test state::recovery::tests::snapshot_b_missing_required_blocks_ready ... ok
test state::tests::bootstrap_buffers_then_applies_only_newer_than_snapshot ... ok
test state::tests::gap_halts_and_reconcile_recovers ... ok
test state::tests::ordering_rules ... ok
test state::tests::reconcile_mismatch_halts ... ok
test state::tests::reconcile_mixed_slot_batches_detects_stale_local ... ok
test state::tests::reconcile_owner_mismatch_halts ... ok
test state::tests::root_policy_rejects_below_root ... ok
test state::tests::slot_events_track_highest ... ok
test submit::tests::path_names_match_ledger ... ok
test tx::tests::locked_accounts_are_deduplicated ... ok
test tx::tests::priority_fee_uses_requested_limit_not_simulated_consumption ... ok
test tx::tests::v1_effective_list_has_no_compute_budget_instructions ... ok
test tx::tests::v1_is_unavailable_without_feature ... ok
test tx::tests::v1_priority_fee_is_total_lamports ... ok
test tx::tests::v1_priority_fee_rounds_up_to_total_lamports ... ok
test tx::tests::v1_measure_uses_canonical_message_size ... ok
test tx::tests::too_many_accounts_is_rejected_before_compile ... ok
test tx::tests::v1_wire_signature_and_exact_size_boundary ... ok

test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/bin/p0-probe-v1.rs (target/debug/deps/p0_probe_v1-6be8da1a64bf1866)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests darkbot_common

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests darkbot_project0

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests darkbot_runner

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests darkbot_solana

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s


```

### format_new_modules

```text

```

### script_syntax

```text

```

## Earlier validation commands (superseded by final gates)

Working directory: exported repo root unless shown otherwise.

```bash
cargo check --locked --manifest-path project_0/native/Cargo.toml --features crate_health
# Initial attempt: FAIL (MarketConfig field + lifetime); second attempt: PASS.
cargo test --locked --manifest-path project_0/native/Cargo.toml -p darkbot-runner --lib --features crate_health econ
# PASS: 21 tests before the final additional capital regression.
cargo test --locked --manifest-path project_0/native/Cargo.toml --workspace --features crate_health
cargo test --locked --manifest-path project_0/native/Cargo.toml --workspace --features 'crate_health txv1'
cargo build --locked --manifest-path project_0/native/Cargo.toml --features 'crate_health txv1'
# All PASS; final variants above rerun after the final edit.
rustfmt --edition 2021 --config skip_children=true project_0/native/crates/darkbot-runner/src/econ.rs project_0/native/crates/darkbot-runner/src/econ_tests.rs project_0/native/crates/darkbot-runner/src/record.rs
rustc --version
cargo --version
```

ZIP input was checked with Python hashlib.sha256; each of 275 entries in input SHA256SUMS.txt
was recomputed. No toolchain installation, network market measurement or dependency upgrade.

## Final retention qualification — 2026-10-08

The first real 600-second C0-E1 run completed but returned `INCOMPLETE`
because its analytics retention model overflowed:

`555626 - 262144 = 293482`

The implementation was changed so every raw Shadow evaluation is streamed
to disk while only bounded report-relevant semantic state remains in RAM.

The one-million-evaluation regression produced:

`received=1000000`
`streamed=1000000`
`coalesced=999984`
`semantic_max=16`
`overflow=0`
`evictions=0`
`raw_rows=1000000`

Twenty-six targeted C0-E1 tests passed.

Full qualification after the final fix:

- rustfmt PASS
- git diff check PASS
- build crate_health PASS
- build crate_health+txv1 PASS
- workspace tests crate_health PASS
- workspace tests crate_health+txv1 PASS
- Differential Clippy candidate-only 0/0

The second real Mainnet run,
`c0e1_retention_smoke_20261008T215650Z`, completed all 600 seconds:

`technical=PASS`
`RUNNER_EXIT=0`
`four_snapshots_present=true`
`market_capture_complete=true`
`clean_automatic_shutdown=true`
`analytics_queue_drops=0`
`retention_overflow=0`
`shadow_evaluations_received=840518`
`shadow_evaluations_streamed=840518`
`shadow_evaluations_coalesced=816112`
`shadow_semantic_entries_max=24406`
`shadow_semantic_evictions=0`
`submissions=0`
`live_ready=false`

No market liquidation occurred during this ten-minute window, therefore
the economic sample remains `INSUFFICIENT_SAMPLE`.

Market-source coverage remains
`MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED`.

The implementation is technically qualified for the four-hour C0-E1
measurement, but is not yet economically qualified or LIVE-ready.

## Authoritative qualified revision before 4h observation

Qualified C0-E1 implementation revision:

`b7f5ed3850e0b1e412d39c673d89a8c8ffc3644a`

Commit message:

`project0: qualify C0-E1 retention-safe economics harness`

This is the exact code source that passed:

- the 1,000,000-evaluation retention stress test;
- 26 targeted C0-E1 tests;
- both builds;
- both complete workspace-test configurations;
- rustfmt and diff checks;
- Differential Clippy candidate-only 0/0;
- the second real 600-second Mainnet smoke with technical PASS.

Successful Mainnet retest:

`c0e1_retention_smoke_20261008T215650Z`

Key retention/safety evidence:

- actual duration 600.0 s;
- four snapshots present;
- market capture complete;
- clean automatic shutdown;
- analytics queue drops 0;
- retention overflow 0;
- evaluations received 840,518;
- evaluations streamed 840,518;
- evaluations coalesced 816,112;
- semantic entries max 24,406;
- semantic evictions 0;
- submissions 0;
- live_ready false.

Next execution step:

Run the 14,400-second C0-E1 observation from this exact code revision. No production-path optimization should be inserted between this qualified checkpoint and that measurement unless qualification is repeated.

The implementation remains observe-only and is not LIVE-ready.
# 2026-10-09 master-contract checkpoint — NOT FULLY IMPLEMENTED

The historical implementation report below does not qualify the expanded master contract.
Current normative resume: top of MEASUREMENT_LEDGER.md. Local HEAD is still db78cdb3...;
no commit/push, pin change, BSC edit, wallet change or submission was performed.

## Pinned-source findings (source evidence, not current deployment attestation)

Inspected Cargo's exact pinned checkout:
`~/.cargo/git/checkouts/marginfi-v2-8cb09da044851022/33c6798`, whose git HEAD is
`33c67987a661f59c6683a65485ebb4f0ef030b77`. `id-crate/src/lib.rs:5` declares
`MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` under mainnet-beta. Cargo.lock/pins unchanged.
The existing upgrade-halt evidence remains separate; no new bytecode equivalence or
deployment-version claim was established in this checkpoint.

| Path | Entry point in programs/marginfi/src/lib.rs | Event / source |
| --- | --- | --- |
| Classic | lending_account_liquidate, line 450 | LendingAccountLiquidateEvent, events.rs:210; instructions/marginfi_account/liquidate.rs:549 |
| Receivership | start_liquidation / end_liquidation, lines 739/747 | LiquidationReceiverEvent, events.rs:279; instructions/marginfi_account/liquidate_end.rs |
| Forced deleverage | start_deleverage / end_deleverage, lines 753/758 | DeleverageEvent, events.rs:288; same liquidation-end module |
| Bad-debt settlement | lending_pool_handle_bankruptcy, line 265 | LendingPoolBankHandleBankruptcyEvent, events.rs:113; instructions/marginfi_group/handle_bankruptcy.rs:224 |
| Sunset balance purge | purge_deleverage_balance, line 888 | Administrative removal of abandoned lending balances after debt settlement; audit classification still open |

The old observer counts only LiquidationReceiverEvent. Classic/Deleverage/Bankruptcy event
coverage is not implemented by the gap patch. No claim that this initial entry-point audit
is exhaustive. In particular, an administrative debt write-off must not be conflated with
collateral seized by a competing profitable bot.

Anchor discriminator derivations (first 8 bytes of SHA256 of exact namespace:name):

| Kind/name | Hex |
| --- | --- |
| global:lending_account_liquidate | d6a997d5fba756db |
| global:start_liquidation | f45d5ad6c0a6bf15 |
| global:end_liquidation | 6e0bf436e5b516b8 |
| global:start_deleverage | 0a8a0a3928e8b6c1 |
| global:end_deleverage | 720efa8ffc68d6d1 |
| global:lending_pool_handle_bankruptcy | a20b388b5a8046ad |
| event:LendingAccountLiquidateEvent | a6a0f99ab72717f2 |
| event:LiquidationReceiverEvent | 2883e0dc975318e6 |
| event:DeleverageEvent | a1086cccd1c60c1e |
| event:LendingPoolBankHandleBankruptcyEvent | a64d298c245e0a39 |

Classic fixed account order (`liquidate.rs:572`): group=0, asset_bank=1, liability_bank=2,
liquidator_marginfi_account=3, authority=4, liquidatee_marginfi_account=5; vault authority,
liquidity vault, insurance vault and token program follow. These indices must be resolved
through full static+loaded addresses, including inner-instruction inspection where applicable.

Classic event header contains optional signer, liquidator account, account authority and
group. Then liquidatee account/authority, asset bank/mint, liability bank/mint, pre/post health,
and two LiquidationBalances records of five f64 amounts each. It contains no direct pair of
USD seized/repaid values. Preserve f64 provenance; do not pretend these are exact integers.
Instruction asset_amount is separately exact, subject to actual instruction decoding.

Source economic caveat (`liquidate.rs:260–318,330–418`): collateral uses the low-biased asset
price, liability the realtime high-biased oracle price. The liquidator pays C_L and the target
receives smaller debt reduction R_L; C_L-R_L is the insurance component. The liquidator may
spend an existing liability-bank deposit before assuming debt. Thus observed liability cost
requires BOTH debt increase AND deposit reduction, not merely target debt reduction.
Historical USD gross edge requires the slot-correct price/decimal evidence. Insurance already
included in C_L must not be subtracted twice. Health differences do not supply the missing prices.

Receivership derives seized/repaid from pre/post Equity health with cached liquidation prices
in liquidation-end processing. Its logged values already provide protocol equity amounts;
gross edge is seized minus repaid. The separate lamps_fee_paid is a SOL-denominated protocol
cost; historical SOL/USD and further winner costs are not inferred from today's prices.
Network meta.fee includes priority; do not subtract priority again. Unobservable external
payments/funding/tips must keep winner-net completeness false until evidenced.

## Implemented gap foundation

`market_gap.rs` adds bounded gap state, persisted GAP_BOUNDARY evidence, post-drain inclusive
getBlock recovery, F8 full-key relevance and instruction/event parity, dedupe and sanitized
error codes. Skipped proof uses a finalized successor parent link; getBlocks is only used
to locate the successor. References: https://solana.com/docs/rpc/http/getblock and
https://solana.com/docs/rpc/http/getblocks. One block at a time; RPC body limit 64 MiB;
default limits 32 gaps, 512 slots/gap, 3 attempts, 10s RPC timeout, 250ms backoff,
180s phase deadline, 2,000,000 scanned TX. Every limit fails closed.

`MarketEvent.t_rx_ns` is now optional, with Live/GapBackfill provenance. Recovered chain slot
classes remain usable with authoritative winner identity, but local receive margins and
speedup stay null. Existing gap-only accounting assigns backfill to final resolution hour 4;
the master engine still needs canonical chain-window/hour attribution. This temporary
accounting is explicitly not historical live receipt time.

`run_gap_reconnect_smoke.sh` and qualification-only `p0-gap-ws-proxy.rs` close one market WSS
session after a real slot notification. The proxy rejects non-market RPC methods and binds
only loopback; Shadow ingress remains separate. The first smoke failed upstream reconnect.
The follow-up upstream timeout change is unqualified on Mainnet. No new four-hour run.

## Offline evidence / current limits

Old original run remains untouched. Checksums read during inspection:

```text
b6baade3228eee3c038c91f0e751304540465f87edcf4030bb6c1583b8cb739b market/coverage.jsonl
9121c55c54c9c4a6ca7122f056579f75ffe8da73f5d84394388906761ca73a27 market/summary.json
4a9cd8bbe5e396c09737fda3eb8c8fade95164bf1b207213b78583e6fea3915f economics_final.json
```

Inspected coverage, summary, final economics, runtime and schemas of raw Shadow/market/MT0
streams. No canonical market first/last slot frame or first post-reconnect slot was persisted.
Transaction min/max and Shadow slot samples are not substitutes. OLD_4H_REQUALIFIED=false;
no whole-market count, Classic split or whole-market gross/net edge can be claimed.

Still required: full-slot engine + slot ledger; exhaustive source classification; Classic and
additional event decoders; historical economics with explicit unknown counts; typed per-event
Market↔Shadow joins and loss attribution; ALL/type/hour aggregates; real Classic fixtures;
expanded mandatory tests; controlled full-slot smoke. Current top-level Market completeness
is deliberately blocked until these exist. No strategy/performance optimization was made.


## RESUME — 2026-10-09 canonical scanner and typed decoder implementation

**MASTER PARTIAL / NOT QUALIFIED. No new 4h run.** This block supersedes the
older gap-only checkpoint immediately below. Starting Project_0 base remains
`db78cdb3d3507da66fe93450742a8235fefcde1a`; current repository HEAD observed on
resume is `ec853c4d38492869388440b5b034c0d8a6ee2677` (BSC-only documentation commit).
No pull/merge/reset/clean, staging, commit or push was performed here. BSC untouched.

Implemented in this continuation:
- `market_reconcile.rs`: inclusive canonical slot window with persisted slotSubscribe
  boundary provenance; bounded finalized getSlot/getBlocks/getBlock pass over the whole
  window, every transaction scanned, static/loaded keys and inner instruction indices
  checked. Parent slot AND previousBlockhash continuity proves omitted slots; trailing
  skips require a fetched finalized successor. No absent/null block treated as skipped.
- Captured slot boundaries are the first and highest in-window processed slot frames,
  subsequently reconciled at finalized. This is an explicit whole-slot window, NOT an
  exact transaction-level wall-clock boundary. blockTime span and monotonic duration are
  different quantities. The old run cannot obtain these missing boundaries retroactively.
- Legacy, v0 and JSON v1 account/instruction views are supported; real six-fixture F8
  coverage includes v0/v1. Unsupported versions, missing inner instruction recording,
  incomplete LUT views, malformed data, timeout and budgets fail closed.
- `market_truth.rs`: pinned-source Classic / existing F8 Receivership / ForcedDeleverage /
  BankruptcySettlement classification; instruction-event identity parity, CPI inspection,
  distinct multiple-event identities, raw relevant TX stored before classification.
  Deleverage and bankruptcy are explicitly not permissionless bot-profit opportunities.
- Exact Classic seized amount comes from instruction u64. Event f64 native balances retain
  their provenance. Liability cost includes assumed debt PLUS consumed liability-bank
  deposits; insurance is already included. Slot-price/decimal evidence absent => USD edge
  UNKNOWN. Receivership/deleverage use protocol cached-equity seized minus repaid.
  meta.fee is transaction context (includes priority), never summed per event. Explicit
  legacy/v0 requested CU-limit/price gives priority with ceil; v1 config priority remains
  UNKNOWN pending authoritative fee-semantic qualification. Winner net remains UNKNOWN.
- Live receipts are retained separately; reconciliation-only events have no receive clock.
  Canonical typed ALL/per-type aggregates are exposed separately from legacy receiver
  analytics. Final econ reports explicitly label legacy scope; generalized join and hourly
  canonical attribution remain false/unqualified.
- Fixed existing gap dedupe: live-seen signatures still undergo instruction/event parity
  validation before skipping duplicate aggregation.

Mainnet diagnostic `project_0/data/market/canonical_probe_20261009T132213Z`:
30-second capture; canonical slots 454878435..454878545; 111 expected/produced/fetched,
0 failed, 0 unresolved, 118502 TX scanned, 3 Marginfi TX, 0 decoded liquidations,
0 transport gaps. **Full-slot pass succeeded**, overall MARKET_DATA_INCOMPLETE as designed.
This was a read-only market run, not combined C0-E1 and not a controlled reconnect.
It cannot qualify all liquidation paths without events. It also precedes final CPI-order,
truncation-guard and report-label refinements; repeat on final source before qualification.
Evidence, hashes and local build/test logs: `docs/evidence/C0E1_CANONICAL_CHECKPOINT_20261009/`.

Exact remaining work (real implementation, not only tests):
1. Generalize Market↔Shadow identity/instrumentation to type + bank pair + target + slot;
   existing OpportunityRecord lacks bank-pair/type evidence. Preserve original production
   economics/capital/race pipeline, extend the evidence rather than infer missing fields.
2. Complete canonical hourly aggregation, capital/loss-dollar attribution, network fee
   dedupe/fee payer and observed winner costs, historical price provenance and economic
   completeness. No guessed historical prices, no artificial zero unknown costs.
3. Finish exhaustive pinned-source/deployment upgrade qualification; sunset balance purge
   is administrative cleanup after debt settlement (lib.rs:880-895), not collateral profit.
   Drift bad-debt claim likewise needs explicit scope documentation before exhaustive claim.
4. Add real Classic/Deleverage/Bankruptcy fixtures, mixed-path same-TX and CPI-order/rollback
   fixtures; qualify all path coverage. Receivership has six real existing fixtures.
5. Integrate full-window recovery with transport-gap bookkeeping, then repeat controlled
   reconnect/full-slot combined Mainnet smoke on the final candidate. Existing separate
   gap recovery remains; an unresolved transport gap cannot yet pass the master gate.
6. Only after full master acceptance consider the next 4h run. Original old 4h remains
   OLD_4H_REQUALIFIED=false because canonical boundaries were never recorded.

LIVE/submission disabled. No source pin change. Current full workspace results and
Clippy differential are stored in the checkpoint results files; no master PASS implied.


## 2026-10-10 canonical economics and pinned-path completion (qualification in progress)

The canonical finalized ledger now drives `canonical.window_delta` and
`canonical.cumulative` in hour_01..04.json and economics_final.json. Earlier top-level
receiver analytics remain explicitly marked legacy for compatibility; they are not
whole-market statistics. `canonical_market_shadow.jsonl` includes each signature,
transaction/event index, type/target/banks, pipeline evidence and six capital outcomes.
All journal rows for each relevant target are replayed with byte-offset and backward
link validation; the earliest executable exact plan is retained. Two identical
same-slot event identities are ambiguous and receive no shared executable edge.
No approximate wall-clock window supplies financial identity. BlockTime lacking
hour attribution is exposed and retained in the final total.

Costs are never re-modelled here: our net/budget come from production selected_costs,
including working capital, fees, rent and reserve. Market meta.fee is deduplicated by
transaction. Compute priority uses requested limit, is already part of meta.fee, and
is not subtracted twice. Visible System transfers have UNKNOWN cost attribution.
Absent slot-correct historical prices keep Classic USD UNKNOWN; absent complete
winner execution costs keep winner net UNKNOWN. Loss dollar figures are opportunity
attribution, never realized monetary loss or guaranteed capture. Stage bottlenecks
are measured correlation, not causal attribution. Same-slot races remain UNKNOWN;
backfill cannot invent a live receipt timestamp or real competitor send deadline.

Pinned source exclusions are now closed: purge_deleverage_balance requires risk_admin,
TOKENLESS_REPAYMENTS_COMPLETE and a liability-free balance; it removes abandoned
asset shares with no payment to a liquidator. drift_claim_bad_debt claims the bank's
allocation into its vault-authority ATA and sweeps to the global fee wallet. Its payer
pays costs and receives no account-liquidation profit. Forced Deleverage and Bankruptcy
remain separately counted administrative settlement. The full exported instruction
namespace of pin 33c67987... is inventoried and unknown successful opcodes fail closed.
Source file hashes and namespace: docs/evidence/C0E1_CANONICAL_FINAL_20261010/source_path_inventory.json.

Deployment qualification uses the existing 0.1.11 upgrade-watch reference ProgramData
hash H57mqzeof3mnCmVTPJzmzeTfs7G6zbpaApQP4yNycr3f, checked via finalized reads before
capture and after full reconciliation. This proves continuity against the qualified
reference; it is explicitly NOT a reproducible-build bytecode/source-equivalence
attestation. The user's contract treats the pinned source as authoritative. New code
hashes require a new audit; no environment switch silently promotes them.

Fixtures: six real finalized Receivership transactions reused; Classic, Deleverage
and Bankruptcy layouts source-grounded; failed TX/CPI, LUT, mixed paths, multiple
events, execution order and UNKNOWN covered. Mixed real receiver + source Classic
is labelled a composed regression fixture, not a real mixed chain transaction.
Current short observation has not supplied new real Classic/Deleverage/Bankruptcy
examples; absence is not evidence that these paths cannot occur.

The first 120s reconnect diagnostic reached a real reconnect slot boundary, but Shadow
root catchup exhausted the original 30s wait and stayed HALTED. The configurable
30..300s wait retains the exact readiness condition; qualification launcher uses120s.
Eight bounded post-drain block reads replace serial loading; ordering, retries,
full-TX validation and fail-closed slot accounting are unchanged. Final qualification
and the 600s combined smoke must still complete. The user explicitly owns the 4h run.


Final user-directed handoff2026-10-10: no more600s agent testing. Implemented
transport corrections and local302/304-test matrix are green; differentialClippy
0/0. Short upstream/proxy probePASS.60s combined smoke remainedINCOMPLETE because
Shadow bootstrap/root catch-up had not produced evaluations; prolonged waiting
was stopped to preserve credits. All evidence retained, no new commit/push,
no4h launch. MEASUREMENT_LEDGER.md contains the exact remaining diagnosis and
resume commands. Code completion is not an end-to-end profitability/stabilityPASS.
