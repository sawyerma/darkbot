> **Superseded findings, 2026-09-28:** The CREATE2 mismatch was caused by erroneous 32-byte deployer padding; the correct 20-byte formula exactly matches the token. The quote token is DOGE/8, not WBNB. The previous BUY fixture is the fifth external BUY, not the first. See [verified correction](../G02_FLAP_CORRECTION_2026-09-28.md) and `gate0-evidence/verify_flap_g02.py`. Historical sections below are retained as an audit trail.


## 9. G0.2 Flap V6 decode and address precomputability check (2026-09-28)

The earlier label `newTokenV7` for selector `0x8cb5772c` was incorrect. The calldata decodes exactly as `newTokenV6` with `tokenVersion = 6` (`TOKEN_TAXED_V3`). The complete decoded fixture is preserved in `gate0-evidence/2026-09-27-g01/flap_v6_fixture_decoded.json`.

Decoded facts:

- `dexThresh = 1`, `migratorType = 1`, `dexId = 0`, `lpFeeProfile = 0`.
- `quoteToken = 0xba2ae424d960c26247dd6c32edc70b295c744c43` (WBNB), `quoteAmt = 80574062404`; transaction value is zero, as expected for an ERC-20 quote.
- `buyTaxRate = 100`, `sellTaxRate = 100`, `taxDuration = 3153600000`, `antiFarmerDuration = 31536000`.
- `mktBps + deflationBps + dividendBps + lpBps = 10000`.
- `dividendToken = WBNB`, `commissionReceiver = address(0)`.

A local Keccak-256 implementation reproduced the documented EIP-1167 init-code hash for the current public BNB Tax Token V3 implementation `0x024f18294970B5c76c0691b87f138A0317156422`. Applying the documented `CREATE2(Portal, salt, init_code)` formula predicted `0xaf5b39fd975d2f1ea9d89ecd8d09676155cb818a`, while the receipt's token is `0xabe1fe16cf6bc48c9524713054efe6e954b17777`. Therefore the address is **not proven precomputable for this historical v5.24.0 fixture** using the currently published implementation/formula. The previous V7/standard-token attempt is discarded and must not be used as evidence.

This is a real blocker for the G0.2 pre-T0 claim: either the v5.24.0 deployment implementation/clone formula differs from the published v5.23.1 documentation, or another address/salt transformation is used. Until an on-chain bytecode/implementation snapshot or an exact v5.24.0 source path resolves this mismatch, Flap token-address precomputation remains `UNKNOWN/PARTIAL`.

The exact historical BUY calldata remains valid as a receipt fixture. Latest-state `eth_call` accepted the same `swapExactInput` calldata and returned a token amount, but this is not a historical parent-state simulation; public RPCs returned `missing trie node` for the required historical state. Consequently `minOutputAmount` and launch-initial reserve reconstruction remain unproven.
