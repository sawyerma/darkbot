# B1 part 1 — typed, segmented Shadow-evaluation lane and byte-bounded econ retention (2026-10-17)

**Verdict:** LOCAL_PASS in scope. Real-load sizes NOT_MEASURED. Still schema 1 (JFI-FIX-05 rest): `ECON_SNAPSHOT`, `ECON_FINAL`, `ECON_OPPORTUNITY`, `ECON_CANONICAL` (report values, rendered for the pinned report reader) — next with B7.

## Changes

- **JFI-FIX-05 (part):** `ECON_SHADOW_EVALUATION` is a typed payload `ShadowEvaluation { previous: Option<EvalRef>, record: OpportunityRecord }` (schema 2), written without any JSON value; removed from `LEGACY_WRITABLE` (a schema-1 write is refused). Historical schema-1 evaluation frames stay readable for offline replay (`econ_canonical` handles both).
- **SK-07 for the Shadow lane:** the evaluation journal rotates at `econ_segment_mib` (default 256 MiB, `P0_ECON_SEGMENT_MIB`); each target's on-disk chain links `EvalRef { segment, offset }` (never forward), read through per-segment handles; arrival order is the global provenance sequence, not the per-file sequence. Offline replay reads the whole chain.
- **SK-03 rest:** one byte budget `econ_retention_mib` (default 1024 MiB, `P0_ECON_RETENTION_MIB`) for every retained collection of the analytics worker (market events, canonical events, slots, Shadow heads and entries, winners, checkpoints). Each admission reserves the entry's typed (bincode) size plus a fixed node allowance before it is retained; removals release exactly the accounted bytes; admission beyond the budget is refused and counted as retention overflow (technical INCOMPLETE) while evaluations keep streaming to disk. Telemetry adds `retention_bytes_current/peak/byte_limit`.
- **SK-06:** `bounded_collections` is now QUALIFIED with the configured byte budgets, and `physical_memory` requires capture max + econ retention + system reserve.

## Tests

- `econ::retention_is_byte_bounded_and_released_exactly` — finite typed sizes, refusal at the limit with overflow recorded and streaming intact, peak ≤ limit, the worker's byte ledger equals the sum recomputed from every collection after a winner recorrelation.
- `econ::million_revisions_stream_every_row_with_bounded_semantic_retention` — 1,000,000 evaluations stream as typed schema-2 frames across more than one linked segment, every one exactly once; semantic retention unchanged.
- econ 49, segments 2, writer rotation 1, journal/evidence 43, preflight 2; Python launcher integration 26/26 (real launcher journals verify through the chain).
