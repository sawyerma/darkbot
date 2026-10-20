# Project 0 — Current status

**Snapshot:** 2026-10-17 · project_0 code at `0e0544d` plus the SK-06 completion of this date (readiness tracker [C0E1_4H_READINESS](qualification/C0E1_4H_READINESS.md)) · Ticket 3: A0/A1/A2 LOCAL_PASS, A3 PARTIAL, A4 LOCAL_PASS in scope (class coverage BLOCKED) · storage: SK-03 PARTIAL, SK-04 LOCAL_PASS, SK-05 PARTIAL, SK-06 LOCAL_PASS in scope (real basis NOT_MEASURED), SK-07 OPEN, SK-08 DEFERRED_PRIORITY, SK-09 OPEN · BSC-only commits on `main` qualify nothing in project_0. Sections 1–2 below keep the 2026-10-16 PERSIST base (`4a27a1c`); §12 is the current per-ticket matrix.

Canonical facts: [PHASENPLAN.md](PHASENPLAN.md), [MEASUREMENT_LEDGER.md](MEASUREMENT_LEDGER.md), their JSON navigation indices, actual Rust code and [PERSIST evidence](evidence/PERSIST_BINARY_20261016/qualification.json). Previous October 14 code locations and test counts are historical; no gate is upgraded by this status page.

**Local follow-up on `9047c7b` (uncommitted):** strict batches now use the binary sink; deterministic fsync error paths and bounded, sealed FE-01 replay pass offline. [Follow-up report](development_history/persist_followup_20261016.md), [evidence](evidence/PERSIST_FOLLOWUP_20261016/qualification.json). Overall PERSIST PARTIAL and FE-01 OPEN remain unchanged.

## 1. Bottom line

| Scope | Verdict | Current evidence |
|---|---|---|
| First realized net profit | **NOT PROVEN** | `MEASUREMENT_LEDGER.json` overall |
| LIVE / value-bearing submission | **DISABLED / NOT AUTHORIZED** | FE-06 remains locked |
| FE-00 | **LOCAL_PASS; NO_ECONOMIC_GO** | Existing FE-00 ledger/evidence |
| FE-01 | **OPEN** | Offline replay/integration-account observation improved at `4a27a1c`; current deployed ProgramData/state and funding-class parity NOT_RUN |
| FE-02 / FE-03 | **OPEN** | No current profitable executable candidate / qualified targeted Shadow |
| FE-04 | **PARTIAL_LOCAL** | Durable binary wallet transitions locally tested; production integration and exit/PnL remain OPEN; deterministic fsync error paths pass locally |
| FE-05 / FE-06 / FE-07 | **NOT_RUN / NOT_AUTHORIZED / NOT_RUN** | Separate provider/Mainnet/value-bearing permissions still required |
| C0-E1 long-duration market master | **NOT_QUALIFIED** | Historical 4h/5h incomplete |
| Binary journal PERSIST-01–05 | **PARTIAL overall, NOT QUALIFIED** | [Qualification](evidence/PERSIST_BINARY_20261016/qualification.json); see below |

## 2. Current code and evidence

