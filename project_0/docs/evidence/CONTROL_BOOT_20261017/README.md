# Terminal control and offline boot audit (FEUI-01 / BOOT-01…12 base) — 2026-10-17

**Verdict:** control boundary and offline boot audit **LOCAL_PASS in scope**. Network boot checks (BOOT-03…08: cluster genesis, WSS/provider, ProgramData, protocol state, dynamic addresses, oracles) **NOT_RUN** — they need explicit Mainnet read permission; an offline audit is never a network READY. Operator guide: [run_control.md](../../operations/run_control.md).

## Implemented

- `orchestration/control.rs`: exclusive runtime-owner `flock` with pid/run id/mode (double start and second writer refused before any stream/journal/RPC; released by the kernel on every exit); SIGTERM/SIGINT handler that only sets a flag.
- `orchestration/startup.rs`, `analytics/econ.rs`, `market/capture.rs`: the econ4h supervisor turns the flag into the existing ordered abort (`operator_stop` + `abort`); standalone market capture polls it directly; the capture stop reason is `OPERATOR_STOP` (also before StreamReady, where it was previously reported as `STREAM_NOT_READY`).
- `orchestration/boot_audit.rs`: typed offline audit (BOOT-01 source/journal registry/discriminators, BOOT-02 owner/unsealed journals/memory/disk/clock, BOOT-09 observe-only flags and public wallet, BOOT-10 scope, BOOT-11 not applicable for observe, BOOT-03…08 NOT_RUN) with a readiness token over the audited identity; `recheck` refuses drift (`BOOT_TOCTOU_DRIFT: source|runtime_config|safety_flags|schema`).
- `bin/p0ctl.rs`: `doctor`, `preflight`, `run start|status|stop`, `verify`, `report`.
- `scripts/capture.sh` exports `P0_RUNTIME_LOCK` for econ4h, market and Shadow; `.gitignore` covers `data/.p0-runtime.lock`.

## Evidence (`control_offline.txt`)

Offline control test against unreachable localhost endpoints (no Mainnet): a second `p0-longtail` exits 2 with `RUNTIME_OWNER_LOCKED`; `run status` shows the owner; `run stop` → the runtime ends `MARKET_CAPTURE_INCOMPLETE: OPERATOR_STOP`, its market journal verifies sealed with `OPERATOR_STOP` in its coverage rows; a second `run stop` reports `NOT_RUNNING`; `doctor` → `OFFLINE_READY_NETWORK_NOT_RUN` with six NOT_RUN network checks; `doctor --scope network` refused; `preflight --duration 4h --chain-verify strict` → start not allowed.

## Tests

`control::owner_lock_refuses_second_owner_and_reports_holder`, `boot_audit::offline_audit_blocks_and_binds_identity` (network NOT_RUN, live owner and LIVE flag block, unsealed journal warns, token drift on safety flags and source); market 104 + control/boot 2 unchanged otherwise; launcher integration `test_measurement_plan` 4/4 incl. `p0ctl run start` through the real launcher (boot audit, run id and lock handover, child exit, idempotent stop).
