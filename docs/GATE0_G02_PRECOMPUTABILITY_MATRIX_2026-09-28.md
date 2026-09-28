# Gate 0 — G0.2 BSC Precomputability Matrix

Status: **DRAFT / EVIDENCE-DRIVEN — PRIMARY SCOPE NOT YET FROZEN**
Date: 2026-09-28

Normative parents:
1. `docs/GATE0_TESTPLAN_SHORT.md`
2. `docs/GATE0_G01_EVIDENCE_FREEZE_2026-09-27.md`
3. `docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md`

This matrix is a feasibility artifact. It does not authorize signing, sending or value-bearing tests.

## 1. Current implementation baseline

The current executable DarkBot codebase still has no native executable BSC BUY adapter.

- existing BSC chain/venue registrations are metadata/research-only;
- no current BSC Flap/Four BUY can be built, signed or handed to a production transport from DarkBot;
- existing Python/Solana/PAPER benchmarks are engineering evidence only, not BSC `T_send` evidence.

Current Mac regression/performance evidence supplied during Gate-0 preparation:

- 287 tests: PASS;
- 119 Python files syntax-compiled in memory: PASS;
- 22,109 benchmark frames;
- 300 ENTRY intents;
- writer failures: 0;
- overflow: false;
- receive -> decode p99: 0.859333 ms;
- receive -> decision p99: 2.545375 ms;
- decision -> enqueue p99: 0.078375 ms;
- receive -> enqueue p99: 2.992500 ms;
- receive -> enqueue max: 3.559292 ms;
- queue callback lateness p99: 0.756292 ms.

These numbers are from the existing Solana/PAPER path on macOS. They do not include BSC transaction construction, signing or real transport handoff and therefore do not satisfy G0.5.

## 2. Candidate A — Flap Portal v5.24.0 / newTokenV6 / TOKEN_TAXED_V3

Evidence:

- live Portal: `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`;
- live `version()` returned `v5.24.0`;
- real canonical `newTokenV6` launch fixture exists;
- selector `0x8cb5772c` decodes as `newTokenV6`;
- `tokenVersion = 6 = TOKEN_TAXED_V3`;
- quote token = DOGE / Dogecoin, 8 decimals;
- `quoteAmt = 80574062404`;
- `buyTaxRate = 100`, `sellTaxRate = 100`;
- tax distribution sums to 10000 bps;
- real successful external BUY fixtures exist after that launch;
- direct Portal BUY evidence uses `swapExactInput` selector `0xef7ec2e7`;
- target-token transfer evidence exists for a successful BUY;
- corrected EIP-1014 calculation reproduces the fixture token address exactly; the earlier mismatch was caused by incorrectly padding the 20-byte Portal address to 32 bytes.

| BUY field/state | Verified source | Earliest availability | Signature-bound? | Precomputable before actionable signal? | Hot-path residual | Invalidation/rebuild condition |
| --- | --- | --- | --- | --- | --- | --- |
| Portal destination | live G0.1 freeze | startup | yes | yes | none | proxy/implementation/version change |
| BUY selector | canonical BUY fixture + ABI | startup | yes | yes | none | ABI/mode change |
| recipient | local wallet config | startup | yes | yes | none | wallet/config change |
| quote token / quote mode | decoded V6 launch | pending launch | yes | yes for this fixture mode | none/mode validation | launch mode mismatch |
| input amount/value | local policy + ERC-20 quote semantics | startup plus policy | yes | partial | patch/finalize amount | policy/state change |
| token address | V6 salt + Portal + verified TaxTokenV3 implementation + EIP-1167 CREATE2 | pending launch | yes | **PROVEN YES** | local CREATE2 only | deployment/implementation/version change |
| launch-internal purchase | decoded `quoteAmt` | pending launch | affects state | yes as input fact | state calculation | payload/mode change |
| tax configuration | decoded V6 params | pending launch | affects economics | yes as input fact | local calculation | payload/mode change |
| initial curve state | v5.24.0 V6 deployment + quoteAmt + extension/curve semantics | potentially pending | affects output | **UNKNOWN/PARTIAL** | reconstruct locally if proven | deployment/extension/state mismatch |
| min output / slippage floor | policy + exact first-external-BUY state | after initial state derivable | yes | **UNKNOWN/PARTIAL** | compute/patch if unavoidable | state/input/policy change |
| nonce | local sender state | continuously maintained | yes | partial/prepared | finalize lane nonce | competing/replacement tx |
| gas/fee fields | local policy + current head/fee state | continuously maintained | yes | partial | patch/finalize | head/policy invalidation |
| extension data | decoded launch extension ID/data | pending launch | may affect validity/state | partial | local mode handling | extension semantics/version change |

