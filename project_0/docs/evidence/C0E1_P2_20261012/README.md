# P2 — Transport / Provider, 2026-10-12

Base: `d40d89d8fef98e69c505141ea17dc4a7a3cde658` (P3).
Scope: PHASENPLAN C0-E1 master contract, package P2 only.
All new network tests use loopback fixtures. No new capture, historical RPC query,
Mainnet smoke, submission, provider purchase or BSC edit was performed for P2.

## Implemented

- Existing `WsRpcControlSource` reused. C0-E1 / market opt into a fixed **120 s
  outage recovery budget / 8 failed attempts**, independent of capture duration.
  Application/slot idle timeout remains 60 s. A previously healthy connection
  therefore has at most the idle-detection interval plus its recovery budget,
  followed by bounded shutdown/drain. Ten seconds of continuing slot progress
  are required before a new failure can reset the recovery budget; a single
  frame or ACK/flapping connection cannot reset it.
- States STARTING, READY (ACKs), CAPTURING, DEGRADED, RECOVERING, RECOVERED,
  INCOMPLETE_ABORT / FINISHED journaled. C0-E1 StreamReady requires all ACKs AND
  actual advancing subscribed slot evidence, with the **same frame T_rx** opening
  the measurement window. Duplicate slots, unknown subscriptions and Ping/Pong
  do not prove restoration. Parsed application health and progress age recorded.
- Handshake/read/write waits bounded; shutdown interrupts the connection and
  backoff. Cancellation also removes queued provider admission so a Tokio
  blocking worker cannot keep ingress alive for a full quota cooldown.
- Market/Shadow ingress exhaustion signals the existing shared abort. The
  observer drains and writes an INCOMPLETE report, returns an error, and defers
  strict/gap/enrichment/deployment-after RPC. Existing raw evidence remains.
- Repeated failures before actual restored slot evidence coalesce into one
  interval; all raw GAP and transport state rows remain. Earliest unknown
  boundary is never replaced by a later known slot; another failed connection
  invalidates a pending ACK. Monotonic/UTC bounds, last healthy evidence, raw
  failure count and local outage duration are reported. Distinct restored
  outages remain distinct; the existing 32-interval cap is retained. Finalized
  gap backfill, not a reconnected stream, establishes recovery completeness.
- A shared Solana provider budget uses the existing SDK `RpcSender` seam and
  existing blocking fetchers. No second trading/economics implementation.
  Account-level conservative aggregate across ALL configured Solana endpoints:
  **5 starts/s, 2 concurrent requests/handshakes, 64 waiting admissions, FIFO**.
  Per-workload spacing: history/simulation/winner 500 ms, WSS handshake 1 s,
  bootstrap/live-read/deployment 200 ms. Different URLs are not assumed to have
  independent purchased quotas. These are client ceilings, NOT proven provider
  capacity; a paid account shared with other processes is outside this limiter.
- HTTP/WSS 429 propagates a shared cooldown. Numeric/date Retry-After is honored;
  exponential backoff plus bounded jitter, then 60 s circuit cooldown after three
  quota responses. SDK reads have at most three attempts within their original
  request time budget. sendTransaction is never retried by this wrapper.
  SDK budget opt-in is only `econ4h`; normal submission client construction and
  ObserveOnly/LIVE gates remain. Provider errors log method/status/workload,
  never endpoint/query/body; endpoint banners now contain scheme+host only.
- Gap/strict/getTransaction fetches share the same quota and their own bounded
  request budgets. Deployment retains a separate 30 s / three-attempt budget
  and distinct DEPLOYMENT_* incomplete errors. The existing Shadow quiescence
  barrier still precedes historical capture verification. P3's separate CLI
  keeps its own bounded index budget and is not launched concurrently here.
- RPC result payloads and structured protocol error codes/data are preserved;
  remote free-text error messages are sanitized. GPA bootstrap has a 512 MiB
  response safety ceiling, other SDK requests 64 MiB; this is no RAM/24h-resource
  qualification (P4 remains open).

## Focused consumer inventory (source topology, not new Mainnet load measurement)

