# Project 0 documentation

- `AUDIT_HANDOFF_2026-10-05.md` — **current authoritative post-audit status and exact next work order**.
- `PHASENPLAN.md` — authoritative production plan.
- `FABLE_WORK_ORDER.md` — implementation work order.
- `MEASUREMENT_LEDGER.md` — measured evidence and gate decisions.
- `P0_REUSE_MATRIX.md` — upstream reuse decisions.
- `R127D_CHANGELOG_ROLLBACK.md` — R127d implementation, qualification, rollback and post-audit correctness correction.
- `measurements/protocols/` — measurement protocols: P6-LT Shadow, C0-MO market observer, C0-E1 econ4h.
- `evidence/AUDIT_20261005/` — immutable Astra audit evidence, commands/results and complete forensic ZIP bundle.

Repository paths are rooted at `project_0/`; the Rust workspace is `project_0/native/`.

After 2026-10-05, resume Project 0 work from `AUDIT_HANDOFF_2026-10-05.md` before making further architecture changes.

## Relocated documents

Ledger entries, evidence inventories and checksums keep the path that was valid when they were written. Resolve old paths here:

| Old path | New path |
|---|---|
| `docs/PHASE_P6LT_SHADOW.md` | [measurements/protocols/p6lt_shadow.md](measurements/protocols/p6lt_shadow.md) |
| `README.md` §17 (C0-MO) | [measurements/protocols/c0mo_market_observer.md](measurements/protocols/c0mo_market_observer.md) |
| `README.md` §18 (C0-E1 econ4h) | [measurements/protocols/c0e1_econ4h.md](measurements/protocols/c0e1_econ4h.md) |
