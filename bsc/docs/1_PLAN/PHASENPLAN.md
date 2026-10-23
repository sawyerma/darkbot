# BSC PHASENPLAN — DarkBot Gate-0 First

Status: **ACTIVE AUTHORITATIVE ROADMAP**
Updated: 2026-10-23 (v8 A/B-Salt/Frontend amendment; all earlier Gate-0 freezes preserved)

## BINDING v8 AMENDMENT — 2026-10-23: two Salt profiles A/B and BSC frontend selector

**User decision (binding):** Implement both alternative salt strategies for the **own G0.7 self-launch test** and allow an independent A/B selection in the BSC frontend and native control plane. Both use **the same existing Rust engine, qualification, quote, risk, build/sign and G0.8 verifier**, never duplicated trading engines.

| Candidate | Source of Salt | Evidence and exact approval |
|---|---|---|
| **A / FROZEN_SALT** | Previously frozen, **publicly known** original salt; existing predicted token address | Preserve all immutable historical evidence; the previous Portal-v5.25.0 PASS is invalid for v5.26.0 and must be freshly requalified before any authorization. |
| **B / FRESH_RANDOM_SALT** | Locally generated CSPRNG salt, checked against the official CREATE2 vanity rule (Tax-V3 suffix 7777), never reusing the public deterministic sequence | Separate token address, calldata, signed hashes, matching DRY/economics/evidence and separately approved V4-or-successor fingerprint. Candidate is fixed once qualified, not silently rerolled on each run. |

**Comparison and timing:** A and B support real-code offline DRY/SHADOW evaluation with individually named binary records and equally controlled chain/Portal version, simulation parent state, gas, budget and timestamps. Compare predicted token identity, effective economics, gas, qualification verdict and measured local processing cost; neither variant intrinsically guarantees earlier acquisition. Do not invent builder acceptance, N/N+1, producer T0 or actual acquisition from offline runs.

**Frontend/control-plane contract:** Show a switch **SELF-LAUNCH CANDIDATE: A / B**, separately from five provider OFF/SHADOW/LIVE switches and global REPLAY/PAPER/SHADOW/LIVE + economic arming. Display selected profile, derived address, binding hashes, DRY/readiness, and eligibility. Do not leak a previously undisclosed B salt/private key into web UI, logs, URLs, telemetry, or ordinary status responses. Persist typed binary, generation-bound configuration. Switching profile changes exact candidate identity and invalidates stale readiness, signed-byte authorization and approval for the newly selected candidate; never silently carry them over, change active nonce/exposure, or authorize live signing/send. Disable unsafe switching while an economic intent is armed or executing; fail closed, no automatic A-to-B fallback.

**Narrow offline-scope exception to the prior no-GUI rule:** User expressly authorizes A/B candidate preparation and a frontend selector for **offline/shadow-only use** before G0.7. Do not implement unrelated GUI changes, C1/C2 source integration, hot-path callbacks, builder/live changes or a second runtime. If the selector cannot be isolated from live guard/nonce/signed bytes, retain the design as PLANNED and do not weaken Gate-0.

**Economic limits unchanged:** Current G0.7 guard remains UNARMED; approved=false; execution_ready=false. One-attempt spend cap 481163500000000 wei, existing exact candidate freezes and the G0.7 -> G0.8 -> PASS/ADAPT/STOP order remain. A/B offline testing does not authorize two actual Mainnet attempts, dual concurrent BUYs, re-salting after approval or any broadcast. Each eventual live attempt requires independent fresh qualification, exact signed-byte/hash and one-off express economic authorization. Portal drift and missing producer witness remain unresolved.

**WebSocket distinction for foreign launches:** A/B is **only our own self-launch testing choice**, not a provider/venue mode. For a foreign Flap/Four launch, a sufficiently early *complete pending launch transaction* may expose the launch calldata including the creator-chosen salt. A version-/deployment-pinned ABI decoder can extract it and calculate the eventual CREATE2 token address from correct Portal deployer, initcode and implementation; validate signed transaction identity and on-chain assumptions. If only a hash arrives, a private launch is hidden, the version is unknown or the observation is after T0, do not invent the address or claim pre-T0 discovery. No user-generated salt for foreign launches. The native trading path remains binary and the product goal successful producer execution T_buy - T0 < 1,000,000 ns remains unchanged.

## BINDING OFFLINE BUGFIX EXCEPTION — 2026-10-21

The user explicitly authorized offline corrections of E5–E8 in `bsc_bug_v1.md` despite the C1/C2 development stop below. This exception permits implementation and focused offline regression tests only. It does not qualify or activate C1/C2, change economic parameters, authorize a BUY/nonce burn, or arm G0.7. All production and economic gates remain binding.

## BINDING G0.7 CLARIFICATION — 2026-10-20: finish code before economic approval

**Immediate development objective: complete the existing G0.7/G0.8 code up to the explicit authorization/send boundary. The actual Mainnet BUY is NOT part of this implementation task.** This dated clarification supersedes prior guidance only where it conflates pre-BUY technical readiness with results that require a real future trial. The signed-byte freeze, all safety constraints and the mandatory G0.7 -> G0.8 -> PASS/ADAPT/STOP order remain unchanged.

