# Gate 0 — Measurement & Evidence Ledger

Status: **AUTHORITATIVE MEASUREMENT LOG**
Created: 2026-09-28
Scope: DarkBot Gate 0, Solana reference measurements and BSC/Flap feasibility evidence

Normative parents:
1. `docs/GATE0_TESTPLAN_SHORT.md`
2. `docs/BSC_DUAL_VENUE_ROADMAP.md`
3. `docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md`
4. `docs/GATE0_G02_PRECOMPUTABILITY_MATRIX_2026-09-28.md`
5. `docs/GATE0_G02_FLAP_ECONOMICS_BUYPATH_2026-09-28.md`

## 0. Hard logging rule

This file is the canonical chronological ledger for Gate-0 measurements and empirical findings.

Before a Gate-0 stage may be declared PASS or the project may advance to a dependent stage, the relevant evidence MUST be recorded here or in a linked immutable/raw evidence artifact.

Every measurement entry MUST state:

- date/time where available;
- venue/chain/scope;
- exact measured interval or semantic claim;
- machine/runtime where relevant;
- sample count;
- included work;
- excluded work;
- raw evidence path;
- result;
- interpretation;
- what the result does **not** prove;
- Gate impact.

Rules:

1. Never silently replace an older result. Append or explicitly supersede it.
2. `N/A` means not measured. Never convert an unavailable latency into zero.
3. Never reuse a Solana/PAPER latency as a BSC BUY latency.
4. Never derive BSC sub-millisecond BUY execution latency from ordinary block timestamps.
5. `T_send` is the actual tested transport handoff, not an internal queue.
6. A successful BUY requires acquisition evidence, not only build/sign/send/ACK/receipt status.
7. No phase transition based on chat text alone. Evidence and interpretation must be frozen first.
8. Secrets/API tokens are never written into this ledger or committed.

## 1. Timing definitions

```text
T_signal = local monotonic timestamp at receipt of actionable signal used by the attempt
T_pre    = T_signal only when independent evidence proves the signal occurred before external tradability
T0       = first execution point at which an independent external BUY can validly execute
T_ready  = exact BUY transaction/bundle fully built and signed
T_queue  = signed payload committed to local adapter/IPC queue, where applicable
T_send   = signed payload handed to the actual tested transport boundary
T_buy    = BUY executes successfully and acquisition evidence proves intended token received
```

Primary local latency target for G0.5:

```text
T_signal -> decode/derive/quote -> build/patch -> sign -> serialize -> T_send
target: < 1.0 ms
preferred: < 0.5 ms
```

Economic/product proof later requires actual successful acquisition and ordering. Standard BSC block/receipt timestamps do not provide a trustworthy sub-ms `T_buy`.

## 2. Current Gate status

| Gate | Current status | Evidence / blocker |
| --- | --- | --- |
| G0.1 Evidence freeze | IN PROGRESS / substantial Flap evidence frozen | Flap/Four/OpenFour evidence exists; no claim that all Gate-0 acceptance criteria are frozen |
| G0.2 Precomputability + primary-scope proof | **PARTIAL** | Archive access now works; exact first-BUY calculation from tx-precise prestate achieved; 65 intervening transactions still require complete relevance/state-change review |
| G0.3 Minimal correctness-first launch->BUY harness | BLOCKED BY G0.2 | Do not begin until G0.2 review closes |
| G0.4 Pending/actionable coverage | may run in parallel per testplan; no new PASS recorded here | separate evidence required |
| G0.5 Local hotpath through true T_send | BLOCKED | no executable BSC BUY path through signing + real transport handoff yet |
| G0.6 Dry ingress x inclusion | BLOCKED | depends on prior correctness/hotpath work |
| G0.7 Controlled minimal-value Mainnet | BLOCKED | no value-bearing authorization from this ledger |
| G0.8 Acquisition + ordering proof | BLOCKED | requires G0.7 |
| G0.9+ | BLOCKED | later gates |

## 3. Frozen Flap fixture and BUY ordering

Selected historical Flap fixture:

