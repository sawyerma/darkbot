# Solana / Pump.fun

Documentation root for the existing Solana/Pump.fun reference system.

## Purpose

The Pump.fun project owns the current capture/replay/PAPER/strategy implementation and remains the semantic reference while shared native Solana infrastructure is introduced.

## Current authoritative navigation

- [RESEARCH_STATUS_2026-09-23.md](RESEARCH_STATUS_2026-09-23.md) — current research and latency evidence.
- [MASTER_TODO_2026-09-20.md](MASTER_TODO_2026-09-20.md) — active work/TODO.
- [R9_IMPLEMENTATION_STATUS.md](R9_IMPLEMENTATION_STATUS.md) — R9 / Token-2022 status.
- [STRATEGY_C_SOURCE_OF_TRUTH.md](STRATEGY_C_SOURCE_OF_TRUTH.md) — Strategy C semantics.
- [SPEC.json](SPEC.json) — authoritative machine-readable specification.
- [TEST_HANDBUCH.md](TEST_HANDBUCH.md) — test handbook.

## Code and project assets

- [Pump.fun implementation](../../darkbot/solana_pump/)
- [Strategies](../../darkbot/solana_pump/strategies/)
- [Venue adapter](../../darkbot/solana_pump/venue/)
- [Shared Solana primitives](../../darkbot/solana/)
- [Config](../../config/solana_pump/)
- [Data](../../data/solana_pump/)
- [Exports](../../exports/solana_pump/)
- [Migrations](../../migrations/solana_pump/)
- [Tests + fixtures](../../tests/solana_pump/)
- [Tools](../../tools/solana_pump/)

## Historical material

- [legacy_backend/](legacy_backend/) — retained historical backend/reference source.
- [../../exports/solana_pump/legacy/](../../exports/solana_pump/legacy/) — historical output archives.
- [../../tests/solana_pump/legacy_audits/](../../tests/solana_pump/legacy_audits/) — historical audit snapshots.

Historical files named `P0_*` mean **Pump.fun Phase-0**, not the Project 0 liquidation protocol.
