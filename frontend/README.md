# DarkBot Frontend

Single React frontend for all DarkBot backend domains.

## Ownership

- `features/bsc/` — BSC Birth Entry / Gate0 UI.
- `features/project0/` — Project 0 liquidator UI.
- `features/solana/` — Pump.fun and shared Solana UI.
- `core/api/` — typed HTTP control-plane client only.
- `core/realtime/` — one WebSocket event stream with reconnect and sequence-gap detection.
- `shared/` — presentation-only UI/layout primitives.

The browser is not a second trading engine. Protocol decoding, health, economics, signing and execution stay in the backend. REST supplies snapshots; WebSocket supplies deltas/events.

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