- **Verified historical evidence:** 2026-10-20 R0 and DRY-0..DRY-4 passed against Flap Portal v5.25.0, with unchanged signed launch/BUY bytes and zero egress; see G07_REQUALIFICATION_RESULT_20261020.md. Do not rerun the long historical scan or an unchanged DRY without evidence of invalidation.
- **Technical pre-LIVE readiness:** PASS_READ_ONLY with exit 0 must be reachable after the mandatory technical conditions actually pass (current chain/Portal, bound V4 draft and immutable evidence, exact signed bytes, nonce, wallet/funding, gas and total budget, head/clock/expiry, one-attempt safety and applicable submission preflight). Genuine failures remain BLOCKED/STOP with exit 2. PASS_READ_ONLY is never economic approval or authority to send. The hard-coded BLOCKED-only code identified on 2026-10-20 must be corrected; code-readiness is not yet verified.
- **Builder outcome:** submission format, price-floor documentation and expiry acceptance have limited evidence. Builder inclusion, 48Club price enforcement and building-time effects of noMerge/positionFirst remain **UNQUALIFIED**, never PASS. These *future outcome risks* must not permanently prevent finishing the code. Where the exact V4 approval contract permits a bounded experiment, explicitly document the residual risks for informed user approval. Never bypass a genuinely mandatory pre-egress check or represent unknown inclusion as verified.
- **Producer execution witness:** source_ready=false means actual T0 -> T_buy and the sub-1-ms timing acceptance remain **UNKNOWN**. The missing original producer timing witness does not by itself prevent preparation of a controlled token-acquisition trial. Binding and integrity of the witness **source contract** remain mandatory.
- **Three separate states:** (1) technical PASS_READ_ONLY, (2) explicit exact-V4 user economic approval and live guard authorization, and (3) actual on-chain acquisition/ordering/timing evidence. No state implies the next. Preserve UNKNOWN/UNQUALIFIED as independent dimensions, not fabricated PASS.
- **Scope until code-ready:** finish existing native G0.7 runner, V4-approval preparation, one-attempt journal/anti-replay and immediate G0.8 acquisition/order/cost reconciliation. Only targeted regression tests for changed logic. No GUI, C1/C2, new venue, refactoring campaign, repeat R0/DRY or synthetic test campaign.
- **Economic boundary:** remain UNARMED, approved=false and execution_ready=false throughout this task. The max one-attempt reserve remains 481163500000000 wei = 0.0004811635 BNB. A separate explicit user approval of the exact **approved-variant V4 SHA-256**, bounded risk and one live attempt is required before any arming, signing for live submission or send. Keep exact hash/byte/chain/wallet/nonce/funding/gas/budget/freshness, evidence durability and anti-replay guards intact.

**Development milestone: G0.7 CODE READY — AWAITING EXPLICIT ECONOMIC APPROVAL. This is NOT G0.7 or Gate-0 PASS.**

## **ABSOLUTE STOP RULE — REAL MAINNET BUY BEFORE ANY MORE C1/C2 WORK**

> **DO NOT continue C1, C1.10 freeze, C2, C3, decoder work, optimization, refactoring or additional synthetic benchmarking before G0.7/G0.8.**
>
> **THE NEXT PRODUCT TEST IS THE REAL CONTROLLED BSC MAINNET BUY:** a value-bearing on-chain BUY must execute successfully, the intended token must actually arrive at the intended buyer, and acquisition/ordering evidence must be recorded.
>
> **Local latency, Testnet BUYs, geth/Fast-Lane measurements, builder ACKs, handoff timing and receipts without acquisition are NOT substitutes for this test.**
>
> Required next sequence:
>
> ```text
> G0.7  controlled minimal-value Mainnet BUY
>   -> G0.8  prove actual token acquisition + ordering + costs
>   -> PASS / ADAPT / STOP
>   -> only then resume C1 freeze / C2 production work
> ```
>
> C1 engineering may remain preserved at its current qualified checkpoint, but it is **PAUSED** until the real BUY test is executed and evaluated.

## BINDING DATA-PATH INVARIANT — BINARY HOT PATH / EXTERNAL PROTOCOL BOUNDARY (2026-10-16)

**Normative scope:** BSC Gate-0-to-production architecture. **JSON is prohibited in all DarkBot-owned internal formats and processing, including cold/control, configuration, authorization contracts, qualification, G0.7/G0.8 evidence and reports; the only protocol exception is unavoidable JSON-RPC on external provider/RPC wire boundaries.** This restriction does **not** authorize any live send, change the G0.6a frozen signed bytes, bypass the acceptance guard, reopen completed DRY evidence, or alter the mandatory G0.7 -> G0.8 -> PASS/ADAPT/STOP -> C1/C2 order. The sub-1-ms local reaction target and the separately witnessed T0 -> T_buy metric retain their existing definitions and acceptance gates.

### Mandatory runtime separation

| Boundary | Rule |
| --- | --- |
| Earliest peer/chain signal -> HOT0 -> classification/state -> quote/policy -> build/sign -> local dispatch/handoff | **Binary only internally:** borrowed/fixed-layout IPC records, typed native Rust state, direct ABI/RLP, preallocated buffers and bounded queues. No JSON parse/serialize, HTTP/JSON-RPC control lookup, WSS, child-process start, filesystem read/write/fsync, or synchronous report generation inside the measured decision/dispatch hot path. Do not move such work into an indirect helper called from it. |
| Builder 48Club / BlockRazor | **Only the externally required HTTP(S) JSON-RPC wire format is exempt.** This is not permission to store, produce or consume JSON as an internal DarkBot representation, in either the hot or cold path. Prepare exact bounded wire-byte templates and TLS connections outside the decision/dispatch hot path; patch only required fields without general JSON serialization. Decode externally received protocol responses at the transport boundary into typed native/binary outcomes; process and persist those outcomes without JSON. Preserve expiry, authentication, byte correctness and replay restrictions. |
| Pinned geth <-> Rust / signed BUY | Native P2P + binary IPC/egress and raw signed RLP; no WSS or JSON bridge as the primary production signal or BUY transport. |
| Live capture / evidence | The canonical runtime event record is a **versioned binary append-only journal** (existing `engine/crates/journal` owner) with ordering, clocks, identity, completeness/drop/error counters and crash-recovery semantics. Bounded asynchronous persistence must not block decision/dispatch. A safety-required durable fence must be completed before the economic release or fail closed; never silently weaken evidence durability to claim lower latency. Journals containing signed, still-executable bytes remain private and untracked. |
| Cold/control/offline | **No internal JSON exception.** Configuration, approval contracts and drafts, DRY/qualification inputs, bootstrap state, G0.7/G0.8 records, audit artifacts and internal interchange use typed, versioned **binary** formats. Human-readable Markdown/text views may be derived for inspection but never replace the canonical binary records. Historical hash-bound JSON files are preserved **immutable as legacy evidence only**, not renewed, generated or used as active execution authority; any required legacy import must be offline, explicitly audited, byte/field-verified and converted to the canonical binary format. Read-only external RPC JSON-RPC is permitted only on its mandatory wire boundary, never as internal storage or representation. |

### Phase-accurate implementation (no new engine or refactor loop)

