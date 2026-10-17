# DarkBot Frontend

Single React frontend for all DarkBot backend domains.

## Gateway-Protokoll und Push-Refresh

- **[Gateway Stream Protocol v1](docs/GATEWAY_STREAM_PROTOCOL_V1.md)** — verbindlicher SOLL-Vertrag für Topics, per-Topic Resume, durable Ereignisse, State-Coalescing, getrennte Commands, Protobuf-Codegen, Heartbeat, Stores und Replay. **Noch keine Backend-Implementierung.**

## Gesamtarchitektur

- **[DarkBot Global Frontend Architecture](docs/DARKBOT_GLOBAL_FRONTEND_ARCHITECTURE.md)** — verbindlicher gemeinsamer DarkBot-Host mit globaler Overview, eigenständigen Project_0-/BSC-Projektoberflächen, späteren Projektmodulen und isolierten Backend-/Wallet-/LIVE-Gates. Design: Graphite V0. **Dokumentiert, nicht bereits in alle Domains implementiert.**

## Ownership

- `features/bsc/` — BSC Birth Entry / Gate0 UI.
- `features/project0/` — Project 0 liquidator UI.
- `features/solana/` — Pump.fun and shared Solana UI.
- `core/api/` — typed HTTP control-plane client only.
- `core/realtime/` — one WebSocket event stream with reconnect and sequence-gap detection.
- `shared/` — presentation-only UI/layout primitives.

The browser is not a second trading engine. Protocol decoding, health, economics, signing and execution stay in the backend. REST supplies snapshots; WebSocket supplies deltas/events.

## Project_0 frontend implementation plan

- **[Frozen V0 Graphite Fleet layout and React tree](docs/V0_GRAPHITE_FLEET.md)** — accepted five-tab graphite design; stable Wallets, multiple Liquidator drafts.
- **[V0 Fleet implementation plan: code variables, persistence, refresh, React ownership](docs/V0_FLEET_IMPLEMENTATION_PLAN.md)** — exact current source mappings and missing backend contracts; no LIVE claims.


- [Accepted Graphite V5 UI & shared financial snapshot contract](docs/GRAPHITE_V5_UI_FINANCE_CONTRACT.md) — five-tab dark macOS-style layout, Overview/Wallets Total Capital and Realized Net PnL parity, latency/parallel execution owner map and gates.

- [Wallets & Treasury: wallet dashboard, manual withdrawals and realized-profit auto-sweeps](docs/WALLETS_TREASURY.md) — FEUI-10A/B/C and safety/approval gates.

- [Boot-Audit, free duration, run-until-stop, one-click backtest and optimization](docs/STARTUP_BACKTEST_OPTIMIZATION.md) – required extended launch/replay/optimizer design.
- [Frontend Phasenplan FEUI-00…09](docs/FRONTEND_PHASENPLAN.md) – phases for p0ctl, Control Center, six liquidation markers, raw market explorer, Profitability Lab and acceptance gates.
- [Project_0 code and variable map](docs/PROJECT_0_CODE_MAP.md) – verified Rust/Python/TS source symbols, CLI flags, environment variables, defaults, journal and economics fields.
- [Project 0 Control protocol](docs/CONTROL_PROTOCOL.md) – proposed lifecycle, typed DTOs, control API, security and CLI/GUI parity tests.

These are **SOLL** architecture and implementation contracts, not evidence that `p0ctl`, the control API or the optimizer is already implemented. Authoritative project gates remain [Project_0 PHASENPLAN](../project_0/docs/PHASENPLAN.md) and [STATUS](../project_0/docs/STATUS.md).

## Development

```bash
cp .env.example .env
npm ci
npm run dev
```

Default frontend: `http://localhost:8080`.
Default development backend proxy: `http://localhost:8100`.

## Realtime contract

Expected envelope:

```json
{"seq":1,"ts_ns":0,"type":"system.status","project":"project_0","payload":{}}
```

A sequence gap is treated as a resync signal; the frontend must reload the relevant REST snapshot instead of guessing state.
