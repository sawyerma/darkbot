# Project 0 Python tests

Status: structure iterations 13–15 (`46be33c`), `operations` package imports since 16–20 (`d234913`); 107 tests (binary journal
completion 2026-10-16). New runs keep their primary data in binary journals (`.p0j`); tests read them only through the
real `p0-journal` tool, never by decoding the format in Python. Offline only: temporary synthetic runs and recorded historical data; no network, no mainnet, no signature.
Launcher tests run the real `capture.sh`, real `cargo` and the real release `p0-longtail` against closed local ports
(`127.0.0.1:1`) with a public-only key file; the binary fails closed at startup. Rust tests stay inline in
`native/crates/*/src` (`cargo test`).

## Run

```bash
python3 -m unittest discover -s project_0/tests -t project_0          # full suite, from the repository root
python3 -m unittest discover -s /abs/path/project_0/tests -t /abs/path/project_0   # from any working directory
python3 -m unittest discover -s project_0/tests/python -t project_0   # one layer: python | integration | regression
project_0/scripts/verify.sh --self-test                               # decoder unit tests (tests/python/test_audit_market_quick.py)
```

Integration tests need the release binaries `p0-measurement-plan` and `p0-longtail` under
`native/target/release/` (`cargo build --release --locked --workspace --features crate_health,txv1`); without them the
real-binary tests report `NOT_RUN` (skip). The launcher tests also need 8 GiB free (storage preflight of the real launcher).

Remaining test doubles (kept on purpose; the code under test is always the production code):

| Double | Where | Why it stays |
|---|---|---|
| `FakeRPC`, `opener=` transports | `fixtures/p0_market.py`, `test_audit_market_quick.py`, `test_operational_config.py` | stand-in for the mainnet RPC (network input); retry, budget and allowlist logic of the real `RPC` class runs |
| `patch.object(a.RPC,'call',side_effect=AssertionError)` | `test_measurement_contract.py`, `test_measurement_p6.py` | proves that no network call happens; not a replacement |
| fault injection (`atomic_json`, `write_snapshots`, `disk_usage` = 0) | `test_audit_market_quick.py`, `test_market_report.py`, `test_measurement_contract.py` | interrupted writes and a full disk cannot be produced deterministically otherwise |

## Layout

| Directory | Content |
|---|---|
| `python/` | unit tests: index decoder and verifier (`test_audit_market_quick.py`), FE preflight, measurement contract, run layout V1/V2 contract and identities (`test_run_layout.py`) |
| `integration/` | release binaries, launchers and CLI end to end: measurement plan / `capture.sh` (via `test.sh`), operational config bridge, run store and launchers incl. V1/V2 runs through `verify.sh`/`report.sh`, report normalization, P6 invariants |
| `regression/` | historical references: FE candidate evidence, preserved local runs resolve by id (names only, read-only), V1 parity with the archived pinned readers |
| `fixtures/` | shared helpers, not tests: `p0_paths.py` (paths, clean environment, puts `project_0/` on `sys.path` for the `operations` package), `p0_market.py` (synthetic transactions, fake RPC, temporary V1 run, report events), `p0_runs.py` (synthetic V1/V2 run directories) |

Historical raw evidence (`docs/evidence/AUDIT_20261005`, `R122`/`R124` JSON) is read in place and never copied into
`fixtures/`; Rust tests embed the same files with `include_str!`.

## Old locations (before iteration 13)

| Old | New |
|---|---|
| `scripts/test_audit_market_quick.py` | `tests/python/test_audit_market_quick.py`; helpers in `tests/fixtures/p0_market.py` |
| `scripts/test_measurement_contract.py`, `scripts/test_run_layout.py` | `tests/python/` |
| `scripts/test_measurement_plan.py`, `test_operational_config.py`, `test_run_store.py`, `test_market_report.py`, `test_measurement_p6.py` | `tests/integration/` |
| `scripts/test_fe_candidates.py` | `tests/regression/test_fe_candidates.py` |
| `test_run_store.Identity.test_historical_local_runs_still_resolve_by_id_read_only` | `tests/regression/test_historical_runs.py` |
| `test_run_layout.ArchivedParity` | `tests/regression/test_v1_archived_parity.py` |

Commands recorded in `docs/evidence/` and the measurement ledger name the old paths; they document what was run at the time
and are not rewritten.

## Runtime-Zielstruktur 2026-10-16

Rust-Tests liegen bei ihren Domains; die früheren Binary-Helper-Tests unter
`native/crates/runtime/src/orchestration/state_owner/tests.rs` laufen jetzt in derselben Runtime-Bibliothek.
Alle ursprünglichen Testnamen sind nach Modulpfad-Normalisierung erhalten (382/384 PASS, je 1 ignored).
Der Market-Sicherheitsscan prüft die neue modulare Observer-Implementierung und den frühen Market-Dispatch.
Echte Launcher-Tests decken zusätzlich relative `--measurement-config`-Pfade und URL-Credentials im Banner ab.
[Struktur-, Paritäts- und Buildnachweis](../docs/architecture/runtime_modularization_20261016.md).