1. **Now, G0.7/G0.8:** close the internal JSON compliance gap in the existing qualification harness, configuration/authorization contract, probe evidence and G0.8 verification with a **minimal, auditable binary migration before any live authorization**. Do not replace the tested BUY/signing algorithms or alter frozen signed launch/BUY bytes, existing DRY hashes or original historical evidence. Establish byte/field equivalence, guard fail-closed behavior, durable-recovery behavior, test parity and G0.8 reader compatibility with a few targeted regressions. Keep the guard `UNARMED` until a separate explicit approval of the final contract and single economic attempt; a changed approval contract needs its own new reviewed binding. There is **no internal JSON exemption for G0.7/G0.8**. Historical JSON remains immutable evidence, not an active authority.
2. **After G0.7/G0.8 verdict, C1.10 -> C2/C3 and venue phases:** implement the single production runtime and actual binary HOT0 -> state -> BUY -> qualified egress path. Prewarm RPC/HTTP(S)/TLS and serialize provider templates outside its measured boundary. Qualify any bounded dynamic patching independently (byte-exact requests, matching signature hashes, expiry/window and provider policy); do not introduce a second BUY/signing algorithm or use G0.7 as the permanent trading runner.
3. **PAPER / final production hot-path requalification (I) before LIVE (J):** verify real-path allocation/blocking/format boundaries with instrumentation and measured worst-case latency, counters for dropped/late records, replay/crash tests and independent on-chain outcome checks. `PASS` requires evidence from the actual runtime, not from `c19_consumer` fixtures, derived reports, FakeTransport, builder ACKs or local-only handoff.

### Non-regression / acceptance checks

- Prove by focused static review and relevant tests that **no internal JSON serialization, parsing, persistence or interchange exists in hot, cold, configuration, qualification, evidence or G0.7/G0.8 processing**. The only JSON-RPC exception is the external network protocol boundary; responses become typed/native records immediately. Separately measure that no WSS client, child process, file access/fsync, dynamic framing or synchronous reporting executes between production ingress and outbound handoff.
- Record the timing boundaries separately: earliest observation, local receipt/decision, signature completion, egress handoff, provider write and actual on-chain acquisition. Never infer `T0 -> T_buy < 1 ms` from a receipt, block ordering or local timing without original producing-execution witness.
- Before activating migrated G0.7/G0.8 or production components, prove byte/field parity against preserved immutable legacy evidence, deterministic binary decoding, no lost signed-TX references, crash recovery, G0.8 compatibility, cryptographic identity and unchanged safety fences. Retire active internal JSON readers/writers/contract files only after this proof. Preserve all original historic files untouched and keep the migration as narrow as possible.
- Never send real BNB, arm the compiled guard, modify approval state or loosen an economic bound on the strength of this section alone.

## G0.6a–G0.8 qualification update — binding 2026-10-13

**Existing evidence remains valid.** The existing G0.6 DRY1–DRY4 PASS is retained with its original scope and exact parameters. This update does not reopen all of G0.6 or authorize a live transaction. Additional qualification applies to the new G07 minimum-value and dynamic gas candidate and to any modified signed parameters. The absolute Gate-0 stop rule above remains unchanged.

### Evidence and scope

| Evidence | Status |
| --- | --- |
| Existing G0.6 final DRY | PASS, preserved |
| G07 1 wei local BUY | PARTIAL: two EthereumJS executions each returned 180299145 raw token units, matching buyer balance; launch gas 5223342, BUY gas 167077 |
| Stable minimum qualification | OPEN: later stability attempt failed during local launch, before BUY |
| Historical RPC state | PARTIAL: observed missing trie node; bounded whole-search fresh-preflight restart exists but does not prove persistent state availability |
| Dynamic gas | PARTIAL: current read-only candidate max(eth_gasPrice, verified applicable builder floor), subject to cap; not optimal bidding |
| G0.7 actual Mainnet BUY and G0.8 acquisition/ordering | NOT EXECUTED / NOT AUTHORIZED |

**Update 2026-10-16 (evidence, not a re-rating of the rows above):** the selected G0.7 attempt-1 candidate (BUY 0.000001 BNB, gas 0.05 gwei = current `eth_gasPrice` = verified builder floor, launch gas limit 6 603 270, BUY gas limit 3 000 000) passed the final DRY-1..4 on 2026-10-15 (`bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/`, `dry_result.json` SHA-256 `0efe78a7…`), and read-only readiness reports `FUNDING=PASS` against these exact parameters. The 1-wei TECHNICAL_MINIMUM above stays unqualified; dynamic gas is frozen at the candidate for attempt 1 (no bid optimization). Details: ledger section "2026-10-16 — G0.7-Scope-Abgleich".

**Update 2026-10-17 (evidence, not a re-rating of the rows above):** the internal-JSON migration of G0.7/G0.8 (M1–M8) is implemented and verified offline; the binary v3 contract is an inactive proposal and the guard stays `UNARMED`. The read-only readiness of 2026-10-17 found the Flap Portal upgraded on Mainnet (`portal_version` v5.24.0 → v5.25.0, new implementation `0xe13227370996f892a56d2ff4a30f6026887b48fb`): per the scope rule (mismatch is STOP) the bound DRY and its signed bytes no longer qualify; G0.7 attempt 1 cannot be armed on them. Evidence: `bsc/native/evidence/g07/readiness_20261017T101215Z/`; ledger entry of 2026-10-17.

One wei is the smallest positive native unit, **not a qualified final minimum** until a reproducible, complete launch-then-BUY test passes. Raw token output is not an economically meaningful quantity without independently verified token decimals and fees.

### G07 reproducibility and monotonicity gates

- Keep using existing local EVM crosscheck, diagnostics and bounded complete restart with fresh preflight. Do not mix evidence from different parent blocks. Do not build a new snapshot subsystem unless the existing mechanisms demonstrably cannot reproduce the frozen test.
- Distinguish LAUNCH_FAIL, RPC_STATE_UNAVAILABLE, EVM_DIVERGENCE, BUY_REJECT and QUALIFICATION_INCOMPLETE. Only a true BUY_REJECT **after successful launch** may advance the failed-BUY lower boundary. An intermittent local launch revert invalidates the candidate's final qualification.
- Exponential bracketing and binary search require a supported monotonic-executability assumption. Validate it for the frozen contract/state and relevant amounts; if violated or unprovable, output QUALIFICATION_INCOMPLETE rather than a false exact minimum.
- Maintain distinct outputs for TECHNICAL_MINIMUM and QUALIFIED_MAINNET_TEST_VALUE. A positive local 1-wei token return alone does not authorize a 1-wei Mainnet test.

### Dynamic gas and total-budget gates

