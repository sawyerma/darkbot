# bugs_v4 closure, 07.11.2026 (order 06.11.: every finding, not only C1/C4/G5/D5/AH1)

Code checked = R14 `b2e8a5ad` (fixes in the commit with this file). The final status per ID is in `STATUS.md` (section bugs_v4-Abschluss); the per-ID code proof of the verification is in `verify_part1.md` (lines 1–747 of bugs_v4: C*, M1, D*–J*, K*/L* first part, P24-M*) and `verify_part2.md` (Revisions 10–20 and AH1), both read against the code by two read-only agents and re-checked where they disagreed (F3/AG1, AR3/P24-M1, O2).

## Fixes (each with a test that fails without it: `mutants.out`, `tools/mut_bv4.py`)
- C1 PAPER arm only on a probe younger than the re-probe interval (a nonce wait outlasting it probes again; a result just observed counts as fresh however long its probe took).
- C2 C2 gone under an armed PAPER path: disarm (fence goes), arm again with C2.
- C4 a strategy exit's lane pre-check and the `strategy_lanes` status count C3's nonce skip (no lane taken and lost).
- D5 a strategy replay of a journal that lost records (run manifest `LOST_RECORDS`, or its gap spans) is INCOMPLETE.
- G5/G6 (=AR1) the Four drift guard keeps its validation through `BSC_CANONICAL_FOUR_PROBE_FAILURES` (2) failed probes in a row only, then every Four curve is unclassified until a MATCH (token derivation stays tied to DRIFT); a spawn failure is journaled like any failure; one validation per MATCH; the browser E2E regex follows the new verdict text.
- AH1 a gas class a configured path needs and the frozen evidence lacks refuses the start; the one unwitnessed class (a Four launch-block BUY) is bound to its family's BUY class.
- D2 the status file is written by its own writer thread (newest wins), the stop's synchronously; a write error (also one finishing beside the stop's) still ends the run.
- AR2 a source switched off parks its adapter (joined once it ended, no second adapter beside it); at the stop as before; the per-source step is one function (`adapter_step`) tested as a table.
- L2 reproduced: a grant revoked by another process while an admission held its read was admitted (`Ok(6000)`); now re-checked after the entry (BSC3-004's rule) → `LIVE_GRANT_CHANGED`.
- AS2 the three release-only timing tests run in the local gate (step RELTEST).

## Reproductions and measurements
- L3 reproduced: a lease returned during an admission only over-counts (refused, never above the grant) → KEEP.
- D4 unreachable: `run_source`'s refusals (URL, limits) are refused at config load, TLS roots are static.
- `d2_f3_ab.txt`: D2 after (status pass p99 0.57–0.60 ms instead of 4.0–5.3 ms) and F3/AG1 QoS A/B (two pairs; off-CPU events 2–5 vs 6–18 per 5 min, waits ≥ 1 ms and maxima not clearly changed) → no code change, measured under launchd in the joint acceptance (the runtime's plist sets no `ProcessType`).
- Measurement candidates otherwise judged with R14's numbers (`r14/r14_20261106`); the LIVE-only ones (no LiveRuntime ran in R14) are a measurement point before the first BUY.