- Portal: `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`
- observed Portal version during G0.1/G0.2 work: v5.24.0
- launch mode: `newTokenV6`
- token version: `TOKEN_TAXED_V3`
- quote token: DOGE `0xba2ae424d960c26247dd6c32edc70b295c744c43`, 8 decimals
- token: `0xabe1fe16cf6bc48c9524713054efe6e954b17777`

Launch:
- tx `0x5b8cc9893c403a66d4f43bd73ed5c69227807661a58d1d6b5611def3f1d8352c`
- block 124415357
- tx index 0

Earliest observed external BUY:
- tx `0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800`
- block 124415358
- tx index 15
- path observed historically through on-chain router
- actual acquired output: `6243568333969093909994622` token base units

Direct Portal reference BUY:
- tx `0x8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b`
- block 124415360
- tx index 17
- fifth observed external BUY, not the first
- selector `0xef7ec2e7`
- signature `swapExactInput((address,address,uint256,uint256,bytes))`
- native input: 0.15 BNB
- actual acquired output: `28438305464314215025968609` token base units

Interpretation:
- both are direct on-chain execution paths;
- the historical earliest observed BUY using a router does **not** prove that DarkBot must use that router;
- the product design remains direct on-chain and may use direct Portal if G0.2/G0.3 prove all required inputs/semantics.

## 4. CREATE2 / token-address derivation

Result: **PASS for the frozen Flap V6 fixture.**

Correct EIP-1014 calculation:

```text
keccak256(0xff ++ Portal(20 bytes) ++ salt(32) ++ keccak256(init_code))[12:]
```

reproduces exactly:

```text
0xabe1fe16cf6bc48c9524713054efe6e954b17777
```

Evidence status:
- 7 local EIP-1014 vectors: PASS
- independent RPC hash cross-check: PASS
- earlier mismatch was caused by incorrectly padding the 20-byte Portal address to 32 bytes

Gate implication:
- token address is proven signal-time local computation for this exact scope.

## 5. Historical economics / curve reproduction

Frozen launch/economics facts used during G0.2:

- `quoteAmt = 80574062404`
- buy tax = 100 bps
- sell tax = 100 bps
- dividend token = DOGE
- reconstructed reserve after launch = `78962581156`
- reconstructed circulating supply = `21453245135239692933310825`
- reconstructed `r = 39956880715754427756773`
- reconstructed `h = 107036751000000000000000000`
- reconstructed `k = 44233735407663336217722200164523`
- DEX threshold = `800000000000000000000000000`

Conditional historical reproduction:
- creator acquisition + six external BUY outputs reproduced with integer equality
- first external BUY conditional expected = `6243568333969093909994622`
- actual = `6243568333969093909994622`
- conditional difference = 0

Important limitation of the earlier conditional result:
- some constants/deductions were learned from historical flows;
- therefore that result alone was not a pending-only/prestate proof.

## 6. Fee/tax observations

Observed protocol fee and tax are separate DOGE deductions.

Naive assumption that both always equal `floor(input/100)` is disproven.

Observed examples:

| quote input | protocol fee | tax |
| ---: | ---: | ---: |
| 111646524369 | 1116465243 | 1116465244 |
| 120878323272 | 1208783232 | 1208783233 |
| 67309479679 | 673094796 | 673094797 |

Do not invent a generic rounding rule from these samples.

Current rule:
- exact first-BUY output can now be reproduced from transaction-precise historical prestate;
- this ledger does not mark the underlying fee/tax source semantics fully closed unless the G0.2 verifier/evidence explicitly proves them.

## 7. RPC / archive diagnostics

### 7.1 Public Binance BSC endpoint

Endpoint family: public Binance BSC dataseed.

Historical `debug_traceTransaction(..., callTracer)` tests for:
- launch;
- first external BUY;
- direct Portal BUY.

Result for all three:
```text
code: -32000
message: missing trie node
```

Interpretation:
- normal public/pruned endpoint lacks the required historical state;
- this is an evidence-source limitation, not a Flap/DarkBot protocol failure.

### 7.2 dRPC attempts

Saved attempts produced:
- DNS resolution failure;
- HTTP 500 Internal Server Error;
- HTTP 429 Too Many Requests.

Interpretation:
- no successful archive witness was established from these attempts.

