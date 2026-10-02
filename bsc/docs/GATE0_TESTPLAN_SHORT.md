# Gate 0 — BSC/Solana Birth-to-BUY Feasibility Gate

> **ABSOLUTE TOP PRIORITY / STOP RULE — REAL BUY ONLY:**  
> For the current BSC Gate-0 work, the only acceptance target is a **real successful on-chain BUY with token acquisition** and a **measured/proven `T0 → T_buy < 1 ms`**.  
> **Until that is achieved, no other tests, optimizations, features, venues, strategies or follow-up work are allowed.**  
> The only permitted work is code, instrumentation, transport/inclusion integration or evidence work **strictly required to execute, measure, prove, or reduce the real `T0 → T_buy` BUY path below 1 ms**.  
> `T_signal → T_send`, build/sign latency, transport handoff, builder ACK, receipt status or ordering alone **do not satisfy this gate**.  
> If a real successful BUY cannot be proven below 1 ms, Gate 0 remains **BLOCKED/FAIL** and all unrelated work stays stopped.

Status: **AUTHORITATIVE / FIRST HARD GATE**
Updated: 2026-10-01

## 1. Product question

Before any further low-latency production build, answer one question:

> When a new coin becomes externally buyable for the first time, can DarkBot obtain the earliest technically possible successful external BUY?

If the launch transaction itself contains a creator/presale/initial BUY, that launch-internal acquisition is not an externally overtakeable BUY. DarkBot's target is then the first technically possible external successful BUY after the launch-internal state transition.

A BUY counts as successful only when the intended token is actually acquired by the intended recipient with a positive received amount and attributable economic debit/cost. Build, sign, send, adapter ACK or receipt status alone are insufficient.

Test objects:
- Solana / Pump.fun
- BSC / Flap
- BSC / Four.Meme Classic (TokenManager2)
- BSC / Four.Meme OpenFour

Four.Meme Classic and OpenFour are distinct current protocol families and MUST NOT share an assumed launch/BUY decoder or scope.

Gate 0 is a minimal feasibility harness, not the production bot.

### BSC latency-source rule — public JSON/WSS is control-only

For BSC birth-entry latency qualification, generic public JSON-RPC/WebSocket
`newPendingTransactions` feeds are **not a valid race ingress**. They may be
used only as a correctness/control source.

The controlled 2026-10-01 Testnet run proved the Rust reaction path itself is
fast (`T_signal -> T_send = 338,000 ns`) but the public WSS signal arrived
`110,659,792 ns` after the launch network handoff. Therefore that sensor is
removed from the Gate-0 performance path.

The next BSC race qualification MUST use one of:

1. direct BSC P2P / local bsc-geth early full-transaction ingress; or
2. a measured low-latency binary/streaming relay such as a BSC public-mempool
   gRPC feed, with the exact provider/region/version frozen in evidence.

The selected ingress must deliver the complete signed launch transaction (or
losslessly equivalent fields) without a synchronous RPC lookup in the hot
path. JSON/WSS may remain in parallel for evidence/recovery only.

Implementation state 2026-10-01: the controlled Flap Testnet G0.7 harness now
uses a standalone BSC devp2p/eth network component (BNB reth/reth-bsc) with an
in-memory transaction pool as the race ingress. It does not start a full-node
database/state sync. Public WSS is kept on a separate connection only as a
same-launch control sensor and, until the inclusion adapter is replaced, as
the submission transport. The empirical P2P-vs-WSS timing remains unproven
until a new controlled run is recorded in the measurement/evidence ledger.

### Hard execution rule — direct on-chain only

Gate 0 must test the path DarkBot will actually use: **construct, sign and submit the BUY directly for on-chain contract execution**.

Forbidden as BUY dependencies:
- Four.meme/Flap website or browser UI;
- frontend trading workflow;
- frontend API response as a readiness gate;
- page refresh/listing state;
- manual click/action.

Allowed inclusion transports are the measured chain paths in this document: local bsc-geth/P2P and verified builder/bundle adapters. They transport the signed transaction/bundle; they do not replace the venue contract or become a frontend dependency.

If the exact BUY can be derived before `T0`, prebuild/pre-sign/pre-submit as far as the proven ordered inclusion mechanism safely permits. If it cannot, derive the missing input from the earliest valid chain evidence and submit immediately. **Never wait for frontend publication when the chain already provides the required facts.**

