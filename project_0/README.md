# Project 0 — Repository Map

Solana / Marginfi Project 0 liquidation system.

This README is the **navigation and file-placement contract** for `project_0/`.
It answers two questions:

1. **Where is the authoritative information?**
2. **Where must new code, evidence, scripts and generated data be placed?**

For the current engineering state and the next technically necessary steps, start here:

> **[Current status — docs/STATUS.md](docs/STATUS.md)**

The 2026-10-05 audit handoff is historical: [docs/development_history/audit_handoff_20261005.md](docs/development_history/audit_handoff_20261005.md).

---

## 1. Start here

| Need | Canonical file / directory |
|---|---|
| Current status, open gates and next steps | **[docs/STATUS.md](docs/STATUS.md)** |
| Documentation navigation | [docs/README.md](docs/README.md) |
| Authoritative production roadmap / architecture | [docs/PHASENPLAN.md](docs/PHASENPLAN.md) |
| Measurements, gates and historical decisions | [docs/MEASUREMENT_LEDGER.md](docs/MEASUREMENT_LEDGER.md) |
| Development history (2026-10-05 audit handoff, audits, R127c/R127d changelogs and rollback) | [docs/development_history/](docs/development_history/) |
| Complete Astra audit findings | [docs/evidence/AUDIT_20261005/AUDIT_FINDINGS.md](docs/evidence/AUDIT_20261005/AUDIT_FINDINGS.md) |
| Exact audit commands and results | [docs/evidence/AUDIT_20261005/COMMANDS_AND_RESULTS.md](docs/evidence/AUDIT_20261005/COMMANDS_AND_RESULTS.md) |
| Complete preserved Astra audit bundle | [docs/evidence/AUDIT_20261005/darkbot_audit_bundle_20261005.zip](docs/evidence/AUDIT_20261005/darkbot_audit_bundle_20261005.zip) |
| Rust production workspace | [native/](native/) |
| Runtime orchestration / Mainnet runner | [native/crates/darkbot-runner/](native/crates/darkbot-runner/) |
| Marginfi / Project 0 domain logic | [native/crates/darkbot-project0/](native/crates/darkbot-project0/) |
| Solana ingress/state/transaction/submission infrastructure | [native/crates/darkbot-solana/](native/crates/darkbot-solana/) |
| Shared timing/config/lifecycle primitives | [native/crates/darkbot-common/](native/crates/darkbot-common/) |
| Qualification and Mainnet-shadow scripts | [scripts/](scripts/) |
| Versioned raw evidence | [docs/evidence/](docs/evidence/) |
| Public/non-secret config | [config/](config/) |
| Local/generated runtime data | [data/](data/) |
| Qualification / gate documentation | [docs/qualification/](docs/qualification/) |
| Project 0 frontend | [../frontend/src/features/project0/](../frontend/src/features/project0/) |

---

## 2. Canonical read order

When resuming Project 0 after a break, audit, model switch or handoff, read in this order:

```text
1. project_0/README.md
2. project_0/docs/STATUS.md
3. project_0/docs/MEASUREMENT_LEDGER.json, then the newest relevant MEASUREMENT_LEDGER.md entries
4. project_0/docs/PHASENPLAN.json, then the relevant PHASENPLAN.md sections
5. relevant immutable evidence under project_0/docs/evidence/
6. relevant production source under project_0/native/
7. relevant qualification script under project_0/scripts/
8. historical context when needed: project_0/docs/development_history/ and project_0/docs/evidence/AUDIT_20261005/
```

`docs/STATUS.md` summarizes; the newest dated ledger entries and the code remain canonical. Historical documents and measurements are never erased.

---

## 3. Fixed directory structure