### 7.3 NodeReal historical use

A prior NodeReal BSC endpoint was used for `eth_getLogs` evidence and returned HTTP 200 transport-level responses.

Interpretation:
- prior NodeReal evidence is useful as historical RPC evidence;
- it was not accepted as a proven archive `debug_traceTransaction` provider for this fixture.

### 7.4 QuickNode archive qualification — PASS

Provider: QuickNode, BNB Smart Chain Mainnet.
Secrets/tokens intentionally omitted.

Both required historical tracer modes succeeded.

Measured trace sizes:

| Transaction | callTracer | prestateTracer |
| --- | ---: | ---: |
| First BUY | 71 call frames | 22 accounts, 67 storage slots |
| Launch | 116 call frames | 28 accounts, 146 storage slots |
| Direct BUY | 50 call frames | 26 accounts, 64 storage slots |

Local raw evidence path reported by the test run:

```text
/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/gate0-evidence/2026-09-28-g02-archive-witness-quicknode-153754
```

All six raw responses were reported SHA256-checked.

No LIVE transaction, signing or production-code change occurred during this archive qualification.

Gate implication:
- the historical archive-data blocker is resolved.

## 8. Transaction-precise independent first-BUY calculation

Latest G0.2 archive-witness result:

```text
expected output from transaction-precise prestate
= 6243568333969093909994622

actual acquired output
= 6243568333969093909994622

full calculation difference
= 0
```

This is stronger than the earlier conditional reproduction because it was calculated from the transaction-precise historical prestate.

Current limitation:
- 65 transactions between launch `124415357:0` and first BUY `124415358:15` have not yet been fully classified for every BUY-relevant state change;
- therefore G0.2 remains **PARTIAL** pending that complete review and verifier update.

Local report path reported by Astra:

```text
/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/G02_FLAP_ARCHIVE_WITNESS_2026-09-28.md
```

No conclusion in this ledger may upgrade G0.2 to PASS solely because difference = 0.

## 9. Historical block timestamp diagnostic — NOT BUY LATENCY

A historical block timestamp gap of approximately **450 ms** was observed between the launch block and the following BUY-containing block.

Classification:
- diagnostic block-level timing only;
- **forbidden as a BUY-latency KPI**;
- does not measure `T_signal -> T_send`;
- does not measure exact EVM execution time for the individual BUY;
- must not be quoted as "BUY latency".

## 10. Existing Solana/macOS performance reference — NOT BSC BUY LATENCY

Existing DarkBot Solana/PAPER Mac evidence:

- tests: 287 PASS
- Python syntax checks: 119 PASS
- benchmark frames: 22,109
- ENTRY intents: 300
- writer failures: 0
- overflow: false
- receive -> decode p99: 0.859333 ms
- receive -> decision p99: 2.545375 ms
- decision -> enqueue p99: 0.078375 ms
- receive -> enqueue p99: 2.992500 ms
- receive -> enqueue max: 3.559292 ms
- queue callback lateness p99: 0.756292 ms

Classification:
- useful engineering/reference baseline only;
- Solana/PAPER path;
- no BSC transaction construction/signing/real transport;
- not `T_send`;
- not G0.5 evidence;
- never quote `2.9925 ms` as BSC BUY latency.

## 11. BSC Flap offline warm microbenchmark — 2026-09-28

Script reported locally:

```text
gate0-evidence/measure_flap_offline_latency.cjs
```

Raw/result path reported locally:

```text
/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/gate0-evidence/2026-09-28-g02-latency-offline/latency_result.json
```

Benchmark configuration:
- warm single process on the user's Mac;
- one frozen historical Flap fixture;
- exact historical state already preloaded in memory;
- 3 rounds x 2,000 measured samples = **6,000 samples**;
- 200 warmup calls before each measured category/run;
- monotonic `process.hrtime.bigint()`;
- timing-call overhead included, not subtracted.

Measured path:

```text
launch calldata ABI decode
-> CREATE2 token address
-> extension price decode
-> BNB/WBNB/DOGE conversion arithmetic used by frozen fixture
-> r/h/k derivation
-> Flap curve BUY output
```