| Code / evidence | Scope | Qualification |
|---|---|---|
| `8bd462e` | Runtime refactor: four crates; orchestration/execution/market/analytics domains | Local tests reported; no economic GO |
| `bb26313` | Initial typed owner writer and binary journal WIP | Intermediate version; superseded by `4a27a1c` |
| **`4a27a1c`** | Production binary journal integration, FE-04 transition journaling, capture/MT0/econ/census/FE-01 journal/replay, reader/export tooling | **PERSIST PARTIAL**, not Mainnet-qualified; Rust 415/417 PASS with 2 ignored each, Python 107 PASS, verifier 29 PASS per recorded local evidence. |
| [Implementation report](development_history/persist_binary_completion_20261016.md) | Exact migration details and limits | Historical report written before commit; now checked in |
| **`479c67a`** (2026-10-21) | [BUGS.md](BUGS.md) E9 part 2: projection consumed only once the owner processed every confirmed account notification up to its simulation slot (strict notification watermark, snapshot bound, root; tradable state only), parked otherwise; ContextBehind backoff; trigger constants — **E9 closed** | **LOCAL_PASS.** Gate: fmt, clippy all/default, `test --all-features` 683/0/6, default 681/0/6, release, Python 108 (1 skipped) exit 0 · 12 more fail-before mutations. NOT_MEASURED: the cross-subscription notification ordering of the real WSS provider (assumption the watermark rests on), oracle cadence, provider `minContextSlot`, drop rate after parking. **No** Mainnet read, LIVE or capture. |
| **`9602562`** (2026-10-21) | [BUGS.md](BUGS.md) E9 part 1: projection context lower bound (`minContextSlot`, typed −32016/older context), ProgramData dependency, bounded FIFO requeue per external trigger | **LOCAL_PASS**, E9 stays PARTIAL (review 21.10.: context upper bound at consume, requeue backoff, typed triggers open). Gate: fmt, clippy all/default, `test --all-features` 680/0/6, default 678/0/6, release, Python 108 (1 skipped) exit 0 · 23 fail-before mutations. **No** Mainnet read, LIVE or capture. |
| **`9a0ae35`** (2026-10-20) | [BUGS.md](BUGS.md) R12 rest: tight `econ_retention_mib` byte budget fail-closed through the econ worker (refused, counted, nothing evicted, INCOMPLETE) and exact `P0_ECON_RETENTION` bound; test-only | **LOCAL_PASS**, R12 stays PARTIAL (historical byte peak NOT_MEASURED: all 22 runs predate SK-03). Gate: fmt, clippy all/default, `test --all-features` 676/0/6, default 674/0/6, release, Python 108 (1 skipped) exit 0 · 6 fail-before mutations. **No** Mainnet read, LIVE or capture. |
| **`d06393e`** (2026-10-20) | [BUGS.md](BUGS.md) V19: census layout class from a typed `HealthError::UnknownLayout` instead of error text, texts byte-identical | **LOCAL_PASS.** Gate: fmt, clippy all/default, `test --all-features` 674/0/6, default 672/0/6, release, Python 108 (1 skipped) exit 0 · 4 fail-before mutations. **No** Mainnet read, LIVE or capture. |
| **`02ac861`** (2026-10-20) | [BUGS.md](BUGS.md) E2 rest: the signed message must be exactly its plan (decompiled incl. LUT, account metas, privileges, compute-budget data, payer, no foreign key); V18 review note (`tx version` via `record_name`) | **LOCAL_PASS** (E2 stays PARTIAL: swap/prerequisite `Other` program-bound). Gate: fmt, clippy all/default, `test --all-features` 672/0/6, default 670/0/6, release, Python 108 (1 skipped) exit 0 · 8 fail-before mutations. **No** Mainnet read, LIVE or capture. |
| **`b30e0ea`** (2026-10-20) | [BUGS.md](BUGS.md) V18 (= [bugs_b.md](bugs_b.md) Q3): explicit persisted names instead of derived Debug, byte-identical (PARTIAL: SDK error Debug in the simulation failure code); new finding V19 | **LOCAL_PASS.** Gate: fmt, clippy all/default, `test --all-features` 671/0/6, default 669/0/6, release, Python 108 (1 skipped) exit 0 · 6 fail-before mutations. **No** Mainnet read, LIVE or capture. |
| **`0dbdc35`** (2026-10-20) | [BUGS.md](BUGS.md) V17 (= [bugs_b.md](bugs_b.md) Q5): typed `PlanError`/`SizingError` from source to `Pipeline::evaluate`, classification by kind only, texts byte-identical (PARTIAL: kind not persisted) | **LOCAL_PASS.** Gate: fmt, clippy all/default, `test --all-features` 670/0/6, default 668/0/6, release, Python 108 (1 skipped) exit 0 · 9 fail-before mutations incl. the old `contains("unavailable")` rule. **No** Mainnet read, LIVE or capture. |
| **`c7ccce0`** (2026-10-20) | [BUGS.md](BUGS.md) V14–V16 (= [bugs_b.md](bugs_b.md) Z13 bankruptcy provenance, Z19 WSS health during silence, Z17 real normal/closeout sizing test), V9 loss from the furthest technical stage (PARTIAL: LIVE safety denial), V13 probe-backlog line | **LOCAL_PASS per ticket.** Gate: fmt, clippy all/default, `test --all-features` 668/0/6, default 666/0/6, release, Python 108 (1 skipped) exit 0 · RED/fail-before mutations per ticket (Z13 2, V9 4, Z19 1, Z17 4, V13 2). **No** Mainnet read, LIVE or capture. |
| **`3f901d3`** (2026-10-20) | [BUGS.md](BUGS.md) V10–V13 (= [bugs_b.md](bugs_b.md) Q2 safety ledger, Z18 integration identity, Z12 pair error isolation, Z14 ingress race) | **LOCAL_PASS per ticket.** Gate: fmt, clippy all/default, `test --all-features` 665/0/6, default 663/0/6, release, Python 108 (1 skipped) exit 0 · RED on the old code for each ticket (Z14 reproduced: 63/1,024 ingress slots free after recovery), fail-before mutations. **No** Mainnet read, LIVE or capture. |
| **`78d79c3`** (2026-10-20) | [BUGS.md](BUGS.md) V2 (= [bugs_b.md](bugs_b.md) Z2, completes B6): one monotonic candidate deadline over Jupiter planning, route-fit re-quotes, fee quote, simulation, provider permit, HTTP request and response read; typed fee-quote errors; adapters without default methods | **LOCAL_PASS per ticket.** Gate: fmt, clippy all/default, `test --all-features` 661/0/6, default 659/0/6, release, Python 108 (1 skipped) exit 0 · 12 deadline tests with the real `RpcSubmitter`/Jupiter provider against loopback HTTP (return ≤ deadline + 100 ms, no request after the deadline), stability [evidence](evidence/BUGS_20261020/V2_deadline_stability.txt). **No** Mainnet read, LIVE or capture. |
| **`423c024`** (2026-10-20) | [BUGS.md](BUGS.md) §18 V1/V3/V4/V5/V6/V8 (= [bugs_b.md](bugs_b.md) Z1/Z3/Z4/Z5/Z6/Z8), V7 stage 1 PARTIAL; ECON_SNAPSHOT/ECON_FINAL schema 4 with explicit earlier layouts | **LOCAL_PASS per ticket.** Gate: fmt, clippy all/default, `test --all-features` 649/0/6, default 647/0/6, release, Python 108 (1 skipped) exit 0; golden over real schema-2/3 journals of the old writers. **No** Mainnet read, LIVE or capture. |
| **`119da22`** (2026-10-20) | [BUGS.md](BUGS.md) package 5: R23, D2 closed; B4, R24, D12, C8, B7, R1, E8 PARTIAL; E2, E9, R12 reopened | **LOCAL_PASS per ticket.** Gate: fmt, clippy all/default, `test --all-features` 640/0/6, default 638/0/6, release, Python 108 (1 skipped) exit 0; fail-before per fix by mutation. **No** Mainnet read, LIVE or capture. |
| `e08d3a7`, `9fad87d`, `07c3f09`, `16ec71a` (2026-10-19) | [BUGS.md](BUGS.md) packages 1–4 (state per ticket there) | LOCAL_PASS or PARTIAL per ticket as recorded in BUGS.md; offline only. |
| **`1d9c6ed`** (2026-10-18) | [BUGS.md](BUGS.md): F6 (rustfmt of the four reporter files, `3b2e8c0`, approved by the user); X1 (launched runtime bound to its approved network audit: record handoff p0ctl → launcher → runtime, refusal before any owner thread, identity binding at the first Ready; `a3ca33c`, follow-up `1d9c6ed`) | **LOCAL_PASS per ticket.** Gate 4 on `1d9c6ed`: `fmt --check`, clippy, `test --all-features` (565 passed / 0 failed / 6 ignored), release build and Python (108, 1 skipped) all exit 0 — first fully green local workspace gate of the hardening. The first Gate 4 run on `c8d2e33` failed Python (4 launcher E2E errors, fixed in `1d9c6ed`). **No** Mainnet read, network qualification (G-B), calibration or LIVE release. |
| **`0de1b9b`** (2026-10-18) | [BUGS.md](BUGS.md) Review 7 (§15/§16): R2 (panicked ingress never seals COMPLETE), R3 (recovery rebuild + full sweep measured: 227–234 ms/100k, no overflow at 50k events/s), C1 test 7 (real owner end to end offline), R4 (BOOT read worst case + doctor line), R5, R6, R9, X3 (BOOT-01/02 matrix, runtime_binary check), F5; R10/R11 NO_BUG_VERIFIED; R1 partial | **LOCAL_PASS per ticket.** Gate 3 on `0de1b9b`: clippy, `test --all-features` (560 passed / 0 failed / 6 ignored), release build and Python (108, 1 skipped) exit 0; `fmt --check` exit 1 only for four foreign reporter files (F6). **No** Mainnet read, network qualification (G-B), calibration or LIVE release. |
| **`d8cc32a`** (2026-10-18) | [BUGS.md](BUGS.md) hardening: C6/D10 + C1 tests 4/6, E4/E5/E6 (BOOT-02 writer probe, bounded seal screening, run volume), D3 (sweep owner-block metric + offline stress), B2/E1/D11/B11 (exact pinned Marginfi decoders), E3/E11 (refresh discriminators from pinned IDLs), F4, E10, C9, F3; earlier the same day H/M/N/L, D1/D4/D5/D8/D9, B5, B13, C7, X2 | **LOCAL_PASS per ticket** (offline tests, `docs/evidence/HARDENING_20261018/`). Gate 2 on `8eccd8f`: clippy `--all-targets --all-features -D warnings`, `test --all-features` (541 ok) and release build exit 0; `fmt --check` exit 1 only for three foreign reporter files of `311f9a4` (BUGS F6). **No** Mainnet read, network qualification (G-B), calibration or LIVE release. |
| [Ledger dated PERSIST entry](MEASUREMENT_LEDGER.md) | Canonical PERSIST verdict and next gaps | Current |