```text
project_0/
├── README.md                         <- this navigation / placement contract
├── .env.example                     <- non-secret environment template only
│
├── docs/                             <- human-readable Project 0 documentation
│   ├── README.md                     <- documentation navigation
│   ├── STATUS.md                     <- CURRENT STATUS + NEXT STEPS
│   ├── PHASENPLAN.md                 <- authoritative production plan
│   ├── MEASUREMENT_LEDGER.md         <- measurements / gates / decisions
│   ├── architecture/                 <- upstream reuse decisions, parallel execution design
│   ├── operations/                   <- operational configuration: profiles, precedence, budgets
│   ├── qualification/                <- qualification / gate documentation index
│   ├── measurements/
│   │   ├── protocols/                <- P6-LT Shadow, C0-MO, C0-E1 econ4h protocols
│   │   └── historical_reports/       <- FIX-129 cost, C0-E1 implementation/qualification reports
│   ├── development_history/          <- 2026-10-05 audit handoff (historical), audits, work order,
│   │                                    R127c/R127d changelogs and rollback
│   └── evidence/                     <- IMMUTABLE versioned evidence
│       ├── AUDIT_20261005/           <- complete Astra audit corpus
│       ├── R114_*.json
│       ├── R116_*.json
│       ├── R117_*.json
│       ├── R118_*.json
│       ├── R119_*.json
│       ├── R120_*.json
│       ├── R121_*.json
│       ├── R122_*.json
│       ├── R124_*.json
│       ├── R125b_*.json
│       ├── R126b_*.json
│       ├── R127_*.json
│       ├── R127c_*.json
│       ├── R127d_*.json
│       └── other qualified raw evidence
│
├── native/                           <- PRODUCTION RUST WORKSPACE
│   ├── Cargo.toml
│   ├── Cargo.lock                    <- tracked reproducible dependency lock
│   └── crates/
│       ├── darkbot-common/           <- common runtime primitives
│       ├── darkbot-solana/           <- Solana transport/state/tx/submit
│       ├── darkbot-project0/          <- Project 0 protocol/domain logic
│       └── darkbot-runner/            <- production orchestration/runtime
│
├── scripts/                          <- reproducible operational/qualification scripts
│   ├── c0_census.py
│   ├── run_shadow_mainnet.sh
│   ├── run_r126_qualify.py
│   └── run_r127_qualify.py
│
├── config/                           <- PUBLIC / NON-SECRET versioned config
│   └── wallet.public.env
│
├── data/                             <- local/generated runtime datasets
├── exports/                          <- generated exports / reports
├── src/                              <- application/control boundary, not Rust production core
├── tests/                            <- non-Rust project-level test area
└── tools/                            <- reserved project-specific tooling area
```

### Structure rule

Do **not** create new top-level Project 0 directories without updating this README.

---

## 4. Production Rust map

The production implementation lives under **[native/](native/)**.

### 4.1 `darkbot-common` — shared infrastructure

Directory: [native/crates/darkbot-common/](native/crates/darkbot-common/)

| Area | File |
|---|---|
| bounded channels / overflow semantics | [channel.rs](native/crates/darkbot-common/src/channel.rs) |
| config parsing | [config.rs](native/crates/darkbot-common/src/config.rs) |
| typed errors / halt reasons | [error.rs](native/crates/darkbot-common/src/error.rs) |
| metrics histograms | [histogram.rs](native/crates/darkbot-common/src/histogram.rs) |
| journal / replay primitives | [journal.rs](native/crates/darkbot-common/src/journal.rs) |
| lifecycle / run state | [lifecycle.rs](native/crates/darkbot-common/src/lifecycle.rs) |
| timing vocabulary / timelines | [timing.rs](native/crates/darkbot-common/src/timing.rs) |

### 4.2 `darkbot-solana` — Solana infrastructure

Directory: [native/crates/darkbot-solana/](native/crates/darkbot-solana/)

| Area | Directory |
|---|---|
| event ingress / WSS / source handling | [src/ingress/](native/crates/darkbot-solana/src/ingress/) |
| canonical state / recovery | [src/state/](native/crates/darkbot-solana/src/state/) |
| transaction construction primitives | [src/tx/](native/crates/darkbot-solana/src/tx/) |
| RPC / Jito submission infrastructure | [src/submit/](native/crates/darkbot-solana/src/submit/) |
| probes / binaries | [src/bin/](native/crates/darkbot-solana/src/bin/) |

### 4.3 `darkbot-project0` — Project 0 protocol/domain logic

Directory: [native/crates/darkbot-project0/](native/crates/darkbot-project0/)

