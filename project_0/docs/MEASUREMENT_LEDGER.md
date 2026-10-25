## 2026-10-13 — C0-E1 P4 core implementation / local qualification

Base `aa37dd3`. Capture finalization now saves local evidence without implicitly
starting strict/gap/enrichment RPC. Added separate capture-clock/live-coverage/
verification status, immutable capture identity/final evidence and spec/attempt/
result lifecycle. Independent Strict driver reuses existing reconciliation in
32-slot restartable batches with cross-batch blockhash anchors and bounded UTC
archive discovery. Missing archive/429/checkpoint/resource proof fails closed.

Executed: **190 Rust PASS / 0 FAIL / 1 ignored**, **release build PASS** (runner,
MeasurementPlan, strict worker), **5 contract + 29 index + 4 CLI Python PASS**.
Evidence/commands: [evidence/C0E1_P4_20261013/README.md](evidence/C0E1_P4_20261013/README.md).
Historical index gate remains **OPEN**, actual provider/archive qualification and
Mainnet smoke **NOT_RUN**. Optional crosscheck retains a separate INCOMPLETE gate
until independent-source qualification. No index result is canonical; no proof
repairs original live coverage. LIVE/submission disabled.

**Continue with P5:** wire typed CLI/post-capture finalization+verification and
normalized reports with all required display filters, reuse existing Economics.
Then P6 offline/recovery/replay tests. No new long capture.

---

## 2026-10-13 — C0-E1 P3 marker/pair fix — LOCAL PASS

Base `c78ae68`; v4.5.1 section L and the downloaded plan are identical.
`start_deleverage` added to the independent verifier, six markers mapped to four
classes. Complete starts/ends require matching target/actor/order and supported
outer start. Missing/ambiguous pairs remain UNKNOWN, failed TX never realized.
Completion events are bound to individual instruction positions: duplicate logs
cannot fill a missing completion elsewhere. `end_deleverage` was already detected;
this is not evidence that all previous deleverage events were absent.

Executed: Python verifier suite **29 PASS / 0 FAIL**, including six real saved
Mainnet fixtures. Evidence and exact command:
[evidence/C0E1_P3_MARKERS_20261013/README.md](evidence/C0E1_P3_MARKERS_20261013/README.md).
New decoder uses `verification/index_v2`; prior index_v1 evidence remains intact.
Historical 429/index gate **OPEN**; no new RPC/Mainnet/capture/submission.

**Next authorized work:** P4 separate capture/verification, stable identities and
bounded resumable strict proofs; then P5 normalized reports/typed display filters,
then P6 offline qualification. Long capture remains the user's action. LIVE disabled.

---

## 2026-10-12 — C0-E1 P2: bounded transport recovery / shared provider budgets — LOCAL PASS

**Resume:** P1 and P3 implemented; P2 now implemented and locally qualified from
`d40d89d8fef98e69c505141ea17dc4a7a3cde658`. No new Mainnet run, historical RPC
attempt or submission. User performs the long measurement after the remaining gates.

Implemented in the existing production path:
- C0-E1 readiness = subscription ACKs + actual advancing slot frame, exact common
  T_rx. Application/slot liveness independent of Ping/Pong; safe state/health journal.
- Fixed outage recovery 120 s / 8 failed attempts, 60 s idle detection, bounded
  backoff/Retry-After and cancellable shutdown. Slot progress across 10 s is needed
  before resetting the failure budget; duration 10m/24h does not raise it.
- 527 repeated failures coalesce into one gap interval without losing raw rows or
  unknown boundaries. Restored transport is separate from finalized gap recovery.
- Process-wide Solana quota ceiling: FIFO, 5 request/handshake starts/s, 2 in flight,
  64 waiters, per-workload spacing; shared 429 cooldown/circuit. SDK reads use the
  existing RpcSender interface; results and structured protocol errors preserved.
  Market deployment/history fetchers share the same budget. Jupiter remains its
  separate existing quote service. Actual provider capacity remains unqualified.
- Exhaustion aborts capture + Shadow through the existing shared flag, drains,
  saves INCOMPLETE reports and **defers expensive post-abort verification**.
  Ordinary submission construction, ObserveOnly/LIVE gates, protocol/economics
  decisions and actual fee/UNKNOWN rules are retained.

Actually executed: library matrix `crate_health,txv1` **240 PASS / 0 FAIL / 1 ignored**;
`crate_health` **238 PASS / 0 FAIL / 1 ignored** (ignored test pre-existing).
Runner binary **9 PASS**, P1 CLI **4 PASS**, final provider HTTP regressions **5 PASS**.
Locked offline release build of `p0-longtail` + `p0-measurement-plan` **PASS**.
Targeted changed-region formatting and `git diff --check` **PASS**. Relevant Clippy
completed, existing warnings retained; no full-workspace zero-warning claim.
Local actual socket fixtures prove WSS disconnect/recovery, ping-only/old-slot
failure, HTTP429 rejection/circuit, prompt cancellation, bounded concurrency and
SDK response/error preservation. Development failures were corrected before the
final qualification and their logs are retained.

Full scope, exact commands, source/binary hashes, consumer inventory and results:
[`evidence/C0E1_P2_20261012/README.md`](evidence/C0E1_P2_20261012/README.md).

**OPEN:** real provider quotas/account-sharing/archive/WSS availability; Mainnet
preflight/smoke; P4 strict partition/resume + resource safety; P5/P6 qualification.
Existing five-hour history remains resumable/HISTORICAL_INCOMPLETE (P3, RPC_RATE_LIMIT).
No historical liquidation count, economics profitability or five-hour stability
PASS follows from these local tests. LIVE remains disabled.

**Next:** report P2 and stop. P4 is the next implementation package; do not start a
new long run. Preserve `bsc/**`, all existing run data and evidence.

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

# Solana / Project 0 — Measurement & Evidence Ledger

## ADAPTIVE ROOT RECOVERY CANDIDATE — 2026-10-10 (LOCAL PATCH / UNQUALIFIED UNTIL TESTED)

- Source base: `f9a877c805272de410a5f8f313cdd7521fdfea9d`, WIP branch `project0/c0e1-startup-wip-20261010`; BSC/main untouched by intended patch.
- User-tested controlled market WSS disconnect smoke `c0e1_gap_smoke_20261010T195016Z`: technical PASS, MARKET_DATA_VALID, 72,816 Shadow evaluations, all 273 slots reconciled, gaps 1/1 recovered, 0 unresolved, 4 reports, 0 submissions; event-order check PASS.
- User-tested normal smoke `econ4h_20261010T204530Z`: final `completeness=PASS`, shell exit0 (detailed field-by-field output not yet independently inspected here). Earlier normal runs INCOMPLETE on 30s RPC response decode then 120s root catch-up soft timeout; `P0_SNAPSHOT_RPC_TIMEOUT_SECS=90` eliminated initial snapshot transport failures in the last inspected attempt.
- Proposed root-only adaptive policy: soft root wait120s, stall60s measured only from strictly higher finalized root notifications, absolute hard cap300s per attempt; 600s Shadow warmup absolute cap. Legacy fixed-timeout calls preserved. A root crossing its B target permits immediate final drain + existing exact reconcile. No overrides for gaps, missing accounts, dirty reconcile, explicit cancellation, ProgramData upgrades, submissions, or LIVE.
- Added deterministic watchdog regressions cover moving-root extension, stall, hard cap, fixed-timeout legacy behavior, and immediate readiness on completed root. Existing cancellation regression exercises adaptive limits. Full Rust matrix, release build and real 60s Mainnet run are **NOT YET PERFORMED ON THIS PATCH**.
- **Gate:** cargo test (both runner feature modes + solana state), cargo release build, `git diff --check`, then user-directed normal 60s Mainnet smoke, then separate Shadow WSS disconnect/ready smoke. Only then consider 4h. No profitability or long-run claim.

---

## CURRENT HANDOFF — 2026-10-10 19:57 Europe/Berlin — C0-E1 startup + controlled-gap smoke IN PROGRESS

**Evidence cutoff:** latest user-supplied terminal excerpt from run `c0e1_gap_smoke_20261010T195016Z`. This is a checkpoint, NOT a final run verdict. Historical entries below remain as-recorded and must not be read as current state. **No C0-E1 end-to-end PASS, no economics qualification, no 4h stability/profit claim, LIVE=false, submission=false.**

### Branch, implementation, test qualifications

- GitHub `main` baseline: `3fdf30b` after Project_0-only checkpoint; separate WIP branch `project0/c0e1-startup-wip-20261010` at `c2ba995e647296d1c3d3b0a134d2f30ff8db1d64` contains Astra's six Rust-file startup patch (+179/-9 lines). BSC uncommitted files are explicitly out of scope; no BSC changes staged or written.
- WIP patch: compressed `base64+zstd` getProgramAccounts snapshot; real Shadow READY barrier before the market capture clock; READY/first-ready monotonic timestamp; technical PASS requires actual in-window Shadow evaluations and readiness before capture; recovery cancellation remains fail-closed. The market-gap loopback proxy and finalized full-slot backfill were already present from previous checkpoints; neither is a production hotpath dependency.
- Local test correction 1, `snapshot.rs`: removed ambiguous `.into()` from two `RpcClient::new_mock_with_mocks("succeeds", ...)` fixtures. Initially Rust E0283/type-inference failure, then `cargo test --locked -p darkbot-solana state::` **exit 0, 19 PASS, 0 FAIL, 20 filtered**, including compressed/corrupt account and cancellation tests.
- Local test correction 2, `econ_tests.rs`: the real-timer 4-report fixture lacked an in-window Shadow evaluation after the new nonempty-measurement gate; added `w.accept(Evidence::Shadow(Box::new(rec()))).unwrap();` to `runtime_worker_writes_four_snapshots_then_identical_final`. Prior Runner `crate_health`: 173 PASS / 1 FAIL / 1 ignored, exit 101; corrected targeted test **exit 0, 1 PASS**. No production-code changes in these two corrections.
- After corrections, complete Runner test commands with `--features crate_health` and `--features "crate_health txv1"` both **exit 0** (user's tail output did not expose aggregate exact counts). Release `cargo build --locked --release -p darkbot-runner --features "crate_health txv1"` **exit 0**; `cargo clippy --locked -p darkbot-runner --all-targets --features "crate_health txv1"` **exit 0** with three reported bin warnings. The warning baseline/differential is NOT yet established for these latest edits.
- At this evidence cutoff the two local test-fixture corrections had **not yet been committed/pushed**. The new documentation checkpoint is a separate remote-only branch update; do not claim `main` or the local Mac worktree includes it before fetching. Do not `git add -A`, reset or clean the mixed BSC/Project_0 worktree.

### Real Mainnet combined smoke — observed, not finished

- Run ID: `c0e1_gap_smoke_20261010T195016Z`; started with `bash project_0/scripts/run_gap_reconnect_smoke.sh`. Actual run files on user's Mac: `project_0/data/econ4h/<run-id>/runtime.log`, market coverage/summary, economics final (when created), and `gap_proxy.log` (copied after launcher completion). These artifacts are LOCAL/UNTRACKED; their existence on GitHub is NOT implied by this entry.
- Launcher printed `C0-E1: build, then 60 seconds; four automatic snapshots; submission disabled`. Capture duration is 60 seconds, **not** total walltime: upstream preflight, Shadow bootstrap, drain, finalized full-slot reconciliation and reporting add time. Market warmup bound 120s (`P0_MARKET_READY_TIMEOUT_SECS`); root-catchup per recovery attempt defaults to 120s in the economics launcher.
- `C0_E1_WARMUP_WAIT timeout_s=120` emitted. Recovery attempt 1 Snapshot A failed at `liability_gpa` with `snapshot: error decoding response body` **despite compression**; RPC/provider response reliability is still open.
- Attempt 2: Snapshot A `slot=455376771`, `liability_accounts=82810`, `banks=204`, `required=494`, `optional=8`, `missing_required=0`, `total=83516`, elapsed `28425 ms`. Snapshot B `slot=455376902`, same counts, elapsed `29561 ms`. Root catch-up `target=455377037`, previous `Some(455376992)`, reached root `455377037` after `12546 ms`.
- Verified exact log order: `[recovery] ... root_catchup ok` -> `recovery: Ready, run_state=Ready` -> `C0_E1_WARMUP_READY; starting market stream and measurement window`. **Shadow reached actual READY before the measurement** in this run; prior 60s-startup race is addressed for this observation, but not yet a repeatability/profitability claim.
- Later runtime excerpt includes market WSS `session ended: ... Connection reset without closing handshake`, followed by `session attempt=2 connecting`, `connected; subscriptions=2 (programs=0 filtered_programs=0 accounts=0 slots=1 roots=0 tx=0)`, `subscription ACKs=1/2`, `2/2`, and `StreamReady`. This **shows a real reconnect and re-subscription**. The provided runtime excerpt alone does **not** show proxy `GAP_PROXY_CONTROLLED_DISCONNECT`, `GAP_RECOVERED`, `FULL_SLOT_RECONCILIATION_FINAL` or final `CONTROLLED_GAP_SMOKE_PASS`; therefore **controlled-gap end-to-end verdict PENDING**.
- Last displayed live Shadow telemetry (intermediate, not final): `ingress` sent=29978, received=29978, overflow=0; `threshold_ready=true`, `sweep_checks=1111897`, `sweep_liquidatable=97536`, `sweep_errors=4504`, `winner_watch dispatched=265`, `results=8`, `resolved=0`, `queue_full_backoffs=671`. Sweep candidates and 'liquidatable' health classifications **are NOT finalized market liquidation events or executable profits**. Winner/RPC backlog and error rates need final interpretation.

### Next decisive checks (in order; no speculative PASS)

1. Allow current test to finish normally; inspect its exit code plus `runtime.log`, `gap_proxy.log`, `market/coverage.jsonl`, `market/summary.json`, `economics_final.json`, four `hour_*.json` and `candidate_manifest.json`. Record the exact terminal `CONTROLLED_GAP_SMOKE_PASS` only if the harness actually emits it.
2. Demand explicit injected disconnect, reconnect subscription ACK + post-slot boundary, drain then `FULL_SLOT_RECONCILIATION_FINAL`, finalized `GAP_RECOVERED`, `gap_count>=1`, `gap_unrecovered=0`, `slots_unresolved=0`, zero analytics drops, four reconciled reports, real Shadow evaluation and no submissions. Any timeout/unresolved slot must remain `INCOMPLETE`.
3. If full-slot RPC backfill again exceeds budget, distinguish provider timeout/rate-limit/pruned slots from local recovery; do not silently truncate the slot window or infer economic zero. The `base64+zstd` change only improves snapshot payload size, not guaranteed upstream reliability.
4. Separately qualify **Shadow-WSS interruption** as distinct from the existing market-only loopback-proxy test; the latter cannot by itself prove Shadow's recovery after its own websocket loss.
5. After final short combined smoke qualification, user alone may start the 14,400s C0-E1 study with sufficient free disk. `main` promotion, LIVE trading, real fills, realized PnL, and profitable daily income remain **NOT QUALIFIED**.

---
## HANDOFF — 2026-10-10, stopped per user testing/credit priority

**Implemented and locally tested; NOT end-to-end qualified; no4h stability claim.**
Latest local base2c03b03. Corrections remain saved directly underproject_0; no new
commit/push because full runtime qualification remains incomplete. BSC untouched.
LIVE/submission disabled. No tests remain running from this task.

- Final workspace builds+release: exit0. Tests:302 health /304 txv1 PASS,
  0FAIL,1ignored each. Proxy2 testsPASS. Extra Shadow error-category testPASS.
- Differential Clippy vsb7f5ed3:0 new warnings in both configurations after
  removing one unnecessary unwrap in the test proxy; proxy tests rerunPASS.
- Edited-file formatting/shell syntax/diff checksPASS. Global cargo fmt remains
  baseline-dirty: all39 files in the user's reported format diffs also differ
  atb7f5ed3. No broad unrelated reformat.
- Direct upstream HTTP/WSS and short proxy ACK/slot/disconnect/reconnectPASS.
-600s runT191151Z: ABORTED_BY_USER,exit143; preserved evidence.
-60s runT191322Z: actual market capture reachedDRAINED, but Shadow still in
  startup recovery (SnapshotB/root catch-up,attempt3) with zero evaluations at
  the inspected fourth snapshot. Stopped prolonged wait under the user's credit
  priority,exit143. **INCOMPLETE**, no finalized full-slot or economic verdict.
  See `smoke_60s_incomplete.json` and `smoke_60s_recovery_excerpt.log`.

Remaining work is runtime diagnosis/qualification, not a claimed PASS:
1. Inspect the preserved60s recovery excerpt and actual runtime log: why the
   streamed root lags SnapshotB; determine provider/backlog versus local recovery
   failure before another combined run. Do not guess a network/TLS root cause.
2. Complete a controlled short combined smoke with real Shadow processing,
   finalized full-slot reconciliation,0unresolved slots/gaps,0drops and four
   reconciled reports. Current60s test starts Market capture before Shadow's
   potentially longer bootstrap; a60s capture alone cannot prove4h stability.
3. Only user starts4h after technical qualification. No additional600s run by
   the agent. No financial zero/known-profit claims from failed or empty runs.

Exact commands if the user resumes tests (already-green matrix need not repeat):
```bash
cd "/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot"
# Optional isolated endpoint/proxy verification; no full bot:
P0_GAP_PROBE_ONLY=true P0_GAP_INTERRUPT_AFTER_SECS=2 bash project_0/scripts/run_gap_reconnect_smoke.sh
# AFTER recovery diagnosis; current defaults60s capture,4x15s snapshots,15s drain:
bash project_0/scripts/run_gap_reconnect_smoke.sh
```
Block reconstruction adds wall time after capture. A stopped run is not PASS.
Exact build/test argv+exit codes, first failed/outdated assertion and its correction,
Clippy differentials, source hashes and diagnosis are saved in
`docs/evidence/C0E1_TRANSPORT_FINISH_20261010/`. The full Rust matrix predates only
an equivalent unwrap removal in the test proxy; its2 tests and both Clippy runs
were repeated after that edit. The smoke manifest identifies its actual binary.


## USER PRIORITY UPDATE — 2026-10-10

User explicitly stopped600s testing to conserve credits. Run
`c0e1_gap_smoke_20261010T191151Z` terminated with SIGTERM,exit143;
`user_requested_stop.json` preserves ABORTED_BY_USER. No PASS claim.
The existing reconnect smoke now defaults to60s capture, four15s snapshots and
15s Shadow drain. Finalized block reconciliation adds wall time after capture.
This short smoke is not four-hour stability proof. No more600s tests by this agent.

## ACTIVE — 2026-10-10 transport finish, implementation first

User checkpoint is now `2c03b03ad166b0c92589da295c3ff5284a51e2db`.
No pull/reset/merge/clean. BSC untouched. LIVE/submission remain disabled.
The user owns the future 4h run. Prioritize code, targeted checks, then additional
qualification only within available budget; never promote unexecuted checks.

New evidence: `docs/evidence/C0E1_TRANSPORT_FINISH_20261010/`.
- Prior phase-barrier smoke `c0e1_gap_smoke_20261010T125859Z`: actual600s,
  Shadow RPC quiescence at782.353s,216/2767 fetched,226991 TX,2551 unresolved,
  RPC_TIMEOUT + failed subsequent deployment RPC; **INCOMPLETE**.873696 evaluations
  received=streamed,4 snapshots,0 analytics drops. The429-only hypothesis was not
  a complete diagnosis; network/provider availability remains a separate risk.
- User run `...T131358Z`: no valid capture; direct local proxy connection was
  insufficient proof of upstream readiness. Historical IO cause remains UNKNOWN.
  DEPLOYMENT_RPC is a separate HTTP failure; SHADOW_RPC_BARRIER_FAILED is a
  consequence of Shadow exiting early, not proof of a barrier deadlock.
- Current direct HTTP200/finalized slot and direct WSS2ACKs+slot verified. Short
  proxy probe PASS: upstream preflight, both ACKs,real slot,controlled disconnect,
  reconnect,post-slot455365112. No change to production ingress.
- Code: proxy readiness now requires upstream2ACKs+slot; safe IO kind/OS-code
  diagnostics, bounded20s probe/connect; optional short probe mode in existing
  smoke script. Retry of a failed prefetched block now fetches only that block,
  never expands the batch under provider pressure; all reconciliation RPC starts
  paced; transient failures use bounded exponential backoff,5 total attempts.
  Deployment checks retry at most3 times and retain safe error categories.
- Targeted regression initially found an outdated assertion expecting250ms for
  timeout backoff; updated to the new2s policy. Original failed result preserved.

**Code corrections implemented; MASTER runtime qualification remains PARTIAL.**
Final full matrix: builds/releasePASS,302 health /304 txv1 testsPASS,0FAIL,
1ignored each; Clippy differential recorded separately. Proxy2 testsPASS and
error-redaction regressionPASS. No new4h run. No new commit/push yet.
The later user instruction supersedes600s: finish the single60s smoke, record its
actual result, then hand off. Do not launch more long tests.


## LATEST QUALIFICATION — 2026-10-10, rate-limit correction pending smoke

**MASTER PARTIAL; do not start 4h yet.** User explicitly owns the future 4h run.
Code is integrated in the production repository's `project_0/` tree, not a ZIP.
No stage/commit/push. No BSC changes by this task. LIVE/submission disabled.

Final-candidate 600s run `data/econ4h/c0e1_gap_smoke_20261010T021411Z`:
- Actual controlled reconnect: gap_count=1, recovered=1, unrecovered=0.
- Shadow state READY at shutdown, ingress overflow=0, winner overflow=0,
  analytics queue drops=0, retention overflow=0, program upgrade halt=false.
- Pinned path/deployment continuity: liquidation_path_coverage_complete=true;
  before/after finalized ProgramData hash matches existing 0.1.11 reference.
- **Full-slot FAIL: RPC_RATE_LIMIT**, fetched43 / expected2752, unresolved2709.
  market_capture_complete=false, technical INCOMPLETE. No Fake-PASS.
- Earlier 120s diagnostic did finish full-slot reconciliation:544/544 blocks,
  591719 TX scanned,0 unresolved; its Shadow recovery failed the old30s timeout.

Correction now implemented: pace parallel getBlock starts at5 requests/sec,
rate-limit retries1s/2s (bounded exponential policy); retain at most8 responses.
Default full-window budget100000 slots/14400s to cover a4h window at the observed
~4.5 slots/sec without truncating the ledger. Post-capture reconciliation can take
hours, separate from the4h observation duration. Unknown/unfetched slots remain FAIL.
Per-run candidate_manifest.json binds executable SHA256 and all code/script hashes.
Missing Shadow rx_slot now means UNKNOWN detection attribution, never false NOT_DETECTED.

Pre-rate-fix complete matrix:buildsPASS,294/296 testsPASS,0FAIL,1ignored each,
differential Clippy0/0. Updated full matrix logs and exact commands are in
`docs/evidence/C0E1_CANONICAL_FINAL_20261010/rate_limit_fix/`.
Next: verify updated matrix, repeat600s controlled reconnect with paced block fetch,
then record exact final status and commit only if master qualification fullyPASS.

## ACTIVE CONTINUATION — 2026-10-10 canonical join and qualification

**MASTER PARTIAL while final qualification is running.** User will run the 4h
observation personally; do not start it. LIVE=false / SUBMISSION=false.
Current HEAD remains ec853c4d38492869388440b5b034c0d8a6ee2677. No pull, merge,
reset, clean, staging, commit or push. BSC and original run evidence untouched.

Implemented beyond the previous checkpoint:
- Typed production plan identity and explicit pipeline progress; final market-driven
  replay of the complete disk journal, exact type/target/bank-pair plus canonical
  chain episode. Same-slot duplicate identities remain ambiguous; missing identity
  cannot create executable/economic proof.
- Canonical event-level join JSONL and four reconciled UTC/blockTime hour windows,
  cumulative/final all-path economics, six capital buckets, loss-dollar attribution,
  stage/race/competition evidence. Production selected_costs is the sole local
  economics source. UNKNOWN is never zero. Unassignable blockTime remains explicit.
- Transaction fees deduplicated by signature; actual meta.fee includes priority.
  Visible transfers retained without invented tip attribution; incremental rent and
  winner net remain UNKNOWN where unproven.
- Pinned instruction namespace inventory; source-qualified Classic, Receivership,
  Forced Deleverage and Bankruptcy. Sunset purge and Drift bad-debt claim explicitly
  excluded from permissionless profit. Deployment continuity checked before/after
  capture against the existing upgrade-watch reference hash; no bytecode/source
  reproducible-build equivalence claim. Different hash or unknown opcode fails closed.
- Full finalized slot proof is completeness authority; contained gaps reuse this
  proof. Post-drain block fetch is bounded to eight concurrent responses, validated
  and recorded in canonical order. Overall owner deadline includes reconciliation.

Local checkpoint: runner 165 PASS / 0 FAIL / 1 ignored before the final report-field
addition. Full builds/tests and differential Clippy now running; exact commands and
logs: `docs/evidence/C0E1_CANONICAL_FINAL_20261010/` (commands.json).

Controlled 120s diagnostic: `data/econ4h/c0e1_gap_smoke_20261010T020556Z`.
Actual forced disconnect and reconnect succeeded; first real post-reconnect slot
455084278 closes the gap from 455084270. Not a final candidate PASS: this binary
predates final path qualification/read-ahead and Shadow root catchup exhausted its
old 30s limit. Root stayed behind snapshot B, so readiness correctly remained HALTED.
The launcher now permits a bounded 120s catchup (same root/reconcile condition;
config 30..300s), and the final controlled smoke is 600s / four 150s snapshots.

Next: finish matrix, remove any candidate-only Clippy warnings, execute final 600s
combined reconnect smoke, record exact completeness and Shadow results. Commit only
if complete master qualification PASS; safe fetch/remote check before any push.
Never start the user's 4h run automatically.

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

Final checkpoint validation: locked workspace builds PASS; crate_health **280 PASS / 0 FAIL / 1 ignored**; crate_health txv1 **282 PASS / 0 FAIL / 1 ignored**; differential Clippy candidate-only **0 for both** against the preserved b7f5ed3 baseline. Edited-file rustfmt and git diff --check PASS. Whole-workspace fmt remains subject to the previously documented baseline failures.

LIVE/submission disabled. No source pin change. Current full workspace results and
Clippy differential are stored in the checkpoint results files; no master PASS implied.


## RESUME — 2026-10-09 MASTER CONTRACT SUPERSEDES GAP-ONLY TASK

**MASTER IMPLEMENTATION PARTIAL / NOT QUALIFIED. No new 4h run permitted.**
This resume block supersedes older completion/next-run statements below. Local starting
HEAD remains `db78cdb3d3507da66fe93450742a8235fefcde1a`; regression reference is
`b7f5ed3850e0b1e412d39c673d89a8c8ffc3644a`. No pull/merge/reset/clean was performed.
BSC work and the original runtime evidence were not modified or staged.

Implemented before the expanded master contract: bounded Gap -> Reconnect ACK -> first
post-reconnect Slot state; inclusive finalized-block recovery after drain; per-slot audit;
full static/loaded-key relevance checks using the F8 parser; signature dedupe; source-tagged
backfill events with null live timestamps; bounded retries/time/body/transaction budgets.
Skipped slots require a finalized successor's parent link, not null/-32007 alone.
The existing receiver decoder/F8 instruction truth is reused. This is **gap-only,
receivership-only infrastructure**, not the requested full-market engine.

Controlled smoke `c0e1_gap_smoke_20261009T081918Z` completed 120 seconds but is **FAIL**:
the proxy interrupted once, then reported `GAP_PROXY_UPSTREAM_CONNECT_FAILED`.
Both observed gaps lacked a post-reconnect boundary (`POST_RECONNECT_SLOT_UNKNOWN`);
`gap_count=2`, `gap_recovered=0`, `gap_unrecovered=2`, no recovery slots fetched.
Therefore this run does NOT qualify block recovery on Mainnet. The proxy's upstream
connection is now bounded by a 10-second timeout; this adjustment has not been retested live.
All original smoke files remain under `project_0/data/econ4h/`.

Master-contract safety adaptation: summaries now expose the previous narrower measure as
`receivership_transport_capture_complete`. Until full-slot reconciliation AND all active
liquidation families are implemented, `market_capture_complete=false`,
`full_slot_reconciliation_complete=false`, `liquidation_path_coverage_complete=false` and
`verdict=MARKET_DATA_INCOMPLETE`. Explicit missing-implementation reasons prevent a
gap-only success from being mistaken for complete Marginfi Market Truth.

Source audit at pinned Marginfi commit `33c67987a661f59c6683a65485ebb4f0ef030b77`:
Classic `lending_account_liquidate` and Receivership `start/end_liquidation` exist, but so
do forced `start/end_deleverage` and terminal bad-debt settlement
`lending_pool_handle_bankruptcy`. These additional paths require explicit classification;
the active-path audit is not yet exhaustive or Mainnet-qualified. Exact source locations,
discriminators and Classic economic caveats are recorded in C0_E1_IMPLEMENTATION_REPORT.md.

Old run `c0e1_4h_20261008T222326Z`: **OLD_4H_REQUALIFIED=false**. It contains no
persisted first/last canonical market slot boundary and no first post-reconnect slot frame.
The market summary's first/last observed slots come from admitted Marginfi transactions;
they are not canonical observation boundaries. Shadow rx/evaluation slots are from another
pipeline/session and cannot establish the missing market boundaries. Runtime contains no
serialized SlotUpdate/slotNotification records. No full-range salvage was attempted from
guessed endpoints; original data remain retention/stability evidence, not whole-market
economic proof. Zero receiver events is NOT zero Marginfi liquidations.

### Exact next implementation sequence (master contract)

1. Finish pinned-source path audit, including forced deleverage/bad-debt settlement, CPI/
   inner instructions, and current upgrade evidence. Define distinct event types without
   treating administrative settlement as permissionless bot profit.
2. Persist canonical slot-complete start/end and boundary provenance during capture;
   keep monotonic duration and chain-window duration separate.
3. Implement bounded post-capture reconciliation of EVERY produced finalized block across
   that entire window, with a slot ledger and proven skipped slots. Gap telemetry becomes
   a subset of this proof; live WSS is not completeness authority.
4. Add source-derived Classic/additional decoders, instruction/event parity, raw native
   amount provenance, unknown historical valuations, per-type and ALL economic aggregates.
   Do not turn a missing price or winner cost into zero.
5. Generalize MarketEvent and Market↔Shadow identity to type/bank pair/target/slot;
   preserve independent market counts, UNKNOWN costs, capital scenarios and race non-claims.
   Reconciliation-only events must never acquire a fake live receive timestamp.
6. Complete mandatory synthetic + real-fixture tests, builds, differential Clippy against
   b7f5ed3850..., and a controlled reconnect/full-slot Mainnet smoke. Only then enable
   completeness, consider salvage, and qualify a new four-hour run.

No commit or push has been made for this work. LIVE/submission remain disabled.
Local qualification evidence for the checkpoint is stored in
`project_0/docs/evidence/C0E1_GAP_MASTER_CHECKPOINT_20261009/` (commands/results/logs);
it is explicitly not a master-contract PASS.

Final checkpoint checks, after the master-contract fail-closed gate change:
both locked builds PASS; workspace `crate_health` 261 PASS / 0 FAIL / 1 ignored;
workspace `crate_health txv1` 263 PASS / 0 FAIL / 1 ignored; differential Clippy
`--workspace --all-targets` against b7f5ed3850... candidate-only 0/0.
Targeted rustfmt on all edited Rust files and `git diff --check` PASS.
Workspace-wide `cargo fmt --all -- --check` is NOT PASS: it exits 1 on existing
formatting outside the edited files; the exact b7f5ed3850... baseline also exits 1.
No broad formatting of unrelated Project_0 files or BSC files was performed.
The original Cargo archive for baseline Clippy initially lacked docs/evidence fixtures;
after exporting those exact baseline fixtures both baseline runs completed successfully.
No new full-slot or all-path Mainnet qualification is implied by these local checks.

Status: **C0-E1 locally integrated; both builds and workspace test suites PASS; differential Clippy / combined Mainnet smoke / 4-hour observation OPEN; LIVE disabled**
Updated: 2026-10-08

This is the single human-readable measurement ledger for the new Solana / Project 0 Liquidator.

Primary economic objective:

> **maximize sustainable realized net profit while avoiding unnecessary recurring portal/SaaS costs.**

No Project 0 latency, capture-rate, winner-position, provider-benefit or realized-PnL measurement is currently recorded here. Values remain **N/A** until produced by the actual implementation.

## 0. Rules

- Never write an unmeasured value as 0.
- Separate local processing latency from network/inclusion/landing evidence.
- Submission ACK or Jito bundle ID is not landing evidence.
- Use the same timing vocabulary across replay, shadow and LIVE.
- Every result records source commit, config/source manifest, sample count and raw-evidence path.
- Gate/benchmark code must exercise production components, not duplicate business logic.
- Paid provider cost must be measured alongside its incremental benefit.
- Do not invent p(win), p(land) or competitor-response probabilities.
- Failed-attempt cost is submission-path/outcome dependent.
- Provider convenience is not evidence of economic necessity.

## 1. Timing vocabulary

~~~text
T_rx               update received by Rust ingress
T_detect           candidate classified as liquidatable/actionable
T_ready            complete execution plan/transaction(s) ready
T_sign             signing complete
T_handoff          payload handed to selected submission adapter
T_observed_landed  landing observed/reconciled from chain evidence
~~~

Derived local intervals may include:

~~~text
T_rx → T_detect
T_detect → T_ready
T_ready → T_sign
T_sign → T_handoff
T_rx → T_handoff
~~~

Landing evidence additionally records:

~~~text
signature
landed slot
transaction index where derivable
confirmation status
submission/bundle id
transaction version
serialized bytes
submission path
single tx / bundle
actual fees
actual tips
observed receiver/token deltas
realized PnL
~~~

## 2. Phase evidence table

| Phase | Measurement | Status | Evidence |
|---|---|---|---|
| S0 | timing/telemetry + provider abstraction contract | WORKSPACE TEST PASS on Mac under `crate_health` and `crate_health txv1` — see 2026-10-02 session | project_0/native/crates/darkbot-common |
| S1-LT | WsRpcControlSource, event model, v1 detection | COMPILES + UNIT TESTS PASS in workspace feature runs; live provider behavior not yet qualified | darkbot-solana/src/ingress |
| S2 | state reconciliation/fork/gap recovery | UNIT TESTS PASS in workspace feature runs; Mainnet recovery evidence still N/A | darkbot-solana/src/state |
| S3-CORE | v0 builder, v1 capability measurement, signer, RPC+Jito adapter, reconciliation | BUILD/UNIT TEST baseline PASS; v1 Sign/Wire/Submission remains unavailable | darkbot-solana/src/{tx,submit} |
| S4 | Python↔Rust Pump.fun parity + native baseline | N/A | not started |
| C0 | account census (types/sizes) | TOOLING WRITTEN, selftest PASS offline, mainnet run NOT done (no egress) — see §9 | project_0/scripts/c0_census.py |
| H0 | historical Project 0 liquidation/composition/competition | N/A | not started |
| P2 | reverse/threshold index, premium+tag scheduler | UNIT TEST baseline PASS in workspace feature runs; P3-02 thresholds still open | darkbot-project0/src/{index,schedule}.rs |
| P3 | health via marginfi crate | `crate_health` compiles/tests PASS against pinned marginfi tag; exact thresholds P3-02 still open | darkbot-project0/src/health.rs |
| P4A | deterministic economics/ranking, premium curve | Prior unit-test baseline PASS; concrete ProtocolSizingProvider now wired; failed-cost evidence still open/fail-closed | darkbot-project0/src/{economics,sizing}.rs |
| P5 | receivership sequence/allowlist/bundle rules | Crate-based builders/planner prior tests PASS; runtime ProtocolSizer + Jupiter V2 RouteProvider + P0ReceivershipBuilder are now wired and CURRENT HEAD compiles | darkbot-project0/src/{planner,receivership}.rs + darkbot-runner |
| P6 | full hotpath + shadow race | N/A | not started |
| P7 | capture/economics/competition/provider-value evidence | N/A | not started |
| P4B | empirical p(win)/tip/stopping ranking | N/A | blocked on P7 |
| P8 | controlled LIVE reconciliation | N/A | not started |
| P9 | production profit/provider economics | N/A | not started |

## 3. Required record shape

Every measurement entry should state:

~~~text
date/time
phase
repo commit SHA
protocol/source manifest
runtime config
provider/region
provider monthly cost
provider usage cost
hardware/runtime environment
sample count
metric definitions
p50
p95
p99
max
raw evidence path
PASS / ADAPT / STOP
known limitations / non-claims
~~~

Where economic evidence applies also record:

~~~text
gross liquidation value
protocol/flat fees
swap/DEX fees
slippage
priority fees
Jito tips
failed-attempt cost
capital used
realized net PnL
provider cost
incremental opportunities attributable to provider/path
incremental realized/expected profit attributable to provider/path
~~~

## 4. Submission-path failure matrix

Every qualified submission path must eventually populate this matrix from real evidence:

| Path / outcome | Base fee | Priority fee | Tip | Other cost | Evidence |
|---|---:|---:|---:|---:|---|
| RPC not landed | N/A | N/A | N/A | N/A | not measured |
| RPC landed success | N/A | N/A | N/A | N/A | not measured |
| RPC landed revert/fail | N/A | N/A | N/A | N/A | not measured |
| Jito single-tx not landed | N/A | N/A | N/A | N/A | not measured |
| Jito single-tx landed success | N/A | N/A | N/A | N/A | not measured |
| Jito single-tx landed revert/fail | N/A | N/A | N/A | N/A | not measured |
| Jito bundle not landed | N/A | N/A | N/A | N/A | not measured |
| Jito bundle landed success | N/A | N/A | N/A | N/A | not measured |
| Jito bundle landed/reverted | N/A | N/A | N/A | N/A | not measured |

No economics code may replace unknown cells with zero.

## 5. Provider economics matrix

Every paid provider considered for permanent production use must eventually have:

| Provider/path | Monthly cost | Usage cost | Coverage benefit | Latency benefit | Incremental captured profit | Keep/remove |
|---|---:|---:|---|---|---:|---|
| N/A | N/A | N/A | N/A | N/A | N/A | not measured |

Qualification rule:

~~~text
keep paid provider only if:
incremental sustainable net profit
>
full recurring/usage/dependency cost
~~~

## 6. Probability-model gate

Before P7:

~~~text
p(win) = UNKNOWN
p(land) = UNKNOWN unless directly measured for the path
optimal_wait = UNKNOWN
~~~

P4A uses deterministic economics only.

P4B may start only when H0/Shadow/P7 provide a versioned empirical dataset sufficient to estimate and validate:

~~~text
p(win | opportunity, premium, tag_age, tip, submission_path)
~~~

## 7. Raw evidence location

A dedicated raw-evidence directory does not yet exist because Project 0 implementation has not started.

When created, use one canonical location and link it here. Do not scatter Project 0 raw measurements across the older Pump.fun P0_* files.

## 8. Current status

~~~text
Project 0 production code:        Full Rust Long-Tail path active on Mainnet Shadow; fixes through [FIX-105] committed
Bootstrap/recovery:               MAINNET PASS (StreamReady → Snapshot A/B → root catch-up → reconcile clean → READY)
Exact Health/Sizing:              MAINNET exercised; bankruptcy/integration cases typed fail-closed
Build + Sign + simulation call:   MAINNET PROVEN (R97/R99/R101)
Successful receivership sim:      OPEN — no ShadowTechnicalReady yet
Current immediate blocker:        ZERO_PREFUND Jupiter route rejection reason; [FIX-105] committed, R105 pending
Liquidator payer:                 FUNDED / VERIFIED; dedicated Project_0 signer
Required Trrq ATAs:               WSOL + USDC PREWARMED and confirmed on Mainnet
R4 hot-path isolation:            PASS — R108 runtime-qualified
Integration refresh completeness: OPEN (Kamino/Drift/JupLend paths)
Failed-cost / realized economics: OPEN / UNKNOWN by design
72 h P6-LT Shadow:                NOT STARTED
Project 0 LIVE:                   NOT ENABLED
First realized net profit:        NOT PROVEN
permanent paid provider approved: NONE
probabilistic ranking approved:   NO
~~~

## 9. Session log

### 2026-10-01 — Sitzung 1 (Fable, Chat-Container ohne Egress)

**Umgebung:** kein `cargo`/`rustc`/`protoc`; Egress-Proxy blockt github.com und mainnet.helius-rpc.com (HTTP 403). Alle Netzwerk-Tickets sind in dieser Umgebung nicht ausführbar.

**BLOCKER (echt, dokumentiert nach Work-Order-Regel):**
- P0-01, P0-02, P0-04: Source-/Lizenz-Lesen von `0dotxyz/eva01`, `mrgnlabs/mary`, `solana-dex-superagg` nicht möglich (403). Betroffene Quelle: GitHub. Minimale Option: Tickets in Claude Code auf dem Mac ausführen; kein Workaround über Sekundärquellen zulässig.
- P1A-01: on-chain ProgramData nicht abrufbar (403). Program-ID `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` und Group `4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8` sind **Sekundärwissen, PRÜFEN**.
- C0-01 Mainnet-Lauf: nicht möglich (403). Tooling geschrieben und offline getestet.

**S0 (S0-01..05) — CODE WRITTEN:**
- `native/Cargo.toml` (workspace, resolver 2), `project_0/native/crates/darkbot-common` (edition 2021, **null externe Dependencies**).
- Module: `timing` (Clock monoton, Stage T_rx..T_observed_landed, Timeline mit Intervallen; fehlender Punkt → None, nie 0; kein Overwrite), `histogram` (log-linear 8 Sub-Buckets, p50/p95/p99 konservativ als obere Bucket-Grenze, max/min/count exakt, leer → N/A), `channel` (bounded über sync_channel, Overflow explizit gezählt, Element zurückgegeben), `lifecycle` (Shutdown, RunState fail-closed: Halted behält ersten Grund, Ready nur über Reconciling), `journal` (Record mit seq/t_rx/source/kind/payload; SeqGap → Fehler; MemJournal + Replay), `config` (Pflichtwerte → Fehler, explizite Defaults), `error` (HaltReason-Taxonomie).
- Unit-Tests in jedem Modul (14 Tests). **Nicht ausgeführt** — Gate S0 bleibt offen bis `cd project_0/native && cargo test -p darkbot-common` auf dem Mac PASS meldet. Ergebnis hier eintragen (Toolchain-Version, Test-Count, PASS/FAIL).
- Bewusst nicht gebaut: lock-freier SPSC-Ring (erst nach Messung), Datei-Journal (S1).

**C0 — TOOLING WRITTEN:**
- `project_0/scripts/c0_census.py`: ein `getProgramAccounts` (confirmed, withContext), JSONL-Dump mit SHA-256, Typ-Census über Anchor-Discriminators, Größenverteilung pro Typ, unbekannte Discriminators gezählt. Health-Klassifikation steht explizit als N/A bis P1A-Layout.
- `--selftest` offline: PASS.
- Discriminators (sha256("account:<Name>")[:8]): MarginfiAccount 43b2826d7e721c2a · Bank 8e31a6f2324261bc · MarginfiGroup b617adf097ceb643 · FeeState 3fe01055c124ebdc · LiquidationRecord 5f74178459d2f5a2. PRÜFEN gegen IDL des deployed Tags (P1A-01). Ein unbekannter Discriminator mit hoher Anzahl im Mainnet-Lauf bedeutet: Typname im Script falsch, nicht Chain-Anomalie.
- Mainnet-Lauf: `HELIUS_API_KEY=... python3 project_0/scripts/c0_census.py --out project_0/data/c0/` → Report + Rohdaten-Pfad + SHA hier eintragen. Credits: ein gPA-Call; Rohdatenmenge (bytes_total) hier dokumentieren.

**Gate-Entscheidungen:** keine. S0 PASS offen (cargo test). C0-GO/STOP offen (Mainnet-Lauf + Health-Klassifikation).

**Nächstes Ticket in einer Umgebung mit Egress:** P0-01, dann P1A-01 (Program-ID verifizieren), dann C0-01 Mainnet-Lauf, dann S0 `cargo test`.

### 2026-10-01 — Sitzung 2 (Fable, Chat-Container ohne Egress) — CHECKPOINT

Auftrag: FIRST EURO PATH ohne Stopp bei fehlenden Testmöglichkeiten; alles **UNTESTED / VERIFY**.
Kein `cargo`, kein Netzwerk. Nichts kompiliert. Offline ausgeführt: nur `c0_census.py --selftest` (Sitzung 1).

**Geschrieben (Rust, ~3.760 LOC inkl. Tests):**
- `darkbot-common` (S0, Sitzung 1) — unverändert.
- `darkbot-solana` (S1-LT, S2, S3-CORE): `ingress/{mod,ws_rpc}.rs` (IngressSource-Contract, Event-Modell, programSubscribe/accountSubscribe/slot/root, T_rx vor Parse, Reconnect → Gap+Reconnected, Sink-Overflow → Gap, v1-Erkennung config→versioned→legacy für JSON und Bytes), `state/{mod,snapshot,blockhash}.rs` (Bootstrap-Puffer, Snapshot-Install mit "Updates > S", Ordnungsregeln slot/write_version mit Zählern, Root-Policy, Reconcile-Report → HALT bei Mismatch, explizite Freigabe; RPC-Snapshot; Blockhash-Cache ohne RPC im Hotpath), `tx/mod.rs` (v0-Builder mit ComputeBudget, 64-Lock-Limit vor Compile, Größenlimit, v1-Stub hinter Feature `txv1` mit dokumentierter API, Signer, Keypair-Loader), `submit/{mod,rpc,jito}.rs` (SubmissionAdapter, Ledger-§4-Outcome-Taxonomie, RPC-Submit + Reconciliation mit maxSupportedTransactionVersion=1, Jito sendTransaction/sendBundle(≤5)/getTipAccounts/getBundleStatuses).
- `darkbot-project0` (P1A-Konstanten, P2, P3-Stub, P4A, P5-Regeln): `protocol.rs` (Discriminators, Dual-Size-Bank 1856/3904 → sonst UNKNOWN_LAYOUT), `index.rs` (ReverseIndex 4 Richtungen; ThresholdIndex pro Bank/Seite mit BTreeMap-Range-Lookup + margin), `schedule.rs` (TimeScheduler Min-Heap mit Invalidation; premium_boundary), `economics.rs` (Premium-Kurve 0.1.12: Basis max(FeeState,5 %), 1 h Grace, linear bis 100 % nach 7 d, blockiert bei Bad Debt/EMODE; Umkehrfunktion time_for_premium; FailedCostMatrix mit Option → Unknown nie 0; evaluate/actionable/rank), `health.rs` (HealthEngine-Contract, NoHealth fail-closed, Crate-Anbindung als Feature `crate_health` offen), `planner.rs` (validate_receivership: End letzte Ix ohne Ausnahme, Allowlist vor/zwischen Start–End, NO_SWAP verbietet Swap, Paid nur nach P7; validate_bundle: kein Start/End-Split, ≤5).

**Bewusst NICHT gebaut:** eigene Health-Formel (Regel 3), lock-freier SPSC-Ring (erst nach Messung), GeyserSource (S1-RACE), Race-S3-Tickets, Direct TPU, Deshred, Instruction-Bau für Receivership/Tagger (braucht Crate/IDLs aus P0/P1A), TX-Template-Cache (P2-06; Invalidation-Liste in schedule.rs dokumentiert).

**VERIFY-Schwerpunkte für den Mac (in dieser Reihenfolge):**
1. `cd project_0/native && cargo build` — Dependency-Versionen in beiden Cargo.toml sind geraten (solana-* 3.x, tokio-tungstenite 0.24, reqwest 0.12, bincode 1). Erwartete Reibung: `Keypair::from_bytes` vs `try_from`, `RpcProgramAccountsConfig.sort_results`, `get_program_accounts_with_config` ohne Context-Slot (siehe Kommentar in snapshot.rs), `compute_units_consumed` OptionSerializer, `Message::Text(.into())` bei tungstenite.
2. `cargo test --workspace` — 40+ Unit-Tests, alle offline.
3. `c0_census.py` Mainnet-Lauf → Report ins Ledger.
4. P0-01/P0-02 (GitHub) → Crate-Tag bestätigen → Feature `crate_health` anbinden (P3-01).

**Gate-Entscheidungen:** keine. Alle Gates offen bis Build/Test/Mainnet-Lauf.

### 2026-10-01 — Sitzung 3 (Fable, Chat-Container ohne Egress) — CHECKPOINT nach Astra-Review

**Korrigiert (materiell, aus Astras Review):**
1. `schedule.rs`: `Trigger` jetzt `Hash` (Compile-Fehler).
2. `economics.rs`: Rampe = 7 d − 1 h → 100 % genau 7 d nach Tagging; VERIFY Bezugspunkt in `liquidate_start.rs`.
3. `state/mod.rs`: `confirm_ready()` nur nach sauberem `reconcile()` nach dem letzten `install_snapshot()` (Flag `reconciled_clean`); Tests angepasst.
4. `state/snapshot.rs`: Context-Slot aus **derselben** gPA-Antwort (`send(GetProgramAccounts)` + `OptionalContext`), kein separates `getSlot`.
5. `install_snapshot`: per-Account-Slots bleiben erhalten; `snapshot_slot` = untere Schranke; gepufferte Deltas per Account-Vergleich. `multiple_accounts` liefert min-Slot statt max.
7. `ws_rpc.rs`: fehlender `context.slot` → Protocol-Fehler; unlesbarer Frame → Session-Ende (Gap/Reconnect), kein `continue`.
12. `jito.rs`: `getTipAccounts` auf `/api/v1/getTipAccounts`; `bundle_status` normalisiert zu `BundleStatus {NotFound|Pending|Landed|Failed}`.
15. `index.rs`: `set_bank_oracles`/`set_bank_integration` entfernen alte Zuordnungen (Invalidation), Test ergänzt.
24. `c0_census.py` Selftest: Bank-Rohlänge 1864 (8 + 1856), konsistent mit `protocol.rs`.

**Neu gebaut (quellenunabhängig):**
- S1-01: `IngressEvent::Transaction` (+ `TransactionObserved`, Version in Pflichtreihenfolge) und `IngressEvent::Health` (`SourceHealth`, alle 5 s). `transactionSubscribe` als Provider-Erweiterung markiert (VERIFY).
- S1-07/S2-04: `state/recovery.rs` — `RecoveryDriver`: Gap → HALT → Snapshot A → install → Snapshot B → reconcile → confirm_ready; begrenzte Versuche, bleibt HALTED bei Mismatch; injizierter Snapshot-Fetch, offline getestet (2 Tests).
- S3-01: `BuildVersion::Legacy` + `build_legacy`; Resource-Ixs gemeinsam.
- P2-06/07: `prepared.rs` — `PreparedCache` mit `InvalidationKey` (account_slot, oracle_slots, tagged_at, premium_flags, lut_version, fee_state_version); Hit/Miss/Invalidation-Zähler.
- P4A: `t_profitable()`-Solver: net(p) > min ⇔ p > Bedarf, gedeckelt durch `premium_cap_bps` (Receivership-Regel), Umkehr über `time_for_premium`; Tests.
- P8-LT: `safety.rs` — Limits (exposure, daily loss, failed attempts, tip, slippage, outstanding), Tagesrollover, HALT ohne Auto-Reset, `record_outcome` mit realisiertem PnL; Tests.
- Runner: `darkbot-runner` — `pipeline.rs` (Health → Plan → Economics → Planner-Validierung → Build → Sign → Send-Boundary [Shadow] | Safety → Submit [Live]; jede Stufe setzt Timing-Punkt; Fehler = Record mit LossReason), `record.rs` (OpportunityRecord, Decision, LossReason inkl. THRESHOLD_TOO_HIGH), `main.rs` (`p0-longtail`: Env-only-Config, Ingress-Thread → bounded channel → State + RecoveryDriver; Live-Modus verweigert Start, bis P8-LT im Ledger PASS ist).

**Weiterhin offen — nur mit Quellen/Compiler lösbar (Begründung, keine Ausrede):**
- S2-02 Fork-Graph: `SlotUpdate.parent` wird gehalten, aber kein Branch-State/Rollback. Stufe 1 läuft auf `confirmed`; Processed-Verarbeitung ist Race (S1-RACE). Als PARTIAL geführt.
- S3-02 v1-Builder: API `v1::Message::try_compile_with_config` nur mit solana-sdk ≥ 4.2 verifizierbar.
- P2-01 RAM-Modell der P0-Typen, P3-01..05 Health, P5-02/05/06/07 Instruction-Bau, T (Tag-Ix): brauchen `marginfi`-Crate/IDL (GitHub gesperrt).
- C0-02..04, S4, P6-LT-Lauf: brauchen RPC bzw. Build.

**Zählung:** ~4.790 Zeilen Rust, 60+ Unit-Tests im Source, **nichts kompiliert**.

### 2026-10-01 — Sitzung 4 (Quellen marginfi-v2-main + eva01-main als Zip gelesen) — CHECKPOINT

**Jetzt BELEGT aus Source (vorher PRÜFEN):** Program-ID Mainnet `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` (id-crate), Receivership-Allowlist (8 Programme), Refresh-Allowlist, Exclusive-Ix-Liste, Ix-Discriminators, Seeds (`liq_record`, `liquidity_vault_auth`, `feestate`), Seize-Grenze `seized ≤ repaid×(1+max(fee,5 %))` in Equity-Werten, Post-Health-Regel, Closeout < 5 USD, Flat-SOL-Fee in `end`, Health-API `HealthPriceMode::Client(Clock)`, Remaining-Accounts-Layout, Oracle-Modell (Pyth Push PDAs on-chain). **Kein System-Programm in der Receivership-TX → Jito-Tip in separater Bundle-TX** (P5-Konsequenz, neu).

**Neu gebaut:** `protocol.rs` (komplett aus Source), `receivership.rs` (P5-06: init_liquidation_record, start, withdraw, repay, end mit exakter Account-Reihenfolge; `build_receivership`), `health.rs` `crate_impl` (P3-01: Observation-Accounts in Programm-Reihenfolge, `get_health_components` Maintenance+Equity über Client-Clock, HealthView mit Closeout/BadDebt), `economics::max_seizable_for_repay`, `state/fork.rs` (S2-02 Fork-Tracker, Orphan-Erkennung, `StateCore::invalidate_slots` → HALT), `IngressHalt` (Out-of-Band), Recovery-Barrier (`drain`) + Dedupe (Gap hält, Reconnected recovert), `begin_reconcile` aus Ready, Lock-Zählung auf finaler Ix-Liste, Safety mit Gesamtkapital/Inventar/Concurrency/Asset-/Klassen-Allowlist + `Pipeline::reconcile_outcome`, Runner: `AccountSource` über StateCore, Trigger (a) auf MarginfiAccount-Updates → Health-Record (JSONL) im Shadow, Feature `crate_health` durchgereicht.

**Astras 10 Punkte:** 1–9 umgesetzt; 10 teilweise (Trigger a → Health-Record verdrahtet; Threshold-Index/Scheduler/Pipeline-Evaluate-Vollverdrahtung folgt mit P3-02 Schwellen und P4A-03 Sizing).

**Weiterhin offen (jetzt mit Quelle, aber ohne Compiler):** P3-02 Preisschwellen (`thresholds()` fail-closed), P4A-03 max liquidatable amount, P5-02 Refresh-Planner (Kamino/Drift/JupLend-Ixs aus deren IDLs in `marginfi-v2/idls`), P5-05 allowlisted Swap-Ix, T (Tagging erst 0.1.12), v1, S4, P6-LT-Lauf, C0-02 (Health-Klassifikation kann jetzt über Crate laufen — braucht Build + RPC).

**Zählung:** ~5.550 Zeilen Rust, 63 Unit-Tests im Source, nichts kompiliert. Ticketnummern angeglichen: NO_SWAP = P5-03, Receivership-Builder = P5-06.

### 2026-10-01 — Sitzung 5 (Merge eva01 / marginfi-v2 / DarkBot) — CHECKPOINT

- `receivership.rs`: manueller Builder **entfernt**; Instructions über `marginfi::accounts::*.to_account_metas(Some(true))` + `marginfi::instruction::*.data()` (BELEGT an `p0-cli/src/processor/account.rs`); Init-Record inline vor Start; EndLiquidation mit `fee_payer: None`, `global_fee_wallet` aus FeeState.
- `execution.rs` neu: No-Route-Quarantäne, exponentielles Target-Backoff mit Drop, Simulate-Gate (nur `Ok` sendet; RPC-Fehler ist kein Freifahrtschein), Bundle-Status → Chain-Confirmation-Regel, sequenzieller Fallback nur bei einer TX. Verhalten aus eva01 adaptiert, kein Code kopiert. 3 Tests.
- Workspace-Pins aus marginfi-v2 übernommen: `solana-sdk =3.0.0`, `solana-client =3.1.12`, `anchor-lang 1.0.2` (Feature `crate_health`).
- `P0_REUSE_MATRIX.md`: Merge-Tabelle mit DIRECT_DEPENDENCY / REUSE_ADAPT / KEEP_DARKBOT / DROP für alle Komponenten.
- Neuer Edge-Kandidat dokumentiert: In-Receivership-Swap (kapitalfrei, vom Protokoll erlaubt) + Tx v1 bei >1232 Bytes — eva01 fährt stattdessen JIT-Buy aus USDC-Reserve. VERIFY: Bytes per Staging-Simulation.
- Offen (Quellen vorhanden, Compiler fehlt): P5-02 Refresh-Ixs (Kamino/Drift/JupLend aus Mocks-IDLs), P5-04/05 JIT-Buy-Fallback + superagg-Economics-Gate, Bundle-Bau mit Tip-TX, P4A-03 Sizing, P3-02 Schwellen, Runner-Vollverdrahtung.
- Zählung: ~5.640 Zeilen Rust, 66 Tests im Source, nichts kompiliert.

### 2026-10-01 — Sitzung 5b (Astra-Zusatz: Funding Ladder, Drift-Muster, ZERO_PREFUND-Fixture)

- `funding.rs`: `FundingClass` {ZERO_PREFUND_SAME_TX, NO_SWAP_INVENTORY, FLASH_FUNDED, JIT_SHORTFALL, PREHELD_INVENTORY}. **Keine feste Ausführungspräferenz:** `feasible_classes()` emittiert alle technisch/protokollarisch machbaren Klassen; P4A + TX-Feasibility + Safety wählen die beste ausführbare Variante. FLASH_FUNDED bleibt für P0 ungültig (Allowlist). `capital_required()`/`tx_count()` bleiben Teil von Economics/Telemetry.
- `execution.rs` erweitert (keep-rs-Muster, adaptiert): `Freshness` (Opportunity-Age, Execution-Deadline), `select_route()` über parallele Quotes nach **realisierbarem** Netto, `priority_fee_percentile()`. Safety: `release_without_outcome()` für deterministische Kapitalfreigabe vor Submission.
- Pipeline: Deadline-Check vor Send-Boundary; Funding-Klasse im Record.
- `fixture.rs` (Feature `crate_health`): ZERO_PREFUND-Harness — vollständige Jupiter-Route (`tokenLedger`, setup, other, swap, cleanup) plus `addressLookupTableAddresses`; v0 lädt **echte LUT-Accounts**, v1 nutzt keine LUTs. Allowlist-Check und vollständige Receivership-Ix-Liste aus Crate-Buildern; Messung bytes/locked/ix-count. Simulation + Staging-Lauf bleibt Mac-Aufgabe (`simulateTransaction`, sigVerify=false).
- **v1 korrigiert:** Solana-3.x (marginfi) und modulare Solana-4.x-v1-Crates können im selben Cargo-Workspace koexistieren; kein zweiter Business-/Execution-Core und kein marginfi-Upgrade ist architektonisch nötig. `darkbot-solana` hat einen isolierten v1-Capability-Adapter nach `solana-foundation/transaction-v1-examples` (`solana-message 4.4.1`, `solana-transaction 4.1.6` Referenz). `measure_v1()` kompiliert die v1-Message/TransactionConfig und misst 4096/64/64-Gates; der finale signierte v1-Wire-/Submission-Adapter bleibt **UNTESTED/VERIFY** bis zur Compiler-Runde.
- Candidate-Auswahl in der Pipeline: alle vom Planner gelieferten Funding-/Route-Klassen werden einzeln auf Protocol-Validität, P4A-Economics, LIVE-Safety und TX-Baubarkeit geprüft; höchster deterministischer Netto-Profit gewinnt, bei Gleichstand höhere Kapital-Effizienz. Shadow wird nicht von `live_enabled=false` blockiert.
- Offline-Evidence dieses Imports: `python3 -m py_compile project_0/scripts/c0_census.py` PASS; `python3 project_0/scripts/c0_census.py --selftest` PASS. Rust weiterhin nicht kompiliert (kein `cargo` in dieser Umgebung).
- Zählung: ~6.400 Zeilen Rust im importierten Stand; Rust-Tests im Source, **nichts als Rust PASS deklariert**, bis Mac-Compilerlauf grün ist.

### 2026-10-01 — Sitzung 6 (Fable-Audit von 87a9dd6 + Korrekturen, Zip) — CHECKPOINT

Audit-Ergebnis: 5 Blocker, 4 High, 3 Medium (Details im Chat-Protokoll / Commit-Message). Korrigiert, markiert als `[FIX-n]` im Code (`grep -rn "\[FIX-" native`):
- FIX-1 `runner/main.rs`: Pipeline in `p0-longtail` eingehängt (Shadow): Keypair, Blockhash-Thread, Safety aus Env, Allowlist aus Source, RpcSubmitter (nur Simulation), Records als JSON. Builder = `NoPlans` (fail closed), bis P4A-03-Sizing existiert.
- FIX-2 `runner/builder.rs` neu: `P0ReceivershipBuilder` mit `SizingProvider`/`RouteProvider`-Traits; ein Plan je machbarer FundingClass; JIT als Prerequisite-TX; Init-Record inline nur wenn PDA fehlt.
- FIX-3 `pipeline.rs`: `ReceivershipPlan.prerequisite_txs`; Kandidaten mit Prerequisites nur über Bundle-Pfad.
- FIX-4 `submit/mod.rs`: `build_tip_tx` (legacy, System-Transfer) + `SubmissionAdapter::submit_bundle`; `jito.rs` implementiert es; `pipeline.rs` baut `[prereqs…, receivership, tip]`.
- FIX-5 `runner/pending.rs` neu + `main.rs`: offene Submissions → `reconcile` → `Safety::record_outcome` (Landed/Failed/Timeout 90 s); `pnl_complete=false`, bis Token-Deltas (P8-05) gebucht werden.
- FIX-6 `submit/mod.rs` `SimResult` + `SubmissionAdapter::simulate`; `rpc.rs` `simulate_transaction_with_config` auf derselben signierten TX; `pipeline.rs` Gate vor Submit, `simulated_cu` im Record.
- FIX-7 `pipeline.rs`: Submit-Ablehnung → `release_without_outcome` statt Fehlversuch.
- FIX-11 `fixture.rs`: Allowlist-Check auf finaler Ix-Liste. FIX-12 `HaltReason::LimitBreached`.
Offen (unverändert): P4A-03 Sizing (numerisch), P3-02 Schwellen, Trigger (b)/(c) im Runner, v1 Sign/Wire, Token-Delta-PnL. Nichts kompiliert.

### 2026-10-01 — Sitzung 7 (Astra-Review des FIX-Zips: 7 Blocker + High) — CHECKPOINT
Korrigiert, markiert `[FIX-13]`…`[FIX-26]`:
13 SDK-3.x: `solana_message::AddressLookupTableAccount`, `solana_commitment_config`, `solana_compute_budget_interface`, `solana_system_interface::instruction::transfer`, `Keypair::try_from`; Component-Crates in Cargo (Versionen VERIFY am marginfi-Graph).
14 `SubmissionAdapter::supports_bundle()` (Jito true); Pipeline nutzt Capability, nicht `path()`.
15 `simulate_bundle` (Jito `simulateBundle`, VERIFY Format); Bundle-Kandidaten werden als Bundle simuliert, Einzel-TX einzeln; Unavailable → fail closed.
16 Kapital/Exposure in USD-Micro (`liability_needed_value`, `liability_inventory_value`); Token-Mengen nur fürs Routing; `profit_per_capital_bps` jetzt einheitenkonsistent.
17 JIT: `funding_bank`/`funding_available_value`; Buy-Route = Funding-Bank → Liability über `shortfall_token_amount`.
18 `SubmitAck.signatures` + Bundle-ID; `PendingSubmission` reconciled alle Signaturen, Gebühren summiert; Sentinel = Receivership-TX.
19 `builder.bank()` prüft Owner/Discriminator/Länge (kein Panic).
20 ZERO_PREFUND als v0- und v1-Kandidat (v1 bleibt bis Sign/Wire `V1Unavailable`, geht aber nicht verloren).
21 `Freshness::is_stale` vor Planung.
22 Globale Top-Level-Allowlist in der Pipeline, unabhängig von den Kategorien.
23 `SubmitError::Transport` nach Handoff → `AmbiguousSubmission`, kein Release, Reconciliation über lokale Signaturen.
24 `record_outcome(realized_pnl: Option<i128>)`: None = UNKNOWN → HALT(UnknownEconomics); NOT_LANDED = Some(0) gemessen; gelandet ohne Token-Deltas = None (P8-05 offen).
25 Allowlist semantisch getrennt (swap: JUP/Titan; integration/refresh: Kamino/Drift/JupLend; refresh + Programm für init_liquidation_record).
26 `health::dependency_keys` + main: Vor-Snapshot → Oracle-/Integrations-/Mint-Keys → accountSubscribe + gemeinsamer Snapshot/Reconcile.
Offen: P4A-03, P3-02, v1 Sign/Wire, ZERO_PREFUND Staging, P8-05 Token-Delta-PnL. Nichts kompiliert.

### 2026-10-01 — Sitzung 8 (Astra-Review v2: 8 Punkte) — CHECKPOINT
Korrigiert `[FIX-27]`…`[FIX-34]`:
27 Observation-/Dependency-Keys **exakt wie `p0-cli::bank_observation_keys`** (am Source im Zip verifiziert): `OracleSetup`-Enum, `keys[0..]` direkt, keine Pyth-PDA-Ableitung, keine u8-Magie; `protocol::remaining_layout` entfernt. Astra hatte recht, meine Ableitung aus eva01 war falsch.
28 `reconcile()`: Vergleich gegen `r.slot` des jeweiligen Remote-Accounts statt globalem Lower-Bound; Regressionstest mit gemischten Batch-Slots.
29 `RouteProvider::{quote_exact_in, quote_exact_out}`; JIT nutzt ExactOut mit Liability-Fehlbetrag als Output; `Route.amount_in/out` vom Router.
30 P0-Allowlist nur auf Receivership-TX; Prerequisite-TXs eigene Allowlist (Swap, System, Token, Token-2022, ATA, ComputeBudget).
31 RPC-Sendefehler klassifiziert: RpcError/Serde/Signing/TransactionError → Rejected; Rest → Transport (Ambiguous).
32 `JitoAdapter::new(block_engine, bundle_sim_rpc: Option, timeout)`; `simulateBundle` nur gegen `P0_BUNDLE_SIM_RPC_URL`, sonst Bundle fail closed.
33 Pending: Pfad aus Record (JITO_BUNDLE/JITO_SINGLE/RPC); `getInflightBundleStatuses` beendet `Failed` sofort; on-chain bleibt authoritative.
34 Dynamische Dependency-Subscription: Bank-Update mit neuen externen Keys → HALT(StateGap) → Ingress-Session neu mit erweiterter Subscription → Recovery → READY. `Producer` Clone.
Offen (unverändert): P3-02, P4A-03, v1 Sign/Wire, Staging, P8-05. Nichts kompiliert.

### 2026-10-01 — Sitzung 9 (letzter Pre-Build-Patch) — CHECKPOINT
`[FIX-35]`…`[FIX-39]`:
35 Dependency-Prüfung **nach** `drv.on_event` (neue Bank-Konfiguration); kein 500-ms-Sleep mehr — Bootstrap und Resubscribe laufen über die StreamReady-Barriere.
36 `ws_rpc`: `Reconnected` = StreamReady, nur nach allen Subscription-ACKs (`pending.is_empty()`), genau einmal pro Session; im Fehlerpfad nur OOB-Halt + Gap.
37 `Snapshot.missing` (Key + Batch-Slot) aus `getMultipleAccounts`; `SnapshotData.missing_required` → Recovery nicht sauber, kein READY; Test.
38 `SubmissionAdapter::supports_tx_version`: Default v1=false (Jito-Bundle/BAM unbelegt), RPC true; Pipeline verwirft v1-Kandidaten auf nicht-fähigen Pfaden.
39 Component-Crates auf `=3.0.0` / `solana-system-interface =2.0.0` mit Feature `bincode` gepinnt.
Vorgemerkt (P4A-03): Candidate = FundingClass × TxVersion × SubmissionPath (Tips/Failed-Cost/Latenz pfadabhängig).
Offen: P3-02, P4A-03, v1 Sign/Wire, Staging, P8-05, v1-Bundle-Capability. Nächster Schritt: die vier Logs.

### 2026-10-01 — Sitzung 10 (P6-LT Messstrukturen M1–M3) — CHECKPOINT
- `project_0/docs/PHASE_P6LT_SHADOW.md`: Phase definiert (Gates G1–G5, Messfelder, PASS/ADAPT/STOP).
- [M1] `pipeline.rs`: Shadow baut, signiert und **simuliert** identisch zu LIVE; Reservierung nur in LIVE (`reserved`-Flag, `release_if`); neue Decision `ShadowSimFailed`; `sign_slot` im Record.
- [M2] `runner/winner.rs`: `find_winner` (getSignaturesForAddress → getTransaction json v1 → END_LIQUIDATION-Discriminator, receiver = 4. Account), `classify` (INGRESS/STATE_RACE/BUILD/THRESHOLD_TOO_HIGH/ECONOMIC_SKIP), `is_shadow_win`; Test. `main.rs`: Watchlist, Prüfung nach ≥ 60 s, Aufgabe nach 15 min, Race-Zeile im Record-Log.
- [M3] `runner/shadow_ledger.rs`: Tagesaggregat (Updates, Health, liquidierbar, fail-closed, Pläne, Sim PASS/FAIL, Shadow-Wins, Losses je Grund, Summe erwarteter Netto der Wins, Latenz-Histogramme p50/p95/p99), `pnl_complete=false`; Tageswechsel schreibt `daily_summary.jsonl`; Test.
- Dependencies runner: solana-commitment-config, solana-transaction-status, bs58.
Offen unverändert: Build/Tests, C0, P4A-03 Sizing, Route, v1 Sign/Wire, Staging, P8-05.

### 2026-10-01 — Sitzung 11 (Astra-Review v5: 10 Punkte) + Rebase auf `project_0/`-Struktur — CHECKPOINT
`[FIX-40]`…`[FIX-47]`:
40 `SlotWatch` (AtomicU64, lockfrei) im Ingress-Contract; ws_rpc aktualisiert bei Slot-/Account-Updates; Pipeline liest `sign_slot` exakt nach dem Signieren. Trigger-Slot ist nur Update-Provenienz.
41 Recovery: `b.missing_required` → HALT(ReconcileFailure), kein READY; Regressionstest `snapshot_b_missing_required_blocks_ready`.
42 Timing: `T_detect` beim ersten actionablen Kandidaten **vor** dem TX-Build; `T_ready` nach Build; `T_rx→T_detect` enthält keinen Build mehr.
43 Verlustgrund strukturell: `ThresholdTooHigh` nur, wenn ein Kandidat positiv aber ≤ MIN_PROFIT war; sonst `EconomicSkip`; kein String-Parsing.
44 Winner-Matcher deterministisch: nur erfolgreiche TXs (Signature-err und meta.err), früheste Landing ≥ since_slot, Pagination bis unter since_slot, Deckel 10 Seiten → `Err` (fail closed).
45 `RaceOutcome {ReadyBeforeWinner | SameSlotUnknown | Loss}`: Win nur bei `ShadowReady` ∧ `sign_slot < winner_slot`; gleicher Slot zählt separat; `ShadowSimFailed`/`Skip` nie Win.
46 Ledger: `plans_built` zählt ShadowReady + ShadowSimFailed; `same_slot_unknown`; letzter (Teil-)Tag beim Shutdown geschrieben + flush.
47 `classify`-Kollision in main.rs per Alias `classify_account` behoben.
Repo-Struktur: `project_0/{native,docs,scripts,data}`; Default-Pfade `project_0/data/shadow/*.jsonl`; Phase-Dokument unter `project_0/docs/PHASE_P6LT_SHADOW.md`.
Offen: Build/Tests (vier Logs), C0, P4A-03 Sizing, RouteProvider, v1 Sign/Wire, Staging, P8-05.


### 2026-10-02 — Mac compiler/test gate — PASS

**Environment:** user MacBook Pro, local repo root `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`. Exact `rustc`/`cargo` versions were not captured in the chat and are therefore **N/A**, not inferred.

**Source baseline:** final reported green run after `[FIX-58]`, commit `870aecdf4d63b33ace2e38e50f37c25748d9cfaf`.

**Commands/results reported by the user:**
- `cargo build --manifest-path project_0/native/Cargo.toml --features crate_health` → **PASS**, warning-free after `[FIX-54]`.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health` → **PASS** after test corrections `[FIX-55..57]`.
- `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → **PASS** after `[FIX-58]`.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → **PASS**.
- Per-test-target counts for the final two successful workspace runs were not fully pasted; do not infer them from earlier failed runs.

**Compiler/test fixes closed in this gate:** `[FIX-48]` dependency alignment, `[FIX-49]` Solana component versions, `[FIX-50..53]` Project_0 compile blockers / trait-object support, `[FIX-54]` warning cleanup, `[FIX-55]` planner test construction, `[FIX-56]` histogram `u64::MAX` overflow/bucket bound, `[FIX-57]` stable reconciliation test owner with owner-mismatch regression, `[FIX-58]` canonical v1 message-size API.

**What this PASS proves:** the workspace compiles and its current unit-test suite passes in both required feature combinations on the user's Mac.

**What this PASS does NOT prove:** Mainnet/Staging correctness, route availability, exact Sizing, P3-02 thresholds, v1 signing/wire/submission, landing behavior, token-delta reconciliation, realized PnL, capture rate, or LIVE profitability.

**Gate decision:** compiler/test baseline **PASS**. Next functional closure remains:
`Exact Health → Sizing → RouteProvider → Funding → P0ReceivershipBuilder → Economics → Build → Sign → Simulate`.
`NoPlans` must not be removed until that real path is wired and fail-closed.


### 2026-10-02 — Sitzung 12 (Compiler closure → exact sizing → Jupiter V2 → real runner wiring) — HANDOFF CHECKPOINT

**Purpose of this checkpoint:** this section is the handoff source for the next chat/session. Do not infer completion beyond the explicit PASS statements below.

**Current repository state at documentation time:** `7a55366182c07f1f99951427d5a98d7d8937a76b` on `main` after `[FIX-70]`. Parallel BSC commits were preserved and never reverted.

#### A. What is conclusively complete

1. **Original Fable compiler/unit-test closure is complete for the pre-wiring baseline.**
   - `cargo build --manifest-path project_0/native/Cargo.toml --features crate_health` → PASS.
   - `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health` → PASS.
   - `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → PASS.
   - `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → PASS.
   - This closed the initial compile blockers through `[FIX-58]`.
   - Therefore the old Fable requirement **"compile/test the written Rust instead of leaving it UNTESTED"** was satisfied for that baseline.

2. **Current post-wiring HEAD compiles with both important features enabled.**
   User-reported command:
   `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"`
   → **PASS** on 2026-10-02 after `[FIX-59..69]`; only one unused-import warning remained.
   `[FIX-70]` removed that feature-gated warning after the reported build.
   A fresh build after `[FIX-70]` is still required to record the CURRENT HEAD as warning-free.

3. **Compiler/test fixes already closed:** `[FIX-48..58]`.
   Important verified corrections include Solana dependency alignment, trait-object AccountSource support, reconciliation-test ownership semantics, histogram `u64::MAX` overflow handling, and v1 message-size API.

#### B. Functional work added after the original green baseline

**[FIX-59] Exact protocol receivership sizing**
- New `darkbot-project0/src/sizing.rs`.
- Uses the pinned Marginfi crate/source instead of an independent health formula.
- Enumerates protocol-valid Asset×Liability pairs rather than hardcoding one "largest" pair.
- Uses Marginfi Maintenance vs Equity price semantics and e-mode reconciliation.
- Uses on-chain `FeeState.liquidation_max_fee`.
- Keeps protocol sizing distinct from executable market realizability.

**[FIX-60..64] Correct economics/route boundaries**
- Protocol oracle/equity value is not treated as executable market value.
- Economics monetary fields made explicitly USD-micro.
- Raw lamports were separated from USD-micro economics fields.
- Trigger slot is carried into sizing.
- RouteProvider changed to mint-based routing.
- Executable realizable value derives from route output relative to protocol liability value.
- Jupiter output already incorporates AMM fees/price impact, so those are not double-subtracted.
- ZERO_PREFUND requires route output ≥ actual repay amount.

**[FIX-65] Jupiter Swap API V2 RouteProvider**
- Concrete `JupiterV2RouteProvider` uses `/swap/v2/build`.
- DarkBot retains transaction ownership: blockhash, CU limits, signing, simulation and submission stay in our core.
- Parses raw setup/swap/cleanup/other instructions and ALT address contents.
- Only Marginfi-allowed top-level Jupiter/ATA route instructions are accepted; incompatible routes fail closed.
- ExactOut/JIT remains disabled until a documented/qualified provider path exists.
- Optional API key; implementation does not require a paid subscription.

**[FIX-66..69] Runtime bridge and actual runner wiring**
- Token / Token-2022 IDs and native SOL mint added.
- Protocol-consistent USD valuation method added for native token amounts using pinned Marginfi equity pricing.
- `ProtocolSizingProvider` bridges exact protocol amounts into runtime economics.
- Liquidator ATA balances are read from reconciled RAM state.
- Token and Token-2022 ATA derivations are supported.
- ATA subscriptions are optional; missing ATAs disable candidates rather than blocking recovery.
- Required oracle/mint dependencies remain fail-closed.
- WSOL Marginfi bank is resolved to value SOL-denominated costs.
- Real `P0ReceivershipBuilder` is wired in `p0-longtail` when `crate_health` is enabled.
- `NoPlans` remains only as the compile-time fallback when `crate_health` is disabled.
- ZERO_PREFUND is **not** assumed qualified: default false; requires `P0_ZERO_PREFUND_QUALIFIED=true`.
- JIT remains fail-closed because ExactOut is not yet qualified.
- `FailedCostMatrix::default()` remains Unknown, deliberately preventing fake-zero economics.

**[FIX-70]**
- Removed the feature-gated unused `ReceivershipBuilder`/`ReceivershipPlan` warning from the real `crate_health` build path.

#### C. Current functional path

With `crate_health` the intended Shadow path is now:

```text
WSS update
→ reconciled StateCore
→ exact Marginfi Health
→ ProtocolSizer
→ ATA inventory from RAM state
→ Jupiter V2 executable route
→ P0ReceivershipBuilder
→ deterministic Economics
→ TX Build
→ Sign
→ Simulate
→ Shadow Send-Boundary
```

This is materially beyond the earlier Fable state where `NoPlans` blocked planning.

#### D. What is NOT complete yet

Do **not** call Project_0 finished. The following are still open:

1. **Post-wiring full test rerun.**
   After `[FIX-59..70]`, run both:
   - `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"`
   - `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health`
   The previous green unit-test baseline predates the new sizing/router/runtime wiring.

2. **Fresh warning-free build after [FIX-70].**
   Re-run:
   - `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"`

3. **Failed-attempt cost matrix.**
   Current cells are unknown by design. Economics therefore stays non-actionable until real path/outcome costs are measured or otherwise qualified. Unknown must not become zero.

4. **Actual Shadow/Mainnet qualification.**
   Current compile success does not prove:
   - Jupiter route availability for live Marginfi opportunities,
   - full receivership simulation success,
   - ZERO_PREFUND feasibility,
   - capture/winner performance,
   - landing behavior,
   - realized token deltas/PnL.

5. **v1 execution remains partial.**
   v1 capability/message measurement compiles, but final v1 sign/wire/submission support is still not closed.

6. **P3-02 threshold scheduler remains open.**
   `HealthEngine::thresholds()` is still fail-closed/Unsupported; trigger (b)/(c) work remains separate from trigger (a).

7. **ExactOut/JIT is not qualified.**
   Jupiter V2 provider currently returns `None` for ExactOut, therefore JIT_SHORTFALL is intentionally disabled.

8. **Integration-bank refresh/withdraw completeness.**
   Generic banks are handled; unsupported integration-bank paths remain skipped/fail-closed until their refresh/withdraw instruction planning is fully wired and qualified.

9. **P8 token-delta reconciliation / realized PnL remains open.**
   Landing alone is not sufficient economic evidence.

10. **LIVE remains deliberately disabled.**
    `P0_MODE=live` still refuses startup until P8-LT qualification is recorded.

#### E. Assessment against Fable's earlier requested work

**Compiler/build/unit-test problem from Fable:** substantially **closed** for the original baseline and current code **builds** after the major wiring changes.

**Fable's broader production objective:** **not closed yet**. Compilation was only one gate. The remaining work is now predominantly functional qualification/evidence rather than basic Rust dependency repair.

Current readiness summary:

```text
Dependency/compiler repair             DONE
Pre-wiring workspace unit tests         DONE
Exact Marginfi Health                   DONE for trigger-a path
Protocol Sizing                         IMPLEMENTED + COMPILES
Jupiter V2 RouteProvider                IMPLEMENTED + COMPILES
P0ReceivershipBuilder runtime wiring    IMPLEMENTED + COMPILES
NoPlans in crate_health runtime         REMOVED
Post-wiring workspace tests             DONE
Failed-cost evidence                    OPEN
Shadow simulation on real opportunities OPEN
ZERO_PREFUND qualification              OPEN
v1 sign/wire/submission                 OPEN
P3-02 thresholds / triggers b,c         OPEN
ExactOut/JIT                            OPEN
P8 realized PnL/reconciliation          OPEN
Controlled LIVE                         BLOCKED intentionally
Production profitability                UNPROVEN
```

#### F. Exact next commands

From repository root:

```bash
cd "/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot"

git pull --ff-only

cargo build \
  --manifest-path project_0/native/Cargo.toml \
  --features "crate_health txv1"

cargo test \
  --manifest-path project_0/native/Cargo.toml \
  --workspace \
  --features "crate_health txv1"

cargo test \
  --manifest-path project_0/native/Cargo.toml \
  --workspace \
  --features crate_health
```

If all three are green, update this ledger again and then proceed to the first real Shadow simulation/qualification gate rather than adding more unmeasured architecture.

### 2026-10-02 — Post-wiring compiler/test gate — PASS

**Source line:** `[FIX-59..70]` functional wiring is now compiler/test qualified on the user Mac.

User-reported requested sequence:
- `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → **PASS**.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → **PASS**.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health` → **PASS**.

The visible final terminal line was a zero-test target (`0 passed; 0 failed`), which is normal and does not represent the total workspace test count. No exact aggregate count is recorded because it was not pasted.

**Gate decision:** dependency/compiler/unit-test work requested by the earlier Fable phase is now **CLOSED for the current wired code**, not merely for the old baseline.

**Next gate:** actual Shadow/Mainnet functional qualification:
`real account update → exact health → exact sizing → executable route → receivership build → sign → simulate → record evidence`.

**Important blocker before Shadow can be called qualified:** `FailedCostMatrix` remains unknown by design. Shadow must not convert unknown failed-attempt cost to zero merely to reach simulation. The qualification path must preserve unknown economics while still allowing technical build/sign/simulate evidence, or first populate those cells from real evidence.


### 2026-10-02 — [FIX-71] Technical Shadow Probe path

Problem: `FailedCostMatrix` is intentionally Unknown until real failed-attempt costs are measured. The previous pipeline therefore rejected every Unknown-economics candidate before TX build/sign/simulation, blocking the first real Shadow qualification.

Resolution:
- LIVE behavior is unchanged: only `Evaluation::Known` with `net > min_profit` can ever become actionable.
- Shadow may now admit `Evaluation::Unknown` only as a **technical probe**.
- Unknown economics are never coerced to zero and `net_profit` stays N/A.
- Separate record decisions: `ShadowTechnicalReady` and `ShadowTechnicalSimFailed`.
- Technical probes count toward build/sign/simulation evidence only.
- They are excluded from virtual winner/PnL accounting; only economically known `ShadowReady` may count as a virtual win.
- When no economically actionable candidate exists, technical probes are selected deterministically by lower serialized bytes, then fewer locked accounts; this is not an economic ranking.

Commits: `5ec5df6`, `2a9f536`, `ffa69d0`, `7ad9a87`.

Next gate: compile/tests for [FIX-71], then run a real Mainnet Shadow session and capture the first `ShadowTechnicalReady` or simulation failure with its exact reason.


### 2026-10-02 — [FIX-71] build/test gate — PASS

User-reported current-head verification after [FIX-71]:
- `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → PASS.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → PASS.
- Final visible line was a zero-test target (`0 passed; 0 failed`), not the workspace aggregate.

Gate decision: technical-shadow path compiles/tests. Next step is the first real Mainnet Shadow run.


### 2026-10-02 — [FIX-72] Mainnet Shadow launcher / ZERO_PREFUND probe separation

- Added `P0_ZERO_PREFUND_SHADOW_PROBE`: it may enable ZERO_PREFUND only while `P0_MODE=shadow`.
- `P0_ZERO_PREFUND_QUALIFIED` remains false by default and is not implied by a technical probe.
- Added executable launcher: `project_0/scripts/run_shadow_mainnet.sh`.
- Launcher stores no secrets; RPC/WSS, group and keypair path must come from environment.
- Mainnet Marginfi program defaults to the source-verified program ID.
- Slippage is required explicitly; no guessed trading value is inserted.
- Next roadmap gate before the 72 h P6-LT run remains G2/C0 Mainnet census, followed by G5 bootstrap/reconcile and first technical Shadow simulation.

Commits: `d0f07e5`, `b2d39ba`.


### 2026-10-02 — C0-01 Mainnet census — PASS

Observed on Mainnet via Helius free RPC:

- context_slot: `452723812`
- accounts_total: `172742`
- bytes_total (full account-space sum): `387399978`
- MarginfiAccount: `165597`
- LiquidationRecord: `6330`
- Bank: `437`
- MarginfiGroup: `162`
- FeeState: `1`
- StakedSettings: `1`
- unknown-discriminator rows: `214`
- compact getProgramAccounts duration: `61.843 s`
- raw evidence: `project_0/data/c0/accounts_slot452723812.jsonl`
- report: `project_0/data/c0/c0_census_slot452723812.json`

Unknown-discriminator review against pinned `mrgn-0.1.11-rc1`:
- `31cf1f2243e1a9ba` ×204 = `BankMetadata` (source-confirmed)
- `86addfb94d561c33` ×1 = `Order` (source-confirmed)
- `184662bf3a907b9e` ×9 = still unidentified auxiliary account type; not classified as MarginfiAccount/Bank and therefore ignored by the liquidation core

**C0-01 decision:** PASS. The program/account census is real Mainnet evidence, not fixture data.

**G2 is not yet final GO:** the first scan aggregates all 162 Marginfi groups. `[FIX-75]` adds a second compact scan for the configured group using `MarginfiAccount.group` plus on-chain `IndexerFlags` (`has_liability`, `was_liquidatable`, `was_underwater`, activity flags). That group-specific result is the remaining C0/G2 evidence.

**[FIX-76] runner scope:** Project_0 now filters Bank dependencies and MarginfiAccount triggers to the configured `P0_GROUP_ID`; foreign groups no longer enter the liquidation pipeline.


### 2026-10-02 — C0-02 target-group census / G2 Shadow GO

Second Mainnet census, target group `4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8`:

- overall context_slot: `452727107`
- group_context_slot: `452727389`
- group MarginfiAccounts: `165123`
- has_liability: `82680`
- lending_only: `82443`
- single_borrower: `67556`
- empty: `4662`
- has_ever_been_liquidated: `5114`
- was_liquidatable_last_pulse: `61`
- was_underwater_last_pulse: `60`
- active_30d: `9954`
- active_60d: `9964`
- trivial_balance: `9`
- group scan duration: `48.198 s`

Evidence:
- `project_0/data/c0/group_accounts_slot452727389.jsonl`
- `project_0/data/c0/c0_census_slot452727107.json`

**G2 decision for Shadow qualification: GO.**
The target group has a large liability population and directly observed recent liquidatable/underwater accounts, therefore there is enough real Mainnet activity to justify the Shadow qualification run.

**Not a profitability claim:** C0 does not prove that realizable liquidation edge exceeds all execution/failure costs. That remains for Shadow simulation, winner matching, failed-cost measurement and realized-PnL evidence.

Follow-up hardening:
- `[FIX-77]` adds filtered Program-account snapshots.
- `[FIX-78]` changes bootstrap/recovery from full-program (~387 MB account-space census) to target-group Banks + target-group liability accounts + explicit required/optional dependencies. MarginfiGroup and FeeState are explicitly required so filtering does not silently drop them.
- `[FIX-79]` applies the same target-group liability/Bank filters to WSS `programSubscribe`.
- Lending-only accounts are excluded from the liquidator runtime state because they cannot be liquidated while they have no liabilities.

Next gate: compile/tests for [FIX-77..79], then G5 Mainnet Bootstrap:
`StreamReady → filtered Snapshot → Reconcile → READY → sample exact Health → first ShadowTechnicalReady/ShadowTechnicalSimFailed`.


### 2026-10-02 — [FIX-77..79] filtered bootstrap/WSS build-test gate — PASS

User-reported current-head verification:
- `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → PASS.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → PASS.
- Final visible line was a zero-test target (`0 passed; 0 failed`), not the aggregate workspace count.

Gate decision: filtered target-group/liability bootstrap, recovery and WSS subscription path compile/tests. Proceed to G5 real Mainnet bootstrap evidence.


### 2026-10-02 — Current Project_0 handoff / progress checkpoint

This checkpoint is the current source for continuation work after C0/G2 and before G5 Mainnet runtime qualification.

#### Evidence already closed

- Compiler/dependency closure through the current wired Project_0 path: PASS on the user Mac.
- Exact Marginfi Health for trigger (a): implemented against pinned Marginfi source.
- Exact protocol receivership sizing: implemented.
- Protocol sizing vs realizable route value: separated.
- Jupiter V2 raw-instruction RouteProvider: implemented.
- P0ReceivershipBuilder wired into the crate_health runtime; `NoPlans` is no longer used in that runtime.
- Technical Shadow path with unknown failed-attempt economics: implemented without converting Unknown to zero.
- C0-01 full-program Mainnet census: PASS.
- C0-02 configured-group census: PASS for Shadow activation.
- Filtered target-group/liability bootstrap, recovery and WSS path: build/test PASS.

#### Mainnet C0 evidence

Configured group: `4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8`

```text
group MarginfiAccounts             165123
has_liability                       82680
lending_only                        82443
single_borrower                     67556
has_ever_been_liquidated             5114
was_liquidatable_last_pulse            61
was_underwater_last_pulse              60
active_30d                           9954
active_60d                           9964
```

Decision: **G2 GO for Shadow qualification.**
This proves a real Mainnet liquidation population exists. It does **not** prove sustainable profit.

#### Recent implementation sequence

```text
[FIX-71] Unknown-economics technical Shadow probe path
[FIX-72] Shadow launcher + ZERO_PREFUND shadow-probe separation
[FIX-73] Census RPC/auth hardening
[FIX-74] Compact 8-byte discriminator census
[FIX-75] Configured-group IndexerFlags census
[FIX-76] Runtime trigger/bank scope restricted to P0_GROUP_ID
[FIX-77] Filtered Program-account snapshot primitive
[FIX-78] Bootstrap/recovery = liability accounts + group banks + explicit dependencies
[FIX-79] WSS programSubscribe uses the same liability/group filters
[FIX-80] Mainnet Shadow launcher derives normal Helius RPC/WSS endpoints from HELIUS_API_KEY
```

#### Current G-gate state

```text
G1 compiler/tests                         PASS
G2 C0 Mainnet market evidence             GO for Shadow
G3 exact sizing                           IMPLEMENTED + build/test qualified
G4 executable Jupiter route provider      IMPLEMENTED + build/test qualified
G5 real Mainnet bootstrap/reconcile       NEXT / NOT YET EVIDENCED
```

#### Exact current next gate

Run G5 on Mainnet using the production Shadow code:

```text
StreamReady
→ filtered target-group/liability Snapshot
→ required dependencies present
→ Recovery/Reconcile
→ READY
→ real MarginfiAccount updates
→ exact Health records
→ exact Sizing
→ executable Route
→ P0ReceivershipBuilder
→ Build
→ Sign
→ Simulate
→ ShadowTechnicalReady or ShadowTechnicalSimFailed
```

Do not mark G5 PASS until the real runtime output shows READY and valid Health evidence. Do not mark P6-LT qualified until a real opportunity reaches build/sign/simulation and the record is captured.

#### Still open after G5

- FailedCostMatrix: real failed-attempt cost evidence.
- ZERO_PREFUND: technical Shadow probe is allowed; LIVE qualification is still false.
- v1 final sign/wire/submission support.
- ExactOut/JIT qualification.
- Integration-bank refresh/withdraw completeness.
- P3-02 thresholds and triggers (b)/(c).
- P8 landing/token-delta reconciliation and realized PnL.
- Controlled LIVE remains intentionally disabled.
- Sustainable production profitability remains unproven.

#### Rough engineering position

Implementation completeness is now roughly **70–75%** of the intended technical product, but production qualification is materially lower because G5/P6/P8 evidence is still missing. This is an engineering estimate, not a measured KPI.


### 2026-10-02 — G5 Mainnet bootstrap evidence — PASS

Observed from the real Mainnet Shadow runner:

```text
dependencies: banks=204 required=384 optional_atas=328
bootstrap: waiting for StreamReady (all subscription ACKs)
recovery: Ready, run_state=Ready
```

What this proves:
- Helius WSS connection succeeded.
- All configured subscriptions reached the StreamReady ACK barrier.
- The filtered target-group/liability snapshot completed.
- Required dependency reconciliation completed.
- StateCore transitioned to `Ready` on real Mainnet state.

At the first observed READY window there were no filtered MarginfiAccount/Bank updates yet. Channel counters still advanced because Slot/Root/Health events were flowing, so the stream was alive.

**G5 bootstrap/recovery decision:** PASS for `StreamReady → Snapshot → Reconcile → READY`.

Still required before P6-LT Shadow qualification:
- receive a real target MarginfiAccount update after READY,
- compute exact Health from reconciled state,
- reach exact sizing and executable route,
- build/sign/simulate a real opportunity,
- capture `ShadowTechnicalReady` or `ShadowTechnicalSimFailed` with the exact reason.

[FIX-81] removes misleading repeated `updates: accounts=0 ...` log spam when the program-account update counter remains zero; reporting now occurs once per real 1000 account updates.


### 2026-10-02 — Real provider interruption observed during Shadow — RECOVERY EVIDENCE OPEN

Observed after the first successful Mainnet READY:

```text
blockhash refresh: error sending request for url (...)
[ws_rpc] session ended: ingress connect: idle timeout — reconnect in 500ms
```

Interpretation:
- HTTP RPC and WSS degraded in the same runtime window.
- The WSS idle watchdog correctly declared the stream unhealthy after the configured idle timeout.
- The reconnect path was entered automatically.
- Because no second `recovery: Ready, run_state=Ready` had yet been observed at documentation time, reconnect/recovery is **not** marked PASS yet.

Required evidence to close this item:
```text
reconnect
→ all subscription ACKs
→ filtered Snapshot
→ Reconcile
→ recovery: Ready, run_state=Ready
```

[FIX-82] redacts provider URLs/API keys from blockhash transport errors and emits a single `recovered` message when the blockhash poller becomes healthy again. Runtime fail-closed behavior is unchanged.


### 2026-10-02 — Real provider interruption recovery — PASS

Observed sequence on Mainnet:

```text
recovery: Ready, run_state=Ready
blockhash refresh: error sending request ...
[ws_rpc] session ended: ingress connect: idle timeout — reconnect in 500ms
recovery: Ready, run_state=Ready
```

Decision: **PASS** for real provider interruption recovery.

This proves the production Shadow runtime can:
`READY → detect feed loss → halt trusted state → reconnect → re-subscribe → filtered snapshot → reconcile → READY`.

This closes the open reconnect/recovery evidence item from the prior checkpoint. It does not yet prove a complete liquidation opportunity through Build/Sign/Simulate.


### 2026-10-02 — [FIX-84] Immediate bootstrap Shadow probe — IMPLEMENTED / VERIFY

Purpose: avoid waiting indefinitely for a new account update after G5 READY when the reconciled snapshot already contains accounts whose on-chain `IndexerFlags.was_liquidatable == 1`.

Implementation:
- after the first successful `RecoveryOutcome::Ready` in Shadow mode only,
- scan the already reconciled target-group MarginfiAccounts in RAM,
- select only accounts with `was_liquidatable == 1`,
- queue those account states back through the existing account-trigger path,
- therefore reuse the same exact Health → Sizing → Route → P0ReceivershipBuilder → Build → Sign → Simulate pipeline,
- run the bootstrap probe once per process; later recovery events do not requeue it.

No LIVE behavior is enabled or changed.

Commits:
- `1ef720f` selector
- `ab727b1` one-shot probe queue

Status: IMPLEMENTED, not yet compiler/Mainnet-qualified. Next evidence is build/test PASS followed by `bootstrap_probe: queued=N` and resulting Shadow records.


### 2026-10-02 — [FIX-84] Mainnet bootstrap probe — QUEUED / OUTCOMES OPEN

Observed on Mainnet Shadow after successful StreamReady and two-snapshot reconciliation:

```text
dependencies: banks=204 required=384 optional_atas=328
[ws_rpc] session attempt=1 connected; subscriptions=716
[ws_rpc] session attempt=1 subscription ACKs=716/716
[ws_rpc] session attempt=1 StreamReady
snapshot: done slot=452753636 liability_accounts=82685 banks=204 required=384 optional=0 missing_required=0 total=83273 elapsed_ms=7328
snapshot: done slot=452753663 liability_accounts=82685 banks=204 required=384 optional=0 missing_required=0 total=83273 elapsed_ms=6099
recovery: Ready, run_state=Ready
bootstrap_probe: queued=61 candidates from was_liquidatable
```

Evidence:
- all 716 subscriptions ACKed;
- StreamReady barrier passed;
- snapshot A/B completed with 0 missing required dependencies;
- reconciled state returned READY;
- the one-shot bootstrap probe selected and queued 61 `was_liquidatable` candidates from the reconciled snapshot.

Status: **QUEUED, not yet counted as fully processed.**
Completion evidence requires confirming the resulting per-account records in `project_0/data/shadow/opportunities.jsonl`.


### 2026-10-02 — [FIX-84] Bootstrap probe Health outcomes — PASS (61/61)

Observed from `project_0/data/shadow/opportunities.jsonl` for snapshot slot `452753636`:

```text
probe health records at slot 452753636: 61
liquidatable=true at slot 452753636: 61
```

Decision:
- **61/61 queued bootstrap candidates were processed through Exact Health.**
- **61/61 were currently liquidatable according to the reconciled Mainnet state and current health engine.**
- This closes the bootstrap-probe Health-processing evidence.
- It does **not yet** classify the 61 downstream Pipeline outcomes because OpportunityRecord decision lines do not carry the snapshot `slot`; downstream outcome distribution must be counted separately.

Known sampled outcomes from the same run showed `bad_debt=true`, `equity_assets_bits=0` and therefore no receivership execution candidate. [FIX-85] separates these bankruptcy/no-collateral cases from Routing on the next build.


### 2026-10-02 — Bootstrap probe downstream outcomes — BLOCKED BEFORE BUILD

Observed distribution across all 61 processed Mainnet bootstrap probes from snapshot slot `452753636`:

```text
records: 61

DECISIONS
61 Skip

LOSSES
61 Routing

REASONS
54 no feasible execution candidates
7 oracle adapter: ReserveStale (Kamino reserve refresh required)
```

Interpretation:
- Exact Health reached all 61 candidates and marked 61/61 liquidatable.
- **0/61 reached Build, Sign, or Simulation.**
- 54/61 produced no protocol-executable receivership candidate.
- 7/61 were blocked during exact sizing/oracle access because the Kamino reserve data was stale and a `refresh_reserve` prerequisite is required.
- Sampled 54/61 no-plan cases showed `bad_debt=true` and `equity_assets_bits=0`, i.e. no-collateral bankruptcy cases rather than DEX routing failures.
- The pinned marginfi bankruptcy path is not permissionless for the production group, so those no-collateral bad-debt accounts are not valid receivership opportunities for this Shadow signer.

[FIX-85]/[FIX-86] improve classification:
- `Bankruptcy` for bad debt with zero equity assets,
- `IntegrationRefresh` for `ReserveStale`,
- bootstrap probes carry `bootstrap_snapshot_probe` trigger provenance.

Current P6-LT blocker: integration refresh completeness and obtaining at least one current liquidatable account with executable collateral/liability sizing. No Build/Sign/Sim evidence exists yet.


### 2026-10-03 — Fable audit remediation R0 / R1-A5 checkpoint

R0 compiler baseline after [FIX-87]:

```text
cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"
cargo test  --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"
...
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Decision: **R0 PASS**.

A5 circuit-breaker remediation is now implemented but not yet compiler/Mainnet-qualified:

- [FIX-88a] off-chain receivership CB gate using explicit Client Clock;
- all breaker/oracle math remains in pinned marginfi `BankImpl` / `OraclePriceFeedAdapter`;
- [FIX-88b] dedicated `LossReason::CircuitBreaker`;
- [FIX-88c] shared `HealthEngine::receivership_eligibility` contract;
- [FIX-88d] shared Pipeline invokes eligibility after liquidatable Health and before Builder/Sizing.

Status: **IMPLEMENTED / VERIFY**. Next gate is build/test green before structured planning-rejection diagnostics are introduced.


### 2026-10-03 — R1 implementation complete — VERIFY

R1-A5 build/test was reported green after [FIX-88].

[FIX-89] adds structured planning rejection evidence without relaxing fail-closed behavior:

- `SIZING:NO_ASSET`
- `SIZING:NO_LIABILITY`
- `SIZING:UNSUPPORTED_INTEGRATION_ASSET`
- `SIZING:UNSUPPORTED_INTEGRATION_LIABILITY`
- `SIZING:NO_SUPPORTED_ASSET_BANK`
- `SIZING:NO_SUPPORTED_LIABILITY_BANK`
- `SIZING:NO_VALID_PROTOCOL_PAIR`
- `ROUTING:NO_ROUTE`
- `ROUTING:INSUFFICIENT_ROUTE_OUTPUT`
- `ATA:MISSING_ASSET:<mint>`
- `ATA:MISSING_LIABILITY:<mint>`

Typed loss categories now include `CircuitBreaker`, `Sizing`, `UnsupportedIntegration`, `IntegrationRefresh`, `MissingAta`, `Funding`, and `Routing`.

Important: A1 (NoSwap/Preheld incorrectly coupled to a market quote) is **not fixed by FIX-89**. A `ROUTING:NO_ROUTE` result remains evidence of that known blocker until R3.

Status: **IMPLEMENTED / VERIFY**. Next gate: workspace build/test green, then Mainnet Shadow rerun to obtain the real 61-probe rejection distribution.


### 2026-10-03 — R1 PASS

Workspace build/test after FIX-88 + FIX-89:

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Decision: **R1 PASS (compiler/workspace qualification).**

Included:
- exact shared receivership circuit-breaker eligibility gate before planning;
- typed rejection diagnostics for sizing, integration support, route, ATA, funding, and circuit-breaker failures;
- no relaxation of fail-closed protocol behavior;
- A1 remains intentionally unfixed so the next Mainnet Shadow rerun can measure the true rejection distribution before remediation.

Next gate: Mainnet Shadow rerun and 61-probe outcome census with typed reasons.


### 2026-10-03 — FIX-90 recovery diagnostics / subsequent Mainnet recovery — PASS

A preceding Mainnet Shadow start exhausted all three recovery attempts before completing Snapshot A. The old RecoveryDriver discarded fetch errors, so the exact provider/RPC cause was not observable.

[FIX-90] added:
- redacted per-attempt Snapshot A/B recovery error logging;
- stage labels for `liability_gpa`, `bank_gpa`, `required_accounts`, and `optional_accounts`;
- clean/dirty reconcile logging;
- no change to recovery/trading semantics.

On the immediately subsequent Mainnet run:

```text
[recovery] attempt=1/3 snapshot=A start
snapshot: done slot=452768986 liability_accounts=82686 banks=204 required=384 optional=0 missing_required=0 total=83274 elapsed_ms=6253
[recovery] attempt=1/3 snapshot=A ok slot=452768986 accounts=83274 missing_required=0
[recovery] attempt=1/3 snapshot=B start
snapshot: done slot=452769009 liability_accounts=82686 banks=204 required=384 optional=0 missing_required=0 total=83274 elapsed_ms=6273
[recovery] attempt=1/3 snapshot=B ok slot=452769009 accounts=83274 missing_required=0
[recovery] attempt=1/3 reconcile=clean snapshot_slot=452769009
recovery: Ready, run_state=Ready
bootstrap_probe: queued=61 candidates from was_liquidatable
```

Decision:
- Recovery remains Mainnet-qualified.
- The previous Exhausted event is classified as **transient / exact root cause unobserved**, because it occurred before FIX-90 exposed fetch errors.
- FIX-90 diagnostics are confirmed operational.
- No evidence currently supports changing retry count or recovery semantics.


### 2026-10-03 — R1 Mainnet typed rejection census — PASS

Mainnet Shadow bootstrap probe after R1 diagnostics:

```text
HEALTH RECORDS: 61
LIQUIDATABLE: 61
DECISION RECORDS: 61

LOSSES
55 Sizing
6 IntegrationRefresh

REASONS
53 SIZING:NO_ASSET
6 JuplendLendingStale
2 SIZING:NO_VALID_PROTOCOL_PAIR

DECISIONS
61 Skip
```

Interpretation:
- 53/61 are no-collateral cases at sizing input (`NO_ASSET`); sampled/previous evidence shows these are bad-debt bankruptcy states, not normal receivership opportunities.
- 6/61 are blocked by stale JupLend integration state; this independently confirms the missing integration-refresh path beyond the earlier Kamino evidence.
- 2/61 have both sides present on supported generic banks but still produce no protocol-valid Asset×Liability pair. These are the highest-value candidates for the next generic-path diagnosis because they reach deeper than the 53 no-asset and 6 integration-stale cases.
- 0/61 reached route/build/sign/simulation, so A1 is not yet exercised by this sample.

[FIX-91] changes bankruptcy classification to depend on protocol state (`bad_debt && equity_assets<=0`) rather than the prior planner loss category, so no-collateral cases are not mislabeled as Sizing merely because diagnostics improved.

Status:
- R1 diagnostics: **Mainnet PASS**.
- FIX-91: **IMPLEMENTED / VERIFY**.
- Next diagnostic target: explain the 2 `NO_VALID_PROTOCOL_PAIR` cases at pair-level before touching route/funding.


### 2026-10-03 — FIX-93 closeout sizing parity — IMPLEMENTED / VERIFY

Mainnet evidence for account `CwSsTePXHZUkiZgmKtXwxmEX2uU1ddY5aUjRKkjKvH6d`:

```text
liquidatable=true
bad_debt=true
equity_assets_bits≈51.4M
pair-level rejection: premium_bound=2
```

Protocol threshold:
`LIQUIDATION_CLOSEOUT_DOLLAR_THRESHOLD = 5 USD`, represented in Q48 as `5 << 48`.
The observed equity-assets value is therefore far below the closeout threshold.

Pinned marginfi semantics:
- `end_liquidation` sets `ignore_healthy=true` when pre-liquidation equity assets are below the closeout threshold;
- in closeout, the normal post-health `<= 0` requirement is bypassed;
- in closeout, the `seized <= repaid * max_fee` premium bound is also bypassed.

[FIX-93]:
- removes the normal `health_cap` only when `HealthView::closeout_allowed == true`;
- skips the premium-bound rejection only in that same closeout branch;
- leaves normal-liquidation sizing unchanged;
- retains the existing liability cap conservatively; no new repay-all strategy is introduced.

Important: this does **not** validate Fable C1 globally. C1 remains false for normal liquidations; FIX-93 implements only the explicit closeout exception from pinned protocol source.

Status: **IMPLEMENTED / VERIFY**.


### 2026-10-03 — FIX-94 deterministic SOL cost bank — IMPLEMENTED / VERIFY

Mainnet evidence after FIX-93:
- `CwSs...` no longer fails on the closeout premium bound;
- the next error is `JuplendLendingStale`;
- the generic ProtocolSizer itself excludes integration banks;
- inventory value is zero for the empty Shadow wallet and performs no price lookup;
- therefore the remaining post-sizing integration-price access is the Flat-SOL-fee valuation through `sol_bank`.

Root cause:
- runner selected `sol_bank` as the first Marginfi bank with the WSOL mint;
- multiple same-mint banks may exist, including integration banks;
- cost valuation could therefore accidentally depend on stale JupLend/Kamino state even for an otherwise generic liquidation candidate.

[FIX-94]:
- replaces first-match WSOL selection with deterministic selection among generic supported banks only:
  `ASSET_TAG_DEFAULT | ASSET_TAG_SOL`;
- requires an operational state supported by ProtocolSizer;
- sorts candidates by pubkey before selecting;
- logs selected cost bank, asset tag, oracle setup and operational state at startup.

Status: **IMPLEMENTED / VERIFY**.


### 2026-10-03 — FIX-94 Mainnet PASS / first post-sizing route evidence

Startup selected:

```text
sol_cost_bank: CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh
asset_tag=1
oracle_setup=PythPushOracle
operational_state=Operational
```

This confirms FIX-94 no longer selects an integration WSOL bank for SOL-denominated cost valuation.

Mainnet bootstrap-probe distribution after FIX-93/FIX-94:

```text
HEALTH: 61
DECISIONS: 61

LOSSES
55 Bankruptcy
4 Routing
1 MissingAta
1 Sizing

REASONS
55 bad debt with zero equity assets
2 Jupiter HTTP 429 Too Many Requests
1 ATA:MISSING_ASSET:So11111111111111111111111111111111111111112
1 ROUTING:INSUFFICIENT_ROUTE_OUTPUT
1 SIZING:NO_VALID_PROTOCOL_PAIR ... coeff_nonpositive=2
1 Jupiter HTTP 400 No routes found
```

Important progression:
- FIX-93 removed the false closeout premium-bound rejection.
- FIX-94 removed the accidental JupLend cost-valuation dependency.
- At least one Mainnet candidate now passes Exact Health + Protocol Sizing + route acquisition and reaches the Receivership builder far enough to fail specifically on the liquidator asset ATA.
- Therefore A6 / R2 (Shadow wallet ATA prerequisites) is now an observed blocker, not a hypothetical one.
- Jupiter provider errors are now also observed separately: rate-limit (429), no-route (400), and insufficient route output.

Status:
- FIX-94: **MAINNET PASS**.
- R2/A6: **CONFIRMED BLOCKER**.
- First Build/Sign/Simulation evidence remains OPEN.


### 2026-10-03 — FIX-95 in-TX ATA creation — IMPLEMENTED / VERIFY

Mainnet evidence after FIX-94 exposed a real `ATA:MISSING_ASSET:So111...` blocker after Health + Sizing + route acquisition.

Pinned marginfi introspection was verified:
- before `StartLiquidation`, only explicit refresh/init discriminators are allowed;
- the Associated Token Program is in the allowed top-level program set;
- therefore ATA creation must be **after StartLiquidation** and before the dependent Withdraw/Swap/Repay;
- `EndLiquidation` remains the final instruction.

[FIX-95]:
- adds `post_start_setup` to the shared receivership assembler;
- derives missing liquidator ATAs with the actual mint token program;
- inserts `create_associated_token_account_idempotent` after Start and before Withdraw;
- deduplicates identical asset/liability ATA creation;
- represents the setup as `Step::Other(ATA_PROGRAM)`, already covered by the existing planner allowlist;
- does not create a separate prerequisite transaction.

Economic caveat:
- ATA rent is not yet measured into `rent_and_token_usd_micro`;
- this is acceptable only for current Shadow technical qualification because FailedCostMatrix/economics remain UNKNOWN and LIVE is disabled.

Status: **IMPLEMENTED / VERIFY**.


### 2026-10-03 — FIX-96 recovery catch-up barrier — IMPLEMENTED / VERIFY

Observed failure during long-running Mainnet Shadow:
- WSS reconnect completed and StreamReady re-established;
- Snapshot A/B fetches intermittently failed on `liability_gpa` transport/response decoding;
- successful Snapshot A/B pairs could still reconcile dirty with 10–23 account mismatches and no missing local/remote accounts;
- mismatch pubkeys changed between attempts.

Interpretation:
- RPC snapshots and WSS account subscriptions both use `confirmed`;
- Snapshot B can therefore contain confirmed state whose corresponding WSS account notifications are still in flight / queued locally;
- immediate reconcile after a single drain can compare a newer Snapshot B against a locally lagging WSS-applied state and produce false mismatches.

[FIX-96]:
- RecoveryDriver optionally waits after Snapshot B until the observed WSS root reaches the maximum per-account context slot in Snapshot B;
- ingress is continuously drained while waiting;
- one final drain occurs after the root barrier;
- only then does byte-for-byte reconciliation run;
- production runner enables this barrier with a 30 s timeout;
- timeout remains fail-closed as ReconcileFailure;
- provider transport/decode errors remain normal retry failures.

Status: **IMPLEMENTED / VERIFY**.


### 2026-10-03 — FIX-96 compiler/workspace qualification — PASS

Qualified commit:
`e8b9143a8e5957ce33832929b11e4beceedab5e2` ([FIX-96c], including FIX-96a/b).

Commands executed:

```text
cargo build \
  --manifest-path project_0/native/Cargo.toml \
  --features "crate_health txv1"

cargo test \
  --manifest-path project_0/native/Cargo.toml \
  --workspace \
  --features "crate_health txv1"
```

Observed results:

```text
darkbot-common      19 passed; 0 failed
darkbot-project0    34 passed; 0 failed
darkbot-runner       3 passed; 0 failed
darkbot-solana      30 passed; 0 failed
doc-tests             0 failed
total                86 passed; 0 failed
```

The only compiler warning was the unused test helper `JupiterV2RouteProvider::with_base_url`; it is functionally irrelevant to FIX-96 and does not block qualification.

Decision: **FIX-96 compiler/workspace PASS**.

Remaining FIX-96 evidence is Mainnet-only: observe the production recovery path with the enabled 30 s root catch-up barrier and require

```text
StreamReady
→ Snapshot A/B
→ root_catchup ok
→ reconcile=clean
→ recovery: Ready, run_state=Ready
```

Any `root_catchup timeout`, `reconcile=dirty`, or exhausted recovery keeps FIX-96 Mainnet qualification open/fail-closed.


### 2026-10-03 — FIX-96 Mainnet recovery qualification — PASS

Qualified runtime commit:
`24e50ca` (code under qualification remains `e8b9143`; `24e50ca` adds only the prior compiler/workspace ledger entry).

Mainnet Shadow bootstrap evidence:

```text
[ws_rpc] session attempt=1 StreamReady
[recovery] attempt=1/3 snapshot=A ok slot=453025148 accounts=83266 missing_required=0
[recovery] attempt=1/3 snapshot=B ok slot=453025183 accounts=83266 missing_required=0
[recovery] attempt=1/3 root_catchup start target=453025227 current=Some(453025198)
[recovery] attempt=1/3 root_catchup ok target=453025227 root=453025227 elapsed_ms=7704
[recovery] attempt=1/3 reconcile=clean snapshot_slot=453025183
recovery: Ready, run_state=Ready
bootstrap_probe: queued=61 candidates from was_liquidatable
```

Snapshot details:

```text
Snapshot A: slot=453025148, liability_accounts=82678, banks=204,
required=384, optional=0, missing_required=0, total=83266, elapsed_ms=9530

Snapshot B: slot=453025183, liability_accounts=82678, banks=204,
required=384, optional=0, missing_required=0, total=83266, elapsed_ms=12257
```

Root catch-up details:

```text
target root: 453025227
initial observed root: 453025198
caught-up root: 453025227
elapsed_ms: 7704
result: reconcile=clean
```

Decision: **FIX-96 Mainnet PASS**.

The root catch-up hypothesis is validated for this Mainnet recovery/bootstrap instance: the runner did not reconcile until the observed root reached the fixed Snapshot-B account-context target, the final reconcile was clean, and the system transitioned fail-closed to `Ready` only after that clean reconcile.

FIX-96 is therefore complete: compiler/workspace qualification PASS + Mainnet recovery qualification PASS.

Next qualification target: continue the same Mainnet Shadow path through the queued generic candidates and obtain downstream evidence for `ShadowTechnicalReady` or `ShadowTechnicalSimFailed`, or capture the next concrete Build/Sign/Sim blocker.


### 2026-10-03 — FIX-97 R3 funding/market decoupling — compiler/workspace PASS

Qualified code commit:
`9797cb399603272eb188b58bb196288c9131e91c` — `[FIX-97] decouple inventory funding from market quote`.

Local workspace test execution completed successfully and returned to the shell with the final Cargo test result `ok`.

Qualified behavior in code:
- `NoSwapInventory` and `PreheldInventory` no longer require an ExactIn market quote for technical feasibility/building.
- `ZeroPrefundSameTx` alone requests the collateral→liability ExactIn route it actually executes.
- Unknown seized-collateral realizable value is represented explicitly as `None`; economics evaluates it as `Unknown`, never as zero and never as an oracle substitute.
- Route-provider failure cannot invalidate already-built inventory/JIT candidates.
- Unit coverage includes inventory feasibility without a market route and fail-closed economics for unknown realizable value.

Decision: **R3/FIX-97 compiler/workspace PASS**.

Remaining evidence: Mainnet Shadow must demonstrate that a generic candidate is no longer stopped solely by the old unconditional ExactIn quote and can progress to the next real Build/Sign/Simulation boundary.


### 2026-10-03 — R97 Mainnet census / R5 first Build→Sign→Simulation boundary

Run:
`r97_20261003T193304Z`

Bootstrap/recovery remained clean:
`StreamReady → Snapshot A/B → root_catchup ok → reconcile=clean → Ready`.

The 61 queued `was_liquidatable` bootstrap candidates resolved as:

```text
55  Bankruptcy
 2  StateRace
 2  Routing
 1  Sizing
 1  ShadowTechnicalSimFailed
61  total
```

The first generic candidate to cross the transaction boundary was:

```text
account=Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7
decision=ShadowTechnicalSimFailed
funding=ZERO_PREFUND_SAME_TX
tx_version=V0
serialized_bytes=1224
locked_accounts=35
sign_slot=453028817
simulation=UiTransactionError(AccountNotFound)
logs=[]
```

Interpretation:
- **R5 Build + Sign + call to simulation is now proven on Mainnet Shadow.**
- Simulation success is **not** proven; R5 remains open.
- The current concrete blocker is a Solana account-loading/debit failure before program execution.
- This run does not directly exercise an Inventory-funded candidate, so the R3/FIX-97 Mainnet inventory-path proof remains separate from its already-passed compiler/unit qualification.
- Jupiter route failures on other candidates remain legitimate ZERO_PREFUND-specific evidence and no longer imply that Inventory feasibility is coupled to ExactIn routing.

Next: **FIX-98** — fail fast at startup if the configured liquidator/fee-payer pubkey has no confirmed on-chain account, and print its lamport balance/owner when present. This distinguishes an unfunded/nonexistent payer from a deeper transaction-account problem before another full census.


### 2026-10-03 — FIX-98 payer preflight — Mainnet diagnosis PASS

Qualified code:
`143e554e0d82e6b9e7db810aadcd912f7017ceab` — `[FIX-98] fail fast on missing Shadow fee payer`.

Mainnet Shadow startup result:

```text
PAYER_ACCOUNT_NOT_FOUND:
liquidator=GqRxBgvH94QKFJQVyDv82fK6jxaKrcyjyjSPkN7ue3zd
has no confirmed on-chain account
```

Decision: **FIX-98 diagnostic PASS.**

This explains the prior R97 `UiTransactionError(AccountNotFound)` with empty logs: the configured signer/fee-payer itself did not yet exist as a funded Mainnet account, so the transaction failed during account loading/debit before any Marginfi/Jupiter instruction could execute.

No further code change is required for this blocker. The exact fee-payer pubkey must be funded with enough SOL for transaction fees and any rent-bearing accounts created by the Shadow transaction. Then rerun Mainnet Shadow and continue R5 from simulation.


### 2026-10-03 — R99 funded payer / recovery PASS

Run:
`r99_20261003T202439Z`

Mainnet payer preflight:

```text
pubkey=GqRxBgvH94QKFJQVyDv82fK6jxaKrcyjyjSPkN7ue3zd
lamports=10000000
owner=11111111111111111111111111111111
executable=false
```

Bootstrap/recovery evidence:

```text
StreamReady
Snapshot A slot=453040250 accounts=83268 missing_required=0
Snapshot B slot=453040279 accounts=83268 missing_required=0
root_catchup target=453040308 root=453040308
reconcile=clean
run_state=Ready
bootstrap_probe queued=61
```

Decision:
- **Funded fee-payer preflight PASS.**
- **Recovery/bootstrap remains PASS with the funded signer.**
- The prior R97 `AccountNotFound` payer blocker is removed.
- R5 simulation outcome is still pending from the 61 queued candidates; do not mark R5 simulation PASS until the JSONL decisions show a successful `ShadowTechnicalReady` / `ShadowReady` or a new concrete simulation failure.


### 2026-10-03 — R99 simulation / FIX-100 root cause

R99 candidate `Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7` progressed past account loading and into Marginfi. Simulation failed at instruction 2:

```text
Instruction: MarginfiAccountInitLiqRecord
Allocate: account 8DP6CQt14htc34gTB38hzYzbDKQ7rUBkx96oP99y84v7 already in use
InstructionError(2, Custom(0))
```

Root cause: the builder inferred liquidation-record existence from whether the PDA happened to be present in DarkBot RAM. That PDA was not part of the subscribed state, so an existing on-chain record was incorrectly treated as missing.

Protocol source confirms the correct zero-RPC signal: `MarginfiAccount.liquidation_record` is set to the record PDA when initialized and reset to `Pubkey::default()` when the record is closed.

FIX-100 changes the builder to use that canonical field:
- default link → include InitLiquidationRecord;
- exact derived PDA → do not initialize;
- any other non-default pubkey → fail closed with LIQUIDATION_RECORD_MISMATCH.

This keeps the decision on reconciled MarginfiAccount state and avoids adding a second RPC/source to the hot path.

### 2026-10-03 — CURRENT HANDOFF / Chatwechsel-Checkpoint — after [FIX-105]

**Purpose:** This is the authoritative short resume point when continuing Project_0 in a new chat. Read this section first. Do not infer readiness beyond what is explicitly marked PASS.

#### Where we are

Project_0 is **materially progressing** and is no longer a demo/planning-only path. The production Rust Long-Tail code has run against Mainnet Shadow through real state bootstrap/recovery, exact Marginfi health/sizing, transaction build, signing and an actual RPC simulation call.

What is still not proven: **no complete receivership simulation has succeeded yet, no liquidation transaction has been submitted by the runner, and no realized profit has been proven.**

Roadmap position:

```text
S1-LT / S2 / S3-CORE     substantially implemented and Mainnet exercised
P3 exact health           Mainnet exercised
P4A sizing/economics      implemented; unknown economics stays UNKNOWN
P5 receivership builder   implemented and repeatedly Mainnet exercised
R0/R1/R3                  compiler/workspace gates PASS
R5                        PARTIAL: Build + Sign + simulation call proven; simulation success OPEN
R4                        PASS: remote winner/reconciliation RPC isolated and R108-qualified
R6                        OPEN: integration refresh/withdraw completeness
R7                        OPEN: failed-cost/economics/edge qualification
P6-LT 72 h Shadow         NOT STARTED
P8-LT LIVE                BLOCKED intentionally
first realized net profit NOT PROVEN
```

#### Mainnet evidence already proven

1. **Recovery / state integrity**
   - filtered MarginfiAccount + Bank program streams;
   - subscription ACK barrier reaches StreamReady;
   - Snapshot A/B;
   - root catch-up barrier;
   - byte-for-byte reconcile clean;
   - `run_state=Ready`;
   - bootstrap probe repeatedly queues about 61 `was_liquidatable` candidates.
   - FIX-96 Mainnet PASS.

2. **Real transaction boundary reached**
   - R97 target candidate `Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7` reached Build + Sign + RPC simulation.
   - First failure was `AccountNotFound` before program execution.
   - FIX-98 proved the configured fee payer did not yet exist on Mainnet.
   - Dedicated Project_0 payer `GqRxBgvH94QKFJQVyDv82fK6jxaKrcyjyjSPkN7ue3zd` was funded and verified.

3. **Liquidation-record state bug fixed**
   - R99 progressed into Marginfi and failed because DarkBot tried to initialize an already-existing liquidation-record PDA.
   - FIX-100 now uses canonical `MarginfiAccount.liquidation_record` from reconciled state:
     default → init; exact expected PDA → reuse; other non-default pubkey → fail closed.
   - FIX-101 removed the stale liquidation-record resubscription reference after the clean repository rebuild.

4. **StartLiquidation / ATA ordering bug fixed**
   - R101 reached `StartLiquidation` and failed with Marginfi `StartNotFirst` / error 6086.
   - Root cause: ATA create/idempotent instructions were inside the receivership transaction. The pinned Marginfi validator scans all non-ComputeBudget top-level instructions and rejects the short ATA instruction encoding.
   - FIX-102 removed ATA creation from the receivership TX and requires prewarmed token accounts outside it.
   - ATA setup is no longer accepted as a receivership top-level instruction.

5. **Required token accounts are really present on Mainnet**
   - FIX-103 exposed the exact missing accounts.
   - Target `Trrq...` requires:
     - WSOL ATA `GT3wnkEdqeHtc9zB76GubCmt1m47ybTbpqtWtqGWWuG1`
     - USDC ATA `DoNVGHiCNWy1y5iR4FqV56z8gLbuQAf9nMREU1ydXjR3`
   - FIX-104 added guarded `p0-prewarm-atas`.
   - Dry-run Mainnet simulation PASS: `units_consumed=31330`.
   - Explicit send created both ATAs.
   - Each confirmed with 1,488,440 lamports rent.
   - Post-create payer balance: 7,018,120 lamports.
   - Follow-up: both `PREWARM_PRESENT`; `PREWARM_READY all requested ATAs already exist`.

#### Latest observed blocker

R104, after both ATAs were present, no longer failed with `MissingAta` and no longer showed `StartNotFirst`. The target candidate stopped before Build with:

```text
account=Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7
decision=Skip
loss=Routing
reason=no feasible execution candidates
```

That reason is too generic because the Jupiter adapter had several materially different fail-closed `Ok(None)` paths.

[FIX-105] commit:
`8386c49ab3efa8eacc12df5d804c7ddda311175a`

FIX-105 preserves fail-closed behavior but exposes the exact ZERO_PREFUND route rejection cause:
- local zero/identical-mint rejection;
- HTTP 404 / no executable Jupiter route;
- invalid input/output amounts;
- route incompatible with Marginfi top-level validation, including offending program IDs and instruction data lengths.

**FIX-105 is committed but not yet runtime-qualified.**

#### Exact next action

Build/test current `main`, then run **R105 Mainnet Shadow** and inspect the same `Trrq...` decision.

The next record must identify one concrete routing cause rather than generic `no feasible execution candidates`.

Decision after R105:
- true no-route → ZERO_PREFUND unavailable for that opportunity; continue with other funding classes/opportunities;
- incompatible Jupiter setup instruction → adapt route/prewarm handling without weakening Marginfi validation;
- valid route → continue Build → Sign → Simulate and capture the next exact protocol result;
- `ShadowTechnicalReady` → R5 technical simulation success proven for that candidate, while landing/PnL/LIVE still remain open.

#### Mandatory gate before 72 h Shadow

Even after the first successful simulation, **do not start the 72 h P6-LT qualification until R4 is fixed.**

Current R4 issue: the main ingress/detection loop still performs synchronous remote work for winner/reconciliation paths (`find_winner(...)` and pending submission reconciliation). These must move to bounded worker channels so remote RPC cannot stall the detection/ingress hot path.

After R4, continue R6/R7:
- integration refresh/withdraw planning for Kamino/Drift/JupLend;
- failed-attempt cost matrix from real evidence;
- rounding/repay-all/Jupiter account-size cases;
- threshold triggers b/c;
- 72 h final-code Shadow;
- controlled P8-LT LIVE gates only after all required qualification.

#### Non-claims

- Project_0 has not proven that it earns money.
- No successful Project_0 liquidation landing exists yet.
- No realized PnL exists.
- No p(win)/capture-rate is approved.
- ZERO_PREFUND remains a Shadow technical probe, not a globally qualified LIVE funding class.
- LIVE remains hard-disabled.

Key recent commits:

```text
9797cb3  [FIX-97]  decouple inventory funding from market quote
143e554  [FIX-98]  fail fast on missing Shadow fee payer
7d73ba1/
20ddcac  [FIX-100] canonical liquidation-record existence
87a7151  [FIX-101] remove stale liquidation-record resubscription filter
52423e8..
876fff0  [FIX-102] prewarmed ATA model / remove invalid in-TX ATA setup
6f091e6  [FIX-103] exact ATA prewarm diagnostics
1b233df/
b67f479  [FIX-104] guarded Mainnet ATA prewarm tool
8386c49  [FIX-105] expose exact Jupiter route rejection reason
```

### 2026-10-03 — R105 exact Jupiter ATA blocker / FIX-106 implemented

R105 target:
`Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7`

Observed after WSOL + USDC ATA prewarm:

```text
decision=Skip
loss=Routing
reason=ROUTING:exact-in So11111111111111111111111111111111111111112→EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v:
jupiter route incompatible with receivership top-level validator:
ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL(data_len=1)
```

Interpretation:
- FIX-105 successfully exposed the exact blocker.
- The required WSOL/USDC ATAs are already present in reconciled Mainnet state.
- Jupiter still emits an idempotent Associated Token setup instruction.
- Marginfi rejects that 1-byte top-level instruction inside the receivership transaction.

FIX-106:
`4cbf40945239c76e7988f27e15cbc34a9fc91f01`

Behavior:
- route planning receives the exact prewarmed ATA identities proven by reconciled state;
- Jupiter setup instructions are compared against a locally constructed canonical
  `create_associated_token_account_idempotent` instruction;
- the setup ix is removed only if program, data, account metas, wallet, mint, token program,
  and derived ATA all match an already-proven prewarmed ATA exactly;
- any other ATA instruction remains fail-closed;
- after filtering, any non-Jupiter top-level instruction still rejects the route;
- unit coverage added for exact-match removal and mismatched-taker rejection.

Status: **IMPLEMENTED / VERIFY**.
Next gate: workspace build/test, then R106 Mainnet Shadow. The required evidence is that the previous
`AToken...(data_len=1)` route blocker disappears without reintroducing `StartNotFirst`.

### 2026-10-03 — R106 deeper Mainnet simulation / FIX-107

R106 target:
`Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7`

Observed result:

```text
decision=ShadowTechnicalSimFailed
loss=Build
funding=ZERO_PREFUND_SAME_TX
tx_version=V0
serialized_bytes=1041
locked_accounts=32
```

The transaction now executes successfully through:
`StartLiquidation → LendingAccountWithdraw → Jupiter RouteV2/Swap → LendingAccountRepay`.

Failure occurs only at `EndLiquidation`:

```text
AnchorError: AccountOwnedByWrongProgram (3007)
Left:  rec2HHDDnjLfj4kE7VyEtFA1HPGQLK33259532cRyHp
Right: MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA
```

Source verification against pinned `0dotxyz/marginfi-v2@mrgn-0.1.11-rc1` found the exact mismatch:
- `StartLiquidation` uses complete bank+oracle observation metas with active banks writable;
- `EndLiquidation` uses `HealthPriceMode::Cached` and therefore expects **banks only**, with those bank accounts writable;
- pinned `p0-cli` explicitly builds:
  - `start_metas = load_observation_account_metas_with_bank_writable(..., true)`
  - `end_metas = load_observation_bank_only_metas(..., true)`
- DarkBot incorrectly reused the full bank+oracle observation list for End, so an oracle/receiver account was parsed as a Bank and failed Anchor ownership validation.

FIX-107:
`77b744fe3f21e15333eea64ee1457a37e03620c8`

Changes:
- distinct Start and End observation layouts in `ReceivershipParams`;
- Start gets full bank+oracle metas, with active banks writable;
- End gets active banks only, writable, descending by raw pubkey order;
- fixture updated to the same semantics.

Status: **IMPLEMENTED / VERIFY**.
Next gate: workspace build/test, then R107 Mainnet Shadow. Expected evidence is that EndLiquidation no longer fails with AccountOwnedByWrongProgram.



### 2026-10-03 — R107 full simulation PASS

Run: `r107_20261003T221630Z`

```text
account=Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7
decision=ShadowTechnicalReady
funding=ZERO_PREFUND_SAME_TX
tx_version=V0
serialized_bytes=963
locked_accounts=28
simulated_cu=256993
```

R5 technical simulation gate: **PASS**.

The full Mainnet Shadow path completed through StartLiquidation, withdraw, Jupiter swap, repay and EndLiquidation with a successful simulation.

Still open: failed-attempt economics, landed execution, realized PnL, R4 hot-path isolation, R6/R7 qualification, 72 h Shadow, and LIVE.

### 2026-10-03 — FIX-108 R4 hot-path isolation implemented

Commits:
- `3fa97ad` — remote worker module stub
- `3bc5bea` — bounded remote RPC worker implementation
- `2d2bf88` — expose worker module
- `9584b3b` — wire workers into `p0-longtail`

Architecture after FIX-108:
- `find_winner(...)` no longer executes in the ingress/state/detection loop;
- `try_reconcile(...)` no longer executes in the ingress/state/detection loop;
- winner lookup owns a dedicated `RpcClient` in thread `winner-rpc`;
- submission reconciliation owns a dedicated `RpcClient` in thread `reconcile-rpc`;
- both directions use bounded channels;
- queue-full returns ownership to the state loop for retry; no silent work-item drop;
- worker result queues may block only the worker, never the ingress/detection thread;
- worker disconnect is fail-closed at dispatch.

Static verification on current `main`:
```text
rpc_recon in main.rs:          absent
synchronous find_winner:       absent
synchronous try_reconcile:     absent
RemoteWorkers::start:          present
winner bounded dispatch:       present
reconcile bounded dispatch:    present
result drains on state thread: present
```

Status: **IMPLEMENTED / COMPILE+RUNTIME VERIFY**.

Next gate:
1. workspace build/test with `crate_health txv1`;
2. R108 Mainnet Shadow;
3. require normal StreamReady/Snapshot/Reconcile/READY and a successful technical candidate path without worker disconnect/panic;
4. then mark R4 PASS if runtime shows no regression.

### 2026-10-03 — R108 validates FIX-108 / R4 PASS

Run:
`r108_20261003T223313Z`

Target:
`Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7`

Result:

```text
decision=ShadowTechnicalReady
loss=null
reason=technical shadow probe: failed-attempt economics unknown
funding=ZERO_PREFUND_SAME_TX
tx_version=V0
serialized_bytes=963
locked_accounts=28
simulated_cu=257259
```

Bootstrap/recovery also remained clean:
- StreamReady reached;
- Snapshot A/B complete;
- required dependencies complete;
- two optional prewarmed ATAs present;
- root catch-up PASS;
- reconcile clean;
- run_state=Ready;
- bootstrap probe queued 61 candidates.

FIX-108 runtime result:
- no synchronous winner RPC remains in the ingress/state/detection loop;
- no synchronous submission-reconciliation RPC remains in that loop;
- the full technical receivership path still reaches `ShadowTechnicalReady` after worker isolation.

Gate decision: **R4 PASS**.

Next mandatory work is R6/R7 qualification:
1. integration refresh/withdraw completeness for Kamino/Drift/JupLend;
2. remaining rounding / repay-all / Jupiter account-size cases;
3. failed-attempt cost matrix and deterministic economics thresholds;
4. then 72 h final-code P6-LT Shadow.

### 2026-10-03 — R6 integration completeness started (FIX-109/FIX-110)

Implemented source-grounded R6 foundation:

```text
09ae89e  [FIX-109a] pin integration layout crates for R6
760566d  [FIX-109b] add deterministic integration refresh planner
129b01e  [FIX-109c] expose integration planner
789b1b4  [FIX-109d] subscribe canonical integration state
5cf73af  [FIX-109e] plan account-wide integration refresh prefix
2bf02d5  [FIX-109f] wire integration refresh prefix into receivership
d3e1c31  [FIX-110] add pinned integration withdraw builders
```

Pinned source: `0dotxyz/marginfi-v2@mrgn-0.1.11-rc1`.

Current implementation:
- Project_0 directly pins the canonical Kamino/Drift/JupLend layout/client crates from the same Marginfi tag.
- Integration state accounts `integration_acc_1/2/3` are now explicit snapshot/WSS dependencies.
- Deterministic pre-Start refresh planning exists for:
  - Kamino: `refresh_reserve` + `refresh_obligation`, using the Reserve's own oracle configuration;
  - Drift: `update_spot_market_cumulative_interest`;
  - JupLend: `update_rate`.
- The receivership builder now prepends the account-wide integration refresh prefix before `StartLiquidation`.
- Pinned Marginfi-specific Kamino/Drift/JupLend withdraw instruction builders are implemented.

Still deliberately OPEN:
- exact projected post-refresh state semantics (R6 A4/B2);
- enabling integration banks in ProtocolSizer;
- runner-side account discovery for all optional Drift reward / Kamino farm cases;
- integration-specific Mainnet simulation matrix.

No fake freshness was introduced. Integration sizing remains fail-closed until exact post-refresh projection is qualified.

Status: **R6 PARTIAL / compiler verification required before further wiring**.

### 2026-10-04 — FIX-111 exact post-refresh projection foundation

Compiler checkpoint before this step: workspace build/test returned to prompt without visible failure.

Commits:
- `8a816cb` — [FIX-111a] exact signed simulation can request selected post-execution accounts;
- `eaa7348` — [FIX-111b] read-only `ProjectedState` overlay;
- `d66ad9b` — [FIX-111c] expose projection module;
- `5152993` — [FIX-111d] dedicated bounded `projection-rpc` worker.

Design:
- canonical reconciled RAM state is never mutated by simulation;
- refresh projection uses the exact signed refresh transaction;
- only writable refresh accounts are requested back from `simulateTransaction`;
- returned account bytes overlay the canonical state for candidate-local Health/Sizing;
- projection RPC is isolated in its own bounded worker so R4's no-remote-RPC detection-loop invariant remains intact;
- empty/duplicate/zero-slot projections fail closed.

Still OPEN before R6 can pass:
- wire projection request/result lifecycle into the runner candidate state machine;
- reject stale projection results when the candidate account advanced meanwhile;
- evaluate Health/Sizing on `ProjectedState`;
- enable integration-bank sizing only on a qualified projected state;
- integration-specific Mainnet simulation matrix.

Status: **FIX-111a-d IMPLEMENTED / compiler verification required; R6 remains PARTIAL**.

### 2026-10-04 — CURRENT PROJECT STATUS / authoritative handoff after FIX-111e

Current proven gates:

```text
R4 hot-path isolation                    PASS  (R108 runtime-qualified)
R5 generic Mainnet receivership sim      PASS  (R107/R108 ShadowTechnicalReady)
R6 integration completeness              PARTIAL
R7 edge/economics qualification          OPEN
72 h P6-LT final-code Shadow              NOT STARTED
P8-LT LIVE                               DISABLED
realized PnL / landed liquidation         NOT PROVEN
```

Most important Mainnet evidence already proven:
- R107 and R108 both reached `ShadowTechnicalReady`;
- complete simulated receivership path:
  `StartLiquidation → Withdraw → Jupiter swap → Repay → EndLiquidation → simulation success`;
- R108 result:
  `ZERO_PREFUND_SAME_TX`, V0, 963 bytes, 28 locked accounts, 257259 simulated CU;
- payer, recovery, root catch-up, reconcile, prewarmed ATA and Jupiter/EndLiquidation blockers are cleared for the generic path.

R6 work currently present in `main`:
- FIX-109: pinned Kamino/Drift/JupLend layouts, deterministic refresh planners, canonical integration dependencies, pre-Start refresh prefix;
- FIX-110: pinned integration-specific withdraw builders;
- FIX-111a: signed simulation can request post-execution account state;
- FIX-111b/c: read-only `ProjectedState` overlay exposed;
- FIX-111d: bounded `projection-rpc` worker added so R4's no-remote-RPC detection-loop invariant remains intact;
- FIX-111e: compiler ownership bug fixed by cloning the RPC URL before moving it into the reconcile worker closure.

Latest local compiler result before FIX-111e:

```text
error[E0382]: borrow of moved value: rpc_url
remote_workers.rs
reconcile-rpc closure moved rpc_url before projection-rpc cloned it
```

FIX:
`6eefae63869754d3b1b3a4c22246319b7293eda8`
`[FIX-111e] fix RPC URL ownership across remote workers`

Status of FIX-111e: **IMPLEMENTED, compiler/test verification still required**.

Next exact steps:
1. pull `main`;
2. workspace build/test with `crate_health txv1`;
3. if green, continue FIX-111 runner lifecycle:
   - dispatch refresh projection work;
   - reject stale projection results if the account/state advanced;
   - evaluate candidate Health/Sizing against `ProjectedState`;
   - only then enable integration-bank sizing;
4. run integration-specific Mainnet Shadow matrix for Kamino/Drift/JupLend;
5. only after R6 PASS proceed to R7.

Do not claim:
- integration banks are production-qualified;
- R6 is complete;
- landed executions or realized profit exist;
- LIVE is enabled.

### 2026-10-04 — FIX-111f-k projection lifecycle wired

Compiler checkpoint before this batch: FIX-111e build/test returned to prompt without visible failure.

Commits:
- `5688c00` — [FIX-111f] allow projected `AccountSource` in receivership builder;
- `8197c58` — [FIX-111g] define coherent integration projection targets;
- `7fdcd51` — [FIX-111h] carry candidate identity/hash through projection worker;
- `f763995` — [FIX-111i] add projection lifecycle state to runner;
- `39def2b` — [FIX-111j] requeue successful projections without mutating canonical state;
- `789d8ab` — [FIX-111k] project integration state before candidate evaluation.

Lifecycle now implemented:
1. candidate with no integration refresh continues on the existing path;
2. candidate requiring Kamino/Drift/JupLend refresh is intercepted before Health/Sizing;
3. exact refresh prefix is built and signed from cached blockhash;
4. bounded `projection-rpc` worker simulates it and requests a coherent account set back;
5. returned candidate account bytes must match the exact canonical account hash/slot that requested projection;
6. stale/mismatched projection results are discarded;
7. valid results are one-shot and requeued as `integration_projection` without writing simulated bytes into `StateCore`;
8. `ProjectedState` overlays candidate/banks/observations/integration state for Health/Sizing/builder evaluation;
9. final receivership still contains the refresh prefix, so final Shadow simulation remains the authoritative transaction check.

Safety/non-claims:
- canonical reconciled StateCore is never mutated by simulation;
- projection work remains outside the detection loop behind a bounded worker;
- integration banks are **still not enabled in ProtocolSizer** yet;
- exact venue-specific collateral/share → underlying sizing and withdraw routing remain the next R6 task;
- no integration Mainnet matrix has passed yet.

Status: **FIX-111 lifecycle IMPLEMENTED / compiler verification required; R6 remains PARTIAL**.

### 2026-10-04 — FIX-112 integration amount/withdraw semantics implemented

Compiler checkpoint before this batch: FIX-111f-k build/test returned to prompt without visible failure.

Commits:
- `6373ff8` — [FIX-112a] separate integration bank units from underlying output;
- `e234850` — [FIX-112b] enable projected integration assets in protocol sizing;
- `073355a` — [FIX-112c] discover Drift reward markets from canonical user state;
- `164e756` — [FIX-112d] expose Drift reward market discovery;
- `abf447b` — [FIX-112e] route integration assets through exact withdraw builders.

Semantics now encoded from pinned Marginfi source:
- Kamino:
  - Marginfi bank units = collateral-token units;
  - Kamino withdraw instruction amount = collateral-token units;
  - post-withdraw Jupiter input = projected reserve `collateral_to_liquidity(...)` underlying amount.
- Drift:
  - Marginfi bank units = scaled-balance units;
  - withdraw instruction amount = underlying native token amount derived from projected cumulative deposit interest;
  - one-unit Drift round-up edge is reduced exactly as pinned program semantics require;
  - reward markets are discovered from the canonical Drift user account, capped at the two supported reward positions.
- JupLend:
  - Marginfi bank units = fToken shares;
  - underlying output = floor(shares × token_exchange_price / 1e12);
  - instruction amount = underlying;
  - burned shares are recomputed with ceil semantics and must not exceed the sized fToken units.

ProtocolSizer:
- integration banks are now eligible only on the asset/collateral side;
- liabilities remain generic-only because pinned Kamino/Drift/JupLend configs are non-borrowable;
- this path is intended to run only against the FIX-111 ProjectedState.

Receivership builder:
- selects generic/Kamino/Drift/JupLend withdraw instruction by asset bank tag;
- ZERO_PREFUND Jupiter ExactIn uses actual underlying output, not wrapped/scaled/share units;
- JupLend CPI accounts and Drift reward markets are derived from pinned protocol state;
- Kamino farm accounts are included when the reserve declares a collateral farm.

Status: **FIX-112a-e IMPLEMENTED / compiler verification required**.
R6 remains **PARTIAL** until compiler passes and the integration-specific Mainnet Shadow matrix proves Kamino/Drift/JupLend paths.

### 2026-10-04 — FIX-112f runner integration crate linkage

Local compiler after FIX-112a-e failed with four unresolved-crate errors in
`darkbot-runner/src/builder.rs` for `kamino_mocks`, `drift_mocks`, and
`juplend_mocks`.

Root cause:
- the integration crates were pinned only in `darkbot-project0`;
- FIX-112e also decodes those canonical layouts directly inside `darkbot-runner`;
- transitive dependencies are not directly nameable from the runner crate.

Fix:
- `darkbot-runner/Cargo.toml` now pins the same three crates at
  `0dotxyz/marginfi-v2@mrgn-0.1.11-rc1`;
- `crate_health` enables those optional direct dependencies.

Commit:
- `5685480` — [FIX-112f] link pinned integration layouts into runner.

Status: **FIX-112f IMPLEMENTED / compiler verification required**.
R6 remains PARTIAL until the full FIX-112 batch compiles and the integration Mainnet Shadow matrix passes.

### 2026-10-04 — FIX-113 integration Shadow matrix instrumentation

Compiler checkpoint before this step: FIX-112f build/test returned to prompt without visible failure.

Commits:
- `c376b38` — [FIX-113a] expose integration-kind census;
- `fb31425` — [FIX-113b] emit integration matrix census in shadow.

Purpose:
- identify which bootstrap-liquidatable accounts actually contain Kamino, Drift, and/or JupLend balances;
- attach `integration_kinds` to projection-pending and projection-ready records;
- print bootstrap counts:
  `bootstrap_probe_integrations: kamino=N drift=N juplend=N`.

This turns the R6 Mainnet qualification into an explicit matrix instead of guessing from account addresses.

Required R6 Mainnet evidence per venue:
1. at least one bootstrap/account-update candidate with that integration kind;
2. refresh projection reaches `status=ready`;
3. projected candidate proceeds through Health/Sizing;
4. venue-specific withdraw builder is selected;
5. final exact receivership simulation succeeds (`ShadowTechnicalReady`) or yields a venue-specific blocker that can be fixed deterministically.

Status: **FIX-113 instrumentation IMPLEMENTED / compiler + R109 runtime verification required**.

### 2026-10-04 — R109 integration census exposed a semantic gap; FIX-114 applied

R109 Mainnet Shadow:
- build/release PASS;
- StreamReady PASS;
- Snapshot A/B PASS;
- required dependencies=493, optional ATAs=2, missing_required=0;
- root catch-up PASS;
- reconcile clean;
- bootstrap probe queued 61 candidates;
- emitted:
  `bootstrap_probe_integrations: kamino=0 drift=0 juplend=0`.

Important interpretation:
- the zero counts only covered **asset_tag-based withdraw integrations**;
- this did **not** prove that no candidate depends on Kamino/Drift/JupLend state for pricing/freshness;
- historical Mainnet evidence already contained stale Kamino/JupLend oracle-wrapper failures, so the R109 census was incomplete by construction.

Pinned Marginfi semantics confirm two distinct concepts:
1. **withdraw integration** — encoded by asset tags Kamino/Drift/JupLend;
2. **pricing integration** — encoded by `OracleSetup` variants such as
   `KaminoPythPush`, `FixedKamino`, `DriftPythPull`, `FixedDrift`,
   `JuplendPythPull`, `FixedJuplend`, `KaminoMSOL/LST`, `JuplendMSOL/LST`, etc.

Fixes:
- `738e91b` — [FIX-114a] separate integration withdraw and pricing footprints;
- `9fa55aa` — [FIX-114b] drive refreshes from pricing integration semantics;
- `bcaf303` — [FIX-114c] distinguish withdraw and pricing integrations in shadow census.

Behavior after FIX-114:
- integration-specific withdraw math/builders still use asset-tag semantics;
- refresh/projection selection uses pricing integration semantics first, with asset-tag fallback;
- Shadow bootstrap now prints both:
  `withdraw_kamino/drift/juplend` and
  `pricing_kamino/drift/juplend`;
- projection records carry both `integration_kinds` and
  `pricing_integration_kinds`.

Status:
**R109 recovery/runtime baseline PASS; R6 integration matrix not yet qualified because R109's original census was incomplete. FIX-114 compiler/runtime verification required.**

### 2026-10-04 — R110 integration census

Run:
`r110_20261004T001002Z`

Bootstrap/recovery:
- release build PASS;
- StreamReady PASS;
- Snapshot A/B PASS;
- required dependencies=493;
- optional ATAs=2;
- missing_required=0;
- root catch-up PASS;
- reconcile clean;
- bootstrap probe queued=61.

Integration census:
```text
withdraw_kamino=0
withdraw_drift=0
withdraw_juplend=0
pricing_kamino=0
pricing_drift=0
pricing_juplend=0
```

Interpretation:
- the current 61 bootstrap-liquidatable accounts contain no Kamino/Drift/JupLend withdraw integrations and no Kamino/Drift/JupLend pricing integrations;
- therefore this R110 sample cannot qualify the integration-specific R6 matrix;
- this is not a failure of FIX-114; it means the qualification sample lacks integration candidates.

Next R6 action:
build a targeted Mainnet integration census/qualification path across Marginfi accounts/banks rather than waiting for the 61 bootstrap-liquidatable sample to contain an integration candidate.

### 2026-10-04 — FIX-115 / R111 targeted integration census

FIX-115:
- `88e3f61` — [FIX-115a] targeted integration qualification probe.

R111 targeted Mainnet census:

```text
integration_target_census:
  withdraw_kamino=195
  withdraw_drift=0
  withdraw_juplend=44
  pricing_kamino=195
  pricing_drift=0
  pricing_juplend=44
  scan_errors=0

integration_target_probe:
  queued_unique=16
  kamino_sample=8
  drift_sample=0
  juplend_sample=8
  limit_per_kind=8
```

Interpretation:
- the target-group liability population contains real Kamino and JupLend integration accounts;
- no Drift integration accounts were present in this current target-group liability snapshot;
- Kamino/JupLend withdraw and pricing populations are identical in this snapshot;
- the prior 61-account `was_liquidatable` bootstrap set was simply not representative of integration coverage;
- 16 deterministic qualification candidates were queued through the normal candidate/projection pipeline.

R6 evidence still required:
- inspect the queued R111 records for projection status and downstream decisions;
- at least one Kamino and one JupLend path must reach exact projected Health/Sizing and final receivership simulation;
- Drift cannot be Mainnet-qualified from this target-group snapshot because no Drift candidate exists; this must be documented as unavailable evidence rather than falsely marked PASS.

Status: **R111 census PASS; Kamino/JupLend qualification candidates found; downstream R111 records pending review.**

### 2026-10-04 — R111 downstream review: projection jobs queued, run stopped before results

R111 record review of the 16 targeted integration samples showed:

```text
8 Kamino-targeted samples
8 JupLend-targeted samples
0 Drift samples

all 16:
  decision=Skip
  loss=IntegrationRefresh
  reason="integration projection pending"
```

Some JupLend samples also contain Kamino balances, so the 16 selected accounts are not mutually exclusive by venue.

No `projection.status=ready|failed|stale` records were present in the reviewed output before the run was interrupted.

Interpretation:
- FIX-115 successfully discovered and dispatched all 16 targeted candidates;
- this evidence does **not** yet qualify or fail the projection worker;
- the Mainnet process was stopped before the asynchronous projection worker returned terminal results;
- R6 remains PARTIAL.

Next exact evidence required:
1. rerun the targeted probe;
2. keep it alive after `integration_target_probe: queued_unique=16`;
3. wait for 16 terminal projection result records (`ready|failed|stale`);
4. allow ready projections to re-enter the candidate pipeline and capture their subsequent Health/Sizing/Build/Simulation records.

### 2026-10-04 — R112 projection results / FIX-116

R112 targeted Mainnet projection results:
- 16 targeted integration candidates returned terminal projection records;
- 4 projections reached `status=ready`:
  - 3 Kamino;
  - 1 JupLend;
- the ready projections re-entered the pipeline and exact Health reported `liquidatable=false`, therefore each failed closed with `decision=Skip`, `reason="not liquidatable"`;
- the remaining projection attempts failed at the RPC response layer because the post-account request exceeded the provider's returned-account limit;
- one transport error record also exposed the configured provider URL in the local evidence log. No secret value is repeated here.

Root cause:
- FIX-111g requested a broad coherent post-account set (candidate + banks + observations + integration accounts);
- only writable accounts in the refresh prefix can actually change;
- `ProjectedState` already falls through to canonical RAM for every unchanged readonly account.

Fixes:
- `81de18f` — [FIX-116a] request only mutated post-refresh accounts;
- `d750b1c` — [FIX-116b] validate projection freshness against canonical candidate slot+hash instead of requiring the readonly candidate in the returned post-account set;
- `4ef7f8f` — [SEC-04] redact provider URLs from projection RPC errors.

R6 interpretation:
- Kamino projection path: **runtime proven to projection-ready**;
- JupLend projection path: **runtime proven to projection-ready**;
- downstream exact Health on the sampled ready accounts: **not liquidatable**, so no integration withdraw/final receivership simulation could be exercised from this sample;
- Drift remains unavailable in the current target-group liability population;
- R6 remains PARTIAL until a liquidatable Kamino/JupLend candidate reaches integration-specific sizing/withdraw and final exact receivership simulation.

Next gate:
1. compiler/test FIX-116;
2. rerun targeted probe;
3. confirm provider account-limit failures are gone;
4. then search the broader integration population for currently liquidatable Kamino/JupLend candidates rather than repeatedly probing arbitrary integration accounts.

### 2026-10-04 — FIX-117 targeted projection sweep completion telemetry

Compiler checkpoint before FIX-117: FIX-116 workspace build/test PASS.

Commit:
- `09c468b` — [FIX-117a] report targeted projection sweep completion.

Behavior:
- tracks the exact set of targeted integration probe accounts;
- counts terminal projection outcomes `ready|failed|stale`;
- emits one deterministic completion line only after every queued targeted projection has returned:

```text
integration_target_projection_complete: total=N ready=R failed=F stale=S
```

Purpose:
- removes manual guesswork about when it is safe to stop the targeted Mainnet qualification run;
- preserves the existing bounded projection worker and canonical-state immutability;
- does not alter trading eligibility, LIVE state, or profitability logic.

Next:
- compile/test;
- run targeted probe;
- wait for the completion line before stopping;
- inspect `integration_projection` records to see whether any projected Kamino/JupLend account is currently liquidatable and, if so, whether it reaches venue-specific sizing/withdraw/final simulation.

### 2026-10-04 — FIX-117 compiler checkpoint

Workspace build/test with features `crate_health txv1` completed without a reported failure after FIX-117.

Status:
- FIX-116 compiler/test PASS;
- FIX-117 compiler/test PASS;
- next evidence run is the targeted R113 Mainnet projection sweep, which must remain running until
  `integration_target_projection_complete` is emitted.

### 2026-10-04 — R113 targeted projection sweep PASS

R113 targeted Mainnet probe completed all queued integration projections:

```text
integration_target_probe:
  queued_unique=16
  kamino_sample=8
  drift_sample=0
  juplend_sample=8
  limit_per_kind=8

integration_target_projection_complete:
  total=16
  ready=16
  failed=0
  stale=0
```

Interpretation:
- FIX-116 removed the previous provider post-account-limit failures;
- all 16 targeted Kamino/JupLend candidates completed exact refresh simulation and produced accepted projected state;
- no projection transport/RPC/account-limit failure remained in this sweep;
- no stale candidate projection was accepted;
- Drift remains unavailable in this current target-group liability population.

Gate impact:
- Kamino refresh/projection path: Mainnet runtime PASS for this qualification layer;
- JupLend refresh/projection path: Mainnet runtime PASS for this qualification layer;
- R6 is still PARTIAL until downstream `integration_projection` records show whether any projected candidate is liquidatable and, for a liquidatable integration candidate, the venue-specific sizing/withdraw/final receivership simulation succeeds.

### 2026-10-04 — R113 downstream review exposed JupLend timestamp race; FIX-118

R113 downstream records:
- multiple Kamino projected candidates evaluated cleanly and were not liquidatable;
- multiple JupLend projected candidates still failed Health with
  `JuplendLendingStale` despite projection `status=ready`;
- this proves the refresh transaction itself executed, but downstream Health was evaluating
  the projected JupLend state against a later wall-clock second.

Pinned JupLend freshness semantics:
- `Lending::is_stale(current_timestamp)` is exactly
  `last_update_timestamp < current_timestamp`;
- therefore an `update_rate` simulation at second T becomes stale if Health is evaluated at T+1.

Fixes:
- `b7d8442` — [FIX-118a] request/decode the Clock sysvar from the same projection simulation and return exact simulation slot + unix timestamp;
- `0e921d6` — [FIX-118b] carry simulation unix time through the projection worker;
- `ec73807` — [FIX-118c] evaluate projected Health/Sizing/Pipeline at that exact simulation timestamp instead of later wall-clock time.

Safety properties:
- canonical StateCore remains unchanged;
- candidate slot/hash freshness check remains mandatory;
- only projected candidates use simulation time; ordinary live canonical evaluation keeps wall-clock time;
- no LIVE enablement.

Status: **FIX-118 IMPLEMENTED / compiler + R114 Mainnet verification required**.


### 2026-10-04 — R114 FIX-118 Mainnet verification PASS

- Source commit: `6fbdb30`; `git pull --ff-only` already current.
- Workspace build and tests (`crate_health txv1`): PASS, 96 tests, zero failures.
- Provider: existing Helius Mainnet configuration; launcher enforced shadow and
  `P0_ZERO_PREFUND_QUALIFIED=false`; no transaction submission.
- Bootstrap: 825/825 subscription ACKs, both snapshots complete, zero missing required
  dependencies, root catch-up and reconcile clean, `Ready`.
- Census: 195 Kamino / 0 Drift / 44 JupLend accounts (venue counts can overlap).
- Target: 8 per available venue, 16 unique; completion: ready=16, failed=0, stale=0.
- All 16 downstream projected Health evaluations succeeded; zero `JuplendLendingStale`.
- All 16 were not liquidatable, so no integration-specific withdraw/final simulation
  evidence was produced. Process stopped after completion and downstream records.
- Reproducible sanitized evidence: `evidence/R114_projection_clock.json`.

Decision: **FIX-118 runtime PASS; R6 remains PARTIAL**. R7 remains OPEN. 72 h final-code
Shadow NOT STARTED; LIVE DISABLED. Next: propagate simulation slot consistently with
its timestamp, add freshness regression coverage, then widen the integration sweep.

### 2026-10-04 — FIX-119 exact projection Clock pair / R115 broad sweep interrupted

FIX-119 binds the simulation timestamp to `ProjectedState` and supplies its slot and
unix time to Health, Sizing, Pipeline and priority-fee valuation. Trigger slot stays
in telemetry separately as provenance; evaluation slot/time are explicit.
Pinned source: `marginfi-v2@mrgn-0.1.11-rc1`, Kamino `MinimalReserve::is_stale(slot)`
and JupLend `Lending::is_stale(timestamp)`. Regression tests call those dependency
methods and prove T+1 / slot+1 is stale while the exact simulation pair stays fresh;
negative simulation timestamps fail closed.

Validation:
- workspace build + tests `crate_health txv1`: PASS, 98 tests;
- workspace build + tests `crate_health`: PASS, 97 tests;
- `git diff --check`: PASS.

R115 expanded the existing target sweep to limit=256 per venue:
- 195 Kamino / 44 JupLend / 0 Drift, 230 unique target accounts;
- clean bootstrap/reconcile, zero missing required dependencies;
- 23 ready projections and 21 downstream Health records before a runtime panic;
- one observed liquidatable account was zero-collateral bad debt, not a usable
  receivership candidate;
- main panicked at Health `serde_json::json!` with `number out of range`.

Root cause: Health emits signed i128 fixed-point bits; the default serde_json Number
cannot hold the full range and `json!` unwraps the conversion error. Small target
samples did not exercise the large-value account. R115 is **INCOMPLETE / FAIL**,
not a successful full-population sweep. This is a telemetry serialization defect,
not evidence of a protocol health-calculation failure.

Gate state: R4/R5 PASS; R6 PARTIAL; R7 OPEN; 72 h NOT STARTED; LIVE DISABLED.
Exact next step: FIX-120 lossless i128 JSON encoding + regression test, build/test
both feature sets, repeat the complete 230-account sweep as R116, then inspect
liquidatable integration paths and final simulations. FIX-119 has not yet received
full-population Mainnet qualification because of the independent JSON panic.

### 2026-10-04 — FIX-120 lossless Health JSON / R116 complete broad sweep

FIX-120 enables serde_json `arbitrary_precision` in the runner. Health I80F48 raw
bits remain exact JSON integers across the full i128 range. Regression covers
min/max i128 and both default JSON integer boundaries; no floats or truncation.
Workspace build/test PASS: `crate_health txv1` 99 tests; `crate_health` 98 tests.

R116 Mainnet (source `9e8ea2b` plus FIX-120, same commit as this evidence):
- initial snapshot response decode error recovered on attempt 2; subsequent
  snapshots/root catch-up/reconcile reached Ready with no required state missing;
- 230 unique targets: 195 Kamino, 44 JupLend, 0 Drift (9 overlap);
- terminal projections: 209 ready / 21 failed / 0 stale;
- all 209 projected Health evaluations succeeded; zero JupLend stale errors;
- all 209 evaluation slot/timestamp pairs exactly match their simulation Clock;
- large fixed-point JSON values persisted successfully; no panic;
- 206 healthy; 3 liquidatable. One has zero collateral and is bankruptcy-only.
- Kamino `A45dVtuyLccxs1xpbVJsTNh7AdtAotAZi4UxEcyMQDMS` and JupLend
  `64vknRpPQgoyLDvro5GZtYk39odDtkLQ99Ye71T3vzEk` have nonzero equity assets,
  but both hit `opportunity stale before planning` after the broad queue delay.
- all 21 projection failures were `BlockhashNotFound`: transactions were signed
  at bulk dispatch and expired while waiting for the sequential RPC worker.
- process stopped after all projection results and downstream records arrived.

Evidence: `evidence/R116_integration_sweep.json` (all projection/Health outcomes,
exact Clock-pair check, decisions and raw-record checksum; no endpoint secrets).
Decision: FIX-119/120 runtime PASS for successful evaluations; **R6 PARTIAL**.
The full sweep is not clean PASS because 21 projections failed. R4/R5 remain PASS;
R7 OPEN; 72 h NOT STARTED; LIVE DISABLED.

Exact next step: FIX-121 pace the explicit qualification sweep one projection at a
time, retain unsigned target metadata until dispatch, start each synthetic probe
at its actual dispatch time, immediately evaluate returned projections, preserve
all live/account-update timestamps and freshness guards; build/test, then R117.
The two nonzero-collateral accounts are priority evidence targets, not yet proven
executable or economic. Never extend deadlines or fake a protocol Clock to pass.

### 2026-10-04 — FIX-121 / R117 full-population projection and Health PASS

FIX-121 keeps qualification targets as unsigned identities until the projection
worker is idle. Each synthetic probe starts at actual dispatch; returned projections
are evaluated first. Real ingress timestamps, RPC latency, canonical slot/hash
checks, deadlines and protocol freshness guards remain intact. In-flight/real
opportunity timestamps are never reset.

Build/test PASS: 101 tests (`crate_health txv1`), 100 (`crate_health`). Queue tests
verify busy-worker retention, FIFO and actual probe-start timestamps.
Mainnet R117: clean bootstrap/reconcile, 230 targets, 230 ready / 0 failed / 0 stale.
All 230 Health evaluations succeeded at the exact simulation Clock pair: 227 healthy,
one zero-collateral bankruptcy case, two liquidatable with nonzero collateral.
One additional non-target account-update projection also succeeded (231 total).
No BlockhashNotFound or stale-planning rejection remained in the target sweep.

R6 downstream blockers:
- Kamino `A45dVtuyLccxs1xpbVJsTNh7AdtAotAZi4UxEcyMQDMS` reaches route/receivership
  build feasibility: v0 transaction 1575 > 1232 bytes. The full v1 signing/submission
  adapter remains unavailable, even with the txv1 capability feature.
- JupLend `64vknRpPQgoyLDvro5GZtYk39odDtkLQ99Ye71T3vzEk` needs prewarmed USDT ATA
  `AFA7wz6gLyiYBxeQKPQNc6qr67xjQi6p1Y2pqwBP3uQS`, mint
  `Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB` (legacy token program).

Evidence: `evidence/R117_paced_projection_sweep.json`. Process stopped after full
completion. FIX-121 runtime PASS; R4/R5 PASS; R6 PARTIAL; R7 OPEN;
72 h NOT STARTED; LIVE DISABLED.

Exact next steps:
1. Bounded Jupiter requotes guided by actual compiled bytes/accounts. This planned
   R7 item is now a direct dependency of R6 proof; R6 is not being skipped.
2. Prepare/simulate the USDT ATA prerequisite and measure rent, fee and remaining
   wallet reserve. Creating it spends real SOL on-chain, unlike Shadow; do not
   submit under the present no-LIVE testing scope without explicit approval.
3. Repeat downstream Mainnet qualification; continue recording FIX commits and
   evidence here. Keep technical/economic wins separate and unknown costs UNKNOWN.

### 2026-10-04 — FIX-122 exact ATA prerequisite cost / dry run PASS

Extended the existing guarded `p0-prewarm-atas` utility to simulate the exact signed
message (`sig_verify=true`), request post-create ATA state, validate mint/owner/token
program, sum actual deposited rent, fetch the message fee and read protocol FeeState.
This supports actual Token-2022 sizes instead of guessing a universal rent amount.
Build/test `crate_health txv1`: PASS (101 tests). USDT ATA dry run on Mainnet PASS,
16417 CU; `P0_PREWARM_SEND=NO`, no transaction sent.

Measured for payer `GqRxBgvH94QKFJQVyDv82fK6jxaKrcyjyjSPkN7ue3zd`:
- balance: 7018120 lamports;
- missing USDT ATA: `AFA7wz6gLyiYBxeQKPQNc6qr67xjQi6p1Y2pqwBP3uQS`;
- actual simulated rent (165 bytes): 1488440 lamports;
- message fee: 5000 lamports; total: 1493440 lamports;
- remaining after creation: 5524680 lamports;
- on-chain protocol flat fee: 1000000 lamports (covered by remaining balance).

Explicit approval requested for this one real prerequisite transaction, capped at
1493440 lamports. Await approval; never infer it from elapsed time. LIVE remains
DISABLED. Continue Jupiter compiled-budget work independently. Before any approved
send recheck the same identity, cost cap, dry-run simulation and available balance.

### 2026-10-04 — approved USDT ATA prerequisite CONFIRMED

User explicitly approved only the missing USDT ATA with total ceiling 1493440
lamports. Added a mandatory `P0_PREWARM_MAX_LAMPORTS` check before any real prewarm
send. Re-simulation PASS, 16417 CU; cost unchanged and within approved ceiling.

Confirmed transaction:
`2AAPtxnp6RBsGNikHuEGZJSN6jpFEcFajXV28b97Sbs7gwn6XDnQ775xHkfc3A7xRwXVvEvjgpgwWSyZdcB8n1t2`

Read-only getTransaction verification: slot 453111832, err=null, fee=5000,
payer before=7018120, after=5524680, actual debit=1493440 lamports. ATA confirmed
owned by the legacy token program with 1488440 lamports. No USDT bought/transferred;
no liquidation submitted; LIVE remains DISABLED. The JupLend USDT prerequisite is
now cleared. Evidence: `evidence/USDT_ATA_20261004.json`.

Workspace build/test both feature sets PASS on the current working tree including
pending FIX-123: 105/104 tests. R118 now tests the bounded compiled-size Jupiter
requotes and the newly available JupLend ATA; do not treat R6 as passed before its
integration-specific final simulation results. R6 PARTIAL, R7 OPEN, 72 h NOT STARTED.

### 2026-10-04 — FIX-123 bounded compiled TX budgeting / R118–R119

Source: Jupiter official reduce-transaction-size documentation
https://developers.jup.ag/docs/swap/advanced/reduce-transaction-size
(retrieved 2026-10-04). `maxAccounts` is a route hint, not proof that the final
receivership fits. Reuse existing pinned transaction compiler and route provider.

FIX-123 compiles the complete v0 receivership with identical payer/resources/LUTs
and signature placeholders before admitting a route. On TooLarge/TooManyAccounts,
request a smaller router budget, at most six requests; never relax final limits.
Only the accepted quote supplies economics. Non-budget errors stop retries; existing
inventory plans survive route failure. Failure diagnostics retain both missing-ATA
and routing reasons across distinct pairs. Current v1 builder is unavailable, so
this precheck qualifies v0 only; revisit version selection with the real v1 adapter.

Four regression tests exercise actual compiled byte/account counts, bounded
exhaustion, exact returned quote output and no retry on unrelated compile errors.
Workspace build/tests both features PASS: 105 (`crate_health txv1`) / 104 (`crate_health`).

R118: 231/231 projections ready; 231 downstream Health successes. Initial cap
reductions could repeatedly return the same route; JupLend eventually got HTTP 429.
R119: improved cap reduction also uses observed route account footprint. Again
231/231 projection+Health success. No HTTP 429 in the two target downstream results.
- Kamino SOL→USDC: 1534 bytes with 13 route accounts and one Jupiter LUT; smaller
  cap returns no route. Another collateral pair needs a separate JitoSOL ATA.
- JupLend USDT→SOL: 1820 bytes / 28 route accounts / two LUTs; requote at cap 9
  gives 1559 bytes / 14 route accounts / one LUT. Further reduction: no route.
- No final integration simulation was reached. These are size/routing rejections,
  not economic wins, runtime qualification of v1, or R6 completion.

Evidence: `evidence/R118_route_budget.json`, `evidence/R119_route_budget.json`.
Both runs stopped after target terminal/downstream completion.

Current gates: R4/R5 PASS; R6 PARTIAL; R7 budget implementation tested but runtime
fit still blocked; remaining R7 items OPEN; 72 h NOT STARTED; LIVE DISABLED.
Next exact step: verify canonical v1 serialization/signature and provider simulation
capability with a read-only/no-send probe before connecting a v1 wire adapter. Also
inspect reuse of published protocol lookup tables if v0 remains necessary. Do not
create/fund more accounts or send value-bearing transactions without authorization.


### 2026-10-04 — FIX-124 / R120 Kamino complete receivership simulation PASS

Reuse finding: official P0 TypeScript SDK publishes main-pool general/native-stake
lookup tables in `src/constants/luts.consts.ts`:
https://raw.githubusercontent.com/0dotxyz/p0-ts-sdk/main/src/constants/luts.consts.ts
(retrieved 2026-10-04; the five public addresses are pinned in `lookup_tables.rs`).
The builder previously supplied only Jupiter route LUTs. Reuse these existing
on-chain protocol tables before expanding the v1 adapter; no new LUT/rent/send.

FIX-124 adds the five group-specific tables to optional Snapshot/WSS dependencies.
Decode using pinned official `solana-address-lookup-table-interface=3.1.0`, from
canonical/projected RAM state only. Validate owner/layout, exclude deactivating and
future-state tables, expose only the prior prefix in an extension slot. Merge with
route tables by key, preserving state-backed indices, both in budgeting and final
v0 plans. Absent/invalid tables fall back to static keys subject to unchanged limits.
Three regressions cover slot activation, invalid/deactivating state, duplicate-key
indices and measured compiled-byte reduction. Workspace build PASS; tests PASS:
108 (`crate_health txv1`), 107 (`crate_health`), zero failures.

R120 Mainnet Shadow:
- five active protocol LUTs, 958 entries observed from state;
- 231/231 target projections ready; failed=0, stale=0;
- 231/231 downstream Health successes at their exact simulation slot+timestamp;
- Kamino account `A45dVtuyLccxs1xpbVJsTNh7AdtAotAZi4UxEcyMQDMS`:
  full refresh → Start → Kamino withdraw → Jupiter → repay → End simulation PASS;
  ZERO_PREFUND_SAME_TX, V0, 1064 bytes, 50 locked accounts, 527915 CU,
  `ShadowTechnicalReady`. Economics remains UNKNOWN, not an economic shadow win.
- JupLend `64vknRpPQgoyLDvro5GZtYk39odDtkLQ99Ye71T3vzEk`: liquidatable at this
  snapshot but no admitted size (`coeff_nonpositive=1; withdraw_zero=1`), unlike
  prior R119 size/routing failure. No final JupLend simulation claimed.
- Drift candidate census remains zero. No transactions sent, process stopped
  after all target projections and decisions completed.

Evidence: `evidence/R120_protocol_luts.json` (all target projections/decisions and
liquidatable Health). R4/R5 PASS; R6 PARTIAL with Kamino end-to-end now PASS;
JupLend/Drift still unqualified; R7 OPEN; 72 h NOT STARTED; LIVE DISABLED.
Next: inspect JupLend exact sizing against pinned protocol semantics and rerun when
an admissible size exists; continue R7 rounding/repay_all and wire qualification.
Do not relax sizing or freshness merely to obtain a PASS.


### 2026-10-04 — FIX-125 / R121 sizing units and pair diagnostics

Pinned `type-crate/src/types/bank.rs::get_balance_decimals` uses scaled balance
units for Drift, not mint decimals. The inverse USD→native amount now uses the
same balance decimals as the forward valuation. A roundtrip regression proves
Drift's previous 1000x mismatch and unchanged JupLend conversion. No Drift Mainnet
execution proof is inferred from this unit test. Workspace tests `crate_health txv1`
PASS: 109, zero failures; R121 release build PASS.

R121: 231/231 projections ready, zero failed/stale, 231 downstream Health successes.
Evidence: `evidence/R121_sizing_pairs.json`. New bounded pair diagnostics explain
JupLend: one liability is dust (withdraw would be <1 native unit); the SOL pair's
health coefficient is negative at the assumed maximum premium. The protocol
`liquidate_end.rs` enforces a premium CEILING, not a minimum actual premium. The
sizer always uses the ceiling, so a lower-premium healing pair may exist. Next:
verify/derive a bounded lower-premium candidate and test rounded health parity.
No threshold/freshness relaxation. R6 PARTIAL; R7 OPEN; 72 h NOT STARTED; LIVE
DISABLED. R121 stopped after terminal target results; zero sends.


### 2026-10-04 — FIX-126 canonical v1 wire / Mainnet RPC simulation PASS

Sources: pinned `solana-message=4.4.1` `v1::Message::serialize`, SIMD-0385
https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0385-transaction-v1.md
and https://solana.com/upgrades/larger-transaction-sizes (retrieved 2026-10-04).
Use official wincode serializer, version-prefixed message followed by signatures.
The old capability measurement omitted the one-byte version prefix; now corrected
and checked against actual signed bytes. Compatible `solana-short-vec=3.2.2` pin is
required by the canonical serializer while preserving the protocol SDK-3 graph.

Added explicit `p0-probe-v1` binary with no sendTransaction path. Probe signs a
zero-lamport self-transfer and a >1232-byte variant with a Memo; `sigVerify=true`,
no blockhash replacement. RPC results: V1 204 bytes PASS (150 CU); V0 control PASS;
V1 1440 bytes PASS (422318 CU), err=null. This proves configured primary RPC accepts
canonical v1 simulation, including beyond the legacy size cap. Simulated fee 5000
is not a real debit; transactions sent=0. Evidence:
`evidence/V1_wire_probe_20261004.json`.

Regression verifies signature over the prefixed message, tamper rejection, wrong
signer rejection and exact byte-limit boundary. Workspace build PASS; tests with
pending FIX-127 sizing changes PASS (112 with txv1 / 110 without). Production
`build_v1` still explicitly unavailable until the typed wire adapter is connected;
Jito, submission/landing/reconciliation and real integration v1 simulation remain
OPEN. No claim that all R7 wire qualification is complete. R6 PARTIAL; R7 OPEN;
72 h NOT STARTED; LIVE DISABLED.


### 2026-10-04 — FIX-127 / R122 JupLend complete receivership simulation PASS

Pinned `liquidate_end.rs::end_liquidation/end_receivership` caps premium and rejects
worsened health; it does not require the maximum premium. FIX-127 preserves the
maximum when its coefficient heals the account; otherwise reduces the factor below
the health-neutral ratio using existing sizing headroom. Never selects a negative
premium. Rechecks weighted health after native-unit floor/ceil rounding; normal
liquidations must stay <=0, closeout retains its source-backed exception. This is
not a relaxation of freshness, premium ceiling or post-health conditions.

Tests reproduce R121's exact fixed-point pair ratios, unchanged feasible maximum,
unhealable pairs and rounded normal/closeout boundaries. Workspace tests PASS:
112 with txv1 / 110 without; R122 release build PASS.

R122: 231/231 projections ready, zero failed/stale, 231 downstream Health successes.
- JupLend `64vknRpPQgoyLDvro5GZtYk39odDtkLQ99Ye71T3vzEk`: projection → Health →
  Sizing → JupLend withdraw → Jupiter → repay → EndLiquidation simulation PASS.
  ZERO_PREFUND_SAME_TX, V0, 1227 bytes, 47 locked accounts, 432536 CU, premium 985 bps.
- Kamino `A45dVtuyLccxs1xpbVJsTNh7AdtAotAZi4UxEcyMQDMS` repeats full simulation
  PASS: V0, 964 bytes, 46 accounts, 556666 CU.
- Both `ShadowTechnicalReady`, economics UNKNOWN, no economic win claimed.
- Drift remains zero candidates in this group. R6 integration matrix therefore
  Kamino PASS / JupLend PASS / Drift UNPROVEN. R6 stays PARTIAL.

Evidence: `evidence/R122_juplend_complete.json`. Controlled stop after full sweep;
transactions sent=0. Next R7: typed v1 execution adapter and provider qualification,
repay_all edge cases, FailedCostMatrix and P3-02 thresholds. Drift requires a real
candidate or an explicitly documented external qualification blocker, never a fake
PASS. 72 h NOT STARTED; LIVE DISABLED.

### 2026-10-04 — FIX-128 repay_all / rounding parity implemented

R7/C2 source review used pinned `marginfi-v2@mrgn-0.1.11-rc1`.

Pinned semantics:
- normal repay uses the integer SPL amount directly and is RepayOnly;
- `repay_all` closes the liability balance and transfers `ceil(current_liability_amount)`;
- `repay_all` is valid only when the liability is positive above
  `ZERO_AMOUNT_THRESHOLD` and opposite-side asset dust satisfies
  `abs(asset_amount) < ZERO_AMOUNT_THRESHOLD`;
- rounding excess and allowed opposite-side asset dust are credited to insurance;
- in receivership, closing the balance clears the liquidation-price-cache lock;
- EndLiquidation measures exact pre/post equity deltas, so the protocol-repaid value is
  the exact liability removed, while economic repayment cost is the actual ceil SPL transfer;
- pinned receivership tests exclude a bank closed by `repay_all` from EndLiquidation's
  bank-only remaining-account set.

Commits:
- `9996bb2` — [FIX-128a] model source-exact near-full repay_all rounding;
- `1eff18f` — [FIX-128b] mirror repay_all instruction account semantics;
- `c8eae48` — [FIX-128c] propagate repay_all through execution and economics;
- `a217ddc` — [FIX-128d] keep repay_all regression independent of generated constants.

Behavior:
- no silent clamp when ceil(target) exceeds liability;
- near-full target may use `repay_all` only when it maps exactly to the source-backed
  full-close transfer amount;
- protocol health/premium uses exact liability delta and accounts for opposite asset dust;
- economics/funding uses the actual transferred SPL value, including ceil overpayment;
- Token-2022 repay includes the mint account as required by the pinned repay instruction;
- `repay_all` carries full pre-close observation accounts;
- Start observes the pre-close bank, while End omits a liability bank actually closed by
  `repay_all`;
- partial repay behavior remains unchanged.

New regressions cover:
- near-full repay_all selection;
- refusal to silently clamp an over-large target;
- strict opposite-dust threshold boundary;
- unchanged partial repay;
- repay_all observation metas;
- Token-2022 mint ordering;
- End bank-set removal only for repay_all.

Status: **FIX-128 IMPLEMENTED / workspace compiler+tests and Mainnet Shadow verification required.**
R6 remains PARTIAL due Drift evidence gap. R7 remains OPEN until FIX-128 is runtime-qualified and FailedCostMatrix / thresholds / v1 execution qualification are closed.



### 2026-10-04 — FIX-128f / FIX-130 / FIX-131 0.1.12 PRE hardening

Source review:
- pinned `marginfi-v2@mrgn-0.1.11-rc1` `p0-cli/src/processor/account.rs` and
  `test-utils/src/marginfi_account.rs` for normal withdraw remaining-account order;
- `marginfi-v2` 0.1.12 branch / patch notes for Bank resize and post-upgrade semantics;
- Solana upgradeable-loader source/docs for ProgramData PDA derivation and upgrade lifecycle.

Implemented:
- **FIX-128f**: normal Token-2022 `LendingAccountWithdraw` now receives the Bank mint
  and inserts it as the first remaining account before health observations, matching pinned
  p0-cli/test-utils. Classic SPL Token withdraw does not append the mint. Runner passes
  `bank.mint` into the builder. Added regressions for Token-2022 ordering and unchanged
  classic behavior.
- **FIX-130**: Bank GPA/WSS filters no longer use exact `dataSize`. They filter by the
  Bank discriminator plus group offset, allowing both the current 1856-byte payload and
  the announced 3904-byte payload through ingress. Exact supported layout lengths remain
  a decode/classification concern; MarginfiAccount keeps its exact-size filter.
- **FIX-131**: derive the Loader-v3 ProgramData PDA from the configured Marginfi program,
  fetch and hash it at startup, include it in required snapshot/WSS dependencies, compare
  every ProgramData update and every recovered canonical snapshot to the startup baseline,
  and latch `HaltReason::ProgramUpgrade` on mismatch. The latch is not cleared by normal
  reconnect/recovery and therefore requires explicit P1B promotion before execution can resume.

Commits:
- `728ee809` — FIX-128f withdraw builder + regressions;
- `8254dfb7` — pass Bank mint from runner builder;
- `7d0f1c7e` — FIX-130 dual-size-compatible Bank ingress filter;
- `b05fd3b1` — FIX-131 ProgramData upgrade watcher with latched halt.

Important boundary:
- this is **PRE-hardening only**. Project_0 remains pinned to `mrgn-0.1.11-rc1`.
- 0.1.12 changes more than Bank size: variable-borrow premium changes debt, health and
  `repay_all` economics; new Scope integration oracle variants also require source-exact
  promotion. None of those semantics are claimed implemented by FIX-130/131.
- do not switch the dependency pin or claim 0.1.12 compatibility until P1B source parity,
  compile/tests and Mainnet Shadow evidence are complete.

Status: **IMPLEMENTED IN SOURCE / NOT YET COMPILED OR TESTED after these commits.**
LIVE remains hard-disabled. 72 h NOT STARTED. Next: compile/test this checkpoint, then
P1B 0.1.12 source-parity promotion before FIX-129 economics closure / long-run qualification.

### 2026-10-05 — FIX-129a reconciliation evidence, deployed 0.1.11 main

User-directed scope: resume deployed `mrgn-0.1.11-rc1` main, leave
`p1b-marginfi-0.1.12` unchanged. Remote P1B ledger at `b47797f` records R123 STOP /
VERSION MISMATCH (1856-byte deployed Bank payload vs 3904-byte compiled layout).
No coercion, no P1B runtime retry, no P3-02/Oracle/72h work in FIX-129.

Audit: FailedCostMatrix already has the required failure dimensions; retain them.
1814 existing local Shadow rows contain no actual network/tip fee observations.
The confirmed USDT prewarm transaction proves its own meta.fee=5000 and rent
1488440 only; neither is a universal liquidation fee/rent constant. Winner matching
observes competitors; their submission path/tip attribution is not ours.

FIX-129a removes false zero-PnL conclusions from elapsed timeout and Jito inflight
Failed. Reconcile every distinct submitted signature with history search and actual
getTransaction meta.fee. Missing lifecycle/metadata/member evidence stays UNKNOWN;
partial bundles cannot be treated as all-or-nothing zero-cost. Carry our explicit
signed tip-transfer signature/amount; charge it only when that transaction landed
successfully, zero transfer on its confirmed revert. Network fee and tip remain
separate; absent Jito intent/evidence stays UNKNOWN. Ambiguous submissions retain
actual path. Complete realized PnL still requires token/protocol/rent deltas.

Sources (retrieved 2026-10-05):
https://solana.com/docs/core/fees
https://solana.com/docs/rpc/http/getfeeformessage
https://docs.jito.wtf/lowlatencytxnsend/ (including uncled-block rebroadcast caveat).

Baseline: 123 tests PASS with txv1. FIX-129a: 126 PASS with txv1 / 124 without,
zero failures. Regressions: landed success/revert meta.fee, missing metadata and
not-landed UNKNOWN, tip payment tied to our successful transaction, partial bundles.
No send. Next FIX-129b: exact-message prospective fee measurement and candidate
Economics wiring, with evidence/provenance and no guessed matrix population.
FIX-129 remains IN PROGRESS. LIVE DISABLED; 72 h NOT STARTED.

### 2026-10-05 — FIX-129b / R124 exact-message RPC economics PASS

Branch: deployed 0.1.11 `main`; P1B branch untouched. Detailed qualification matrix
and sources: `FIX_129_COST_QUALIFICATION.md`. Existing FailedCostMatrix dimensions
retained. Actual landing reconciliation and prospective scenario pricing are
separate: missing/expired status still does NOT prove a zero-cost attempt.

Implementation:
- RPC adapter obtains getFeeForMessage for the exact compiled message, retaining
  message hash/context slot/lamports. No fixed 5000×signature calculation.
- CandidateInput explicitly requires total transaction fee (base + requested
  priority) and incremental rent evidence. Total fee replaces priority-only cost;
  it is not added on top of an already total fee. Cost conversion rounds upwards;
  missing conversion stays None instead of a sentinel/zero.
- For a measured single RPC message with no prerequisite/tip: source-backed
  prospective not-landed network charge=0 and landed-revert charge=message fee.
  Existing conservative policy reserves ONE ADDITIONAL failed attempt. This is
  a strategy reserve, not measured PnL or double charging one transaction.
- No RPC sample populates Jito cells. A missing exact fee/rent/market value remains
  UNKNOWN. Jupiter output already includes route cost/impact; no invented second
  deduction. ATA prewarm rent is not recharged. New LiquidationRecord rent remains
  UNKNOWN pending exact incremental measurement.
- Per-candidate cost evidence remains visible on negative EconomicSkip records;
  top-level economics flags are also aligned with the best known rejected result.

Evidence:
`evidence/FIX129_fee_sources.json`: no-send signed simulations and finalized reads.
No priority: quote/simulation 5000 lamports. Requested 1,400,000 CU at 2000
micro-lamports/CU: quote/simulation 7800 = 5000 + 2800 priority despite only 600 CU
consumed. A simulated execution revert has the same measured fee, but is NOT a
landed-revert claim. Historical authorized USDT ATA meta.fee=5000; third-party
finalized revert meta.fee=11001, actual err present. Its submission path is UNKNOWN
and its expense is not ours. Replay regressions use these actual metadata values.

`evidence/R124_rpc_economics.json`: 229/229 target projections ready, 0 failed/stale,
229 downstream Health successes. Three actual candidate messages quote 5000 each;
converted network cost=604 USD-micro and separate failure reserve=604. Net values:
- generic Trrq...: -121889 USD-micro;
- Kamino A45d...: -118606 USD-micro;
- JupLend 64vk...: -91691 USD-micro.
All correctly EconomicSkip; no liquidation signed/simulated after economic rejection,
no send, no economic win. R124 stopped after all target results. The run predates
final top-level skipped-record metadata alignment; per-candidate evidence is complete.

Final requested workspace commands PASS:
- `cargo test --workspace --features crate_health,txv1`: 129 passed, 0 failed;
- `cargo test --workspace --features crate_health`: 127 passed, 0 failed.
Workspace build and diff whitespace check PASS. Regressions cover actual landed
success/revert fee preservation; absence/missing metadata UNKNOWN; exact RPC
scenario pricing; missing network/rent evidence UNKNOWN; requested-CU priority;
our matching landed tip vs revert/partial bundle; historical metadata replay.

Precise claim: FIX-129 implementation and current RPC candidate-cost qualification
PASS. Full provider/outcome empirical qualification is NOT claimed: Jito lifecycle,
actual paid Jito tip and our landed-revert sample remain UNKNOWN. Complete realized
liquidation PnL still requires token/protocol/rent deltas. R7 overall remains OPEN.
R6 retains Kamino/JupLend PASS and Drift UNPROVEN. LIVE DISABLED; 72 h NOT STARTED.

Next within FIX-129: obtain concrete Jito bundle lifecycle + all constituent
transaction metadata before qualifying its cells. Do not manufacture zero costs,
assign competitor paths, or send fee-bearing trials without explicit authorization.
No P3-02, Oracle-trigger, 72h or P1B development was performed in this work.


### 2026-10-05 — FIX-133a/b Oracle-trigger foundation (pre-threshold)

Baseline: `main@56e9d47` after FIX-129 RPC economics qualification.

Implemented without changing candidate admission/detection semantics:
- **FIX-133a**: instantiate and maintain the existing `ReverseIndex` in the runner.
  On Recovery READY it is rebuilt deterministically from canonical RAM state:
  Bank → official observation/dependency keys and MarginfiAccount → active Banks.
  Subsequent canonical Bank/Account updates replace only their own reverse-index entries.
  Unsupported oracle layouts are removed from the trigger map fail-closed and reported;
  no guessed dependency mapping is inserted.
- **FIX-133b foundation**: `ThresholdIndex` price keys changed from `u64` to `u128`
  positive raw I80F48 bits. The index was not yet wired into detection, so this is a
  precision/type preparation only and does not create Oracle-trigger candidates.

Commits:
- `2c1d08e2` — FIX-133a canonical Oracle/Bank/Account reverse index.
- `b914e6fc` — FIX-133b preserve exact I80F48 price bits in threshold index.

Important boundary:
- Oracle updates do **not** yet enqueue accounts.
- `CrateHealth::thresholds()` remains fail-closed/Unsupported.
- no Full Scan per Oracle update was introduced.
- FIX-129 Jito cells remain UNKNOWN; no fee-bearing Jito trial was sent.
- LIVE remains disabled; 72 h has not started.

Next gate: workspace compile/tests for this structural checkpoint. After PASS, implement
source-backed threshold derivation and only then enable Oracle→ThresholdIndex→exact Health
candidate wakeups. No `health_cheap` approximation and no guessed thresholds.


### 2026-10-05 — FIX-133c/d source-backed Oracle-trigger implementation

Precondition:
- FIX-133a/b structural checkpoint passed both workspace test feature sets locally.
- FIX-133c threshold derivation and repeated-Bank aggregation subsequently passed both
  workspace feature sets locally; only the pre-existing runner `with_base_url` dead-code
  warning remained.

Implemented after that gate:
- `0ab82511`: ThresholdIndex consumes the two distinct Marginfi Maintenance risk-price
  axes. Collateral compares against confidence-biased `Low`; liabilities compare against
  confidence-biased `High`. They are never collapsed into one price.
- `c322a272`: Oracle/dependency wakeups are active in the runner.
  - Recovery rebuilds ReverseIndex, clears stale threshold state and creates a bounded,
    incremental threshold-bootstrap queue containing liability-bearing MarginfiAccounts.
  - `P0_THRESHOLD_BOOTSTRAP_BATCH` defaults to 16 and keeps the WSS/state loop responsive;
    zero is rejected.
  - Account updates replace their ReverseIndex entry and immediately recompute that account's
    source-backed thresholds.
  - Bank configuration updates invalidate/requeue only accounts using that Bank.
  - Updates to subscribed pricing dependencies map through
    dependency -> Bank -> ThresholdIndex -> deduplicated Account candidates.
  - Candidate evaluation reuses the existing integration projection -> exact Health ->
    Sizing -> Economics pipeline.
  - While bootstrap is incomplete, or if a particular account cannot be threshold-qualified,
    price updates conservatively wake all/fallback accounts for the affected Bank. UNKNOWN never
    becomes "not a candidate".
  - If current Maintenance risk-price extraction fails for a Bank, all accounts for that Bank
    are conservatively woken.
- FIX-133c threshold math indexes the exact confidence-biased effective Maintenance price
  produced by Marginfi's own oracle adapter (including integration multiplier), preserves raw
  positive I80F48 bits, reconciles e-mode from the pinned type-crate, aggregates repeated
  Balance slots by Bank/side, and refuses mixed asset+liability use of the same Bank.
- Before accepting any threshold set, the derived total Maintenance asset/liability values
  must exactly equal the pinned `get_health_components` output; mismatch -> fallback.

No full global account scan is performed per oracle update. The only full account enumeration
is Recovery bootstrap construction; threshold calculations themselves are bounded across loop
iterations.

Status: **IMPLEMENTED IN SOURCE / FIX-133d COMPILE+UNIT TEST PENDING.**
LIVE remains disabled. No transaction sent. FIX-129 Jito lifecycle cells remain UNKNOWN.


### 2026-10-05 — FIX-133e Oracle wakeup coalescing / runtime telemetry

After FIX-133d passed local compile/unit tests, the Oracle-trigger queue was hardened before
Mainnet Shadow:
- repeated Oracle/dependency updates for the same MarginfiAccount are coalesced into one pending
  wakeup;
- the queued work is replaced with the newest ingress timestamp/slot so freshness is measured
  from the latest observed price-state transition;
- stale queue-order entries are skipped deterministically;
- runtime counters now expose dependency updates, threshold/fallback candidate hits, newly
  enqueued wakeups, coalesced wakeups, price-extraction fallbacks, current/max pending Oracle
  queue depth, bootstrap remaining, tracked threshold accounts and fallback accounts;
- regression tests cover coalesced-latest work and stale-order skipping.

Commit: `fba6b5e6`.

Status: **COMPILE+UNIT TEST PASS** on 2026-10-05.
User-reported local verification:
- `cargo test --workspace --features "crate_health txv1"` → PASS;
- `cargo test --workspace --features "crate_health"` → PASS;
- no failures; only the pre-existing `JupiterV2RouteProvider::with_base_url` dead-code warning.

Next gate: short Mainnet Shadow qualification. Verify `threshold_index_ready`, fallback ratio,
real `oracle_threshold` records, ingress/channel health, coalescing effectiveness and bounded
Oracle pending depth before any 72 h run.


### 2026-10-05 — R125 FIX-133 Mainnet Shadow, bootstrap phase PASS / trigger phase pending

Mainnet Shadow reached Recovery READY cleanly:
- snapshot A/B: 82,700 liability accounts, 204 Banks, 494 required dependencies,
  83,406 total accounts, missing_required=0;
- ReverseIndex: 204 Banks, 82,700 MarginfiAccounts, 0 unsupported Bank oracle layouts;
- liability-bearing threshold bootstrap population: 77,984 accounts.

Threshold bootstrap completed:
- tracked thresholds: 77,756;
- conservative fallback accounts: 228;
- fallback ratio: 228 / 77,984 = 0.2924%.

Observed fallback excerpts are protocol-originated stale integration state:
Kamino `ReserveStale` and JupLend `JuplendLendingStale`. These accounts remain
conservative fallback candidates; they are not silently excluded. The integration census in
the same run found 194 Kamino and 43 JupLend accounts (Drift 0 in the current census).

The run was manually stopped immediately after
`threshold_index_ready: tracked=77756 fallback=228`.
Therefore this establishes **bootstrap/runtime construction PASS only**. It does not yet
qualify the live Oracle wakeup phase: `oracle_trigger_stats`, real
`trigger=oracle_threshold` records, queue/coalescing behavior and sustained ingress/channel
health still require evidence.

No transaction sent. LIVE disabled. 72 h NOT STARTED.


### 2026-10-05 — R125b FIX-133 Oracle trigger runtime qualification

Evidence: `docs/evidence/R125b_oracle_trigger_runtime.json`.

Observed Mainnet Shadow state:
- Recovery clean: 82,699 liability accounts, 204 Banks, 494 required dependencies,
  83,405 total accounts, missing_required=0.
- ReverseIndex: 82,699 accounts, 204 Banks, 0 unsupported oracle layouts.
- Threshold bootstrap complete: 77,984 liability-bearing accounts considered,
  77,756 tracked, 228 conservative fallback, bootstrap_remaining=0.
- Fallback ratio = 0.2924%.
- Integration census: Kamino 194, JupLend 43, Drift 0 in current account census.

Observed active Oracle-trigger sample:
- threshold_ready=true;
- dependency_updates=7;
- candidate_hits=15,535;
- enqueued=15,535;
- coalesced=0 in this short sample;
- price_fallbacks=38;
- pending at measurement=0;
- max_pending=6,578;
- detect_queue=0.
- ingress channel: capacity=65,536, sent=6,105, received=5,829, overflow=0.

Observed price-fallback causes were protocol-originated stale integration state:
Kamino `ReserveStale`, JupLend `JuplendLendingStale`, and at least one Drift
`DriftSpotMarketStale`. In each case FIX-133 correctly used the conservative Bank-account
fallback rather than treating missing current pricing as no opportunity.

Assessment:
- **FIX-133 functional Oracle wakeup path: PASS.**
  Real dependency updates reached ReverseIndex/Bank mapping, produced candidate wakeups,
  entered the existing exact pipeline, and the Oracle pending queue drained back to zero.
- **Threshold-index construction/runtime: PASS.**
- **Ingress-loss gate for this sample: PASS** (overflow=0).
- **72 h readiness: NOT YET PASS.**
  The observed burst max_pending=6,578 from only seven dependency updates is materially high.
  It appears dominated by conservative all-account wakeups for Banks whose integration pricing
  is stale. This is safe/correct but potentially too expensive for sustained operation.
  Coalescing was zero in this short sample, so no evidence yet that repeated updates materially
  compress this workload.

Decision:
- Keep FIX-133 enabled on main.
- Do not weaken stale integration handling or silently exclude those accounts.
- Before 72 h, reduce safe fallback fan-out by refreshing/projecting the affected integration
  state before threshold-price extraction, or otherwise prove a bounded per-Bank fallback path
  that preserves exact Health semantics.
- Re-run a short R126 load qualification and require bounded pending depth, no ingress overflow,
  and stable drain under repeated dependency updates.

No transaction sent. LIVE remains disabled. FIX-129 Jito lifecycle cells remain UNKNOWN.


### 2026-10-05 — R126 stale-integration Bank projection fanout reduction

Motivation from R125b:
- functional Oracle-trigger path PASS;
- queue drained and ingress overflow remained zero;
- however max Oracle pending reached 6,578 after only seven dependency updates;
- observed bursts were dominated by conservative full-Bank fallback when Marginfi pricing
  rejected stale Kamino/JupLend/Drift integration state.

Implemented:
- `582d73fd`: added a dedicated bounded RPC worker for integration-Bank price projection.
- `7a08ac06`: on stale canonical integration pricing, the runner now:
  1. builds the exact existing per-Bank integration refresh prefix;
  2. simulates it once for that Bank;
  3. overlays only the refreshed writable integration accounts;
  4. extracts the fresh Marginfi Maintenance Low/High risk prices;
  5. applies ThresholdIndex to select Accounts;
  6. includes existing conservative threshold-fallback Accounts.
- a second dependency update for the same Bank while projection is pending is coalesced into
  the latest ingress timestamp/slot rather than spawning another projection.
- projection result is accepted only if the canonical Bank is unchanged and the returned
  simulation slot is at least the newest buffered dependency-event slot.
- simulation/build/sign/queue/projection/price failures retain the previous conservative
  full-Bank wakeup; correctness is never traded for fanout reduction.
- `e9014c8c`: result ownership made explicit before compilation to avoid borrow/move ambiguity.
- telemetry extended with Bank-projection pending/submitted/ready/failed/stale counters.

Expected R126 effect:
- common stale-integration case should move from thousands of immediate Account wakeups to one
  Bank refresh simulation plus threshold-selected Accounts;
- worst-case semantics remain identical to R125b via full-Bank fallback.

Status: **COMPILE+UNIT TEST PASS** on 2026-10-05.
User-reported local verification:
- `cargo test --workspace --features "crate_health txv1"` → PASS;
- `cargo test --workspace --features "crate_health"` → PASS;
- no failures.

No transaction sent. LIVE disabled. This is a short-load qualification step; no 72 h run is
planned. Next: run approximately 10–30 minutes and compare max_pending,
candidate_hits/update, price_fallbacks, Bank-projection success ratio and ingress overflow
against R125b.


### 2026-10-05 — R126 automated qualification runner

Added `project_0/scripts/run_r126_qualify.py` to remove manual timing/log collection.

Behavior:
- launches the existing send-disabled `run_shadow_mainnet.sh` in its own process group;
- waits for `threshold_index_ready`;
- starts the qualification timer only after threshold bootstrap is complete;
- default observation window = 15 minutes;
- startup timeout = 12 minutes;
- captures terminal log, opportunity JSONL, summary JSONL, JSON report and text report under
  `project_0/data/shadow/r126_<UTC stamp>_*`;
- stops the entire Shadow process group with SIGINT after the observation window, escalating
  only if the process does not exit;
- automatically parses threshold readiness, Oracle trigger stats, ingress overflow,
  Bank-projection counters, queue depth and opportunity records;
- compares `max_pending` to the R125b baseline of 6,578;
- emits PASS / PARTIAL / FAIL and a compact report suitable for review without copying full logs.

PASS requires, among other safety checks:
- threshold_index_ready observed;
- real Oracle dependency updates observed;
- ingress overflow = 0;
- max_pending below the R125b baseline;
- final Oracle pending queue drained;
- R126 stale-Bank projection path exercised and at least one projected Bank price became ready.

If the stale-Bank projection path is not exercised during the chosen window, the result is
PARTIAL rather than falsely claiming R126 optimization PASS.

Commit: `1002d318`.


### 2026-10-05 — R126 runtime FAIL and R126b scheduler fairness fix

R126 automatic qualification report:
- verdict: FAIL;
- threshold_ready=true, tracked=76,464, fallback=1,521;
- dependency_updates=39;
- candidate_hits=91,861 (2,355.4/update);
- final pending=0 but max_pending=67,065 versus R125b baseline 6,578;
- coalesced=319;
- Bank projections: submitted=31, ready=26, failed=5, stale=0, ready ratio=83.9%;
- ingress stats sample still reported overflow=0, but WSS emitted repeated
  `ingress protocol: sink overflow` and reconnect loops, so the run is invalid as a
  lossless-ingress qualification.
- 447,541 `oracle_threshold` records were emitted before manual abort.

Interpretation:
- the stale-Bank projection idea itself is viable (26/31 ready), but it did not solve the
  dominant load problem;
- the main state/detection loop starved ingress while draining large detection queues,
  allowing the bounded ingress sink to fill and forcing reconnects;
- the observed `max_pending=67,065` is a hard regression versus R125b.

R126b implementation:
- `a281e7e8`: bounded scheduler fairness between canonical ingress and queued detection.
  When both are available, ingress is drained non-blocking but at most
  `P0_INGRESS_BURST_BEFORE_DETECT` events (default 64) before one queued detection item is
  forced. This prevents either ingress or detection work from starving the other.
- telemetry now includes estimated `ingress_backlog` and `ingress_since_detect`.
- `95353b69`: the automated qualifier treats any
  `ingress protocol: sink overflow` as an immediate hard FAIL, stops the process group and
  records the overflow in the compact report. The stats parser remains backward-compatible
  with pre-R126b logs.

Status: **R126 FAIL / R126b IMPLEMENTED, COMPILE+UNIT TEST PENDING**.
No transaction sent. LIVE disabled.


### 2026-10-05 — R126b ingress fairness PASS / candidate-fanout FAIL; R127 direction

Evidence: `docs/evidence/R126b_scheduler_fairness.json`.

User-reported automatic qualification:
- threshold_ready=true; tracked=77,756; fallback=229;
- dependency_updates=830;
- candidate_hits=994,177 = 1,197.8/update;
- final Oracle pending=74,005;
- max_pending=78,214 versus R125b baseline 6,578;
- price_fallbacks=1,922; coalesced=797,256;
- Bank projections: submitted=648, ready=513, failed=117, stale=11, ready ratio ~80%;
- ingress: sink_overflow_lines=0, overflow counter=0, final backlog=6;
- Oracle-threshold records=246,458.

Assessment:
- **R126b scheduler fairness: PASS.** The previous WSS sink-starvation/reconnect failure did not
  recur; canonical ingress remained caught up.
- **R126b overall: FAIL.** Candidate fanout is still unbounded for sustained operation.
- Bank refresh projection is useful (80% ready) but is not the dominant fix.
- Root cause: the current threshold query returns every account already beyond its stored
  threshold on every relevant price update, so already-endangered accounts are repeatedly
  re-woken.
- A pure old-price/new-price crossing query was prototyped in `378a69f` but is **NOT promoted
  to the production trigger path**: an account's threshold for Bank A depends on the prices of
  its other Banks, so a stored threshold can become stale after unrelated price updates. Wiring
  that directly would risk false negatives.

Reference check against `0dotxyz/eva01@15625d5f57dea2ed0a47e857de814fdd68d2513d`:
- startup: one scan of all accounts with liabilities;
- Oracle update: Oracle -> affected Banks -> accounts with liabilities using those Banks;
- Bank update: affected Bank -> accounts with liabilities using that Bank;
- account updates: only that debtor account;
- account evaluation is parallelized and uses Marginfi health/protocol math.

Decision for R127 / First-Euro Long-Tail:
- stop spending iterations trying to make the current threshold index the primary Long-Tail
  admission mechanism;
- implement an eva01-style **affected-Bank targeted exact-health sweep**, deduplicated across a
  burst and isolated from ingress, with measured worker/backlog bounds;
- this is not a global 78k scan per Oracle tick; only debt accounts tied to the changed Banks are
  evaluated;
- keep ThresholdIndex as a later Race optimization only after an exact invalidation model is
  proven;
- after R127 is stable, immediately run historical/Shadow profit evidence rather than another
  large infrastructure detour.

No transaction sent. LIVE remains disabled.


### 2026-10-05 — R127 simplify Long-Tail admission to affected-Bank exact-health sweep

Reason:
R126b proved that ingress fairness can be kept lossless, but the threshold-trigger admission
still generated 994,177 candidate hits from 830 dependency updates and max_pending=78,214.
That mechanism is too complex and too noisy for the First-Euro Long-Tail path.

Reference:
`0dotxyz/eva01@15625d5f57dea2ed0a47e857de814fdd68d2513d` uses the simpler model:
- startup scan of debt accounts;
- Oracle update -> affected Banks -> debt accounts using those Banks;
- Bank update -> debt accounts using that Bank;
- MarginfiAccount update -> that account;
- exact account evaluation uses protocol health math; CPU parallelism is an optimization, not
  part of protocol semantics.

R127 implementation:
- `a7160c68`: `P0_USE_THRESHOLD_TRIGGER=false` is now the default.
- ReverseIndex remains authoritative for Oracle -> Bank -> Account fanout.
- Oracle dependency update deduplicates only accounts tied to affected Banks and labels them
  `oracle_bank_sweep`.
- Bank update likewise queues only accounts tied to that Bank as `bank_update_sweep`.
- Before integration projection, sizing, routing or economics, those sweep records run exact
  canonical Marginfi Maintenance health.
- canonical healthy -> drop immediately;
- canonical liquidatable -> continue through the existing production pipeline;
- health error/stale integration -> fail closed and continue through the existing
  refresh/projection path rather than being silently discarded.
- Existing direct MarginfiAccount updates remain direct triggers.
- ThresholdIndex code remains available behind `P0_USE_THRESHOLD_TRIGGER=true` for later
  Race/optimization research but is no longer the default Long-Tail admission mechanism.
- `1afebde3`: legacy threshold enqueue compatibility fixed after the generic trigger-aware
  dedup queue change.
- `a229cde4`: added `run_r127_qualify.py`, a self-stopping 10-minute qualifier starting only
  after `r127_health_sweep_ready`. It fails immediately on WSS sink overflow and reports
  sweep checks/healthy/liquidatable/errors, queue depth and ingress backlog.

Deliberate staging:
- the eva01-style one-time full startup scan of every debtor is not added in this first R127
  change. Existing bootstrap probes remain. This isolates the event-driven sweep behavior first;
  the one-time startup sweep can be added after runtime stability is proven.
- no new RPC or approximation is introduced in the admission filter; it uses the already-pinned
  official Marginfi health engine over canonical RAM state.

Status: **COMPILE+UNIT TEST PASS** on 2026-10-05.
User-reported local workspace test run completed without failures; final Doc-tests also PASS
(`0 passed; 0 failed` means no doctest cases existed, not a failure).

Next gate: automated R127 short Mainnet Shadow qualification with
`project_0/scripts/run_r127_qualify.py`.

No transaction sent. LIVE disabled.


### 2026-10-05 — R127 FAIL; R127c complete fanout-before-queue fix

Evidence: `docs/evidence/R127_20261005T143113Z.json`.

R127 automatic qualification:
- dependency_updates=96,825;
- exact-health checks=757,250;
- healthy=697,151 (92.1%);
- liquidatable=58,007; health errors=2,092;
- coalesced=15,106,838;
- pending_final=77,230; max_pending=78,453;
- ingress backlog final=29, max=439;
- ingress overflow=0; WSS sink-overflow lines=0.

Assessment:
- dependency mapping/coverage PASS;
- ingress losslessness PASS;
- R127 admission placement FAIL: account fanout happened once per dependency update and only
  then was deduplicated in the pending queue. The 92.1% healthy rejection proves exact Health is
  a strong admission filter, but it was applied after the expensive fanout had already happened.

R127c correction:
- `609ca943`: Rayon enabled specifically for CPU-parallel exact-health admission.
- `077a7645`: dependency and Bank writes now dirty Banks only; no per-update account fanout.
  A flush expands the union of dirty Banks once to a unique Account map, runs exact canonical
  Marginfi Health in parallel, drops healthy accounts before the pending queue, and forwards only
  liquidatable plus fail-closed health-error accounts to the existing projection/economics path.
  New metrics: sweep_batches, sweep_account_refs, sweep_unique_accounts, sweep_forwarded,
  sweep_max_unique_batch, sweep_max_dirty_banks.
- `abcf3237`: batching is real and latency-bounded: flush after 64 dependency/Bank events or
  5 ms maximum batch age. An open batch polls ingress every 1 ms so the 5 ms bound cannot turn
  into the old 250 ms idle timeout.
- `fcd262e7`: removed per-candidate `protocol_luts` terminal spam; LUT behavior is unchanged.
- `28474060`: R127 qualifier now requires batch execution/dedup evidence and treats
  max_pending >10,000 or final pending >1,000 as PARTIAL even when improved; no dependency
  updates remains FAIL.
- `69b10360`: added the missing eva01-style one-time startup scan by marking every current
  group Bank dirty once after recovery; the same parallel exact-health admission handles it.

Resulting default Long-Tail path:

```text
canonical ingress
→ apply all updates immediately
→ dependency/Bank writes mark dirty Banks only
→ ≤5 ms / ≤64-event batch
→ union Bank→Accounts once
→ unique accounts
→ parallel exact Marginfi Health
→ healthy DROP
→ liquidatable / stale-error only
→ existing refresh/projection → sizing → economics → build/sim
```

This removes the previous ~78k healthy pending backlog at its source rather than adding another
queue/filter after it.

Status: **R127 FAIL / R127c IMPLEMENTED, COMPILE+UNIT TEST PENDING**.
No transaction sent. LIVE disabled.


### 2026-10-05 — R127c rollback anchor + compile/unit PASS

For an explicit rollback/reproduction point, the exact pre-R127c tree was frozen as:

- commit: `bdcaa41c2c41eeefdacad08e7a8285a7f1921b79`
- branch: `archive/r127b-pre-batch-20261005`

A dedicated change/rollback document now exists at:

- `project_0/docs/R127C_CHANGELOG_ROLLBACK.md`
- documentation commit: `f70d3105068c6ff0674ab0a3b8c023005896211a`

That document records:
- the complete R127c implementation commit chain;
- exact before/after runtime semantics;
- new batching ENV controls;
- new telemetry;
- startup-sweep behavior;
- files changed between pre-R127c and complete R127c;
- safe reproduction and revert procedures.

User-reported local validation after the complete R127c code change:
- workspace `crate_health txv1`: PASS;
- workspace `crate_health`: PASS;
- visible unit-test group: 30 passed / 0 failed;
- all shown Doc-tests PASS (0 cases where none exist).

The remaining compiler warning
`value assigned to detect is never read`
does not fail compilation/tests and is deliberately not changed during the already-running
R127c Mainnet Shadow qualification, to keep the measured binary/source semantics stable.

Current R127c Shadow qualification prefix:
`r127_20261005T160040Z`.

Status: **R127c COMPILE+UNIT PASS / MAINNET SHADOW QUALIFICATION RUNNING**.
No transaction sent. LIVE disabled.


### 2026-10-05 — R127c PARTIAL → R127d Candidate Cache implemented

R127c Mainnet Shadow `r127_20261005T161806Z`:
- verdict: PARTIAL;
- dependency_updates=95,833;
- sweep_checks=12,326,220;
- healthy=11,212,366 (91.0%);
- liquidatable=1,056,733;
- errors=57,121;
- batches=26,253;
- account refs=20,407,185;
- unique accounts=12,326,220;
- batch dedupe=39.6%;
- forwarded=1,113,854;
- max_pending=6,279 (R126b baseline 78,214);
- pending_final=6,154;
- ingress backlog max=506; final=24;
- ingress overflow=0; sink-overflow lines=0.

Evidence: `docs/evidence/R127c_20261005T161806Z.json`.

Interpretation:
- R127c batching is structurally successful and remains the base architecture.
- Remaining backlog is repeated downstream admission of persistent Liquidatable/Error Accounts,
  not raw ingress or pre-queue fanout.
- PHASENPLAN P2 explicitly requires a Candidate Cache with hard invalidation; R127d implements
  that missing layer.

Rollback anchor before R127d:
- commit `dde69699a9943e376ca4343d454f8e3177ee9b36`;
- branch `archive/r127c-batch-partial-20261005`.

R127d:
- `1047896a`: Candidate Health-state cache; exact Health still runs every affected batch;
  transitions forward immediately; Bank writes force-rearm; persistent Liquidatable candidates
  recheck every 64 slots by default; persistent health errors every 16 slots; repeat work is
  soft-capped at 1,024 pending while transition/forced work bypasses that cap; cache clears on
  Recovery.
- `a319e0a1`: qualifier adds cache suppression/re-arm evidence and gates.
- `project_0/docs/R127D_CHANGELOG_ROLLBACK.md`: exact semantics + rollback procedure.

Status: **R127c PARTIAL / R127d IMPLEMENTED, COMPILE+UNIT TEST PENDING**.
No transaction sent. LIVE disabled.


### 2026-10-05 — R127d qualification-default correction before runtime test

Review of `1047896a` found a mismatch between the intended Candidate Cache semantics and the
initial config defaults:

- intended: unchanged persistent candidates can be reconsidered once per new Solana slot;
- initial code: Liquidatable every 64 slots, Error every 16 slots.

The 64/16-slot settings were judged too slow for a profit-sensitive Long-Tail pipeline and were
not accepted as the qualification configuration.

Correction:
- `e5577da37941082d3eb1f36da3bdb7fb18b0383c`
- `P0_CANDIDATE_RECHECK_SLOTS=1`
- `P0_CANDIDATE_ERROR_RECHECK_SLOTS=1`
- `P0_CANDIDATE_REPEAT_SOFT_CAP=1024` unchanged;
- active config is emitted as `r127_candidate_cache_config` and in
  `oracle_trigger_stats`.

Rollback anchors:
- pre-Candidate-Cache R127c PARTIAL:
  `archive/r127c-batch-partial-20261005`;
- initial 64/16-slot R127d:
  `archive/r127d-initial-64slot-20261005`;
- corrected one-slot R127d pre-qualification:
  `archive/r127d-1slot-prequal-20261005`.

Status: **R127d 1-slot configuration IMPLEMENTED / COMPILE+UNIT TEST PENDING**.
No transaction sent. LIVE disabled.


### 2026-10-05 — R127d Mainnet Shadow PASS

Run: `r127_20261005T171043Z`.

Runtime qualification anchor:
- commit `cf5085e796b4956fcf2376d08369b2b818dff39a`;
- branch `archive/r127d-pass-runtime-20261005`.

Result:
- verdict: **PASS**;
- dependency_updates=96,816;
- sweep_checks=12,430,220;
- healthy=11,316,714 (91.0%);
- liquidatable=1,054,611;
- errors=58,895;
- sweep_batches=22,023;
- sweep_account_refs=20,657,014;
- sweep_unique_accounts=12,430,220;
- batch_dedupe=39.8%;
- sweep_forwarded=151,045;
- candidate_cache_entries=78,488;
- candidate_cache_suppressed=962,461 (86.4%);
- candidate_cache_softcap_suppressed=945,968;
- candidate_cache_transition_forwards=6,891;
- candidate_cache_periodic_forwards=108,728;
- candidate_cache_forced_forwards=35,426;
- pending_final=835;
- max_pending=6,285;
- coalesced=32,342;
- ingress backlog final=0, max=460;
- ingress overflow=0;
- WSS sink-overflow lines=0.

Comparison to R127c PARTIAL:
- downstream forwards 1,113,854 → 151,045;
- pending_final 6,154 → 835;
- downstream coalesces 986,849 → 32,342;
- steady-state queue now drains below the qualification limit while ingress stays lossless.

Interpretation:
- R127c solved the pre-queue dependency fanout;
- R127d solved repeated downstream admission of persistent candidates;
- the remaining max_pending ~6.3k peak is consistent with the one-time startup/full-debtor sweep
  and is not accompanied by sustained backlog or ingress loss in this run.

Gate decision:
**R127 scheduler/admission gate PASS.**
Do not spend another architecture iteration on this scheduler without new evidence.

Next priority is economic proof:
1. historical/real profitable liquidation replay through the current pipeline;
2. Drift refresh source-exact repair/verification where required by those cases;
3. exact net-profit evidence;
4. short Mainnet Shadow competition evidence;
5. only then controlled value-bearing qualification.

Evidence: `docs/evidence/R127d_20261005T171043Z.json`.

No transaction sent. LIVE remains disabled.

---

## 2026-10-05 — Astra production/economic audit handoff

### Provenance

Audited runtime source:

```text
799048f8fa95df4c29f2aa6b35bcc3615405fe19
```

Preserved audit corpus/bundle:

```text
project_0/docs/evidence/AUDIT_20261005/
project_0/docs/evidence/AUDIT_20261005/darkbot_audit_bundle_20261005.zip
```

Authoritative synthesis / next-work order:

```text
project_0/docs/AUDIT_HANDOFF_2026-10-05.md
```

### Tests

- workspace `crate_health,txv1`: 136 PASS / 0 FAIL;
- workspace `crate_health`: 134 PASS / 0 FAIL.

### R127d interpretation after adversarial audit

The R127d Mainnet Shadow result remains a valid scheduler/queue/fanout performance PASS:
lossless ingress, materially reduced forwarding and a draining pending queue were measured.

However, Candidate Cache **correctness is reopened** because the checked-in predicate reproduced:

1. same-slot repeated `Liquidatable` suppression after a changed price/profit input;
2. same `Error` class suppression after changed integration input;
3. deterministic soft-cap starvation: with 1,025 persistent candidates and cap 1,024, the final candidate received zero periodic forwards for 100 simulated slots.

Evidence:

```text
project_0/docs/evidence/AUDIT_20261005/cache_counterexamples.py
project_0/docs/evidence/AUDIT_20261005/cache_counterexamples.txt
```

Gate decision:

```text
R127 PERFORMANCE/FANOUT = PASS
R127 CANDIDATE CORRECTNESS = REOPENED / BLOCKED
CONTROLLED LIVE = BLOCKED
```

### Drift

Unsigned current-Mainnet simulation returned:

```text
InstructionFallbackNotFound
Custom(101)
```

Evidence:

```text
project_0/docs/evidence/AUDIT_20261005/drift_dispatch.json
```

Drift coverage remains OPEN/BLOCKED until source/deployed-program compatibility is proven, or Drift-dependent candidates are explicitly classified unsupported/fail-closed.

### Historical real liquidation corpus

Six recent successful receivership liquidations are preserved with signatures, slots, decoded instructions, fees and wallet/token deltas.

Gross equity deltas are observable, but:

```text
complete_realized_usd_pnl = UNKNOWN
historical_darkbot_classification = UNKNOWN
historical_prestate_missing = true
```

No realized profit claim is permitted from this corpus until complete attribution and prestate replay exist.

### Auxiliary winner/economics probe limitation

The helper `p0-audit-json.rs` did not reach its intended winner/economics assertions; execution failed earlier during SDK JSON deserialization. Therefore those intended assertions are **not evidence**.

The route-floor and winner-parser concerns remain high-priority hypotheses requiring independent direct regression tests.

### Current economic verdict

```text
B — economic mechanism valid; specific implementation/evidence blockers
currently prevent reliable profitable-capture proof.
```

### Next ordered gates

```text
1. Candidate same-slot revision correctness
2. Candidate repeat fairness/no starvation
3. R127 scheduler/admission requalification
4. Executable-floor economics proof/fix
5. Winner parser fixture proof/fix
6. Drift qualification or explicit unsupported scoping
7. Historical prestate reconstruction
8. Six-case production replay
9. Complete realized-PnL attribution where defensible
10. Short competitive Mainnet Shadow
11. Controlled value-bearing execution
12. First realized positive net P&L
```

Long Race/TPU/provider/architecture work is paused unless new measured evidence makes it necessary.



---

## 2026-10-06 — FIX-134 P1-A same-slot causal revision repair integrated

### Source / integration

Original FIX-134 commit:

```text
45a7cbcea60346ce030da132cd9af39d6aba9563
```

Integrated onto then-current `main` as:

```text
35184b42ef5c4df16dfa90a4fda1badff103f586
```

Only:

```text
project_0/native/crates/darkbot-runner/src/main.rs
```

changed relative to the prior `main`. Concurrent BSC work was not modified.

### Correctness change

FIX-134 closes the reproduced same-slot Candidate Cache blindness by carrying a monotonic causal input revision from Bank/oracle/integration dependency updates into candidate admission.

Changed causal input is correctness work and bypasses the ordinary periodic-repeat soft cap. Exact duplicate same-revision evaluations remain suppressible.

Recovery clears Candidate Cache revision state. Multiple dirty Banks collapse per account while retaining the newest causal revision.

### Regression evidence

Targeted regression:

```text
test integration_probe_tests::candidate_cache_forwards_transition_force_revision_and_bounded_periodic_recheck ... ok
test result: ok. 1 passed; 0 failed
```

Full workspace regression markers:

```text
FIX134_WORKSPACE_TXV1_PASS
FIX134_WORKSPACE_HEALTH_PASS
FIX134_ALL_REGRESSION_GATES_PASS
```

Gate decision:

```text
P1-A same-slot candidate revision correctness = PASS
FIX-134 = INTEGRATED
R127 candidate-admission correctness = STILL OPEN pending P1-B fairness/no-starvation
CONTROLLED LIVE = BLOCKED
```

Next ordered gate:

```text
FIX-135 / P1-B: Candidate repeat fairness / no-starvation
```


---

## 2026-10-06 — FIX-135 P1-B fairness / no-starvation integrated

Integrated commit:

```text
cacdbc82941d5a277ce49e1e52d3d5b4fd8c3b3b
```

Regression evidence:

```text
candidate_cache_forwards_transition_force_revision_and_bounded_periodic_recheck ... ok
candidate_fairness_services_all_above_soft_cap_without_starvation ... ok
candidate_fairness_repeat_admission_remains_bounded ... ok

FIX135_WORKSPACE_TXV1_PASS
FIX135_WORKSPACE_HEALTH_PASS
FIX135_ALL_REGRESSION_GATES_PASS
```

Acceptance covered:

- persistent candidates above the 1,024 repeat cap are serviced without deterministic starvation;
- repeat admission remains bounded by the configured cap;
- FIX-134 transition / forced / causal-revision correctness path remains green;
- fairness telemetry added for wait depth, fairness rotations and periodic deferrals.

Gate decision:

```text
P1-B candidate repeat fairness / no-starvation = PASS
FIX-135 = INTEGRATED
R127 scheduler/admission = READY FOR MAINNET SHADOW REQUALIFICATION
CONTROLLED LIVE = BLOCKED
```


---

## 2026-10-06 — Experimental parallelism evidence retained after rollback

**Status:** EVIDENCE RETAINED / EXPERIMENTAL IMPLEMENTATION REVERTED

The FIX-140..143 implementation branches were experimental and are **not** the current Project-0 runtime baseline. Their runtime code was intentionally removed from `main`. The measurements below are retained because they materially inform the new authoritative parallel-execution design.

Current implementation baseline remains the clean pre-parallel FIX-139 Project-0 content.

Authoritative implementation plan:

`project_0/docs/PARALLEL_EXECUTION_MASTER_PLAN.md`

### A. FIX-137 single-flight evidence

Run:

`r127_20261006T122505Z`

Measured:

```text
sweep_forwarded=1,016,955
candidate_cache_revision_forwards=993,356
candidate_pending_coalesced=890,319
candidate_revision_coalesced=890,319
candidate_physical_enqueues=126,636
pending_final=6,179
max_pending=6,294
ingress_overflow_counter=0
sink_overflow_lines=0
```

Conclusion:

- logical causal-revision forwarding was heavily coalesced before physical execution work;
- the million-level logical forwarding count was not itself the physical queue bottleneck;
- downstream execution/drain remained the material bottleneck.

### B. FIX-138 serial pipeline timing baseline

Run:

`r127_20261006T130642Z`

Measured:

```text
dependency_updates=28,894
dependency_content_unchanged=7
dependency_content_changed=28,887

detect_health_calls=70,022
detect_health_ns_total=237,557,841
detect_health_ns_max=117,709

detect_pipeline_calls=70,022
detect_pipeline_ns_total=109,925,348,708
detect_pipeline_ns_max=2,654,144,083

candidate_physical_enqueues=78,878
pending_final=6,239
```

Derived:

```text
Health average   ≈ 3.39 µs/call
Pipeline average ≈ 1.57 ms/call
Pipeline max     ≈ 2.654 s
Pipeline / Health average ratio ≈ 460x
unchanged dependency writes ≈ 0.024 %
```

Conclusion:

- exact Health was not the primary bottleneck;
- duplicate dependency updates were negligible;
- synchronous planning / fee RPC / simulation inside the serial candidate consumer was the dominant latency risk;
- this measurement is the primary justification for redesigning execution parallelism.

### C. R127 15:21 coalescing follow-up

Run:

`r127_20261006T152101Z`

Measured:

```text
sweep_forwarded=315,802
candidate_cache_revision_forwards=304,944
candidate_cache_transition_forwards=6,480
candidate_cache_forced_forwards=4,378
candidate_cache_suppressed=0

coalesced=231,212
enqueued=84,590
pending_final=5,184
max_pending=6,306

ingress_backlog_final=0
ingress_backlog_max=266
ingress_overflow_counter=0
sink_overflow_lines=0
```

Identity:

```text
315,802 logical forwards
- 231,212 coalesced
= 84,590 physical enqueues
```

Approximately 73.2% of logical forwards were collapsed into existing pending work.

Conclusion:

- `candidate_cache_suppressed=0` alone does not mean single-flight/coalescing is ineffective;
- further cache-suppression tuning was not the correct primary lever;
- execution drain remained the relevant target.

### D. FIX-141 experimental cross-candidate execution result

Run:

`r127_20261006T204714Z`

Measured:

```text
exec_workers=11
exec_dispatched=118,977
exec_completed=118,680
exec_inflight=297
exec_job_backlog=286
exec_job_overflow=1,500,739
exec_queue_full=128,813
exec_stale_results=7,695
exec_same_account_coalesced=7
exec_max_inflight=716
exec_latency_ns_total=1,096,835,779,624
exec_latency_ns_max=3,105,137,167
pending_final=6,309
candidate_physical_enqueues=121,508
ingress_overflow_counter=0
sink_overflow_lines=0
```

Derived over 180 s:

```text
completed throughput ≈ 659.3 candidates/s
serial FIX-138 throughput ≈ 389.0 candidates/s
throughput gain ≈ +69.5 %

average worker execution latency ≈ 9.24 ms
average concurrent worker occupancy ≈ 6.09 / 11
stale-result fraction ≈ 6.5 %
```

Evidence conclusion:

- cross-candidate parallel execution can materially improve throughput;
- the measured gain was approximately +69.5% versus the serial FIX-138 baseline;
- however this specific implementation suffered severe dispatch retry churn:
  `exec_queue_full=128,813`,
  `exec_job_overflow=1,500,739`;
- therefore FIX-141 is evidence for the architectural direction, **not** approved implementation code.

### E. FIX-142 experimental dispatch-churn result

Run:

`r127_20261006T205631Z`

Measured:

```text
candidate_physical_enqueues=118,617

exec_workers=11
exec_dispatched=112,847
exec_completed=112,735
exec_inflight=112
exec_job_backlog=101
exec_job_overflow=0
exec_queue_full=0
exec_stale_results=4,428
exec_same_account_coalesced=12
exec_max_inflight=716
exec_latency_ns_total=1,044,099,656,610
exec_latency_ns_max=2,889,421,584
exec_available_slots=603

pending_final=6,217
max_pending=6,988
ingress_backlog_final=9
ingress_backlog_max=42
ingress_overflow_counter=0
sink_overflow_lines=0
```

Derived:

```text
completed throughput ≈ 626.3 candidates/s
average worker execution latency ≈ 9.26 ms
average execution concurrency ≈ 5.8 workers
physical admissions - completed ≈ 5,882
pending_final = 6,217
```

Evidence conclusion:

- dispatch retry churn was successfully eliminated in this experiment:
  - `exec_job_overflow: 1,500,739 -> 0`
  - `exec_queue_full: 128,813 -> 0`;
- worker backlog fell from 286 to 101;
- stale results fell from 7,695 to 4,428;
- 603 execution queue slots were still free at stop;
- despite free execution capacity, total pending remained ~6.2k;
- therefore the next bottleneck moved upstream into candidate preparation / dispatch preparation;
- simply increasing execution worker count would not solve the queue.

### F. What is retained vs discarded

Retained as evidence:

```text
FIX-138 serial timing baseline
FIX-137/coalescing measurements
FIX-141 +69.5% throughput observation
FIX-141 queue/retry failure evidence
FIX-142 zero-overflow dispatch observation
FIX-142 upstream-preparation bottleneck conclusion
```

Discarded as implementation:

```text
FIX-140 runtime changes
FIX-141 worker implementation
FIX-142 dispatch implementation
FIX-143 batch-preparation implementation
subsequent branch-only hardening patches
```

These discarded implementations must not be resurrected wholesale or cherry-picked.

The new implementation must follow `PARALLEL_EXECUTION_MASTER_PLAN.md` from MT0 onward, with one bounded step, one test gate, and one measurement gate at a time.

### 2026-10-07 — P6-MT Tranche 1 r3 local qualification — PASS

Qualified code commit:
`91435080ca7eb01d6ae201402b31d2f368210c26` — `[Project 0] integrate P6-MT tranche 1 r3`

Integration base:
`9e2a7a4a6893b078e984f33c67c9c27f1d53624a`

External candidate:
`P6MT_v4.6_tranche1_r3.zip`

SHA-256:
`a95b8b7405a74bbcbee8e9c7cc94c4506439ec3f04774815cc23ccfd74a0cb1d`

Qualified scope:

- **F1:** bounded/deduplicated WinnerTracker with monotonic revisions, stale-completion protection and at most one in-flight query per account.
- **F8:** typed winner parsing with versioned/loaded-address handling, explicit transaction identity/slot checks, target/receiver validation, failed-transaction filtering and fail-closed parsing.
- **P1-C:** executable-floor economics; quoted output never substitutes for an unknown executable minimum. Missing floor remains `Evaluation::Unknown` and cannot become actionable.
- Evidence marker: `route_min_out_unknown` in `CandidateCostRecord`.
- Three candidate-specific Clippy corrections: `ok_or`, `if let`, and boxing the large resolved `WinnerWatch` variant.

Final local acceptance on the committed code:

```text
cargo build --locked --workspace --features crate_health
PASS

cargo build --locked --workspace --features "crate_health txv1"
PASS

cargo test --locked --workspace --features crate_health
PASS

cargo test --locked --workspace --features "crate_health txv1"
PASS

F8_SDK_DECODE_PROBE ok=true err=None
test winner::tests::f8_probe_sdk_encoded_transaction_decoding ... ok
```

Strict Clippy 0.1.93 differential qualification:

```text
darkbot-runner baseline / crate_health            8 normalized lints
darkbot-runner candidate / crate_health           8 normalized lints
new candidate lints                               0

darkbot-runner baseline / crate_health + txv1     8 normalized lints
darkbot-runner candidate / crate_health + txv1    8 normalized lints
new candidate lints                               0
```

The eight remaining strict-Clippy findings are present on the exact pre-tranche base. This is **not** a claim that the complete workspace is strict-Clippy clean.

Evidence:
`project_0/docs/evidence/P6MT_T1_R3_20261007/`

Decision:
**P6-MT Tranche 1 r3 LOCAL CODE QUALIFICATION PASS.**

Scope boundary / non-claims:

- This F1 result is WinnerTracker hardening and does **not** close the separate audit findings P1-A/P1-B for candidate-admission revision/fairness.
- No Mainnet Shadow qualification is claimed by this entry.
- Full P6-MT remains incomplete.
- Drift qualification remains open.
- No landing/capture probability or realized Project-0 PnL is proven.
- LIVE remains blocked.


### 2026-10-08 — C0-E1 Fable handoff / 4h economic-race measurement preparation

Status: **HANDOFF READY / FABLE IMPLEMENTATION PENDING**

### Qualified source state

Authoritative C0-E1 implementation base:

`57bef334e60764f614712785438a95edb7638352`

This base contains qualified C0-MO r6.

C0-MO r6 local qualification:

```text
build crate_health                     PASS
build crate_health + txv1              PASS
workspace tests crate_health           PASS
workspace tests crate_health + txv1    PASS
rustfmt                                PASS
differential Clippy crate_health       candidate-only=0
differential Clippy crate_health+txv1  candidate-only=0
```

600 s Mainnet logs smoke:

```text
verdict=MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED
actual_observation_seconds=600.0
transactions_received=63
unique_transactions=63
successful_transactions=26
failed_transactions=37
liquidation_transactions=0
liquidation_events=0
gap_count=0
reconnect_count=0
ingress_overflow_count=0
decoder_error_count=0
logs_truncated_total=2
logs_truncated_qualified=2
logs_truncated_unqualified=0
logs_truncated_liquidation_mismatch=0
rpc_fallback_count=2
decode_complete=true
market_capture_complete=true
source_coverage_verified=false
incomplete_reasons=[]
FINAL_REPORT_KEYS=PASS
R6_600S_QUALIFICATION=PASS
```

The zero-liquidation 600 s smoke is a technical qualification, not economic evidence.

### Decision: expand the 4-hour gate before running it

The previously planned 4-hour C0-MO observation is not started yet.

Before spending the 4-hour window, the measurement is expanded to **C0-E1** so one run measures the complete decision funnel:

```text
Market Gross Edge
→ current Shadow Executable Net Edge
→ Funding / required capital
→ capital buckets 200/500/1000/2500/5000/10000 USD
→ actual winner / competition
→ pre/same/post winner-slot evidence
→ same-clock local race margin
→ required observed speedup for late opportunities
→ stage latency attribution
→ bottleneck edge attribution
→ runtime stability
```

C0-E1 must reuse the actual existing Project_0 pipeline and economics. Unknown costs/economics remain UNKNOWN.

No submission is allowed. LIVE remains blocked.

### Compiled Fable baseline package

File:

`DarkBot_Project0_C0E1_FULL_COMPILED_BASE_57bef33.zip`

SHA-256:

`e829116253cd14da0236e31b0b10dd469becb52dc5d309406a2758600634b850`

Bytes:

`1660074`

Base:

`57bef334e60764f614712785438a95edb7638352`

Marginfi pin:

`33c67987a661f59c6683a65485ebb4f0ef030b77` / `mrgn-0.1.11-rc1`

Packaging timestamp:

`2026-10-08T20:09:36Z`

Baseline machine/toolchain:

```text
macOS 15.6.1
Darwin 24.6.0
arm64
rustc 1.93.1
cargo 1.93.1
```

Packaging/build verification:

```text
SOURCE_BASE_PASS
SOURCE_ARCHIVE_PASS
MARGINFI_PIN_CONTEXT_PASS
PACKAGE_HYGIENE_PASS
cargo_metadata=PASS
build_crate_health=PASS
build_crate_health_txv1=PASS
test_crate_health=PASS
test_crate_health_txv1=PASS
clippy_crate_health=PASS
clippy_crate_health_txv1=PASS
release_p0_longtail=PASS
INTERNAL_SHA256_PASS
FABLE_PACKAGE_SIZE_PASS
```

The package proves only the supplied baseline. It does not prove Fable's modified candidate.

### Required post-Fable gates

After candidate return:

1. verify declared base `57bef334...`;
2. verify ZIP/internal SHA-256 and changed-file scope;
3. inspect secrets/generated data/binaries;
4. `git apply --check`;
5. apply locally;
6. `git diff --check` and exact-scope review;
7. authoritative builds for both feature sets;
8. both workspace test suites;
9. candidate formatting check;
10. differential Clippy, candidate-only warnings = 0;
11. C0-E1 accelerated smoke:
    - `P0_ECON_DURATION_SECS=600`
    - `P0_ECON_SNAPSHOT_SECS=150`
    - exactly four snapshots + final JSON;
12. only if that passes: real 4-hour C0-E1 run:
    - `P0_ECON_DURATION_SECS=14400`
    - `P0_ECON_SNAPSHOT_SECS=3600`;
13. record the four hourly JSON summaries + final decision metrics in the ledger;
14. derive Economic GO / CONDITIONAL / STOP;
15. prioritize MT6–MT10 from the measured dominant bottleneck, not assumption.

### Required 4-hour decision outputs

The final report must be sufficient without uploading raw 4-hour JSONL data to answer:

- observed Market Gross Edge and distribution;
- current known Project_0 executable net edge;
- required working capital / known fee reserve;
- edge by default capital budget;
- PRE / SAME / POST winner-slot counts and known net edge;
- unresolved race/economics/capital counts;
- local observed race-margin distributions;
- required observed speedup p50/p90/p99 for late opportunities;
- speedup sensitivity;
- Rx→Detect / Detect→Ready / Ready→Sign attribution;
- top bottlenecks by known blocked net edge;
- technical stability for the complete four hours.

### Non-claims

The test does not establish a true competitor send deadline, guaranteed Shadow capture, realized PnL, future daily/monthly profit, source coverage, or LIVE readiness.

**LIVE remains blocked.**


## 2026-10-08 — C0-E1 integrated locally into DarkBot

User authorized direct integration after isolated package delivery. Integration HEAD:
`12c4616f9b24bf036d99cdd549e56ff2d448d372`; implementation base:
`57bef334e60764f614712785438a95edb7638352`.

Only PHASENPLAN and this ledger had changed within Project_0 since that base.
The complete patch check correctly rejected the appended PHASENPLAN hunk; the
code patch passed `git apply --check --exclude=project_0/docs/PHASENPLAN.md` and
was applied unchanged. PHASENPLAN was merged manually, retaining the existing
handoff requirements. All nine other package files match the verified return
end_state byte-for-byte and in file permissions. Cargo.lock is unchanged.
Existing untracked BSC evidence and market data were preserved.

### Local qualification on the integrated working tree

Commands were executed from `project_0/native`:

```bash
cargo build --locked --features crate_health
cargo build --locked --features 'crate_health txv1'
cargo test --locked --workspace --features crate_health
cargo test --locked --workspace --features 'crate_health txv1'
```

- Both builds: PASS.
- Workspace `crate_health`: 242 PASS / 0 FAIL / 1 existing ignored test.
- Workspace `crate_health txv1`: 244 PASS / 0 FAIL / 1 existing ignored test.
- `git diff --check`: PASS.
- `bash -n project_0/scripts/run_econ4h.sh`: PASS.
- Targeted rustfmt (`econ.rs`, `econ_tests.rs`, `record.rs`, edition 2021,
  `skip_children=true`): PASS; this is not a full candidate-formatting claim.
- Package internal SHA256SUMS: all 13 PASS before integration.

Evidence: `project_0/evidence/C0_E1_local_integration_20261008/results.json`
and `1.log` through `4.log`. Historical implementation commands/results and
non-claims: `project_0/docs/C0_E1_IMPLEMENTATION_REPORT.md`.

### Handoff / exact next steps

1. Complete candidate formatting and differential Clippy versus the base for both feature sets.
2. Run combined Mainnet smoke from repository root:
   `P0_ECON_DURATION_SECS=600 P0_ECON_SNAPSHOT_SECS=150 project_0/scripts/run_econ4h.sh`.
3. Inspect all four snapshots and `economics_final.json`; require technical completeness,
   inspect UNKNOWN evidence and source identity before qualifying the run.
4. Only after successful smoke: `project_0/scripts/run_econ4h.sh` for 14400/3600 observation.

The user takes over further qualification to conserve credits. No combined Mainnet
smoke, four-hour observation, candidate release build or differential Clippy was
performed in this integration. Uncommitted source identity remains UNKNOWN as
implemented; version the reviewed candidate before a run intended to qualify a
specific source commit. No commit or push was made during integration.
No submission was performed. LIVE remains disabled. This is local code/test
qualification, not economic GO, source-coverage proof, P8 PASS or LIVE readiness.

## C0-E1 RETENTION QUALIFICATION — 2026-10-08

### Scope

C0-E1 combines C0-MO Market Truth with the existing Project_0 Shadow
production path:

Health → Sizing → Funding → Economics → Build → Sign → Simulation → Winner/F8

Submission remains disabled and LIVE remains blocked.

### Initial 600-second smoke

Run:

`c0e1_smoke_20261008T212122Z`

The full 600-second run completed with four snapshots, complete market
capture, clean shutdown, zero submissions and LIVE disabled, but the
technical verdict was `INCOMPLETE`.

Root cause:

- pipeline evaluations: 555626
- retention capacity: 262144
- retention overflow: 293482

Exact relation:

`555626 - 262144 = 293482`

The original C0-E1 analytics implementation retained effectively one
in-memory record per Shadow evaluation/revision. This was structurally
unsuitable for the planned four-hour run.

### Retention architecture fix

The analytics retention path was redesigned:

- every Shadow evaluation remains streamed to `shadow_evaluations.jsonl`;
- normal revisions are no longer retained one-for-one in RAM;
- bounded semantic state is retained per report-relevant opportunity group;
- revision coalescing does not count as evidence loss;
- `shadow_offsets.bin` provides disk-backed lookup for later Winner
  recorrelations;
- later Winner evidence can reconstruct relevant account history from disk;
- complete evaluation counters and hour deltas are retained;
- real semantic capacity loss remains fail-closed.

New telemetry:

- `shadow_evaluations_received`
- `shadow_evaluations_streamed`
- `shadow_evaluations_coalesced`
- `shadow_semantic_entries_current`
- `shadow_semantic_entries_max`
- `shadow_semantic_evictions`

### Stress qualification

Synthetic stress:

- evaluations received: 1000000
- evaluations streamed: 1000000
- evaluations coalesced: 999984
- maximum semantic RAM entries: 16
- retention overflow: 0
- semantic evictions: 0
- raw rows: 1000000

Targeted C0-E1 regression tests:

- 26 passed
- 0 failed

### Full local qualification

After the retention correction:

- rustfmt: PASS
- git diff check: PASS
- build `crate_health`: PASS
- build `crate_health txv1`: PASS
- full workspace tests `crate_health`: PASS
- full workspace tests `crate_health txv1`: PASS
- Differential Clippy `crate_health`: candidate-only 0
- Differential Clippy `crate_health txv1`: candidate-only 0

Result:

`C0E1_RETENTION_LOCAL_QUALIFICATION_PASS`

### Retention-safe 600-second Mainnet retest

Run:

`c0e1_retention_smoke_20261008T215650Z`

Result:

- runner exit: 0
- technical verdict: `PASS`
- actual duration: 600.0 s
- four snapshots present: true
- snapshot writes: 4
- final reconciliation: complete
- market capture complete: true
- clean automatic shutdown: true
- analytics queue drops: 0
- retention overflow: 0
- submissions: 0
- live_ready: false

Retention telemetry:

- evaluations received: 840518
- evaluations streamed: 840518
- evaluations coalesced: 816112
- semantic entries current: 24406
- semantic entries maximum: 24406
- semantic evictions: 0

Therefore all received Shadow evaluations were persisted by the analytics
layer and no semantic evidence was evicted.

The ten-minute economic sample contained no observed market liquidation:

- liquidations: 0
- executable opportunities: 0
- market status: `MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED`
- sample status: `INSUFFICIENT_SAMPLE`
- economics unknown: 24406
- winner unresolved: 24406
- exact competitor submission deadline: `UNKNOWN`

Observed bottleneck labels included:

- MissingAta
- Routing
- Bankruptcy

These are measurements of the current Shadow system, not conclusions about
their final economic importance.

### Qualification decision

C0-E1 600-second technical qualification: **PASS**

Original retention scalability blocker: **RESOLVED**

Still not established:

- representative profitability;
- verified complete market-source coverage;
- exact competitor submission deadlines;
- same-slot capture probability;
- four-hour stability;
- LIVE readiness.

Next gate:

**Four-hour C0-E1 Economic + Capital + Competition + Race Attribution run
from the exact committed and pushed source revision.**

LIVE and transaction submission remain blocked.

## C0-E1 QUALIFIED SOURCE CHECKPOINT — 2026-10-08

Authoritative qualified C0-E1 code commit:

`b7f5ed3850e0b1e412d39c673d89a8c8ffc3644a`

Commit message:

`project0: qualify C0-E1 retention-safe economics harness`

This exact revision passed the complete pre-4h qualification chain:

- retention architecture correction implemented;
- 1,000,000-evaluation retention stress PASS:
  - received 1,000,000;
  - streamed 1,000,000;
  - coalesced 999,984;
  - semantic max 16;
  - retention overflow 0;
  - semantic evictions 0;
- targeted C0-E1 regression suite: 26 passed, 0 failed;
- rustfmt PASS;
- git diff check PASS;
- build `crate_health` PASS;
- build `crate_health txv1` PASS;
- full workspace tests for both feature sets PASS;
- Differential Clippy against `57bef334e60764f614712785438a95edb7638352`: candidate-only 0/0;
- retention-safe Mainnet retest `c0e1_retention_smoke_20261008T215650Z`: technical PASS;
- actual duration 600.0 s;
- four snapshots present;
- final reconciliation complete;
- market capture complete;
- analytics queue drops 0;
- retention overflow 0;
- Shadow evaluations received/streamed: 840,518 / 840,518;
- Shadow evaluations coalesced: 816,112;
- semantic entries max: 24,406;
- semantic evictions: 0;
- submissions: 0;
- live_ready: false.

The preceding run `c0e1_smoke_20261008T212122Z` remains failure evidence:
technical `INCOMPLETE` because `retention_overflow=293482`.

Open non-claims:

- profitability is not yet established;
- the ten-minute retest had zero observed market liquidations and remains `INSUFFICIENT_SAMPLE`;
- market-source coverage remains `MARKET_DATA_SOURCE_COVERAGE_UNVERIFIED`;
- exact competitor submission deadlines remain `UNKNOWN`;
- same-slot capture probability is not established;
- four-hour stability is not yet established;
- LIVE readiness is not established.

Next gate:

Run the 14,400-second C0-E1 Economic + Capital + Competition + Race Attribution measurement with four 3,600-second snapshots, LIVE disabled and submissions disabled.

The 4h measurement must identify its tested Project_0 code source as `b7f5ed3850e0b1e412d39c673d89a8c8ffc3644a`.

Do not optimize observed Winner/Shadow bottlenecks before this measurement; they are part of the system being measured.

## C0-E1 FIRST 4H MAINNET OBSERVATION — INCOMPLETE (2026-10-09)

Run:

`c0e1_4h_20261008T222326Z`

Qualified code under test:

`b7f5ed3850e0b1e412d39c673d89a8c8ffc3644a`

Repository head during run:

`cb4528554948d094ba590d065b90bd41fa3dee08`

The repository-head delta from the qualified code commit contained documentation only.

### Runtime result

- requested duration: 14,400 s
- actual duration: 14,400.0 s
- four snapshots present: true
- final reconciliation complete: true
- clean automatic shutdown: true
- analytics queue drops: 0
- retention overflow: 0
- semantic evictions: 0
- submissions: 0
- live_ready: false
- Shadow evaluations received: 20,056,722
- Shadow evaluations streamed: 20,056,722

The retention redesign therefore survived the full four-hour load without loss.

### Market qualification failure

Technical verdict:

`INCOMPLETE`

Market verdict:

`MARKET_DATA_INCOMPLETE`

Exact market reason:

- `gap_count=1`
- `reconnect_count=1`
- `coverage_complete=false`
- `market_capture_complete=false`

Gap evidence:

- timestamp: 2026-10-09T00:10:54Z
- t_rx_ns: 6447081629916
- from_slot: 454701191
- to_slot: UNKNOWN at gap emission

Reconnect evidence:

- timestamp: 2026-10-09T00:10:55Z
- t_rx_ns: 6448207038375
- attempt: 2

Observed transport outage between GAP and StreamReady/Reconnected evidence was approximately
1.125408459 seconds.

The missing slot interval is not yet proven because the current `WsRpcControlSource`
emits `Gap { from_slot, to_slot: None }` on session failure and emits
`Reconnected` after subscription ACK completion, before a subsequent slot frame provides the
post-reconnect slot boundary.

### What remained healthy

- ingress overflow: 0
- fatal count: 0
- decoder complete: true
- decoder errors on successful transactions: 0
- drain complete: true
- enrichment complete: true
- truncated logs total: 81
- truncated logs qualified: 81
- truncated logs unqualified: 0
- truncated liquidation mismatch: 0

Therefore the run failed specifically on market-stream continuity, not retention, decode,
truncation qualification, drain, analytics queueing or shutdown.

### Economic interpretation

The report contained zero observed market liquidations, but this run MUST NOT be interpreted as
proof that no liquidation occurred during the four-hour window because one unbackfilled
WebSocket gap exists.

Gross edge, Market↔Shadow match counts, executable edge and race/capture conclusions from this run
are therefore not qualified economic evidence.

### Required correction before another 4h run

Do not weaken the fail-closed gate.

Add deterministic C0-MO gap recovery:

1. On `GAP`, preserve the last safe slot and mark a pending gap.
2. On successful reconnect, keep the gap pending.
3. Close the gap range using the first trustworthy post-reconnect slot frame.
4. Conservatively include boundary slots in recovery so partial-slot loss cannot be silently
   excluded.
5. Perform RPC backfill only after the capture phase / outside the capture hot path.
6. Reconstruct the missing Marginfi transaction evidence through the existing decoder/F8 truth.
7. Deduplicate against already captured signatures.
8. Mark a gap recovered only if every required slot is deterministically fetched and decoded.
9. Any unavailable/pruned/error/ambiguous slot remains fail-closed and leaves
   `market_capture_complete=false`.
10. Record explicit recovered/unrecovered gap telemetry and recovered liquidation counts.

Before repeating the 4h gate, qualify this correction with deterministic unit/regression tests and
a short Mainnet smoke including a controlled/forced reconnect.

Winner/Shadow performance bottlenecks observed during this run remain measurement targets and are
not to be optimized as part of the market gap-recovery fix.


## 2026-10-13 — C0-E1 P5 normalized reports / typed post-capture workflow

**IMPLEMENTED / LOCAL QUALIFICATION PASS.** P3 `aa37dd3` and P4 `2c62d13` reused. Evidence: `docs/evidence/C0E1_P5_20261013/` (commands, outputs, source/binary hashes, first failed timer fixture retained).

`test.sh`, `verify.sh`, `report.sh` share `MeasurementConfigV2`. Automatic existing verifier after drain: off/index/strict, with existing FREE_ONLY preflight. Normalized unfiltered records and immutable result-bound queries; all four classes, six markers, four individually selectable related operations; provenance/time/winner/competition/economics filters affect display only. All three renderers use identical records. Captured raw evidence remains immutable. Strict partial checkpoints are exposed for reporting without changing their proof rules.

`p0-econ-replay` calls the existing production canonical selector, cost/capital/loss/race functions on saved evidence. No RPC, new economics model or approximate repricing. Four MeasurementPlan snapshots and final report; canonical inputs absent => replay NOT_RUN/UNKNOWN. Shadow-only never a chain winner or realized profit. Historical verification never repairs the live-coverage axis. Report restart and wrong-run result rejection covered.

Tests: **44 Python PASS; runner lib 191 PASS / 0 FAIL / 1 pre-existing ignored; four release binaries PASS; changed-region rustfmt, shell syntax, diff whitespace PASS.** First runner attempt failed the fixed-sleep snapshot fixture; bounded observed-snapshot synchronization repaired the test, not production qualification.

**OPEN / NOT_RUN:** P6 full offline matrix next; real failed-5h replay reserved to user; provider quota/archive/WSS qualification and separately authorized Mainnet smoke NOT_RUN; historical 429 gate OPEN. No 4h/5h capture, LIVE or submission. No Master PASS / profitability claim. Next: P6 offline invariants and fixture replay; user subsequently performs actual-run replay and approved Mainnet qualification.


## 2026-10-13 — C0-E1 P6 offline invariants / handoff

P5 pushed: **`c092bcd9f7c27c0305203ef29a65214bef376e48`**. P6 local evidence: `docs/evidence/C0E1_P6_20261013/` (exact commands, raw outputs, source/binary hashes, machine-readable gates, user handoff).

**P6 OFFLINE PASS:** 52 Python tests; workspace `crate_health,txv1` **340 PASS / 0 FAIL / 1 existing ignored**; workspace `crate_health` **338 PASS / 0 FAIL / 1 existing ignored**. Four release binaries, changed-region format and shell checks PASS. Eight added P6 tests include 240 filter combinations × 3 renderers, immutable raw/proof/economics invariance, related-event negative/CPI/LUT fixtures, exact time filters, UNKNOWN-net preservation, stable capture/spec IDs across resume with separate attempt/result/query IDs, cached-proof/snapshot corruption rejection and strict→production-economics fixture replay. Existing strict interruption test additionally proves completed partial batches remain exposed after a later failure.

P6 fixes found by qualification: cached report revalidates nested strict evidence and snapshots; missing/disagreeing canonical event inputs cannot yield zero replay economics; boundary-second gap attribution stays UNKNOWN. No new verifier/economics engine. Live coverage is never upgraded by historic proof. Python fixture resource measurement: **3.55s / 47,742,976-byte maximum RSS**, not representative of a 4h multi-GB run.

**Overall P6/provider qualification remains OPEN.** Real failed-5h replay **NOT_RUN / USER-OWNED**, by explicit user instruction; provider WSS/RPC/free-quota/archive measurement **NOT_RUN**; Mainnet smoke **NOT_RUN / separate permission required**. Historical index/429 gate **OPEN**. No new long run, LIVE or submission. **No Master PASS, four-hour stability or profitability claim.**

**Exact next step:** user runs saved-data replay/report on their existing run; then measured FREE_ONLY provider qualification and explicitly authorized short read-only Mainnet smoke before a user-started four-hour capture. P1/P2/P3/P4 were not reimplemented. All original run data and BSC work preserved.


## 2026-10-13 — FE-00/FE-01 selection and source-trust correction

Base **28f6d53e779857a1b4342ced929bb61f8c35225d**, fast-forward only; BSC/raw runs preserved. FE-00 report and source-trust fix **IMPLEMENTED / LOCAL TEST PASS**. Raw source names (including `full_slot_reconciliation`) no longer qualify INDEX/STRICT; source requires immutable capture-bound spec/result proof, strict complete batch + files. Positive/negative regressions included.

Evidence: `docs/evidence/FE00_20261013/candidates.json` and adjacent logs. Six real historical TX signatures/slots decoded; R122 two historical technical ZERO_PREFUND successes; R124 three known negative edges: **−121889 / −118606 / −91691 USD-micro**. Historical != currently tradable. **NO_ECONOMIC_GO.** First class for requalification: **ZERO_PREFUND_SAME_TX**, existing atomic collateral exit + floor; NO_SWAP has no qualified executable exit, inventory UNKNOWN. JIT remains required when neither initial class qualifies; its ExactOut/absent funding-bank blockers retained.

Tests: **55 Python PASS; workspace 340 txv1 / 338 health PASS, 0 FAIL, 1 pre-existing ignored each; both release builds PASS.** Clippy baseline warnings retained; compiled Rust unchanged in this package (differential delta zero). No fresh Mainnet RPC or 5h replay. FE-01 current deployment/state and FE-02 profitable complete simulation **NOT_RUN / authorization-dependent**, no PILOT_READY. Independent FE-04 persistent one-shot reservation work continues. LIVE/SUBMISSION=false.


## 2026-10-13 — FE-04 durable hold foundation; pilot remains blocked

FE-00 commit pushed **b1e69300fe520d12a6fce5a9358c117b3b19124d**. Independent FE-04 subpackage: versioned approval/intent, per-wallet lock, atomic/fsynced hold before handoff, expiry/code/build/program/funding/target/wire/signature/budget checks, no duplicate callback, UNKNOWN after crash/restart/timeout, complete finalized member/fee/delta/exit requirement, permanent one-shot HALT. Existing Safety now keeps its hold on `record_outcome(None)`; later complete reconciliation never auto-clears HALT.

Evidence `docs/evidence/FE04_LOCAL_20261013/`. Full workspace: **346 txv1 / 344 health PASS, 0 FAIL, 1 existing ignored each**; after adding bundle partial/duplicate-membership regression, final journal subset **6 PASS in each feature set**. Both release builds PASS; production code unchanged by final test addition. Changed-region format checked; differential Clippy recorded separately. Support Python suite **59 PASS**.

**FE-04 overall PARTIAL:** durable backend is not a runnable authorized LIVE pilot yet. Still open: terminal approval lifecycle, current production evidence/intent assembly and exact adapter binding, finalized chain-backed wallet/exit receipts. Core structs/fixtures are NOT proof of actual chain execution. Normal `P0_MODE=live` remains refused. FE-07 realized PnL NOT_RUN. No authorizing production profile, submission or new capture.

FE-01 limited read-only observation (3 RPC, 1 rps, 30s, 16MiB; program/ProgramData/two R122 accounts) prepared using the existing bounded RPC transport, user permission requested and pending. No actual Mainnet request executed here. FE-00 still NO_ECONOMIC_GO; no current positive candidate or source parity. Exact next step: authorized bounded deployment/state observation, then candidate-specific current parity/health/funding/route/fee/full-simulation proof and finish pilot integration. Large 5h replay remains user-owned.

## 2026-10-13 — FE-04 exact signed adapter binding; runtime integration remains OPEN

- Closed the public hash/callback bypass: journal reservation and submission require an immutable signed payload and a non-deserializable simulation token from the existing adapter. Payer/signatures, RPC/Jito path, ordered bundle membership and every wire byte are bound. The same adapter simulates and submits exactly that payload.
- UNKNOWN is durable before handoff. ACK, rejection, timeout and mismatched ACK never release holds or rearm; all member signatures remain required for reconciliation. Normal LIVE stays disabled.
- LOCAL PASS: workspace txv1 350 / health 348, zero failures, one pre-existing ignored each; Python 59; both release builds; changed-region format/diff; Differential Clippy 38→38, zero added warning keys. Nine journal tests include real local signing and mock adapters, not on-chain execution.
- Evidence: `project_0/docs/evidence/FE04_BINDING_20261013/` (commands, complete test/build logs, warning comparison). No Mainnet calls and no 5h replay.
- FE-04 remains PARTIAL_LOCAL: terminal approval lifecycle, production Intent/Current ownership immediately at handoff and finalized token/fee/exit reconciliation remain open. FE-01/02 positive current economic proof and FE-05/06 permission remain unresolved; no realized profit claim.
- Next: complete finalized receipt parsing and production-owned pilot integration; bounded current Mainnet observation only after explicit permission. JSON source hash/status/references refreshed with this delivery; phase plan unchanged.

## 2026-10-13 — FE-04 finalized fee/wallet observation; exit accounting remains OPEN

- Reused RPC reconciliation metadata transport with an explicit finalized-only entrypoint. Parser binds signature, intent minimum slot, wallet payer and raw/loaded account mapping. Missing fee/metadata never becomes zero. Existing confirmed reconciliation behavior is preserved.
- Fee = meta.fee on success/revert; SOL delta already includes network fees/rent/transfers. Token amounts remain exact integers, aggregated by mint/owner. Missing/ambiguous token metadata, invalid indices, changed ownership/mints and unproven create/close transitions remain UNKNOWN. No price/PnL approximation.
- Journal persists finalized member observations without releasing holds. Partial bundle results and unknown token deltas remain HandoffUnknown; old journals without observation fields retain their hold. Normal LIVE remains blocked.
- LOCAL PASS: 17 targeted journal/receipt tests; final workspace txv1 358 / health 356, zero failures, one existing ignored each; both release builds; changed-region format/diff; Differential Clippy 38→38, no added warnings. Python code unchanged from the preceding 59 PASS package. Local mock/loopback tests only, no Mainnet or 5h replay.
- Evidence: `project_0/docs/evidence/FE04_RECEIPTS_20261013/`. Preceding signed adapter binding committed/pushed as `48db13c90a638e6034b7a8d39689a2c9368a2bd7`.
- FE-04 still PARTIAL_LOCAL: terminal approval lifecycle, production-owned Intent/Current immediately at the actual pipeline boundary, qualified exit and complete attempt/cumulative-loss accounting remain OPEN. Fee/token observations alone are not realized profit or exit proof. FE-01/02/05/06 gates unchanged.
- Next: integrate those runtime/approval/exit gates using the existing pipeline; obtain separately authorized current parity/candidate evidence. Do not infer a LIVE permit from these offline tests. Markdown/JSON source hash and section references updated together.


### 2026-10-14 — Configurable operational contract / FE continuation (LOCAL PASS)

The initial FE smoke envelope is now a configurable profile, not a permanent product limit. The existing Rust provider budget, WSS ingress, runner, MeasurementPlan and Python measurement/preflight bridge consume a shared typed operational contract. Profiles, explicit endpoints/fallback priority, rates/burst/concurrency/queue, WSS capacities, timeouts/recovery, byte/call budgets, CPU workers, phase/verification budgets and diagnostics have help/effective-config metadata. Finance/submission authorization remains separate. Private config handoffs are owner-only, ephemeral and redacted in evidence. Empty endpoint arrays inherit the launcher's local endpoints; an explicit operational endpoint wins over the legacy audit alias so qualification and Strict use the same provider identity.

Bounded priority admission, adaptive overload backoff, common Retry-After, socket/subscription leases and chunked payload accounting reuse the current transport. Calls through asynchronous RPC/WSS and blocking history/enrichment share the Rust process budget. Python observation/index has its own bounded serial phase. Higher selected rates never become provider proof. Verification preflight now checks the requested rate rather than the previous implicit 2-RPS cap. Raw market capture, economics UNKNOWN handling, single-flight and LIVE restrictions remain intact.

Qualification: **370 txv1 / 368 crate_health Rust tests PASS, 0 FAIL, 1 existing ignored per matrix; 65 Python tests PASS; both release builds PASS; changed-region format/diff checks PASS; all-targets differential Clippy 38 baseline / 38 current, 0 additions**. Loopback fixtures exercise 429, retry/fallback and no send retry, capacity/resource exhaustion, config precedence/redaction/private cleanup, odd/long duration boundaries, bootstrap timing, terminal inspection without starting a run, and insufficient provider qualification. Existing offline report/recovery/economics regressions remain green. The preview checkpoint `e01560f` was preserved and followed by the endpoint-identity correction and complete qualification evidence. No Mainnet calls, 5h replay, long capture or submission occurred.

Evidence/commands/source hashes: `project_0/docs/evidence/OPERATIONS_CONFIG_20261014/`. Usage/scope: `project_0/docs/OPERATIONAL_CONFIG.md`. Only Project_0 paths were changed. This is LOCAL_PASS for the operational package, not provider/Mainnet/profit qualification. FE-01/02 remain OPEN; FE-04 remains PARTIAL_LOCAL (terminal approval lifecycle, production Intent/Current assembly, qualified exit, complete attempt and cumulative realized-PnL accounting). FE-03/05 real Shadow/provider evidence and FE-06 money authorization remain separate gates. Race/Jito/JIT/M4 obligations remain binding.

Exact next gate: separately authorized read-only FE-01 observation using explicitly reviewed effective settings (smoke defaults are available but configurable), then current source/state parity, full selected transaction simulation and conservative exit/net-edge proof. Never convert profile selection, technical simulation or unpriced assets into positive economic/realized-profit claims.

### 2026-10-16 — PERSIST binary journal integration (`4a27a1c`): PARTIAL / no LIVE

Source of truth for this dated delivery: GitHub `main` commit `4a27a1c6ab4032b322d5d6be3eb0b10c0d9d0d91`, [implementation report](development_history/persist_binary_completion_20261016.md), [qualifications](evidence/PERSIST_BINARY_20261016/qualification.json). The exact binding contract is `PHASENPLAN.md` PERSIST-01–05, lines 5247–5261. The delivery changed 47 `project_0/` files, no `bsc/` files. Current branch was confirmed at this commit.

**PERSIST overall PARTIAL — not QUALIFIED.** Existing `analytics/journal.rs` / `analytics/evidence.rs` extended to the single binary `.p0j` contract; new operational records for Shadow Owner, Market Capture, C0-E1 economic workers/phases, MT0, FE-01 read-only collection, R126/R127 Census and FE-04 durable wallet transitions use journals. Reader, deterministic offline view export and replay reuse the same contract. Historical JSON evidence/strict identities remain preserved.

- **PERSIST-01 PARTIAL:** production streams listed above are journal-backed; the hash-pinned strict verifier still uses `market_verification.rs` / `JsonlSink` for batch evidence. A re-pin requires reviewed identity migration, test parity and explicit scope decision; do not re-pin silently. This is not proof that every active consumer has migrated.
- **PERSIST-02 LOCAL_PASS / live tails NOT_MEASURED:** owner source guard and bounded typed evidence queue tested; no synchronously emitted owner JSON/FS/log. Capturer/recorder I/O runs outside owner. No Mainnet-load measurement or LIVE admission proof.
- **PERSIST-03 PARTIAL_LOCAL:** FE-04 wallet transition journal durably syncs before the one-shot handoff, preserves UNKNOWN and rejects corrupt/truncated transitions. Tests cover several crash/double-send cases. Actual filesystem fsync-failure injection NOT_RUN; one-shot pilot integration, exit/accounting and finalized live chain proof still OPEN. No value-bearing submission authorized.
- **PERSIST-04 LOCAL_PASS:** historical immutable JSON/JSONL and pinned specs preserved; new JSON/JSONL reports generated only as deterministic offline/legacy views with byte-equality checks. Those views are not a second authoritative primary journal.
- **PERSIST-05 PARTIAL:** locally reported Rust `crate_health` 415 PASS / 0 FAIL / 2 ignored; `crate_health,txv1` 417 PASS / 0 FAIL / 2 ignored; Python 107 PASS, verifier selftest 29 PASS, census selftest PASS, both release builds and featureless check PASS, differential Clippy 41 unique old → 38 current (0 additions). Qualifying Mainnet load/coverage/latency is NOT_RUN. The local benchmarks are synthetic, not production measurements.

**Synthetic local writer sample (M4 Max/APFS, release):** market 65,111 frames/s; append p50 14.7 µs/p99 24.7 µs; size 1.077× equivalent JSONL. Opportunity 111,481 frames/s; p50 8.4 µs/p99 11.8 µs; size 1.746×. MT0 2.541× JSONL. FE-04 append+fsync p50 4.23 ms / p99 5.31 ms. Thus no proven disk savings, no qualified trading-hot-path speed gain and no proof of sub-ms financial handoff.

**FE-01 still OPEN:** offline replay and integration-account observability improved, built TX pinned to V0; current deployed ProgramData/source/group/bank/oracle/integration parity and actual cluster TX support NOT_RUN/NOT_PROVEN. Requires separately authorized bounded FREE_ONLY Mainnet observation.
**FE-04 still PARTIAL_LOCAL:** binary durable transitions are real local code, but production-owned Intent/Current construction, terminal approval lifecycle, actual exit, cumulative-loss and realized-PnL accounting not qualified; the test helper is not a LIVE caller.
**Other gates unchanged:** FE-00 NO_ECONOMIC_GO; FE-02/03 OPEN; FE-05 NOT_RUN; FE-06 NOT_AUTHORIZED; FE-07 NOT_RUN; C0-E1 market master NOT_QUALIFIED; 72h Shadow NOT_STARTED. No Mainnet calls, no LIVE/submission or economic profit claims from this delivery.

**Next targeted engineering:** Review whether pinned Strict JSONL is only immutable compatibility output or an active primary writer; if the latter, implement a separately reviewed re-pin/migration with stable/verifiable old identities. Exercise fsync fault injection using controlled I/O abstraction or a suitable test filesystem. Refresh documentation (this entry/JSON index/STATUS), then prioritize FE-01 current parity and FE-02 executable economics; preserve all authorization and provider gates.


## 2026-10-16 — PERSIST strict binary / fsync errors / FE-01 replay follow-up (local)

Base `9047c7b`; new uncommitted local work, not a rewrite of the preceding `4a27a1c` evidence. [Report](development_history/persist_followup_20261016.md), [qualification/logs](evidence/PERSIST_FOLLOWUP_20261016/qualification.json).

Strict batch primary evidence now uses existing `JournalSink`/`market.p0j`; terminal flush derives byte-identical legacy views. Pinned verifier sources and historical contracts unchanged; new binary digests still flow through the existing spec identity mechanism. Deterministic test-only I/O-boundary fsync errors (Other/StorageFull) prove blocked SEND, sticky failure and UNKNOWN recovery with preserved holds. Physical filesystem/power-loss fault experiment NOT_RUN.

FE-01 replay requires complete ordered sealed archives and exact read counts; all frame bytes and read limits bounded before accumulation. Missing seals/results, INCOMPLETE, reordered/corrupt archives and limit violations rejected. Historical replay parity retained, never current funding qualification.

Final offline validation: Rust **421/423 PASS**, 0 FAIL, 2 pre-existing ignored each; both release builds and featureless check PASS; Python/verifier/census PASS; Clippy both configurations exit 0 (warning inventories attached). Initial import/test-fixture failures were corrected before these final results. Existing synthetic benchmark not rerun; Mainnet tails NOT_MEASURED.

**Overall PERSIST PARTIAL; FE-01 OPEN; FE-04 PARTIAL_LOCAL.** Current source/state/funding proof, production pilot/accounting integration and real-load qualification remain open. No Mainnet/LIVE/submission, staging, commit or push; BSC unchanged. Next: authorized bounded FREE_ONLY current observation, then candidate-specific qualification; do not infer authorization from this local pass.

## 2026-10-16 — P0 reuse / license / routing / oracle / staging: PASS

Base `c476a37`. [Gate verdict and routing audit](evidence/P0_20261016/README.md), [reuse matrix](evidence/P0_20261016/REUSE_MATRIX.md), [license matrix](evidence/P0_20261016/LICENSE_MATRIX.md), [oracle model](evidence/P0_20261016/ORACLE_MODEL.md), [staging model](evidence/P0_20261016/STAGING_MODEL.md).

All seven P0 gate items PASS; existing reuse decisions unchanged. All 669 third-party Rust packages permissive. `solana-dex-superagg` stays unlinked (returns finished transactions, no allowlist check); DarkBot keeps Jupiter `/swap/v2/build` (FREE_ONLY 1 RPS, not a hot-path stream); DFlow not allowlisted. Staging (`stag8s…`) needs a `staging` protocol build plus allowlist change, implemented with P5/P8. Provider items marked UNKNOWN are re-check items before enabling the respective path. No Mainnet, staging or provider API call; no code change.

## 2026-10-16 — P1A current Mainnet freeze: source side PASS, gate BLOCKED (current read)

Base `87da67d`. [Manifest](manifests/CURRENT_MAINNET.md).

- P1A-01: typed Loader-v3 decoder in `protocol.rs` (`decode_program`, `decode_program_data`: deployment slot, upgrade authority, full-account hash); FE-01 observation, market deployment check and Shadow startup now share it. Current Mainnet value NOT_RUN (permission).
- P1A-02: source pin `mrgn-0.1.11-rc1`/`33c67987`, program ID, account/instruction discriminators and Bank 1856 B tested against the type crate and `Cargo.lock`. Pin has no marginfi IDL; IDL hash NOT_RUN.
- P1A-03: receivership/refresh allowlists source-pinned; instruction set tested against `ix_discriminators`.
- P1A-04: upgrade watch closed at startup: a ProgramData other than the pinned reference `H57mqz…` (or an unpinned program) latches PROGRAM_UPGRADE HALT before detection; runtime latch unchanged.

Tests (targeted): `darkbot-project0` protocol 4/4 PASS (`crate_health`); `darkbot-runner` fe_qualification 9/9 and market paths 2/2 PASS (`crate_health,txv1`). No Mainnet call, no LIVE.

**Gate P1A BLOCKED:** one authorized finalized read of program + ProgramData is needed to confirm the deployment running today.

## 2026-10-16 — P1B 0.1.12 freeze: tickets done, trial build against 0.1.12 NOT_RUN

Base `52ad582`. [Manifest and promotion procedure](manifests/UPCOMING_0_1_12.md).

- Fail-open fixed: Health observation, dependency discovery, sizing, receivership builder, Shadow bank helper and fixtures decoded any Bank of at least 1864 B by its first 1856 B. A 0.1.12-resized bank (3904 B, byte-identical prefix) would have been read without its premium state. All now use `protocol::decode_bank`: exact `8 + size_of::<Bank>()` of the pinned crate, otherwise `UNKNOWN_LAYOUT` (`OtherVersion`/`Length`/`Discriminator`).
- P1A-04 follow-up: the runtime upgrade HALT now logs the decoded new deployment (slot, authority, hash).
- P1B-02 tests: dual-size decoder and a Health-level resized-bank test. P1B-03: promotion procedure with test tripwires (source pin, bank size, reference hash).

Tests (targeted): `darkbot-project0` lib 65/65 PASS (`crate_health`); `darkbot-runner` builder/fixture/state_owner 21/21 PASS (`crate_health,txv1`); `cargo check` featureless project0 and runner `crate_health` PASS. No Mainnet call.

**Gate P1B PARTIAL:** upgrade-safe (halt + no misread), but a trial build against the deployed 0.1.12 tag is NOT_RUN until that tag is released and deployed.

## 2026-10-16 — P1B RC2 probe build: LOCAL_PASS, deployment BLOCKED

Base `25eeea5`. [Manifest RC2 section](manifests/UPCOMING_0_1_12.md). Isolated scratch copy, productive 0.1.11 pin unchanged.

`darkbot-project0` + `darkbot-runner` (all targets, `crate_health,txv1`) type-check against `mrgn-0.1.12-rc2` (`9fcb729`) after exactly 4 call-site argument additions (`premium_scratch`, `ignore_pt_emergency` ×3). Promotion tripwires fire against RC2 as designed (Bank 3904 ≠ 1856; 1864 B bank no longer decodes). RC2 IDL `marginfi.json` SHA-256 `b4f80a6c…72fb2`: 5 account and 8 receivership instruction discriminators equal `protocol.rs`; new tag/close liq-record instructions; `repay_all: Option<bool>`. Premium-aware Health/repay semantics recorded as promotion work. No Mainnet call.

## 2026-10-16 — C0 market census: classifier implemented, gate NOT_PROVEN

Base `581237e`. [C0_MARKET_CENSUS](evidence/C0_MARKET_CENSUS.md).

- New `darkbot_project0::census` (typed, no JSON): healthy / premium_driven / race_now / long_tail (`t_profitable`, ≤1 h/24 h/7 d) / uneconomic / bad_debt / unsupported by reason (incl. `UnknownLayout` for 0.1.12-resized banks), with equity USD sums per class. Explicit cost model, no defaults.
- Shadow hook: one census at the first READY with `P0_C0_CENSUS=1` and mandatory `P0_C0_REALIZABLE_BPS`, `P0_C0_FIXED_COST_USD_MICRO`, `P0_C0_MIN_PROFIT_USD_MICRO`; one `c0_census:` log line.
- Existing evidence reused and labelled historical: C0-01 census (slot 452723812), group flags (slot 452727389), 4 h run rejection mix (24,569 opportunities, executable edge 0, market liquidations 0 / data incomplete).

Tests: `census::tests` 2/2 PASS; `cargo check` runner `crate_health,txv1` all targets and featureless PASS. No Mainnet call.

**Gate C0 NOT_PROVEN:** current census run needs Mainnet read-only permission and a measured cost model.

## 2026-10-16 — C0-FIX-01..05: non-blocking census, chain clock, no zero fallbacks (LOCAL_PASS)

Base `50a2767`. [C0_MARKET_CENSUS](evidence/C0_MARKET_CENSUS.md).

- C0-FIX-01: the synchronous owner census is removed. The R127 startup sweep (already one parallel Health pass over all debtors) builds census rows for the armed batch; a single-slot bounded worker classifies/aggregates. No second scan, no state copy, no RPC. Coverage gap, full queue, clock or FeeState error → INCOMPLETE with reason, request kept for the next READY.
- C0-FIX-02: premium decay, facts and Health errors are reasons (`Premium`, `Facts`, …), never zero.
- C0-FIX-03: Clock sysvar added as required dependency; sweep, threshold set/bootstrap, oracle projection, detect and census evaluate at its slot/unix time. Missing clock → evaluation error / detect SKIP, never local or zero time. Bookkeeping wall time (`detect`, `remote_apply`) errors instead of `unwrap_or(0)`.
- C0-FIX-04: per-account facts (banks, oracle setups, integrations, e-mode, observation/TX load, distance to threshold, value); 0.1.12-only facts are `None`.
- C0-FIX-05: classes renamed `model_*`, output marked `classes=MODEL_NOT_EXECUTABLE`; model validated (realizable 1..=10 000 bps, cost > 0, profit floor ≥ 0, FeeState ≤ 100 %).

Tests: project0 `census::tests` 3/3; runner `census::tests` 2/2, full runner lib 270/270 (2 pre-existing ignored), `crate_health,txv1` and featureless check PASS. Owner cost of the census batch is reported per run (`owner_batch_ms`), not measured on Mainnet (NOT_MEASURED).

Open, outside C0: `detect` builder uses `Pubkey::default()` when the FeeState read fails (P5); `remote_workers` reconcile TTL still uses wall time with `unwrap_or(0)` (P8). Gate C0 unchanged: NOT_PROVEN.

## 2026-10-16 — JFI-FIX-01/02: typed journal payloads, owner evidence migrated (LOCAL_PASS, JFI overall OPEN)

Code in `d3dabb3` (on top of `c3698bc`).

- JFI-FIX-01: `analytics/payload.rs` — one `Payload` type per kind (bincode: LE, fixint incl. exact i128/u128, no trailing bytes, frame-size limit); frame `schema` = payload version (≥ 2), any other version, unregistered typed kind or truncation fails closed on read. `.p0j` header, sequence, CRC, hash chain and provenance unchanged. Manifest is typed. Schema 1 (value tree) stays readable only through `Frame::legacy_value` and is written only by `append_legacy` for kinds not migrated yet. `p0-journal frames` prints typed frames as text.
- JFI-FIX-02: owner evidence (`OWNER_RECORD/OPPORTUNITY/RECONCILE/SUMMARY/DIAGNOSTIC/RECOVERY`) encoded directly from the native events on the writer thread; frame provenance from the typed event, no JSON lookup. Owner side unchanged (bounded queue, no I/O). Static strings stay borrowed (`Cow`); the internally tagged JSON `Winner` gets a native `RaceWinner` payload; `DaySummary.losses` is a `BTreeMap` (deterministic bytes). No reader consumed owner records as JSON (no render, no Python export).
- Local writer-thread measurement (release, synthetic, 50 000 owner opportunity frames): JSON value path p50/p95/p99/max 9.5/12.7/16.3/361 µs, 1141 B/frame; typed 2.5/4.6/5.2/192 µs, 447 B/frame. Not a Mainnet measurement.

Tests: runner analytics 75/75 (2 ignored benchmarks), typed fail-closed test, full runner lib 267/270 before the three legacy-reading evidence tests were moved to typed reads (all pass after). Featureless check PASS.

**JFI overall OPEN.** Still schema 1 (`append_legacy`/`legacy_value`): FE-04 transitions and legacy import (JFI-FIX-06), market capture rows (JFI-FIX-03), C0-E1 econ kinds and their renders for the pinned Python verifier (JFI-FIX-05), measurement phases, MT0 snapshots, FE-01 reads/results, C0 census import; runtime JSON profiles and measurement plans (JFI-FIX-04); Jupiter REST (JFI-FIX-07, decision pending).

## 2026-10-16 — Ticket 1: journal write admission against legacy rollback (LOCAL_PASS, JFI overall OPEN)

Base `91380ad`.

- `payload.rs` holds the one registry: typed kind → payload type → schema version, plus `LEGACY_WRITABLE`, the explicit list of temporary migration paths (FE-04, market capture, econ, measurement, MT0, FE-01, C0 census import — each an open JFI item; kinds only leave it).
- `Writer::append` admits a typed kind only in its registered version and schema 1 only on a migration path; a schema-1 write of a migrated kind (`LEGACY_WRITE_OF_TYPED_KIND`, incl. manifest), an unregistered kind or a wrong version fails closed and leaves the journal usable. Readers keep accepting historical schema-1 frames of every kind (offline compatibility); frame structure, CRC, hash chain, manifest and provenance unchanged.
- Tests: native write / refused rollback / unregistered kind / wrong version; historical schema-1 frame of a migrated kind replays via `legacy_value` and is not decodable as typed; registry invariant (every kind typed xor migration path). Tests that need foreign frames on disk write them through test-only writers.

Tests: journal+evidence 23/23, one_shot+fe_replay 26/26, full runner lib 273 PASS / 3 ignored after fixes; all-targets and featureless check PASS.

## 2026-10-16 — Ticket 2: C0 census identity coverage and typed per-account evidence (LOCAL_PASS, C0 NOT_PROVEN)

Base `f0da568`. [C0_MARKET_CENSUS](evidence/C0_MARKET_CENSUS.md).

- Fixed: `expected = indexed_accounts` counted every group account (also without liabilities or active balances) while the sweep fan-out reached only accounts on decodable dirty banks — a count match could hide missing debtors or include non-debtors.
- Population (debtors + undecodable group members) is collected in the existing reverse-index scan; the armed startup sweep evaluates each debtor exactly once (debtors outside the fan-out added); the worker proves identity coverage (missing / extra / duplicate pubkeys) and journals one typed row per account plus a typed result frame (`C0_CENSUS_ROW` 902, `C0_CENSUS_RESULT` 903, attempt journals never overwritten, sealed COMPLETE only without gap).
- `census::usd_micro` returns `None` instead of saturating; unrepresentable values are `unsupported_ValueRange`.
- C0 with threshold-trigger mode is refused at startup (`C0_CENSUS_REQUIRES_R127_SWEEP_MODE`); no second detection pass.
- Local sizing: 82 782 synthetic rows → worker 232 ms incl. 30.6 MB typed journal and fsync; ≈ 17.2 MB row memory. Owner batch cost per run in `owner_batch_ms`; Mainnet NOT_MEASURED.

Tests: project0 census 3/3, runner census 3/3, project0 lib 68/68, runner lib 274/274 (4 ignored). One fe_replay failure in the first full run came from temp directories left by an earlier failed run (deterministic `Pubkey::new_unique` names); removed, test passes.

**C0 gate NOT_PROVEN** (no current Mainnet run; separate bounded read-only permission required).

## 2026-10-17 — Ticket 3 step 1–2: one typed chain transaction model; winner `meta.err` fix (LOCAL_PASS, Ticket 3 OPEN)

Base `3ef129c`.

- New `darkbot_solana::chain` (JFI-02 wire boundary): `ChainTx` / `ChainBlock` from `getTransaction`, `getBlock` entries and `transactionNotification`; JSON only in private wire structs. Keys as `Pubkey`, instruction data decoded, LUTs + loaded addresses, inner instructions with stack height, logs, fee, SOL and token balances (exact atomic `u64`), compute units, `costUnits`, v1 `transactionConfig`. Status keeps `err: null` = success, error = failed, absent `err` / absent `meta` / undecodable error = UNKNOWN (never success). Block entries keep their index; undecodable entries stay visible errors.
- The former half-typed `RpcTransaction` in `winner.rs` is replaced (no second model). **Fixed:** `#[serde(default)] err: Option<Value>` made an absent `err` look successful and could produce a winner; `end_liquidation_hit` now returns `StatusUnknown`.
- `qualify_truncated`, gap recovery, `validate_full_transaction`, `market_truth` (CPI walk, logs, status, fee, index, native evidence, visible payments) and enrichment read `ChainTx`. `TruthEvent.native_evidence` / `visible_payments` are typed (report keys unchanged). Enrichment now reads v1 compute limit and priority fee from `transactionConfig` (solana-message 4.4.1: priority fee in lamports) — previously `None` for v1; verified on the real v1 audit fixtures (fee 19 000 = 5 000 + 14 000).
- Still JSON in Ticket 3 scope: ingress `raw` payloads, market sink rows, block envelopes in gap/reconcile fetch, `Subscription.program_filters`, verifier/report renders.

Tests: chain 4/4 on real Mainnet v1/v0 fixtures (status table, LUTs, exact units, block entries); winner 33/33 incl. new absent-`err` regression; market 113/113; runner lib 274/274 (4 ignored); solana lib 70/70; featureless check PASS. `market_truth.rs` was not rustfmt-clean before; part of its diff is formatting only.

## 2026-10-17 — Ticket 3 step 3: typed block envelopes in gap and reconcile (LOCAL_PASS, Ticket 3 OPEN)

Base `bb5a11a`. Gap recovery and full-slot reconciliation decode each `getBlock` answer once into `ChainBlock` (`decode_block`, new `decode_block_header` for header-only answers); header checks, validation, classification and recovery read the typed block and its entries (an undecodable entry fails the block closed with its reason). `validate_full_transaction` and `truth::classify` take `&ChainTx`. `previousBlockhash` is optional in the model and required, as before, only in the reconcile parent-chain proof. Reason codes unchanged. Legacy market rows still carry their raw payload until the typed rows (step 4) replace them — no evidence is dropped in between.

Tests: market 113/113 (gap, reconcile, truth through decode-then-call helpers), chain 4/4, featureless check PASS.

## 2026-10-17 — Ticket 3: typed program filters; storable chain model (LOCAL_PASS, Ticket 3 OPEN)

Base `ae13ab2`. `68fb932`: `Subscription.program_filters` and `getProgramAccounts` filters are typed `ProgramFilter` (`dataSize` / `memcmp` bytes); filter JSON exists only in the WSS and snapshot adapters, byte-identical to before (test). Follow-up: `ChainTx` and its parts are binary-serializable for the typed market rows; `TxStatus` stores its UNKNOWN reasons only from the closed code set (forged codes fail to decode).

Tests: solana lib 71/71 incl. filter wire parity and ChainTx bincode round trip; state_owner 13/13; featureless check PASS.

**Remaining in Ticket 3 (scope confirmed):** typed `MARKET_TRANSACTION` / `MARKET_LIQUIDATION` / `MARKET_COVERAGE` rows with ingress payloads as `ChainTx` (no `raw`), ~20 coverage variants incl. gap records, slot boundaries, reconciliation summary and provider snapshot (still built with `json!`); the Rust strict verifier batches and checkpoints (JSONL/JSON today); `summary.json`/report; a versioned Rust verification with parity against the pinned Python verifier on historical runs; renders only offline for historical journals. New captures cannot be verified by the pinned Python pipeline once rows are typed, so rows, strict verifier and the new verification land together.

## 2026-10-17 — P5 fee wallet and P8 reconcile clock fail closed; STATUS §12 Ticket 3 / SK-01…09 corrected (LOCAL_PASS for the two fixes)

Base `d72e180` (latest project_0 code `12523e5`; `8f3280e`/`d72e180` are BSC-only).

- **P5 (confirmed defect, fixed):** `detect.rs` replaced an unreadable FeeState with `Pubkey::default()` as the EndLiquidation fee receiver. `P0ReceivershipBuilder.global_fee_wallet` is now the FeeState read result; `plans()` and the receivership build fail with `FEE_STATE_UNAVAILABLE:<reason>` before sizing/routing. The pipeline records such a rejection as `Decision::Unavailable` / `LossReason::Unknown` — never a judged skip, never an executable candidate or positive net edge with an unknown mandatory fee. Full P5 acceptance remains OPEN.
- **P8 (confirmed defect, fixed):** the reconcile worker passed epoch 0 to `try_reconcile` when the wall clock was unreadable. The worker now reports `WALL_CLOCK_BEFORE_UNIX_EPOCH` instead of running; the owner keeps the submission pending (exposure stays held) and stops with `RECONCILE_CLOCK_UNAVAILABLE` (HALT). Full P8 acceptance remains OPEN.
- Removed dead winner code left from the typed chain model (`parse_key`, `InvalidInstructionData`).
- STATUS §12: snapshot 2026-10-17; Ticket 3 A0 (chain model, `meta.err`, v0/v1/LUT/CPI, typed `getBlock`, `ProgramFilter`, storable `ChainTx`) LOCAL_PASS per the three dated entries above — no rebuild; A1 raw-free ingress, A2 native `MARKET_*` frames, A3 native strict verifier/reports, A4 parity listed OPEN; JFI-FIX-04…07 and remaining kinds as separate rows; SK-01 PARTIAL, SK-02…09 OPEN (SK-06 NOT_MEASURED, SK-08 DEFERRED_PRIORITY). Facts behind SK-04: `JournalSink` writes each frame unbuffered on the capture thread via `append_legacy` and renders the three JSONL views at every seal.
- Found, not changed: the in-capture gap/reconcile phases (`gap_recovery_phase`, `full_reconciliation_phase`) have no production caller since `2c62d13` (capture/verification split); decision in Ticket 3 A3.

Tests: `p5_unreadable_fee_state_blocks_instead_of_default_receiver`, `p5_fee_state_unavailable_is_unknown_not_a_routing_loss`, `p8_unreadable_wall_clock_is_an_error_not_epoch_zero` PASS; winner 33/33 (1 ignored, unchanged).

## 2026-10-17 — Ticket 3 A1/A2: raw-free ingress and typed MARKET_* frames (LOCAL_PASS; A3 OPEN, A4 PARTIAL)

Base `5946b4d`.

- **A1 ingress:** `TransactionObserved` carries `tx: Result<ChainTx, ChainError>` (decoded once in the WSS adapter via `decode_notification`), `LogsObserved` carries typed `ChainLogs` (`decode_logs_notification`: signature, status table, log lines). No `serde_json::Value` leaves the adapter; `detect_version_json` removed. **Fixed:** an absent `meta`/`err` used to count as success in the observer (`success = err.is_none()`); such transactions are now `unknown_status_transactions` (incomplete reason `TX_STATUS_UNKNOWN`), their logged events are never counted. An undecodable payload keeps identity and reason (`decode_error`, incomplete) instead of a JSON tree or a reconnect loop; a non-string log line is a protocol error (previously dropped silently). `ProviderSnapshot` is typed.
- **A2 frames:** `MARKET_TRANSACTION` / `MARKET_LIQUIDATION` / `MARKET_COVERAGE` are schema-2 payloads (`market/rows.rs`), registered in the payload registry and removed from `LEGACY_WRITABLE` (schema-1 market writes are refused). Coverage is one exhaustive enum (window events START…STOPPED, warmup, slot boundary, full-slot results and final, gap boundary/slot/result/block/recovery). `TruthEvent` provenance fields are owned; `NativeEvidence` stays untagged in JSON and tagged in the binary journal. Producers in capture, core, gap and reconcile build typed rows; no `json!` row remains. The capture writes no JSONL any more; `summary.json` binds the journal only (`MarketJournalBinding/2`). START records the transport policy actually applied (recovery budget from the runtime config, attempts, idle timeout, `HEALTHY_RESET`) instead of descriptive literals.
- **Offline compatibility:** `payload::legacy_view` renders typed market frames to the historical rows for `p0-journal renders/export` (explicit, offline; `measurement_verify.journal_views` already calls it before hashing), so the pinned Python index verifier and report reader keep working unchanged on new runs. The strict verifier's batch sink still renders its batch views at the batch boundary (unchanged contract until A3).
- **Parity evidence:** the market tests store the legacy render of every typed row, so all existing row assertions check typed capture → render against the historical JSONL contract (113/113). Real v1/v0 (5 LUT) audit transactions re-encode losslessly for every modelled wire field (`uiAmountString` exact, no float).
- **Shared-decoder boundary (A4, documented):** for new typed runs the pinned readers see the Rust decode re-encoded, not the provider bytes; a Rust wire-decode defect would reach them. Not rendered: rewards, returnData, `meta.status`, float `uiAmount`. The Python index verifier's own `getTransaction` fetch stays independent. The Python differential on historical runs is NOT_RUN.
- **Still open:** A3 — native strict batches/checkpoints/resume and anchor reading (today JSONL views + JSON checkpoints), `summary.json` / the Python report chain (B7), removal of the dead in-capture gap phases; SK-04 — frames are still written unbuffered on the capture thread.

Tests: solana lib 72/72 (logs status table, re-encode round trip on real fixtures, wire decode of notifications); market 113/113 (1 ignored, unchanged) incl. new unknown-status regression and typed journal/render tests; runner lib 277/278 in the full run — the one failure was the write-admission test still naming `MARKET_TRANSACTION` as an untyped kind; updated to `ECON_FINAL` and rerun PASS (journal 14/14); featureless check and `crate_health txv1` bins check PASS.

Found, not changed: `scripts/smoke.sh` is stale since `2c62d13` (asserts in-capture gap recovery and `FULL_SLOT_RECONCILIATION_FINAL` in the capture coverage) and now also needs `p0-journal renders` before reading `coverage.jsonl`; it needs network access and was NOT_RUN.

## 2026-10-17 — SK-03/SK-04: capture byte budget and separate market journal writer (LOCAL_PASS in scope; SK overall OPEN)

Base `1dd7605`.

- **SK-04 writer lane:** `storage::writer::MarketJournalWorker` is the market journal's one ordered writer on its own thread behind the existing bounded channel. The capture thread only encodes typed rows (bincode, no JSON) and admits them; file writes, flush and sync happen only on the writer. Frames are written in bounded batches with one sequential write per batch through the one journal codec (`Writer::append_batch`, same chain/CRC/SHA/manifest), synced at a configurable interval and at seal. A writer failure is sticky (every later submit and the seal fail → capture stops `WRITE_ERROR`, INCOMPLETE); a dropped worker seals INCOMPLETE. OS-buffered frames are not crash-safe before a sync. FE-04/P8 financial durability is untouched (separate lane).
- **SK-03 byte budget:** `storage::budget::MemoryBudget` / `BytePermit` reserve payload + frame/queue overhead before admission and release after the batch is written. The admission limit starts at `initial`, grows only while a backlog needs it, never above `max`, and only while peak RSS + growth stays below physical memory minus the system reserve (unknown → no growth); it returns to `initial` when the backlog is gone. A row that cannot be admitted within the stall budget is refused (INCOMPLETE), never dropped; `max + reserve > physical` refuses the configuration. Peak RSS and the budget snapshot are reported in `MarketJournalBinding/3.storage` (frames, batches, bytes, max batch, max write/sync time, queue-full waits).
- **Config (existing typed operational contract):** `market_capture_initial_mib` 64, `market_capture_max_mib` 512, `system_reserve_mib` 4096, `market_writer_batch_frames` 512, `market_writer_flush_ms` 1000, `market_writer_sync_ms` 60000, `market_writer_stall_ms` 30000 (env `P0_MARKET_CAPTURE_*`, `P0_SYSTEM_RESERVE_MIB`, `P0_MARKET_WRITER_*`). These are conservative defaults, not measured capacity: SK-06 sizing for 4/24/48 h stays NOT_MEASURED.
- `Consumer::pop_timeout_or_closed` (common channel) lets the writer tell a quiet channel from a closed one.
- **Correction to `1dd7605`:** the solana crate's own tests still indexed the provider snapshot as JSON after it was typed (the solana lib tests had run before that change); fixed here. All workspace test targets now compile with all features.
- **Open:** SK-05 terminal backlog/exhaustion forecast and stall handling beyond the stall budget; SK-06 CapacityPreflight; SK-07 segmentation; SK-08 offline SQLite (deferred); bounded byte budgets for EconWorker/winner/shadow collections; real-load throughput/latency NOT_MEASURED.

Tests: storage 3/3 (hard/adaptive budget incl. waiting admission and release, ordered batched writes + seal + verify, over-budget refusal, INCOMPLETE seal on drop, sticky failure); journal + market 127 (unchanged expectations, every market test now runs through the writer lane); solana lib 72/72; common 26/26; `cargo test --workspace --all-features --no-run` PASS.

## 2026-10-17 — SK-03 completion: every capture-lane byte inside the budget; memory observation (LOCAL_PASS for the writer lane; SK-03 PARTIAL)

Base `e7fe08d`.

- `MarketJournalWorker::submit` reserves before it encodes: `payload::encoded_len` (bincode `serialized_size`, no allocation) sizes the frame; the permit covers the payload and its copy in the batch buffer (`2·len + FRAME_WIRE`); only then is the payload encoded. `Writer::append_batch` allocates its buffer with the exact capacity of its frames (no growth doubling). Queue slots and the writer's batch vector are reserved once at start (`fixed_reserved_bytes`; start refused when they do not fit `initial`). Nothing of this lane can exceed `max`.
- Memory observation: current RSS (macOS `task_info`, Linux `statm`), peak RSS, system swap in use and the macOS memory-pressure level are sampled on the writer thread at every batch and at seal (`StorageMetrics.memory`, worst values). The admission limit grows only without memory pressure and only while current RSS + growth fits physical memory minus the reserve.
- Hot path: the market capture runs on its own thread with its own WSS source (`orchestration/startup.rs` `econ-market`); writer backpressure can only hold that thread, whose ingress overflow is counted (`INGRESS_OVERFLOW`, INCOMPLETE). `econ::Handle::emit` is non-blocking (drops counted, economics incomplete). The trading owner never waits on the market writer.
- Still open in SK-03: byte bounds for the EconWorker/winner/shadow retained collections (count-bounded today; done with the B1 typing, which gives them native sizes); real-load evidence NOT_MEASURED.

Tests: storage 5/5 — budget hard/adaptive incl. memory sample; ordered batches + seal; reservation precedes encoding (refusal without allocation, slots exceeding `initial` refuse the start, INCOMPLETE seal with every admitted frame); overload refused at `max` after the stall budget while every admitted frame reaches the journal and the budget is fully released; injected write failure is sticky for submit and seal and leaves the written prefix unsealed (INCOMPLETE).

## 2026-10-17 — Ticket 3 A3: native strict verifier evidence, native reader, dead in-capture recovery removed (A3 PARTIAL; LOCAL_PASS for the strict engine)

Base `8cf863b`.

- **Native strict evidence:** `market_verification.rs` writes the resolved scope (`STRICT_SCOPE`), every chain read as a typed cache entry (`STRICT_CHAIN_CACHE` 318: read, `WIRE_DECODER_VERSION`, decoded answer; F10 25.10.; the earlier raw wire-text cache `STRICT_RPC_CACHE` 311 is historical, read-only), each batch as a typed market journal through the SK-04 writer lane, each batch checkpoint (`STRICT_CHECKPOINT`: spec, range, parent anchors, batch journal SHA-256/frames, reconciliation proof) and every attempt's result (`STRICT_RESULT`, content-addressed) as sealed single-value `.p0j` journals (`journal::write_typed_value`/`read_typed_value`: immutable, equal value accepted, conflict refused). No JSON checkpoint, scope, cache or JSONL view is written. Resume reads only this evidence; a checkpoint must bind spec, range, parent anchor, a complete proof and an unchanged verifiable batch journal.
- **Independence unchanged:** liquidation truth comes from the finalized blocks (`reconcile_slots` → `truth::classify`), never from `MARKET_LIQUIDATION` rows of the capture.
- **Bounded prefetch transferred:** the read-ahead that only existed in the dead `full_reconciliation_phase` now serves the strict verifier (≤ 8 produced slots, retry of a failed slot alone, call and byte budgets accounted in slot order, responses beyond the byte budget refused). Resource guards use RSS (`task_info`/`statm`) and `statvfs` instead of spawning `ps`/`df` per RPC call; disk usage is tracked incrementally.
- **Minimal native reader:** `read_report` / `p0-strict-verify --show DIR` verifies scope, checkpoint chain and batch journals and reports slots, transactions and canonical events per class, counting the re-fetched parent anchor block of each batch once. Exit 3 for an incomplete verification.
- **Transitional compatibility (explicit):** `p0-strict-verify JOB --compat` additionally renders the legacy checkpoint JSON, batch views and JSON summary from the verified native evidence for the Python driver (`measurement_verify.py`, now passing `--compat`) and the pinned report until the native chain (B7) replaces them. The job file input and the Python spec/attempt/result identities stay until then.
- **Dead code removed:** in-capture gap recovery (`recover_gaps`, `recover_block`, `gap_recovery_phase`, `gap_slot`, `gap_request`, `GapSlot`), `full_reconciliation_phase`/`read_ahead_block`/`prefetch_blocks`, the uncalled enrichment `get_transaction`, and the never-written gap-recovery coverage variants and summary counters (only the stale `smoke.sh` read them). `GapConfig` keeps `max_gaps`. Live gap bookkeeping is unchanged; `recover_transaction` (finalized program transactions not seen live) stays on the strict path.
- **Tests transferred:** failed/plain program transactions create no event and backfill rows carry no live timestamp; an LUT receiver counts only with loaded addresses; a truncated-log mismatch fails the block — now through `reconcile_slots`. Twelve tests of the removed recovery path were removed.
- **Open in A3:** native verification driver (typed spec/attempt/result, capture-evidence binding, job input) and the native report consumers (B7); `smoke.sh` still stale (NOT_RUN).

Tests: verification 4/4 (resume from native checkpoints, native reader, compat render idempotent, corrupt checkpoint/cache, wrong parent, disk budget, scope change, bounded read-ahead); market 104/104 (1 ignored, unchanged); storage incl. `statvfs`; runner lib 272 + 1 in the full run — the one failure was a pre-existing temp-directory name collision in `journal_tests` (deterministic `Pubkey::new_unique`), made unique and rerun PASS (journal 13/13); Python `test_measurement_p6` + `test_measurement_contract` 13/13; `cargo test --workspace --all-features --no-run` PASS.

## 2026-10-17 — Ticket 3 A4: historical differential against the pinned Python verifier (PARTIAL, `A4_HISTORICAL_SAMPLE_INCOMPLETE`)

Base `c44ad93`. Evidence: [A4_PARITY_20261017](evidence/A4_PARITY_20261017/README.md).

- Same historical provider bytes, two independent decoders: the pinned `audit_market_quick.py` (`detect`/`decode_logs`, imported unchanged by `operations/qualification/parity_differential.py`) against the Rust chain decode + `truth::classify` + receiver-log decoder (`p0-parity`, `market/parity.rs`). Sources: the 6 AUDIT_20261005 liquidation transactions and every historical run's raw `full_slot_reconciliation` block entries and `logsSubscribe` notifications. Read-only, no RPC, no new capture.
- 2,869 compared (52 full transactions, 2,816 logs, 1 truncated historical line reported identically): **2,839 identical** in status, fee, version, fee payer, markers with positions, events (class, target, actor, index, exact atomic units) and classification. **30 differ only in priority fee** — all v1: Rust reads `transactionConfig.priorityFee`, the pinned Python reports UNKNOWN; in all 30 `meta.fee − priority = 5,000` (one signature fee), so the Rust value agrees with the chain's total fee. Python stays unchanged.
- Not covered historically (unit/synthetic only, not counted): legacy transactions, absent `meta`/`err`, CLASSIC, FORCED_DELEVERAGE, BANKRUPTCY_SETTLEMENT, the markers `lending_account_liquidate`, `start_deleverage`, `end_deleverage`, `lending_pool_handle_bankruptcy`, CPI positions. Shared boundary: one provider; the Anchor discriminator names; new typed captures carry no provider JSON, so this parity is historical only.

Tests: `parity::real_v1_receivership_line` 1/1; differential run as above (exit 1 because of the 30 documented priority-only differences).

## 2026-10-17 — Ticket 3 A4 completed in scope: order-preserving differential, pinned discriminators, mutation vectors (LOCAL_PASS in scope; class coverage BLOCKED)

Base `25a4dff`. Evidence: [A4_PARITY_20261017](evidence/A4_PARITY_20261017/README.md) (replaces the first run's report).

- Markers compared in execution order with top-level/CPI positions and events in log order (no sorting); signature count and both sides' reason codes on every line; undecodable inputs keep an aligned line with their reason.
- All six discriminators bound to pinned sources: Anchor rule of the pinned entrypoints and, for the four start/end markers incl. `start_deleverage`, `marginfi_type_crate::constants::ix_discriminators` (pin `33c67987`).
- 48 mutations of the six real transactions (missing meta/err, failed, undecodable error, truncated/missing logs, missing loaded addresses/inner instructions), generated from the raw bytes.
- 2,917 compared: 2,848 identical, 69 explained by five explicit rules (E1 v1 priority = meta.fee − signature fees; E2 undecodable error UNKNOWN by contract vs FAILED; E3 Rust fail-closed without inner instructions; E4 truncated logs with identical events; E5 missing meta/err with equal status/class/events), **0 unexplained**; full report kept, nothing capped.
- BLOCKED: `A4_NO_HISTORICAL_RAW_SAMPLE_CLASSIC_DELEVERAGE_BANKRUPTCY_CPI` — no offline raw payload of those classes, the four other markers in transactions, CPI positions or legacy; needs a separately permitted archive read of known signatures.

Tests: `parity::real_v1_receivership_line`, `parity::marker_discriminators_match_pinned_sources` 2/2 (`crate_health`); differential exit 0.

## 2026-10-17 — SK-05: writer backpressure monitoring, bounded adaptive control, disk reserve (LOCAL_PASS in scope; SK-05 PARTIAL)

Base `ef60a4e`.

- `MarketJournalWorker::status()` (via `JournalSink::storage_status`) reports queue bytes (current/peak), admission limit and hard max, free budget, ingress vs. write bytes/s since the last sample, lag (admitted − written frames), seconds to budget exhaustion at the current rates, disk free space, the disk reserve and seconds until it is reached. The capture prints it every 2 s as `[MARKET/STORAGE] …`; the sealed `StorageMetrics` keep the highest rates, the minimum disk free space, backlog batches and deferred syncs.
- Adaptive control only within reserved capacity: with a queue depth of at least two batches the writer collects up to 4× the configured batch (the larger batch vector is part of the fixed reservation) and defers the periodic sync while the backlog lasts, at most to twice the sync interval (bounded unsynced tail). Never above `max`, never a drop.
- Disk reserve (`market_disk_reserve_mib`, default 2048, operational config): checked before every batch; below it the lane fails sticky (`DISK_RESERVE_EXHAUSTED`) and the capture stops INCOMPLETE before the filesystem is full.
- Fixed a race: a failing writer closed its channel before recording the reason, so a concurrent submit could report `MARKET_WRITER_GONE` instead of the cause; the receiver now outlives the failure record.
- Open in SK-05: forecasts under real load NOT_MEASURED; pausing optional offline indexers (none exist yet, SK-08).

Tests: storage 7/7 (adaptive decision bounds, disk reserve fail-closed with status/lag, plus the SK-03/04 set); market 104 + parity 2 unchanged; solana lib 72/72.

## 2026-10-17 — SK-06: CapacityPreflight completed and wired into the launcher (LOCAL_PASS in scope; real basis NOT_MEASURED)

Base `e000ecb` (completes WIP `0e0544d`). Evidence: [SK06_CAPACITY_20261017](evidence/SK06_CAPACITY_20261017/README.md). Readiness tracker: [C0E1_4H_READINESS](qualification/C0E1_4H_READINESS.md).

- **Basis from every lane:** the newest ≤ 3 sealed complete runs of the requested capture source; all capture-time journals (market, Shadow evaluations, economics reports, owner evidence, MT0, phases) count, but only frames in the format the current binary writes (`payload::check_write`); crashed, incomplete, foreign-format, other-source and too-short runs are skipped with their reason. Program-transaction and slot rates come from the market journal.
- **Mode-dependent verifier sizing:** `off` NOT_APPLICABLE (never blocked by a strict budget); `index`/`crosscheck` from the measured transaction rate (signatures, pages, calls, walltime); `strict` from the measured slot rate plus per-slot calls/bytes of earlier kept strict results. A too-small budget is BLOCKED with the flags to raise or `--chain-verify off` + later resume; a 4 h strict run with 2,000 calls is never strict-qualified. Provider quota for verification stays with the existing verification preflight.
- **Added checks:** sync cost from the write probe vs. the stall budget, swap vs. system reserve, econ/winner/Shadow collections (UNVERIFIED while only count-bounded, SK-03), capture provider NOT_RUN (BOOT-04 needs permission). Disk need includes the verifier disk only when a mode is selected.
- **Launcher:** `capture.sh` runs `p0-capacity` (built with the other release binaries) instead of the Python 60 GiB heuristic, with the run's verification budget; refusal stops before any capture. Native record `capture/capacity_preflight.p0j` (immutable); `storage_preflight.json` is a catalogued render of it for the pinned run store (`renders::storage_preflight_view`), replacing the WIP `--compat-json` writer that used `json!` (JFI-01).
- **On this host:** no sealed `.p0j` run exists → every capture > 600 s is refused (CAPACITY_UNVERIFIED), 600 s calibration allowed; measured write 101.7–106.3 MiB/s, sync max 5.2–5.7 ms.

Tests: `storage::preflight` 2/2 (mode sizing and blocks; basis from a real run directory incl. skip reasons, probe, immutable report, render CREATED/MATCHES/mismatch); launcher integration `test_measurement_plan` 4/4 with release binaries; clippy without findings in the changed files.

Identity note: `market_verification.rs` (only `BATCH_SLOTS` made public) is a strict decoder pin, so strict `verification_spec_id`s of new verifications change; no local strict verification is in progress.

Open: the real basis (calibration capture, needs explicit Mainnet permission); BOOT-04 provider evidence (C4); byte-bounded econ collections (SK-03/B1); removing the `storage_preflight.json` render needs a run-store re-pin decision (JFI-FIX-04).

## 2026-10-17 — Ticket 3 A4 rest: RELATED_NON_LIQUIDATION as native typed rows (LOCAL_PASS in scope; historical positives BLOCKED)

Base `2db6121` (isolated Project_0 worktree). Evidence: [A4_RELATED_20261017](evidence/A4_RELATED_20261017/README.md).

- `market/related.rs` decodes the four L.5 instructions by pinned discriminator with exact position (outer, CPI, stack height), transaction status and pinned account roles; new journal kind `MARKET_RELATED` (303).
- The strict full-slot scan records them for every program transaction (success and failed); the native batch reader and strict report count them once per signature/position and separately from the four classes. Never classified, joined, priced or counted as winner/profit. Live `logs` capture cannot observe instructions: live related coverage UNKNOWN.
- A4 differential extended with a `related` field on both sides: 2,917 sources, 0 unexplained (rule E6 for key-less mutations). No historical full transaction contains a related instruction: `A4_NO_HISTORICAL_RAW_SAMPLE_RELATED` BLOCKED.
- Identity note: `market_reconcile.rs`/`market_verification.rs` are strict decoder pins; new strict `verification_spec_id`s change.

Tests: related 2/2, reconcile related 1/1, market + journal 135 (2 ignored), parity/verification 8, differential exit 0.

## 2026-10-17 — Terminal control `p0ctl`, runtime-owner lock, ordered operator stop and offline boot audit (LOCAL_PASS in scope; network boot checks NOT_RUN)

Base `84a0cdb`. Evidence: [CONTROL_BOOT_20261017](evidence/CONTROL_BOOT_20261017/README.md); guide [operations/run_control.md](operations/run_control.md).

- Runtime-owner lock in `p0-longtail` (`P0_RUNTIME_LOCK`, exported by every launcher mode): double start / second writer refused before any stream, journal or RPC.
- Operator stop (SIGTERM/SIGINT, `p0ctl run stop`, Ctrl-C under `p0ctl run start`) reuses the existing ordered abort: admission closed, drain, journals sealed, coverage INCOMPLETE `OPERATOR_STOP`; idempotent. **Fixed:** a stop before StreamReady was reported as `STREAM_NOT_READY`.
- Offline boot audit BOOT-01…12 with readiness token and TOCTOU recheck at start; network checks BOOT-03…08 NOT_RUN (permission required), never PASS.
- `p0ctl doctor | preflight | run start | run status | run stop | verify | report` — control only; existing scripts stay usable.

Tests: control 1/1, boot audit 1/1, market 104 (+related) unchanged, launcher integration 4/4 incl. `p0ctl run start`; offline control evidence recorded.

Open: network boot audit (BOOT-03…08, needs permission), typed RunStatus/control DTO and browser API (frontend scope), `runs list`/`logs --follow`, `UntilStopped`.

## 2026-10-17 — SK-07: hash- and sequence-linked journal segments, chain reader (LOCAL_PASS in scope for the market lane)

Base `ca8fc19`. Evidence: [SK07_SEGMENTS_20261017](evidence/SK07_SEGMENTS_20261017/README.md).

- `storage/segments.rs`: rotation between batches at `market_segment_mib` (default 256 MiB) into linked complete journals of the one contract (`SEGMENT_NEXT` 331 / `SEGMENT_LINK` 330: predecessor name, frame count, sealed root, cumulative events, set id). No codec or format change; sealed segments never change.
- `ChainReader`: cross-segment replay; missing/foreign/corrupt/unsealed-intermediate/orphan segments fail closed with reason; an unsealed last segment (crash) replays its prefix and stays INCOMPLETE.
- Market writer lane, renders/export, capacity basis, `p0ctl`, `p0-journal verify/frames` read chains; strict batch journals stay single files (bound by file hash).
- Corrected the stale `test_run_store` render expectation (capture writes no JSONL since A2).

Tests: segments 2/2, lane rotation 1/1, storage/journal/market/verification 144, Python integration 26/26.

Open: Shadow-evaluation lane segmentation (with its offset index, B1); real-load segment counts NOT_MEASURED.

## 2026-10-17 — B1 part 1: typed and segmented Shadow-evaluation lane; byte-bounded econ retention (LOCAL_PASS in scope)

Base `fe539dd`. Evidence: [B1_SHADOW_RETENTION_20261017](evidence/B1_SHADOW_RETENTION_20261017/README.md).

- `ECON_SHADOW_EVALUATION` typed (`ShadowEvaluation`, schema 2, no JSON value), out of `LEGACY_WRITABLE`; historical schema-1 frames stay readable offline.
- Shadow lane segmented (`econ_segment_mib`); chain references `EvalRef { segment, offset }`; global arrival sequence.
- One byte budget for all retained analytics collections (`econ_retention_mib`, default 1024 MiB): reserve before retain, exact release, overflow = technical INCOMPLETE, never growth past the limit. Capacity preflight now counts it (bounded_collections QUALIFIED, physical memory incl. econ budget).

Tests: econ 49 (incl. new byte-ledger test and 1M-evaluation stream over linked segments), segments/writer 3, journal/evidence 43, preflight 2, Python launcher integration 26/26.

Open (JFI-FIX-05 rest, with B7): `ECON_SNAPSHOT/FINAL/OPPORTUNITY/CANONICAL` still schema-1 report values.

## 2026-10-17 — One duration parser (s/m/h/d), typed `RunHorizon::Fixed`, open-ended horizons refused (LOCAL_PASS)

Base `615e792`.

- `runtime_config::duration_seconds` is the only duration parser (operational config, `MeasurementPlan::parse_duration`, `p0ctl`): positive integer with `s`, `m`, `h` or `d`, bounded by exact nanosecond quarter offsets; the second parser in `measurement.rs` is removed. The four snapshot offsets stay derived from the one `MeasurementPlan` (unchanged arithmetic, existing plans byte-compatible).
- `RunHorizon::Fixed { duration_seconds }` is the plan's typed horizon. `until-stopped`/`inf`/`unbounded` are refused with `RUN_HORIZON_UNTIL_STOPPED_NOT_AVAILABLE` — no `u64::MAX` or placeholder duration. `UntilStopped` (rolling windows, re-preflight, segment policy) remains FEUI follow-up scope.

Tests: measurement 8 (incl. `2d`, `75m`, refusals), solana lib 72, release duration matrix `test_measurement_plan` 4/4 (`2d` = 172,800 s; `inf`, `until-stopped`, `0d` refused).

## 2026-10-17 — JFI-FIX-05 part 2: typed `ECON_CANONICAL` and `ECON_OPPORTUNITY` rows (LOCAL_PASS)

Base `98cfcc8`.

- `ECON_CANONICAL` stores the canonical join (`Joined`) as a typed payload; the join identity status decodes only into its closed set of six values (anything else is refused). `ECON_OPPORTUNITY` stores the typed inputs of an opportunity row (`OpportunityRow`: record, winner, market event, winner-slot receipt, capital scenarios). Both left `LEGACY_WRITABLE`; no JSON value is built to write them.
- Their historical JSONL rows (`canonical_market_shadow.jsonl`, `opportunity_economics.jsonl`) are catalogued renders computed by the same functions the run used (`canonical_legacy_view`, `opportunity_view`), so the pinned readers see byte-identical rows.
- `Winner`/`WinnerIdentity`, `MarketEvent` are binary-storable: text formats keep the internally tagged winner shape, the binary journal stores the externally tagged form.

Tests: econ/journal/winner 104 (incl. the layout-V2 run whose report files must equal the renders of the sealed report journal byte for byte, and `typed_canonical_row_renders_identical_view`), Python launcher integration 26/26, Python unit suite PASS.

Open in JFI-FIX-05: `ECON_SNAPSHOT`/`ECON_FINAL` report documents and the market summary values handed to the analytics worker are still JSON value trees; the native report chain (B7) follows.

## 2026-10-17 — JFI-FIX-04 part 1: typed measurement phases; latent strict-fixture defect corrected (LOCAL_PASS)

Base `78f5761`.

- `MEASUREMENT_PHASE` frames are typed (`PhaseRecord { phase, at_ns, unix_ns }`, `Phase` = BOOTSTRAP, WAIT_STREAM_READY, CAPTURE(`CaptureWindow`), DRAIN, VERIFICATION, OWNERS_FINISHED, REPORTS_FINALIZED); no JSON value is built at the seven call sites. `measurement_phases.jsonl` for the pinned readers is the catalogued render (`PhaseRecord::legacy_json`), same keys and values; a schema-1 phase write is now a refused rollback. Codec tests that used the phase kind as a generic legacy kind use `FE01_READ`.
- **Found and corrected (test fixture, not production):** `test_measurement_p6::test_strict_fixture_replay_and_nested_batch_integrity` wrote its strict batch event in the pinned Python decoder's shape; since Ticket 3 (`bb5a11a`) typed `TruthEvent` (list `visible_payments`, typed `native_evidence`, required provenance fields), `p0-econ-replay` refuses it (`MARKET_DECODE`). The earlier 13/13 record ran against a release binary older than that change. The fixture now writes the Rust `TruthEvent` render; identity still from the independent Python decode.

Tests: measurement/journal/econ 80, Python launcher integration 26/26, full Python suite 107 PASS (1 skipped; release `p0-fe-qualify` built for the FE replay tests).

Open in JFI-FIX-04: run manifest and operational-config report (JSON values written by the runtime for the pinned readers), measurement plan / candidate manifest handoffs, Python ↔ Rust CLI handoffs.

## 2026-10-17 — JFI-FIX-04 part 2: typed measurement-plan handoff (LOCAL_PASS)

Base `0772897`.

- The plan compiler writes the plan as a typed single-value journal (`MEASUREMENT_PLAN` 501, `capture/measurement_plan.p0j`, immutable) and the launcher hands that journal to the runtime (`P0_MEASUREMENT_PLAN`); the runtime no longer parses a JSON plan on the launch path. The JSON on stdout stays the pinned run store's staged `measurement_plan` part (render of the same struct). A JSON plan file remains readable for historical/manual invocations.

Tests: `measurement::typed_plan_journal_is_the_runtime_input` (typed load, conflicting legacy override refused, tampered journal refused), measurement/journal 37, Python launcher integration 26/26 (manifest plan equals the staged part).

## 2026-10-17 — JFI-FIX-04 part 3: typed run manifest and effective-config record (LOCAL_PASS)

Base `74f60cf`.

- `RunManifest` (`RUN_MANIFEST` 503) is written first as an immutable typed journal (`capture/run_manifest.p0j`, inside the V2 names the pinned run store allows); `manifest.json` / `measurement_manifest.json` is its render (`RunManifest::legacy_json`, V1 without `layout`), still written at bootstrap because the pinned run store recognizes the layout by it.
- The runtime keeps the effective operational configuration as a typed, secret-free record (`EffectiveConfig`, `OPERATIONAL_CONFIG` 502: values with endpoint counts only, the source of every value, fixed FREE_ONLY/no-authorization facts) instead of a JSON value. The manifest embeds it; the Shadow setup writes it as `operational_config_<ns>.p0j` instead of a JSON file (no reader used that file). Changed content: the manifest's `operational_config` object is now this record (no pinned reader parses it); `--print-effective-config` keeps its JSON output for the Python launcher (external CLI handoff, open with the launcher migration).

Tests: measurement 9 (typed manifest journal equals the `manifest.json` render, no secret), measurement/econ/journal 81, solana lib 72, Python launcher integration 26/26 (pinned run store recognizes both layouts).

Open in JFI-FIX-04: config resolution through a JSON value tree, the `--print-effective-config` handoff and the Python measurement-config/launcher JSON (until `p0ctl` replaces the Python launcher with parity).

## 2026-10-17 — Ticket 3 A3 rest: native strict verification driver with typed identities (LOCAL_PASS in scope)

Base `79b5568`.

- `market/strict_driver.rs`, `p0-strict-verify --run RUN [--resume-verification] [--offline] [--verify-* N]`: for layout-V2 runs the driver reads the typed run manifest and the typed CAPTURE phase (exactly one; window must equal the plan duration), fixes the capture evidence once (`CAPTURE_EVIDENCE` 315: every capture-time file with size and SHA-256, `verification/` and `reports/` excluded) and re-checks it on every attempt and after the attempt, derives the stable `verification_spec_id` (`STRICT_SPEC` 314: capture id, manifest hash, window, mode, program, verifier binary as decoder pin, protocol pin, journal version, FREE_ONLY policy and the endpoint's SHA-256 — never the endpoint — and budgets), takes an exclusive writer lock, records each attempt's start and end separately (`STRICT_ATTEMPT` 316) and binds the strict engine's content-addressed result as `verification_result_id`. No JSON job file; existing strict engine, checkpoints and resume unchanged.
- L.3 invariants: the spec id never contains attempts, checkpoints or result digests; resume requires the identical spec (`RESUME_IDENTITY_MISMATCH` otherwise) and refuses a second fresh run (`SPEC_EXISTS_USE_RESUME_VERIFICATION`); a changed capture is refused (`CAPTURE_EVIDENCE_CHANGED`). Historical/V1 runs keep the Python driver (`NATIVE_MANIFEST_MISSING`), whose identities and the pinned report stay unchanged; native results are consumed by the native report (B7).

Tests: `strict_driver::spec_identity_is_stable_and_bound_to_its_inputs`, `strict_driver::driver_records_identities_and_refuses_changed_capture` (offline end to end on a V2 run with a real manifest and CAPTURE phase), verification/journal 36.

## 2026-10-17 — B7 / SK-09 (5): native C0-E1 report over `.p0j` (LOCAL_PASS in scope)

Base `4390f01`.

- `analytics/report.rs`, `p0ctl report --run RUN [--report-* …] [--verification-result ID] [--format table|csv]`: one normalized dataset per capture and selected native strict result (`REPORT_DATASET` 340, immutable, `NORMALIZED_DATASET_CHANGED` on drift) from the live market journal chain, the result's verified batches only (`read_report_with`), the production canonical join (`econ_canonical::join`, extracted from the offline replay) over the strict events + `shadow_evaluations.p0j`, and `ECON_OPPORTUNITY` rows. Entities are keyed by signature/slot/kind/position/event (L.4); `best` STRICT → LIVE; `all` lists each entity once; Shadow-only rows are HYPOTHETICAL and separate; RELATED rows are never liquidations/winners/profit. Every query records `report_query_identity` (`REPORT_QUERY` 341: capture, result, dataset digest, query, renderer). Filters act on the view only; table and CSV render the same cells; JSON output and `index` provenance are refused (JFI precedence; the pinned Python report keeps index and historical runs, `p0ctl` routes them there).
- Six markers natively (`market/markers.rs`, pinned discriminators/roles, start/end pairing as the pinned audit); live events in the classifier's `event_index` order (`LogDecode::sequence`); typed capture completion axes (`MARKET_COMPLETION` 304, `market_completion.p0j`, the values `summary.json` reports, computed once) so the report never reads JSON; `strict_driver::bind_capture` shared by driver and report (`CAPTURE_EVIDENCE_CHANGED[_DURING_REPORT]`).
- Fix: the runtime read its code identity (git head, source hashes) from the compile-time path; a binary reused from another checkout failed with ENOENT before the manifest (launcher integration tests). It now uses the launcher's `P0_PROJECT_DIR` (`measurement::repo_root`). Writer tests no longer depend on timing under parallel load.

Tests: `report_e2e::native_report_binds_sources_and_filters_only_the_view` (V2 run, live journal, offline strict from cached finalized blocks, CANONICAL_VERIFIED; raw capture/verifier hashes and dataset digest invariant under queries; entity once in `all`; marker hiding keeps the class; related separate; Shadow-only separate; table = CSV cells; explicit older result → PROVISIONAL; changed capture fails closed), `report_e2e::report_arguments_follow_the_contract_and_refuse_json_and_index`, `markers::tests` 2; runner lib 259 (default) and 296 (`crate_health`); Python integration run_store/measurement_plan/market_report/p6 43/43 (twice, second without rebuild).

Open: native index provenance (no native index verifier), JFI-FIX-04/05 rests (config JSON tree, ECON_SNAPSHOT/FINAL, market summary values), network BOOT-03..05, Mainnet calibration/smoke (permission required).

## 2026-10-17 — JFI-FIX-05 rest: typed economics snapshots/final report and market summary (LOCAL_PASS)

Base `58ffb00`.

- `ECON_SNAPSHOT` (`SnapshotFrame`) and `ECON_FINAL` (`SnapshotReport`) are typed (`analytics/econ_docs.rs`, schema 2); both left `LEGACY_WRITABLE` (schema-1 writes are a refused rollback). `hour_0N.json` / `economics_final.json` are their renders. Metrics, technical qualification, capital, race timing, bottlenecks, canonical summary (`CanonicalSummary`) and MT0 telemetry are typed values.
- The market summary handed to the analytics worker is typed (`MarketSummary`, `MarketFinal`, `TruthSummary`); `summary.json` / `final_report.txt` are renders. Capture counters and checkpoint deltas use typed counter lists (`MarketStats::counters`, `MarketSummary::delta_counters`) instead of JSON key scans. MT0 has a typed `Snapshot`; its text is unchanged (verified against the previous builder).
- Parity: golden renders recorded with the previous code (`testdata/golden/`): the market summary is identical; the economics documents are identical except `retention_bytes_current/peak` (retained checkpoints are now accounted in their typed encoding, SK-03).

Tests: golden `market_summary_render_matches_golden`, `report_documents_match_golden`; runner lib 258 (default) / 298 (`crate_health`); common 26; Python integration run_store/measurement_plan/market_report/p6 43/43.

Open: JFI-FIX-04 config JSON tree and Python launcher handoffs, MT0_SNAPSHOT/FE01 legacy kinds, terminal text still printed from the render.

## 2026-10-17 — JFI-FIX-04 / JFI-FIX-05 rest: no productive JSON, native config text, p0ctl-only launch (LOCAL_PASS, scope)

Base `53f6d24`.

- Productive runs write no JSON/JSONL (JFI-03, PERSIST-04); `.p0j` is the record. `hour_0N.json`, `economics_final.json`, the economics JSONL views, market `summary.json`, `manifest.json` / `measurement_manifest.json`, `config.json` (V1: `measurement_plan.json`, `storage_preflight.json`, `candidate_manifest.json`) and the phase JSONL exist only as explicit offline renders (`p0ctl export --run RUN`, `p0-journal renders RUN --create`). The market summary is the typed `MARKET_SUMMARY` journal (`market_summary.p0j`, kind 305); `final_report.txt` and the economics terminal summary are text from typed values. Native capture evidence excludes the derived views.
- Launch chain: `test.sh` / `capture.sh econ4h` → `p0ctl run start` → `capture.sh econ4h-launcher` with native arguments (typed plan journal `measurement_plan.p0j` via `p0-measurement-plan --out`), post-capture `p0-strict-verify --run` (strict) and `p0ctl report`; no Python launcher, no JSON handoff. No recursion: `P0_CTL_LAUNCH_RUN_ID` marks the launcher child, `p0ctl run start` and `capture.sh econ4h` refuse inside it (`RECURSIVE_LAUNCH`), the launcher refuses without it (`LAUNCHER_REQUIRES_P0CTL`). A relative `-- --config FILE` keeps the caller's directory.
- `OFFLINE_READY_NETWORK_NOT_RUN` is no Mainnet release: `p0ctl run start` starts a capture only with the operator's `--mainnet-approved TOKEN` for exactly the audited state (token as `p0ctl doctor` prints it); otherwise `MAINNET_APPROVAL_REQUIRED` and nothing starts.
- Layout detection: native consumers take the layout from the typed run manifest (`capture/run_manifest.p0j`, V2 without `manifest.json`); historical runs keep the pinned `manifest.json` rule (typed read). `run_store.py` and the other pinned readers are unchanged; `verify.sh` / `report.sh` run `p0ctl export` before them.
- Config: profiles (`config/profiles/*.conf`, `Project0/RuntimeProfile/2`) and config files (`--config`, `P0_OPERATIONAL_CONFIG_FILE`, `config/examples/*.conf`) are native `key = value` text with CLI names and values (unknown or duplicate keys fail); the JSON profile/example files are removed. Typed parameter table (`PARAMETERS`, `Limits::get/set`); `--print-effective-config` is a redacted text view. A JSON config is accepted only as the private handoff of the pinned bridge `operational_config.py` (`--export-environment`, JFI-03) and refused by the productive entry points (runtime start, `p0-measurement-plan --out`, `p0-strict-verify --run`: `CONFIG_JSON_REFUSED`).

Tests: `runtime_config` 9 (text profiles/config, forbidden keys, duplicate/unknown keys, JSON handoff refused natively, redacted text view), `manifest_is_exclusive_and_phase_journal_preserves_exact_window` (V2 detected from the typed manifest alone), `legacy_views_are_checked_and_missing_ones_rendered_from_the_journals`; Python integration `test_measurement_plan` 5 (recursion refused, approval gate incl. wrong/duplicate token, relative text config recorded as `config_file`, no JSON in the run before the export) and `test_run_store` 22 (launcher only via p0ctl, export before the pinned readers); workspace regression `crate_health` and `crate_health txv1`: runner lib 298/298 (4 ignored), solana 73/75, liquidation 68, common 26; Python full suite 108 OK (1 skipped: no local historical runs in the isolated worktree).

BLOCKED (pinned, unchanged): index/crosscheck chain verification exists only as the pinned Python reader writing JSON evidence; the native launcher refuses `--chain-verify index|crosscheck` (`CHAIN_VERIFY_*_BLOCKED`), historical index verification stays an explicit offline `verify.sh`. The pinned bridge (`operational_config.py`, `measurement_contract.endpoint()`, `transport_options()`) reads its private JSON handoff, and `p0-measurement-plan` without `--out` (JSON on stdout) stays the pinned `market_report.py` contract, as does the offline `p0-econ-replay` it calls (writes its JSON documents); removing any of them needs a re-pin.

Open: FE-01 (`p0-fe-qualify` JSON, `FE01_*` kinds), `MT0_SNAPSHOT` legacy kind, network boot audit BOOT-03…08, Mainnet calibration capture and 4h start (operator approval).

## 2026-10-20 — BUGS packages 5–7 / review 9 (LOCAL_PASS per ticket, offline only)

Base `2ffcc20`; commits `119da22` (package 5), `423c024` (review 9: V1/V3–V8 = bugs_b Z1/Z3–Z8), `78d79c3` (V2 = bugs_b Z2, completes B6). Acceptance rule (user, 20.10.2026): every fix has a regression test that fails when the old behaviour is restored (checked by mutation), changed binary layouts are read through explicit earlier layouts with goldens from real old journals, and an independent read-only review precedes every push.

- Gates (exit 0 each): `119da22` — fmt, clippy all/default, `test --all-features` 640/0/6, default 638/0/6, release, Python 108 (1 skipped); `423c024` — 649/0/6, 647/0/6, release, Python 108 (1 skipped); `78d79c3` — fmt, clippy all/default, `test --all-features` 661/0/6, default 659/0/6, release, Python 108 (1 skipped) exit 0.
- V2 deadline contract: 12 tests — the real `RpcSubmitter` behind `ObserveOnly`, the real Jupiter provider, `P0ReceivershipBuilder::plans` and `Pipeline::evaluate` against loopback HTTP endpoints (hanging fee quote, hanging simulation, 503 retries then hanging, slowly read response, blocked provider permit, expired deadline, hanging Jupiter, provider backoff past the deadline, deadline cut without provider degradation, Jito bundle simulation, route-fit re-quotes, adapter without candidate transport) plus two transport tests (legacy `send` still degrades a hanging provider with its old codes; unbudgeted candidate transport needs no permit and keeps the deadline). Deadline 300 ms, bound 400 ms; stability runs in `evidence/BUGS_20261020/V2_deadline_stability.txt`.
- ECON schema 4 (V1/V5): golden test over sealed, chain-verified ECON journals written by the schema-2 (2026-10-17) and schema-3 (2026-10-18) writers in synthetic test runs (provenance and sha256 in BUGS.md V5); existing figures unchanged, new fields `NOT_RECORDED`/`PRE_N1` for old frames.
- NOT_RUN / not claimed: no Mainnet read, provider measurement, capture, calibration or LIVE; the real-load candidate rate under the 5 s default deadline and the share of `RpcDeadline` losses are NOT_MEASURED.

## 2026-10-20 — bugs_b Q2/Z18/Z12/Z14 = BUGS V10–V13 (LOCAL_PASS per ticket, offline only)

Commit `3f901d3` (rebased onto foreign `45ca27e`, `project_0/native` tree identical to the gated state). Each ticket was reproduced RED on the old code before the fix and is guarded by fail-before mutations; independent read-only review before the push (two review findings fixed before the commit: V13 probes first sat in `detect_queue` ahead of oracle work).

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 665/0/6, default 663/0/6, release, Python 108 (1 skipped) exit 0.
- V10 (Q2): checked safety ledger transitions, HALT `LEDGER_INVARIANT` without applying anything; owner reconcile path unreachable since B1 (no `PendingSubmission`), proven at `Safety` level.
- V11 (Z18): integration accounts only with the pinned owner (`declare_id!` of the pinned mock crates) and discriminator; Kamino reserve owner checked before decoding.
- V12 (Z12): pair errors isolated; account-wide errors fatal; no-plan classification unchanged.
- V13 (Z14): race reproduced offline with the real owner (after a recovery with 1,100 was-liquidatable accounts only 63 of 1,024 ingress slots were free); probes now in a dedicated owner queue after oracle work.
- NOT_RUN: no Mainnet read, capture or LIVE; B3/C3 (durable multi-attempt safety ledger, settlement) open.

## 2026-10-20 — bugs_b Z13/Z19/Z17 = BUGS V14–V16, V9, V13 telemetry (LOCAL_PASS per ticket, offline only)

Commit `c7ccce0` (rebased onto foreign `63142e3`, `project_0` tree identical to the gated state). Each ticket is guarded by fail-before mutations (the old behaviour restored makes its test fail); independent read-only review before the push.

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 668/0/6, default 666/0/6, release, Python 108 (1 skipped) exit 0.
- V9: the loss of a candidate without a plan is the furthest technical stage (BUILD/SUBMISSION) unless a plan reached the economics; from `c7ccce0` such candidates no longer count as ECONOMIC_SKIP (older journals unchanged). PARTIAL: a LIVE safety-preview denial is still attributed economically (needs a new `LossReason` variant; LIVE locked).
- V13: probe backlog on its own diagnostic line `bootstrap_probes:` (the R126/R127-parsed `oracle_trigger_stats` line unchanged).
- V14 (Z13): bankruptcy replaces only no-plan follow-on causes and keeps their provenance.
- V15 (Z19): WSS health snapshot also during silence, proven with the real session against a loopback WebSocket; production interval 5 s unchanged.
- V16 (Z17): real `sizes()` test on both sides of the 5 USD closeout threshold.
- Test hygiene (no production change): the first gate run of this package failed 6 econ tests with `AMBIGUOUS_SHADOW_SOURCE` — the test worker directory `c0_e1_<pid>_<seq>` in the system temp dir was reused by a recycled process id and still held a `shadow_evaluations.jsonl` of a 2026-10-15 run. The helper now clears its directory before use; the gate was repeated in full.
- NOT_RUN: no Mainnet read, capture or LIVE; real probe waits and health cadence under Mainnet load NOT_MEASURED.

## 2026-10-20 — bugs_b Q5 = BUGS V17 typed planning errors (LOCAL_PASS, PARTIAL, offline only)

Commit `0dbdc35`. Independent read-only review before the push.

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 670/0/6, default 668/0/6, release, Python 108 (1 skipped) exit 0.
- Planning failures carry a typed kind from their source (FeeState, sizing census, no-size code, stale integration reserve by pinned error code, health, ATA, routing); decision and loss follow from the kind only. Reason texts are byte-identical, so journals and renders are unchanged; no schema change.
- Attribution changes: a non-FeeState planning failure whose text contains "unavailable" is now SKIP (was UNAVAILABLE); a typed stale integration reserve from sizing/observation is INTEGRATION_REFRESH (was ROUTING, practically unreachable).
- PARTIAL: the kind itself is not persisted; OTHER/HEALTH_UNAVAILABLE/ROUTING remain one ROUTING loss (needs a `LossReason` extension with schema versioning, with Q3).
- NOT_RUN: no Mainnet read, capture or LIVE.

## 2026-10-20 — bugs_b Q3 = BUGS V18 explicit persisted names (LOCAL_PASS, PARTIAL, offline only)

Commit `b30e0ea`.

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 671/0/6, default 669/0/6, release, Python 108 (1 skipped) exit 0.
- Persisted and reported enum values (loss names, legacy bottleneck keys, shadow-ledger day losses, cost-record path, tx version, simulation result, census unsupported names, landing confirmation status) come from explicit names, byte-identical to the former Debug output (checked once against every variant), so journals, reports and Python readers are unchanged; no schema change.
- PARTIAL: the simulation failure code and the landing `err` still embed the Debug of the pinned SDK `TransactionError`. New finding V19: census `UNKNOWN_LAYOUT` classified by text.
- NOT_RUN: no Mainnet read, capture or LIVE.

## 2026-10-20 — BUGS E2 rest: signed message equals its plan (LOCAL_PASS, E2 PARTIAL, offline only)

Commit `02ac861`.

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 672/0/6, default 670/0/6, release, Python 108 (1 skipped) exit 0.
- `tx::message_matches_plan` decompiles every message before signing (receivership and prerequisites) and requires exactly the plan's compute-budget and validated instructions with the same account keys (lookup tables resolved), the union privileges, the payer first and no other key; 11 tampering cases refused with their own code, 8 fail-before mutations.
- E2 stays PARTIAL: swap and prerequisite `Other` instructions are bound by program only (no pinned Jupiter IDL); Jupiter lookup-table contents are the API's claim, not checked on chain; the bundle tip transaction is constructed, not checked; instruction amounts are not checked against the sizing.
- NOT_RUN: no Mainnet read, capture or LIVE.

## 2026-10-20 — BUGS V19 typed unknown layout (LOCAL_PASS, offline only)

Commit `d06393e`.

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 674/0/6, default 672/0/6, release, Python 108 (1 skipped) exit 0.
- The strict decoders return `HealthError::UnknownLayout`; the census class and the shadow-ledger fail-closed count follow the variant, never the text; displayed texts unchanged. Layout refusals inside the pinned crate stay `Crate`; builder layout errors stay untyped (V17).
- B4 rest made precise: payer SOL coverage is already checked at the FE-04 admission; open is filling `Intent.rent_lamports` from the plan (B1).
- NOT_RUN: no Mainnet read, capture or LIVE.

## 2026-10-20 — BUGS R12 rest: byte budget and retention bound tests (LOCAL_PASS, R12 PARTIAL, offline only)

Commit `9a0ae35` (test-only).

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 676/0/6, default 674/0/6, release, Python 108 (1 skipped) exit 0.
- Tight `econ_retention_mib` byte budget: the next distinct evidence is refused and counted, nothing evicted, verdict INCOMPLETE; `P0_ECON_RETENTION` bound exact (2,000,000 accepted, 2,000,001 refused).
- Historical byte peak NOT_MEASURED: read-only check of all 22 runs in the local backup outside the repository (`Backup/DarkBot/project_0/data/econ4h`) — none carries `retention_bytes_peak` (pre SK-03). Historical counts as in A1 (max 1,682 transactions in 4h).
- NOT_RUN: no Mainnet read, capture or LIVE; 600-s calibration needs separate approval.

## 2026-10-21 — BUGS E9 part 1: projection context bound and bounded requeue (LOCAL_PASS, E9 PARTIAL, offline only)

Commit `9602562`.

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 680/0/6, default 678/0/6, release, Python 108 (1 skipped) exit 0.
- Projection simulations carry `minContextSlot` = newest canonical slot of the candidate request and its dependencies; an older context or JSON-RPC −32016 is a typed `ContextBehind`. ProgramData is a projection dependency. Stale results and lagging nodes requeue at most twice per external trigger, FIFO; other failures wait for the next trigger.
- Offline proof through the real projection worker and owner result handling against a scripted loopback RPC; no Mainnet read.
- PARTIAL: no upper bound on the projection context at consume (mixed-state risk), no requeue backoff, trigger kinds by string. NOT_MEASURED: provider support of `minContextSlot`, discard/requeue rates under real oracle cadence.

## 2026-10-21 — BUGS E9 part 2: projection context upper bound at consume, E9 closed (LOCAL_PASS, offline only)

Commit `479c67a`.

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 683/0/6, default 681/0/6, release, Python 108 (1 skipped) exit 0.
- A cached projection is consumed only once the owner processed every confirmed account notification up to its simulation slot (newest account notification − 1, snapshot lower bound or root; tradable state only); otherwise the work is parked and released in slot order. A ContextBehind requeue waits for one more processed slot. E9 closed on all code criteria of the 21.10 order.
- Assumption (not checked against agave source or the provider): one confirmed-bank notification pass over all subscriptions, in slot order, per WSS connection.
- NOT_MEASURED: that ordering at the real WSS provider, real oracle cadence, provider `minContextSlot` behaviour, drop rate of parked projections (`deferred` vs `dropped` on the first real run).

## 2026-10-21 — BUGS V10 (bugs_b Q2) safety edge cases (LOCAL_PASS, offline only)

Commit `c771881`.

- Gate (exit 0): fmt, clippy all/default, `test --all-features` 685/0/6, default 683/0/6, release, Python 108 (1 skipped) exit 0.
- `-realized_pnl` overflowed at `i128::MIN` (no release `overflow-checks`: wraps, the daily-loss HALT never fired — shown with the old code in a `--release` test run); replaced by `realized_pnl <= -max_daily_loss` at both sites.
- `roll_day` before validation: time-driven UTC-day contract, no break; failed transitions leave the full ledger unchanged except the time-driven roll, also across UTC midnight.
- Open contract question for the user: an outcome reconciled after midnight books its PnL and failed attempt into the new UTC day (daily limit by reconciliation time, not attempt time; documented as the current contract until decided).
- NOT_RUN: no Mainnet read, capture or LIVE (the ledger has no production caller since B1).

## 2026-10-21 — BUGS R1 drain deadline case and run result (LOCAL_PASS package, R1 PARTIAL, offline only)

Commit `310be1c`.

- Gate (exit 0 each): fmt, clippy all/default, `test --all-features` 687/0/6, default 685/0/6, release, Python 108 (1 skipped).
- Total post-stop timeout: was `drain_timeout_secs` + seal + a second full `drain_timeout_secs` join (≈ 240 s worst case at the 120 s default); now `drain_timeout_secs` + seal + ≤ 50 ms (join within the remaining drain budget). A thread still alive at the join ⇒ `INGRESS_NOT_QUIESCENT`, handle dropped, stop already requested.
- RED reproduced: drain deadline with a live ingress ⇒ journal sealed INCOMPLETE, thread ends before the join ⇒ `run()` result `Ok(())`. Fixed: `MARKET_CAPTURE_INCOMPLETE: DRAIN_INCOMPLETE`, all earlier classes keep precedence. 8 fail-before mutations.
- Open: `run()`-level end-to-end capture with a fake WSS; NOT_RUN: teardown of an established TLS session. No Mainnet read, capture or LIVE.

## 2026-10-21 — BUGS V20 runner feature matrix and R1 run() end to end (LOCAL_PASS, offline only)

Commits `0c269ad` (V20), `86f790c` (R1 test).

- Gate (exit 0 each): fmt, clippy all/default and isolated `-p darkbot-runner` default/`txv1`, `test --all-features` 688/0/6, default 686/0/6, isolated runner default 334/0/3 and `txv1` 334/0/3, release, Python 110 (1 skipped).
- V20: `cargo check -p darkbot-runner` without `crate_health` exited 101 since X2/R4; workspace builds unify `crate_health` into the runner through the reporter, so no gate built that configuration. RED/GREEN `tests/regression/test_feature_matrix.py`.
- R1: `capture::run` over an established loopback `ws://` session through the real WSS adapter seals COMPLETE and returns `Ok` (≈ 1.08 s, 3 runs); a real hang (both stop paths disabled) returns `Err INGRESS_NOT_QUIESCENT` after window + one drain budget (6.11 s).
- L3: persisted `deferred_intervals`/`ingress_dropped_count` judged not gate-relevant (no readiness requirement, PERSIST-02 met through the overflow count, no reader, nested schema migration MARKET_SUMMARY + ECON_SNAPSHOT/ECON_FINAL without verdict change); L3 stays PARTIAL.
- NOT_RUN: teardown of an established TLS session (webpki roots only; authorized smoke). No Mainnet read, capture or LIVE.

## 2026-10-21 — C0-E1 readiness unit E: verification levels end to end on the production run path (LOCAL_PASS, offline only)

Commit `67ab023`.

- Gate (exit 0 each): fmt, clippy all/default and isolated runner default/`txv1`, `test --all-features` 689/0/6, default 687/0/6, isolated runner default 334/0/3 and `txv1` 334/0/3, release, Python 110 (1 skipped), all exit 0.
- `run_econ4h_with` (behaviour-identical split; production passes `run_shadow`) runs the plan-bound capture over a loopback `ws://` peer, the analytics worker, the supervision and the phase journal; a Shadow-owner stand-in publishes readiness like `ShadowOwner` and ends at the drain expiry. Without and with a transport gap (peer drops its first session; 2 sessions): owners finish without failure, `capture_clock_complete`; live coverage COMPLETE vs INCOMPLETE and unchanged after `off` (`NOT_RUN`) and offline `strict` (not canonical); report `PROVISIONAL/INCOMPLETE`, master false; technical qualification INCOMPLETE (run result). 3/3 runs ≈ 12.3 s; first reconnect ≤ 750 ms, gap at ≈ 1 s of a 4-s window.
- Finding V21: no code path computes the separate master qualification after verification (PHASENPLAN L.2); the technical verdict at run end is INCOMPLETE for every C0-E1 run since L.2.
- BLOCKED: `index`/`crosscheck` (pinned Python verifier, re-pin decision). No Mainnet read, capture or LIVE.

## 2026-10-21 — BUGS E2 rest: tip transaction checked against its plan (LOCAL_PASS, E2 PARTIAL, offline only)

Commit `ba4a3c3`.

- Gate (exit 0 each): fmt, clippy all/default and isolated runner default/`txv1`, `test --all-features` 690/0/6, default 688/0/6, isolated runner default 334/0/3 and `txv1` 334/0/3, release, Python 110 (1 skipped), all exit 0.
- `tx::message_matches` (extracted, `message_matches_plan` delegates); `submit::tip_matches` checks the production tip transaction (one System transfer of the planned lamports to the configured tip account); 8 manipulations with their own codes, 2 fail-before mutations.
- E2 stays PARTIAL: amounts vs sizing, Jupiter swap discriminators (BLOCKED, no pinned IDL), prerequisite `Other`, Jupiter LUT on-chain content. No Mainnet read, capture or LIVE.

## 2026-10-22 — Nachtrag: offline packages of 22.10 (LOCAL_PASS per package, offline only)

Recorded after the fact from the commit messages and the STATUS rows of 22.10; no number below is new.

- `3e631ab` C0E1-P5-AUTO/REPORT post-capture chain (verify → report → qualify), deployment proof re-checked, qualifier hash fail-closed: tests 696/0/6, 694/0/6, runner 340/0/3, 4 fail-before mutations; index/crosscheck BLOCKED (re-pin).
- `d144f6b` C0E1-P6-GATE `scripts/gate.sh`: P6_GATE_OFFLINE=PASS, 696/0/6, 694/0/6, runner 340/0/3 ×2, release, Python OK.
- `e242639` JIT ExactIn solver (`02b16cd`), JIT capital (`a82f1a7`), class comparison (`71287b3`/`bb7a3dc`), P8-RES-01/02 (`1be59b3`): gate PASS 702/0/6, 700/0/6, runner 343/0/3; real routes NOT_RUN.
- `a79ec2a`, `a132618`, `8390b8a`, `5825d47`, `2774a5e` P8-RES owner integration, FE-04-backed owner book, P8-INV-01, P2-06, P2-07/P5-07: gate PASS 710/0/7, 708/0/7, runner 348/0/3 ×2; 19 fail-before mutations; plans() fixture bench p50 52.3 → 49.0 µs.
- `154a223`, `3e28b1a`, `4bb683d`, `1a97a5b`, `8fe7ce0`, `b2f0f58`, `6d0d9e8`, `90252d3` BUGS pass C11/B15/B3/B14/B7: gate PASS 713/0/9, 711/0/9, runner 349/0/3 ×2; 13 fail-before mutations; plans() fixture p50 33.5 → 4.25 µs; B7 profile health 16 banks p50 6.1 µs / 54 allocations, sizing 64 pairs 36.8 µs / 74 allocations.
- NOT_RUN for all: Mainnet read, provider, capture, LIVE.

## 2026-10-23 — FE-01 freeze, FE-02/B4, C3/P8-05, FE-04 admission, S1-02/S1-08 Binary Geyser (LOCAL_PASS offline; gates NOT_RUN where stated)

Commits `6aac2e4f`, `91ed707a`, `2605556c`, `8bfa63f2`, `94086af8`, `6c83355f`, `dc54bdda`, `8246fcfb`.

- Gate on `8246fcfb` (exit 0 each): fmt, clippy all/default/runner default/`txv1`, `test --all-features` 746/0/10, default 744/0/10, runner default and `txv1` 355/0/3 each, release, Python OK (1 skipped).
- FE-01 `6aac2e4f`: typed `Fe01Freeze` (journal kind `FE01_FREEZE`), drift check, `p0-fe-qualify --against`; tests on the pinned marginfi source `33c67987` accounts (SHA-256 pinned): 7 real banks decode with the exact 0.1.11 layout, the 1064-byte historical group layout and a foreign-owned bank are refused; 5 mutations. FE-01 stays INCOMPLETE: current Mainnet parity NOT_RUN.
- FE-02/B4 `91ed707a`: `SolCosts`, `sol_debit_lamports`, flat SOL fee as conservative cost; 4 mutations. No current executable candidate: NO_ECONOMIC_GO unchanged.
- C3 `2605556c`: settlement in the FE-04 journal (`reconcile_observed`, `reconcile_not_landed`, `settle` → `Phase::Settled`); 6 tests, 5 mutations; C3 PARTIAL (no production caller, no RPC non-landing proof).
- FE-04 `8bfa63f2`: durable admission at the pipeline handoff, settled replay into the day; counting fake submitter (1 submit, double trigger 0, refusal without hold, SOL budget, lost transport UNKNOWN + HALT); 6 mutations; B1/B3/B15 PARTIAL.
- S1-02/S1-08: Yellowstone protobuf of `rpcpool/yellowstone-grpc` `2a548669` (field-number fidelity test), mapping onto `IngressEvent`/`StateCore` (write version, roots, fork orphaning, duplicates, contradicting copies, dead/cleared banks, parent gaps), gRPC transport h2 0.4.19 + rustls (loopback HTTP/2 tests: streaming, ping/pong, token header, 6 rejection cases, unreachable provider, endpoint rules), source control OFF/OBSERVE/ACTIVE, provider admission, arbiter, health; 18 mutations. Offline bench (release, M4 Max, n=20000, frame decode + map): account 165 B p50 0.92 µs / p99 1.08 µs, v0 transaction with ALT and meta p50 7.0 µs / p99 9.96 µs.
- NOT_RUN: PublicNode capability/quota/latency/coverage/fork fidelity, real TLS session, S1-RACE, K-02/K-02b, Mainnet read, LIVE.

## 2026-10-23 — A23 package and S1-02 owner integration (LOCAL_PASS offline; Actions PASS)

Commits `f1f90572`, `39d53f67`, `0db0a1f2`, `34ecc8a6`, `2fb6d214`, `caa605dc`.

- Gate on `caa605dc` (exit 0 each): 764/0/10, 762/0/10, runner 358/0/3 ×2, release, Python OK. GitHub Actions `project0-gate` 5/5 PASS (run 39993951105, Linux, toolchain 1.93.1); job `native` (gateway) red since BSC `9a43fb05`, outside Project_0.
- owner_wakeup_ns (debug test, 1000 results): p50 27 µs, p95 49 µs, p99 74 µs, max 135 µs; push→applied p99 87 µs. Real-run p99 NOT_MEASURED (P6-MT open).
- Mutations fail-before: A23-01/08 11, A23-03/04 6, S1-02 4 (+1 equivalent).
- NOT_RUN: Mainnet, provider, S1-RACE, LIVE.

## 2026-10-23 — FE-02 ZERO_PREFUND hardening (LOCAL_PASS offline)

Commits `106c11c2`, `4a2168f7`.

- Gate on `4a2168f7` (exit 0 each): 771/0/10, 769/0/10, runner 358/0/3 ×2, release, Python OK.
- Repay floor from the signed Jupiter v6 swap bytes (IDL `jupiter-cpi@12bc5f67`, SHA-256 `764ea6d7`); enforced slippage on the plan; floor rejections visible beside other plans; Token-2022 fee/hook/undecodable mints refused before quoting; surplus at the low-biased price; worst-case daily loss; derived admission evidence; `P0_ZERO_PREFUND_QUALIFIED=true` refused. 13 mutations fail-before.
- Synthetic Jupiter responses only (no recorded `/build`); deployed Jupiter program parity, current candidate, positive net edge: NOT_RUN / NOT_PROVEN.

## 2026-10-23 — FE-02 E2E and FE-04 C3/C12 settlement (LOCAL_PASS offline)

Commits `263624a0`, `d850d478`.

- Gate on the package (exit 0 each): 775/0/10, 773/0/10, runner 361/0/3 ×2, release, Python OK.
- FE-02 end to end through the real Jupiter/RPC adapters on loopback (synthetic `/build`): ZERO_PREFUND selected and simulated PASS with a linked record; Known but negative when it pays the record rent.
- C12/C3: Intent schema 3 (`last_valid_block_height`; pinned schema-2 bytes 1108 B still read, schema 3 1117 B), block-height non-landing proof, `settle_step`, conservative valuation; 10 mutations fail-before.
- NOT_RUN: real RPC history/archive behaviour, Mainnet, LIVE.

## 2026-10-23 — C3 FE-04 settlement in the running owner (LOCAL_PASS offline)

Commit `122e7e9c`.

- Gate on `122e7e9c` (exit 0 each): 778/0/10, 776/0/10, runner 361/0/3 ×2, release, Python OK. 6 mutations fail-before.
- Owner holds the FE-04 journal; settlement worker reads off the owner; Safety booked once (reserved capital released once, pre-restart finalized attempt books its day, settled attempt not again); UNKNOWN keeps holds, 10-s backoff.
- NOT_RUN: real RPC settlement read, landed attempt with real token deltas, Mainnet, LIVE.

## 2026-10-23 — FE-04 financial close hardening (LOCAL_PASS offline)

Commit `8c7acbba`.

- Gate on `8c7acbba` (exit 0 each): 783/0/10, 781/0/10, runner 362/0/3 ×2, release, Python OK; GitHub Actions `project0-gate` 5/5 PASS. 4 repro tests FAIL on `122e7e9c`, 8 mutations fail-before.
- Booking second persisted (FE04_TRANSITION schema 4, settled pin 1330 B; schema-3 pin 1117 B unchanged); token inflows = open inventory (unrealized), realized = SOL + outflows; refused Safety booking retains holds; RAM reconcile telemetry only.
- NOT_RUN: inventory exit (sale) evidence, real chain reconciliation, Mainnet, LIVE.

## 2026-10-23 — FE-04 exit core (LOCAL_PASS offline)

Commit `c7b0e6f1`.

- Gate on `c7b0e6f1` (exit 0 each): 789/0/10, 787/0/10, runner 367/0/3 ×2, release, Python OK; GitHub Actions `project0-gate` 5/5 PASS. 11 mutations fail-before.
- Exit automaton in the wallet journal (schema 5, pin 2291 B; schema-4 pin 1330 B reproduced): only a settled whole-position sale at/above its floor realizes; partial/foreign/below-floor/unknown stay UNKNOWN with holds; restart replays each exit into its day.
- NOT_RUN: exit route/build (native SOL unwrap), owner exit settlement, approval CLI, real chain exit, Mainnet, LIVE.

## 2026-10-23 — FE-04 owner exit settlement (LOCAL_PASS offline)

Commit `cbb0c243`.

- Gate on `cbb0c243` (exit 0 each): 792/0/10, 790/0/10, runner 367/0/3 ×2, release, Python OK; GitHub Actions `project0-gate` 5/5 PASS. 6 mutations fail-before.
- Owner reads, applies and books exits (tagged worker reads, booking = restart replay, unpriced finalized exit settled later from the journal); fixed-oracle SOL test: +5000 lamports → 749 USD micro realized.
- NOT_RUN: exit route/build, approval CLI, real chain exit, Mainnet, LIVE.

## 2026-10-23/24 — FE-04 exit admission and approval profile (LOCAL_PASS offline)

Commits `760f858c`, `0b3db124`.

- Gates (exit 0 each): `760f858c` 794/0/10, 792/0/10, runner 367/0/3 ×2; `0b3db124` 796/0/10, 794/0/10, runner 368/0/3 ×2; release, Python OK; GitHub Actions `project0-gate` 5/5 PASS on both. 5 + 4 mutations fail-before.
- Exit admission over loopback Jupiter/RPC through the real adapters: native-SOL sale of the whole open position (floor 19 900 000 lamports from the signed bytes, exact message fee, one simulation, one send); seven refusals without hold or send.
- `p0ctl approval show`: strict schema-1 attempt/exit profile, static check, bindings to the clean HEAD and the p0ctl+p0-longtail digest; exit 0 only when valid and both MATCH.
- NOT_RUN: real Jupiter route, real chain exit, production caller (FE-06), Mainnet, LIVE.

## 2026-10-24 — FE-01 smoke read, approved (Mainnet read-only)

Evidence `docs/evidence/FE01_SMOKE_20261024/fe01_observation.json` (observed 2026-10-23T23:54:49Z).

- User approval 2026-10-24: FE-01 smoke only, public `https://api.mainnet-beta.solana.com`, profile smoke (3 calls, 1 RPS, 1 attempt, no fallback, 30 s, 16 MiB). Actual: 3 calls (getAccountInfo ×2, getMultipleAccounts ×1), 610 B sent, 13 988 167 B received, status `OBSERVED_NOT_QUALIFIED`, exit 0.
- Program `MFv2…VacA` executable; ProgramData `4Q8u…PQR7` SHA-256 `3e3626a9…00de`, deployment slot 456 093 551, context slots 460 637 278–294; both FE targets present (2312 B).
- Finding: ProgramData ≠ pinned 0.1.11 reference `H57mq…` (`eec8f8f7…`, observed at slots ≈455.09 M); deployed after the reference ⇒ marginfi upgraded. FE-01 BLOCKED until the deployed source is identified and pinned; source parity UNKNOWN, no simulation, no LIVE.

## 2026-10-24 — P1B: marginfi 0.1.12-rc2 integrated (LOCAL_PASS offline)

Commit `c9decd7f`.

- Gate on `c9decd7f` (exit 0 each): 805/0/10, 803/0/10, runner 368/0/3 ×2, release, Python OK; GitHub Actions `project0-gate` 5/5 PASS (`contract-and-ui` red from BSC frontend TS errors, outside Project_0). 9 mutations fail-before.
- Pins `mrgn-0.1.12-rc2` (`9fcb729a`); 3912 B banks decode, 1864 B fail closed; RC2 premium semantics in Health/sizing/economics; deployment unqualified (reference bound to its 0.1.11 source) → runtime halted.
- NOT_RUN: bank resize per bank, FE-01 parity on the new deployment (P1B-03 steps 5–6), Mainnet, LIVE.

## 2026-10-24 — FE-04 attempt archive, cumulative FE-07 accounting, read-only treasury view (LOCAL_PASS offline)

Commits `286f9cc8`, `366271b5`. Evidence `docs/evidence/FE04_ACCOUNTING_20261024/README.md`.

- Gates (exit 0 each): `286f9cc8` 822/0/10, 820/0/10, runner 377/0/3 ×2; `366271b5` 823/0/10, 821/0/10, runner 378/0/3 ×2; release, Python OK. 6 + 2 mutations fail-before.
- One journal = one attempt; a finished attempt (settled, exit settled, no position) is sealed into `attempts/<wallet>/<approval_id>.p0j`; approval ids single-use across the archive; owner start replays archived bookings into the Safety day; corrupt/unsealed archive files fail closed.
- `execution::accounting::Ledger`: cumulative realized net over all own attempts and exits, fees, open-inventory mark, FE-07 verdict (`FIRST_REALIZED_PROFIT_EVIDENCED` only all-settled, no position, positive); `withdrawable` = min(realized profit in the asset, balance − holds − reserve), UNKNOWN for cross-asset profit or unknown balance. `p0ctl fe04 accounting|archive`, `approval show --journal-dir`.
- NOT_RUN: real attempts/exits/chain finality (FE-06/07), production admission caller (FE-06), treasury transfer (no code, separate approval), Mainnet, LIVE.

## 2026-10-24 — Paket 2: P2-04 premium scheduler, P3-03 tag-adjusted premium, P2-05 tag scheduler (LOCAL_PASS offline, rc2)

Commits `4c2cd285`, `b231be36`.

- Gates: `4c2cd285` 826/0/10, 824/0/10, runner 378/0/3 ×2, release, Python PASS; `b231be36` fmt, clippy ×4, 830/0/10, 828/0/10 PASS, runner 379/0/3 ×2, release, Python PASS before a clippy-only merge of one condition. 17 distinct mutations fail-before (P2-04 6, P3-03 4, P2-05 7; the `b231be36` message says 14 — wrong count); 1 equivalent at test resolution (per-second decay valuation).
- P2-04: healthy premium-active accounts wake the R127 sweep at the first chain second pending premium alone makes them liquidatable; decay valued per day and rounded up (never late; the per-second I80F48 value was), boundary = first second with health < 0; scheduler heap compacted.
- P3-03/P2-05: sizing uses the program's `tag_adjusted_premium` (base on bad debt / EMODE boost from the crate's health cache); a tagged plan below `min_profit` records `t_profitable_unix_s` (`tag_wakeup`, early side) and the owner re-evaluates and re-forwards it then.
- NOT_RUN: P1A deployment parity (BLOCKED), Mainnet, real tagged accounts, wake-ups in the non-default threshold mode. Separate: CI-N1 (`native` job, gateway step).

## 2026-10-24 — Paket 3: JFI-04 closed, no schema-1 writer, PERSIST-05 negative matrix (LOCAL_PASS offline)

Commits `1d62596b`, `a5609301`. Evidence `docs/evidence/BUGS_20261024/a23_09_jfi04_inventory.md`.

- Gates: `1d62596b` fmt, clippy ×4, 835/0/10, 833/0/10, runner 384/0/3 ×2, release, Python PASS; `a5609301` 836/0/10, 834/0/10, runner 385/0/3 ×2, release, Python PASS. 8 mutations fail-before, 1 equivalent (redundant symlink check).
- JFI: FE01_CONFIG/READ/RESULT typed (schema 2), RPC answer decoded into `WireMultipleAccounts` at the edge, report as text; C0 census import typed from a `key=value` handoff; Jupiter `otherAmountThreshold` typed (JFI-FIX-07); `LEGACY_WRITABLE` empty, schema-1 writer API removed (encoder test-only), `JsonlSink` test-only. Inventory 354 occurrences in 40 files, no internal category.
- Historical schema-1 FE-01 journals and `read_*.json` replay through `fe_replay::historical_read` — fixture-proven only: no real FE-01 journal exists (the 24.10. smoke used the Python probe).
- PERSIST-05 negatives added: chain break behind valid CRC/SHA, forged manifest (manifest only by `seal`), segment link missing / unsealed intermediate / symlinked successor, fsync failure at segment rotation and seal, econ report-journal write failure, market queue stall.
- Open: Mainnet volume/tails (NOT_MEASURED), physical fault test, Python FE-01 smoke probe still writes JSON evidence, journal-level reorg (by design: finalized-only settlement), SK-08 SQLite deferred.

## 2026-10-24 — Paket 4: P6-MT MT2a, MT3, MT4, MT5a, MT5b (LOCAL_PASS offline)

Commits `1a05bd78` (MT2a), `f52e7c21` (MT3), `3e745129` (MT4), `d73f5ae5` (reporter test for protocol `0495fd04`), `fe755331` (MT5a), `9dbb6607` (MT5b). Implemented under the Fable rule (P6-MT.17); reviewed, extended, mutation-proven and gated by the integrator.

- Gates PASS each; latest `9dbb6607`: fmt, clippy ×4, 876/0/10, 874/0/10, runner 400/0/3 ×2, release, Python. 13 mutations fail-before.
- MT2a: exact Health once per detection (3 → 1), carried through pipeline, builder and sizer. MT3: lanes L0/L1/L2, WDRR 8:3:1, `t_rx_first` kept, FIX-21 freshness unchanged. MT4: route-cap cache (hint only, real build validates, re-probe every 6th hit). MT5a: `state_epoch`, readsets (bound 199), candidate identity, central result policy on projection results. MT5b: per-account ownership IDLE→EXECUTING→SIGNED→SUBMITTED→RELEASED, SUBMITTED from the P8-RES-02 book.
- CI: `3e745129` project0-gate red (reporter test vs protocol `0495fd04` from the frontend session), green again from `d73f5ae5`.
- NOT_RUN / open: P6-MT.15 gate values (duty cycle, wake-up p99, L0 waits, NO_SWAP T_state→T_sign) NOT_MEASURED; C0-MO, MT11/MT12 (Mainnet, user); MT6–MT10 after C0-MO per P6-MT.16; local fee model (≥10k qualified messages); UB prioritization of remote order (MT8 queue); P6MT-J1 (route/fee/simulation on the owner) and P6MT-R1 (retry wait in the caller).

## 2026-10-24 — Paket 5: JIT, source control, Geyser, lead, p0ctl (LOCAL_PASS offline, PARTIAL)

Commits `26f6194b` (JIT), `98254f75` (source control, Geyser, lead, p0ctl/UntilStopped).

- Gates PASS: fmt, clippy ×4, `888/0/10`, `886/0/10`, runner `407/0/3` ×2, release, Python. 27 mutations fail-before (JIT 10, source control 5, Geyser 4, lead 4, p0ctl/horizon 4).
- JIT: buy spread charged as swap cost, holds liability and funding inventory, enforced buy slippage, class name matches the FE-04 approval, production funding bank `P0_JIT_FUNDING_BANK` (empty = no JIT).
- Source control: every request in sequence order, a full owner queue delays instead of dropping; the owner's decision is a typed record (`SOURCE_CONTROL_OUTCOME` 1101) shown by `p0ctl source status`.
- Geyser: WSS backoff policy, the budget counts only consecutive failed sessions, stop during backoff; one token variable. Lead: canonical WSS copy measured against the observed Geyser copy (content + slot), only while an observer runs.
- p0ctl: `run preflight`, `runs list`, `run stop --run`, `--profile`; `RunHorizon::UntilStopped` typed, start refused `RUN_HORIZON_UNTIL_STOPPED_GATED`.
- Open: JIT admission (bundle fees unquoted), current quotes/simulation NOT_RUN, cross-class quote budget; backtest/optimizer (native typed jobs; the existing replay is the legacy JSONL path); inventory planner has no caller (live balances + quotes); UntilStopped activation (rolling SK-07, runway preflight); gateway capture path without BOOT/lock (outside Project_0); Geyser `created_at`; provider NOT_RUN.

## 2026-10-25 — Korrekturauftrag Einheit 0 und F-01 (LOCAL_PASS offline)

- Einheit 0 (`bf646966`…`c14bc83b`, `c8e75987`): keine produktive interne JSON-Kante mehr (JFI-04 offene Liste leer, Pin 287); Strict-Cache typisiert (`STRICT_CHAIN_CACHE` 318); Layout-Pins 318/350–355/704; Gate PASS (fmt, Clippy ×4, 905/903/421/421, Release, Python).
- F-01/C07 (`760fc483`, `276e0998`): Provider-Budget immer aktiv; Shadow nur über `p0ctl run start --mode shadow`; `CaptureState.provider` (Schema-ID `e545c226…`) bis Capture-Reiter; Reporter/Gateway-E2E PASS; nicht deployt (Reporter-Rebuild nötig).
- NOT_RUN: Gap-Smoke und jeder Mainnet-Lauf (Freigabe nötig); Review U-6 offen; Owner-Publisher (Shadow/econ4h → Overview/Liquidator/Wallets) offen.
- Einheit 1 (`77f00f04`, `214679e1`, `b7f4b296`): Wanduhr-/Chain-Uhr-Fallbacks auf 0 entfernt (Verweigerung/Unknown), unbewertetes Inventar Unknown; Gate PASS auf `b7f4b296` (918/916/427/427, Release, Python). Review (ein Agent) Einheit 0+1: 14 Befunde behoben `4035dd80` (u. a. econ4h immer Layout V2, Shadow-Run-ID im Owner-Lock, Backtest matched/Mindestgewinn, FE-01-Smoke Owner-Prüfung).
- Einheit 4 B03/U-9 (`e8748c3a`…`79d0d039`): Journal-Kinds 356 `ANALYTICS_JOB_REQUEST`/357 `ANALYTICS_JOB_OUTCOME` (Schema 2, Layout-Pins), Limits-Revision 9 (`analytics_status_ms`, `analytics_runs_listed`, `analytics_jobs_listed`; Revisionen 4–8 lesbar), Topic 15 `AnalyticsJobs` mit eigenem Publisher `p0-analytics-reporter`; Lauf mit echten Daten NOT_RUN.
- Einheit 4 B01/S1-02 (`67815a7e`, `72b74fc1`, `43c8c86f`): Limits-Revision 10 (`rebalance_observe_ms`; Revisionen 4–9 lesbar), `LiquidatorState.rebalance/sources/last_source_decision/source_stats` (owner.proto 7–10); Offline-Gate `37e68357` + Runner-Clippy `fbe84af3` PASS; Rebalancing-Quotes/Provider-Lauf NOT_RUN.