- Gas is selected dynamically for the candidate from current BSC eth_gasPrice and verified *applicable* provider rules, not a permanently correct 0.05 or 1 gwei. BlockRazor, 48Club and direct P2P have different admission/ranking economics. Current max-rule is DYNAMIC_GAS_CANDIDATE_ONLY, not proof of the fastest inclusion.
- Freeze the RPC observation, block, builder mode/evidence, gas candidate and approved ceiling. If pricing or total approved spend exceeds the ceiling, STOP; never auto-increase, silently clamp or substitute unqualified bids.
- Budget the **entire** attempt: launch gas + BUY gas + builder tips/payments + BUY input value + other charges. Distinguish worst-case gas-limit reserve from actual charged fees.
- If any final signed field, gas price, nonce, calldata, gas limit or BUY value differs from the prior qualified DRY, requalify the affected exact-byte transaction and necessary G0.6 DRY1–DRY4 acceptance checks. The historical PASS remains intact. Refresh gas and builder requirements before G0.7, then require explicit acceptance and funding approval.

### Priority and deferred optimization

1. Preserve existing G0.6 DRY PASS; fix only observed G07 state/launch instability and verify minimum-search assumptions.
2. Qualify technical minimum, economically allowed minimal test spend, gas candidate and exact signed G0.7 parameters.
3. Run required changed-parameter final DRY checks, followed by explicit authorization for at most the approved Mainnet attempt.
4. G0.7 actual controlled on-chain BUY; G0.8 confirm canonical buyer token acquisition, position relative to launch and competition, builder route and complete costs. Treat T0-to-T_buy as UNKNOWN without a valid producing-execution witness; position N+1 alone is not sub-ms timing.
5. PASS / ADAPT / STOP. Only then resume frozen C1/C2 work.

**DYNAMIC_INCLUSION_BID_OPTIMIZER: PLANNED / BLOCKED UNTIL G0.7/G0.8.** Develop distinct P2P, 48Club and BlockRazor gas/bundle/tip policies under hard total-spend and profitability bounds, with safe signing invalidation on bid changes. Benchmark actual successful acquisition, inclusion position, validator reach and net costs, not RPC suggestions, ACKs or local handoff alone.

## TOP PRIORITY — LATENCY MEASUREMENT

The immediate Gate-0 priority is to reach the real end-to-end latency measurement as quickly as technically valid.

Critical unknowns:

```text
BUY latency        NOT MEASURED
T0 -> T_buy        NOT MEASURED
T_signal -> T_send NOT MEASURED for the target Mainnet path
```

These measurements are the decisive unresolved Gate-0 evidence. All other work is supporting work and should be limited to what is strictly necessary to make the measurement valid, safe, reproducible, and interpretable.

Do not delay the latency experiment for duplicate simulations, cosmetic hardening, additional refactoring, or re-testing facts that are already established. Once the minimum blockers for a valid controlled measurement are closed, proceed directly to the measurement.


Normative documents, highest priority first:
1. `docs/bsc/GATE0_TESTPLAN_SHORT.md` — feasibility gate; decides whether BSC/Solana low-latency work proceeds
2. `docs/bsc/BSC_BIRTH_ENTRY_ARCHITECTURE.md` — BSC birth-entry objective and evidence rules
3. `docs/bsc/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md` — production runtime contract after Gate-0 PASS
4. phase-specific BSC contracts
5. `docs/bsc/BSC_PHASE_C_PYTHON_RETIREMENT.md` — retired Python BSC prototype policy

If an older document conflicts with Gate 0, Gate 0 wins.

## 0. NON-NEGOTIABLE EXECUTION INVARIANT — DIRECT ON-CHAIN ONLY

DarkBot **NEVER** uses the Four.meme or Flap website, browser UI, frontend trading flow, frontend API, page refresh, or manual interaction as part of BUY execution.

The BUY path is always:

```
earliest verified chain/pending evidence
-> venue decode / derive required BUY inputs
-> local quote/input calculation
-> construct exact contract transaction or ordered bundle
-> local secp256k1 signing
-> measured inclusion adapter
-> direct BSC execution against the verified venue contract
```

For Four.Meme this means direct interaction with the verified current protocol family selected by Gate 0: either Classic TokenManager2 or OpenFour Core. These are separate protocol families and MUST NOT share assumed launch/BUY semantics. For Flap it means direct interaction with the verified current Flap/BNB Portal contract surface. Exact addresses, versions, selectors and modes must be frozen from current evidence before use.

A builder/bundle provider or local bsc-geth is an **inclusion transport**, not a trading frontend. It may carry the already-constructed signed transaction/bundle to block execution, but it never supplies strategy, quote, BUY logic or readiness.

Website/frontend state is **never** a readiness signal. DarkBot does not wait for a page to list, refresh, enable a button or expose a frontend API response.

The only external facts relevant to earliest valid execution are the verified on-chain/pending facts required by the current contract semantics. Internal strategy/admission/risk rules may still decline a BUY.

If token address and all mandatory BUY inputs are derivable before `T0`, DarkBot precomputes as much as possible before `T0` and may hand an ordered launch->BUY bundle to a proven inclusion path before the launch executes. If any mandatory input is not derivable before `T0`, DarkBot reacts at the earliest chain-visible point at which it becomes derivable. In both cases the execution remains direct on-chain.

Frontend/UI/API latency is therefore outside the BUY latency budget and outside the production dependency graph.

## 1. Core objective

The BSC product succeeds only if it can obtain the earliest technically possible successful BUY after a new launch becomes executable.

Fast local code, pending lead, geth acceptance or builder acknowledgement are not substitutes for actual execution ordering.

## 2. Current state

- Existing Solana/Pump.fun runtime remains operational.
- Python Phase-C BSC/EVM prototype remains retired / do not merge.
- No full BSC production runtime is built before Gate-0 feasibility is proven.
- Gate 0 may use a deliberately small native Rust measurement harness.

## 3. Phase sequence

