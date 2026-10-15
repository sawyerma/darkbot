# Run storage

Where new measurement runs are stored and how a run directory is created.
Status: structure iterations 8b.2/8b.3 (**PENDING_INDEPENDENT_REVIEW**). New econ4h / C0-E1 runs go to `data/runs/<run_id>/`
in run layout V2 by default; historical runs stay where they are and remain readable in layout V1
(see [run layout versioning](../architecture/run_layout_versioning.md)).

## Contract

Implementation: [`scripts/run_store.py`](../../scripts/run_store.py), used by `run_econ4h.sh` (and therefore `test.sh`),
`run_gap_reconnect_smoke.sh` and `run_market_observer.sh` to create runs, and by `verify.sh`, `report.sh` and every pinned
reader to find runs and resolve their files.

| `P0_RUN_STORE` | econ4h / C0-E1 runs | C0-MO market runs |
|---|---|---|
| unset (default) | `data/runs/<run_id>/`, layout **V2** | `data/market/<run_id>/`, layout V1 |
| `runs` | `data/runs/<run_id>/`, layout V2 | `data/runs/<run_id>/`, layout V1 |
| `legacy` | `data/econ4h/<run_id>/`, layout V1 | `data/market/<run_id>/`, layout V1 |

- **Explicit roots keep their legacy semantics.** `P0_ECON_OUT_DIR` / `P0_MARKET_OUT_DIR` still select another directory, e.g.
  another volume, and write layout V1 there. An explicit `data/runs` selects the runs store. With `P0_RUN_STORE=runs` an
  explicit root must be `data/runs`, otherwise the launcher stops with `RUN_ROOT_CONFLICT`.
- **Standalone C0-MO stays V1:** it writes no pre-capture manifest, which layout V2 requires. Its Rust runtime rejects
  `P0_RUN_LAYOUT=v2`.
- **Shadow mainnet (`run_shadow_mainnet.sh`) and `c0_census.py` are not runs:** they append to `data/shadow/` or write `data/c0/`
  and keep that semantics.
- **Run id:** 1–128 characters of `A-Z a-z 0-9 _ -`. Path separators and dots are impossible, so an id cannot leave its root.
  The Rust runtime validates the same character set (`econ.rs`, `market_observer.rs`).
- **Created exactly once:** the run directory is created atomically. An id that already exists in the target root or in any of
  `data/econ4h`, `data/market`, `data/runs` is rejected with `RUN_ID_COLLISION`; an existing run is never reused or overwritten.
  Allocations are serialized by an exclusive lock on the `data/` directory itself (no lock file). A V2 allocation also creates
  `capture/`, `snapshots/` and `logs/`.
- **Fail closed before any build or network access:** `RUN_STORE_INVALID`, `RUN_ROOT_CONFLICT`, `RUN_ID_INVALID`, `RUN_ID_COLLISION`,
  `RUN_STORE_IO`; exit code 2. `run_gap_reconnect_smoke.sh` checks the id before it starts its proxy.
- **Finding runs:** every entry point (`verify.sh`, `report.sh`, and direct calls of `measurement_verify.py`, `market_report.py`,
  `audit_market_quick.py`) resolves `--run ID` by exact id across `data/econ4h`, `data/market` and `data/runs`, or takes an explicit
  directory. An id present in more than one root stops with `RUN_NOT_FOUND_OR_AMBIGUOUS`; an empty value is rejected.
- **Layout check:** V1 and V2 runs are read; a mixed, incomplete or malformed run stops with `RUN_LAYOUT_INVALID`, an unknown
  layout version with `RUN_LAYOUT_UNSUPPORTED`. Nothing is written to a rejected run.
- **Git:** `/data/runs/` is ignored by [`project_0/.gitignore`](../../.gitignore). `docs/evidence/`, `config/` and `exports/` are not affected.

## Layout V2 run directory

```text
data/runs/<run_id>/
├── manifest.json        Rust pre-capture manifest (C0-E1/MeasurementManifest/3, layout Project0/RunLayout/2, plan, provenance)
├── config.json          launcher: measurement plan, measurement/operational config, verification/storage preflight, candidate manifest
├── capture/             phase journal, shadow evaluations/offsets, opportunities, daily summary, mt0, market/ streams and summary
├── snapshots/           hour_01..04.json, economics_final.json, opportunity_economics.jsonl, canonical_market_shadow.jsonl
├── verification/        capture_evidence_manifest.json, <verification_spec_id>/, index_v3/
├── reports/             dataset_<key>/, query_<id>/, post_capture_report.<format>, post_capture_result.json
└── logs/                build.log, console_summary.txt, runtime.log, gap_proxy.log
```

