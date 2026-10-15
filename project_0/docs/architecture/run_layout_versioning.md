# Run layout versioning

Status 2026-10-15, structure iterations 8b.2/8b.3, **PENDING_INDEPENDENT_REVIEW**. Decision: **Option A, one coordinated,
versioned re-pin** (implemented). Operating instructions: [run storage](../operations/run_storage.md).

## 1. Layout versions

| Version | Runs | Identification | Readers |
|---|---|---|---|
| **V1** `Project0/RunLayout/1` | every historical run (`data/econ4h`, `data/market`, `data/runs` from 8b.1), new C0-MO runs, explicit legacy roots | no `manifest.json`; none of `manifest.json, config.json, capture, snapshots, logs` at the root | yes, unchanged behaviour |
| **V2** `Project0/RunLayout/2` | new econ4h / C0-E1 runs in `data/runs` (default since 8b.3) | `manifest.json` with `"layout": "Project0/RunLayout/2"` and `"schema": "C0-E1/MeasurementManifest/3"`; root contains only `manifest.json, config.json, capture/, snapshots/, verification/, reports/, logs/` (dotfiles ignored) | yes |

Anything else is `RUN_LAYOUT_INVALID` (mixed names, a V2 run without its manifest, a malformed manifest); a layout tag
`Project0/RunLayout/<n>` other than 1 or 2 is `RUN_LAYOUT_UNSUPPORTED`. No fallback guesses a layout. Implementation:
`detect_layout` in [`scripts/run_store.py`](../../scripts/run_store.py); the artifact map is `ARTIFACTS` there and
`RunPaths` in `native/crates/darkbot-runner/src/run_layout.rs` (Rust writers). Tests pin both to the same literal paths.

In V2 there are no reader fallbacks: the manifest is the only identity and plan source, the phase journal and the three market
streams must exist (`RUN_ARTIFACT_MISSING`), and the capture evidence index is validated before reuse.

## 2. Pinned files

| Pinned file | Identities that hash it |
|---|---|
| `measurement_contract.py` | `verification_spec_id` (decoder pin), report dataset key (`contract_sha256`) |
| `audit_market_quick.py` | `verification_spec_id` (decoder pin), index checkpoint (`decoder_sha256`), report dataset key (`decoder_sha256`) |
| `market_report.py` | report dataset key (`normalizer_sha256`) |
| `run_store.py` (**new pin**) | `verification_spec_id` (decoder pin), index checkpoint (`layout_sha256`), report dataset key (`layout_sha256`) |

Strict mode additionally pins `market_verification.rs`, `market_reconcile.rs`, `market_truth.rs`, `market_gap.rs` (unchanged)
and the `p0-strict-verify` binary; report datasets pin the `p0-econ-replay` binary. Not pinned: `measurement_verify.py`,
`measurement_cli.py`, `operational_config.py`, the shell launchers. Because `run_store.py` is pinned, any later change to it
creates new identities exactly like a change to the readers.

## 3. Identity versions after the re-pin

| Identity | Before (archived) | Now | Effect |
|---|---|---|---|
| `capture_run_id` | manifest `capture_run_id`/`run_id`, or legacy name + summary hash | unchanged; V2 only from `manifest.json` | independent of root and layout |
| capture evidence | `C0-E1/CaptureEvidence/2` in `capture_evidence_manifest.json` | V1 unchanged (byte-identical); V2 `C0-E1/CaptureEvidence/3` with `layout` in `verification/capture_evidence_manifest.json` | — |
| `verification_spec_id` | `C0-E1/VerificationSpec/2` | `C0-E1/VerificationSpec/3` with `layout`, `run_store.py` in `decoder_pins` | new spec ids for new verifications |
| index checkpoint | `C0-E1/INDEX/2`, default `verification/index_v2` | `C0-E1/INDEX/3` with `layout` and `layout_sha256`, default `verification/index_v3` | old checkpoints are not continued |
| report dataset key | `C0-E1/NormalizedReport/2` | `C0-E1/NormalizedReport/3` with `layout` and `layout_sha256` | a new dataset is built from raw evidence |
| report records, hourly snapshots, renderer | `C0-E1/NormalizedReport/2` | unchanged format and version | identical records give identical `records_sha256` and `report_query_identity` |
| decoder version in records and index summaries | `C0-E1/INDEX/2` | unchanged (decoder logic unchanged) | — |
| Rust manifest | `C0-E1/MeasurementManifest/2` | V1 unchanged (byte-identical); V2 `C0-E1/MeasurementManifest/3` with `layout` and `phase_journal` `capture/measurement_phases.jsonl` | — |

**V1 stays historically reproducible.** For V1 runs the re-pinned readers produce byte-identical capture evidence, legacy
identity, window, report records and hourly snapshots compared with the archived readers of checkpoint
`p0-before-claude-iter15-20261015` (`908b14c`); only versioned identities differ (test `ArchivedParity` in
`tests/regression/test_v1_archived_parity.py`). Stored specs, results and datasets are validated against their own digests, never recomputed,
so all existing verification and report ids stay readable and traceable. Reproducing an archived id bit for bit needs the
commit whose pinned file hashes equal the `decoder_pins` in its `spec.json`.

## 4. Resume rules

- **Verification spec:** `--resume-verification` when the current spec does not exist yet, but a spec for the same capture and
  mode written by another implementation (different `schema`, `decoder_pins` or `layout`) does, fails with
  `RESUME_REQUIRES_ARCHIVED_IMPLEMENTATION: <spec id> (<schema>)` before anything is created. Resume that attempt with the archived
  implementation, or start a new verification without `--resume-verification`. Same implementation with other budgets or
  provider behaves as before.
- **Index checkpoint:** a stored identity with another `schema`, `decoder_sha256` or `layout_sha256` fails with
  `RESUME_REQUIRES_ARCHIVED_IMPLEMENTATION`; same implementation but another run or window fails with
  `RESUME_IDENTITY_MISMATCH`. A false resume PASS is impossible.
- **Report datasets:** a changed dataset identity builds a new dataset; existing datasets stay untouched and verified.
- Local inventory 2026-10-15: no spec-based verification directory exists in any local run; the only checkpoint is
  `market5h_20261011T010758Z/verification/index_v1`, which neither the archived nor the current code resumes.

## 5. Options considered

| Option | Content | Decision |
|---|---|---|
| **A: one coordinated, versioned re-pin** | readers resolve paths through `run_store` by layout; explicit identity versions | **implemented** |
| B: separate V2 reader copies | second verifier and report implementation | rejected (no second engine) |
| C: postpone V2 | keep V1 under `data/runs` | superseded |

## 6. Tests

`tests/python/test_run_layout.py` (artifact table, store/layout rules, V2 skeleton, `config.json` assembly, detection and whitelist,
identity versions, equal capture id and records in V1 and V2, archived resume detection,
self-test without tests fails), `tests/regression/test_v1_archived_parity.py` (V1 parity with the archived readers),
`tests/integration/test_run_store.py` (launcher end-to-end in V1 and V2, V2 verify and report through
`verify.sh` / `report.sh` by id), Rust `run_layout`, `measurement` (V1 and V2 manifest) and `econ_tests`
(`runtime_worker_writes_layout_v2_capture_and_snapshots`).
