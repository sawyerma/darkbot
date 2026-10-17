# Run control (`p0ctl`) — terminal boundary of the one runtime

`p0ctl` (`native/crates/runtime/src/bin/p0ctl.rs`) starts, observes and stops the existing launcher and runtime and calls the existing verifier/report entry points. It holds no trading, chain, classification or economics logic and never starts a second runtime. Observe-only: LIVE and submission stay disabled. Frontend contract: `frontend/docs/CONTROL_PROTOCOL.md` (FEUI-01); the browser/API boundary is not built.

```bash
cargo build --release --locked --manifest-path project_0/native/Cargo.toml --features "crate_health txv1" --bin p0ctl
```

| Command | What it does |
|---|---|
| `p0ctl doctor` | Offline boot audit BOOT-01…12: source/schema/discriminator identity, runtime owner, unsealed journals, memory/disk reserve, clock, observe-only flags, public wallet identity, measurement scope. Network checks BOOT-03…08 are `NOT_RUN` (verdict at best `OFFLINE_READY_NETWORK_NOT_RUN`). `--scope network` is refused until a permitted network audit exists. |
| `p0ctl preflight --duration D [--capture-source …] [--chain-verify …] [--verify-rps/calls/seconds N]` | Advisory SK-06 capacity preflight against the run store's sealed runs plus the boot verdict; the binding preflight runs inside the launcher for the real run directory. |
| `p0ctl run start --duration D [test.sh options …]` | Offline boot audit (refuses on BLOCKED), refuses a live runtime owner, assigns the run id (`P0_ECON_RUN_ID`, else `econ4h_<UTC>`), re-checks the audited identity right before the start (BOOT-12 TOCTOU) and runs `scripts/test.sh` in its own process group. Ctrl-C forwards an ordered stop. |
| `p0ctl run status [--run ID|PATH]` | Runtime owner (pid, run id, mode) from the lock, the run's capacity verdict, market journal frames/seal state and the last status lines of its runtime log. |
| `p0ctl run stop` | Sends SIGTERM to the live lock holder only: admission closes, the capture drains, journals are sealed, coverage is recorded INCOMPLETE with reason `OPERATOR_STOP`. Repeating it, or stopping when nothing runs, is harmless (`NOT_RUNNING`). |
| `p0ctl verify …` | `scripts/verify.sh` with the same arguments. |
| `p0ctl report --run RUN [--report-* …] [--verification-result ID] [--format table\|csv]` | Native report (`analytics/report.rs`) for runs with `capture/run_manifest.p0j` and `capture/market/market_completion.p0j`; `--report-provenance index`, `--format json` and historical runs go to `scripts/report.sh` (pinned). Writes only `reports/native/` (dataset and query identity). |

**Runtime owner lock.** `p0-longtail` takes an exclusive lock on `P0_RUNTIME_LOCK` (launchers: `<run store parent>/.p0-runtime.lock`, i.e. `project_0/data/.p0-runtime.lock` by default) before any stream, journal or RPC exists, and refuses a second owner (`RUNTIME_OWNER_LOCKED`). The kernel releases it on every exit. Only the observation modes (`econ4h`, `market`) install the ordered stop; Shadow keeps the default signal behaviour.

**Not covered yet:** a typed `RunStatus`/control DTO for the frontend, `runs list`, `logs --follow`, `UntilStopped` horizons and the network boot audit (needs explicit Mainnet read permission).
