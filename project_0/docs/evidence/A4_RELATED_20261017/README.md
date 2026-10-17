# RELATED_NON_LIQUIDATION — native typed rows and historical differential (2026-10-17)

**Verdict:** native related-instruction evidence **LOCAL_PASS in scope**; historical positive sample **BLOCKED** (`A4_NO_HISTORICAL_RAW_SAMPLE_RELATED`): none of the 52 historical full transactions (6 audit liquidations + 46 full-slot rows) contains one of the four instructions, so real positives need a separately permitted archive read of known signatures.

## What exists

- `market/related.rs`: the four PHASENPLAN L.5 instructions (`drift_claim_bad_debt`, `purge_deleverage_balance`, `marginfi_account_keeper_close_order`, `marginfi_account_init_liq_record`) detected by exact discriminator only (Anchor rule of the pinned entrypoints; a test binds each to the pinned audited instruction namespace and keeps them disjoint from the six markers). Each row: signature, slot, block index, outer index, CPI index, stack height, transaction status (SUCCESS/FAILED/UNKNOWN, never relabelled) and the account roles of the pinned Accounts structs (`33c67987`): drift — payer, bank; purge — account, risk_admin, bank; keeper — account, fee_recipient (not a signer); init_liq_record — account, fee_payer.
- New typed journal kind `MARKET_RELATED` (303). The strict full-slot scan writes one row per related instruction of every program transaction (success or failed) into the batch journal; the native batch reader and `p0-strict-verify --show` count them once per signature and position, **separately** from the four liquidation classes. They never reach `truth::classify`, the canonical join, economics or the winner path. Missing CPI groups or account indexes fail the block closed (absence unprovable).
- Live capture: `logs` carries no instruction data, so live related coverage is UNKNOWN by construction; live receipts join by signature after verification.

## Differential (`differential.txt`)

Same command and sources as [A4_PARITY_20261017](../A4_PARITY_20261017/README.md) with a `related=name@outer.inner:target:actor` field on both sides (pinned Python `detect` instruction evidence vs. Rust `related::decode`). 2,917 compared, 2,845 identical, 72 explained, **0 unexplained**. New rule E6: with `loadedAddresses` removed Rust reports related `UNKNOWN` (keys unresolvable) while the pinned Python stops before any instruction evidence (3 mutation lines); E3/E5 now also cover the `related` field under the same conditions.

## Tests

`related::discriminators_are_pinned_and_disjoint_from_markers`, `related::positions_roles_status_and_fail_closed` (top level and CPI, roles, failed status, foreign program, marker ignored, missing CPI groups, short account layout, typed round trip), `full_slot_related_instructions_are_recorded_but_never_liquidations` (production reconciler: failed transaction and CPI rows, canonical liquidation count unchanged); market + journal 135 and parity/verification 8 unchanged.
