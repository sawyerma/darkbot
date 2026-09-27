# Gate 0 — G0.1 BSC Evidence Freeze

Status: **PARTIAL PASS — DOCUMENT + ON-CHAIN EVIDENCE PRESENT; FINAL SCOPE FREEZE STILL OPEN**
Date: 2026-09-27

This file records the current G0.1 evidence. It is not Gate-0 PASS and does not authorize value-bearing tests.

## 1. Scope inventory

Three distinct current BSC protocol families are relevant and MUST NOT be conflated:

1. Flap Portal
2. Four.Meme Classic / TokenManager2
3. Four.Meme OpenFour

G0.2 must compare candidate modes and freeze exactly one:

`PRIMARY_GATE0_SCOPE = venue family + deployment/version + launch mode + BUY method/mode + quote mode`

PASS never transfers automatically to an untested family/mode.

## 2. Local evidence already collected on 2026-09-27

The Mac preparation/evidence run reported:

- BSC chain id `0x38 = 56`;
- reference block `124414293`, hash `0x58c1892369c7544d6ed4d2ee38f54f0295114816de0fe2668cd610cb45811def`;
- later reference block `124414743`, hash `0x827501e0a628f4427ec213709e1e858110283eeb71dbcdece9b07b2a39096c51`;
- Flap Portal `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`;
- Flap proxy implementation observed as `0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20`;
- Four Classic TokenManager2 `0x5c952063c7fc8610FFDB798152D69F0B9550762b`;
- Four Classic proxy implementation observed as `0xf32ee25a7a94f5858692b90815493bda9f992eee`;
- bytecode was cross-read through two public BSC RPCs and matched;
- a real Four Classic `createToken(bytes,bytes)` transaction was captured:
  - tx `0x8cc24c0cb9f40ee92240959fbafece1a2fc22f2e367b42366050a33cba827e5b`
  - block `124415333`
  - transaction index `43`
  - receipt status `1`
  - selector `0x519ebb10`;
- local Four evidence artifacts exist under the user's `gate0-evidence/2026-09-27-g01` workspace.

The previously reported SHA-256 digests of deployed runtime bytecode are retained only as supplemental byte fingerprints. They are **not** labeled EVM codehashes.

## 3. Keccak / ABI hashing rule

A hand-written Keccak implementation used during local preparation passed 22/24 generic vectors and failed the 135-byte and 271-byte boundary vectors.

Therefore:

- that implementation is REJECTED as a general Gate-0 crypto primitive;
- do not use it to establish selectors, topics, CREATE2 addresses, transaction hashes, or runtime-code Keccak fingerprints;
- use a vetted library/implementation and cross-check against an independent reference where practical;
- the 15 already cross-checked relevant Four selector/event signatures may be retained as evidence, but future derivations use the vetted primitive.

Known Classic Four signatures already cross-checked against BSC `web3_sha3` include:

- `createToken(bytes,bytes)` -> `0x519ebb10`
- `buyTokenAMAP(address,uint256,uint256)` -> `0x87f27655`
- `buyTokenAMAP(address,address,uint256,uint256)` -> `0x7f79f6df`

## 4. Flap — current documented surface

Official Flap sources inspected on 2026-09-27:

- BNB Portal documentation: v5.23.1
- Portal: `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`
- official repository `flap-sh/FlapVaultExample`, main observed at `5949cc7eb99bcb5ac5f679cc710ae456e627f12a`
- `src/flap/IPortal.sol` blob `f4d50e1580339125c40a7d032b6f8fca3da19663`

Current interface exposes multiple launch families, including `newTokenV6` and `newTokenV7`.

Important semantics:

- launch params contain CREATE2 `salt` semantics;
- `quoteAmt` is the initial quote amount spent during creation, therefore `quoteAmt > 0` is `LAUNCH_INTERNAL_BUY`;
- external trading includes `swapExactInput`;
- extension-aware modes include `swapExactInputV3`;
- acquisition event surface includes `TokenBought`.

G0.2 must choose an exact Flap launch/trade mode and prove:

- exact token-address derivation from pending launch calldata for that mode;
- exact initial state/quote data available before external tradability;
- exact external BUY call;
- whether extension/tax/quote modes change the first-BUY transaction.

