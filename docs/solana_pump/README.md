# Solana / Pump.fun

Documentation root for the existing Solana/Pump.fun system.

## Current navigation

- [RESEARCH_STATUS_2026-09-23.md](RESEARCH_STATUS_2026-09-23.md)
- [MASTER_TODO_2026-09-20.md](MASTER_TODO_2026-09-20.md)
- [R9_IMPLEMENTATION_STATUS.md](R9_IMPLEMENTATION_STATUS.md)
- [STRATEGY_C_SOURCE_OF_TRUTH.md](STRATEGY_C_SOURCE_OF_TRUTH.md)
- [SPEC.json](SPEC.json)
- [TEST_HANDBUCH.md](TEST_HANDBUCH.md)

Historical files named `P0_*` in this folder are **Pump.fun Phase-0** material. They are not Project 0 liquidation-protocol documentation.

Historical backend/reference material:
- [legacy_backend/](legacy_backend/)

Current implementation remains under:
- `../../darkbot/solana_pump/` — Pump.fun application, strategy, replay/PAPER, economics, venue and project-specific Solana wiring
- `../../darkbot/runtime/` — shared runtime/profile assembly
- `../../darkbot/chains/` and `../../darkbot/venues/` — shared contracts/registries
- `../../darkbot/solana/` — shared native Solana primitives as they are extracted for reuse by Project 0

Historical backend/reference material is retained once under `legacy_backend/`; duplicate copies previously stored under `old_system/` were removed.