## 2. Authoritative timing definitions

```
T_signal = local monotonic timestamp at receipt of the actionable signal used by this attempt
T_pre    = T_signal only when independent evidence proves that signal occurred before external tradability
T0       = first execution point at which an independent external BUY can validly execute
T_ready  = our exact BUY transaction/bundle is fully built and signed
T_queue  = signed payload committed to the selected local adapter/IPC queue, where applicable
T_send   = signed payload handed to the actual tested transport boundary defined for that adapter
T_buy    = our BUY actually executes successfully and acquisition evidence confirms the intended token was received
```

Desired product target:

```
T_buy - T0 < 1 ms
```

Important measurement rule:

Standard BSC receipts/blocks expose transaction ordering, not a trusted sub-millisecond execution timestamp for each transaction. Therefore Gate 0 MUST NOT invent a numeric `T_buy-T0` value from ordinary block timestamps.

For BSC the hard economic proof is therefore:

```
launch/activation transaction = position N
our successful BUY            = earliest valid position after N
successful competing BUYs between launch and ours = 0
```

If a trusted builder/validator trace later exposes intra-block execution timestamps, record the direct `T_buy-T0` value as additional evidence.

Every latency report MUST state the exact adapter-specific `T_send` boundary. An internal Rust queue alone is not `T_send`. Examples: for a local-geth path, report both Rust->IPC queue commit and the measured geth/network handoff; for a builder path, report queue/enqueue separately from the write/handoff to the live builder transport. Hidden movement of the timing boundary is forbidden.

## 3. Pre-lead

```
pre_lead = T_pre - T0
```

The desired design direction is negative lead, historically around `-10 ms` or better.

This is **not an assumed fact** and is **not a Gate-0 PASS criterion by itself**.

Do not derive an authoritative millisecond pre-lead from ordinary canonical block observation. A numeric pre-lead is valid only when the reference clock actually represents the first executable point with sufficient timing fidelity.

## 4. What Gate 0 must prove on BSC

Investigate Four.meme and Flap, then drive one exactly defined venue/version/mode through the complete first end-to-end proof before expanding scope.

1. current Mainnet contracts/versions/selectors and supported launch modes are frozen from verified evidence;
2. the launch transaction can be recognized before or at first external executability;
3. every mandatory BUY field has an explicit preparation record: source, earliest availability, precomputable portion, signature binding and invalidation conditions;
4. a minimal exact `launch -> BUY` path is semantically valid for the selected venue/version/mode before performance optimization begins;
5. pending/actionable coverage is measured as soon as launch detection is frozen and continues in parallel with G0.2/G0.3;
6. the complete controllable local path through `T_send` stays below the frozen latency target;
7. candidate ingress sources and candidate inclusion paths are tested as a matrix, not as isolated components;
8. controlled minimal-value Mainnet validation proves actual intended-token acquisition;
9. actual external BUY ordering, competing BUYs, realized costs and failure outcomes are verified;
10. PASS / ADAPT / STOP applies only to the exact proven scope; untested venue/version/modes remain unproven.

## 5. BSC ingress × inclusion paths are part of Gate 0

Do not postpone either early-ingress comparison or builder/bundle testing to a late LIVE phase.

Gate 0 compares complete paths:

```
ingress source -> actionable launch facts -> local BUY path -> inclusion adapter -> actual acquisition/order result
```

Minimum ingress candidates where currently available and relevant:

- own local bsc-geth / P2P full-transaction visibility;
- 48Club Auction Feed as a separate candidate evidence source where its current filtered simulation output contains the required launch facts;
- another independently verified early source only if it can provide losslessly attributable facts required by the exact BUY.

Minimum inclusion candidates:

- own local bsc-geth / P2P submission;
- 48Club/Puissant ordered or backrun-target bundle path where currently supported;
- BlockRazor ordered/bundle path where currently supported;
- another current BSC builder only if independently verified.

Do not assume an ingress source is useful merely because it is early. It must expose enough verified facts for the selected BUY scope, and its real coverage must be measured.

