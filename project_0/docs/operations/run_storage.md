# Run storage

Where new measurement runs are stored and how a run directory is created.
Status: structure iterations 8b.2/8b.3 (`46be33c`; entry points renamed in 21–25). New econ4h / C0-E1 runs go to `data/runs/<run_id>/`
in run layout V2 by default; historical runs stay where they are and remain readable in layout V1
(see [run layout versioning](../architecture/run_layout_versioning.md)).

## Contract

Implementation: [`operations/capture/run_store.py`](../../operations/capture/run_store.py), used by `scripts/capture.sh`
(econ4h and market) and `scripts/smoke.sh` to create runs, and by `verify.sh`, `report.sh` and every pinned reader to find runs
and resolve their files. `scripts/test.sh` (PHASENPLAN operator command) is `capture.sh econ4h`; the former launcher names
were removed (25.10., P0-PROD-CTRL-09).

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
- **Shadow mainnet (`capture.sh shadow` → `p0ctl run start --mode shadow`, F-01) and `operations/capture/c0_census.py` are not V1/V2 runs:** a Shadow run gets
  `data/shadow/<P0_SHADOW_RUN_ID>/` (created once, with its `measurement_plan.p0j`); the census writes `data/c0/`.
- **Run id:** 1–128 characters of `A-Z a-z 0-9 _ -`. Path separators and dots are impossible, so an id cannot leave its root.
  The Rust runtime validates the same character set (`econ.rs`, `market_observer.rs`).
- **Created exactly once:** the run directory is created atomically. An id that already exists in the target root or in any of
  `data/econ4h`, `data/market`, `data/runs` is rejected with `RUN_ID_COLLISION`; an existing run is never reused or overwritten.
  Allocations are serialized by an exclusive lock on the `data/` directory itself (no lock file). A V2 allocation also creates
  `capture/`, `snapshots/` and `logs/`.
- **Fail closed before any build or network access:** `RUN_STORE_INVALID`, `RUN_ROOT_CONFLICT`, `RUN_ID_INVALID`, `RUN_ID_COLLISION`,
  `RUN_STORE_IO`; exit code 2. `smoke.sh` checks the id before it starts its proxy.
- **Finding runs:** every entry point (`verify.sh`, `report.sh`, and direct calls of `operations/verification/measurement_verify.py`,
  `operations/reporting/market_report.py`, `operations/verification/audit_market_quick.py`) resolves `--run ID` by exact id across `data/econ4h`, `data/market` and `data/runs`, or takes an explicit
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

The complete artifact map is `ARTIFACTS` in `operations/capture/run_store.py` and `RunPaths` in `native/crates/runtime/src/run_layout.rs`;
tests pin both tables to the same literal paths.

## Usage

```bash
project_0/scripts/capture.sh --duration 10m                          # new run in data/runs/<run_id>, layout V2
P0_RUN_STORE=legacy project_0/scripts/capture.sh --duration 10m      # explicit legacy store, layout V1
project_0/scripts/capture.sh market                          # C0-MO: data/market, layout V1
project_0/scripts/verify.sh --run <run_id> --mode index
project_0/scripts/report.sh --run <run_id>
project_0/scripts/verify.sh --run project_0/data/runs/<run_id> --mode index   # explicit paths keep working
python3 project_0/operations/capture/run_store.py locate <run_id>             # prints the run directory
python3 project_0/operations/capture/run_store.py path <run_dir> economics_final   # resolves an artifact by layout
```

## Identity

- `capture_run_id` comes from the run's manifest (`measurement_manifest.json` in V1, `manifest.json` in V2), or for legacy V1
  runs without a manifest from the directory name and the summary hash. It does not depend on the root or the layout.
- `run_store.py` is hash-pinned together with `measurement_contract.py`, `audit_market_quick.py` and `market_report.py` (all under `operations/`);
  identity versions and the resume rules are in [run layout versioning](../architecture/run_layout_versioning.md).

## Run path inventory (2026-10-15)

