# DarkBot — Python Phase-C Retirement / Clean Native Rebuild

Status: **NORMATIVE — DO NOT MERGE THE PASS-6 PYTHON BSC RUNTIME**
Updated: 2026-09-27

## 1. Decision

The Pass-6 Python Phase-C ZIP is retained only as external engineering evidence.

It MUST NOT be merged wholesale into production `main`.

The current GitHub `main` still contains the clean Phase-A multichain seam and does not contain the Pass-6 Python EVM/BSC runtime. This is the desired starting point.

The BSC runtime is rebuilt natively from this clean baseline.

## 2. Exact Pass-6 ZIP content that must NOT enter production

Do not merge these Python BSC/EVM runtime files:

```
darkbot/chains/bsc/__init__.py
darkbot/chains/bsc/capture.py
darkbot/chains/bsc/chain.py
darkbot/chains/bsc/dispatch.py
darkbot/chains/bsc/feeds.py
darkbot/chains/bsc/units.py

darkbot/chains/evm/__init__.py
darkbot/chains/evm/blocks.py
darkbot/chains/evm/domain.py
darkbot/chains/evm/gas.py
darkbot/chains/evm/hexint.py
darkbot/chains/evm/mempool.py
darkbot/chains/evm/nonce.py
darkbot/chains/evm/receipts.py
darkbot/chains/evm/rpc.py
darkbot/chains/evm/websocket.py
```

Do not merge this Phase-C database schema:

```
migrations/0010_evm_bsc_capture.sql
```

Do not merge the Phase-C BSC/EVM Python tests as production tests:

```
tests/evm_helpers.py
tests/test_bsc_capture_hardening.py
tests/test_bsc_capture_hardening2.py
tests/test_bsc_capture_hardening3.py
tests/test_bsc_capture_hardening4.py
tests/test_bsc_capture_hardening5.py
tests/test_bsc_capture_hardening6.py
tests/test_bsc_capture_integrated.py
tests/test_bsc_chain.py
tests/test_evm_core.py
```

Do not merge the Phase-C BSC Python benchmarks:

```
tools/validation/bench_bsc_capture.py
tools/validation/bench_bsc_hotpath_stress.py
```

Do not add Phase-C validation output to the production repository:

```
exports/phase_c_validation/**
```

The ZIP also modifies existing files. Those modifications must not be copied over the clean `main` baseline:

```
darkbot/chains/registry.py
darkbot/main.py
darkbot/storage.py
tests/test_chain_registry.py
```

The current GitHub versions of those files remain authoritative until native BSC integration explicitly changes them.

The ZIP additionally contains Phase-C preflight/validation-tool work:

```
tools/validation/compare_parity.py
tools/validation/bench_hotpath.py
tests/test_validation_tools.py
```

Those are not required by the native BSC runtime and are not to be imported merely because they were in Pass 6. If an equivalent validation capability is later needed, implement it deliberately in the appropriate current toolchain.

## 3. What is retained from Pass 6

Do not retain the implementation; retain the proven engineering requirements.

Port these invariants into Rust tests/specification:

- exact earliest-first-seen semantics;
- same-sender+nonce candidate competition;
- observation order does not prove replacement;
- canonical path independent from pending visibility;
- reorg/orphan/uncertain handling;
- restart/backfill/high-water semantics;
- receipt completeness and log-index uniqueness;
- immutable raw evidence identity;
- exact integer arithmetic;
- explicit evidence/provenance;
- bounded state and explicit overflow;
- no silent evidence loss;
- malformed/oversize calldata fail-closed;
- late-pending metadata must never rewrite final canonical resolution;
- capture/replay deterministic ordering.

The Pass-6 tests are therefore a requirements mine, not a runtime dependency.

Re-create the relevant counterexamples as native Rust fixtures/tests.

## 4. What from Phase A stays

Keep the clean generic seam already on `main`:

```
darkbot/chains/base.py
darkbot/chains/registry.py
darkbot/chains/solana/**
darkbot/venues/base.py
darkbot/venues/registry.py
darkbot/venues/pumpfun/**
darkbot/runtime/**
market-profile metadata
existing Solana/Pump.fun runtime
existing Strategy A/B/C evidence
```

BSC profile metadata may remain non-runnable until native BSC is wired.

Do not make the Python chain registry instantiate a Python BSC adapter.

## 5. Database policy

Do not introduce the old `evm_* ` SQLite tables as the primary BSC runtime/capture format.

The native BSC primary evidence format is:

```
versioned immutable binary journal
+ manifest
```

SQLite/Parquet are derived offline analysis products.

If generic cross-chain run metadata is needed later, add a small new schema change for generic run identity only. Do not resurrect the Phase-C EVM capture schema just to obtain generic columns.

## 6. No hidden compatibility layer

Forbidden:

- Python BSC compatibility adapter behind the Rust runtime;
- replay conversion through the old Python EVM classes;
- live fallback from native ingest to old Python WSS ingest;
- old `evm_* ` database as a required replay source;
- duplicate Rust/Python Strategy C;
- duplicate Rust/Python venue decoders.

There is one BSC runtime source of truth: Rust.

## 7. Historical evidence policy

The Pass-6 ZIP/report can be retained outside the production source tree or in immutable archival storage.

Its purpose is audit/reference only.

Do not import it into runtime modules, tests, migrations or package dependencies.

## 8. Acceptance

The retirement is complete when:

- production `main` contains no Python Phase-C EVM/BSC runtime;
- no Phase-C EVM migration is required by startup;
- native BSC work starts from the clean Phase-A seam;
- all relevant Phase-C counterexamples exist as Rust tests before the corresponding native subsystem is accepted.