| Phase | Status | Deliverable |
| --- | --- | --- |
| A | **DONE / KEEP** | Existing Solana/Pump.fun + Phase-A multichain seam |
| C-PY | **RETIRED** | Old Python BSC/EVM prototype retained only as engineering evidence |
| G0.1 | **IN PROGRESS** | Freeze Flap + Four Classic + OpenFour Mainnet contracts, versions, launch/BUY semantics and live evidence |
| G0.2 | **PASS PER SCOPE / OTHER MODES OPEN** | `G0.2_FLAP_PASS` for the historical DOGE-quote mode (ledger); native-BNB self-launch fields covered by the 2026-10-02 Curve-20 crosscheck PASS and the 2026-10-15 final DRY; other Flap modes, Four Classic and OpenFour open |
| G0.3 | **PASS PER SCOPE** | `G0.3_PASS_PINNED_OFFLINE_SCOPE` (historical case); exact launch→BUY semantics of the frozen G0.7 bytes by DRY-1/DRY-2 (2026-10-15); new wallets/amounts/paths need their own case |
| G0.4 | **PARTIAL** | QuickNode source probe without a frozen-scope denominator; not required for the controlled self-launch; Mainnet ingress coverage NOT PROVEN for external launches |
| G0.5 | **PASS FOR PRESIGNED SELF-LAUNCH / EXTERNAL ADAPT** | 2026-10-02: prepared self-launch local+transport p99 0.584 ms (48Club) / 0.527 ms (BlockRazor), 0/100 > 1 ms each; transport evidence only, not T_buy; external-launch decode/quote/sign path diagnostic p99 1.64/2.22 ms |
| G0.6 | **G0.6a FINAL DRY PASS / MATRIX PARTIAL** | Final DRY PASS 2026-10-11 (1-gwei profile) and 2026-10-15 (attempt-1 candidate); builder semantics (pre-arm checks 6/10) UNQUALIFIED until the approved builder probe runs; full ingress×inclusion and concurrent-launch matrix for external launches still PARTIAL |
| G0.7 | **NEXT / ONLY PRODUCT WORK** | Controlled minimal-value Mainnet BUY with value-bearing execution |
| G0.8 | **IMMEDIATELY AFTER G0.7** | Prove actual token acquisition, ordering, competing external BUYs, costs and entry price |
| G0.9 | **BLOCKED** | Classify external builder dependency: NONE / BENEFICIAL / REQUIRED / UNKNOWN |
| G0.10 | **BLOCKED** | Own-builder feasibility: validator/proposer reach, operating cost, bid competitiveness and independence from third-party providers |
| G0.11 | **BLOCKED** | PASS / ADAPT / STOP decision per venue |
| C0 | **IN PROGRESS / FOUNDATION FROZEN** | Unit-6 startup authorization + recorder/replay foundation IMPLEMENTED_TESTED/FROZEN for foundation scope; carried unchanged through the R1–R13 restructure (2026-10-15/16, regression identical to baseline, STATUS.md); full production qualification remains separate |
| C1 | **PAUSED BEFORE FREEZE — DO NOT CONTINUE BEFORE G0.7/G0.8** | C1.1–C1.8 PASS and C1.9/C1.9b qualified on the pre-restructure source (ledger); C1.10A checkpoint 2026-10-10: regression PASS with pinned-baseline race exception but `C1_10A_COMPLETE = NO` (full non-race suite, final build, identity binding, frozen-binary C1.9 measurement and freeze still open); no C1.10B acceptance record; after the R1–R13 restructure and C1.5–C1.8 re-integration (2026-10-16) the C1.9/C1.9b binding and the C1.10 freeze must be done on the new source; freeze deliberately deferred until the real Mainnet BUY/acquisition test is executed and evaluated |
| C2 | **BLOCKED BY C1.10** | Production pending/canonical/reorg/nonce/replacement state |
| C3 | **BLOCKED** | Production local build/sign/geth path benchmark; engineering proof only |
| D | **BLOCKED** | Full production Flap Rust decoder |
| E | **BLOCKED** | Full production Four.meme Rust decoder |
| F | **BLOCKED** | Native dual-venue capture and pending-coverage/market evidence |
| G | **BLOCKED** | Full production economics/quote verification |
| H | **BLOCKED** | Strategy C Rust + deterministic replay/EV |
| I | **BLOCKED** | PAPER/shadow plus full production hot-path requalification before LIVE |
| J | **BLOCKED** | Production LIVE enablement using Gate-0-proven inclusion adapters |
| KB | **BLOCKED / OPTIONAL AFTER PASS** | Own BSC builder track if external builder access is materially beneficial or required |
| K | **BLOCKED** | Only measured performance escalation |
| S0-S3 | **FUTURE** | Optional later native Solana convergence |

Every phase STOPs for independent review.

## 4. Gate 0 is not C0

Gate 0 deliberately does **not** require the complete production architecture.

Gate-0 ordering rule: prove the smallest exact BUY semantics first, collect ingress coverage in parallel, and only then invest in fine local latency optimization.

It needs only enough code to answer:

```
Can we know enough before/at first tradability
to construct the exact BUY,
and can that BUY occupy the earliest valid execution position?
```

Do not require full journal/replay/reorg/Strategy C/PAPER before answering this.

## 5. Gate-0 measurement model

```
T_pre = earliest actionable pre-tradability evidence
T0    = first execution point where the coin is buyable
T_buy = our successful BUY execution
```

Desired product target:

```
T_buy - T0 < 1 ms
```

For ordinary BSC chain evidence, do not fabricate a sub-ms `T_buy-T0` number.

Economic PASS evidence is actual transaction ordering:

```
launch position N
our successful BUY = earliest valid position after N
successful competing BUYs between = 0
```

Local engineering target remains <1 ms for the controllable hot path.

## 5a. BSC Parallel Ingress Portfolio — five independently switchable signal sources (binding proposal adopted 2026-10-22, v6)

**Scope and priority.** This is an additive ingress contract, not a second engine, an alternative Gate-0 acceptance rule, an exception for signing/sending, or authority to continue paused C1/C2 product implementation. Preserve G0.7 -> G0.8 -> PASS/ADAPT/STOP, the frozen exact-V4 economic approval, the UNARMED default and all binary internal-format rules. Bounded read-only G0.4 observation of these candidates is allowed; no value-bearing send or unapproved private-feed expense follows from this section.

### 5a.1 Product objective and timing — never confuse block arrival with T0

Goal: first independently actionable evidence of a foreign Flap, Four Classic or OpenFour launch; prepare an exact valid BUY as early as possible, preferably before `T0`; obtain earliest valid successful external acquisition by qualified ordered inclusion. The existing objective **`T_buy - T0 < 1,000,000 ns`** is unchanged and requires original producing-execution evidence. Initial birth-entry placement also requires launch index N, our successful BUY index N+1 in the same canonical block, zero successful external BUYs between and attributable positive token acquisition. N/N+1 alone is not a physical sub-ms timing witness.

- `T_first_hash`: first local hash observation (possibly non-actionable).
- `T_full`: first locally received complete cryptographically bound transaction (where relevant).
- `T_actionable`: first source event independently verified as sufficient for the exact venue, deployment, state and qualifying BUY path.
- `T_ready`: exact valid BUY transaction/bundle completely built and signed.
- `T_queue` and `T_send`: existing separately measured queue and real adapter handoff boundaries.
- `T0`: first producing execution point at which independent external BUY becomes valid following successful launch/activation.
- `T_buy`: our actually successful producing BUY execution with proven acquisition.
- `T_head_local`: local arrival of a block/head, **not T0**. Header timestamp and chain block interval are also **not T0**.

