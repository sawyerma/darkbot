# SK-07 segmented `.p0j` journals — 2026-10-17

**Verdict:** SK-07 **LOCAL_PASS in scope (market lane)**; the Shadow-evaluation lane is still one file (its offset index is reworked with B1); real-load segment counts NOT_MEASURED.

## Design (no second codec, no format change)

- `storage/segments.rs`: `SegmentedWriter` rotates a journal between batches when the current segment reaches `market_segment_mib` (operational config, default 256 MiB, env `P0_MARKET_SEGMENT_MIB`). The first segment keeps the base name (`market.p0j`), successors are `market.000002.p0j`, … Each segment is a complete sealed journal of the one contract. Before sealing, a segment gets a typed `SEGMENT_NEXT` (331) frame naming its successor; the successor's first frame is a typed `SEGMENT_LINK` (330) with the predecessor's file name, index, frame count, sealed chain root (hash of its manifest frame) and the cumulative event count, under one random set id. Sealed segments are never touched again.
- `ChainReader` replays the chain in order and returns only event frames. Fail closed: `SEGMENT_SUCCESSOR_MISSING`, `SEGMENT_LINK_MISSING`, `SEGMENT_LINK_MISMATCH` (foreign/swapped segment, wrong root or count), `SEGMENT_INTERMEDIATE_UNSEALED`, `SEGMENT_DATA_AFTER_NEXT`, `SEGMENT_ORPHAN` (continuation file beyond the chain end), plus every codec error (CRC, chain, sequence) with the segment index. An unsealed last segment (crash/power loss) replays its intact prefix and reports `sealed = false` → INCOMPLETE; nothing is repaired or rewritten.
- Writers/readers switched: market writer lane (`MarketJournalWorker`, `StorageMetrics.segments`, binding schema `MarketJournalBinding/4`); offline renders and `journal::export_views`; capacity-preflight basis (chain heads only); `p0ctl run status`; `p0-journal verify` (chain verified first, `segments` reported) and `frames`. Strict batch journals stay single files on purpose (`segment_bytes = 0`; a checkpoint binds the batch file hash; a segmented batch is refused `BATCH_JOURNAL_SEGMENTED`).

## Tests

- `segments::segmented_replay_equals_single_journal` — the chain replays exactly the frames (order, payload, provenance) of the same rows written unsegmented; segment and event counts.
- `segments::crash_missing_corrupt_and_orphan_segments_fail_closed` — unsealed last segment readable and INCOMPLETE; missing successor, corrupted middle segment, foreign segment swapped in, orphan beyond the end: each refused with its reason.
- `writer::lane_rotates_segments_and_chain_replays_every_frame` — the real writer lane rotates (> 2 segments) and every admitted frame replays once and in order.
- Affected suites: storage/journal/market/verification 144 + 1, Python integration `test_run_store` + `test_measurement_plan` 26/26 (incl. `p0-journal verify` status of real launcher journals and renders across chains).

**Corrected stale test expectation:** `test_run_store` expected market JSONL views right after a capture; since A2 (`1dd7605`) the capture writes no JSONL and views are created only by `p0-journal renders --create`. The test now requires MISSING-or-matching before, explicit creation, and byte-identical MATCHES afterwards.
