# Project_0 operational configuration

The shared Rust contract is `native/crates/solana/src/runtime_config.rs`.
It configures existing transports, admission budgets, workers, MeasurementPlan and
read-only verifier/preflight tools. It does not authorize financial actions.

## Inspect without contacting a provider

From the repository root, after building the release workspace:

```bash
project_0/native/target/release/p0-longtail --help
project_0/native/target/release/p0-longtail --profile smoke --print-effective-config
project_0/native/target/release/p0-longtail --profile custom --rpc-rps 20 --rpc-burst 4 --rpc-concurrency 4 --duration 61s --print-effective-config
project_0/scripts/capture.sh --profile custom --duration 61s --verify index --verify-rps 6 --print-effective-config
project_0/scripts/preflight.sh --profile smoke --print-effective-config
```

Inspection requires neither a signer nor endpoint credentials. The text view lists
name, type, unit, default, range, active value, source and restart requirement.
Endpoint URLs are redacted; array position retains priority. Inspecting a profile
never claims provider capacity, LIVE approval or submission approval.

Precedence is **profile < environment < config file < CLI**. Unknown fields, invalid
types, zero budgets, arithmetic overflow and inconsistent limits are errors.
Use `--config FILE`, `P0_OPERATIONAL_CONFIG_FILE`, `P0_PROFILE`, or the environment
aliases printed by `--help`. Config files are native text (`key = value`, one per line, `#`
comments, CLI names and values) and contain only overridden fields, e.g. `profile = custom`,
`duration = 61s`, `rpc-rps = 12`; duplicate or unknown keys fail. A JSON config is refused
(`CONFIG_JSON_REFUSED`; the Python bridge was removed 25.10., P0-PROD-CTRL-02)
([`config/examples/`](../../config/examples/) holds parser-validated examples).
Empty endpoint values inherit local endpoints. The first explicit endpoint has
highest priority; fallbacks require `--fallback on` and explicit ordered URLs.
Store secret-bearing config files outside Git with owner-only permissions.

All changes require restart. Existing invocations without operational flags or a
profile/config file retain their legacy configuration interface. The Python bridge
uses the compiled Rust parser; it does not maintain a second set of profile defaults.
`P0_CONFIG_BIN` can point to the matching built configuration binary.

## Profiles and execution scope

The four profiles are typed files in [`config/profiles/`](../../config/profiles/) (schema
`Project0/RuntimeProfile/2`, the same `key = value` text format): the only source of their values. `runtime_config.rs` embeds them at
build time and validates them; a profile file may set only `duration`, known `limits` fields and the
`workers` rule. Endpoints, modes, signer, LIVE or submission cannot be expressed, unknown keys fail.
Profiles stay the lowest precedence layer; `--config FILE` remains the override file format.
Changing a profile file requires a rebuild; the binary hash recorded in run manifests covers it.

- `smoke`: 1 aggregate RPC start/s, burst 1, concurrency 1, one HTTP attempt,
  60s capture, 16 MiB response cap, 64 MiB process payload budget. FE observation
  retains its configurable initial 3 calls / 30s / 16 MiB envelope.
- `standard`: conservative existing aggregate 5 RPS, burst 1, concurrency 2;
  see `--print-effective-config` for all values.
- `performance`: uses available CPU parallelism for the existing Rayon pool;
  it does not invent higher qualified provider throughput.
- `custom`: standard defaults plus explicit overrides. Rates above old 1/2/5-RPS
  ceilings are representable; actual provider qualification is a separate gate.

Global, workload, job and phase ceilings all apply; the smallest applicable
ceiling controls a request. `workers` selects the existing Rayon worker count;
fixed-role remote workers keep their existing ownership and use `worker-queue`.
No second execution engine or new submission path is introduced.
`diagnostics` controls the existing integration probe. `verification` controls
post-capture verification through `capture.sh`/`verify.sh`; direct runner invocations
cannot silently request an unconnected post-capture verifier.