`T_head_local - T_full`, even when positive by hundreds of milliseconds, measures only a lead relative to *local head reception*. It does not establish `T_full < T0`, `T_ready < T0`, builder acceptance, or real `T_buy - T0`. Standard RPC, receipts and timestamps cannot invent the original producer's sub-ms execution clock. Preserve `UNKNOWN` where the original producer/validator witness is missing. Block interval is not an additive component of local `T_actionable -> T_send` latency; physical inclusion happens at the builder/validator and is measured separately.

### 5a.2 Five independent inputs and switches

| Source ID | Input | Class | Initial release eligibility |
|---|---|---|---|
| `BSC_INGRESS_GETH` | Pinned bsc-geth early Fast-Lane hook -> native binary IPC | P2P/full transaction | Native baseline; production LIVE still unqualified |
| `BSC_INGRESS_PUBLICNODE` | PublicNode BSC `eth_subscribe` | Public WSS full pending TX | Read-only evidence; requalification needed |
| `BSC_INGRESS_DRPC` | dRPC BSC `eth_subscribe` | Public WSS pending TX | Read-only evidence; observed public endpoint rate limit |
| `BSC_INGRESS_BLOCKREQ` | BlockReq BSC `eth_subscribe` | Public WSS pending TX | Read-only evidence; observed rate limits/low coverage |
| `BSC_INGRESS_48CLUB` | 48Club `newPendingAuctions` | Authenticated private/auction target | UNQUALIFIED, credentials/Soul Point eligibility not proven |

Each input has a user-controlled, independently persisted configuration mode `OFF | SHADOW | LIVE` and a **separate**, automatically derived `HEALTHY | DEGRADED | RATE_LIMITED | DISCONNECTED | UNQUALIFIED` state. `OFF`: no candidate contribution; close/disable source safely. `SHADOW`: ingest and record without authorizing a source-triggered BUY. `LIVE`: source may contribute only after exact capability, chain/deployment, venue, version, health and release-profile qualification plus separate global economic/live authorization. Setting a source to LIVE is never an arming or send permission. A source disabled for signal ingress must not remove mandatory canonical chain, safety, state, reconciliation or reorg services; if such services lack an authorized replacement, safe operation is blocked.

48Club Auction Feed access is not assumed free; never buy subscriptions, bind capital for Soul Points or silently introduce paid providers. `FREE_ONLY` is the baseline. If a source's free tier degrades, degrade/isolate that source, not the native path or safety controls. Every source is independently configurable from terminal/control plane and later from GUI, with coherent persisted atomic generations and measured mode-transition/rollback semantics.

### 5a.3 Parallel race: FIRST VERIFIED ACTIONABLE WINS

All eligible sources run concurrently. No serial wait, preference delay, or synchronous remote hash/receipt/quote lookup on the trading hot path. Preserve first hash, first full, first actionable and source provenance per source separately. The first independently verified actionable source event wins the candidate decision, regardless of provider brand. A hash-only event cannot outrank an already actionable full event, and merely accepting `newPendingTransactions(true)` is not proof that full TX objects are delivered: verify actual event shape and signed identity. A private builder target qualifies only when its exact capability/simulation/state/ordered-inclusion semantics allow correct construction without inventing missing full transaction bytes.

One canonical candidate/intent/nonce/economic owner and **one existing native Rust venue/state/quote/risk/build/sign implementation** serve all inputs. Duplicates or later enrichments cannot create conflicting intents, nonces, signatures or duplicate economic exposure. Inclusion/output adapters (local geth, 48Club, BlockRazor) remain separately versioned and independently qualified; an ingress source does **not** implicitly authorize a corresponding inclusion adapter. Do not modify existing `submission_policy/capability.rs` to disguise ingress capability as inclusion.

External WSS JSON-RPC is permitted only at the unavoidable network protocol boundary. Validate and convert it to typed/binary events before internal processing; no JSON/WSS parsing, HTTP lookup, file IO, logging, network await or dynamic parsing on the measured native decision/dispatch hot path. Separate source budgets for CPU, sockets, bounded queues, memory, reconnection/rate-limit/backpressure. An ill-behaved source cannot delay, starve or invalidate the native geth Fast Lane. Preserve current single binary journal with explicit source health, gaps, drops and replay identity.

### 5a.4 Qualification, evidence and phases

Per source + venue + deployment/version + inclusion adapter, record: actual full/hash/target object and crypto identity; canonical launch denominator; full and actionable coverage; true on-Mac same-hash/source timestamp races against local geth; p50/p95/p99/max and all tail failures; disconnects/reconnects/rate-limits/drops; resource overhead and nonregression; cost and eligibility. Distinguish `T_head_local - T_actionable` from producer `T0 - T_actionable` and retain UNKNOWN without a valid original source witness. Keep exact raw samples, fixture hashes, source/build/config identities and failed attempts, not only selected winners.

- **G0.4:** scoped read-only parallel observation and evidence under the existing stop-rule; no production LIVE promotion.
- **G0.5/G0.6:** integrated source-to-send and ingress x inclusion qualification when permitted under the Gate-0 execution sequence.
- **G0.7/G0.8:** remain strictly tied to their original frozen controlled minimal-value attempt and approved SHA; no alteration of launch/BUY bytes or broader live source set by this amendment.
- **C1/C2:** after G0.7/G0.8 verdict, source adapters, binary ingress and candidate/state dedup; pinned geth remains an independent baseline.
- **C3/D/E/F/G/H:** exact native hot-path, distinct venue decoders, journal/market evidence, economics and common strategy.
- **I/J:** all-mode switching, failure isolation and strict qualified LIVE gates; no unqualified source can trigger a BUY.
- **K:** additional escalation only with reproducible evidence.

**No automatic success claims.** The 2026-10-22 PublicNode/BlockReq/dRPC observations establish reachable public feeds and some earlier-than-local-head full launch sightings; they do not qualify `T_pre < T0`, a N+1 BUY or a production LIVE source. Recorded original producer execution evidence remains mandatory for actual physical `T_buy - T0 <1 ms` acceptance.


## 6. Gate-0 inclusion race is mandatory

Do not wait until J/K to investigate builders.

BSC production uses PBS/builders, and Gate 0 must compare the real available inclusion paths before a large implementation is justified.

