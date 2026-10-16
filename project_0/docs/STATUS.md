# Project 0 — Current status

**Snapshot:** 2026-10-16 · production implementation `4a27a1c6ab4032b322d5d6be3eb0b10c0d9d0d91` · document-only synchronization after implementation.

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