| Responsibility | File |
|---|---|
| exact health / Marginfi health integration | [health.rs](native/crates/darkbot-project0/src/health.rs) |
| Bank/Account reverse and threshold indices | [index.rs](native/crates/darkbot-project0/src/index.rs) |
| integration projection/refresh semantics | [integration.rs](native/crates/darkbot-project0/src/integration.rs) |
| state projection | [projection.rs](native/crates/darkbot-project0/src/projection.rs) |
| receivership planning | [planner.rs](native/crates/darkbot-project0/src/planner.rs) |
| receivership instructions / rules | [receivership.rs](native/crates/darkbot-project0/src/receivership.rs) |
| liquidation sizing | [sizing.rs](native/crates/darkbot-project0/src/sizing.rs) |
| deterministic economics / candidate ranking | [economics.rs](native/crates/darkbot-project0/src/economics.rs) |
| funding classification | [funding.rs](native/crates/darkbot-project0/src/funding.rs) |
| execution abstractions | [execution.rs](native/crates/darkbot-project0/src/execution.rs) |
| prepared candidate / execution state | [prepared.rs](native/crates/darkbot-project0/src/prepared.rs) |
| scheduling logic | [schedule.rs](native/crates/darkbot-project0/src/schedule.rs) |
| safety rules | [safety.rs](native/crates/darkbot-project0/src/safety.rs) |
| protocol constants / IDs | [protocol.rs](native/crates/darkbot-project0/src/protocol.rs) |

### 4.4 `darkbot-runner` — production orchestration

Directory: [native/crates/darkbot-runner/](native/crates/darkbot-runner/)

| Responsibility | File |
|---|---|
| main runtime / candidate cache / event loop | **[main.rs](native/crates/darkbot-runner/src/main.rs)** |
| receivership builder integration | [builder.rs](native/crates/darkbot-runner/src/builder.rs) |
| end-to-end pipeline helpers | [pipeline.rs](native/crates/darkbot-runner/src/pipeline.rs) |
| pending candidate handling | [pending.rs](native/crates/darkbot-runner/src/pending.rs) |
| Jupiter route integration | [route.rs](native/crates/darkbot-runner/src/route.rs) |
| LUT handling | [lookup_tables.rs](native/crates/darkbot-runner/src/lookup_tables.rs) |
| winner / competitor matching | [winner.rs](native/crates/darkbot-runner/src/winner.rs) |
| shadow evidence record model | [record.rs](native/crates/darkbot-runner/src/record.rs) |
| shadow ledger | [shadow_ledger.rs](native/crates/darkbot-runner/src/shadow_ledger.rs) |
| fixture support | [fixture.rs](native/crates/darkbot-runner/src/fixture.rs) |
| remote worker helpers | [remote_workers.rs](native/crates/darkbot-runner/src/remote_workers.rs) |
| runtime/probe binaries | [src/bin/](native/crates/darkbot-runner/src/bin/) |

---

## 5. Documentation map

Directory: [docs/](docs/)

### Current / authoritative

| File | Purpose |
|---|---|
| **[STATUS.md](docs/STATUS.md)** | Current code/qualification state, FE-00…FE-07, open gates, next steps, known contradictions. **Read first.** |
| [PHASENPLAN.md](docs/PHASENPLAN.md) | Long-term production architecture, phases and constraints. |
| [MEASUREMENT_LEDGER.md](docs/MEASUREMENT_LEDGER.md) | Historical measurements, PASS/FAIL/OPEN decisions and evidence references. |

### Architecture

Directory: [docs/architecture/](docs/architecture/)

| File | Purpose |
|---|---|
| [p0_reuse_matrix.md](docs/architecture/p0_reuse_matrix.md) | P0 reference study and upstream reuse/merge decisions (eva01, marginfi-v2). |
| [parallel_execution_plan.md](docs/architecture/parallel_execution_plan.md) | Parallel execution design and MT0–MT10 sequence (baseline 2026-10-06). |

### Operations

Directory: [docs/operations/](docs/operations/)

| File | Purpose |
|---|---|
| [operational_config.md](docs/operations/operational_config.md) | Operational configuration: inspection commands, precedence, profiles, budgets, authorization limits. |

### Qualification

Directory: [docs/qualification/](docs/qualification/)

| File | Purpose |
|---|---|
| [README.md](docs/qualification/README.md) | Qualification/gate documentation index: where gate definitions, results and raw evidence live. |

### Measurement protocols

Directory: [docs/measurements/protocols/](docs/measurements/protocols/)

| File | Purpose |
|---|---|
| [p6lt_shadow.md](docs/measurements/protocols/p6lt_shadow.md) | P6-LT Shadow phase definition: gates, measurement structures, required ledger fields. |
| [c0mo_market_observer.md](docs/measurements/protocols/c0mo_market_observer.md) | C0-MO unfiltered read-only market observer. |
| [c0e1_econ4h.md](docs/measurements/protocols/c0e1_econ4h.md) | C0-E1 combined economic observation (`P0_MODE=econ4h`). |

### Historical measurement reports

