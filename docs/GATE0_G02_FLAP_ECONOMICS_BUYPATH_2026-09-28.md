# Gate 0 — Flap G0.2 Economics + BUY-path evidence

Status: **G0.2_FLAP_PARTIAL — G0.3 BLOCKED**
Date: 2026-09-28

This file records the verified G0.2 Flap economics/BUY-path findings from the read-only/offline evidence run. It does not authorize signing, sending or value-bearing tests.

## 1. Reproducible verifier

Local evidence verifier:

`python3 gate0-evidence/verify_flap_g02_economics.py`

Reported result:

- exit code: `2`
- status: `G0.2_FLAP_PARTIAL`
- CREATE2 proof: PASS
- vetted selector/ABI proof: PASS
- full router command semantics: OPEN
- pending-only initial-state derivation: OPEN
- independent conversion pre-state: OPEN
- exact fee/tax rounding source: OPEN
- full pending-only first-BUY reproduction: OPEN
- `minOutputAmount`: UNKNOWN

The verifier treats unresolved evidence as unresolved; it does not turn conditional numerical agreement into Gate PASS.

## 2. Earliest observed external BUY

A four-block / 211-receipt rescan identified:

- tx: `0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800`
- block: `124415358`
- transaction index: `15`
- status: success
- sender/final token recipient: `0xe6b55031e42587e540c629807a059ff7d1a849b6`
- router destination: `0x43dd5eebeebead726fbc84e2511567d9827ae1bb`
- top-level selector: `0x24856bc3`
- ABI-compatible envelope: `execute(bytes,bytes[])`
- native value: `0.03 BNB`
- command bytes: `0x0b090816`

The router contract identity/dispatcher semantics are not source-bound yet. An ABI-compatible envelope is not sufficient to name the router implementation.

Observed asset flow:

```
EOA
 -> 0.03 BNB Router
 -> 0.00015 BNB observed router fee
 -> 0.02985 BNB wrapped to WBNB
 -> WBNB/DOGE pool 0xac109c8025f272414fd9e2faa805a583708a017f
 -> 24051320534 DOGE units to Router
 -> same DOGE amount to Flap Portal
 -> 6243568333969093909994622 new-token units to Router
 -> same token amount to final EOA
```

The observed router path is direct on-chain execution; it is not a frontend dependency.

## 3. Direct Portal BUY evidence

Reference direct BUY:

- tx: `0x8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b`
- block: `124415360`
- transaction index: `17`
- this is the fifth external BUY, not the first
- destination: Portal `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`
- selector: `0xef7ec2e7`
- signature: `swapExactInput((address,address,uint256,uint256,bytes))`
- `inputToken = address(0)`
- `outputToken = 0xabe1fe16cf6bc48c9524713054efe6e954b17777`
- native value: `0.15 BNB`
- permit data: empty
- actual acquired output: `28438305464314215025968609` token base units

Its observed native-to-DOGE path differs from the earliest router BUY and reaches a PancakeSwap-V3-compatible pool:

`0xce6160bb594fc055c943f59de92cee30b8c6b32c`

The pool emitted a swap delivering `120878323272` DOGE units to the Portal. Latest read-only pool getters returned fee `2500 ppm` and tick spacing `50`; latest state is not treated as the historical pre-state.

## 4. Launch / initial-state reconstruction

Frozen launch scope:

- Portal v5.24.0
- `newTokenV6`
- `TOKEN_TAXED_V3`
- token `0xabe1fe16cf6bc48c9524713054efe6e954b17777`
- quote token DOGE: `0xba2ae424d960c26247dd6c32edc70b295c744c43`
- DOGE decimals: `8`
- `quoteAmt = 80574062404`
- `buyTaxRate = 100 bps`
- `sellTaxRate = 100 bps`
- dividend token = DOGE
- token address CREATE2 derivation = PASS

Conditionally reconstructed post-launch state:

- reserve: `78962581156`
- circulating supply: `21453245135239692933310825`
- `r = 39956880715754427756773`
- `h = 107036751000000000000000000`
- `k = 44233735407663336217722200164523`
- DEX threshold: `800000000000000000000000000`

