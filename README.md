# DarkBot

DarkBot is organized by product/domain at repository root.

```text
darkbot/
├── bsc/         BSC Birth Entry / Gate0
├── project_0/   Solana Project 0 liquidator
├── solana/      Solana / Pump.fun and shared Solana Python infrastructure
├── frontend/    one shared React control-plane/dashboard
└── README.md
```

## Ownership

**BSC:** everything specific to BSC lives below `bsc/`: source boundary, Gate0, tests, docs, config, tools and evidence.

**Project 0:** everything specific to the Project 0 liquidator lives below `project_0/`. The Rust production workspace is `project_0/native/`; the authoritative phase plan is `project_0/docs/PHASENPLAN.md`.

**Solana:** the existing Pump.fun/reference system and reusable Python Solana infrastructure live below `solana/`.

**Frontend:** there is one frontend for all three domains, not one frontend per bot. Domain separation is inside `frontend/src/features/{bsc,project0,solana}`. Shared API, realtime, layout and UI code exist exactly once.

## Frontend rule

The frontend is a control plane, not another trading engine.

```text
backend/core
  -> REST snapshot
  -> sequenced WebSocket events
  -> React frontend
```

Protocol decoding, canonical state, health, economics, transaction construction, signing, submission and reconciliation remain in the backend/native code.

The frontend technical base comes from the existing `ws_ai_front` React/Vite/Tailwind setup, but its old application structure is not carried forward.

## Project entry points

- `bsc/README.md`
- `project_0/README.md`
- `project_0/docs/PHASENPLAN.md`
- `solana/README.md`
- `frontend/README.md`