48Club currently documents that Auction Feed publishes backrun targets with simulated logs and, for the state variant, state changes; its current filtered topic list explicitly includes a Four.meme Create topic. It also documents that when a bundle is submitted with `backrunTarget`, the target transaction is automatically prepended ahead of the submitted bundle transactions. Gate 0 must test whether those feed fields are early enough, complete enough and available often enough for the selected scope. This is directly relevant to testing:

```
[target launch]
[our BUY]
```

Source snapshot:
- BNB Chain PBS/MEV overview: https://docs.bnbchain.org/bnb-smart-chain/validator/mev/overview/
- BNB Chain user guide: https://docs.bnbchain.org/bnb-smart-chain/validator/mev/user-guide/
- 48Club Auction Feed / backrunTarget: https://docs.48.club/puissant-builder/auction-transaction-feed
- 48Club Send Bundle: https://docs.48.club/puissant-builder/send-bundle
- BlockRazor support/docs index: https://www.blockrazor.io/support/

No builder is assumed fastest. Gate 0 measures the result.

## 6. Venue-specific minimum proof

### Flap

Freeze current BNB Portal/version and launch/buy semantics.

Current public documentation snapshot (reverified 2026-09-27) lists BNB Chain Portal v5.23.1 at `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`:
https://docs.flap.sh/flap/developers/deployed-contract-addresses

G0.1 MUST reverify and freeze the current deployment/version/address/ABI/selectors immediately before implementation.

Prove from current ABI/on-chain evidence whether token address derivation is deterministic before `T0`, then build the exact first BUY path.

### Four.Meme Classic

Freeze current TokenManager2 create/trade surface.

Current public Classic integration documentation states that new Classic launches use `TokenManager2.createToken` and emit `TokenCreate` containing the token address:
https://github.com/four-meme-community/fourmeme-docs/blob/main/docs/create-guide.md

Do **not** assume that the future token address is derivable from the pending create transaction. Gate 0 must prove this from current contract behavior/calldata or mark it unavailable.

### Four.Meme OpenFour

OpenFour is a separate modular protocol family and is not TokenManager2 Classic.

Current BSC Registry documented by the OpenFour integration guide:
`0x912CEf0C3aE9Ab6eB3Ec87cab69371cFb317Ab94`

Resolve and freeze `openFourCore()` and `openFourTool()` from that Registry at the G0.1 reference block. Current Core trade surface includes `buy(...)` and `buyByBudget(...)`; creation uses `createToken(bytes,bytes)`. A nonzero documented `presaleQuote` is a launch-internal creator acquisition.

Do **not** assume pre-execution token-address derivation for OpenFour. Prove it from the selected preset/create payload/deployer semantics or mark it unavailable.

If a candidate address cannot be known before execution, quantify the earliest valid alternative and its effect on first-external-BUY placement.

## 7. Minimal Gate-0 harness

Gate 0 may use a small native Rust harness:

```
gates/bsc/gate0/
  Cargo.toml
  src/
    main.rs
    clock.rs
    result.rs
    bsc_ingress.rs
    bsc_inclusion.rs
    flap_probe.rs
    fourmeme_probe.rs
    solana_pump_probe.rs
```

It may contain only what is required to answer feasibility:

- monotonic timestamps;
- launch recognition;
- minimal verified venue decode;
- token/address derivation where proven;
- minimal exact initial quote/fee requirements;
- nonce/gas/head state required for a valid BSC BUY;
- transaction build/sign;
- candidate inclusion adapters;
- result recording after the hot path.

Do not add Strategy C, research DB architecture, full reorg engine, full capture framework or GUI before Gate-0 PASS.

## 8. Gate-0 stages

### G0.1 Evidence freeze
Freeze current Flap/Four contracts, versions, selectors, launch and BUY semantics.

Before any value-bearing G0.7 attempt, freeze a numeric acceptance contract covering at minimum: trial count / minimum evaluable sample, denominator rules, minimum successful-acquisition rate, maximum revert/drop/failure rate, maximum late-BUY rate, maximum missed-actionable-launch rate, pending-coverage accounting, maximum authorized spend, concurrency/nonce stress requirement, and exact PASS / ADAPT / STOP thresholds.

Every attempt selected by the frozen sampling rule stays in the statistics. No post-hoc removal of unfavorable outcomes.