**PERSIST-01 PARTIAL:** runtime and strict batch primary records now use the existing binary journal; the unchanged pinned verifier consumes derived JSONL views at the batch boundary. Historical proof IDs remain immutable; new binary digests use the existing spec identity mechanism.

**PERSIST-02 LOCAL_PASS:** typed bounded owner-to-recorder path and offline source guard; owner reaction latency under Mainnet load NOT_MEASURED.

**PERSIST-03 PARTIAL_LOCAL:** FE-04 binary durability barrier/UNKNOWN replay tested; deterministic I/O-boundary fsync errors pass locally. Physical filesystem faults and real pilot integration remain unqualified.

**PERSIST-04 LOCAL_PASS:** existing historical JSON/JSONL and verifier contracts preserved; journal-derived JSON reports remain compatible exports, never a second canonical journal.

**PERSIST-05 PARTIAL:** local synthetic writer throughputs 65,111 market frames/s and 111,481 opportunity frames/s, p99 append 24.7/11.8 µs; journal size **1.077×/1.746× JSONL** respectively. FE-04 append+fsync p99 ~5.31 ms. No Mainnet stress/latency/volume qualification; no proven disk-saving or profitable-trade speedup.

## Historical status matrix (snapshot 2026-10-14; superseded where inconsistent)

The following capability and FE tables are retained as historical detail only; their outdated file paths, counts and WIP claims are not the current status. Current verdict is sections 1–2 above and the 2026-10-16 dated ledger entry. No historical content below grants LIVE authorization.

## 3. Status levels

| Level | Meaning |
|---|---|
| **Implemented** | Code exists on `main`. |
| **Offline tested** | Unit/regression/fixture tests pass locally (mock RPC, loopback, recorded historical transactions). No network. |
| **Mainnet measured** | Read-only/Shadow run against Mainnet produced a recorded verdict in the ledger. |
| **Economically qualified** | Current, candidate-specific, executable positive conservative net edge including all costs and exit (FE-02) — and ultimately realized PnL (FE-07). |
| **LIVE approved** | Explicit separate user authorization for a value-bearing attempt (FE-06). |

## 4. Capability matrix

| Capability | Implemented | Offline tested | Mainnet measured | Economically qualified | LIVE approved |
|---|---|---|---|---|---|
| WSS/RPC ingress, bootstrap, recovery | Yes (`solana/src/ingress`, `state`) | Yes | Bootstrap PASS 2026-10-02 (L1389); recovery PASS 2026-10-03 (L1633, L1898); controlled 60 s market WSS disconnect: gap 1/1 recovered (user-tested, L177). Long-run continuity **FAILED**: 4h run 1 unrecovered gap (L5107-5228); 5h run 527 GAPs / HTTP 429 (PHASENPLAN L5005-5016) | n/a | No |
| Canonical state (snapshot → reconcile → READY) | Yes | Yes | PASS in bootstrap and C0-E1 runs (L1389, L4971-5044) | n/a | No |
| Exact Health (marginfi crate) and sizing | Yes (`liquidation/src/health.rs`, `sizing.rs`) | Yes | Exercised at scale: R127d 12.4 M sweep checks (L4028) | n/a | No |
| Candidate admission (R127d cache, P1-A revision, P1-B fairness) | Yes | P1-A PASS (FIX-134, L4211-4270); P1-B PASS (FIX-135, L4278-4310) | R127d Shadow PASS 2026-10-05 **before** FIX-134/135 (L4028); dedicated R127 requalification after the fixes **not recorded** | n/a | No |
| Economics, executable floor (P1-C), funding classes | Yes (`economics.rs`, `funding.rs`) | P1-C PASS (P6-MT T1, L4569) | Historical candidates only: 3 negative edges −121889 / −118606 / −91691 USD-micro (R124) | **No** — `NO_ECONOMIC_GO` (L5255-5263) | No |
| Builder, signing, transaction simulation | Yes (`builder.rs`, `solana/src/tx`) | Yes | Simulation call path proven (R97/R99/R101, L1964; FIX-126 v1 wire, L3203); R122: 2 historical technical ZERO_PREFUND successes, not current opportunities (ledger JSON `economic_evidence`) | **No** — full current candidate simulation NOT_PROVEN (FE-02) | No |
| Winner tracking / parsing (F1, F8) | Yes (`winner.rs`, `winner_tracker.rs`) | PASS locally (P6-MT T1, L4569); 6/6 real audit winners resolve (fixture tests) | No qualified Mainnet winner/race result | n/a | No |
| RPC / Jito submission infrastructure | Yes (`solana/src/submit`) | Mock/loopback only | No liquidation was ever submitted. Only separately authorized ATA prewarm/creation transactions (ledger L739, FIX-129 doc) | Jito lifecycle and tip costs UNKNOWN (FIX-129) | **No** |
| C0-MO market observer | Yes (`market_observer.rs`) | Yes | 600 s smoke 2026-10-08: enhanced `transactionSubscribe` HTTP 403; `logs` source observed 6 liquidations, verdict `MARKET_DATA_INCOMPLETE` (truncated logs), `INSUFFICIENT_SAMPLE` (`evidence/C0MO_20261008/SMOKE_600S.md`) | n/a | No |
| C0-E1 econ4h harness, MeasurementPlan, index/strict verification, reports (P1–P6) | Yes | P6 OFFLINE PASS (L5242-5254) | 600 s retention retest **PASS** 2026-10-08 (L4971-5044); 4h **INCOMPLETE** 2026-10-09 (L5107); 5h **INCOMPLETE** 2026-10-11 (PHASENPLAN L5005); historical index **OPEN** (RPC 429, L95-135) | n/a | No |
| Operational configuration contract | Yes (`runtime_config.rs`) | LOCAL_PASS (L5294-5304) | NOT_RUN | n/a | No |
| FE-01 read-only state/deployment qualification (`p0-fe-qualify`) | Yes, **WIP** (`4c271cb`) | Green per user baseline; not recorded in ledger | NOT_RUN | n/a | No |
| FE-04 one-shot journal, adapter binding, finalized receipts | Partial (`one_shot.rs`, `one_shot/`) | LOCAL PASS for the implemented parts (L5264-5291) | NOT_RUN | n/a | No |
| P6-MT parallel execution | Tranche 1 only | T1 PASS locally (L4569) | No | n/a | No |
| Drift integration refresh | Open | — | `Custom(101)` simulation evidence (audit); qualification OPEN since 2026-10-07 (L4635) | n/a | No |