### Flap CREATE2 proof — PASS for this exact scope

The previous mismatch was a verifier bug, not a protocol ambiguity. The CREATE2 deployer address must be encoded as exactly 20 bytes under EIP-1014; the earlier local calculation incorrectly padded it to 32 bytes.

Correct inputs reproduce the token exactly:

`pending V6 salt + Portal + TaxTokenV3 implementation + EIP-1167 init code -> 0xabe1fe16cf6bc48c9524713054efe6e954b17777`

Seven EIP-1014 reference vectors and an independent RPC-hash cross-check passed.

Therefore token-address derivation is no longer a G0.2 blocker.

### Corrected BUY ordering and path question

A four-block / 211-receipt rescan found the earliest observed external BUY at block `124415358`, transaction index `15`, via an on-chain router. The previously used reference BUY was the fifth external purchase.

This creates a mandatory G0.2 comparison:

1. decode the earliest router transaction completely;
2. identify router destination, selector, calldata and downstream Portal calls;
3. compare it with direct `Portal.swapExactInput`;
4. prove whether both paths are executable from the same pending-launch facts;
5. determine whether either path requires additional state/quote data;
6. freeze the exact on-chain BUY path used by `PRIMARY_GATE0_SCOPE`.

Router use here is still direct on-chain execution; it is not a frontend/API dependency.

### Remaining Flap G0.2 blocker: initial state and economics

The fixture's quote asset is DOGE with 8 decimals, not WBNB.

For a BUY funded with native BNB, exact local semantics must include the BNB -> DOGE conversion before/inside the selected Flap trading path. The official Flap example confirms that `Portal.swapExactInput` can take native BNB as `inputToken = address(0)` while buying a token whose reserve quote asset is an ERC-20, but Gate 0 must reproduce the exact conversion and first-BUY economics for this fixture.

Required next proof:

1. derive the post-launch DOGE reserve/circulating state from V6 launch parameters and `quoteAmt`;
2. freeze the curve parameters and native-to-quote conversion mode applicable to this token;
3. decode the earliest router BUY and the direct Portal reference BUY;
4. reproduce protocol fees, conversion fees, taxes and exact output locally;
5. determine a valid `minOutputAmount` without synchronous RPC in the hot path;
6. compare local results against historical receipts/transfers.

Current Flap assessment: **strongest first candidate; address derivation is proven, but G0.3 remains blocked until DOGE initial-state/economics and exact router-vs-direct BUY semantics are resolved**.

## 3. Candidate B — Four.Meme Classic / TokenManager2 V2

Evidence:

- live TokenManager2 `0x5c952063c7fc8610FFDB798152D69F0B9550762b`;
- real successful `createToken(bytes,bytes)` launch fixture;
- Helper3 latest-state probe reports version 2 and exact TokenManager2;
- official/current BUY surfaces and selectors are pinned.