G0.1 crypto/evidence rule: do not use hand-written Keccak/ABI hashing code as an authority. Function selectors, event topics and runtime-code Keccak fingerprints must be produced by a vetted implementation and cross-checked against an independent reference where practical. A separate SHA-256 bytecode fingerprint may be retained as supplemental evidence but is not labeled an EVM codehash.

### G0.2 Pre-computability matrix + primary scope selection
For each candidate venue/version/mode, create a mandatory BUY-field preparation table before writing performance code.

Minimum columns:

| BUY field/state | Verified source | Earliest availability | Signature-bound? | Precomputable before signal? | Hot-path residual | Invalidation/rebuild condition |
| --- | --- | --- | --- | --- | --- | --- |
| token address | evidence required | measured/proven | yes/no | yes/no/partial | exact remaining work | exact invalidator |
| recipient | local config | startup | yes | yes | none unless changed | wallet/config change |
| buy amount/value | local policy + venue semantics | startup or launch-dependent | yes | yes/no/partial | exact remaining work | policy/state change |
| minimum output / slippage bound | verified local quote inputs | venue-dependent | yes | yes/no/partial | exact remaining work | reserve/state/input change |
| nonce | own account state | continuously maintained | yes | prepared lane/state | patch/finalize | competing/replacement tx |
| gas/fee fields | frozen policy + current required head/fee state | continuously maintained | yes | partial/full if valid | patch/finalize | head/policy invalidation |
| destination/selector/calldata constants | G0.1 evidence | startup | yes | yes | dynamic fields only | contract/version/mode change |
| launch activation/state dependency | verified venue semantics | venue-dependent | indirectly/yes | prove | exact residual | mode/state mismatch |

Add every other field that affects the signing payload or BUY validity.

Rules:

- prepare every static byte/state component before the actionable signal;
- pre-sign only when **all** signature-bound fields are already final and the signed payload remains valid for the intended launch;
- otherwise prebuild static serialization/template state and leave only the proven dynamic patch/finalization work in the hot path;
- unknown provenance or invalidation semantics => unsupported.

After comparing both venues, freeze one `PRIMARY_GATE0_SCOPE = venue + deployment/version + launch mode + BUY selector/mode` for the first complete proof. PASS later applies only to this exact scope.

### G0.3 Minimal correctness-first launch→BUY proof
As soon as G0.2 supplies enough verified fields for a minimal transaction builder, prove the smallest exact BUY path before fine performance work:

```
selected launch mode/state
-> exact required BUY inputs
-> exact transaction
-> launch -> BUY sequential validity
-> expected acquisition semantics
```

This proof must include launch-internal creator/presale actions, activation conditions, fee/tax semantics needed for validity, recipient and minimum-output rules.

Do not spend time on low-level latency tuning for a BUY path that has not passed this semantic validity proof.

### G0.4 Pending/actionable coverage — starts in parallel
Start bounded coverage collection as soon as G0.1 freezes recognizable launch evidence. Run it in parallel with G0.2 and G0.3 rather than waiting for their completion.

For each candidate ingress source record:

- launches observed;
- launches actionable with all fields required by the selected scope;
- first actionable local timestamp;
- missing/filtered fields;
- source gaps/reconnects;
- launches first learned only after the useful window;
- launches never visible through that ingress but later observed canonically.

The purpose is early falsification: if the selected scope is rarely actionable through an ingress source, discover that before optimization.

### G0.5 Local hotpath proof
Only after the G0.3 minimal BUY path is semantically valid, benchmark and optimize the complete controllable path through transport handoff:

```
T_pre
-> minimal venue decode
-> patch/finalize only the G0.2 hot-path residual
-> local quote only where G0.2 proved it cannot be removed
-> transaction finalize
-> sign
-> final serialization / adapter queue
-> T_send
```

Record every stage separately, including `T_ready` and `T_send`, so no post-sign work disappears from the budget.

Target: `T_pre -> T_send < 1 ms` on target-like hardware.

### G0.6 Ingress × inclusion dry qualification + concurrency validity
Before any value-bearing G0.7 trial, qualify the candidate matrix without claiming a real-race winner:

```
candidate ingress A/B/...
x
candidate inclusion path X/Y/...
-> field completeness
-> transport readiness
-> deterministic bundle/transaction construction
-> simulation/fork/sequential validity where applicable
```