Minimum candidates at the time of test:

- local bsc-geth/P2P;
- 48Club/Puissant ordered/backrun-target path if still supported;
- BlockRazor ordered/bundle path if still supported.

Reverify current APIs immediately before the test.

The winning path is selected from real execution placement, not ACK latency.

## 7. Gate-0 venue rules

### Flap

Before full Phase D, Gate 0 is allowed to implement a **minimal factual Flap probe** sufficient for:
- launch recognition;
- BUY-field preparation matrix;
- token-address derivation proof;
- exact first-BUY construction;
- semantic launch→BUY validity;
- ingress coverage measurement for the selected scope.

This is not the full production decoder.

### Four.Meme Classic / OpenFour

Gate 0 treats Four Classic TokenManager2 and OpenFour Core as separate current protocol families. Before full Phase E, minimal factual probes may be implemented for each family sufficient for the same feasibility questions, but assumptions/decoders must not be shared without evidence.

Do not assume pre-execution token-address derivation. Prove it or record that it is unavailable.

## 8. C0 — production foundation after PASS

Only after at least one BSC venue passes Gate 0:

- create clean Rust workspace;
- versioned native event schema;
- immutable binary journal;
- deterministic replay API;
- bounded queues/overflow evidence;
- exact integer/address/hash primitives;
- native latency records.

STOP.

## 9. C1 — production earliest ingress

Implement minimal pinned bsc-geth hooks:

- earliest practical complete transaction event;
- txpool acceptance;
- canonical/reorg events;
- source peer/message provenance;
- shared-memory SPSC into Rust.

Measure peer decode → Rust visibility.

Pending coverage remains a quality metric; do not assume completeness.

Current local qualification checkpoint, 2026-10-09:

```text
reopened C1.3/C1.4 regressions       PASS
C1.4a REV2                           PASS / GREEN_STRONG_HEADROOM / max 20 us
C1.5 Rust HOT0                       PASS
C1.6 M4 development profile          PASS
C1.7 local-geth outbound             PASS
C1.8 concurrency/state safety        PASS
C1.9 measurement                     COMPLETE / integrity PASS / eligible
C1.9 measured max T_ingress->L2      418 us
C1.9 measured max ingress->1st peer  438 us
C1.9 final source/build binding       OPEN pending final full regression/freeze bind
C1.9b Testnet infrastructure          READY / REAL CHAPEL PEER PASS
C1.9b chain                           97 / 0x61
C1.9b pinned geth                     v1.7.8 / cdb7548b5baacf...
C1.9b controlled ETH/68 hook/path     PASS
C1.9b hardened qualification gate     PASS
C1.9b fresh-pin reproducibility       PASS
C1.9b affected geth tests/build       PASS
C1.9b controlled observations         1/1
C1.9b invalid_records/lane_faults     0 / 0
C1.9b analyzer                        PASS / missed=0
C1.9b Mainnet ingress coverage        NOT PROVEN
C1.10 freeze                          BLOCKED pending complete final C1 regression + bind
T0->T_buy <1 ms                       NOT PROVEN
LIVE                                  BLOCKED
```

Detailed commands, identities, distributions, counters and scope/non-claims are recorded in `GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md`. The C1.9 local selected-peer handoff is not a purchase/acquisition timestamp.

C1.9b controlled Testnet hook/path qualification is now locally PASS. The qualification-only pre-sync override is hardened: it requires `DARKBOT_FASTLANE_V2_SCOPE=c19b-testnet`, `DARKBOT_C19B_ACCEPT_TXS_UNSYNCED=YES`, and network ID 97; invalid combinations fail closed. A fresh-pin generated candidate passed targeted/affected geth tests and build, then the hardened runtime produced 1/1 controlled observation with zero misses, invalid records, or lane faults. The hardened candidate was copied byte-for-byte into the local vendor files and rebuilt/retested. These local source changes are **not yet C1.10-frozen**. The next operation is the complete final C1.5-C1.9/REV2 regression and source/build/config binding, followed by C1.10 freeze.


### C1.10A checkpoint — 2026-10-10

Pre-freeze regression has started. Python qualification tooling, Rust fmt/clippy/debug/release tests, C14 interop, targeted geth tests and `go test -race ./darkbot/fastlane` are PASS.

Broad pinned-geth race suites are not clean even on the untouched accepted pin. `./eth/protocols/eth -race` reproduces the same `CachingDB.NoTries` / `core/state/database.go:280` family on current and clean-pin trees. `./eth -race` also fails on both; the current run reported 55 race warnings and the clean pin 51 with common core signatures and substantial failed-test overlap. This is recorded as `PINNED_BASELINE_PRESENT`, not as a claim of exact race equivalence or a race-free upstream tree. DarkBot Fast-Lane itself is race-clean in the targeted run.

C1.10A remains open until the full non-race geth suite, final exact build, source/build/config/evidence binding and final measurement/freeze steps complete.

STOP.

## 10. C2 — production BSC state

Implement only runtime semantics needed for correctness:

- tx identity/first-seen;
- sender+nonce competition/replacement;
- included/reverted;
- canonical/orphan/uncertain;
- bounded reorg ancestry;
- restart/recovery;
- explicit gaps/provenance.

Port relevant Python Phase-C counterexamples as Rust tests.

STOP.

## 11. C3 — production local execution engineering gate

C3 no longer pretends to prove a real BUY.

It proves only the production local path:

```
native event
-> Rust
-> decode/state/decision/quote
-> build/sign
-> selected local submission adapter
```

Target local controllable latency: <0.5 ms where achievable; >=1 ms investigated; reproducible >3 ms blocks release until explained.

Actual economic BUY feasibility was already proven in Gate 0.

STOP.

## 12. D/E — full production venue decoders

D = full Flap decoder.
E = full Four.meme decoder.

Gate-0 minimal probes are not independent production engines.

Production decoder requirements:

- one Rust source of truth;
- verified current deployment/version;
- pending + canonical factual decode;
- unknown modes fail closed;
- same functions in capture/replay/PAPER/LIVE;
- no remote fetch in hot path.

STOP after each.

## 13. F — native market evidence

Run the production dual-venue capture.

Primary purpose:

- pending coverage;
- complete/full-tx arrival paths;
- peer first-seen spread;
- venue version/mode census;
- canonical outcomes;
- quality gaps;
- replay identity;
- production ingest/decode tails.

Canonical timing may be retained as diagnostic data, but **`EARLY_TX_SEEN - canonical observation <= -10 ms` is not a product PASS gate**.