## 5. First-profit release train FE-00 … FE-07

Acceptance criteria: `PHASENPLAN.json` → `first_profit_tickets`. Status: `MEASUREMENT_LEDGER.json` → `current_status.first_profit_release_train`, cross-checked against code.

| Ticket | Goal | Status | What exists / what is missing |
|---|---|---|---|
| FE-00 Opportunity Selection | Positive current candidate or `NO_ECONOMIC_GO` | **LOCAL_PASS · `NO_ECONOMIC_GO`** (`b1e6930`) | First requalification class: `ZERO_PREFUND_SAME_TX`. `NO_SWAP_INVENTORY` blocked (no qualified exit); `JIT_SHORTFALL_EXACT_IN` still required for the full product |
| FE-01 One-Class Freeze | Verified deployment/source/state parity and one executable funding class | **OPEN** | WIP tool `p0-fe-qualify` in code (`4c271cb`); its `--execute` refuses to run without measured FREE_ONLY provider evidence (`FREE_ONLY_PROVIDER_QUALIFICATION_REQUIRED`, `bin/p0-fe-qualify.rs`). Current on-chain observation NOT_RUN; requires separate permission |
| FE-02 Executable Economics | Full signed TX simulation, all costs, exit, positive net edge | **OPEN** | Current executable route floor, full simulation, realizable exit NOT_PROVEN |
| FE-03 Targeted Production Shadow | Shadow/replay with winner/latency/loss evidence | **OPEN** | No targeted Shadow for the selected class |
| FE-04 Serial Safety and Accounting | One-shot approval, persistent holds, crash-safe UNKNOWN, finalized reconciliation | **PARTIAL_LOCAL** (`a9a8c34`, `48db13c`, `cf540f4`) | Done locally: durable journal, signed adapter binding, finalized fee/SOL/token observation. Missing: terminal approval lifecycle, production Intent/Current at the pipeline boundary, qualified exit, attempt/cumulative-loss and realized-PnL accounting |
| FE-05 Read-only Preflight | Authorized bounded FREE_ONLY Mainnet observation → `PILOT_READY` or `BLOCKED` | **NOT_RUN** | Needs separate user permission and provider qualification |
| FE-06 Controlled One-Shot Mainnet | One authorized attempt, then HALT | **NOT_AUTHORIZED** | LIVE refused in code |
| FE-07 Realized Net PnL | Positive cumulative realized PnL from finalized evidence | **NOT_RUN** | No attempt exists |

## 6. Open measurements and safety gates

**Open measurements (none may be assumed PASS):**

- FE-01 current Program/ProgramData/state parity observation — NOT_RUN, permission required (ledger L5294-5304).
- FREE_ONLY provider qualification (WSS/RPC quota, archive) — NOT_RUN; prerequisite for `p0-fe-qualify --execute`.
- Saved-data replay/report of `market5h_20261011T010758Z` — NOT_RUN, user-owned (L5242-5254).
- Short read-only Mainnet smoke before any new long capture — NOT_RUN, permission required.
- C0-E1 four-hour market master — NOT_QUALIFIED; previous 4h and 5h runs INCOMPLETE.
- Separate Shadow WSS disconnect/readiness smoke — required by the 2026-10-10 gate, not recorded (L181).
- Dedicated R127 admission Shadow requalification after FIX-134/135 — not recorded.
- 72 h P6-LT Shadow — not started ([measurements/protocols/p6lt_shadow.md](measurements/protocols/p6lt_shadow.md)).
- BUGS.md items still open before 4h readiness (as of `1d9c6ed`, 2026-10-18): R1 rest (consumer buffer full/half-full, end-to-end capture drain; blocked ping/pong send and TLS handshake stall covered by `1ac2630`); X1 real network case (approved record against Mainnet state) NOT_RUN; real-load sweep owner-block p95/p99 (`sweep_timing` line), real ingress peak rate (R3) and Mainnet MarginfiGroup/FeeState lengths (B2) — NOT_MEASURED until an authorized G-B/G-C run.

**Safety gates in force (unchanged):**

- `P0_MODE=live` refused in code; submission disabled; econ4h uses `ObserveOnly`.
- FREE_ONLY: no paid provider or automatic paid fallback (ledger JSON `permissions_and_invariants`).
- Unknown economics or chain evidence is never zero and never a pass.
- Before any value transfer, all items in `PHASENPLAN.json` → `priority_contract.mandatory_before_value_transfer` must be evidenced.
- FE-06 requires an explicit, separate user authorization for code, wallet, program, budget and limits; one attempt, then HALT.

## 7. Key Mainnet measurements

| Date | Run / scope | Result | Source |
|---|---|---|---|
| 2026-10-02 | G5 Mainnet bootstrap | PASS | ledger L1389 |
| 2026-10-03 | R97/R99/R101 build → sign → simulation boundary | Simulation call path proven; no successful current receivership | ledger L729-749, L1964 |
| 2026-10-05 | R127d Mainnet Shadow `r127_20261005T171043Z` (queue/fanout) | PASS; correctness findings by the audit afterwards (fixed offline by FIX-134/135) | ledger L4028, L4118 |
| 2026-10-05 | FIX-129 fee sources (reads + signed simulations) | Exact-message pricing PASS; Jito costs UNKNOWN | [measurements/historical_reports/fix129_cost_qualification.md](measurements/historical_reports/fix129_cost_qualification.md) |
| 2026-10-08 | C0-MO 600 s smoke | `transactions` source HTTP 403; `logs` source: 6 liquidations observed, `MARKET_DATA_INCOMPLETE` (truncated logs), descriptive gross edge only | `evidence/C0MO_20261008/SMOKE_600S.md` |
| 2026-10-08 | C0-E1 retention-safe 600 s `c0e1_retention_smoke_20261008T215650Z` | Technical PASS (600 s only) | ledger L4971-5044 |
| 2026-10-09 | C0-E1 4h `c0e1_4h_20261008T222326Z` | INCOMPLETE: 1 unrecovered WSS gap; zero liquidations observed is **not** proof of none | ledger L5107-5228 |
| 2026-10-10 | Controlled market WSS disconnect smoke `c0e1_gap_smoke_20261010T195016Z` (60 s capture) | User-tested: technical PASS, `MARKET_DATA_VALID`, gaps 1/1 recovered, 0 unresolved slots, 0 submissions | ledger L177 |
| 2026-10-10 | Normal smoke `econ4h_20261010T204530Z` | User-tested `completeness=PASS`; details not independently inspected | ledger L178 |
| 2026-10-11 | Market 5h `market5h_20261011T010758Z` | INCOMPLETE: 527 GAPs, persistent HTTP 429, 15,960 s without slot evidence, 0 canonical blocks | PHASENPLAN L5005-5016; ledger L95-135 |

