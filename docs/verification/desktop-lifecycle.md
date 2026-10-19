# macOS Desktop lifecycle qualification

Implementation base: 638297ace3f4eea40f55bdc796f682e6fe594e2a, original checkout, main.
User correction: no fixed recording time. Explicit setting `P0_APP_CAPTURE_SECS=until-stopped`.

## Behavior

`darkbotctl app` ensures the gateway, then one read-only capture including its reporter.
The desktop app uses this same path. While a capture runs, clicking the icon offers orderly
Stop or opening the cockpit. Closing a browser window never stops recording. There is no
trading/submission/wallet activation. BSC implementation and configuration are unchanged.

Capture is a launchd job without KeepAlive and outside LaunchAgents. One short lifecycle
lock serializes starts/signals. Stop remembers launch UUID and PID; waits cannot follow a
replacement run. A separate down reservation prevents starting while stopping the gateway.
The engine's reporter generation lock remains authoritative. Stale sockets/locks/temp
files are removed only without live owners. Completed results are atomically written 0600
and attributed to launch UUID, run ID and generation. Old results cannot describe a crash.

macOS launchd cannot read this Desktop checkout under current privacy controls (observed
Operation not permitted). Capture therefore executes the deployed binary in Application
Support directly. The controller stages configuration privately before launch. Runtime data
is stored under `~/Library/Application Support/DarkBot/data/market/`.

Continuous recording uses an explicit `until-stopped` setting. A numeric zero remains an
invalid input; emitted configured duration zero denotes this explicit continuous mode.
At stop, the capture window is frozen before the existing drain/seal path. Operator-stopped
journals remain sealed INCOMPLETE/unverified under the existing qualification rules. This
is not data loss or an assertion of verified complete market coverage. Hard-killed journals
are never rewritten or relabeled complete; recovery creates a distinct run directory.

The UI requires increasing application frames and expires freshness after 11 seconds.
Run-ID, source-ID and reporter-generation changes reset this evidence atomically. First
snapshots are Checking, finished runs Last run, detached snapshots last known. HTTP 429 is
visible; failed stream startup has a 600-second automatic restart cooldown. No timer retries
or provider escalation are added. Transport connected is explicitly a last-reported value.

HTML/navigation responses revalidate; successful non-HTML Vite assets are immutable.
Schema/major rejection reloads once per rejected server schema, then remains BLOCKED.
Origin, cookie, host and protocol rejection checks remain enabled.

## Observations before changes

Installed commit was already 638297a. Existing icon called `up`, which only started the
gateway. Actual fresh browser probe showed Gateway OPEN, overview.aggregates LIVE, Project0
Detached with an ended run. BLOCKED was not reproducible, and historical rejection evidence
was unavailable. It is therefore not claimed that a historical BLOCKED cause was proven.

## Reproducible verification

- Frontend type-check, eslint, import boundaries, frozen design and CSS checks.
- 15 frontend unit tests, including unknown/stale progress, run/generation reset and skew guard.
- Five browser scenarios: lifecycle, missing/malformed data, mobile layout, autonomous
  freshness expiration and run/generation reset, rejected schema reload with zero subscriptions.
- Real reporter -> real gateway -> browser, restart from generation 1 to generation 2.
- Reporter: 23 tests (including atomic/private result persistence); runtime market: 127 passed,
  one pre-existing ignored SDK probe; status tap: eight passed.
- Gateway: 21 tests; external reporter integration separately exercised by browser test.
- Protocol lint, breaking check and exact regeneration; wire schema ID unchanged:
  `5cc6d23211a5c12641c8c7c6d52471e6dbb870f500619b4cecb2a14d092a4752`.
- `python3 ops/tests/lifecycle_test.py`: real macOS launchd, private com.darkbot.test.* jobs,
  local fake Solana only. Four concurrent starts retain one PID/launch/generation. Real browser
  counters grow. Gateway SIGKILL/recovery preserves capture. Stop seals completion artifact.
  Capture SIGKILL leaves no falsely attributed result; stale IPC cleanup and newer generation
  succeed. Concurrent down/start is rejected. HTTP 429 appears in browser and cooldown prevents
  a second automatic capture. All test jobs are booted out in cleanup; no production kill.

Successful isolated evidence: `/private/tmp/darkbot-test.m0sc3am6/` (health-live.png,
health-429.png, logs, private test plists, journal files, PASS). Test data is synthetic.

## Installation

After commit/push, rebuild with `ops/darkbotctl build`, deploy, update the Desktop app via
`ops/darkbotctl install-apps`, and set `~/.darkbot/app.conf` explicitly:

```text
P0_APP_AUTOSTART_CAPTURE=1
P0_APP_CAPTURE_SECS=until-stopped
```

The actual installed icon start and installed/local/remote commit equality are a subsequent
installation acceptance step, not asserted by this source-level report.
