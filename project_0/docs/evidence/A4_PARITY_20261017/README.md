# Ticket 3 A4 — historical differential: pinned Python verifier vs. Rust decode (2026-10-17)

**Verdict:** parity **LOCAL_PASS** in the covered scope (0 unexplained differences over 2,917 sources); class coverage **BLOCKED** (`A4_NO_HISTORICAL_RAW_SAMPLE_CLASSIC_DELEVERAGE_BANKRUPTCY_CPI`): no offline raw payload of those paths exists; it needs a separately permitted archive read of known signatures.

## Method

Both decoders read the **same provider bytes**, read-only, no RPC, nothing written into runs:

- 6 `getTransaction` results of real liquidations (`docs/evidence/AUDIT_20261005/<signature>.json`, v0 with 5 LUTs and v1);
- every historical market run under `data/econ4h/*/market/transactions.jsonl` and `data/market/*/transactions.jsonl`: the raw `full_slot_reconciliation` block entries (full transaction + meta) and raw `logsSubscribe` notifications;
- 48 documented mutations of the 6 real transactions (harness-generated from the raw bytes, never from Rust output): `meta_missing`, `err_missing`, `failed`, `err_undecodable`, `logs_truncated`, `logs_missing`, `loaded_missing`, `inner_missing`.

Python: `operations/qualification/parity_differential.py` imports the pinned `operations/verification/audit_market_quick.py` **unchanged** and decodes independently (`detect`, `decode_logs`). Rust: `p0-parity` (`market/parity.rs`) reuses `darkbot_solana::chain`, `truth::classify`, `decode_receiver_events` and the enrichment compute budget — no second classifier. Compared per source: status, fee, priority fee, version, fee payer, signature count, the six markers **in execution order with top-level/CPI position** (`name@outer.inner`, no sorting: start/end order, pairing structure and duplicates are compared as decided), events **in log order** (class, target, actor/receiver/winner, event index, exact seized atomic units), classification; logs: status, truncation, events. Reason codes of both sides are recorded on every line (`reasons=`; vocabularies differ, so they are reported, not equated). Undecodable inputs keep a line with their reason (never dropped).

```
cargo build -p darkbot-runner --bin p0-parity
python3 -I operations/qualification/parity_differential.py native/target/debug/p0-parity \
  tx docs/evidence/AUDIT_20261005/<6 signature files> \
  -- rows data/econ4h/*/market/transactions.jsonl data/market/*/transactions.jsonl \
  -- mut docs/evidence/AUDIT_20261005/<6 signature files>
```

## Discriminators

All six marker discriminators are bound to pinned sources (`parity::marker_discriminators_match_pinned_sources`): each equals the Anchor rule `sha256("global:<name>")[..8]` of the pinned program entrypoint (`marginfi-v2` @ `33c67987`, `programs/marginfi/src/lib.rs`), and `start_liquidation`, `end_liquidation`, `start_deleverage` (`[10,138,10,57,40,232,182,193]`) and `end_deleverage` equal `marginfi_type_crate::constants::ix_discriminators` (checked against the IDL inside the pinned source). `start_liquidation`/`end_liquidation` also equal `darkbot_project0::protocol::ix`.

## Result (`differential.txt`, complete; nothing capped)

compared 2,917: 100 full transactions (52 historical + 48 mutations), 2,816 logs notifications, 1 truncated historical line (a killed capture's partial last line, reported identically, left unchanged). **2,848 identical, 69 explained, 0 unexplained.** Accepted rules (each requires every other field to agree):

| Rule | Count | Why |
|---|---|---|
| E1 pinned Python blind to v1 priority | 39 alone, +12 combined | Rust reads v1 `transactionConfig.priorityFee`; for every case `meta.fee = priority + 5,000 × signatures` (checked by the rule) |
| E2 undecodable error | 6 | user contract: undecodable `err` is UNKNOWN (Rust); pinned Python says FAILED; neither is success, no events |
| E3 missing inner instructions | 6 | Rust fails closed (CPI coverage unprovable); pinned Python treats them as empty |
| E4 truncated logs | 6 | identical events; Rust qualifies by instruction parity (r6), pinned Python marks UNKNOWN |
| E5 missing meta / err | 12 | status, class and events agree (UNKNOWN, none); pinned Python stops before fee/version/payer/markers, Rust keeps the message/meta facts |

## Coverage

| Path | Covered |
|---|---|
| Versions | v0 (22 + mutations, incl. 5-LUT), v1 (30 + mutations); **legacy: no raw sample** (unit tests only) |
| Status | SUCCESS, FAILED (historical); UNKNOWN via meta/err/undecodable mutations |
| Classes | RECEIVERSHIP (8 transaction events, 1 logs event); **CLASSIC, FORCED_DELEVERAGE, BANKRUPTCY_SETTLEMENT: BLOCKED (no offline raw sample)** |
| Markers | `start_liquidation`, `end_liquidation` top-level; **other four markers and CPI positions: no raw sample** (discriminators verified against pinned sources) |
| Truncation / decoder | 116 truncated historical logs + mutations; missing logs, loaded addresses, inner instructions via mutations |

## Independence boundary

Independent: decoders and classification code. Shared: one provider's bytes; Anchor instruction names (pinned source); event layouts from the same pinned source. New typed captures carry no provider JSON, so this parity uses historical provider bytes only.