Directory: [docs/measurements/historical_reports/](docs/measurements/historical_reports/)

| File | Purpose |
|---|---|
| [fix129_cost_qualification.md](docs/measurements/historical_reports/fix129_cost_qualification.md) | FIX-129 cost-model qualification history (2026-10-05). |
| [c0e1_implementation_report.md](docs/measurements/historical_reports/c0e1_implementation_report.md) | C0-E1 implementation, validation and qualification history (2026-10-08 to 2026-10-12). |

### Development history

Directory: [docs/development_history/](docs/development_history/)

| File | Purpose |
|---|---|
| [audit_handoff_20261005.md](docs/development_history/audit_handoff_20261005.md) | **Historical** post-audit handoff (2026-10-05, P6-MT override 2026-10-07); superseded by [STATUS.md](docs/STATUS.md). |
| [r127d_changelog_rollback.md](docs/development_history/r127d_changelog_rollback.md) | R127d candidate cache / re-arm change, measured queue PASS, rollback points and post-audit correctness reopening. |
| [r127c_changelog_rollback.md](docs/development_history/r127c_changelog_rollback.md) | R127c batching implementation and rollback. |
| [fable_audit_20261003.md](docs/development_history/fable_audit_20261003.md) | Earlier audit snapshot; historical, not current status. |
| [fable_work_order.md](docs/development_history/fable_work_order.md) | Earlier implementation work order (2026-09-30). |

---

## 6. Evidence map

All versioned Project 0 evidence belongs under:

> **[docs/evidence/](docs/evidence/)**

### Current Astra audit corpus

Directory: **[docs/evidence/AUDIT_20261005/](docs/evidence/AUDIT_20261005/)**

| File | Meaning |
|---|---|
| [AUDIT_FINDINGS.md](docs/evidence/AUDIT_20261005/AUDIT_FINDINGS.md) | Audit findings and limitations. |
| [COMMANDS_AND_RESULTS.md](docs/evidence/AUDIT_20261005/COMMANDS_AND_RESULTS.md) | Commands actually run and observed results. |
| [BUNDLE_README.md](docs/evidence/AUDIT_20261005/BUNDLE_README.md) | Contents and provenance of the complete bundle. |
| [manifest.json](docs/evidence/AUDIT_20261005/manifest.json) | Audit source SHA, reference-repository SHAs and test counts. |
| [cache_counterexamples.txt](docs/evidence/AUDIT_20261005/cache_counterexamples.txt) | Reproduced Candidate Cache correctness/fairness counterexamples. |
| [cache_counterexamples.py](docs/evidence/AUDIT_20261005/cache_counterexamples.py) | Reproducer for the checked-in cache predicate. |
| [history_summary.json](docs/evidence/AUDIT_20261005/history_summary.json) | Summary of six preserved real liquidations. |
| [history_analysis.json](docs/evidence/AUDIT_20261005/history_analysis.json) | Detailed decoded transaction analysis. |
| [recent_liquidation_records.json](docs/evidence/AUDIT_20261005/recent_liquidation_records.json) | Decoded recent liquidation records. |
| [selected_signatures.json](docs/evidence/AUDIT_20261005/selected_signatures.json) | Selected historical signatures / scan context. |
| [drift_dispatch.json](docs/evidence/AUDIT_20261005/drift_dispatch.json) | Drift `Custom(101)` simulation evidence. |
| [tests_health.txt](docs/evidence/AUDIT_20261005/tests_health.txt) | Compact `crate_health` test results. |
| [tests_txv1.txt](docs/evidence/AUDIT_20261005/tests_txv1.txt) | Compact `crate_health,txv1` test results. |
| **[darkbot_audit_bundle_20261005.zip](docs/evidence/AUDIT_20261005/darkbot_audit_bundle_20261005.zip)** | Complete forensic audit package: audited source archive, public reference-source snapshots, raw transaction data, scripts, logs and checksums. |

The six per-signature JSON files in that directory are the preserved raw transaction payloads for the historical receivership examples.

### Runtime / qualification evidence

Important versioned evidence includes:

| Evidence | Purpose |
|---|---|
| [R114_projection_clock.json](docs/evidence/R114_projection_clock.json) | Projection clock/timestamp evidence. |
| [R116_integration_sweep.json](docs/evidence/R116_integration_sweep.json) | Integration sweep. |
| [R117_paced_projection_sweep.json](docs/evidence/R117_paced_projection_sweep.json) | Paced projection sweep. |
| [R118_route_budget.json](docs/evidence/R118_route_budget.json) | Route/account budget evidence. |
| [R119_route_budget.json](docs/evidence/R119_route_budget.json) | Follow-up route budget evidence. |
| [R120_protocol_luts.json](docs/evidence/R120_protocol_luts.json) | Protocol LUT evidence. |
| [R121_sizing_pairs.json](docs/evidence/R121_sizing_pairs.json) | Sizing-pair qualification. |
| [R122_juplend_complete.json](docs/evidence/R122_juplend_complete.json) | JupLend integration evidence. |
| [R124_rpc_economics.json](docs/evidence/R124_rpc_economics.json) | RPC economics evidence. |
| [R125b_oracle_trigger_runtime.json](docs/evidence/R125b_oracle_trigger_runtime.json) | Oracle-trigger runtime evidence. |
| [R126b_scheduler_fairness.json](docs/evidence/R126b_scheduler_fairness.json) | R126b scheduler evidence. |
| [R127_20261005T143113Z.json](docs/evidence/R127_20261005T143113Z.json) | Initial R127 qualification. |
| [R127c_20261005T161806Z.json](docs/evidence/R127c_20261005T161806Z.json) | R127c PARTIAL batching evidence. |
| [R127d_20261005T171043Z.json](docs/evidence/R127d_20261005T171043Z.json) | R127d queue/fanout qualification evidence. |
| [FIX129_fee_sources.json](docs/evidence/FIX129_fee_sources.json) | Fee-source qualification evidence. |

### Evidence rule

Evidence is **append-only / immutable after qualification**.

If a conclusion changes later:

```text
do not overwrite the old evidence
-> add new evidence
-> update MEASUREMENT_LEDGER.md
-> update the relevant changelog/handoff
```

Raw evidence should identify the source commit, run/dataset ID, environment/provider where applicable, and explicitly state non-claims.

---

## 7. Scripts map

Directory: [scripts/](scripts/)

| Script | Purpose |
|---|---|
| [c0_census.py](scripts/c0_census.py) | Mainnet account census / classification tooling. |
| [run_shadow_mainnet.sh](scripts/run_shadow_mainnet.sh) | Standard Mainnet Shadow runner entry. |
| [run_r126_qualify.py](scripts/run_r126_qualify.py) | R126 qualification parser/gates. |
| [run_r127_qualify.py](scripts/run_r127_qualify.py) | R127/R127c/R127d qualification parser/gates. |

A qualification script must exercise production components; it must not silently implement a second version of business logic.

---

## 8. Data, exports and generated artifacts

### `data/`

Directory: [data/](data/)

Use for generated/local runtime datasets, shadow output and temporary measurement material that is not yet frozen as canonical evidence.

**Do not treat local `data/` files as durable evidence.**

When a run becomes a gate/decision input:

```text
data/... raw/local result
-> validate / summarize
-> copy the required immutable artifact to docs/evidence/
-> reference it from MEASUREMENT_LEDGER.md
```

### `exports/`

Directory: [exports/](exports/)

Generated human/machine exports may live here. They are non-authoritative unless separately frozen under `docs/evidence/` and referenced by the ledger.

### Build artifacts

Rust `target/`, caches, temporary logs and other build outputs are **not repository evidence** and must not be committed merely because they were produced by a test.

---

## 9. Configuration and secrets

### Versioned public config

Directory: [config/](config/)

Current public wallet/config example:

- [config/wallet.public.env](config/wallet.public.env)
- [.env.example](.env.example)

### Secrets

Never commit:

```text
private keys
seed phrases / mnemonics
API keys
bearer tokens
private RPC URLs containing credentials
real .env files
signing keypair files
```

Public addresses and non-secret configuration may be versioned when useful.

---

## 10. Current Project 0 status

The current status is maintained in:

> [docs/STATUS.md](docs/STATUS.md)

The 2026-10-05 status block that stood here remains in the git history (up to commit `1b87387`); the corresponding
audit-era assessment is in [docs/development_history/audit_handoff_20261005.md](docs/development_history/audit_handoff_20261005.md).
Do not copy status into this README. Update the measurement ledger and `docs/STATUS.md` instead.

---

## 11. Current implementation priority

The current ordered First-Profit path is FE-00 → FE-07 (`PHASENPLAN.json` → `current_prioritized_technical_path`).
Status per ticket and the next technically necessary steps are in [docs/STATUS.md](docs/STATUS.md).

