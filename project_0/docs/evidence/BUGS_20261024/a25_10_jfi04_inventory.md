# A11 — JFI-04 production JSON inventory, regenerated (25.10.)

Source: live scan of `project_0/native/crates` at `72b74fc1` (the regression test `test_jfi04_json_inventory.py`; the scan equals the pin). Supersedes the table of `BUGS_20261024/a23_09_jfi04_inventory.md` for the current code (that file stays as the 24.10. evidence, unchanged).

**305 occurrences in 33 files; open internal (I) list: empty.** By category (a file can carry several): C 1, H 161, R 28, W 183. DEC-12: +2 W (`sendTransaction` on the deadline-bound wire); DEC-02: +1 W (`simulateBundle` context slot); DEC-03: +13 W (B/C non-landing reads). Update 26.10.: DEC-09 — every H use is compiled only with the `historical` feature (standard build: none, `test_dec09_historical_feature.py`); DEC-10 — the FE-04 approval is native `key = value`, its historical JSON reader is H.

W external wire · R offline render/stdout/export · H historical reader or explicit offline import · C operator config/approval file input · I internal candidate.

| File | Count | Category | Reader / reason |
|---|---:|---|---|
| runtime/src/analytics/econ.rs | 2 | H | read-only historical view check only (renders::check, p0-journal renders); no JSON writer exists (25.10., F4) |
| runtime/src/analytics/econ_canonical.rs | 2 | H | read-only historical view check only (renders::check, p0-journal renders); no JSON writer exists (25.10., F4) |
| runtime/src/analytics/econ_docs.rs | 12 | H | read-only historical view check only (renders::check, p0-journal renders); no JSON writer exists (25.10., F4) |
| runtime/src/analytics/journal.rs | 24 | H | legacy_value of historical schema-1 frames; export_views is #[cfg(test)] (25.10., F4) |
| runtime/src/analytics/measurement.rs | 11 | H | read-only historical view check only (renders::check, p0-journal renders); no JSON writer exists (25.10., F4) |
| runtime/src/analytics/payload.rs | 1 | H | read-only historical view check only (renders::check, p0-journal renders); no JSON writer exists (25.10., F4) |
| runtime/src/analytics/renders.rs | 13 | H | read-only historical view check only (renders::check, p0-journal renders); no JSON writer exists (25.10., F4) |
| runtime/src/bin/p0-fe-qualify.rs | 2 | W | getMultipleAccounts wire params only (JSON config input removed 25.10., F6) |
| runtime/src/bin/p0-parity.rs | 8 | H | DEC-08: + the historical INDEX/PAYLOAD source reads (parity gate) |
| runtime/src/bin/p0-probe-costs.rs | 12 | WR |  |
| runtime/src/execution/one_shot/approval_file.rs | 2 | H | DEC-10: historical JSON approval shown by the historical build only, never authorizing |
| runtime/src/execution/one_shot/non_landing.rs | 13 | W | DEC-03: B/C non-landing reads (`getBlockHeight`, `minimumLedgerSlot`, `getSignatureStatuses`, `getTransaction`, `getBlocksWithLimit`, `getBlock`) on the deadline-bound JSON-RPC wire |
| runtime/src/execution/one_shot.rs | 7 | H | <wallet>.json decoded only by the explicit migration (p0ctl fe04 migrate); open refuses it (F8, 25.10.) |
| runtime/src/market/deployment.rs | 1 | W |  |
| runtime/src/market/observer.rs | 1 | W |  |
| runtime/src/market/rows.rs | 34 | H | read-only historical view check only (renders::check, p0-journal renders); no JSON writer exists (25.10., F4) |
| runtime/src/market/summary.rs | 4 | H | read-only historical view check only (renders::check, p0-journal renders); no JSON writer exists (25.10., F4) |
| runtime/src/market/winner.rs | 8 | W | DEC-08: the signature page as raw JSON-RPC into the one wire decoder |
| solana/src/state/blockhash.rs | 2 | W | DEC-13b: getLatestBlockhash params with its answering slot |
| runtime/src/market_gap.rs | 3 | W |  |
| runtime/src/market_reconcile.rs | 9 | W | wire_params/wire_answer: the one JSON-RPC edge of verifier and reconcile (DEC-08: + getSignaturesForAddress/getTransaction); engine and strict cache typed, verifier file JSON-free (F10, 25.10.) |
| runtime/src/market_truth.rs | 2 | H | read-only historical view check only (renders::check, p0-journal renders); no JSON writer exists (25.10., F4) |
| runtime/src/orchestration/boot_network.rs | 12 | W |  |
| runtime/src/orchestration/fe_replay.rs | 3 | H |  |
| runtime/src/orchestration/provider_evidence.rs | 1 | C | measured operator file (F13) |
| solana/examples/p0-gap-ws-proxy.rs | 10 | W |  |
| solana/src/bin/p0-probe-v1.rs | 16 | WR |  |
| solana/src/chain.rs | 41 | WH | wire decode (DEC-08: + getSignaturesForAddress page); encode_* only for the read-only historical view check (25.10., F4) |
| solana/src/ingress/mod.rs | 5 | W |  |
| solana/src/ingress/ws_rpc.rs | 14 | W |  |
| solana/src/provider.rs | 13 | W | V2-05: DeadlineRpc::call_as (JSON-RPC wire params/result) |
| solana/src/state/snapshot.rs | 4 | W |  |
| solana/src/submit/jito.rs | 18 | W | DEC-02: `context.slot` of `simulateBundle` |
| solana/src/submit/rpc.rs | 11 | W | DEC-12: `sendTransaction` params/result on the deadline-bound JSON-RPC wire |

## Execution-path assertions (A10)

- `test_view_renders_reach_only_the_read_only_check`: the historical renders are reached only by the read-only check (`renders.rs`, `p0-journal renders`, the strict driver's derived-path exclusion); `payload::legacy_view` only from `renders.rs`; the JSONL view builder is `#[cfg(test)]`.
- `test_productive_controllers_and_owners_reach_no_json`: the analytics job controller, the post-capture chain, the runtime start, every state-owner module, source control, the status tap and the reporter's analytics projection reference neither `serde_json`, `json!(`, the renders nor `journal::export` outside their test modules.
- `a08_post_capture_job_runs_the_production_chain` (Rust): the native post-capture chain leaves no `*.json`/`*.jsonl` in the run; the native backtest test asserts the same for its results.
- F-16 `test_owner_ingress_and_provider_print_through_the_console`: no direct stderr in the owner, the runtime start, the WSS ingress or the provider.

Watched (no violation): `JsonlSink` (`market/sink.rs`) is `#[cfg(test)]`; `p0-strict-verify` has no `--compat` path (A05).
