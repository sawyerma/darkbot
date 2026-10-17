# C0-E1 4h readiness — IST / SOLL / evidence (tracker)

**Started:** 2026-10-17 · base `origin/main` = `e000ecb` (project_0 code last changed in `0e0544d`, SK-06 WIP) · authority: [PHASENPLAN](../PHASENPLAN.md) §13 C0-E1 A–L, PERSIST, SK-01…09, JFI-01…05; frontend [CONTROL_PROTOCOL](../../../frontend/docs/CONTROL_PROTOCOL.md), [STARTUP_BACKTEST_OPTIMIZATION](../../../frontend/docs/STARTUP_BACKTEST_OPTIMIZATION.md) BOOT-01…12. Per-ticket verdicts stay in [STATUS §12](../STATUS.md) and the dated [ledger](../MEASUREMENT_LEDGER.md) entries; this page only orders the critical path to `READY_FOR_AUTHORIZED_4H` and is updated with every unit.

Target state: `READY_FOR_AUTHORIZED_4H` only after every mandatory local gate **and** a separately authorized short Mainnet smoke. READY never starts the 4h run.

## Contradictions found at start (resolved in this pass)

| # | Finding | Resolution |
|---|---|---|
| 1 | STATUS snapshot says project_0 code at `e7fe08d`; actual last code commit `0e0544d` | snapshot updated with the SK-06 unit |
| 2 | STATUS §12 SK-06 row: code "—", OPEN; `storage/preflight.rs` + `bin/p0-capacity.rs` exist since `0e0544d` | row corrected with the SK-06 unit |
| 3 | Rust capture and strict verifier do not record the four RELATED instructions; only the pinned Python index verifier decodes them | gap C2 below |
| 4 | `p0-capacity --compat-json` writes JSON with `serde_json::json!` (JFI-01) | replaced by the catalogued render `renders::storage_preflight_view` (unit B) |

## IST / SOLL / evidence

| Area | SOLL (source) | IST (code, evidence) | Gap → unit |
|---|---|---|---|
| Six markers / four classes | PHASENPLAN L.1, task §3 | `market_truth.rs` classifies CLASSIC/RECEIVERSHIP/FORCED_DELEVERAGE/BANKRUPTCY_SETTLEMENT; six discriminators bound to pin `33c67987`; A4 differential 2,917 sources, 0 unexplained ([A4_PARITY](../evidence/A4_PARITY_20261017/README.md)) | historical raw sample for CLASSIC, deleverage, bankruptcy, CPI positions, legacy: **BLOCKED** (archive read needs permission) — C1 keeps it documented |
| Four RELATED instructions | L.5, task §3 | pinned Python index verifier only | **C2**: native typed related rows in capture + strict verifier, never liquidation/profit |
| Report filters / ReportQuery | L.3/L.5, task §4 | `measurement_contract.py` (pinned) parses every flag; `market_report.py` (pinned, legacy consumer) | **D3 (B7)**: native ReportQuery over `.p0j`, filter invariance of raw hashes |
| Terminal control `p0ctl` | FEUI-01, CONTROL_PROTOCOL §2 | none; scripts `test.sh/capture.sh/verify.sh/report.sh` | **C3**: thin Rust `p0ctl` (doctor, preflight, run start/status/stop, verify, report) over the existing runtime and scripts; run lock, idempotent stop |
| SK-06 CapacityPreflight | SK-06, task §5 | WIP `preflight.rs`/`p0-capacity` (`0e0544d`), strict budget not mode-dependent, no provider/econ checks; `capture.sh` still Python 60 GiB heuristic | **B** |
| Boot audit | BOOT-01…12, task §7 | pieces only (startup protocol pin, FE-01 qualify tool, discriminator tests); no typed audit | **C4**: typed offline boot audit + bound token; network checks NOT_RUN without permission |
| A3 verification driver | L.3, A3 | strict engine native; spec/attempt/result identities and job input still in Python driver (`--compat`) | **D2** |
| SK-07 segments | SK-07, task §6 | one `market.p0j` per run | **D1** |
| SK-03 rest / B1 | SK-03, JFI-FIX-05 | `ECON_*` kinds schema 1; econ/winner/shadow collections count-bounded | **D3** |
| JFI-FIX-04 handoffs | JFI-04 | `measurement_plan.json`, `storage_preflight.json`, `candidate_manifest.json` JSON handoffs (pinned `run_store.py` requires them) | **D4**: native records primary, JSON only as catalogued renders until a re-pin decision |
| JFI-FIX-06/07 | JFI | FE-04 journal schema 1; Jupiter adapter wire exception | not on the observe-only 4h path (no FE-04 handoff, no submission); stays OPEN, recorded as deferred, not done |
| Duration / RunHorizon | P1, STARTUP §3, task §8 | `MeasurementPlan` fixed, `s/m/h`, two parsers | **C5**: `RunHorizon::Fixed` + `d` unit, one parser; `UntilStopped` stays follow-up scope |
| Verification levels | L.2, task §9 | off/index/strict/crosscheck separated (C0E1-P4/P5/P6 evidence) | keep; E2E in **E** |
| Mainnet smoke | task §11 | NOT_RUN | **F**: only with explicit user permission |