G0.6 may compare diagnostic timing and adapter behavior, but it MUST NOT claim that a path wins real successful BUY placement before G0.7 executes controlled Mainnet trials and G0.8 evaluates their acquisition/order results.

Also prove that multiple simultaneously actionable launches do not create sender/nonce collisions, accidental replacement, head-of-line blocking beyond the frozen latency target, token/recipient cross-wiring, overwritten BUY inputs, duplicate economic submission, or shared-state races.

Field completeness is distinct from exact-mode live frequency/coverage: lossless V6 calldata and decoded mode fields do not require an additional randomly observed DOGE-valued sample. Exact-scope coverage still requires its canonical denominator in G0.4. Local bsc-geth/P2P is an optional candidate when unavailable on the current machine.

A synthetic allocation/projection PASS alone does not prove submission idempotency, recipient isolation or nonce-gap/head-of-line behavior. Invalid-wire transport qualification does not prove valid bundle acceptance. Preserve these limitations in the versioned candidate matrix; do not promote diagnostic p99 or unique TX hashes to a full concurrency PASS.

The same qualified ingress × inclusion combinations that survive G0.6 become the frozen candidate matrix for G0.7.

### G0.7 Controlled Mainnet validation
Only after dry qualification and after the G0.1 acceptance contract is frozen, use minimal user-authorized test value.

No unattended strategy trading is needed for Gate 0.

Execute the frozen G0.6 candidate matrix under the G0.1 authorized budget/attempt limits. Record every selected trial outcome, including successful BUY, revert, dropped/not-included submission, late BUY, missed actionable launch, launch not visible with sufficient pending data, and unsupported venue mode.

### G0.8 Successful-acquisition and ordering proof
For every controlled validation attempt record the full outcome.

A trial counts as `SUCCESSFUL_BUY` only when all required acquisition evidence is present:

- expected token address equals acquired token address;
- expected recipient equals actual recipient;
- acquired token amount is positive;
- token transfer/balance evidence is attributable to our BUY;
- actual debit plus measurable fees/costs are recorded;
- venue BUY execution is attributable to our submitted transaction/bundle;
- launch/BUY ordering is verified;
- successful competing external BUYs between launch and ours are counted;
- entry price versus earliest observable successful external BUY price is recorded;
- inclusion path is recorded.

A successful transaction receipt without acquisition evidence is not a successful BUY.

Launch-internal creator/presale/initial purchases are recorded separately as `LAUNCH_INTERNAL_BUY`; they are not externally overtakeable competitors. Aggregate acceptance uses the frozen G0.1 denominator and thresholds.

### G0.9 Builder-dependency classification

Also classify external builder dependency:

```
BUILDER_DEPENDENCY = NONE | BENEFICIAL | REQUIRED | UNKNOWN
```

- NONE: third-party builder access adds no material benefit.
- BENEFICIAL: it improves the result but is not strictly necessary.
- REQUIRED: the target result is not reproducible without builder access.
- UNKNOWN: evidence is insufficient.

If BENEFICIAL or REQUIRED, G0.10 evaluates an own-builder alternative before any permanent paid-provider assumption.

### G0.10 Own-builder feasibility

Evaluate only the infrastructure question:

- can the current official BSC builder interface be implemented/operated by us;
- what validator/proposer integration is required;
- what practical proposer coverage is available;
- what operating/maintenance cost results;
- whether an own builder could match the measured inclusion quality of the external path.

Gate 0 does not require the full own builder to be built. It records whether the later KB phase is justified.

### G0.11 Decision

```
PASS for the exact `PRIMARY_GATE0_SCOPE` only:
- G0.3 proved the exact launch->BUY semantics before performance tuning;
- the complete controllable path reaches `T_send` inside the frozen latency requirement;
- real validation proves actual token acquisition, not merely submission/receipt;
- earliest technically possible external BUY placement satisfies every frozen G0.1 acceptance threshold;
- ingress coverage and ingress × inclusion evidence are sufficient for the frozen scope;
- concurrency/nonce stress satisfies its frozen requirement.

No PASS automatically transfers to another venue, deployment version, launch mode or BUY selector/mode.

ADAPT:
- one or more frozen thresholds miss and the miss is attributable to a measured, realistically removable bottleneck.

STOP:
- after credible ingress/inclusion paths are tested, the venue cannot satisfy the frozen successful-acquisition / first-external-BUY acceptance contract.
```

