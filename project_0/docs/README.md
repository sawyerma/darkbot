# Project 0 documentation

- `AUDIT_HANDOFF_2026-10-05.md` — **current authoritative post-audit status and exact next work order**.
- `PHASENPLAN.md` — authoritative production plan.
- `FABLE_WORK_ORDER.md` — implementation work order.
- `MEASUREMENT_LEDGER.md` — measured evidence and gate decisions.
- `architecture/` — architecture decisions: P0 upstream reuse matrix, parallel execution plan.
- `operations/` — operational configuration: profiles, precedence, budgets, authorization limits.
- `qualification/` — qualification/gate documentation index.
- `R127D_CHANGELOG_ROLLBACK.md` — R127d implementation, qualification, rollback and post-audit correctness correction.
- `measurements/protocols/` — measurement protocols: P6-LT Shadow, C0-MO market observer, C0-E1 econ4h.
- `measurements/historical_reports/` — historical measurement/qualification reports: FIX-129 cost qualification, C0-E1 implementation report.
- `evidence/AUDIT_20261005/` — immutable Astra audit evidence, commands/results and complete forensic ZIP bundle.

Repository paths are rooted at `project_0/`; the Rust workspace is `project_0/native/`.

After 2026-10-05, resume Project 0 work from `AUDIT_HANDOFF_2026-10-05.md` before making further architecture changes.

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

Inside the moved historical reports only location-relative paths to `docs/evidence/` were re-rooted; measurement results and wording are unchanged.

`OPERATIONAL_CONFIG.md` remains as a redirect because `PHASENPLAN.json` and hash-protected evidence
reference that path and are not edited during the structure reform.

In `qualification/README.md` the two dead links of the former `gates/README.md` were corrected and its
"reserved for gate code" sentence replaced by the current role; the top-level `gates/` directory no longer exists.
