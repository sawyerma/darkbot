# Project 0 Python tests

Status: structure iterations 13–15, **PENDING_INDEPENDENT_REVIEW**. Offline only: temporary synthetic runs, fake RPC and fake
binaries; no network, no mainnet, no signer. Rust tests stay inline in `native/crates/*/src` (`cargo test`).

## Run

```bash
python3 -m unittest discover -s project_0/tests -t project_0          # full suite, from the repository root
python3 -m unittest discover -s /abs/path/project_0/tests -t /abs/path/project_0   # from any working directory
python3 -m unittest discover -s project_0/tests/python -t project_0   # one layer: python | integration | regression
project_0/scripts/verify.sh --self-test                               # decoder unit tests (tests/python/test_audit_market_quick.py)
python3 -m unittest discover -s project_0/scripts -p 'test_*.py'      # pre-reform command, kept by scripts/test_suite.py
```

Integration tests need the release binaries `p0-measurement-plan`, `p0-econ-replay` and `p0-longtail` under
`native/target/release/` (`cargo build --release --locked --workspace --features crate_health,txv1`).

## Layout

| Directory | Content |
|---|---|
| `python/` | unit tests: index decoder and verifier (`test_audit_market_quick.py`), FE preflight, measurement contract, run layout V1/V2 contract and identities (`test_run_layout.py`) |
| `integration/` | release binaries, launchers and CLI end to end: measurement plan / `test.sh`, operational config bridge, run store and launchers incl. V1/V2 runs through `verify.sh`/`report.sh`, report normalization, P6 invariants |
| `regression/` | historical references: FE candidate evidence, preserved local runs resolve by id (names only, read-only), V1 parity with the archived pinned readers |
| `fixtures/` | shared helpers, not tests: `p0_paths.py` (paths, clean environment, puts `scripts/` on `sys.path`), `p0_market.py` (synthetic transactions, fake RPC, temporary V1 run, report events), `p0_runs.py` (synthetic V1/V2 run directories) |

Historical raw evidence (`docs/evidence/AUDIT_20261005`, `R122`/`R124` JSON) is read in place and never copied into
`fixtures/`; Rust tests embed the same files with `include_str!`.

## Old locations (before iteration 13)

| Old | New |
|---|---|
| `scripts/test_audit_market_quick.py` | `tests/python/test_audit_market_quick.py`; helpers in `tests/fixtures/p0_market.py` |
| `scripts/test_fe_preflight.py`, `scripts/test_measurement_contract.py`, `scripts/test_run_layout.py` | `tests/python/` |
| `scripts/test_measurement_plan.py`, `test_operational_config.py`, `test_run_store.py`, `test_market_report.py`, `test_measurement_p6.py` | `tests/integration/` |
| `scripts/test_fe_candidates.py` | `tests/regression/test_fe_candidates.py` |
| `test_run_store.Identity.test_historical_local_runs_still_resolve_by_id_read_only` | `tests/regression/test_historical_runs.py` |
| `test_run_layout.ArchivedParity` | `tests/regression/test_v1_archived_parity.py` |

Commands recorded in `docs/evidence/` and the measurement ledger name the old paths; they document what was run at the time
and are not rewritten.