## Progress

| Unit | State | Commit | Evidence |
|---|---|---|---|
| A — baseline/contradictions | done | — | this page |
| Git isolation (after the 506c349 BSC incident) | Project_0 works in a separate worktree with its own index; guarded pathspec commits | — | memory rule; `dfc5e14` restore reported to the user |
| D2 — A3 native strict driver (typed identities) | LOCAL_PASS in scope | (this unit) | ledger 2026-10-17 (A3 rest) |
| D4c — typed run manifest + effective-config record | LOCAL_PASS | (this unit) | ledger 2026-10-17 (JFI-FIX-04 part 3) |
| D4b — typed measurement-plan handoff | LOCAL_PASS | (this unit) | ledger 2026-10-17 (JFI-FIX-04 part 2) |
| D4a — typed measurement phases; p6 fixture defect corrected | LOCAL_PASS | (this unit) | ledger 2026-10-17 (JFI-FIX-04 part 1) |
| D3b — typed ECON_CANONICAL / ECON_OPPORTUNITY | LOCAL_PASS; snapshot/final documents open | (this unit) | ledger 2026-10-17 (JFI-FIX-05 part 2) |
| C5 — one duration parser (s/m/h/d), `RunHorizon::Fixed` | LOCAL_PASS; `UntilStopped` FEUI follow-up | (this unit) | ledger 2026-10-17 (run horizon) |
| D3a — B1 part 1: typed+segmented Shadow lane, byte-bounded econ retention | LOCAL_PASS in scope | (this unit) | [B1_SHADOW_RETENTION_20261017](../evidence/B1_SHADOW_RETENTION_20261017/README.md) |
| D1 — SK-07 segments + chain reader (market lane) | LOCAL_PASS in scope; Shadow lane open (B1) | (this unit) | [SK07_SEGMENTS_20261017](../evidence/SK07_SEGMENTS_20261017/README.md) |
| C3/C4 — `p0ctl`, owner lock, operator stop, offline boot audit | LOCAL_PASS in scope; network boot NOT_RUN | (this unit) | [CONTROL_BOOT_20261017](../evidence/CONTROL_BOOT_20261017/README.md) |
| C2 — RELATED_NON_LIQUIDATION native rows | LOCAL_PASS in scope; historical positives BLOCKED | (this unit) | [A4_RELATED_20261017](../evidence/A4_RELATED_20261017/README.md) |
| B — SK-06 CapacityPreflight, launcher-integrated | LOCAL_PASS in scope; real basis NOT_MEASURED, provider NOT_RUN | (this unit) | [SK06_CAPACITY_20261017](../evidence/SK06_CAPACITY_20261017/README.md) |
