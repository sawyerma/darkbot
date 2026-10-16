# C0-E1 combined economic observation candidate

> Measurement protocol. Moved unchanged from `project_0/README.md` §18 (commit `4c271cb`); only this heading and note were added.

`P0_MODE=econ4h` runs the existing C0-MO observer and existing Shadow owner in one process,
with copies of one `darkbot_common::timing::Clock`. The owner publishes typed evidence to a
bounded analytics worker. Health, sizing, route-floor economics, builder, signing, simulation
and F8 winner lookup remain the production implementations. `ObserveOnly` rejects every
submission call; startup rejects LIVE/submission flags. No P8 gate is changed.

```bash
# Usual owner-only project_0/.env and external signer prerequisites apply.
project_0/scripts/capture.sh econ4h-launcher
# Integration qualification before the full run:
P0_ECON_DURATION_SECS=600 P0_ECON_SNAPSHOT_SECS=150 project_0/scripts/capture.sh econ4h-launcher
```

Defaults: 14400 seconds, 3600-second snapshots, capital scenarios
`200,500,1000,2500,5000,10000` USD. Exactly four equal intervals are required. Optional:
`P0_ECON_CAPITAL_USD`, `P0_ECON_QUEUE_CAPACITY` (8192), `P0_ECON_RETENTION` (262144),
`P0_ECON_DRAIN_SECS` (180), `P0_ECON_OUT_DIR`, `P0_ECON_RUN_ID`, `P0_PROVIDER_CLASS`,
`P0_RUN_STORE=legacy` (historical `data/econ4h/` root and layout V1; default since 8b.3 is `data/runs/` in layout V2,
see [run storage](../../operations/run_storage.md)).
The market source defaults to `logs` in the script; a distinct `P0_MARKET_WS_URL` can supply
an enhanced transaction stream while Shadow keeps its existing `P0_WS_URL`.

Generated output (layout V2 path in parentheses; V1 keeps every file at the run root): the binary journals
`economics_reports.p0j` with its renders `hour_01.json` through `hour_04.json`, `economics_final.json`,
`opportunity_economics.jsonl` and `canonical_market_shadow.jsonl` (`snapshots/`); `shadow_evaluations.p0j`,
`opportunities.p0j`, `mt0.p0j` and `measurement_phases.p0j` with its render `measurement_phases.jsonl` (`capture/`);
console and runtime logs (`logs/`); and the C0-MO capture `market.p0j` with its renders `transactions.jsonl`,
`liquidations.jsonl`, `coverage.jsonl` plus `summary.json` under `market/` (`capture/market/`). Runs before
2026-10-16 keep their JSONL files and `shadow_offsets.bin`; see [run storage](../../operations/run_storage.md).
These files are runtime data, not versioned evidence. The run stops automatically; bounded
winner and market drain/enrichment can extend wall time beyond the observation duration.

Hourly files contain `window_delta` and `cumulative` sections for market, Shadow, matching,
economics, capital, competition, race timing, bottlenecks and unknowns. They are provisional
at each live boundary and atomically replaced after drain with reconciled winner evidence.
Hour 4 and final are identical. Receive-time `capture_counters` assign even delayed ingress to
its original interval; the original r6 checkpoints remain visible, with post-capture
qualification/enrichment explicitly charged to the final resolution window.

Each candidate-cost record preserves the existing economics inputs and exact chosen plan.
Fee/tip reserve is transaction fee + tip + known rent + flat SOL fee + protocol fee + failed
attempt reserve, separately from working capital. Missing terms make budget eligibility
unknown. Swap costs already embedded in the executable route floor are not added again as
upfront capital. These are conservative per-opportunity budget scenarios; no concurrent
portfolio allocation, realized return or reusable-capital compounding is claimed.

All evaluation records remain on disk. Bounded retained evaluations are coalesced for the
economic report: earliest executable signed opportunity per target/F8 winner, otherwise latest
per target/hour. Multiple reevaluations do not multiply one winner's economic edge. Overflow
fails technical qualification. The current runtime only creates authoritative F8 watches under
its existing winner-relevance policy; unsupported/technical-only candidates can remain unresolved.
The report does not invent winners for them. `UNKNOWN` source identity is explicit for archive
builds; a clean real Git checkout resolves its current implementation commit at startup.

Slot evidence and local provider-observation margins are distinct. The competitor's true send
instant stays UNKNOWN. Same-slot ambiguity is retained, including available transaction indices;
indices never resolve the race. Speedup sensitivity and removing a whole stage are mathematical
local-boundary calculations, not capture guarantees. Source coverage remains unverified and
`live_ready=false` in every report. `T_state` is the existing pipeline state/eligibility-check
mark; it is not relabelled as a newly measured canonical apply timestamp.

Candidate status: local build/regression results are in the external return-package report.
600-second combined Mainnet smoke, differential Clippy, full four-hour observation and any LIVE
qualification must be performed separately; documentation alone claims none of those gates.