The useful pre-lead question is whether actionable evidence arrives before first executability often enough to support the Gate-0-proven path.

### F storage/capture extension — binding ownership, not a new phase

Phase F implements the production storage, bounded-resource and native offline-analysis contract in `BSC_PRODUCTION_BUILD_MASTER_PLAN.md` §11, subsection "Production storage, resource and offline-analysis contract". Preserve the existing `DBJNL001` journal and `DBREC001` standalone-record codecs, recorder/replay and the one Rust workspace; add only narrowly owned modules where missing. Never create a competing journal, trade/quote/strategy implementation, Rust workspace or JSON internal data path.

- Preallocated/typed events -> bounded asynchronous recorder -> batch writer -> integrity-checked, segmented `DBJNL001` on SSD; `DBREC001` for typed configuration/run manifests and standalone evidence. No SSD/SQLite/JSON/conversion, child process, wait for the writer or sync logging on HOT0 -> outbound handoff. Required pre-egress financial durability fences remain intact and are never replaced by an asynchronous queue ACK.
- Bound **total actual capture memory by bytes**, including payload ownership, queued items, batches and caches. Permit bounded adaptive growth, never unbounded RAM; preflight sustainable disk throughput/capacity and RAM headroom before an approved long run. A running capture must not silently lose records, claim COMPLETE with loss, or treat exhausting hardware as a successful run; persist explicit gaps/recovery status and retain diagnostics.
- Build an optional, separately budgeted **offline SQLite index** only from verified sealed journal segments. It is rebuildable and nonauthoritative; raw events, canonical verification, economics and financial state stay in native Rust/binary evidence. Exact integer amounts must not be coerced to lossy SQL REAL.
- Phase F retains its separately required continuous **30-minute** native binary coverage run and exact-core replay; per-attempt 15-second windows remain separate. 4h/24h/48h campaigns are scoped future operating qualifications with explicit CapacityPreflight, not new Gate-0 or universal Phase-F prerequisites. Final RAM/backpressure/replay/crash and true hot-path timing checks belong to Phase I. Document and qualify actual source/build/host evidence; no automatic PASS.

STOP.

## 14. G — full economics

Implement complete executable quote/cost semantics once in Rust.

Gate 0 already contains only the minimal exact economics needed to make its test BUY valid.

Phase G adds full supported-mode economics:

- protocol fees/taxes;
- exact quote/output math;
- gas/revert/replacement costs;
- sell/approval path;
- migration/graduation economics.

No synchronous external quote in the live hot path.

STOP.

## 15. H — Strategy C / EV

Only now add Strategy C to the BSC production runtime.

Use same:
- state;
- venue decode;
- economics;
- builder;
- signer.

Positive expectancy after complete costs is a separate question from Gate-0 feasibility.

STOP.

## 16. I — PAPER/shadow + final production hot-path requalification

Run the same production semantics through build/sign in PAPER/shadow, but distinguish two boundaries:

```
production event
-> full production venue decoder
-> production state
-> production economics / quote
-> production strategy / admission
-> production transaction build
-> production signer
-> production serialization
-> PAPER test-sink handoff = T_shadow_sink
```

`T_shadow_sink` is useful for isolating local production-code cost but is **not** the real transport boundary and must never be reported as `T_send`.

Before broad LIVE is enabled, run a controlled transport revalidation using the actual Gate-0-proven production adapter:

```
production event
-> same full production path
-> actual adapter queue/IPC
-> actual defined transport handoff = T_send
```

The final production program must still satisfy the frozen local latency requirement using the same adapter-specific timing boundary defined in Gate 0 **G0.5**. A fast Gate-0 prototype, C3 result or PAPER test sink cannot waive the real transport revalidation.

Also rerun the required concurrent-launch/nonce stress cases against the final production runtime.

STOP.

## 17. J — production LIVE

J is production enablement, not the first time inclusion is tested. Its first controlled step revalidates the actual production transport boundary before broader LIVE is permitted.

Use only inclusion adapters already validated in Gate 0 and revalidated for current service/API behavior.

Validate:
- actual intended-token acquisition by the intended recipient;
- ordering against external competing BUYs;
- realized gas/tips/slippage and total debit/cost;
- replacement/reconciliation;
- replay/decision parity;
- continued compliance with the final production hot-path latency gate.

STOP.

## 18. KB — own-builder independence track

This phase is activated only if Gate 0 shows that an external builder materially improves or is required for the target inclusion quality.

External builders are not a permanent architectural dependency by default.

Evaluate whether DarkBot can operate its own BSC builder using current official builder interfaces/reference implementations.

The acceptance criteria are infrastructure-level:

- validator/proposer integration is technically available;
- sufficient proposer coverage can be reached;
- valid competitive block bids can be produced reliably;
- operating cost is known;
- maintenance complexity is known;
- third-party dependency is reduced without degrading the measured inclusion result.

Possible outcomes:

- **OWN_BUILDER_PASS** — own builder can replace the external provider for the required quality;
- **OWN_BUILDER_PARTIAL** — hybrid operation is justified;
- **OWN_BUILDER_FAIL** — retain the external builder only where its measured benefit justifies the dependency/cost.

Building or operating a validator is not required by this phase.

STOP.

## 19. K — measured escalation only

Escalate only when production measurements identify the bottleneck.

Possible order:

1. peer-path optimization;
2. inclusion/builder path optimization;
3. geographic sensors;
4. direct Rust devp2p;
5. kernel/NIC specialization.

Do not prebuild all levels.

## 20. Architecture invariants

This section additionally inherits the binding binary-hot-path / external-protocol boundary defined near the top of this roadmap (2026-10-16). The required builder JSON-RPC wire frame is an external protocol exception, not a general runtime JSON processing exception.

Production remains:

```
pinned bsc-geth
+ native event path
+ one Rust runtime
+ one decision implementation
+ one build/sign implementation
+ one recorder/replay format
+ measured inclusion adapters
```

Forbidden:

- Python BSC trading runtime;
- duplicate strategy/decoder/quote implementations;
- public WSS as the sole primary production signal;
- remote quote/RPC lookup in the decision hot path;
- treating local geth acceptance or builder ACK as a BUY;
- claiming exact sub-ms on-chain execution from ordinary block timestamps.

## 21. Decision rule

If Gate 0 returns STOP for both Four.meme and Flap, the BSC Birth-Sniping production roadmap stops.

If one venue passes, production work proceeds only for the proven venue first. The second venue may remain ADAPT/STOP until separately proven.
