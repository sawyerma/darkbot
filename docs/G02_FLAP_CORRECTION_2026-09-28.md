# Gate 0 — Flap G0.2 correction — 2026-09-28

Status: **EVIDENCE CORRECTION / NO PRODUCTION CODE CHANGE**

This note records three corrections to the Flap v5.24.0 / `newTokenV6 / TOKEN_TAXED_V3` evidence.

## 1. CREATE2 token-address derivation is proven

The earlier mismatch was caused by an incorrect local CREATE2 preimage: the Portal address had been padded to 32 bytes.

EIP-1014 requires:

`keccak256(0xff ++ address(20 bytes) ++ salt(32 bytes) ++ keccak256(init_code))[12:]`

Using the Portal as exactly 20 bytes, the decoded V6 salt, the standard EIP-1167 init code and the verified Tax Token V3 implementation:

- Portal: `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`
- Tax Token V3 implementation: `0x024f18294970B5c76c0691b87f138A0317156422`
- salt: `0x0c5ae0f6ffd0a19acfb501ad8eb95542ce1ee50305987db5b6d388537a128e2c`
- expected/actual token: `0xabe1fe16cf6bc48c9524713054efe6e954b17777`

Result: **exact match**.

The local verifier also passed seven EIP-1014 reference vectors and an independent RPC hash cross-check.

Conclusion for this exact frozen deployment/mode:

`pending newTokenV6 calldata + known Portal + known TaxTokenV3 implementation -> token address`

is locally computable before the launch executes.

## 2. Quote token is DOGE, not WBNB

The launch fixture quote token was previously mislabeled.

Read-only contract calls identify the quote token as:

- name: `Dogecoin`
- symbol: `DOGE`
- decimals: `8`

Therefore all launch-purchase and initial-state calculations for this fixture must use DOGE base units.

A later BUY funded with native BNB does not make the bonding-curve quote asset BNB. The BNB -> DOGE conversion semantics used by the selected on-chain BUY path are part of the exact G0.2 economics and must be reproduced locally.

## 3. Earliest observed external BUY correction

A complete scan of four blocks / 211 receipts found that the previously used reference BUY was not the first external purchase.

Earliest observed external BUY:

- block: `124415358`
- transaction index: `15`
- tx hash in local evidence: `0x189585be…926800`
- path: on-chain router

The previously used reference BUY is the **fifth external purchase**.

This does not introduce a frontend dependency. A router contract is still a direct on-chain execution path. However, Gate 0 must now compare the exact router path with direct `Portal.swapExactInput` rather than assuming the latter represents the earliest external BUY path.

## 4. Current G0.2 status

Proven:

- V6 calldata decoding;
- `TOKEN_TAXED_V3`;
- tax parameters;
- exact token-address derivation from the launch signal;
- real launch fixture;
- real external BUY ordering evidence.

Still open:

- exact post-launch initial curve state;
- effect of launch `quoteAmt` in DOGE base units;
- native-BNB -> DOGE conversion semantics for the selected BUY path;
- protocol/router fees and taxes relevant to the first external BUY;
- exact first-BUY output;
- whether the earliest router path and direct Portal path have equivalent readiness/semantics;
- exact BUY path to freeze in `PRIMARY_GATE0_SCOPE`.

Therefore:

`G0.2 = IN PROGRESS`

`G0.3 = BLOCKED BY G0.2`

No sub-millisecond BSC BUY or `T_send` result is proven by this correction.
