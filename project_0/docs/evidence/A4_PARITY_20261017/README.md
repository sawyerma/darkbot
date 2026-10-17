# Ticket 3 A4 — historical differential: pinned Python verifier vs. Rust decode (2026-10-17)

**Verdict: PARTIAL** (`A4_HISTORICAL_SAMPLE_INCOMPLETE`). No PASS: the historical raw sample does not contain every class/marker/version path (see coverage).

## Method

Both decoders read the **same historical provider bytes**, read-only, nothing written into runs, no RPC:

- 6 `getTransaction` results of real liquidations (`docs/evidence/AUDIT_20261005/<signature>.json`, v0 with 5 LUTs and v1);
- every historical market run under `data/econ4h/*/market/transactions.jsonl` and `data/market/*/transactions.jsonl`: the raw `full_slot_reconciliation` block entries (full transaction + meta) and the raw `logsSubscribe` notifications.

Python side: `operations/qualification/parity_differential.py` imports the pinned `operations/verification/audit_market_quick.py` **unchanged** and uses its `detect` (full transactions) and `decode_logs` (logs). Rust side: `p0-parity` (`market/parity.rs`) uses `darkbot_solana::chain` decoding, `truth::classify`, `decode_receiver_events` and the enrichment compute budget. One canonical line per source; field-by-field comparison: status, fee, priority fee, version, fee payer, the six markers with instruction position (outer.inner), events (class, target, actor, event index, exact seized atomic units), classification; for logs: status, truncation, events.

```
cargo build -p darkbot-runner --bin p0-parity
python3 -I operations/qualification/parity_differential.py native/target/debug/p0-parity \
  tx docs/evidence/AUDIT_20261005/<6 signature files> \
  -- rows data/econ4h/*/market/transactions.jsonl data/market/*/transactions.jsonl
```

## Result (`differential.txt`)

- compared 2,869 sources: 52 full transactions, 2,816 logs notifications, 1 truncated historical line (a killed capture's partial last line, reported identically by both sides and left unchanged).
- **2,839 identical** in every compared field.
- **30 differ only in `priority`**: all are v1 transactions. Rust reads the v1 `transactionConfig.priorityFee`; the pinned Python verifier reads ComputeBudget instructions only and reports UNKNOWN (`-`) for v1. For all 30, `meta.fee − priority = 5,000` (one signature fee), i.e. the Rust value is consistent with the chain's own total fee. This is a limitation of the pinned Python verifier; it is not changed.

## Coverage of the historical sample

| Path | Covered historically |
|---|---|
| Versions | v0 (22, incl. 5-LUT fixture), v1 (30); **legacy: none** |
| Status | SUCCESS, FAILED; **absent `meta`/`err` (UNKNOWN): none** |
| Classes | RECEIVERSHIP (8 events); **CLASSIC, FORCED_DELEVERAGE, BANKRUPTCY_SETTLEMENT: none** |
| Markers | `start_liquidation`, `end_liquidation` (outer); **`lending_account_liquidate`, `start_deleverage`, `end_deleverage`, `lending_pool_handle_bankruptcy`, CPI (inner) positions: none** |
| Logs | 2,816 notifications incl. 116 truncated |

The uncovered paths are covered only by synthetic/unit fixtures (chain status table, legacy decode, truth classifier tests), which do not count as historical parity.

## Independence boundary (explicit)

- Independent: the decoders (Python regex/base58/`struct` vs. Rust serde/bs58/typed chain model) and the classification code.
- Shared: the provider bytes themselves (no second provider); the Anchor discriminator names (Python derives `sha256("global:<name>")`, Rust uses pinned byte constants for five markers and derives `start_deleverage`) — an IDL naming error would be shared; the event layouts follow the same pinned source.
- New typed captures no longer carry provider JSON; for them the pinned readers only see the Rust re-encoding (documented with A1/A2). This differential therefore uses historical provider bytes only.
