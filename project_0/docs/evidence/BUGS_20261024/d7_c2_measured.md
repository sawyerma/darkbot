# BUGS D7 (closed by measurement) and C2 (measured, stays open) — 2026-10-24

Offline, no Mainnet. Test commit: see the BUGS.md line (this file is part of the same commit).

## D7 — market keys and retention
- Bound and fail-closed: `r12_live_retention_cap_is_exact_and_fail_closed` (262,144 / 262,145),
  `r24_market_only_retention_is_configured_bounded_and_fail_closed` (configured `P0_MARKET_RETENTION`,
  replaces the former 100,000 fallback): beyond the cap INCOMPLETE, nothing evicted.
- New `d7_retained_transaction_keys_are_count_bounded_and_measured`: 262,144 real signatures (87
  base58 chars), exact hashbrown table + heap accounting of `seen` and `live_receipts`:
  **307 B per retained transaction, 76 MiB at the cap**; one transaction from a second slot/receipt
  stays one entry with its first receipt; the 262,145th marks `MARKET_RETENTION_OVERFLOW`.
- Projection: the largest real capture (A1: 1,682 unique tx in 4 h) retains ≈ 0.5 MB. Typed
  `Signature` keys would save ≈ 40 % of a worst case far above any observed load and touch the
  JSONL/wire key schema — not justified; no code change.

## C2 — health computed three times per pipeline candidate
- Measured (MEASUREMENT_LEDGER, 4 h run): `detect_health_calls=70,022`, `detect_health_ns_total=237.6 ms`
  ⇒ 3.4 µs average, 118 µs max. The two further computations (pipeline, sizer) cost ≈ 7 µs per
  candidate that reaches the pipeline. Sharing one view needs a state-generation-bound handoff
  (stale-view risk); deferred until a profile shows it on the critical path. Stays open.
