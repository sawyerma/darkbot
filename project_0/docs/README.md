# Project 0 documentation

**Start here: [STATUS.md](STATUS.md)** — current code and qualification state, FE-00 … FE-07, open gates,
next steps and known contradictions.

Repository paths are rooted at `project_0/`; the Rust workspace is `project_0/native/`.

## Canonical documents

| File | Role |
|---|---|
| [STATUS.md](STATUS.md) | Current status summary derived from ledger, plan, evidence and code. Never upgrades a gate. |
| [operations/restructure_handoff.md](operations/restructure_handoff.md) | **Active repository-reform handoff**: 7/32 complete; iteration 8a Gitignore done; 8b next; tests and Git rules. |
| [architecture/restructure_32_step_plan.md](architecture/restructure_32_step_plan.md) | **32-iteration structure roadmap**: A–F phases, milestones, all remaining tasks and target tree. |
| [PHASENPLAN.md](PHASENPLAN.md) · [PHASENPLAN.json](PHASENPLAN.json) | Authoritative production plan; the JSON is a compact index, the Markdown is canonical. |
| [MEASUREMENT_LEDGER.md](MEASUREMENT_LEDGER.md) · [MEASUREMENT_LEDGER.json](MEASUREMENT_LEDGER.json) | Measured evidence and gate decisions, newest entries first; the JSON is a compact index. |

## Documentation directories

### [architecture/](architecture/)

| File | Purpose |
|---|---|
| [p0_reuse_matrix.md](architecture/p0_reuse_matrix.md) | P0 reference study and upstream reuse/merge decisions (eva01, marginfi-v2). |
| [parallel_execution_plan.md](architecture/parallel_execution_plan.md) | Parallel execution design and MT0–MT10 sequence (baseline 2026-10-06). |

### [operations/](operations/)

| File | Purpose |
|---|---|
| [operational_config.md](operations/operational_config.md) | Operational configuration: inspection commands, precedence, profiles, budgets, authorization limits. |
| [run_storage.md](operations/run_storage.md) | Run storage contract: run roots, opt-in `data/runs/`, run id and collision rules, run path inventory. |

### [qualification/](qualification/)

| File | Purpose |
|---|---|
| [README.md](qualification/README.md) | Where gate definitions, gate results and raw evidence live. |

### [measurements/protocols/](measurements/protocols/)

| File | Purpose |
|---|---|
| [p6lt_shadow.md](measurements/protocols/p6lt_shadow.md) | P6-LT Shadow phase definition: gates, measurement structures, required ledger fields. |
| [c0mo_market_observer.md](measurements/protocols/c0mo_market_observer.md) | C0-MO unfiltered read-only market observer. |
| [c0e1_econ4h.md](measurements/protocols/c0e1_econ4h.md) | C0-E1 combined economic observation (`P0_MODE=econ4h`). |

### [measurements/historical_reports/](measurements/historical_reports/)

| File | Purpose |
|---|---|
| [fix129_cost_qualification.md](measurements/historical_reports/fix129_cost_qualification.md) | FIX-129 cost-model qualification (2026-10-05). |
| [c0e1_implementation_report.md](measurements/historical_reports/c0e1_implementation_report.md) | C0-E1 implementation, validation and qualification history (2026-10-08 to 2026-10-12). |

### [development_history/](development_history/)

| File | Purpose |
|---|---|
| [audit_handoff_20261005.md](development_history/audit_handoff_20261005.md) | **Historical** post-audit handoff (2026-10-05, P6-MT override 2026-10-07); superseded by [STATUS.md](STATUS.md). |
| [r127d_changelog_rollback.md](development_history/r127d_changelog_rollback.md) | R127d candidate cache / re-arm change, rollback and post-audit correctness correction. |
| [r127c_changelog_rollback.md](development_history/r127c_changelog_rollback.md) | R127c batching implementation and rollback. |
| [fable_audit_20261003.md](development_history/fable_audit_20261003.md) | Earlier audit snapshot. |
| [fable_work_order.md](development_history/fable_work_order.md) | Earlier implementation work order (2026-09-30). |

### [evidence/](evidence/) — immutable

Evidence is append-only after qualification: never edit it; add new evidence and reference it from the ledger.