Raw run data lives only on the local machine (`project_0/data/`, not versioned).

**P0 PASS (2026-10-16):** reuse, license, routing/cost, oracle and staging gates closed; see [evidence/P0_20261016](evidence/P0_20261016/README.md).

**P1A BLOCKED (2026-10-16):** source-pinned freeze and startup upgrade pin done ([manifest](manifests/CURRENT_MAINNET.md)); confirming today's deployment needs one authorized read-only Mainnet read.

**P1B LOCAL_PASS, deployment BLOCKED (2026-10-16):** strict pinned Bank layout, dual-size tests, promotion procedure; RC2 probe type-checks after 4 call-site changes, IDL discriminators equal ([manifest](manifests/UPCOMING_0_1_12.md)). Which release runs on Mainnet: P1A read.

**C0 NOT_PROVEN (2026-10-16):** census rides on the R127 startup sweep with a bounded worker and the canonical Clock sysvar; identity coverage (missing/extra/duplicate accounts) and one typed `.p0j` row per debtor plus a result frame (C0-FIX-01..05 and Ticket 2 LOCAL_PASS); model classes only; historical evidence only ([C0_MARKET_CENSUS](evidence/C0_MARKET_CENSUS.md)); current run needs separate Mainnet read-only permission.

**JFI OPEN (2026-10-16):** typed `.p0j` payload contract and owner evidence migrated (JFI-FIX-01/02 LOCAL_PASS, owner opportunity writer p99 16.3 → 5.2 µs locally); FE-04, market capture, econ/verifier, measurement/MT0/FE-01/census kinds, runtime JSON config and the Jupiter REST boundary remain open.

## 8. Open development tasks

- **FE-01 WIP:** record its qualification (tests, Clippy differential, format, evidence directory, ledger entry) before it is relied on.
- **FE-02:** current candidate-specific parity, Health, Oracle/Bank, funding, Jupiter ExactIn minimum output, fees, full signed transaction simulation and exit economics.
- **FE-04:** terminal approval lifecycle; production Intent/Current at the existing pipeline boundary; qualified exit; attempt, cumulative-loss and realized-PnL accounting.
- **FE-03:** targeted production Shadow with winner/latency/loss evidence for the selected class.
- **Market qualification:** WSS gap recovery under provider 429, historical index/strict verification with a qualified FREE_ONLY provider.
- **Drift:** qualify or explicitly scope out (open since 2026-10-07).
- **Full product** (still binding, not on the first-pilot critical path): see `PHASENPLAN.json` → `priority_contract.mandatory_final_product` (Race/Geyser, Jito, JIT ExactIn, full P6-MT, multi-wallet reservations, inventory, P7/P4B, frontend, P8/P9).

## 9. Next technically necessary steps

Order follows the ledger's "next decisive work" and the code's actual preconditions. Nothing here authorizes a Mainnet call; each Mainnet step needs separate user permission.

1. Record the qualification of the FE-01 WIP code (`4c271cb`) in a ledger entry and evidence directory.
2. Measure FREE_ONLY provider capacity for the intended endpoints. `p0-fe-qualify --execute` requires this evidence.
3. With permission: bounded read-only FE-01 observation of the current Program/ProgramData/state, using reviewed effective settings (`--print-effective-config`, see [operations/operational_config.md](operations/operational_config.md)).
4. FE-02 candidate-specific proof: positive conservative net edge, or keep `NO_ECONOMIC_GO`.
5. Finish FE-04 runtime integration (approval lifecycle, Intent/Current, exit and loss accounting).
6. FE-03 targeted Shadow; FE-05 read-only preflight → `PILOT_READY` or `BLOCKED`.
7. Only then, and only with explicit separate approval: FE-06 one-shot, FE-07 realized PnL.

In parallel, user-owned: saved-data replay of `market5h_20261011T010758Z`, then a short read-only smoke before any user-started long capture.

## 10. Contradictions and documentation gaps

| # | Finding | Resolution |
|---|---|---|
| 1 | The README entry points and status block (2026-10-05) still listed candidate revision/fairness as BLOCKED and winner parsing/executable floor as UNPROVEN. The ledger records P1-A/P1-B PASS (2026-10-06) and F8/P1-C local PASS (2026-10-07). | README now points to this file; the handoff is marked historical. |
| 2 | Ledger §8 "Current status" (L729-749, FIX-105 era, ~2026-10-03) is stale, but `MEASUREMENT_LEDGER.json` → `reference_sections.measured_state_overview` still points to it. | Not changed (ledger/JSON frozen during the reform). Use the newest dated entries instead. |
| 3 | Code at HEAD contains FE-01 WIP (`4c271cb`) that the ledger and both JSON indices do not mention. | Documented here; qualification record is next step 1. |
| 4 | `MEASUREMENT_LEDGER.json` → FE-04 `commit: null`; the receipts package is commit `cf540f4`. | Not changed (JSON frozen). |
| 5 | In the WIP code, FE-01 full execution requires provider qualification evidence, while the ledger orders FE-01 before FE-05. | Reflected in step order (section 9). |
| 6 | README rollback anchors named six `archive/*` branches that exist neither locally nor on `origin`. Their key commits (`bdcaa41`, `cf5085e`, `dde6969`) are reachable from `main`. | README section 14 corrected to existing tags and commit SHAs. |
| 7 | `PHASENPLAN.json` → `operational_configuration.contract` points to `docs/OPERATIONAL_CONFIG.md`. | Resolved by the documented redirect (structure iteration 4). |
| 8 | README section 4 (Rust map) lists only part of the modules (e.g. no `econ*`, `market_*`, `measurement.rs`, `one_shot*`, `fe_qualification.rs`). | Open; updated with the Rust restructuring (phase E). |
| 9 | `tests/README.md` cites the non-existent `docs/project_0/PHASENPLAN.md`. | Open; fixed with the tests restructuring (phase C). |
| 10 | `docs/evidence/FE01_SCOPE_20261014/` exists only locally, untracked and empty. | No FE-01 evidence exists. |

## 11. Maintaining this file

Update this file together with every new dated ledger gate entry: snapshot line, sections 1, 2, 4–7 and 10.
Copy verdicts only from the ledger or evidence. Never mark a gate PASS here first, and never infer
economic or LIVE readiness from offline tests.

## 12. Gap and acceptance matrix — every binding ticket of PHASENPLAN §8 (as of 2026-10-17, project_0 code after `12523e5`)

