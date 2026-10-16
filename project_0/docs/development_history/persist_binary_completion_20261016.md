# Binary journal completion (PERSIST-01 to PERSIST-05), 2026-10-16

Base: `bb26313` (binary journal WIP); **committed to `main` as `4a27a1c`**. Der folgende Bericht enthält die ursprüngliche lokale Arbeitsbeschreibung; Aussagen „uncommitted / not pushed“ am Dokumentende sind historisch, nicht der aktuelle Git-Status. Contract: `docs/PHASENPLAN.md`, PERSIST-01 to
PERSIST-05. The existing `analytics/journal.rs` and `analytics/evidence.rs` were continued; no second journal
implementation. Python never encodes or decodes the format: it calls `p0-journal`. Evidence:
[PERSIST_BINARY_20261016](../evidence/PERSIST_BINARY_20261016/qualification.json).

**Overall: PARTIAL, not QUALIFIED.** Offline evidence is complete for what can be run locally; the open items below block
the QUALIFIED claim.

## What changed

| Area | Before | Now |
|---|---|---|
| Journal core (`journal.rs`) | header identities from `Pubkey::new_unique()` (a process-local counter: identical in every run) | run = random per process, session = random per writer, boot = Linux `boot_id` or a process-scoped random value; event-kind and source registry; canonical key order in `encode`; table CRC32 (same values); no payload copy per append; atomic multi-view export (all or nothing, never overwrites); parent fsync fixed for bare file names |
| FE-04 wallet journal (`one_shot.rs`) | one JSON snapshot per wallet (`pending` + rename) | replayable automaton in `<wallet>.p0j`: every transition (`Reserve`, `Handoff`, `ReopenUnknown`, `Observe`, `Finalize`) is a frame with the full state version; the state advances only after fsync confirmed that exact frame; `handoff` sends only after its durable `HandoffUnknown`; replay rejects illegal transitions, foreign kinds, seals, truncation and corruption; a pre-binary `<wallet>.json` is imported once, bound by SHA-256 and never rewritten |
| Shadow owner (PERSIST-02) | `eprintln!` on the owner thread (per event in places; stderr goes through `tee`), env reads per evaluation, `json!` at shutdown | diagnostics are formatted on the owner and leave through the bounded evidence queue; the writer thread journals them (kind 104) and only then prints the same text; route-fit and recovery lines are buffered in memory (bounded, overflow counted) and forwarded the same way; env values cached at setup; typed `ShadowStats`; guard test over every owner-loop file |
| MT0 | `mt0.jsonl`, append across runs, no fsync, never joined, no final snapshot, write errors only printed | `mt0.p0j` (kind 600) via a sink on the reporter thread; final snapshot, seal and join at shutdown; a lost snapshot seals INCOMPLETE and fails the run (`MT0_EVIDENCE_INCOMPLETE`); default `<record>.mt0.p0j` |
| C0-E1 phases | `measurement_phases.jsonl` appended live | `measurement_phases.p0j` (kind 500, fsync per row); sealed on every supervisor exit; the JSONL is rendered then |
| C0-E1 econ worker | `shadow_evaluations.jsonl` + `shadow_offsets.bin`; reports written as JSON | `shadow_evaluations.p0j` (kind 400, `{previous_target_offset, record}`: the per-target chain replaces the offset index; every hop CRC/SHA verified); `economics_reports.p0j` (kinds 410-413) with the legacy report files as renders; `opportunity_economics.jsonl` no longer truncates (`create_new`) |
| C0-MO market capture | `JsonlSink` | `JournalSink` writes `market.p0j` (kinds 300-302) on the same thread; `finalize` seals and renders the three JSONL views, then writes `summary.json` with the `evidence_journal` binding; an early exit seals INCOMPLETE and still renders. `JsonlSink` is unchanged (the pinned strict verifier uses it) |
| Offline replay | JSONL only | `p0-econ-replay` reads `shadow_evaluations.p0j`, falls back to the legacy JSONL, refuses both at once (`AMBIGUOUS_SHADOW_SOURCE`); byte-identical renders from either source (test) |
| FE-01 | one `read_NNNNNN.json` per RPC read | `fe01.p0j` (config, each read fsynced before decoding, result, seal); `--replay` reads the journal, keeps reading old directories, refuses both sources; result pins the built TX version (`TRANSACTION_VERSION_PIN`), cluster support stays an explicit gap |
| Views for pinned readers | — | `analytics/renders.rs` catalog; `p0-journal renders RUN [--create]`; `verify.sh` renders missing views and requires byte equality before the capture is hashed (`JOURNAL_VIEWS_INVALID` otherwise) |
| Tools/scripts | — | `p0-journal verify|frames|export|renders|import` (import: census kinds only); `capture.sh` builds it, per-run Shadow directory, `.p0j` paths, no `P0_SUMMARY_PATH`; R126/R127 count from the journal and report a missing journal instead of zeros; `c0_census.py` writes its rows through `p0-journal import` |