The complete artifact map is `ARTIFACTS` in `run_store.py` and `RunPaths` in `native/crates/darkbot-runner/src/run_layout.rs`;
tests pin both tables to the same literal paths.

## Usage

```bash
project_0/scripts/test.sh --duration 10m                          # new run in data/runs/<run_id>, layout V2
P0_RUN_STORE=legacy project_0/scripts/test.sh --duration 10m      # explicit legacy store, layout V1
project_0/scripts/run_market_observer.sh                          # C0-MO: data/market, layout V1
project_0/scripts/verify.sh --run <run_id> --mode index
project_0/scripts/report.sh --run <run_id>
project_0/scripts/verify.sh --run project_0/data/runs/<run_id> --mode index   # explicit paths keep working
python3 project_0/scripts/run_store.py locate <run_id>                        # prints the run directory
python3 project_0/scripts/run_store.py path <run_dir> economics_final         # resolves an artifact by layout
```

## Identity

- `capture_run_id` comes from the run's manifest (`measurement_manifest.json` in V1, `manifest.json` in V2), or for legacy V1
  runs without a manifest from the directory name and the summary hash. It does not depend on the root or the layout.
- `run_store.py` is hash-pinned together with `measurement_contract.py`, `audit_market_quick.py` and `market_report.py`;
  identity versions and the resume rules are in [run layout versioning](../architecture/run_layout_versioning.md).

## Run path inventory (2026-10-15)

| Component | Writes / reads | Default | Override |
|---|---|---|---|
| Rust econ4h runtime (`econ.rs`, `econ_canonical.rs`) | shadow evaluations/offsets (capture), hourly snapshots, final economics, opportunity economics, canonical join (snapshots) via `RunPaths` | `project_0/data/econ4h/<id>`, layout V1 when started without the launcher | `P0_ECON_OUT_DIR`, `P0_ECON_RUN_ID`, `P0_RUN_LAYOUT` |
| Rust MeasurementPlan/manifest (`measurement.rs`) | manifest and phase journal by layout | — | `P0_MEASUREMENT_PLAN` (plan file) |
| Rust C0-MO (`market_observer.rs`) | `transactions.jsonl`, `liquidations.jsonl`, `coverage.jsonl`, `summary.json`, `final_report.txt` | `project_0/data/market/<id>`; inside econ4h `<run>/market/` (V1) or `<run>/capture/market/` (V2) | `P0_MARKET_OUT_DIR`, `P0_MARKET_RUN_ID` |
| Rust Shadow records (`main.rs`) | `opportunities.jsonl`, `daily_summary.jsonl`, `mt0.jsonl`, `operational_config_<ns>.json` | `project_0/data/shadow/`; econ4h: run directory (V1) or `capture/` (V2) | `P0_RECORD_PATH`, `P0_SUMMARY_PATH`, `P0_MT0_PATH` |
| `run_econ4h.sh` | allocates the run, selects the layout; logs, plan, storage preflight, candidate manifest; V2 assembles `config.json` from a temporary staging directory before the capture starts | via `run_store.py` | as above |
| `run_gap_reconnect_smoke.sh` | delegates to `run_econ4h.sh`; adds `gap_proxy.log`; PASS checks read through `run_store` | via `run_store.py` | as above |
| `run_market_observer.sh` | allocates the C0-MO run (always V1) | via `run_store.py` | as above |
| `measurement_cli.py` (prepare/finish) | measurement/operational config and verification preflight (V1 run root, V2 staging for `config.json`); `post_capture_*` (V1 run root, V2 `reports/`) | run path from the launcher | — |
| `verify.sh`, `report.sh` | locate `--run` through `run_store.py` | all three roots | explicit path |
| `measurement_verify.py` | `<run>/verification/<verification_spec_id>/` | `--run` | — |
| `audit_market_quick.py` | `<run>/verification/index_v3/` | `--run` | `--output` (API) |
| `market_report.py` | `<run>/reports/` | `--run` | — |
| `run_shadow_mainnet.sh` | appends to `data/shadow/*.jsonl`; not a run directory | unchanged | as above |
| `c0_census.py` | `data/c0/`; not a run directory | unchanged | `--out` |
| `p0-fe-qualify` | separate new evidence directory | — | `--output` |