Do not start another broad architecture rewrite unless new evidence requires it.

---

## 12. File-placement rules

| New artifact | Put it here |
|---|---|
| Production Rust logic | `native/crates/<appropriate-crate>/src/` |
| New executable/probe that belongs to Rust production workspace | appropriate crate `src/bin/` |
| Operational/qualification script | `scripts/` |
| Current human-readable status / handoff | `docs/` |
| Measured immutable evidence | `docs/evidence/` |
| Audit-specific evidence corpus | `docs/evidence/AUDIT_<date>/` |
| Local generated run data | `data/` |
| Generated export/report | `exports/` |
| Public non-secret configuration | `config/` |
| Gate definition/documentation | `docs/qualification/` |
| Project-wide non-Rust tests | `tests/` |

Rules:

```text
1. Do not scatter evidence across source directories.
2. Do not put generated runtime data into docs/.
3. Do not overwrite old evidence when a later result disagrees.
4. Do not create duplicate implementations of health/economics/routing for test tooling.
5. Do not add new top-level directories without documenting their role here.
6. Every important gate result must be linked from MEASUREMENT_LEDGER.md.
7. Every major architectural/current-state change must update the relevant handoff/changelog.
8. The audit ZIP is forensic backup; normal work should use the extracted/versioned files first.
```

---

## 13. Build and test entry points

Rust workspace:

```bash
cd project_0/native

cargo build --locked --features crate_health
cargo build --locked --features "crate_health txv1"

cargo test --locked --workspace --features crate_health
cargo test --locked --workspace --features "crate_health txv1"
```

Mainnet Shadow entry:

```bash
project_0/scripts/run_shadow_mainnet.sh
```

Qualification:

```bash
python3 project_0/scripts/run_r127_qualify.py ...
```

Do not enable value-bearing LIVE solely because workspace tests are green. Follow the gates in `docs/STATUS.md` and the measurement ledger.

---

## 14. Git / rollback anchors

Project 0 tags (structure reform baselines):

```text
project0-last-qualified-20261014     0782d8b  qualified operational baseline
project0-last-unstructured-20261014  4c271cb  reform start; includes FE-01 WIP
project0-runtime-config-preview-20261014  e01560f  runtime configuration preview
```

The `archive/*` branches named in the 2026-10-05 changelogs no longer exist locally or on `origin`
(checked 2026-10-14). Their documented commits are still reachable from `main`:

```text
archive/r127b-pre-batch-20261005           bdcaa41c2c41eeefdacad08e7a8285a7f1921b79
archive/r127c-batch-partial-20261005       dde69699a9943e376ca4343d454f8e3177ee9b36
archive/r127d-pass-runtime-20261005        cf5085e796b4956fcf2376d08369b2b818dff39a
archive/project0-audit-baseline-20261005   799048f8fa95df4c29f2aa6b35bcc3615405fe19 (audited runtime source)
archive/r127d-initial-64slot-20261005      no commit SHA documented
archive/r127d-1slot-prequal-20261005       no commit SHA documented
```

Use the changelog/rollback documents before reverting runtime behavior:

- [r127c_changelog_rollback.md](docs/development_history/r127c_changelog_rollback.md)
- [r127d_changelog_rollback.md](docs/development_history/r127d_changelog_rollback.md)

---

## 15. Frontend boundary

Project 0 browser/UI code lives outside this directory:

> [frontend/src/features/project0/](../frontend/src/features/project0/)

The frontend must not become a second implementation of protocol health, liquidation sizing, economics, receivership or execution semantics. Production truth remains in the Rust Project 0 core.

---

## 16. Maintenance rule for this README

Update this file whenever any of the following changes:

```text
- a top-level project_0 directory is added/removed;
- a canonical document changes role;
- a production crate is added/removed;
- evidence storage conventions change;
- the role of `docs/STATUS.md` changes;
- the primary build/qualification entry point changes.
```

Keep links relative so they work both on GitHub and in a local clone.

## 17. Unfiltered Live Market Observer (C0-MO)

Measurement protocol (purpose, start, flow, output, verdicts, meaning):
[docs/measurements/protocols/c0mo_market_observer.md](docs/measurements/protocols/c0mo_market_observer.md).

## 18. C0-E1 combined economic observation candidate

Measurement protocol (`P0_MODE=econ4h` entry, defaults, output schema, limitations):
[docs/measurements/protocols/c0e1_econ4h.md](docs/measurements/protocols/c0e1_econ4h.md).