Measured aggregate for the full `launch_decode_create2_quote` path:

- median / p50: **0.252 ms**
- p99: **0.405 ms**
- maximum: **2.229 ms**

Correctness guard:
- benchmark output checked against `6243568333969093909994622`;
- calculated output matched the frozen first-BUY output.

Explicitly excluded:
- network receive;
- mempool detection;
- state refresh/invalidation;
- strategy filters;
- transaction build;
- nonce management;
- signing;
- final transaction serialization;
- socket/transport handoff;
- RPC acknowledgement;
- inclusion;
- on-chain execution.

Interpretation:
- proves the warm local decode/address/quote arithmetic is currently sub-ms at p99 on this Mac for this frozen fixture;
- demonstrates that the calculation component alone is not obviously incompatible with the <1 ms local target.

Does **NOT** prove:
- complete BSC BUY latency;
- `T_signal -> T_send < 1 ms`;
- actual BUY execution latency;
- first external placement;
- G0.5 PASS.

## 12. BUY latency — current authoritative status

As of 2026-09-28:

```text
BSC T_signal -> T_send              = N/A / NOT YET MEASURED
BSC T_signal -> successful BUY      = N/A / NOT YET MEASURED
exact per-tx T_buy - T0 in ms       = N/A with ordinary BSC evidence
```

Known partial local component only:

```text
warm launch decode + CREATE2 + quote
p50 = 0.252 ms
p99 = 0.405 ms
max = 2.229 ms
```

The next valid local BUY-latency measurement must include:

```text
T_signal
-> minimal decode
-> only G0.2-proven residual calculation
-> direct Portal BUY transaction finalize/build/patch
-> nonce/gas finalization from pre-resident state
-> secp256k1 signing
-> final serialization
-> actual tested transport handoff T_send
```

Stage-by-stage timestamps must be retained.

## 13. Why G0.2 must finish before the real hotpath benchmark

The latency goal is the primary engineering KPI, but performance numbers are only meaningful for a semantically valid BUY.

G0.2 exists to prove which data may be:
- frozen at startup;
- kept pre-resident;
- derived from pending launch facts;
- calculated at signal time;
- invalidated by intervening state changes.

Without this classification, an artificially fast benchmark could omit a required state read or use information unavailable at `T_signal`.

Therefore current order remains:

```text
G0.2: finish 65-intervening-TX relevance/state review
-> G0.3: exact minimal Direct-Portal launch->BUY correctness harness
-> G0.4: freeze/measure actionable signal coverage
-> G0.5: complete local T_signal -> T_send benchmark
-> G0.6: ingress x inclusion dry/concurrency
-> G0.7: controlled minimal-value Mainnet BUY
-> G0.8: actual acquisition + ordering proof
```

## 14. Required evidence before the next phase transition

Before declaring G0.2 complete:

- classify all 65 intervening transactions for BUY-relevant state effects;
- bind any relevant state mutation into the independent first-BUY reproduction;
- rerun first-BUY reproduction;
- maintain expected output = actual output and difference = 0;
- update `verify_flap_g02_economics.py` from evidence, not hard-coded PASS flags;
- record remaining UNKNOWNs explicitly;
- review whether all required G0.2 requirements are actually true.

Before declaring G0.5 complete later:

- executable BSC Direct-Portal BUY path exists and passed G0.3;
- every timing stage is monotonic and separately recorded;
- build + sign + final serialization are included;
- `T_send` is a true live/dry-qualified transport boundary, not an internal queue;
- sample counts and p50/p95/p99/max are frozen;
- no synchronous RPC/HTTP/DB/filesystem/DNS/lazy init enters the measured hot path;
- exact machine/runtime/config and evidence hashes are recorded.

## 15. Preservation rule

All future Gate-0 measurements must be appended to this ledger with an evidence path and gate interpretation.

A phase is not considered historically auditable unless a future reviewer can answer:

1. What exactly was tested?
2. On what machine/runtime/config?
3. How many samples/attempts?
4. What was measured?
5. What was excluded?
6. Where are the raw artifacts?
7. What did the result prove?
8. What did it explicitly not prove?
9. Which Gate decision changed because of it?