## 9. Local latency classification

For the controllable local reaction path, not as a substitute for actual execution:

- <1 ms: target PASS
- 1–3 ms: ADAPT only with a concrete removable bottleneck
- >3 ms: current local path FAIL
- ~10 ms or worse: STOP that local architecture for Birth Sniping

## 10. Not valid as the primary Gate-0 success metric

Do not use any of these alone:

- block time;
- finality;
- canonical block timestamp;
- local decode time;
- signing time;
- local geth acceptance;
- RPC/builder acknowledgement;
- `EARLY_TX_SEEN - canonical observation`;
- a simulated fill.

## 11. Project rule

Gate 0 comes before:

- full BSC C0/C1/C2/C3 production build;
- full Flap/Four production decoder phases;
- 30-minute research capture;
- Strategy C BSC integration;
- PAPER;
- production LIVE;
- optional kernel/NIC/multi-region optimization.

Only a venue/chain that passes Gate 0 proceeds.

## Mandatory measurement/evidence ledger

Gate 0 uses `docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md` as the canonical chronological measurement log.

Before any dependent Gate-0 stage is declared PASS or started, every new empirical result that influences the decision MUST be frozen there (or linked there to immutable/raw evidence) with scope, sample count, includes/excludes, evidence path, interpretation and explicit non-claims.

No latency or correctness result may exist only in chat output, an agent summary or an unreferenced local terminal session. Historical results are appended or explicitly superseded; they are never silently overwritten.

This rule does not change the stage order. It makes each stage auditable and prevents later phases from reusing measurements outside their proven scope.


## G0.7 status freeze — 2026-10-01

Current measured state:
- Direct on-chain Flap Testnet launch -> BUY is functionally proven.
- Successful BUY acquisition is proven repeatedly.
- Same-block immediate ordering has been proven, including runs with zero successful BUYs between launch and DarkBot.
- Local actionable-signal -> BUY transport handoff is already below 1 ms:
  - 433,125 ns in one controlled run;
  - 535,250 ns in the raw-P2P/hash-fetch run.
- Therefore decode/CREATE2/build/sign/queue is not the present Gate-0 bottleneck.

Ingress is NOT qualified:
- Earlier public-WSS-controlled run: WSS launch observation was ~110.66 ms after launch handoff.
- First standalone P2P adapter stamped only after txpool admission and was slower than WSS.
- Moving the boundary before txpool separated the delay:
  - launch handoff -> first matching P2P hash: 133,377,959 ns;
  - P2P hash -> full transaction fetch: 101,263,875 ns;
  - actionable full P2P transaction: 234,641,834 ns.
- This proves the dominant delay is network acquisition/topology, not the local BUY reaction path.

Current P2P qualification blocker:
- The Chapel sensor does not reliably reach a 4-peer readiness threshold within 30 s.
- The four addresses embedded in reth-bsc are bootnodes; treating them as guaranteed transaction-feed/static peers was incorrect and has been reverted.
- Gate0 now logs peer-session establishment/closure, remote address, client version, negotiated ETH version, peer kind and live peer count before changing the architecture again.
- Current BNB Chain client development has moved its primary wire protocol to eth/70 while retaining eth/68 compatibility; the pinned bnb-chain/reth v0.1.2 defaults to eth/69/68/67/66 capability advertisement. This is a compatibility/topology item to measure explicitly, not assume away.
- BSC transaction propagation itself intentionally sends full non-blob transactions only to approximately sqrt(peer_count) peers and sends hash announcements to the remaining peers; transactions above the broadcast-size threshold are hash-announced and fetched. Therefore receiving BSC_P2P_HASH_FETCH is expected behavior for ordinary peers, and peer placement/selection materially affects earliest actionable full-TX time.

Stop condition:
Do not optimize the Rust BUY hotpath further unless a new measurement shows a regression. The next work item is ingress qualification: identify stable transaction-carrying peers / protocol compatibility, then compare direct full broadcast, hash-fetch, independent WSS control and any verified low-latency relay on the same controlled launch.