| Consumer | Existing demand / placement | P2 budget integration |
| --- | --- | --- |
| Shadow discovery/snapshots/recovery | filtered GPA, getMultipleAccounts, finalized/root reads; main owner | Bootstrap SDK sender |
| Blockhash maintenance | dedicated refresh worker | LiveRead SDK sender |
| Health | primarily local canonical state; integration projection external | unchanged health logic; projection quota below |
| Projection + oracle projection | two existing bounded workers; simulation/post-accounts | Simulation SDK sender via RpcSubmitter |
| Pipeline build/sign/simulate | existing ObserveOnly adapter | same Simulation quota; no submit |
| Winner/reconciliation | two existing bounded workers | Winner SDK sender |
| Deployment before/after | existing finalized ProgramData inspection | Deployment quota, 30 s deadline |
| Shadow + market WSS | up to two active streams, plus replacement sessions | shared handshake admission + outage circuit |
| Canonical/gap/log enrichment | after capture AND Shadow exit; existing up-to-eight read-ahead jobs | aggregate at most two active transfers; History 2 starts/s |
| External historical index | existing Python verifier, explicit invocation only | existing P3 bounded rate/call/time/byte budget; not run here |
| Jupiter quote | separate Jupiter endpoint, existing sequential 1.5 s timeout | unchanged separate service; not Solana account quota |
| Jito / LIVE | not submitted in C0-E1 | remains blocked |

Actual provider tariff, method weights, global account demand from other processes,
archive horizon, long-lived WSS availability and whether quotas are shared across
provider hostnames remain **OPEN**. No provider upgrade or guaranteed quota is inferred.

## Executed qualification

Commands below run from the repository root. Output files alongside this report
are the actual command output, not inferred PASS values. Only trailing empty
lines at the end of text logs are normalized for Git whitespace checks.

```sh
cargo test --manifest-path project_0/native/Cargo.toml -p darkbot-solana -p darkbot-runner --lib --features crate_health,txv1 --locked --offline
cargo test --manifest-path project_0/native/Cargo.toml -p darkbot-solana -p darkbot-runner --lib --features crate_health --locked --offline
cargo test --manifest-path project_0/native/Cargo.toml -p darkbot-solana --lib provider --features txv1 --locked --offline
cargo test --manifest-path project_0/native/Cargo.toml -p darkbot-runner --lib market_observer --features crate_health,txv1 --locked --offline
cargo test --manifest-path project_0/native/Cargo.toml -p darkbot-runner --bin p0-longtail --features crate_health,txv1 --locked --offline
cargo build --release --locked --offline --manifest-path project_0/native/Cargo.toml -p darkbot-runner --features crate_health,txv1 --bin p0-longtail --bin p0-measurement-plan
cargo clippy --manifest-path project_0/native/Cargo.toml -p darkbot-solana -p darkbot-runner --all-targets --features crate_health,txv1 --locked --offline
python3 project_0/scripts/test_measurement_plan.py
python3 project_0/docs/evidence/C0E1_P2_20261012/check_changed_format.py
git diff --check
```

- Libraries: **186 runner + 54 solana PASS** with txv1; **186 + 52 PASS**
  without txv1; zero failed, one pre-existing ignored test in each runner suite.
- Runner binary: **9 PASS**; P1 CLI: **4 PASS**; final provider regression: **5 PASS**.
- New cases cover 527 raw failures, unknown boundaries/invalidated ACK, no-RPC
  abort finalization, actual loopback HTTP429/date parsing/circuit, SDK success
  and structured simulation failure, concurrency/admission cleanup, WSS429,
  ACK/Ping-only silence, repeated old slots, disconnect→proven recovery and
  cancellation while waiting for provider admission.
- The final small diagnostic-only addition (method/status on HTTP errors) was
  compiled/retested in `provider_final.txt`, binary tests and the final release/
  clippy logs after the two library matrices. Decision/error semantics unchanged.
  Final UTC journal mapping uses the common frame Clock during capture and drain;
  all **89 market-observer tests PASS** afterward (`market_final.txt`).
- Formatting: new files plus modified regions checked against the recorded base;
  unrelated legacy formatting is deliberately not rewritten. Clippy exits zero
  with existing warnings; no repository-wide zero-warning or strict Clippy claim.
- Development runs found and corrected (before qualification): raw GAP count vs
  interval completeness, admission budget incorrectly shortened to frame-idle
  timeout, old slot telemetry counter compatibility, and a loopback fixture's
  incorrect HTTP Content-Length. Failed development outputs are preserved in
  `development_failures/`; final outputs above supersede them.

**NOT RUN / NOT CLAIMED:** Mainnet preflight, 60/600 s Mainnet smoke, five-hour
capture or full historical replay, P4 strict partition/resume + resource fixes,
P5/P6 master qualification. Local transport recovery is not finalized market
completeness, historical economics, profitability or production readiness.

Next: report P2 and await the next package. The user performs the long measurement
only after the remaining technical/provider gates. No automatic five-hour run.
