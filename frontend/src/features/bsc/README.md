# BSC React UI (read-only)

Five existing Graphite panels (Overview, Launch / BUY, Monitoring, Analytics, Wallets) with one
Zustand store and one adapter over the shared Protobuf WebSocket. No Project_0 imports and no BUY,
SEND, ARM, SIGN or SWEEP controls.

- **Data:** `bsc.status`, `bsc.launch.state`, `bsc.monitoring.state` and `bsc.wallets.state` are decoded
  from `darkbot.bsc.v1` (`api/bscAdapter.ts`). Typed fields carry the guard, gates, readiness (native
  verdict, mode, candidate binding), jobs and wallets. Every other public native leaf arrives as a
  `NativeRecord` with kind and unit.
- **Placement:** every native field has exactly one panel section in the fixed order of
  `frontend/docs/FRONTEND_PHASENPLAN.md` §13. The rules live in `frontend/contracts/bsc-fields.json`.
  `model/placement.generated.ts` is a copy of them (`npm run gen:bsc-placement`), and
  `check:bsc-fields` fails when the copy is stale or a census field is unplaced. Record families
  without data show `NO_RECORD`; planned producers show `FUTURE_PHASE_BLOCKED`.
- **Controls:** there is one readiness start/stop (Launch / BUY section 8) and the offline A/B
  candidate request. Process state comes only from the pushed job, never from HTTP polling. The
  command outcome comes from the POST response; a lost acknowledgement gets one lookup.
- **Never shown:** salts, raw transaction bytes, the private B economics record and absolute
  local paths (native census sensitivity). Wei amounts are displayed exactly.

Tests:
- `src/features/bsc/**/*.test.ts` and `src/core/realtime/{readiness,candidate}Control.test.ts`
- `tests/bsc.spec.ts`
- the real gateway→browser run `ops/tests/bsc_control_test.py`
