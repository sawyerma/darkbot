# Data, cache and export policy

Status 2026-10-15, structure iterations 9 and 12, **PENDING_INDEPENDENT_REVIEW**. Defines where generated data lives, what is
evidence and what may be deleted. This policy deletes nothing; historical runs (about 75 GB under `data/econ4h/` and
`data/market/`) stay where they are. Run storage details: [run_storage.md](run_storage.md).

## 1. Classes

| Class | Location | Versioned | Reproducible | Deletion |
|---|---|---|---|---|
| **Qualified evidence** | `docs/evidence/<ID>/` | yes, immutable after qualification | no (it is the record) | never; add new evidence instead |
| **Raw run data** | `data/runs/<id>/{manifest.json,config.json,capture/,snapshots/,logs/}`; V1 runs in `data/econ4h/`, `data/market/`, `data/runs/` | no (ignored) | no | only by the owner, never by tooling; freeze needed parts into `docs/evidence/` first |
| **Verification artifacts** | `<run>/verification/<spec_id>/{spec.json,results/,attempts/}`, `<run>/verification/capture_evidence_manifest.json` (V2) or `<run>/capture_evidence_manifest.json` (V1) | no | results are immutable records | keep with the run; ids are referenced by reports |
| **Run-bound recoverable caches** | `<run>/verification/<spec_id>/index/{pages,transactions}/`, `<run>/verification/index_v3/`, `<run>/verification/<spec_id>/strict/cache/` | no | yes, by repeating the bounded RPC calls (costs FREE_ONLY quota) | only when no verifier runs and no resume of that spec is planned; they back resume and offline replay and count towards the disk budget |
| **Derived reports** | `<run>/reports/dataset_<key>/` (dataset, snapshots), `<run>/reports/query_<id>/`, V2 `reports/post_capture_*` | no | yes, from raw data and stored results | datasets may be rebuilt; `attempt_<uuid>/` work directories may be deleted when no report runs (the dataset keeps its hard-linked `all_records.jsonl`) |
| **Project cache** | `data/cache/<tool>/` (today: `data/cache/gap_proxy/<run_id>.<random>/` from `run_gap_reconnect_smoke.sh`) | no (`/data/cache/` ignored) | yes | any time no producer runs; probe-only proxy results there are diagnostics, copy them to `docs/evidence/` if they become a gate input |
| **Temporary data** | system `TMPDIR`: `p0-operational-*` (private config with endpoint keys, mode 0600), `p0-measurement-cli-*`, `p0-run-config.*` (V2 config staging), SQLite spill files | no | — | removed by their owner on exit; secret-bearing files never move into the project tree |
| **Shadow and census output** | `data/shadow/`, `data/c0/` (append/overwrite semantics of the legacy tools) | no (ignored) | no | owner only |
| **Exports** | `exports/` | eligible, not ignored | yes | free; non-authoritative unless frozen under `docs/evidence/` and referenced by the ledger |
| **Build and tool caches** | `native/target/`, `__pycache__/` | no (root `.gitignore`) | yes | any time (`cargo clean`); Python readers expect release binaries under `native/target/release/` |

Not moved on purpose: the caches inside a run are bound to the verification spec and resume identity, and the strict cache path
lives in strict-pinned Rust (`market_verification.rs`); moving them would change identities without benefit. Secret-bearing
temporary files must not persist in the project tree.

## 2. Writers and default paths

| Writer | Output | Default |
|---|---|---|
| econ4h capture (`run_econ4h.sh`, `test.sh`) | run directory | `data/runs/<id>` layout V2; `P0_RUN_STORE=legacy` → `data/econ4h/<id>` |
| C0-MO (`run_market_observer.sh`) | run directory | `data/market/<id>` layout V1 |
| `run_gap_reconnect_smoke.sh` | run via `run_econ4h.sh`, proxy working data | run store as above; `data/cache/gap_proxy/` |
| `verify.sh` / `measurement_verify.py` | verification artifacts | inside the run |
| `report.sh` / `market_report.py` | dataset and query records inside the run; rendered table/json/csv on stdout | redirect to `exports/` for files, e.g. `report.sh --run <id> --format csv > project_0/exports/<id>.csv` |
| `measurement_cli.py finish` | `post_capture_report.<format>`, `post_capture_result.json` | V2 `reports/`, V1 run root |
| `fe_candidate_report.py`, `fe_readonly_preflight.py`, `p0-fe-qualify` | evidence candidates | explicit `--output` (required) |
| `c0_census.py` | census snapshots | `data/c0` resolved from the script location (no longer from the working directory) |
| `run_shadow_mainnet.sh`, `run_r126_qualify.py`, `run_r127_qualify.py` | shadow records and qualification logs | `data/shadow/` |

## 3. Ignore boundaries

`project_0/.gitignore` ignores `/data/econ4h/`, `/data/market/`, `/data/shadow/`, `/data/c0/`, `/data/runs/`, `/data/cache/`,
the local integration directory and `/env`; the repository root ignores `.env*` (except `.env.example`), keys, `*.log`, SQLite
files and `target/`. Deliberately **not** ignored: `docs/evidence/`, `config/` (profiles, examples, public wallet identity),
`exports/`, `data/.gitkeep`. Tests pin these boundaries (`scripts/test_run_store.py` `GitIgnore`).

## 4. Reproducibility and immutability rules

- Evidence and verification results are written once (`create_new`, hard link, fsync) and validated against their digests on
  every read; readers never recompute stored identities.
- Freezing a run for a gate: copy the required files with `SHA256SUMS` into a new `docs/evidence/<ID>/`, reference it from the
  ledger, never edit an existing evidence directory.
- Deleting caches or derived reports never changes a capture identity; deleting raw run data or results does and is the owner's
  decision only.
