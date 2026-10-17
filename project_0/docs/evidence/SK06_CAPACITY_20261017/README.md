# SK-06 CapacityPreflight — completion of the `0e0544d` WIP and launcher integration (2026-10-17)

**Verdict:** SK-06 **LOCAL_PASS in scope** (decision logic, measurement, launcher integration, native record). Real basis **NOT_MEASURED**: no sealed `.p0j` run exists on this host (`data/runs` absent, 22 historical `data/econ4h` runs are JSONL only), so every capture longer than 600 s is refused until a calibration capture (≤ 600 s, explicit Mainnet permission) has produced a basis. Provider capacity is **NOT_RUN** (BOOT-04 network audit needs permission). This is the intended fail-closed state, not a capacity PASS.

## What the preflight measures (no assumed values)

| Check | Input | Rule |
|---|---|---|
| `data_rate` | newest ≤ 3 sealed complete runs of the requested capture source under the run root; every journal lane (market, Shadow evaluations, economics reports, owner evidence, MT0, phases) in exactly the frame format the current binary writes (`payload::check_write`) | no valid run → CAPACITY_UNVERIFIED; crashed, incomplete, foreign-format, other-source and shorter-than-10 s runs are skipped with their reason |
| `disk` | lane bytes/s × duration × (1 + `capacity_safety_percent`) + `market_disk_reserve_mib` + verifier disk (only when a verification mode is selected) | free < need → BLOCKED |
| `write_rate` | journal-codec write probe on the target volume (default 64 MiB, synced every 8 batches) vs. the summed lane bursts (10 s windows, conservative sum) | < avg → BLOCKED; < burst → UNVERIFIED |
| `sync_latency` | the probe's measured sync times vs. the writer stall budget | max × safety > stall budget → BLOCKED |
| `ram_backlog` | market burst × stall budget × 2 (payload + batch copy) vs. `market_capture_max_mib` | above max → BLOCKED |
| `bounded_collections` | econ/winner/Shadow collections are count-bounded only | UNVERIFIED until SK-03 byte bounds (B1) exist |
| `physical_memory`, `memory_pressure`, `swap`, `cpu` | OS | max + system reserve > physical → BLOCKED; pressure ≥ 4 → BLOCKED; warning pressure, swap above the system reserve, load > 0.75 × cores → UNVERIFIED |
| `verification` | **selected mode only**: `off` NOT_APPLICABLE; `index`/`crosscheck` measured program-transaction rate → signatures, pages, calls, walltime; `strict` measured slot rate (+ per-slot calls/bytes of earlier kept strict results) → calls, walltime, response bytes | budget too small → BLOCKED with the flags to raise or `--chain-verify off` + later `--resume-verification`; unmeasured → UNVERIFIED. A 4 h strict run with 2,000 calls is never strict-qualified |
| `provider` | capture provider (WSS liveness, 429, FREE_ONLY quota, archive) | NOT_RUN offline (counts as UNVERIFIED) |

`start_allowed` = QUALIFIED, or UNVERIFIED with duration ≤ 600 s (calibration). Provider quota for verification stays with the existing verification preflight (`verification_preflight.json`, pinned `measurement_contract.py`); this preflight sizes the configured budget against the duration.

## Integration

- `scripts/capture.sh` runs `p0-capacity` instead of the former Python 60 GiB heuristic, with the run's verification budget from the typed measurement config; exit ≠ 0 stops the launch before any capture (`logs/capacity_preflight.txt`).
- Native record: `capture/capacity_preflight.p0j` (V2; V1: run root), kind `CAPACITY_PREFLIGHT` 320, immutable (`write_typed_value`), hashed into the capture evidence manifest like every pre-capture file.
- `storage_preflight.json` is now a **catalogued render** of the native report (`analytics/renders.rs::storage_preflight_view`, typed struct, byte-stable, never overwritten) for the pinned run store's `CONFIG_REQUIRED`; the former `p0-capacity --compat-json` (`serde_json::json!`) is gone. Transitional under JFI-FIX-04 until a run-store re-pin decision.

## Runs on this host (`runs.txt`, release binary, 64 MiB probe)

| Request | Verdict | start_allowed |
|---|---|---|
| 14,400 s, `off` | CAPACITY_UNVERIFIED (no basis, provider NOT_RUN, collections) | false |
| 14,400 s, `index` | CAPACITY_UNVERIFIED (index sizing needs a basis) | false |
| 14,400 s, `strict` | CAPACITY_UNVERIFIED (~36,000 slots context vs 2,000 calls; never strict-qualified unmeasured) | false |
| 600 s, `off` | CAPACITY_UNVERIFIED | **true** (calibration) |
| 14,400 s, `off`, basis root `data/econ4h` | CAPACITY_UNVERIFIED, 22 historical runs skipped `NO_JOURNAL` | false |

Measured on this host: sustained journal write 101.7–106.3 MiB/s, 76 syncs mean 4.6–4.8 ms / max 5.2–5.7 ms, 128 GiB physical, pressure level 1, swap 2.31 GiB, 87.9 GiB free.

## Tests

- `storage::preflight::verdicts_follow_measured_inputs_and_mode` — no basis long/calibration, mode sizing (off never blocked by strict, 4 h strict with 2,000 calls BLOCKED, index refused/qualified by budget, strict qualified with measured per-slot cost), disk/RAM/write/sync blocks.
- `storage::preflight::basis_and_write_probe_are_measured` — basis from a real run directory, unsealed/other-source/empty runs skipped with reasons, rates and slot rate exact, probe sync statistics and cleanup, `run()` keeps the immutable report, render CREATED → MATCHES → mismatch refused.
- `tests/integration/test_measurement_plan` (launcher, release binaries) 4/4: 10 m and 61 s captures start through the new preflight, `storage_preflight.json` carries the duration.
- Clippy (runner lib, bins, tests) without findings in the changed files.
