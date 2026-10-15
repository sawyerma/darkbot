# G0.2 Precomputability Matrix — 2026-09-28

This is a read-only feasibility matrix. It does not authorize signing or sending.

## Four Classic / TokenManager2

Scope evidence: real `createToken(bytes,bytes)` receipt in `canonical_fixture_scan/`.

| BUY field | Current evidence | Before T0? | Consequence |
|---|---|---:|---|
| token address | `TokenCreate` event after create receipt | **unproven** | Cannot assume pending-create derivation; must prove or wait for earliest valid chain fact |
| recipient | local account/config | yes | startup constant |
| buy selector | official ABI + selector `0x87f27655` for budget overload | yes | selector can be preloaded |
| buy amount/value | local policy + quote semantics | partial | value can be prepared; exact min-output depends on state |
| min output/slippage | curve state / quote | no, unless state snapshot is valid | must be rebuilt/invalidated on state change |
| nonce | local sender state | partial | maintained lane, never synchronous RPC on hot path |
| gas/fee | current head/policy | partial | cached and invalidated by head/policy change |
| destination | TokenManager2 `0x5c9520...762b` | yes | frozen only for this deployment/version |

Result: Classic is semantically evidenced for launch detection, but pre-T0 token-address knowledge is not proven.

## Flap Portal

Scope evidence: real `newTokenV6` (`TOKEN_TAXED_V3`) launch and `swapExactInput` BUY receipts. Live `version()` returns `v5.24.0`.

| BUY field | Current evidence | Before T0? | Consequence |
|---|---|---:|---|
| token address | V6 salt + Portal (20 bytes) + observed implementation reproduce TokenCreated exactly; 7 EIP-1014 vectors and RPC hash agree | **fixture proven from signal + cached implementation** | do not assume future implementation/cache freshness or pending visibility |
| recipient | local account/config | yes | startup constant |
| buy selector | `swapExactInput` `0xef7ec2e7` in canonical fixture | yes | selector can be preloaded |
| input token/value | launch quote/dividend is DOGE (8 decimals); later direct Portal BUY enters with native BNB | partial | separate input asset, DOGE reserve asset and conversion route |
| min output | historical parent-state quote unavailable; latest-state call only | **UNKNOWN/PARTIAL** | exact state-dependent field must be reconstructed or refreshed off hot path |
| nonce | local sender state | partial | maintained lane |
| gas/fee | current head/policy | partial | cached and invalidated |
| destination | Portal `0xe2ce...c9de0` | yes | implementation version must be frozen at v5.24.0 |

Result: V6 token address is reproduced exactly. The old mismatch came from invalid 32-byte deployer padding (97-byte preimage). First external BUY is router transaction `0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800`, block 124415358 index 15; previous `0x8aff…` fixture is the fifth external BUY. G0.2 remains open on initial state, DOGE quote/conversion, route and exact output reproduction. See `G02_FLAP_CORRECTION_2026-09-28.md`.

## Four OpenFour

Scope evidence: Registry/Core/Tools live resolution only; no launch fixture in the bounded 64-block search.

| BUY field | Current evidence | Before T0? | Consequence |
|---|---|---:|---|
| token address | integration guide says final `TokenCreated` is canonical | unknown | no primary scope until a real launch fixture exists |
| recipient | local account/config | yes | generic only |
| buy selector | official pinned ABI: `buy(...)` / `buyByBudget(...)` | yes | selector known, semantics not fixture-proven |
| quote/min output | Tools staticCall + Core state | unknown | not eligible for hot-path proof |
| nonce/gas | generic BSC requirements | partial | no venue qualification |
| destination | dynamically resolved Core | yes after registry read | current address can change via registry |

Result: OpenFour cannot support an implementation claim yet.

## Gate decision from this matrix

- Existing Phase-A Solana path: measurable and borderline at the 3 ms local enqueue gate.
- BSC implementation: not yet present in the codebase; BSC registries are metadata-only.
- Flap: first candidate for a minimal executable BSC proof after v5.24.0 ABI/mode freeze.
- Four Classic: second candidate; token-address precomputability remains the key blocker.
- OpenFour: keep separate and defer until a canonical launch fixture is captured.

No evidence here proves a sub-millisecond successful BSC BUY. That requires the BSC adapter, signing path, tested transport boundary and controlled execution evidence.