| BUY field/state | Verified source | Earliest availability | Signature-bound? | Precomputable before actionable signal? | Hot-path residual | Invalidation/rebuild condition |
| --- | --- | --- | --- | --- | --- | --- |
| TokenManager2 destination | live G0.1 freeze | startup | yes | yes | none | deployment/proxy change |
| candidate BUY selector | pinned ABI/selectors | startup | yes | yes | none | selected overload/mode change |
| recipient | local config | startup | yes | yes | none | wallet/config change |
| token address | `TokenCreate` event after execution; pending derivation unproven | unknown | yes | **UNKNOWN** | derive/patch or wait for earliest valid fact | create semantics/deployer change |
| buy amount/funds | local policy + selected quote mode | startup/launch-dependent | yes | partial | patch/finalize | policy/mode change |
| initial curve state | create payload + contract semantics | potentially pending | affects quote | **UNKNOWN/PARTIAL** | local derivation if provable | preSale/fee/tax/mode change |
| min output / max funds | exact first-external-BUY state | state dependent | yes | **UNKNOWN/PARTIAL** | local calculation/patch | state/input change |
| AntiSniper fee state | selected token mode | launch-dependent | affects value/output | **UNKNOWN/PARTIAL** | local calculation | fee-plan/mode/head semantics |
| nonce | local sender state | continuous | yes | partial | finalize | competing/replacement tx |
| gas/fee | local policy/head state | continuous | yes | partial | finalize | head/policy invalidation |

Key blocker: prove or reject pre-execution token-address derivation from the pending Classic create transaction. Until then Classic cannot outrank Flap for the first full Gate-0 proof.

Current Classic assessment: **qualified candidate, second priority**.

## 4. Candidate C — Four.Meme OpenFour

Evidence:

- BSC Registry `0x912CEf0C3aE9Ab6eB3Ec87cab69371cFb317Ab94`;
- current Core `0xebe7b6c1089d9f72ad07f34e36d898e44e5e27f3`;
- current Tools `0x97da82f56fef1a0b974587a48135fa9da2a4e905`;
- current proxy implementations frozen in G0.1;
- official pinned ABI exposes create/buy/buyByBudget/estimate methods;
- bounded 64-block canonical search found no current Core launch fixture.

| BUY field/state | Current evidence | Precomputability status | Consequence |
| --- | --- | --- | --- |
| Core destination | live Registry read | known after registry freeze | must invalidate on Registry/Core change |
| candidate BUY selector | pinned ABI | statically known | semantics remain fixture-unproven |
| recipient | local config | known | generic only |
| token address | documentation says final `TokenCreated` canonical | unknown before execution | must prove from real create payload/preset/deployer semantics |
| preset/modules | no canonical fixture | unknown | cannot select exact BUY semantics |
| quote/min output | Tools/Core semantics only | unknown | cannot claim hot-path computability |
| nonce/gas | generic BSC state | partial | venue qualification missing |

Current OpenFour assessment: **not eligible for PRIMARY_GATE0_SCOPE until a canonical launch fixture is captured and decoded**.

## 5. Provisional G0.2 decision

Current evidence ranks the candidates for **order of proof only**, not as a performance winner:

1. Flap v5.24.0 / real `newTokenV6 / TOKEN_TAXED_V3` family — first to finish precomputability proof;
2. Four Classic / TokenManager2 V2 — second;
3. OpenFour — deferred until canonical launch evidence exists.

This is not yet a frozen `PRIMARY_GATE0_SCOPE`.

### Condition to freeze Flap as PRIMARY_GATE0_SCOPE

All of the following must be true:

- exact live v5.24.0 `newTokenV6 / TOKEN_TAXED_V3` mode and ABI semantics pinned;
- real fixture fully decoded;
- token address derivation proven from pre-execution/pending data — **already satisfied for the frozen V6 fixture**;
- launch-internal DOGE purchase, BNB->DOGE conversion and initial curve state reproduced locally;
- exact earliest-router and direct-Portal BUY semantics compared; selected on-chain BUY path reproduced locally;
- every signature-bound BUY field classified as startup-precomputed, pre-signal-derived, signal-time-patched or unavoidable later input;
- no synchronous RPC/HTTP/helper call required in the local BUY hot path.

If these pass, freeze:

`PRIMARY_GATE0_SCOPE = Flap / BSC / Portal v5.24.0 / newTokenV6 / TOKEN_TAXED_V3 / exact selected on-chain BUY path / DOGE quote mode`

and proceed to G0.3 minimal correctness-first Launch -> BUY harness.

## 6. What this matrix does NOT prove

It does not prove:

- a DarkBot BSC BUY;
- `T_signal -> T_send < 1 ms`;
- first external BUY placement;
- real pending coverage;
- builder superiority;
- profitable strategy/EV.

Those remain G0.3 through G0.11 evidence.
