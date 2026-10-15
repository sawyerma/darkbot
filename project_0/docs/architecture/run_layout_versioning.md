# Run layout versioning

Status 2026-10-15, structure iteration 8b.2. Decision record for moving runs to the target layout without
breaking historical identities. Operating instructions: [run storage](../operations/run_storage.md).

## 1. Layout versions

| Version | Runs | Identification | Readable by the current verifier and reports |
|---|---|---|---|
| **V1** | every run written so far (`data/econ4h`, `data/market`, and `data/runs` since 8b.1) | no layout marker; no `manifest.json`, `config.json`, `capture/`, `snapshots/` or `logs/` at the run root | yes |
| **V2** | none yet; no writer produces it | `manifest.json` with `"layout": "Project0/RunLayout/2"` | **no**; rejected with `RUN_LAYOUT_UNSUPPORTED` |

Anything else, for example a V1 run with a `logs/` directory or a `manifest.json` without the V2 tag, is
`RUN_LAYOUT_INVALID`. There is no fallback that guesses a layout. Implementation: `detect_layout` in
[`scripts/run_store.py`](../../scripts/run_store.py). All local runs were checked on 2026-10-15; none contains a V2-only name.

## 2. Pinned readers and what they bind

Three Python files are hashed into identities. They read V1 paths directly.

| Pinned file | Reads / writes in the run directory | Identities that hash this file |
|---|---|---|
| `measurement_contract.py` | `measurement_manifest.json`, `market/summary.json` or `summary.json`; writes `capture_legacy_identity.json`, `capture_evidence_manifest.json`; hashes every file except `verification/` and `reports/` | `verification_spec_id` (decoder pin), report dataset key (`contract_sha256`) |
| `audit_market_quick.py` | `measurement_manifest.json`, `measurement_phases.jsonl`, `market/summary.json`, `market/transactions.jsonl`, `market/liquidations.jsonl`, `market/coverage.jsonl`, `canonical_market_shadow.jsonl`, `opportunity_economics.jsonl`; writes `verification/index_v2/` | `verification_spec_id` (decoder pin), index checkpoint identity, report dataset key (`decoder_sha256`) |
| `market_report.py` | `verification/*/spec.json` and `results/`, `market/`, `measurement_manifest.json`, `shadow_evaluations.jsonl`, `canonical_market_shadow.jsonl`, `opportunity_economics.jsonl`, `hour_01..04.json`, `economics_final.json`; writes `reports/` | report dataset key (`normalizer_sha256`) |

Not pinned: `measurement_verify.py`, `measurement_cli.py`, `operational_config.py`, `run_store.py`, the shell launchers.
In strict mode the spec also pins `market_verification.rs`, `market_reconcile.rs`, `market_truth.rs`, `market_gap.rs`
and the `p0-strict-verify` binary; report datasets also pin the `p0-econ-replay` binary.

## 3. Identity chains

| Identity | Inputs | On a changed pin |
|---|---|---|
| `capture_run_id` | `measurement_manifest.json` (`capture_run_id` or `run_id`); legacy: run directory name + summary hash | unchanged; independent of the root |
| `verification_spec_id` | schema `C0-E1/VerificationSpec/2`, capture identity, window, mode, protocol pin, decoder pins, FREE_ONLY provider binding, budgets | new verifications get a new spec id |
| `verification_attempt_id` / `verification_result_id` | random per attempt / digest of the result payload | new ids, as for every attempt |
| index checkpoint | schema `C0-E1/INDEX/2`, run directory name, window, `sha256(audit_market_quick.py)` | resume fails closed with `RESUME_IDENTITY_MISMATCH` |
| report dataset key | `C0-E1/NormalizedReport/2`, normalizer, decoder, contract and replay-binary hashes, capture and result ids | a new dataset is built from raw evidence |
| `report_query_identity` | capture id, result id, query, renderer version, `records_sha256` of the dataset | unchanged if the records are unchanged |

Stored artifacts are never recomputed with current code: reports validate a stored `spec.json` against its own
digest, results against their digest and artifact hashes. Historical verifications therefore stay readable under
their original ids after a re-pin; reproducing them bit for bit needs the commit that created them.

**What a re-pin breaks:** only in-progress resumes. An unfinished verification attempt or index checkpoint created
before the change can no longer be resumed and fails closed (`RESUME_IDENTITY_MISMATCH`, or a new spec id without
the old attempt). Local inventory 2026-10-15: no spec-based verification directory exists in any local run; the only
checkpoint is `market5h_20261011T010758Z/verification/index_v1`, which the current code (`index_v2`) already does not resume.

## 4. Decision required before V2 readers and writers

Layout V2 needs the three pinned readers to resolve paths by layout. That is a re-pin and is not done in 8b.2.

| Option | Content | Consequence |
|---|---|---|
| **A: one coordinated, versioned re-pin** (recommended) | Pinned readers resolve paths through `run_store` by layout; `run_store.py` joins the decoder pins; spec schema `C0-E1/VerificationSpec/3`, index schema `C0-E1/INDEX/3` and report version `C0-E1/NormalizedReport/3` carry the layout | V1 and V2 both readable; new ids for all new verifications; unfinished attempts must be finished with the old commit or restarted, never mixed |
| B: separate V2 reader copies | new modules for V2, V1 files untouched | second implementation of the verifier and report logic; contradicts the reform rules |
| C: postpone V2 | keep V1 under `data/runs` (current state) and continue with 8b.3 and phase C | no identity change; target layout deferred |

Preconditions for A: confirm on every machine that no unfinished verification or index checkpoint must be resumed;
record the transition in the ledger; tests as in section 6.

## 5. V2 artifact map (for the writer step after the decision)

| V1 path | V2 path | Writer |
|---|---|---|
| `measurement_manifest.json` | `manifest.json` (plus `layout`) | Rust `measurement.rs` |
| `measurement_phases.jsonl` | `capture/measurement_phases.jsonl` | Rust `measurement.rs` |
| `measurement_plan.json`, `measurement_config.json`, `operational_config.json`, `verification_preflight.json` | `config.json` and `config/` details | launcher, `measurement_cli.py` |
| `market/*` | `capture/market/*` | Rust `market_observer.rs` |
| `shadow_evaluations.jsonl`, `shadow_offsets.bin`, `opportunities.jsonl`, `daily_summary.jsonl`, `mt0.jsonl` | `capture/` | Rust `econ.rs`, `main.rs` |
| `hour_01..04.json`, `economics_final.json`, `opportunity_economics.jsonl`, `canonical_market_shadow.jsonl` | `snapshots/` | Rust `econ.rs` |
| `verification/`, `reports/` | unchanged | `measurement_verify.py`, `market_report.py` |
| `build.log`, `runtime.log`, `console_summary.txt`, `gap_proxy.log` | `logs/` | launchers |
| `capture_evidence_manifest.json`, `candidate_manifest.json`, `storage_preflight.json` | to be fixed in the decision | `measurement_contract.py`, launcher |

## 6. Tests required for the V2 step

Synthetic end-to-end runs in V1 and V2 through the same launcher; byte-identical results for V1 before and after;
spec, checkpoint and report schema versions present in new artifacts; resume under the old spec id refused, never
mixed; `capture_run_id` equal for identical runs in either layout; collision and parallel allocation; malformed
mixed layouts rejected; full Python suite, both Rust matrices, both release builds, differential Clippy.
