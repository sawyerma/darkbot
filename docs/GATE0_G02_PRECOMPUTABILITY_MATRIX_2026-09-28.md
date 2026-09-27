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

## 2. Candidate A — Flap Portal v5.24.0 / newTokenV7

Evidence:

- live Portal: `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`;
- live `version()` returned `v5.24.0`;
- real canonical `newTokenV7` launch fixture exists;
- real successful external BUY fixtures exist after that launch;
- canonical BUY fixture uses `swapExactInput` selector `0xef7ec2e7`;
- target-token transfer evidence exists for a successful BUY.

| BUY field/state | Verified source | Earliest availability | Signature-bound? | Precomputable before actionable signal? | Hot-path residual | Invalidation/rebuild condition |
| --- | --- | --- | --- | --- | --- | --- |
| Portal destination | live G0.1 freeze | startup | yes | yes | none | proxy/implementation/version change |
| BUY selector | canonical BUY fixture + ABI | startup | yes | yes | none | ABI/mode change |
| recipient | local wallet config | startup | yes | yes | none | wallet/config change |
| input token / quote mode | selected launch mode + launch calldata | pending launch if encoded there | yes | **partial / must prove** | mode branch or none | quote/mode mismatch |
| input amount / BNB value | local policy, constrained by venue semantics | startup plus launch semantics | yes | **partial** | patch/finalize amount | policy or mode/state invalidation |
| token address | CREATE2/salt semantics + selected implementation/deployer | potentially pending launch | yes | **UNKNOWN — must prove against v5.24.0 fixture** | derive/patch if proven | implementation/salt/deployer mismatch |
| launch-internal purchase state | `newTokenV7` params including `quoteAmt` and selected mode | potentially pending launch | affects validity/quote | **UNKNOWN/PARTIAL** | local state derivation | launch payload/mode change |
| initial curve state | selected v5.24.0 launch semantics | potentially derivable from launch payload | affects min output | **UNKNOWN/PARTIAL** | local curve calculation if needed | any state-changing launch-internal action |
| min output / slippage floor | local policy + exact first-external-BUY state | after required state is derivable | yes | **UNKNOWN/PARTIAL — do not assume no** | compute/patch only if unavoidable | state/input/policy change |
| nonce | local sender state | continuously maintained | yes | partial/prepared | finalize lane nonce | competing/replacement tx |
| gas/fee fields | local policy + current head/fee state | continuously maintained | yes | partial | patch/finalize | head/policy invalidation |
| extension/proof data | selected launch/trade mode | mode dependent | yes | unknown until mode freeze | possibly none or exact bytes | extension/mode change |

### Flap G0.2 questions that must be answered next

1. Decode the real `newTokenV7` fixture fully against the live v5.24.0 ABI.
2. Identify `salt`, `quoteAmt`, quote mode, tax/extension configuration and every field affecting first external BUY validity.
3. Reproduce the fixture token address from the pending launch inputs with the exact CREATE2/deployer semantics.
4. Reconstruct the post-launch/pre-external-BUY curve state locally.
5. Reproduce the first external BUY's executable output/cost semantics locally without synchronous RPC.
6. Determine whether `minOutputAmount` can be prepared before `T0` or needs a final local patch.
7. Freeze one exact Flap mode only after these proofs.

Current Flap assessment: **strongest first candidate, but not yet PRIMARY_GATE0_SCOPE**.

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

1. Flap v5.24.0 / real `newTokenV7` family — first to finish precomputability proof;
2. Four Classic / TokenManager2 V2 — second;
3. OpenFour — deferred until canonical launch evidence exists.

This is not yet a frozen `PRIMARY_GATE0_SCOPE`.

### Condition to freeze Flap as PRIMARY_GATE0_SCOPE

All of the following must be true:

- exact live v5.24.0 `newTokenV7` mode and ABI semantics pinned;
- real fixture fully decoded;
- token address derivation proven from pre-execution/pending data, or earliest unavoidable later derivation point explicitly proven;
- launch-internal purchase and initial curve state reproduced locally;
- exact external `swapExactInput` BUY semantics reproduced locally;
- every signature-bound BUY field classified as startup-precomputed, pre-signal-derived, signal-time-patched or unavoidable later input;
- no synchronous RPC/HTTP/helper call required in the local BUY hot path.

If these pass, freeze:

`PRIMARY_GATE0_SCOPE = Flap / BSC / Portal v5.24.0 / exact newTokenV7 mode / exact swapExactInput mode / exact quote mode`

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
