# Audit findings captured in this bundle

## Verified

- Audited source commit: `799048f8fa95df4c29f2aa6b35bcc3615405fe19`.
- Workspace tests passed: 136 tests with `crate_health,txv1`; 134 tests with `crate_health`; zero failures. Raw outputs are `tests_txv1.txt` and `tests_health.txt`.
- The cache predicate was compiled and exercised with adversarial traces. It forwards class transitions and forced Bank re-arms, but a same-slot repeated `Liquidatable` evaluation is suppressed. With 1,025 persistent candidates and the checked-in pubkey sort plus soft-cap, the last candidate was starved for 100 simulated slots. See `cache_counterexamples.txt`.
- The Drift refresh discriminator and account order match the official Drift protocol and `0dotxyz/p0-ts-sdk`, but an unsigned read-only simulation against current Mainnet returned `InstructionFallbackNotFound` / `Custom(101)`. This is a live external/protocol-version blocker, not a successful-refresh proof. See `drift_dispatch.json`.
- Six recent real Marginfi liquidation transactions were retrieved and preserved with signatures, slots, timestamps, instruction decoding, fees, payer/receiver deltas, token deltas, and protocol flat-fee transfers. See `history_summary.json`, `history_analysis.json`, and the per-signature JSON files.
- Historical full realized USD PnL is deliberately `UNKNOWN`: standard RPC does not provide the liquidator's prestate, route execution valuation, or complete wallet attribution. No USD profit is invented.

## Important limitations

- The local checkout did not contain `project_0/.env`; the replay used the public Solana RPC. A private RPC command was therefore not run.
- The audit source snapshot covers the complete tracked repository inventory, with deep code review focused on Project 0. BSC, Pump.fun, and frontend source files are included in the inventory and archive, but their independent production correctness was not certified because they are outside Project 0's dependency boundary.
- A standalone winner/economics Rust probe failed before its assertions because the SDK deserializer expected a different JSON shape. The failure is retained verbatim; no result from that probe is used as evidence.
- No transaction was signed or sent. LIVE remains disabled in the audited runner.
