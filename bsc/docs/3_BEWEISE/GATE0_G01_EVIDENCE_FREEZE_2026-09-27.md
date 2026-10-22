# Gate 0 — G0.1 BSC Evidence Freeze

Status: **G0.1 IN PROGRESS — STRONG ON-CHAIN EVIDENCE PRESENT; OPENFOUR LAUNCH FIXTURE + FINAL MODE FREEZE STILL OPEN**
Date: 2026-09-28

This file records the current G0.1 evidence. It is not Gate-0 PASS and does not authorize value-bearing tests.

## 1. Scope inventory

Three distinct current BSC protocol families are relevant and MUST NOT be conflated:

1. Flap Portal
2. Four.Meme Classic / TokenManager2
3. Four.Meme OpenFour

G0.2 must compare candidate modes and freeze exactly one:

`PRIMARY_GATE0_SCOPE = venue family + deployment/version + launch mode + BUY method/mode + quote mode`

PASS never transfers automatically to an untested family/mode.

## 2. Local read-only evidence collected on 2026-09-27/28

The Mac evidence run reported:

- BSC chain id `0x38 = 56`;
- reference block `124414293`, hash `0x58c1892369c7544d6ed4d2ee38f54f0295114816de0fe2668cd610cb45811def`;
- later reference block `124414743`, hash `0x827501e0a628f4427ec213709e1e858110283eeb71dbcdece9b07b2a39096c51`;
- no key was read, no transaction was signed and no transaction was sent by DarkBot during G0.1;
- RPC wall-clock reads around 0.5–0.9 s are evidence-fetch times only and are **not** DarkBot latency measurements;
- no `T_signal`, `T_ready`, `T_queue`, `T_send` or `T_buy` latency has yet been measured.

### Four Classic live evidence

- TokenManager2 `0x5c952063c7fc8610FFDB798152D69F0B9550762b`;
- proxy implementation observed as `0xf32ee25a7a94f5858692b90815493bda9f992eee`;
- bytecode cross-read through two public BSC RPCs matched;
- real `createToken(bytes,bytes)` launch:
  - tx `0x8cc24c0cb9f40ee92240959fbafece1a2fc22f2e367b42366050a33cba827e5b`
  - block `124415333`
  - block hash `0xbcf5ab6e8e741e11acb17495d58a05af93f2ce75d6acf1e4ffbd3fa253a94abd`
  - transaction index `43`
  - receipt status `1`
  - selector `0x519ebb10`
  - decoded token `0xa89a24209a3ae8ba1ffb3f0567b69ef4765bffff`
  - decoded creator `0x1985ec0215a94db223f5ae98b436074c4be1219b`
  - decoded requestId `103111172`
  - decoded symbol `YING`;
- Helper3 `0xF251F83e40a78868FcfA3FA4599Dad6494E46034` latest-state probe returned version `2` and the exact TokenManager2 address;
- historical Helper3 read failure with `missing trie node` is classified as public-RPC archive limitation, not protocol evidence.

### OpenFour live evidence

- Registry `0x912CEf0C3aE9Ab6eB3Ec87cab69371cFb317Ab94`;
- current `openFourCore()` -> `0xebe7b6c1089d9f72ad07f34e36d898e44e5e27f3`;
- current `openFourTool()` -> `0x97da82f56fef1a0b974587a48135fa9da2a4e905`;
- current `feeRouter()` -> `0x7383b244b416db16aa9dd0ab7c296aa60255b9a2`;
- current `wrappedNative()` -> WBNB `0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c`;
- EIP-1967 implementation slots observed:
  - Registry -> `0x9d6f9e62a2ffe33ce81bd1f291bdea74e3a9dc6b`
  - Core -> `0x42a7378dcbc7dcd644b5c9b196c5451f847cbedc`
  - Tools -> `0x7eca56ed477345ad17cf327f3b54fa5226434b98`;
- official OpenFour source commit pinned at `0527d529d59c6a3fbc9927ce391ecc97c66e1b01`;
- official ABI confirms `createToken(bytes,bytes)`, `buy(...)`, `buyByBudget(...)`, `estimateBuy(...)` and `estimateBuyByBudget(...)`.

OpenFour still lacks a canonical real create/BUY fixture in the current G0.1 evidence set.

### Flap live evidence