The process budget bounds active HTTP calls, queued admissions, calls, payload
bytes, WSS connections/subscriptions and individual WSS messages. Priority goes
to state/health and simulation; every eighth admission serves the oldest waiter.
HTTP 429 Retry-After is respected across endpoints/workloads, including a longer
server delay than the configured local backoff maximum. Adaptive mode halves the
allowed start rate after overload/transport failure and increases it after eight
successful requests, never above the configured ceiling. This is observed local
backpressure, not a provider quota certificate. Explicit read retries may use the
next endpoint; `sendTransaction` is never retried or sent to a fallback here.

Byte budgets count serialized requests and consumed response/frame payloads,
not TCP/TLS/header overhead or unread error bodies. Blocking recovery/enrichment
responses and asynchronous RPC/WSS responses consume the same Rust process
budget. The serial Python verifier applies equivalent request/response accounting
within its own phase. Existing decoder/checkpoint limits may impose tighter
per-response bounds. Disk/RAM preflight and guards remain active for verification.

`duration` feeds the existing MeasurementPlan: exact quarter snapshots, including
odd seconds, separate bootstrap/drain/verification/shutdown budgets. Captures do
not consume their duration during bootstrap. Configured standalone Shadow starts
its window at state/threshold readiness, does not extend it after reconnect and
checks bootstrap expiry on the owner loop. A blocking startup RPC is bounded by
its transport timeout; this is not a preemptive process watchdog. Long measurement
runs retain the existing launcher lifecycle and separate phase budgets.

The launcher persists redacted operational metadata next to its MeasurementConfig;
the Rust measurement manifest also records the effective contract. Standalone
Shadow stores an immutable timestamped operational record next to its ledger.
The private Rust-to-Python handoff is mode 0600, is never overwritten and is
removed after the child workflow exits. Endpoint inheritance is resolved again
after the launcher's existing owner-only environment loader.

## Separate authorization gates

A future authorized FE observation can pass `--profile`, `--fe-calls`,
`--fe-seconds`, `--fe-max-mib`, endpoint and transport options to
`scripts/preflight.sh --fe-mode smoke --execute --output NEW_DIRECTORY` (native
`p0-fe-qualify`, typed `fe01_smoke.p0j` plus text report). The observation still
performs the three dependent program/ProgramData/target reads; larger
budgets do not manufacture additional evidence or a profitability claim.

The network boot audit (`p0ctl doctor --scope network`, BOOT-03…08) has its own budget,
used exclusively by that audit; it changes no capacity or quota assumption:

| Parameter | Unit | Default | Meaning |
|---|---|---|---|
| `--boot-calls` / `P0_BOOT_CALLS` | requests | 64 | RPC reads of one audit, capped by `--max-requests`. Refused before any request (`BOOT_BUDGET_TOO_SMALL`) below the reads the audit runs: 5 cluster reads, program and ProgramData, and every `--fe-snapshot-rounds` round over the initial scope (program, ProgramData, group, clock, fee state, targets) in `--fe-batch-size` batches (15 with defaults). Dependencies discovered on chain add batches; an exhausted budget blocks the audit, never passes it. |
| `--boot-seconds` / `P0_BOOT_SECONDS` | s | 120 | Wall-clock budget of one audit, at most 300. |

The FE-01 observation limits (`--fe-calls`, `--fe-seconds`) do not apply to the boot audit.

No Mainnet observation, new capture, user-owned 5h replay, provider purchase,
signing/submission or LIVE execution is authorized by these examples or profiles.
FREE_ONLY remains mandatory; endpoint registration is not proof of free access.
Provider qualification must cover the requested verifier rate/call budget; missing
or insufficient evidence remains OPEN/HISTORICAL_INCOMPLETE. Current funding,
complete transaction simulation, executable exit and positive realized profit
remain the FE-01/02/04 qualification tasks.