These values reproduce the historical arithmetic but their complete derivation from only pending launch calldata plus previously frozen/pre-resident state is **not yet proven**.

The V6 launch contains a nonzero `extensionID`; historical extension binding/semantics remain open.

## 5. Conditional curve reproduction

Using the observed curve constants and actual observed DOGE fee/tax deductions, the creator acquisition plus all six examined external BUY outputs reproduce exactly:

`conditional_expected_output - actual_output = 0`

including the first external BUY:

- actual: `6243568333969093909994622`
- conditional expected: `6243568333969093909994622`
- difference: `0`

This is strong arithmetic evidence for the curve model.

It is **not** yet the required pending-only proof because the calculation currently consumes some values learned from historical events/flows rather than proving that every required value is known or pre-resident before the BUY.

## 6. Fee/tax rounding evidence

Observed DOGE protocol fee and tax are separate deductions.

A naive rule that both are always `floor(input / 100)` is disproven. For multiple BUYs, observed tax is one DOGE base unit higher than that floor.

Examples:

- quote `111646524369`: fee `1116465243`, tax `1116465244`
- quote `120878323272`: fee `1208783232`, tax `1208783233`
- quote `67309479679`: fee `673094796`, tax `673094797`

The exact historical contract rounding/exemption rule is still not source/runtime-bound. Do not invent a rounding rule from these samples.

## 7. Current quote/config version gap

Latest quote-configuration reads produced words:

`[1, 41, 41, 7, 0]`

The saved interface defines the relevant native-to-quote mode only through values 0..6. Therefore the semantics of observed current mode `7` are unresolved.

Latest configuration must not be silently projected backward onto the historical fixture.

## 8. BUY-field status

Key status for the prospective Flap scope:

- destination: PRECOMPUTABLE
- direct Portal selector: PRECOMPUTABLE
- token address: SIGNAL_TIME_LOCAL_CALC / proven CREATE2
- input amount: PRECOMPUTABLE policy value
- recipient: PRECOMPUTABLE for selected path
- permit data: PRECOMPUTABLE for native-input direct path
- nonce/gas: pre-resident conditional runtime state
- DOGE conversion state: UNKNOWN until complete pool/converter state provenance is proven
- curve state: UNKNOWN until pending/pre-resident derivation is proven
- tax state: UNKNOWN until exact runtime/source rounding + exemption semantics are bound
- router command fields: UNKNOWN until dispatcher semantics are bound
- extension state: UNKNOWN until historical extension implementation/config is bound
- `minOutputAmount`: UNKNOWN

No synchronous RPC/HTTP lookup is accepted as the later BUY hot-path solution.

## 9. Decision

`G0.2_FLAP_PARTIAL`

This is not `G0.2_FLAP_BLOCKED` because no field has yet been proven inherently unavailable early enough.

This is not `G0.2_FLAP_READY_FOR_G0.3` because the exact historical execution/state provenance and complete pending-only economics are not yet proven.

`G0.3 = BLOCKED BY G0.2`

No BSC `T_signal`, `T_ready`, `T_queue`, `T_send` or `T_buy` latency is established here.

## 10. Exactly one next technical test

Run an **archive-backed, transaction-index-correct execution-witness replay** covering:

`launch 124415357:0 -> all intervening state changes -> first external BUY 124415358:15`

The witness must bind:

- historical runtime code/implementations;
- transaction-level pre-state, not only end-of-block state;
- full call tree for the earliest router BUY;
- actual Portal entry selector called by the router;
- exact quote/converter mode;
- historical V2 pool pre-reserves;
- curve `r/h/k` provenance;
- extension implementation/config and effects;
- fee exemption state;
- exact integer fee/tax rounding;
- every intervening transaction that can invalidate cached state.

The replay must then reproduce the first BUY from:

`pending launch facts + state that could have been frozen/pre-resident before the BUY + BUY calldata`

without feeding the observed BUY output back into the calculation.

BNB Chain documents archive nodes specifically for querying historical contract state, and its BSC Erigon archive configuration exposes `debug` and `trace` APIs. A normal pruned/public endpoint returning `missing trie node` is therefore an evidence-source limitation, not a protocol conclusion.

STOP after this test. Do not start G0.3 until its result is reviewed.
