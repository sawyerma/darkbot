# delete/ — removed legacy files (structure iteration 30)

Files moved here have no active consumer in code, tests or binding documents. They are kept only so they can be
recovered; nothing here is executed, imported, built or tested. Delete this directory after review.

| Moved file | Reason | Replacement |
|---|---|---|
| `scripts/run_econ4h.sh` | forwarding wrapper | `scripts/capture.sh econ4h-launcher` (typed path: `scripts/capture.sh` / `scripts/test.sh`) |
| `scripts/run_market_observer.sh` | forwarding wrapper | `scripts/capture.sh market` |
| `scripts/run_shadow_mainnet.sh` | forwarding wrapper | `scripts/capture.sh shadow` |
| `scripts/run_gap_reconnect_smoke.sh` | forwarding wrapper | `scripts/smoke.sh` |
| `scripts/fe_readonly_preflight.sh` | forwarding wrapper | `scripts/preflight.sh` |
| `scripts/<module>.py` (10 files) and `scripts/_compat.py` | forwarding of old `python3 scripts/<module>.py` command lines | `python3 -m operations.<area>.<module>` or the file under `operations/` |
| `scripts/test_suite.py` | old test discovery command | `python3 -m unittest discover -s project_0/tests -t project_0` |
| `src/darkbot/__init__.py`, `src/darkbot/project_0/__init__.py` | unused Phase-1 package identity, no importer | — |
| `tools/.gitkeep` | empty reserved directory | — |

Commands recorded in `docs/evidence/` and `docs/MEASUREMENT_LEDGER.md` name these paths historically and are not rewritten.
`scripts/test.sh` stays: `docs/PHASENPLAN.md` binds `test.sh`/`verify.sh` as the operator CLI.

## Mock-ban cleanup (2026-10-15)

`mock_ban_20261015/removed_stubs_and_fake_tests.patch` holds the complete removal diff (working tree against the staged
iterations 30–32). `git apply --reverse` restores it. It is not applied, built or tested.

| Removed | Where | Reason | Replacement |
|---|---|---|---|
| `NoHealth` (always `HealthError::Unavailable`) | `native/crates/liquidation/src/health.rs` | stub; the binary requires `crate_health`, no remaining user | `health::crate_impl::CrateHealth` (marginfi crate) |
| `NoPlans` (`ReceivershipBuilder` without plans) | `native/crates/runtime/src/longtail/accounts.rs` | stub, only used in the never compiled `cfg(not(crate_health))` branch | real builder (`builder.rs`) |
| 12 `#[cfg(not(feature = "crate_health"))]` blocks | `native/crates/runtime/src/main.rs` | never compiled: `p0-longtail` has `required-features = ["crate_health"]` | the `crate_health` branches, unchanged |
| `test_launcher_end_to_end_legacy_and_runs_store_synthetic` | `tests/integration/test_run_store.py` | fake `p0-longtail`, fake `cargo`, disk mock | `test_launcher_end_to_end_real_binary_writes_layout_v1_and_v2` |
| `test_shadow_from_any_directory_resolves_manifest_outputs_and_signer_once` | same | fake `cargo` | `test_shadow_runs_the_real_binary_from_any_directory` |
| argv-echo readers in `ReaderCli.test_wrappers_hand_located_directories_to_the_readers` | same | fake readers | the same test with the real readers |
| fake capture binary, fake `cargo`, disk mock in `test_launcher_uses_persisted_plan_and_no_total_capture_timebox` | `tests/integration/test_measurement_plan.py` | fakes | the same test with real `cargo` and the real `p0-longtail` |
