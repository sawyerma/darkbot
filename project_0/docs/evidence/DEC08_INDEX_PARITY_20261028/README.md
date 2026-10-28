# DEC-08 parity gate — native INDEX vs. pinned Python verifier (2026-10-28)

**Verdict:** source parity **LOCAL_PASS** on the full A4 scope: 2,917 sources, 2,896 identical, 21 explained (I2/I3/I4/I6), **0 unexplained** (`differential_index.txt`). Scope proven at the pin: the unchanged A4 differential over the same files reproduces `compared=2917 kinds={'TX': 100, 'LOGS': 2816, 'TRUNCATED_LINE': 1}`, 0 unexplained (`differential_a4_scope.txt`; E6 is the harness's later related-instruction rule). Every source and the pinned verifier are hashed in `sources.sha256` (historical rows read read-only from the 2026-10-11 backup of `project_0/data`; not committed).

Compared per transaction: pinned `detect` (classification, program events per kind, events) vs. `index::classify`; per live logs notification: the pinned import's LIVE_PAYLOAD events (`decode_logs` without errors) vs. `index::live_payload`. CI re-runs the committed part (6 real transactions + 48 mutations, 54 sources) in `tests/dec08_index_parity.rs`.

```
cargo build -p darkbot-runner --features historical --bin p0-parity
python3 -I operations/qualification/parity_differential.py native/target/debug/p0-parity index \
  tx docs/evidence/AUDIT_20261005/<6 signature files> \
  -- rows data/econ4h/*/market/transactions.jsonl data/market/*/transactions.jsonl \
  -- mut docs/evidence/AUDIT_20261005/<6 signature files>
```

| Rule | Count | Why |
|---|---|---|
| I2 = E2 | 6 | undecodable `err`: native UNKNOWN by contract, pinned FAILED; never success, no events |
| I3 = E3 | 6 | missing inner instructions: native fails closed (CPI coverage unprovable); counts agree |
| I4 = E4 | 6 | truncated logs: identical events and counts; native qualifies by instruction parity |
| I6 | 3 | successful transaction without loaded addresses: pinned stops at key resolution; native counts the program-attributed log events as observed only (never qualified) |

Found by the gate and fixed before the pin: native counted log events of a transaction without proven success (missing/undecodable `err`) as observed; now only a successful transaction's logs are observed (rolled-back effects are never counted).

## Decision parity (`tests/dec08_index_parity.rs`, CI)

The pinned `Audit.execute()` (import, paging, fetch, `compare`, `report`; `operations/qualification/index_decision_parity.py`, fixture opener, no network) and the native `index::run` read the same typed run — its pinned views rendered from the typed journals — and the same provider files; canonical lines (status, scope, qualified/observed per kind, comparison counts, BOT_ONLY, per-event detection/Shadow/source statuses) must be identical. Scenarios: verified window (live MATCH, miss within the observed interval, failed transaction, BOT_ONLY, exact Shadow join); page budget with a proven outage and identity-less Shadow; history exhausted before the start; `null` transaction and resume; pruned transaction and resume.

| Rule | Why |
|---|---|
| D1 | a pruned answer (-32009) ends the pinned attempt (`RPC_PROVIDER_ERROR`, rest unread); native keeps the transaction unresolved and works the window — both HISTORICAL_INCOMPLETE, both resume to identical verified lines |
| D2 | the pinned import reads live logs at `raw.value(.meta)`; a rendered `transactionSubscribe` row nests them under `raw.transaction`: only native matches the live transaction's own payload events (`LIVE_PAYLOAD=MATCH` entries; detection statuses unaffected) |

Found by the gate and fixed: an opportunity row keyed by its winner alone (the pinned rows nest the winner's signature in its identity) — native now keys Shadow opportunities by their strongly joined market event only, as the pinned import.