- Portal `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`;
- proxy implementation observed as `0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20`;
- live `version()` returned **`v5.24.0`**;
- the public deployment page still showed `v5.23.1` at the time of the run; the live contract therefore overrides that stale documentation value for Gate-0 evidence;
- real `newTokenV6` launch:
  - tx `0x5b8cc9893c403a66d4f43bd73ed5c69227807661a58d1d6b5611def3f1d8352c`
  - selector `0x8cb5772c`
  - block `124415357`
  - transaction index `0`
  - decoded token `0xabe1fe16cf6bc48c9524713054efe6e954b17777`
  - name `Koyuki`
  - symbol `こゆき`
  - `tokenVersion = 6 = TOKEN_TAXED_V3`
  - `quoteToken = DOGE (Dogecoin), 8 decimals; previous WBNB label was wrong`
  - `quoteAmt = 80574062404`
  - transaction native value `0` (ERC-20 quote)
  - `buyTaxRate = 100` bps
  - `sellTaxRate = 100` bps
  - `mktBps + deflationBps + dividendBps + lpBps = 10000`
  - `dividendToken = DOGE`
  - `commissionReceiver = address(0)`;
- full ordering rescan covered four blocks / 211 receipts;
- earliest observed external BUY:
  - block `124415358`
  - transaction index `15`
  - tx `0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800`
  - execution path: on-chain router;
- the previously selected reference BUY was actually the **fifth external purchase**;
- later successful BUY fixtures remain useful for semantic/transfer evidence, but their previous "first"/"second" ordering labels are withdrawn.

This historical Flap fixture proves factual launch/BUY decoding and ordering for one real v5.24.0-era `newTokenV6 / TOKEN_TAXED_V3` launch. The earliest observed external BUY is the router transaction above; the previously used direct Portal reference is the fifth external BUY. It is not DarkBot's BUY and proves no local latency.

### Flap CREATE2 precomputability proof

The earlier CREATE2 mismatch was caused by a bug in the local verifier: the Portal/deployer address was incorrectly left-padded to 32 bytes.

EIP-1014 uses a **20-byte address** in the CREATE2 preimage:

`0xff ++ address(20 bytes) ++ salt(32 bytes) ++ keccak256(init_code)`

With the correct 20-byte Portal address, decoded V6 salt, verified EIP-1167 clone init code and embedded Tax Token V3 implementation, the predicted address exactly equals:

`0xabe1fe16cf6bc48c9524713054efe6e954b17777`

Cross-checks reported by the local verifier:

- seven EIP-1014 reference vectors: PASS;
- independent RPC hash cross-check: PASS;
- actual token runtime is EIP-1167;
- embedded implementation exactly equals `0x024f18294970B5c76c0691b87f138A0317156422`.

Therefore, for this exact v5.24.0 V6/TOKEN_TAXED_V3 deployment mode, **token-address derivation from the pending launch signal is PROVEN**.

This removes CREATE2/address derivation as the current Flap blocker.

The previously reported SHA-256 digests of deployed runtime bytecode may be retained only as supplemental byte fingerprints. They are **not** labeled EVM codehashes.

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

## 4. Flap — current documented + live surface

Official Flap sources inspected on 2026-09-27/28:

- Portal: `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`
- live on-chain `version()`: **`v5.24.0`**
- public deployment documentation observed one version behind at `v5.23.1`
- official repository `flap-sh/FlapVaultExample`, main observed at `5949cc7eb99bcb5ac5f679cc710ae456e627f12a`
- `src/flap/IPortal.sol` blob `f4d50e1580339125c40a7d032b6f8fca3da19663`

Current interface exposes multiple launch families, including `newTokenV6` and `newTokenV7`.

The real canonical fixture establishes that `newTokenV6` with `TOKEN_TAXED_V3` is an active launch path and that external successful BUYs follow such a launch.

Important semantics:

- launch params contain CREATE2 `salt` semantics;
- `quoteAmt` is an initial quote amount spent during creation, therefore `quoteAmt > 0` is `LAUNCH_INTERNAL_BUY`;
- external trading includes `swapExactInput`;
- extension-aware modes include `swapExactInputV3`;
- acquisition event surface includes `TokenBought`.

Remaining before Flap can be frozen as a G0.2 candidate scope:

- pin the exact v5.24.0 ABI/interface semantics for the selected `newTokenV6 / TOKEN_TAXED_V3` mode;
- freeze the exact external BUY path only after comparing the earliest observed on-chain router BUY with direct Portal `swapExactInput`;
- retain the proven pre-execution CREATE2 token-address derivation and fail closed on deployment/implementation/version mismatch;
- reconstruct the exact launch-internal `quoteAmt` effect in DOGE base units and post-launch initial curve state;
- identify extension/tax/quote-mode conditions and native-BNB -> DOGE conversion semantics that change the first external BUY;
- prove whether `minOutputAmount` can be derived locally from pending launch data without historical/synchronous RPC.

## 5. Four.Meme Classic — current documented surface

Official/community integration sources inspected at repository main `5f7f589b042e4e3b41c2c214d0fe3a81b36725a9`.

Published BSC addresses:

- TokenManager V1: `0xEC4549caDcE5DA21Df6E6422d448034B5233bFbC`
- TokenManager2 V2: `0x5c952063c7fc8610FFDB798152D69F0B9550762b`
- TokenManagerHelper3: `0xF251F83e40a78868FcfA3FA4599Dad6494E46034`

New Classic launches use TokenManager2/V2. The real captured launch fixture and Helper3 latest-state probe both support this routing for the sampled token.

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

Live resolution in the read-only run returned:

- Core `0xebe7b6c1089d9f72ad07f34e36d898e44e5e27f3`
- Tools `0x97da82f56fef1a0b974587a48135fa9da2a4e905`
- FeeRouter `0x7383b244b416db16aa9dd0ab7c296aa60255b9a2`
- WBNB `0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c`

Observed proxy implementations:

- Registry `0x9d6f9e62a2ffe33ce81bd1f291bdea74e3a9dc6b`
- Core `0x42a7378dcbc7dcd644b5c9b196c5451f847cbedc`
- Tools `0x7eca56ed477345ad17cf327f3b54fa5226434b98`

Current Core surface includes:

- `createToken(bytes,bytes)`
- `buy(address,uint256,uint256,uint256,bytes)`
- `buyByBudget(address,uint256,uint256,uint256,bytes)`

The documented creation flow contains `presaleQuote`, a creator initial quote budget. Nonzero `presaleQuote` is `LAUNCH_INTERNAL_BUY`.

Documentation notes that the API token-address placeholder may be empty before confirmation and that `TokenCreated` is the canonical final token-address source. G0.2 must still test whether the pending signed create payload plus selected preset/deployer semantics makes the address derivable before execution.

OpenFour presets/modules/quote modes are explicit scope dimensions. No Classic Four decoder or BUY assumptions may be reused without proof.

## 7. Remaining G0.1 requirements

The following are still required before `G0.1_COMPLETE`:

1. **OpenFour canonical fixture**
   - find and preserve at least one real current create transaction for the resolved Core;
   - decode `TokenCreated`;
   - preserve a real external BUY fixture for the same token when available;
   - pin the preset/module/quote mode represented by that fixture.

2. **Flap v5.24.0 exact mode freeze**
   - pin exact ABI/interface evidence for the real `newTokenV6 / TOKEN_TAXED_V3` fixture;
   - freeze the external BUY method/selector used by the selected fixture;
   - retain the exact CREATE2 address proof from pending V6 calldata and frozen deployment inputs;
   - archive-bind the reconstructed DOGE launch state/curve constants, native-BNB->DOGE conversion state and intervening invalidations;
   - token-address derivation is proven; conditional curve outputs reproduce exactly, but prove the same output from pending/pre-resident inputs only and bind exact fee/tax rounding, extension and router/direct path semantics.

3. **Vetted Keccak fingerprints**
   - generate runtime-code Keccak-256 fingerprints and any remaining selectors/topics with a vetted implementation;
   - retain independent cross-check evidence.

4. **Candidate-scope inventory**
   - record which Flap, Four Classic and OpenFour modes remain eligible for G0.2;
   - unsupported/unknown modes fail closed.

A separate Four Classic external BUY fixture is useful evidence but is **not required to close G0.1** if the selected BUY surface, ABI/selectors and launch semantics are frozen. Semantic `launch -> BUY` validity is tested in G0.3.

No DarkBot BUY transaction is required for G0.1.

## 8. G0.1 completion rule

Current state:

- Flap live deployment + fully decoded real `newTokenV6 / TOKEN_TAXED_V3` launch + exact CREATE2 + corrected first-BUY ordering + exact conditional curve reproduction: **STRONG PARTIAL; historical state provenance/router semantics/fee rounding/minOutput remain unresolved**
- Four Classic live deployment + real launch fixture + Helper3 routing evidence: **STRONG PARTIAL**
- OpenFour live Registry/Core/Tools/proxy freeze + pinned ABI: **STRONG PARTIAL; canonical launch fixture still missing**
- exact `PRIMARY_GATE0_SCOPE`: **NOT YET SELECTED** — selection belongs to G0.2 after G0.1 candidate evidence is complete

Therefore:

`G0.1_COMPLETE = false`

Next work:

1. obtain/decode canonical OpenFour launch fixture;
2. run the transaction-index-correct archive execution-witness replay defined in `docs/GATE0_G02_FLAP_ECONOMICS_BUYPATH_2026-09-28.md`;
3. generate remaining Keccak fingerprints with a vetted primitive;
4. close G0.1;
5. begin G0.2 preparation-matrix comparison and select `PRIMARY_GATE0_SCOPE`.

No value-bearing DarkBot transaction is needed before G0.7.
