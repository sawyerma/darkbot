# BSC React UI

Six existing Graphite tabs (Overview, Launch / BUY, Monitoring, Analytics, Wallets, LIVE) with one
Zustand store and one adapter over the shared Protobuf WebSocket. No Project_0 imports. Every
control is a request the native owner applies or refuses; LIVE is effective only under the owner's
LIVE gate for the operator's granted scope, and the browser signs or sends nothing.

- **Data:** `bsc.status`, `bsc.launch.state`, `bsc.monitoring.state`, `bsc.wallets.state` and
  `bsc.live.state` are decoded from `darkbot.bsc.v1` (`api/bscAdapter.ts`). Typed fields carry the
  guard, gates, readiness (native verdict, mode, candidate binding), jobs, wallets and the LIVE gate;
  `bsc.live.state` is the one LIVE source (tabs, overview and gateway); it also carries the LIVE
  campaign verdict and its attempts as recorded (REV.1.0 R2b, evidence only). Every other public native
  leaf arrives as a `NativeRecord` with kind and unit.
- **Placement:** every native field has exactly one panel section in the fixed order of
  `frontend/docs/FRONTEND_PHASENPLAN.md` §13. The rules live in `frontend/contracts/bsc-fields.json`.
  `model/placement.generated.ts` is a copy of them (`npm run gen:bsc-placement`), and
  `check:bsc-fields` fails when the copy is stale or a census field is unplaced. Record families
  without data show `NO_RECORD`; planned producers show `FUTURE_PHASE_BLOCKED`.
- **Controls:** one command client (`core/realtime/bscControl.ts`): the readiness start/stop, the
  offline A/B candidate request and the native owner's requests (modes, wallet and economic
  switches, limits, the LIVE grant of a confirmed scope and its revocation). Process and owner
  state come only from the push, never from HTTP polling; a lost acknowledgement gets one lookup.
- **Never shown:** salts, raw transaction bytes, the private B economics record and absolute
  local paths (native census sensitivity). Wei amounts are displayed exactly.

Tests:
- `src/features/bsc/**/*.test.ts` and `src/core/realtime/bscControl.test.ts`
- `tests/bsc.spec.ts` (`npm run test:bsc`, CI workflow `bsc`)
- the real gateway→browser run `ops/tests/bsc_control_test.py`