## 5. Four.Meme Classic — current documented surface

Official/community integration sources inspected at repository main `5f7f589b042e4e3b41c2c214d0fe3a81b36725a9`.

Published BSC addresses:

- TokenManager V1: `0xEC4549caDcE5DA21Df6E6422d448034B5233bFbC`
- TokenManager2 V2: `0x5c952063c7fc8610FFDB798152D69F0B9550762b`
- TokenManagerHelper3: `0xF251F83e40a78868FcfA3FA4599Dad6494E46034`

New Classic launches use TokenManager2/V2.

Creation:

- `createToken(bytes,bytes)`
- documented `preSale` is creator pre-buy quote amount
- `preSale > 0` is `LAUNCH_INTERNAL_BUY`
- `TokenCreate` exposes the token address after creation

External BUY methods include:

- `buyTokenAMAP(address,uint256,uint256)`
- `buyTokenAMAP(address,address,uint256,uint256)`
- `buyToken(address,uint256,uint256)`
- `buyToken(address,address,uint256,uint256)`

G0.2 MUST NOT assume the future token address is derivable before execution. It must prove that from the selected pending create payload/current contract semantics or mark pre-execution address knowledge unavailable.

Generic Helper3 calls are not permitted as synchronous birth-BUY hot-path dependencies.

Mode dimensions requiring separate treatment include:

- native BNB vs BEP20 quote;
- AntiSniperFeeMode;
- TaxToken variants;
- creator preSale.

## 6. Four.Meme OpenFour — separate current protocol family

OpenFour is not Classic TokenManager2.

Official/community OpenFour repository inspected at main `0527d529d59c6a3fbc9927ce391ecc97c66e1b01`.

Published BSC Registry:

`0x912CEf0C3aE9Ab6eB3Ec87cab69371cFb317Ab94`

Current Core/Tools addresses are intentionally resolved dynamically via:

- `OpenFourRegistry.openFourCore()`
- `OpenFourRegistry.openFourTool()`

Current Core surface includes:

- `createToken(bytes,bytes)`
- `buy(address,uint256,uint256,uint256,bytes)`
- `buyByBudget(address,uint256,uint256,uint256,bytes)`

The documented creation flow contains `presaleQuote`, a creator initial quote budget. Nonzero `presaleQuote` is `LAUNCH_INTERNAL_BUY`.

Documentation notes that the API token-address placeholder may be empty before confirmation and that `TokenCreated` is the canonical final token-address source. G0.2 must still test whether the pending signed create payload plus selected preset/deployer semantics makes the address derivable before execution.

OpenFour presets/modules/quote modes are explicit scope dimensions. No Classic Four decoder or BUY assumptions may be reused without proof.

## 7. Remaining G0.1 live-read requirements

Before `G0.1_COMPLETE`, perform read-only live checks for all candidates that remain under consideration:

- latest chain/reference block and hash;
- non-empty deployed runtime code;
- proxy/implementation resolution where applicable;
- vetted Keccak-256 runtime code fingerprint;
- exact ABI/interface source commit/blob;
- current Flap `version()`;
- current OpenFour Registry `openFourCore()` and `openFourTool()`;
- Core/Tools runtime code fingerprints;
- exact candidate launch and BUY selectors/topics;
- at least one real canonical launch fixture per candidate family/mode that remains eligible.

Any mismatch reopens G0.1.

## 8. G0.1 completion rule

Current state:

- Flap document + partial on-chain evidence: **PARTIAL**
- Four Classic document + real launch fixture + partial on-chain evidence: **STRONG PARTIAL**
- OpenFour document evidence: **PARTIAL; live registry/core/tool freeze still required**
- exact `PRIMARY_GATE0_SCOPE`: **NOT YET SELECTED**

Therefore:

`G0.1_COMPLETE = false`

Next allowed work:

1. finish read-only OpenFour live freeze;
2. finish Flap exact selector/mode freeze and obtain real launch fixture;
3. use vetted Keccak/ABI hashing only;
4. then close G0.1 and begin G0.2 preparation-matrix comparison.

No BUY transaction is required for G0.1.