## Status per requirement

| Requirement | Status | Basis / open |
|---|---|---|
| PERSIST-01 one primary store | **PARTIAL** | every productive runtime, capture, shadow, winner/reconcile, replay, FE-01, FE-04, MT0 and census stream is binary. Open: the strict verifier's batch files (`market_verification.rs` builds `JsonlSink` and hashes its output; the file is hash-pinned into strict spec IDs, so migrating it needs an explicit re-pin decision). Fork provenance is written as UNKNOWN (no fork id is available at the writers). Segment rotation not implemented (one file per stream and run). |
| PERSIST-02 hot-path boundary | **QUALIFIED offline** | owner loop free of JSON, file access, flush/fsync, env reads and direct stderr/stdout (source guard `owner_thread_sources_have_no_json_io_env_or_blocking_logging`); full queue closes admission, capture INCOMPLETE; no synchronous fallback. Owner-side timing under mainnet load: NOT_MEASURED. |
| PERSIST-03 money and crash recovery | **PARTIAL** | FE-04 automaton with durability barrier; tests: crash before/after handoff, reopen to UNKNOWN, torn/corrupt/truncated/foreign/sealed journal, illegal transition, write failure before send (stands for disk full), double trigger/send, legacy import binding. fsync-failure injection: NOT_RUN (no fault-injecting filesystem). Reorg: receipts accept only finalized observations (existing tests); not a journal-level feature. FE-04 has no production caller yet (LIVE stays locked). |
| PERSIST-04 historical contracts | **QUALIFIED offline** | pinned readers and their identities unchanged; legacy JSONL runs, legacy FE-01 read files and legacy FE-04 JSON journals stay readable (tests); historical evidence untouched. |
| PERSIST-05 qualification | **PARTIAL** | (1) inventory done; (2) writers, drain, manifest, readers/replay migrated with consumer parity (byte-identical views and replay renders); (3) both feature/release builds, Rust/Python/verifier regressions PASS; (4) negative tests listed above, fsync failure NOT_RUN; (5) local synthetic benchmark measured (below), mainnet volume/throughput/tails NOT_MEASURED; (6) therefore PARTIAL. |

## Local writer measurement (release, Apple M4 Max, APFS, synthetic)

| Payload | append p50 / p95 / p99 / max | frames/s | journal : JSONL size |
|---|---|---|---|
| market row (~3.2 KB) | 14.7 / 18.6 / 24.7 µs / 3.05 ms | 65,111 | 1.08 |
| opportunity record (~0.84 KB) | 8.4 / 11.0 / 11.8 µs / 0.53 ms | 111,481 | 1.75 |
| MT0 snapshot (~0.75 KB) | 11.3 / 13.8 / 15.4 µs / 0.19 ms | 84,760 | 2.54 |
| durable append + fsync (FE-04, phases) | 4.23 / 5.24 / 5.31 ms / 8.17 ms | — | — |

Integers are stored as 16-byte i128/u128, which inflates integer-heavy records; a compact encoding would be a new journal
version. On macOS `sync_all` is a full flush: the FE-04 durability barrier costs about 4-5 ms before a handoff.

## Tests changed with the contract (intent kept)

- `econ_tests`: V2 layout test checks `shadow_evaluations.p0j` (no JSONL, no offset index) and byte-exact report renders;
  the million-row test counts journal frames (1,000,000) instead of JSONL lines; the corruption test damages a journal
  frame instead of truncating `shadow_offsets.bin`. Dev builds compile `sha2` with `opt-level = 3`
  (`native/Cargo.toml`) because unoptimized SHA-256 dominated that test.
- `one_shot`: the legacy-journal test writes a pre-binary JSON journal and checks import, binding and immutability.
- `measurement`: the phase test reads the rendered JSONL after `finalize_phases`.
- `test_run_store.py`: the real-binary launcher test checks the journals with `p0-journal verify` and the views with
  `p0-journal renders`.

## Not done here

PHASENPLAN and MEASUREMENT_LEDGER (and their JSON indexes) were not edited; a ledger entry for this work is left to the
user. No mainnet run, no LIVE, no submission, no commit or push.
