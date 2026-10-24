# BUGS A23-09 / JFI-04 — production JSON callsite inventory (2026-10-24)

Reproducible: `tests/regression/test_jfi04_json_inventory.py` (part of the gate's Python stage)
scans `native/crates` for production uses of `serde_json::…`, `json!(` and — in files importing it —
bare `Value` (outside `#[cfg(test)]` modules, `*_tests.rs`, `tests/`, `#[cfg(test)] mod x;` files),
and pins the per-file count and categories. The pin is the progress counter: any new file, changed
count or category fails until table and evidence are updated together.

The reviewer's narrower metric (`serde_json::Value|json!(`) reproduces as 222 production occurrences
in 35 files (420 incl. inline tests). The broad metric is **419 occurrences in 44 files** (it also
finds `serde_json::from_*`/`to_*` and imported bare `Value`, e.g. the MT0 path the narrow metric
misses).

Categories: **W** external wire · **R** offline render/stdout/export · **H** historical reader or
explicit offline import · **C** operator config/approval file · **I** internal candidate.

| file | n | cat | execution path (callers) |
|---|---|---|---|
| solana/src/chain.rs | 40 | W R | decoders of getTransaction/getBlock/WSS notifications (winner.rs, market_reconcile.rs, market/core.rs, ws_rpc.rs); `encode_*` only inside `MarketTransaction::legacy_json` |
| runtime/src/market/rows.rs | 34 | R | `legacy_json` views only via payload/journal export: `p0-journal export/renders`, `p0ctl export`, `p0-strict-verify --compat`, test `JsonlSink` |
| runtime/src/analytics/journal.rs | 29 | H I | schema-1 value-tree codec: `legacy_value`/`decode` (historical frames) and the schema-1 writer (`append_legacy`/`append_value`/`write_value`) used by the I paths below |
| solana/src/runtime_config.rs | 25 | R H | config export/`--export-environment` stdout, FE-01 `operational_config.json`; `decode_legacy_json` for historical config |
| runtime/src/market_verification.rs | 19 | W R | strict verifier RPC params/responses (getSlot/getBlocks/getBlock); `render_compat` only for `--compat` |
| runtime/src/bin/p0-fe-qualify.rs | 18 | W R I | getMultipleAccounts body; result.json/stdout; **FE01_CONFIG/READ/RESULT written as schema-1 value frames** |
| runtime/src/orchestration/fe_replay.rs | 17 | W R I | request rebuild for comparison; `--replay` report; **`bind_scope` mutates the FE-01 report Value journaled as FE01_RESULT** |
| solana/src/submit/jito.rs | 17 | W | Jito block-engine JSON-RPC (transport, pipeline, observe-only) |
| solana/src/bin/p0-probe-v1.rs | 16 | W R | simulateTransaction probe, stdout |
| runtime/src/analytics/renders.rs | 14 | R | `render_config`/`render` for `p0ctl export`, `p0-journal renders` |
| runtime/src/orchestration/fe_qualification.rs | 14 | W I | `decode_batch` (wire); **`report` is a Value data model persisted as FE01_RESULT** |
| solana/src/ingress/ws_rpc.rs | 14 | W | subscribe requests, notification frames |
| runtime/src/bin/p0-journal.rs | 13 | R I | verify/renders/import stdout; **`import` writes stdin JSONL as new schema-1 C0_CENSUS frames** ("open JFI item" in code) |
| runtime/src/analytics/econ_docs.rs | 12 | R | `legacy_json`/text renders (human-readable Serialize only) |
| runtime/src/analytics/measurement.rs | 12 | R H | `PhaseRecord::legacy_json` view; JSON plan file only for historical/manual runs (launcher hands a `.p0j`) |
| runtime/src/bin/p0-probe-costs.rs | 12 | W R | getTransaction/getSignaturesForAddress probe, stdout |
| runtime/src/orchestration/boot_network.rs | 12 | W | `p0ctl doctor --scope network` reads |
| solana/src/provider.rs | 11 | W | metered HTTP JSON-RPC sender |
| solana/examples/p0-gap-ws-proxy.rs | 10 | W | example loopback WS proxy (qualification) |
| solana/src/submit/rpc.rs | 9 | W | simulate/fee/send bodies, `sim_result_of` |
| runtime/src/execution/one_shot.rs | 7 | H | `read_legacy` of a pre-binary `<wallet>.json` (once, bound by hash) |
| runtime/src/analytics/econ_canonical.rs | 6 | R H | `joined_value` view; `p0-econ-replay` reads JSONL market events / legacy shadow JSONL and writes hour_NN.json |
| runtime/src/analytics/econ.rs | 5 | R | `opportunity_view`, `atomic_json` for replay outputs |
| runtime/src/orchestration/fe_freeze.rs | 5 | I | **freeze typed, its JSON summary merged into FE01_RESULT** |
| runtime/src/execution/route.rs | 5 | W | Jupiter `/build` response |
| solana/src/ingress/mod.rs | 5 | W | programSubscribe / getProgramAccounts filters |
| runtime/src/market/summary.rs | 4 | R | `MarketFinal::legacy_json` (summary.json render) |
| runtime/src/bin/p0-parity.rs | 4 | H | offline parity over historical getTransaction JSON |
| runtime/src/analytics/fixture.rs | 4 | W | Jupiter swap-instructions parser (no production caller) |
| solana/src/state/snapshot.rs | 4 | W | getProgramAccounts decode |
| runtime/src/market/winner.rs | 3 | W | `fetch_transaction` |
| runtime/src/market_gap.rs | 3 | W | `gap_request_metered` |
| runtime/src/market_reconcile.rs | 3 | W | `reconcile_range` (getBlock) |
| runtime/src/market_truth.rs | 2 | H | human-readable branch of `NativeEvidence` (p0-econ-replay JSONL input) |
| runtime/src/bin/p0ctl.rs | 2 | C | operator `Approval`/`ExitApproval` files |
| runtime/src/analytics/mt0_journal.rs | 1 | I | **MT0 snapshot JSON text (`common::mt0::snapshot_json`) stored as schema-1 value frames** |
| runtime/src/analytics/payload.rs | 1 | R | `legacy_view` (export) |
| runtime/src/bin/p0-econ-replay.rs | 1 | C | measurement plan file |
| runtime/src/bin/p0-measurement-plan.rs | 1 | R | stdout |
| runtime/src/bin/p0-strict-verify.rs | 1 | C | legacy `JOB` file (native path: `--run`, typed) |
| runtime/src/market/deployment.rs | 1 | W | getAccountInfo |
| runtime/src/market/observer.rs | 1 | W | RPC fetch closure type |
| runtime/src/orchestration/provider_evidence.rs | 1 | H | explicit offline import of measured provider JSON (productive paths read the typed record) |
| runtime/src/orchestration/startup.rs | 1 | R | `--export-environment` stdout |

## Internal candidates (I) — the targeted JFI follow-ups (no historical evidence rewritten)
1. **FE-01 evidence model** (`p0-fe-qualify`, `fe_qualification::report`, `fe_replay::bind_scope`,
   `fe_freeze::record`): the qualification report is a `serde_json::Value` and `fe01.p0j` stores
   FE01_CONFIG/READ/RESULT as schema-1 value frames. Target: typed payloads (schema 2) with the
   historical schema-1 frames (FE-01 smoke `da8512ad`) kept readable — before the next FE-01 run.
2. **MT0 snapshots** (`common::mt0::snapshot_json` → `mt0_journal`): telemetry persisted as JSON
   value frames on the reporter thread (the owner itself stays JSON-free).
3. **`p0-journal import`**: offline import of external JSONL rows as schema-1 C0_CENSUS frames.

Not violations, watched: `JsonlSink` is a public production type built only in tests;
`p0-strict-verify --compat` renders legacy JSON after a live verify (opt-in, transitional).