| Evidence | Entry document |
|---|---|
| [AUDIT_20261005/](evidence/AUDIT_20261005/) | [AUDIT_FINDINGS.md](evidence/AUDIT_20261005/AUDIT_FINDINGS.md), [COMMANDS_AND_RESULTS.md](evidence/AUDIT_20261005/COMMANDS_AND_RESULTS.md) |
| Single-file runtime evidence 2026-10-04/05: R114 … R127d, FIX129, USDT_ATA, V1_wire_probe | [evidence/](evidence/) (per-file purpose: project README §6) |
| [P6MT_T1_R3_20261007/](evidence/P6MT_T1_R3_20261007/) | [QUALIFICATION.md](evidence/P6MT_T1_R3_20261007/QUALIFICATION.md) |
| [C0MO_20261008/](evidence/C0MO_20261008/) | [SMOKE_600S.md](evidence/C0MO_20261008/SMOKE_600S.md) |
| [C0E1_CANONICAL_CHECKPOINT_20261009/](evidence/C0E1_CANONICAL_CHECKPOINT_20261009/), [C0E1_GAP_MASTER_CHECKPOINT_20261009/](evidence/C0E1_GAP_MASTER_CHECKPOINT_20261009/), [C0E1_CANONICAL_FINAL_20261010/](evidence/C0E1_CANONICAL_FINAL_20261010/) | — |
| [C0E1_P1_20261011/](evidence/C0E1_P1_20261011/), [C0E1_P3_20261011/](evidence/C0E1_P3_20261011/) | — |
| [C0E1_P2_20261012/](evidence/C0E1_P2_20261012/) | [README.md](evidence/C0E1_P2_20261012/README.md) |
| [C0E1_P3_MARKERS_20261013/](evidence/C0E1_P3_MARKERS_20261013/) | [README.md](evidence/C0E1_P3_MARKERS_20261013/README.md) |
| [C0E1_P4_20261013/](evidence/C0E1_P4_20261013/) | [README.md](evidence/C0E1_P4_20261013/README.md) |
| [C0E1_P5_20261013/](evidence/C0E1_P5_20261013/) | [README.md](evidence/C0E1_P5_20261013/README.md) |
| [C0E1_P6_20261013/](evidence/C0E1_P6_20261013/) | [README.md](evidence/C0E1_P6_20261013/README.md) |
| [FE00_20261013/](evidence/FE00_20261013/) | [README.md](evidence/FE00_20261013/README.md) |
| [FE04_LOCAL_20261013/](evidence/FE04_LOCAL_20261013/) | [README.md](evidence/FE04_LOCAL_20261013/README.md) |
| [FE04_BINDING_20261013/](evidence/FE04_BINDING_20261013/) | [README.md](evidence/FE04_BINDING_20261013/README.md) |
| [FE04_RECEIPTS_20261013/](evidence/FE04_RECEIPTS_20261013/) | [README.md](evidence/FE04_RECEIPTS_20261013/README.md) |
| [OPERATIONS_CONFIG_20261014/](evidence/OPERATIONS_CONFIG_20261014/) | [README.md](evidence/OPERATIONS_CONFIG_20261014/README.md) |

Not versioned (local only): `evidence/C0E1_TRANSPORT_FINISH_20261010/` (referenced by the C0-E1 report) and
`evidence/FE01_SCOPE_20261014/` (empty).

## Relocated documents

Ledger and PHASENPLAN entries, evidence inventories and checksums keep the path that was valid when they were written. Resolve old paths here:

| Old path | New path |
|---|---|
| `docs/PHASE_P6LT_SHADOW.md` | [measurements/protocols/p6lt_shadow.md](measurements/protocols/p6lt_shadow.md) |
| `README.md` §17 (C0-MO) | [measurements/protocols/c0mo_market_observer.md](measurements/protocols/c0mo_market_observer.md) |
| `README.md` §18 (C0-E1 econ4h) | [measurements/protocols/c0e1_econ4h.md](measurements/protocols/c0e1_econ4h.md) |
| `docs/FIX_129_COST_QUALIFICATION.md` | [measurements/historical_reports/fix129_cost_qualification.md](measurements/historical_reports/fix129_cost_qualification.md) |
| `docs/C0_E1_IMPLEMENTATION_REPORT.md` | [measurements/historical_reports/c0e1_implementation_report.md](measurements/historical_reports/c0e1_implementation_report.md) |
| `docs/P0_REUSE_MATRIX.md` | [architecture/p0_reuse_matrix.md](architecture/p0_reuse_matrix.md) |
| `docs/PARALLEL_EXECUTION_MASTER_PLAN.md` | [architecture/parallel_execution_plan.md](architecture/parallel_execution_plan.md) |
| `docs/OPERATIONAL_CONFIG.md` | [operations/operational_config.md](operations/operational_config.md) |
| `gates/README.md` | [qualification/README.md](qualification/README.md) |
| `docs/AUDIT_HANDOFF_2026-10-05.md` | [development_history/audit_handoff_20261005.md](development_history/audit_handoff_20261005.md) |
| `docs/FABLE_AUDIT_2026-10-03.md` | [development_history/fable_audit_20261003.md](development_history/fable_audit_20261003.md) |
| `docs/FABLE_WORK_ORDER.md` | [development_history/fable_work_order.md](development_history/fable_work_order.md) |
| `docs/R127C_CHANGELOG_ROLLBACK.md` | [development_history/r127c_changelog_rollback.md](development_history/r127c_changelog_rollback.md) |
| `docs/R127D_CHANGELOG_ROLLBACK.md` | [development_history/r127d_changelog_rollback.md](development_history/r127d_changelog_rollback.md) |

Inside the moved historical reports only location-relative paths to `docs/evidence/` were re-rooted; measurement results and wording are unchanged.

`OPERATIONAL_CONFIG.md` remains as a redirect because `PHASENPLAN.json` and hash-protected evidence
reference that path and are not edited during the structure reform.

In `qualification/README.md` the two dead links of the former `gates/README.md` were corrected and its
"reserved for gate code" sentence replaced by the current role; the top-level `gates/` directory no longer exists.

`development_history/audit_handoff_20261005.md` carries a two-line "historical" banner above its unchanged original text.
