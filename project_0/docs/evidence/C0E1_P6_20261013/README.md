# P6 offline qualification — 2026-10-13

P5 base: `c092bcd9f7c27c0305203ef29a65214bef376e48` (pushed). P3/P4 reused.

## Executed gates

- 52 Python tests PASS. Eight P6 tests include 240 display combinations × 3 renderers; no raw, proof, normalized-record or stored-economics mutation. All four individually selectable related types, six markers/four classes covered across the suite.
- Workspace `crate_health,txv1`: 340 PASS / 0 FAIL / 1 pre-existing ignored; `crate_health`: 338 PASS / 0 FAIL / 1 pre-existing ignored. Includes bounded provider/reconnect/429, strict batch interruption/resume/parent/skip/cache/disk and existing economics/race/UNKNOWN regressions. These are local fixture tests, not provider qualification.
- All four release binaries built; changed-region Rust format, shell syntax and whitespace checks PASS. Existing warnings remain; no claim of warning-free full-tree formatting or Clippy.
- Synthetic strict finalized fixture → existing Rust economics → four MeasurementPlan snapshots/final PASS, six capital buckets present. Empty/inconsistent canonical event inputs yield UNKNOWN economics, never an invented zero.
- Off/resume stable capture/spec IDs, distinct attempt/result IDs and result-bound query IDs; failed live coverage unchanged. Crosscheck/off never canonical. Raw truncation, altered proof, altered batch file and altered snapshot fail closed, including repeated cached-report reads.
- Report validation improvements found during P6: verify immutable result/batch digests before serving cached normalization; verify snapshot hashes; reject economics event-input disagreement; retain second-precision boundary uncertainty.
- Timed Python suite: 3.55s wall, 47,742,976 bytes maximum RSS. This measures tiny test fixtures only, NOT the real multi-GB capture/report resource envelope.

## Open / NOT_RUN

The real failed five-hour run and its multi-GB journals were not replayed or changed. This is explicitly user-owned. No WSS/RPC provider measurement, archive qualification, Mainnet smoke, long capture, LIVE, submission or paid provider use. Historical 429/index gate remains OPEN. P6 offline PASS is not overall P6/provider PASS or C0-E1 Master PASS. Four-hour stability/profitability remains unproven.

## User handoff (commands not executed on the real run)

From repository root, set `RUN` to the existing run directory. The following report is entirely offline; it can be heavy on large files. It creates derived immutable outputs under the run's `reports/` directory and preserves raw files:

```bash
./project_0/scripts/report.sh --run "$RUN" --report-types all --report-markers all --report-related-events on --report-related-types all --report-provenance all --report-view summary --format json
```

The full unfiltered dataset path is printed on stderr as `dataset`; it is independent of display filters. Four `hour_01..04.json` and `economics_final.json` share the original MeasurementPlan boundaries. A five-hour plan has four 75-minute quarters; a four-hour plan has four hourly snapshots. Existing legacy reports/manifests are not rewritten.

For source/status-specific views, use `--report-provenance best|live|index|strict|shadow|all`, `--report-view summary|liquidations|winners|competition|economics`, `--report-from`, `--report-to`, `--report-min-net-usd`, and on/off switches `--report-winners`, `--report-competition`, `--report-economics`. Individual selectors accept `all,-NAME` or explicit comma-separated names. Table/JSON/CSV use identical selected records.

Verification requires a measured, recent FREE_ONLY qualification; missing provider evidence blocks it. Do not manufacture that file. Local preflight (does not itself measure the provider):

```bash
./project_0/scripts/verify.sh --run "$RUN" --mode strict --provider-qualification "$QUALIFICATION" --preflight
```

After real provider qualification and user decision, the existing verifier is invoked with the same arguments without `--preflight`; add `--resume-verification` for an existing identical spec. The new typed capture entrypoint is `test.sh --duration 4h --chain-verify strict --provider-qualification "$QUALIFICATION"`; this is NOT approval to start a capture now. `--chain-verify off` disables historical RPC and supplies no chain proof.

Real capture, technical qualification, historical verification, economics and Master gates remain separate. UNKNOWN costs/coverage are not zero or PASS.
