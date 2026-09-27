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
- quote token = WBNB `0xba2ae424d960c26247dd6c32edc70b295c744c43`;
- `quoteAmt = 80574062404`;
- `buyTaxRate = 100`, `sellTaxRate = 100`;
- tax distribution sums to 10000 bps;
- real successful external BUY fixtures exist after that launch;
- canonical BUY fixture uses `swapExactInput` selector `0xef7ec2e7`;
- target-token transfer evidence exists for a successful BUY;
- current published Tax Token V3 CREATE2 recipe does **not** reproduce the actual fixture token address.

| BUY field/state | Verified source | Earliest availability | Signature-bound? | Precomputable before actionable signal? | Hot-path residual | Invalidation/rebuild condition |
| --- | --- | --- | --- | --- | --- | --- |
| Portal destination | live G0.1 freeze | startup | yes | yes | none | proxy/implementation/version change |
| BUY selector | canonical BUY fixture + ABI | startup | yes | yes | none | ABI/mode change |
| recipient | local wallet config | startup | yes | yes | none | wallet/config change |
| quote token / quote mode | decoded V6 launch | pending launch | yes | yes for this fixture mode | none/mode validation | launch mode mismatch |
| input amount/value | local policy + ERC-20 quote semantics | startup plus policy | yes | partial | patch/finalize amount | policy/state change |
| token address | V6 salt + actual deployment semantics | potentially pending | yes | **UNKNOWN/PARTIAL — published formula mismatches fixture** | unresolved | implementation/deployer/salt-transform/version mismatch |
| launch-internal purchase | decoded `quoteAmt` | pending launch | affects state | yes as input fact | state calculation | payload/mode change |
| tax configuration | decoded V6 params | pending launch | affects economics | yes as input fact | local calculation | payload/mode change |
| initial curve state | v5.24.0 V6 deployment + quoteAmt + extension/curve semantics | potentially pending | affects output | **UNKNOWN/PARTIAL** | reconstruct locally if proven | deployment/extension/state mismatch |
| min output / slippage floor | policy + exact first-external-BUY state | after initial state derivable | yes | **UNKNOWN/PARTIAL** | compute/patch if unavoidable | state/input/policy change |
| nonce | local sender state | continuously maintained | yes | partial/prepared | finalize lane nonce | competing/replacement tx |
| gas/fee fields | local policy + current head/fee state | continuously maintained | yes | partial | patch/finalize | head/policy invalidation |
| extension data | decoded launch extension ID/data | pending launch | may affect validity/state | partial | local mode handling | extension semantics/version change |

### Flap CREATE2 blocker

Current official `flap-skills` salt-finding reference uses:

`CREATE2(Portal, salt, EIP1167(TaxTokenV3 implementation))`

with Tax Token V3 implementation `0x024f18294970B5c76c0691b87f138A0317156422`.

For the historical fixture salt:

`0x0c5ae0f6ffd0a19acfb501ad8eb95542ce1ee50305987db5b6d388537a128e2c`

the published recipe predicts:

`0xaf5b39fd975d2f1ea9d89ecd8d09676155cb818a`

but the actual token is:

`0xabe1fe16cf6bc48c9524713054efe6e954b17777`

Therefore no pre-T0 token-address claim is allowed yet.

### Runtime result and required resolution order

Runtime inspection is complete:

- actual token `0xabe1...7777` is an EIP-1167 clone;
- embedded implementation exactly matches `0x024f18294970B5c76c0691b87f138A0317156422`;
- the implementation-address hypothesis is eliminated;
- the official Flap example repository's OpenZeppelin `Clones.sol` uses the same standard 55-byte EIP-1167 creation sequence as the published salt recipe.

Next:

1. Try `debug_traceTransaction` / equivalent trace for launch tx `0x5b8cc9893c403a66d4f43bd73ed5c69227807661a58d1d6b5611def3f1d8352c`, capturing CREATE2 creator/deployer, salt and creation input.
2. If public tracing is unavailable, use an archive/trace-capable BSC endpoint.
3. Freeze the Portal EIP-1967 implementation slot specifically at block `124415357`.
4. Analyze the historical implementation/deployment path to determine whether it transforms salt or delegates CREATE2 to another factory/helper.
5. Recompute the address from the **observed** CREATE2 preimage and require exact match to `0xabe1...7777`.
6. Only then reconstruct the post-launch initial state and `minOutputAmount` semantics.

Public historical RPC `missing trie node` means current public endpoints are insufficient for the needed historical-state proof; it does not imply the state is unknowable in production.

Current Flap assessment: **strongest first candidate, but G0.3 remains blocked until the V6 deployment/address and initial-state semantics are resolved**.

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
- token address derivation proven from pre-execution/pending data, or earliest unavoidable later derivation point explicitly proven;
- launch-internal purchase and initial curve state reproduced locally;
- exact external `swapExactInput` BUY semantics reproduced locally;
- every signature-bound BUY field classified as startup-precomputed, pre-signal-derived, signal-time-patched or unavoidable later input;
- no synchronous RPC/HTTP/helper call required in the local BUY hot path.

If these pass, freeze:

`PRIMARY_GATE0_SCOPE = Flap / BSC / Portal v5.24.0 / newTokenV6 / TOKEN_TAXED_V3 / exact swapExactInput mode / exact quote mode`

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
