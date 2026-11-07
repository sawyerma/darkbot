# BSC React UI

Graphite V8 (06.11.2026, `frontend/design/reference/bsc_graphite_v8_approved.html`, FRONTEND_PHASENPLAN
§13A): four areas **Overview | Training | Live | Wallets**, the Health pill top right with its own window
and the Settings drawer, over one Zustand store and one adapter on the shared Protobuf WebSocket. No
Project_0 imports. Every control is a request the native owner applies or refuses; LIVE is effective only
under the owner's LIVE gate for the operator's granted scope, and the browser signs or sends nothing.

- **Data:** `bsc.status`, `bsc.launch.state`, `bsc.monitoring.state`, `bsc.wallets.state` and
  `bsc.live.state` are decoded from `darkbot.bsc.v1` (`api/bscAdapter.ts`); `bsc.live.state` is the one LIVE
  source. Public native leaves arrive as `NativeRecord`s with kind and unit.
- **V8 contracts:** training, Strategy C settings, trade history, wallets and WATCH in
  `protocol/schemas/darkbot/bsc/v1/{common,training,history,wallet,watch,query}.proto` (`api/bscV8Adapter.ts`,
  golden bytes `protocol/golden/bsc/v8`). Their bounded query (`core/realtime/bscQuery.ts`,
  `POST /api/v1/bsc/query`, the reporter's `--query`) answers the Strategy C settings state natively
  (`pages/useStrategySettings.ts`: the form basis = stored over the configuration file, the native bounds,
  the apply status); every other query is NOT_IMPLEMENTED until its native owner exists, and the UI shows
  that function disabled with its reason, never data of its own.
- **Wallets** (`pages/Wallet*.tsx`, `model/wallets.ts`): Accounts → Balances → Manual transfers → Automatic
  profit transfer (policy A) → Automatic payout (policy B) → Transaction history → Balance & profit. Main,
  Trading (Buyer), External (receive-only, no MetaMask connection) and the separate G0.7 launcher are never
  summed. A manual transfer is a prepare (the owner's DRAFT with its scope text, its SHA-256 checked here)
  and an authorization of that digest; a destination is saved, then confirmed on its own; an enabled policy
  carries an authorization (expiry, per-transfer and total maximum within the owner's bounds) with its
  canonical scope text's SHA-256. All go to the native treasury owner as typed commands at its version
  (`walletCommand`), offered only while it answers the treasury query; an open (DRAFT/PENDING/SENT/UNKNOWN)
  transfer locks its path. Drafts live in the view only.
- **Placement:** every native field keeps its section from `frontend/contracts/bsc-fields.json`
  (`model/placement.generated.ts`, `check:bsc-fields`); the pre-V8 groups (Overview, Launch / BUY,
  Monitoring, Analytics) render in the Settings drawer, the LIVE groups in the Live drawers.
- **Controls:** one command client (`core/realtime/bscControl.ts`): readiness, candidate, source/venue/
  component modes, wallet and economic switches, limits, the LIVE grant/revoke/lot/nonce decisions and
  Strategy C settings (one change: `bsc.strategy.set`; several: one `bsc.strategy.set_all` generation
  with the complete filter list, `model/strategyForm.ts` `saveRequest`). A lost acknowledgement gets one lookup.
- **Never shown:** salts, raw transaction bytes, the private B economics record and absolute local paths.
  Wei amounts are exact (`pages/V8.tsx` `bnb()`, `model/strategyForm.ts`), never floating point.

Tests:
- `src/features/bsc/**/*.test.ts` (incl. `model/v8.test.ts`, `model/wallets.test.ts`, `api/bsc-v8-golden.test.ts`) and
  `src/core/realtime/bscControl.test.ts`
- `tests/bsc.spec.ts` (`npm run test:bsc`) and `tests/visual/bsc-v8.spec.ts` (`npm run visual`)
- the real gateway→browser run `ops/tests/bsc_control_test.py`
