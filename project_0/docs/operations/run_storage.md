# Run storage

Where new measurement runs are stored and how a run directory is created.
Status: structure iteration 8b.2. The unified root `data/runs/` is **opt-in**; the legacy defaults are unchanged.
All runs use layout V1; layout V2 is defined but not yet written or read (see [run layout versioning](../architecture/run_layout_versioning.md)).

## Contract

Implementation: [`scripts/run_store.py`](../../scripts/run_store.py), used by `run_econ4h.sh` (and therefore `test.sh`),
`run_gap_reconnect_smoke.sh` and `run_market_observer.sh` to create runs, and by `verify.sh` and `report.sh` to find them.

| `P0_RUN_STORE` | econ4h / C0-E1 runs | C0-MO market runs |
|---|---|---|
| unset or `legacy` (default) | `data/econ4h/<run_id>/` | `data/market/<run_id>/` |
| `runs` | `data/runs/<run_id>/` | `data/runs/<run_id>/` |

- **Explicit roots keep working.** `P0_ECON_OUT_DIR` and `P0_MARKET_OUT_DIR` still select another directory, e.g. another volume.
  With `P0_RUN_STORE=runs` they must point to `data/runs`, otherwise the launcher stops with `RUN_ROOT_CONFLICT`.
- **Run id:** 1–128 characters of `A-Z a-z 0-9 _ -`. Path separators and dots are impossible, so an id cannot leave its root.
  The Rust runtime validates the same character set (`econ.rs`, `market_observer.rs`).
- **Created exactly once:** the run directory is created atomically. An id that already exists in the target root or in any of
  `data/econ4h`, `data/market`, `data/runs` is rejected with `RUN_ID_COLLISION`; an existing run is never reused or overwritten.
  Allocations are serialized by an exclusive lock on the `data/` directory itself (no lock file), so concurrent runs cannot claim the same id in two roots.
- **Fail closed before any build or network access:** `RUN_STORE_INVALID`, `RUN_ROOT_CONFLICT`, `RUN_ID_INVALID`, `RUN_ID_COLLISION`, `RUN_STORE_IO`; exit code 2.
  `run_gap_reconnect_smoke.sh` checks the id before it starts its proxy; `run_econ4h.sh` then allocates the directory.
- **Finding runs:** `verify.sh` and `report.sh` resolve `--run ID` (or `--run=ID`) by exact id across `data/econ4h`, `data/market` and
  `data/runs`, or take an explicit directory, and hand the pinned readers the directory. An id present in more than one root
  stops with `RUN_NOT_FOUND_OR_AMBIGUOUS`; an empty value is rejected.
- **Layout check:** only layout V1 reaches the readers. A V2 run stops with `RUN_LAYOUT_UNSUPPORTED`, a mixed or malformed
  run with `RUN_LAYOUT_INVALID`; nothing is written to it.
- **Git:** `/data/runs/` is ignored by [`project_0/.gitignore`](../../.gitignore). `docs/evidence/`, `config/` and `exports/` are not affected.

## Usage

```bash
P0_RUN_STORE=runs project_0/scripts/test.sh --duration 10m
P0_RUN_STORE=runs project_0/scripts/run_market_observer.sh
project_0/scripts/verify.sh --run <run_id> --mode index
project_0/scripts/report.sh --run <run_id>
project_0/scripts/verify.sh --run project_0/data/runs/<run_id> --mode index   # explicit paths keep working
python3 project_0/scripts/run_store.py locate <run_id>                        # prints the run directory
```

Calling `measurement_verify.py`, `market_report.py` or `audit_market_quick.py` directly with a bare id still searches only
`data/econ4h` and `data/market`, because their lookup lives in pinned code; use the shell entry points or a path.

## Identity

- `capture_run_id` comes from the run's `measurement_manifest.json`, or for legacy runs from the directory name and the
  summary hash. It does not depend on the root; identical runs under `data/econ4h` and `data/runs` get the same identity.
- Pinned code: `verification_spec_id` hashes `measurement_contract.py` and `audit_market_quick.py`; the index verifier
  checkpoint binds `audit_market_quick.py` (resume fails with `RESUME_IDENTITY_MISMATCH` after a change); report identities hash
  `market_report.py`, `audit_market_quick.py` and `measurement_contract.py`. Iterations 8b.1 and 8b.2 change none of these files;
  the full analysis is in [run layout versioning](../architecture/run_layout_versioning.md).

## Run path inventory (2026-10-15)

| Component | Writes / reads | Default | Override |
|---|---|---|---|
| Rust econ4h runtime (`econ.rs`) | run directory: `shadow_evaluations.jsonl`, `shadow_offsets.bin`, `hour_01..04.json`, `economics_final.json`, `opportunity_economics.jsonl` | `project_0/data/econ4h/<id>` | `P0_ECON_OUT_DIR`, `P0_ECON_RUN_ID` |
| Rust MeasurementPlan/manifest (`measurement.rs`) | `measurement_manifest.json`, `measurement_phases.jsonl` in the run directory | — | `P0_MEASUREMENT_PLAN` (plan file) |
| Rust C0-MO (`market_observer.rs`) | `transactions.jsonl`, `liquidations.jsonl`, `coverage.jsonl`, `summary.json`, `final_report.txt` | `project_0/data/market/<id>`; inside econ4h `<run>/market/` | `P0_MARKET_OUT_DIR`, `P0_MARKET_RUN_ID` |
| Rust Shadow records (`main.rs`) | `opportunities.jsonl`, `daily_summary.jsonl`, `mt0.jsonl` | `project_0/data/shadow/`; econ4h: run directory | `P0_RECORD_PATH`, `P0_SUMMARY_PATH`, `P0_MT0_PATH` |
| `run_econ4h.sh` | allocates the run; `build.log`, `measurement_plan.json`, `storage_preflight.json`, `candidate_manifest.json`, `console_summary.txt`, `runtime.log` | via `run_store.py` | as above |
| `run_gap_reconnect_smoke.sh` | delegates to `run_econ4h.sh`; adds `gap_proxy.log` | via `run_store.py` | as above |
| `run_market_observer.sh` | allocates the C0-MO run | via `run_store.py` | as above |
| `measurement_cli.py` (prepare/finish) | `measurement_config.json`, `operational_config.json`, `verification_preflight.json`, `post_capture_*` | run path from the launcher | — |
| `verify.sh`, `report.sh` | locate `--run` through `run_store.py` | all three roots | explicit path |
| `measurement_verify.py` | `<run>/verification/<verification_spec_id>/` | `--run` | — |
| `audit_market_quick.py` | `<run>/verification/index_v2/`; `find_run` id lookup (legacy roots, direct calls only) | `--run` | — |
| `market_report.py` | `<run>/reports/` | `--run` | — |
| `run_shadow_mainnet.sh` | appends to `data/shadow/*.jsonl`; not a run directory | unchanged | as above |
| `c0_census.py` | `data/c0/`; not a run directory | unchanged | `--out` |
| `p0-fe-qualify` | separate new evidence directory | — | `--output` |

## Next steps

Target layout V2: `data/runs/<run_id>/{manifest.json, config.json, capture/, snapshots/, verification/, reports/, logs/}`.
Runs in `data/runs` currently use layout V1; `verification/` and `reports/` already match the target.

- **V2 writers and readers:** need the re-pin decision in [run layout versioning](../architecture/run_layout_versioning.md), section 4.
- **8b.3:** make `runs` the default store once qualified; historical runs stay where they are.