Status vocabulary: **PASS** = every gate criterion evidenced · **LOCAL_PASS** = targeted local tests/offline evidence · **PARTIAL** = some criteria evidenced · **CODE** = implementation present, acceptance not evidenced · **OPEN** = not implemented · **BLOCKED** / **NOT_RUN** = external prerequisite or permission missing · **DEFERRED_PRIORITY** = binding, ordered after the FE path per §8 (never cancelled). Ledger references are dated entries in `MEASUREMENT_LEDGER.md`. Update this table with every ticket.

| Ticket | Code | Evidence | Status | Missing / next step |
|---|---|---|---|---|
| P0-01…07 | — | `evidence/P0_20261016/` | **PASS** | — |
| P1A-01 current ProgramData | `protocol.rs` decoder, Shadow startup pin | ledger 2026-10-16 P1A | CODE · **BLOCKED** | finalized read of program + ProgramData (separate read-only permission) |
| P1A-02 source/IDL manifest | `protocol.rs` parity tests | `manifests/CURRENT_MAINNET.md` | LOCAL_PASS | on-chain IDL hash NOT_RUN |
| P1A-03 allowlists | `protocol.rs`, `planner.rs` | same | LOCAL_PASS | — |
| P1A-04 upgrade watcher | `setup.rs`, `events.rs` | same | LOCAL_PASS | Mainnet behaviour NOT_RUN |
| P1B-01…03 | `decode_bank`, tripwire tests | `manifests/UPCOMING_0_1_12.md` (RC2 probe) | LOCAL_PASS | deployed release unknown (P1A); promotion: `premium_scratch`, repay/`repay_all`, new layouts/oracles |
| C0-01 account census | `c0_census.py` (+ import) | 2026-10-02 slot 452723812 | historical PASS · current NOT_RUN | current read (permission); census import still schema-1 (JFI) |
| C0-02 health classification | `census.rs`, Shadow census worker | Ticket 2, C0-FIX-01…05 | LOCAL_PASS · current **NOT_RUN** | permitted Shadow start with measured cost model |
| C0-03 race opportunity map | census facts (distance, oracles, integrations, TX load) | `evidence/C0_MARKET_CENSUS.md` | PARTIAL | exact v0/v1 fit per account needs built TX (pipeline); current data NOT_RUN |
| C0-04 long-tail / `t_profitable` | `economics.rs`, census `model_long_tail` | same | LOCAL_PASS (model) | current data NOT_RUN; tagging only from 0.1.12 |
| H0-01 historical extractor | `market/*`, `market_truth.rs`, gap/reconcile | C0-E1 runs (INCOMPLETE) | CODE | 30-day window over saved runs; Ticket 3 A1–A4 (typed capture + native verifier) first |
| H0-02 winner matcher | `market/winner.rs`, `winner_tracker.rs` | 6/6 audit fixtures; absent-`meta.err` fix 2026-10-17 | LOCAL_PASS | historical window evaluation |
| H0-03 execution classifier | `market_truth.rs` (four types / six markers) | C0E1-P3 evidence | PARTIAL | apply to H0 window |
| H0-04 tag timeline after 0.1.12 | — | — | OPEN · BLOCKED | needs 0.1.12 deployed |
| H0-05 competitor min-profit inference | — | — | OPEN | after H0-01…03 |
| H0-06 report | — | — | OPEN | `evidence/H0_LIQUIDATIONS.md` |
| S0-01 workspace | 4 crates | builds/tests per ledger | LOCAL_PASS | — |
| S0-02 timing / S0-03 telemetry | `common/timing.rs`, `mt0.rs` | MT0 evidence | LOCAL_PASS | Mainnet tails NOT_MEASURED |
| S0-04 bounded channels / lifecycle | `common/channel.rs`, `lifecycle.rs` | unit tests | LOCAL_PASS | — |
| S0-05 journal / replay | `analytics/journal.rs`, `payload.rs` | PERSIST + JFI entries | PARTIAL | JFI migration of remaining kinds (Ticket 3 A2, FIX-04…06); segments SK-07 |
| S1-01 IngressSource / native events | `solana/src/ingress` | bootstrap PASS 2026-10-02; typed `ProgramFilter` `68fb932`; raw-free events ledger 2026-10-17 (A1) | PARTIAL | native events LOCAL_PASS; Mainnet long-run qualification open (S1-03/S1-07) |
| S1-02 BinaryGeyserSource | — (enum mention only) | — | OPEN · DEFERRED_PRIORITY | S1-RACE |
| S1-03 WsRpcControlSource | `ws_rpc.rs` | Mainnet bootstrap/recovery PASS | PARTIAL | long-run continuity FAILED (4 h / 5 h gaps, 429) |
| S1-04 snapshot + reconcile | `state/snapshot.rs`, `recovery.rs` | PASS in bootstrap/C0-E1 runs | PARTIAL | Long-Tail/Premium snapshot semantics for 0.1.12 |
| S1-05 binary fast lane / queues | bounded channels | — | OPEN | qualified binary source (S1-RACE) |
| S1-06 v1 fidelity | `txv1` feature, `p0-probe-v1` | `V1_wire_probe_20261004.json` | PARTIAL | v1 submission capability gate |
| S1-07 reconnect / gap / reconcile | `ws_rpc.rs`, `market_gap.rs`, `market_reconcile.rs` | 60 s disconnect recovered | PARTIAL | 4 h unrecovered gap; 429 handling |
| S1-08 Mainnet binary source + cost | — | — | NOT_RUN · DEFERRED_PRIORITY | — |
| S2-01…04 canonical state, fork/root, bootstrap, recovery | `solana/src/state/*` | Mainnet PASS (bootstrap/recovery) | LOCAL_PASS · partial Mainnet | long-run qualification |
| S2-05 blockhash / LUT / token metadata | `blockhash.rs`, `execution/lookup_tables.rs` | R120 protocol LUTs | PARTIAL | token metadata cache not separate |
| S3-01 v0 builder | `solana/src/tx`, `execution/builder.rs` | simulation path proven (R97–R101) | PARTIAL | current candidate simulation (FE-02) |
| S3-02 v1 builder | `txv1` | wire probe | PARTIAL | submission adapter capability |
| S3-03 signer / serialization | `solana/src/tx` | tests | LOCAL_PASS | — |
| S3-04 RpcSubmitter | `submit/rpc.rs` | mock/loopback | CODE | never submitted; LIVE disabled |
| S3-05 JitoSubmitter core | `submit/jito.rs` | mock | CODE | Jito costs/lifecycle UNKNOWN (FIX-129) |
| S3-06 landing reconciliation | `execution/pending.rs`, FE-04 receipts | FE-04 local | PARTIAL | finalized reconciliation in pilot |
| S3-R00…R03 Jito access, gRPC, prewarm, race benchmark | ATA prewarm only | — | OPEN · DEFERRED_PRIORITY | after S1-RACE |
| S4-01…04 parity corpus / decode / tx / performance | — | — | OPEN · DEFERRED_PRIORITY | — |
| C0E1-P3-M06 … C0E1-P6-GATE | `market_*`, `analytics/*`, `p0-strict-verify` | `C0E1_P3…P6` evidence | LOCAL_PASS (offline) | 4 h master NOT_QUALIFIED; native report B7 LOCAL_PASS (index provenance stays pinned Python); dead in-capture recovery removed 2026-10-17; `scripts/smoke.sh` stale since `2c62d13` (NOT_RUN) |
| P2-01 RAM model / P2-02 reverse indices | `state`, `liquidation/index.rs` | R127d Shadow | LOCAL_PASS | requalification after FIX-134/135 not recorded |
| P2-03 threshold index | `index.rs` | — | CODE | not the default since R127 |
| P2-04 premium scheduler / P2-05 tag scheduler | `schedule.rs` | unit tests | CODE | 0.1.12 semantics |
| P2-06 prepared candidate / TX template cache | `prepared.rs` | — | PARTIAL | template cache not built |
| P2-07 invalidation + prebuild benchmark | — | — | OPEN | — |
| P3-01 crate integration / exact fallback | `health.rs` | R127d 12.4 M checks | LOCAL_PASS | — |
| P3-02 premium health | — | RC2 API identified | OPEN | 0.1.12 promotion (`premium_scratch`) |
| P3-03 receivership rule | `sizing.rs` | unit tests | PARTIAL | 0.1.12 health-drop / bad-debt rules |
| P3-04 integration parity | `integration.rs` | R110–R122 | PARTIAL | Drift qualification OPEN since 2026-10-07 |
| P3-05 parity corpus | — | — | OPEN | — |
| P4A-01 realizable value | `economics.rs`, route floor | R124 historical | PARTIAL | current executable floor (FE-02) |
| P4A-02 fee model | FIX-129, `MessageFeeQuote` | FIX129 doc | PARTIAL | Jito tip cost UNKNOWN |
| P4A-03 failed-attempt matrix / P4A-04 `t_profitable` / P4A-06 ranking | `economics.rs` | unit tests | LOCAL_PASS | Mainnet calibration |
| P4A-05 capital model | `funding.rs` | P6-MT T1 | LOCAL_PASS | — |
| P4A-JIT-01/02 | `route.rs` (ExactIn build), `funding.rs` | P1-C PASS | PARTIAL | bounded shortfall solver vs. classes, current quotes |
| P5-01 allowlist / P5-03 NO_SWAP | `planner.rs` | unit tests | LOCAL_PASS | — |
| P5-02 refresh planner | `integration.rs` | R116–R122 | PARTIAL | Drift |
| P5-04 local/precomputed route | — | — | OPEN | — |
| P5-05 free route | Jupiter `/swap/v2/build` adapter | R118/R119 | PARTIAL | adapter typing per JFI-02 exception (FIX-07) |
| P5-06 receivership builder / P5-07 prepared plan | `receivership.rs`, `builder.rs` | simulation path; P5 fee-wallet fix ledger 2026-10-17 | PARTIAL | fee-wallet fallback **fixed fail-closed** (`FEE_STATE_UNAVAILABLE` blocks every plan → Unavailable/Unknown, LOCAL_PASS); full P5 acceptance OPEN |
| P5-08 staging fixtures | — | `STAGING_MODEL.md` | OPEN | staging build + allowlist (permission for staging reads) |
| P5-JIT-01 | `route.rs`, `builder.rs` | — | PARTIAL | — |
| T-01…03 tag TX / state / economics | economics curve only | — | OPEN · BLOCKED | 0.1.12 deployment |
| P6-01 E2E telemetry | MT0, owner journal | MT0 evidence | LOCAL_PASS | Mainnet tails NOT_MEASURED |
| P6-02 race shadow | — | — | OPEN · DEFERRED_PRIORITY | S1-RACE |
| P6-03 long-tail shadow | Shadow runtime | R127d before FIX-134/135 | PARTIAL | 72 h P6-LT NOT_RUN |
| P6-04 winner comparison / P6-05 loss classifier | `winner.rs`, `LossReason` | local | LOCAL_PASS | Mainnet sample |
| P7-01…06 | — | — | OPEN · DEFERRED_PRIORITY | after P6 evidence |
| P4B-01…05 | — | — | OPEN · DEFERRED_PRIORITY | after P7 |
| P8-01 safety gates / P8-02 limits | `safety.rs`, pipeline | unit tests | PARTIAL | full P8 before multi-LIVE |
| P8-03 staging live / P8-04 controlled mainnet | LIVE refused in code | — | NOT_AUTHORIZED | separate approvals |
| P8-05 reconciliation / P8-RES-01/02 | FE-04 `one_shot.rs` journal, holds, UNKNOWN | FE-04 local evidence; P8 clock fix ledger 2026-10-17 | PARTIAL_LOCAL | reconcile wall-clock `unwrap_or(0)` **fixed fail-closed** (owner HALT, submission stays pending, LOCAL_PASS); global multi-mint reservation; FE-04 typed journal (FIX-06) before any financial handoff |
| P8-INV-01 inventory / rebalance | — | — | OPEN | — |
| P9-01…03 | — | — | OPEN | after pilot |
| P9-04 protocol-upgrade watch | = P1A-04 | — | LOCAL_PASS | — |
| K-01…06 | — | — | OPEN · DEFERRED_PRIORITY | — |
| FE-00…07 | see §5 | — | FE-00 LOCAL_PASS · NO_ECONOMIC_GO; FE-01…07 OPEN/NOT_RUN/NOT_AUTHORIZED | FE path has priority when a qualified candidate exists (§0.6) |
| PERSIST-01…05 | journal, renders, FE-04 | PERSIST entries | PARTIAL | Mainnet tails, physical fault test |
| JFI-01…05 overall | typed payloads, write admission, `darkbot_solana::chain` | JFI entries | **OPEN** | rows below; JFI-FIX-01/02 + write admission LOCAL_PASS |
| JFI Ticket 3 A0: chain model | `solana/src/chain.rs`: `ChainTx`/`ChainBlock`, `TxStatus` (`err:null`/error/absent = SUCCESS/FAILED/UNKNOWN), legacy/v0/v1, LUT, CPI + stack height, fees, CU, v1 `transactionConfig`, exact token units; winner `meta.err` fix; typed `getBlock` in gap/reconcile; `ProgramFilter`; binary-storable `ChainTx` | ledger 2026-10-17 (steps 1–2, step 3, filters/storable) | LOCAL_PASS | done — no rebuild without a concrete regression |
| JFI Ticket 3 A1: raw-free ingress | `ingress/mod.rs`, `ws_rpc.rs`, `chain.rs` | ledger 2026-10-17 (A1/A2) | LOCAL_PASS | — (absent status now UNKNOWN, not success) |
| JFI Ticket 3 A2: native `MARKET_*` frames | `market/rows.rs`, `sink.rs`, producers | ledger 2026-10-17 (A1/A2) | LOCAL_PASS | capture writes typed frames only; legacy views by explicit offline export |
| JFI Ticket 3 A3: native strict verifier / reports | `market_verification.rs` engine; `market/strict_driver.rs` + `p0-strict-verify --run` (typed spec/evidence/attempt/result) | ledger 2026-10-17 (A3, A3 rest) | LOCAL_PASS (scope) | native report consumer done (B7, `p0ctl report`); `--compat` render stays for the Python driver of historical runs |
| JFI Ticket 3 A4: bundle parity | `p0-parity` + `parity_differential.py`; pinned discriminator test; `market/related.rs` (`MARKET_RELATED`) | ledger 2026-10-17 (A4 completed in scope; A4 rest RELATED); `evidence/A4_PARITY_20261017`, `evidence/A4_RELATED_20261017` | LOCAL_PASS (scope) · historical coverage BLOCKED | 0 unexplained over 2,917 sources incl. related field; CLASSIC/deleverage/bankruptcy/CPI/legacy and related positives need a permitted archive read; live `logs` related coverage UNKNOWN by construction |
| JFI-FIX-04 config / measurement plans | typed `MEASUREMENT_PHASE`, `MEASUREMENT_PLAN` handoff, `RUN_MANIFEST`, `OPERATIONAL_CONFIG`; native `key = value` profiles/config files; launch `test.sh` → `p0ctl run start` → `econ4h-launcher` without JSON handoff; run-store views only by explicit export | ledger 2026-10-17 (SK-06, JFI-FIX-04 parts 1–3, JFI-FIX-04/05 rest) | LOCAL_PASS (scope) | BLOCKED (pinned): index/crosscheck verifier (Python, JSON evidence) and the private JSON handoff of `operational_config.py`; re-pin decision needed |
| JFI-FIX-05 econ / reports | typed `ECON_*` frames, `MARKET_SUMMARY` journal; the productive run writes no `hour_0N.json` / `economics_final.json` / `summary.json` / JSONL (offline renders only, byte-identical) | ledger 2026-10-17 (B1 part 1, JFI-FIX-05 part 2, JFI-FIX-05 rest, JFI-FIX-04/05 rest) | LOCAL_PASS | pinned `market_report.py` / offline `p0-econ-replay` JSON stay legacy (JFI-03); native report chain done (B7) |
| JFI-FIX-06 FE-04 journal | `one_shot.rs` | FE-04 local | OPEN | typed FE-04 kinds before any financial handoff |
| JFI-FIX-07 Jupiter adapter | `/swap/v2/build` | wire exception in PHASENPLAN JFI-02 (`f0da568`) | OPEN | typed consumer behind the wire adapter |
| JFI remaining kinds | `MEASUREMENT_PHASE`, `MT0_SNAPSHOT`, `FE01_*`, `ECON_*`, `C0_CENSUS_ACCOUNT/GROUP` import | `LEGACY_WRITABLE` list | OPEN | migrate each kind, then remove from `LEGACY_WRITABLE` |
| FEUI-01 terminal control / runtime owner (frontend plan, required before 4h) | `bin/p0ctl.rs`, `orchestration/control.rs`, `P0_RUNTIME_LOCK` in `capture.sh` | ledger 2026-10-17 (p0ctl); `evidence/CONTROL_BOOT_20261017` | LOCAL_PASS (scope) | typed RunStatus/control DTO, browser API, `runs list`, `logs --follow` (frontend scope) |
| BOOT-01…12 integrity boot audit (frontend STARTUP spec) | `orchestration/boot_audit.rs`, `p0ctl doctor` | same | offline LOCAL_PASS · network NOT_RUN | BOOT-03…08 network checks need explicit Mainnet read permission |
| SK-01 three layers, one truth | `.p0j` journal; typed market capture (A2); native strict evidence (A3) | PERSIST entries; ledger 2026-10-17 | PARTIAL | native report reads only `.p0j` (B7); `ECON_SNAPSHOT/FINAL` and summary values still JSON |
| SK-02 module structure | `storage/{budget,writer,preflight,segments}.rs` | ledger 2026-10-17 (SK-03/04, SK-06, SK-07) | PARTIAL | `offline_analysis/` with SK-08 only |
| SK-03 hard RAM byte budget | `storage/budget.rs`, `storage/writer.rs`; econ worker retention ledger (`analytics/econ.rs`); operational config `market_capture_*`, `system_reserve_mib`, `econ_retention_mib` | ledger 2026-10-17 (SK-03/04, SK-03 completion, B1 part 1) | LOCAL_PASS (scope) | capture lane and every retained econ/winner/Shadow collection byte-bounded; real-load evidence NOT_MEASURED |
| SK-04 separate batch writer | `storage/writer.rs` `MarketJournalWorker`, `Writer::append_batch` | ledger 2026-10-17 (SK-03/04, SK-03 completion) | LOCAL_PASS | implemented and locally tested incl. sticky write failure; crash/power-loss tail and real-load throughput NOT_MEASURED |
| SK-05 backpressure / 48 h availability | `storage/writer.rs` status/adaptive control/disk reserve; `[MARKET/STORAGE]` line | ledger 2026-10-17 (SK-05) | PARTIAL · lane LOCAL_PASS | real-load forecast evidence NOT_MEASURED; offline-indexer pause with SK-08 |
| SK-06 CapacityPreflight | `storage/preflight.rs`, `bin/p0-capacity.rs`, `scripts/capture.sh`; render `renders::storage_preflight_view` | ledger 2026-10-17 (SK-06); `evidence/SK06_CAPACITY_20261017` | LOCAL_PASS (scope) · basis NOT_MEASURED | calibration capture ≤ 600 s for a real basis (Mainnet permission); provider check via BOOT-04; byte-bounded econ collections (SK-03) |
| SK-07 segmentation | `storage/segments.rs` (`SegmentedWriter`, `ChainReader`, `SEGMENT_LINK`/`SEGMENT_NEXT`); market and Shadow-evaluation lanes | ledger 2026-10-17 (SK-07, B1 part 1); `evidence/SK07_SEGMENTS_20261017` | LOCAL_PASS (scope) | real-load segment counts NOT_MEASURED |
| SK-08 optional SQLite offline cache | — | — | OPEN · DEFERRED_PRIORITY | after Ticket 3, native C0-E1 and first profit; never on the capture/financial/strict path |
| SK-09 order / acceptance | — | — | OPEN | integration not accepted; order: Ticket 3 A3/A4 (minimal native reader) → SK-05/06/07 → B1/B7 (B7 LOCAL_PASS 2026-10-17); storage acceptance does not block the FE critical path |