| Component | Writes / reads | Default | Override |
|---|---|---|---|
| Rust econ4h runtime (`econ.rs`, `econ_canonical.rs`) | `shadow_evaluations.p0j` (capture); `economics_reports.p0j` plus its renders `hour_0N.json`, `economics_final.json`, `opportunity_economics.jsonl`, `canonical_market_shadow.jsonl` (snapshots) via `RunPaths` | `project_0/data/econ4h/<id>`, layout V1 when started without the launcher | `P0_ECON_OUT_DIR`, `P0_ECON_RUN_ID`, `P0_RUN_LAYOUT` |
| Rust MeasurementPlan/manifest (`measurement.rs`) | manifest (JSON document) and `measurement_phases.p0j`; `measurement_phases.jsonl` is rendered when the run ends | — | `P0_MEASUREMENT_PLAN` (plan file) |
| Rust C0-MO (`market/`) | `market.p0j`; renders `transactions.jsonl`, `liquidations.jsonl`, `coverage.jsonl`; `summary.json` (with `evidence_journal` binding), `final_report.txt` | `project_0/data/market/<id>`; inside econ4h `<run>/market/` (V1) or `<run>/capture/market/` (V2) | `P0_MARKET_OUT_DIR`, `P0_MARKET_RUN_ID` |
| Rust Shadow owner (`orchestration/state_owner`) | `opportunities.p0j` (records, opportunities, reconcile rows, daily summary, owner diagnostics), `<record>.mt0.p0j`, `operational_config_<ns>.json` | `capture.sh shadow`: `project_0/data/shadow/<P0_SHADOW_RUN_ID>/`; econ4h: run directory (V1) or `capture/` (V2) | `P0_RECORD_PATH`, `P0_MT0_PATH` (`P0_SUMMARY_PATH` is no longer read) |
| `capture.sh` (econ4h-launcher) | allocates the run, selects the layout; logs, plan, storage preflight, candidate manifest; V2 assembles `config.json` from a temporary staging directory before the capture starts | via `run_store.py` | as above |
| `smoke.sh` | delegates to `capture.sh econ4h-launcher`; adds `gap_proxy.log`; PASS checks read through `run_store` | via `run_store.py` | as above |
| `capture.sh market` | allocates the C0-MO run (always V1) | via `run_store.py` | as above |
| `operations/cli/measurement_cli.py` (prepare/finish) | measurement/operational config and verification preflight (V1 run root, V2 staging for `config.json`); `post_capture_*` (V1 run root, V2 `reports/`) | run path from the launcher | — |
| `verify.sh`, `report.sh` | locate `--run` through `operations/capture/run_store.py` | all three roots | explicit path |
| `operations/verification/measurement_verify.py` | `<run>/verification/<verification_spec_id>/` | `--run` | — |
| `operations/verification/audit_market_quick.py` | `<run>/verification/index_v3/` | `--run` | `--output` (API) |
| `operations/reporting/market_report.py` | `<run>/reports/` | `--run` | — |
| `capture.sh shadow` | one new directory per run under `data/shadow/`; journals are created exclusively, never appended across runs | as above | as above |
| `operations/capture/c0_census.py` | `data/c0/`: census journals `accounts_slot<N>.p0j`, `group_accounts_slot<N>.p0j` (via `p0-journal import`) and the report JSON | unchanged | `--out` |
| `p0-fe-qualify` | separate new evidence directory: `fe01.p0j` (config, every read, result), `operational_config.json`, `result.json` | — | `--output` |
| FE-04 one-shot journal (`execution/one_shot.rs`) | `<wallet>.p0j` (single-flight automaton); a pre-binary `<wallet>.json` is read, bound by SHA-256 and never rewritten | caller's directory | — |

## Binary journals (PERSIST-01 to PERSIST-04, 2026-10-16)

New runtime events are written to the one binary journal contract (`native/crates/runtime/src/analytics/journal.rs`,
magic `P0JRNL01`): little-endian frames with kind, schema, journal and source sequence, provenance (source, slot, fork,
receive and wall-clock time, each explicitly UNKNOWN when absent), lossless integers up to i128/u128, CRC32, a SHA-256
chain over all frames and a sealing manifest frame (`COMPLETE` or `INCOMPLETE`). Event kinds are listed in `journal::kind`.

| Journal | Kinds | Written by | Legacy view (explicit export, rendered from the journal) |
|---|---|---|---|
| `opportunities.p0j` | 100–104 | Shadow owner evidence writer (off the owner thread) | — |
| `<record>.mt0.p0j` or `capture/mt0.p0j` | 600 | MT0 reporter thread | — |
| `shadow_evaluations.p0j` | 400 | econ analytics worker | — (replay reads the journal) |
| `economics_reports.p0j` | 410–413 | econ analytics worker | `hour_0N.json`, `economics_final.json`, `opportunity_economics.jsonl`, `canonical_market_shadow.jsonl` |
| `measurement_phases.p0j` | 500 | supervisor, market and analytics threads | `measurement_phases.jsonl` |
| `market.p0j` | 300–302 | C0-MO capture thread | `transactions.jsonl`, `liquidations.jsonl`, `coverage.jsonl` |
| `fe01.p0j` | 700–702 | `p0-fe-qualify --execute` | — (`--replay` reads the journal; old `read_NNNNNN.json` directories stay readable) |
| `<wallet>.p0j` | 200–201 | FE-04 one-shot journal | — |
| `accounts_slot<N>.p0j`, `group_accounts_slot<N>.p0j` | 900–901 | `c0_census.py` through `p0-journal import` | — |

The views exist only because the hash-pinned readers (`measurement_contract.py`, `audit_market_quick.py`,
`market_report.py`, `run_store.py`, the strict Rust verifier) read these paths; changing them would change bound
verification and report identities. A view is created only when missing, never overwritten, and must equal its render
byte for byte. `verify.sh` runs `p0-journal renders <run> --create` before the capture is hashed: missing views (for example
after a killed capture) are rendered, a differing view fails closed with `JOURNAL_VIEWS_INVALID`. Runs without journals are
left as they are.

`native/target/release/p0-journal` (built by `capture.sh`):

```bash
p0-journal verify JOURNAL               # magic, version, every CRC, the chain and the manifest; prints status and root
p0-journal frames JOURNAL               # every frame with kind and provenance as JSONL (explicit offline export)
p0-journal export JOURNAL DEST [--kind N]
p0-journal renders RUN [--create]       # check (and create missing) legacy views of a run
p0-journal import KIND JOURNAL < rows.jsonl   # census kinds only
```

Still JSON by design (PERSIST-04, single documents or external contracts, not event streams): manifests, `config.json`,
`summary.json`, `final_report.txt`, `operational_config*.json`, `result.json`, verification and report outputs of the
pinned readers, and the strict verifier's batch files (`market_verification.rs` is hash-pinned; PARTIAL until a re-pin
decision).
