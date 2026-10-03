# DarkBot BSC — Consolidated Production Build Master Plan

**Revision:** 2026-10-03 / consolidated implementation contract, strict acceptance amendment.  
**Intended reader:** Fable, implementer and reviewer of the BSC production runtime.  
**Product requirement:** actual successful acquisition, **`T0 → T_buy < 1,000,000 ns`**, strict inequality.  
**Non-negotiable acceptance:** this is a per-acquisition maximum, never an average, percentile, best-effort SLA or local-send substitute. Missing verification leaves the requirement OPEN; it never relaxes the threshold or authorizes a weaker release claim.  
**Local engineering requirement:** complete actionable ingress → actual transport handoff **`<1 ms`**, preferred **`<0.5 ms`**.  
**Authorization:** production construction is authorized; production LIVE remains blocked.  
**Current evidence:** local sub-ms paths and real testnet acquisitions exist; the product requirement is **NOT YET PROVEN**.  
**Import policy:** reviewed ZIPs remain isolated until corrected and accepted. This document does not import code or authorize a value-bearing test.

## 1. Purpose, precedence and boundaries

Build one production runtime from the demonstrated Gate-0 semantics and construction. Preserve the experiments' positive results, failures, measurement boundaries and scope restrictions. Do not rebuild historical experiments as a prerequisite to production construction.

**The tests and measurements are the specification of what was demonstrated.** Their exact scope, counterexamples and unknowns are part of that specification. They do not turn a historical sample into a universal latency, coverage, economic or inclusion guarantee.

This consolidated plan combines the supplied production plan, the Revision-2 review and the frozen Git sources listed in §19. Apply these rules when sources disagree:

1. The current human instruction authorizes construction now and preserves the actual acquisition target. It does not authorize production LIVE.
2. This plan governs production ownership, sequencing, acceptance and the precise measurement definitions below. It explicitly replaces older “C0 only after all Gate-0 PASS” scheduling clauses and ambiguous local-send definitions.
3. The Gate-0 ledger and raw evidence govern historical facts. A newer scope-specific correction takes precedence over an older ledger entry; never rewrite the experiment's original boundary or result.
4. The Gate-0 testplan and phase contracts supply retained safety, semantic and regression requirements. Scheduling exceptions above do not waive them.
5. Historical harnesses supply fixtures and counterexamples. They are not a mandate to copy their allocation patterns, process layout or single-sink transport implementation.

For a remaining contradiction, cite both clauses and the affected invariant, give the smallest correction and stop only the affected work. Do not silently invent a protocol, change the selected architecture, or reopen settled decisions without evidence.

Solana/Pump.fun remains intact and separate, with its existing work preserved. No Solana refactor or shared multichain framework is part of this build. Flap, Four Classic and OpenFour have separate qualification scopes.

## 2. What counts as meeting the goal

The path is direct on-chain:

```text
verified actionable pending/private-flow evidence
  → resident launch/state/economics
  → economic admission
  → exact BUY / ordered submission
  → qualified transport / producer
  → successful canonical token acquisition
```

No website, listing, chart, frontend API or browser is a BUY dependency. Nodes/builders/relays provide ingress or inclusion; they do not determine venue semantics or the strategy's quote.

Define:

- **T0:** the first successful execution boundary after which an independent external BUY may execute. For the qualified launch→BUY pair this is the successful launch transaction completion boundary, including launch-internal effects. An internal creator purchase is not an external BUY. If a supported mode enables trading later, qualify that later boundary explicitly.
- **T_buy:** completion of our successful BUY execution with acquisition and economic effects attributable to the intended token and recipient. A transient transfer in a subsequently reverted transaction does not count.
- **T_signal:** a declared source-observation boundary. Record hash-only, full-TX and first actionable observation separately.
- **T_ingress:** the earliest locally observed actionable boundary of the selected native ingress path. For geth, this precedes SPSC publication.
- **T_runtime:** Rust's consumption of that event. This secondary metric must not hide geth→Rust delay.
- **T_ready:** original completion of the exact signed, valid payload. It can precede T_signal; preserve that original timestamp. Payload selection has a separate T_select.
- **T_queue:** commit to a specific local queue/IPC boundary, when one exists.
- **T_send:** completion of a precisely identified adapter transport handoff, as defined in §8. It does not mean receipt, inclusion or purchase.

The primary test is **`T_buy - T0 < 1,000,000 ns`** on the actual successful producing execution attempt, with the conservative uncertainty bound in §9. At exactly 1,000,000 ns the requirement is not met.

First external BUY rank, same-block adjacency, candle placement, price and local latency are separate results. None substitutes for the primary test. For the initial birth-entry release profile, same-block launch/BUY adjacency (indices N/N+1) and zero intervening successful external BUYs are also mandatory placement gates, preserving the retained first-BUY objective. Count launch-internal creator purchases separately. A different supported activation mode needs its own explicit placement contract; it cannot silently weaken this profile. If suitable original execution timing is absent, report `T0_TO_BUY = UNKNOWN`, even if acquisition and first rank are proven.

“First candle” is not an alternative acceptance boundary. Any chart claim must declare its data source, bucket width and alignment and remain separate from acquisition timing and transaction order. No chart dependency is introduced into execution.

Work safely possible before T0 is intentionally moved before T0. Keep its duration and invalidation evidence; it is not part of the product interval. Network delay still affects whether the BUY reaches the producer in time. Do not subtract ping, RTT or “internet latency ×2” from acquisition latency. Price/OHLC cannot recover missing sub-ms execution timestamps.

The objective is mandatory; this plan does not claim it is already achieved or physically guaranteed. If measured producer execution or placement cannot meet it for a scope, record FAIL and its cause. Never change the meaning of “bought” to make the gate pass.

Completion means every required implementation and qualification gate for the declared release scope is satisfied with linked evidence. A plan, a code review, one successful attempt or a percentile is not completion. The campaign and release rules in §13/J apply in addition to the per-attempt timing rule above.

## 3. Frozen evidence and its exact production meaning

Counts below describe particular historical snapshots, not totals to be added across experiments. Preserve original files and hashes.

### 3.1 Flap deployment and historical semantics

The primary historical fixture is **newTokenV6 / tokenVersion 6 / TOKEN_TAXED_V3**, with the qualified DOGE quote/dividend and extension-mode scope. It is not newTokenV7 and is not the native-BNB Curve-20 profile.

```text
Portal:               0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0
BUY selector:         0xef7ec2e7
BUY signature:        swapExactInput((address,address,uint256,uint256,bytes))
Tax V3 implementation:0x024f18294970b5c76c0691b87f138a0317156422
Historical token:     0xabe1fe16cf6bc48c9524713054efe6e954b17777
Launch:               block 124415357, transaction index 0
First external BUY:   block 124415358, transaction index 15
First BUY hash:       0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800
First BUY amount:     6243568333969093909994622
Later direct BUY:     0x8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b
Later direct amount:  28438305464314215025968609
```

The first external BUY uses a Router→Portal path. The later direct BUY is a different transaction/state point. Neither output may be used as input to an “independent” quote calculation.

The corrected CREATE2 model reproduces the real token using the witnessed deployer, salt and exact EIP-1167 init-code/implementation binding; seven EIP-1014 vectors are regression sources. The specific historical verifier defect was **padding the 20-byte deployer to 32 bytes**, producing a wrong 97-byte preimage instead of the required 85 bytes. Preserve that exact negative vector, as well as wrong implementation/init-code rejection; do not rewrite the recorded root cause as an unspecified init-code error. See §20 SN02.

The historical semantic evidence includes 65 intervening transactions, 22 accounts/67 slots and 111 state checks; 237 callframes/100 logs; 106 launch storage changes and created-code/nonce effects. Treat these as the scoped execution witness, not proof of all BSC consensus/gas-settlement semantics or current deployment coverage.

### 3.2 Independent BUY/economics fixtures

The immediately executable post-launch **direct DOGE BUY** is a separate sequential EVM fixture:

```text
expected output:                    30669575743216741084405716
100-bps policy fixture minOutput:    30362879985784573673561658
minOutput = expected output:         success
minOutput = expected output + 1:     revert, no buyer acquisition
BUY before launch:                   revert
missing required executable code:   UNWITNESSED, never fabricated zero state
```

This scope is caller/state-bound and single-segment for the witnessed V3 conversion. It does not qualify arbitrary amounts, new callers, tick crossings or modes.

The independently crosschecked **native-BNB Curve-20** scope is separate:

```text
protocol fee:          100 bps
tax:                   100 bps, enabled in this profile
0.001 BNB output:      176664965031639901123273
0.0001 BNB output:     17669034223232267628301
```

Keep deployment/code-hash, tax toggle, caller exemption and all quote inputs attached to the fixture. The local EVM's positive output must not be supplied as execution input.

Derive rounding from witnessed runtime semantics:

```text
combined_fee = floor(q * (protocol_bps + effective_tax_bps) / 10000)
protocol_fee = floor(q * protocol_bps / 10000)
tax_fee      = combined_fee - protocol_fee
```

Use overflow-safe exact integer arithmetic with identical operation order. With 100/100 bps, tax can exceed `floor(q/100)` by one. Tax-disabled and exempt-caller branches require their own evidence and tests.

Production minOutput is strictly positive and computed from the qualified quote plus validated slippage policy. Reject invalid bounds; never default to zero or copy a historical/test minOutput as operational policy. Regression cases include slippage 0, 1 and 9,999 bps; 10,000 bps cannot produce an admissible positive minimum, values above 10,000 are invalid; zero quote, zero rounded minimum, overflow and invalid arithmetic are rejected. No unchecked subtraction or modulo wraparound.

Measured launch gas exceeded the old 5-million cap: estimate 5,502,725, 120% reserve 6,603,270, local EVM launch 5,223,342; local BUY 359,291. These are fixture results, not universal gas limits. Startup validates the selected launch/BUY limits and total budget against the qualified profile.

The recorded 1-gwei Stage-1 bid is a conservative policy, not a proven 48Club minimum. Recorded BlockRazor documentation gave 0.05 gwei; revalidate the actual API/fee conditions before live qualification. Low acceptance floors do not guarantee launch adjacency.

### 3.3 Unit 1 has now actually been checked

Revision 2 was compiled in an isolated arm64 Mac workspace with rustc/Cargo **1.93.1**:

- 27 tests PASS in debug and release; Clippy `-D warnings` PASS.
- Actual Rust `alloy-rlp` differential comparison: 27,624 decoder inputs PASS.
- Supplied formatting FAIL; isolated `cargo fmt --all` fixes formatting, subsequent check PASS.
- Cargo.lock generated offline for the checked workspace. The normalized review package contains it, no targets.
- Historical 343-byte mainnet transaction, EIP-155 example, short r/s, chain mismatch, low/high-S and 1,024 signing/recovery cases are regression sources.

This is scoped library correctness evidence. No production integration, new-library latency, all-dependency zero-allocation or exhaustive fuzz claim follows. Homebrew's matching compiler does not itself enforce rustup pins; CI must enforce the actual version and committed lock.

`gas_price: u64` is accepted for the explicitly bounded own-outgoing legacy BSC API, with overflow/range rejection. Foreign typed transactions use a distinct validated ingress representation. Do not expand this type's scope implicitly.

### 3.4 Preserve all local timing results and placement counterexamples

| Frozen experiment | Result | Work/boundary restriction |
|---|---|---|
| Solana/PAPER receive→enqueue | p99 2.9925 ms; max 3.559292 ms | Old Solana path; not a BSC sample |
| Warm historical JS decode/CREATE2/quote, 6,000 samples | p50 0.252 ms; p99 0.405 ms; max 2.229 ms | No TX build/sign/send/inclusion |
| Rust local UnixDatagram, 50,000 samples | p99 0.036750 ms; max 0.069417 ms | Full fixture build/sign through local datagram return; no geth/network/acquisition |
| Rust local stability, 10 × 50,000 samples | per-run p99 0.035333–0.037125 ms; worst max 0.550875 ms | 500,000 additional local datagram samples; no transport/inclusion qualification |
| Historical signed TX → local IPC, 100,000 samples | p99 0.003584 ms; max 0.019208 ms | Hex/framing/UnixStream only; excludes building/signing; no real geth |
| Rust integrated local IPC, 50,000 samples | p99 0.038875 ms; max 0.537042 ms | Decode/quote/build/sign/framing/UnixStream in one measured path |
| Early remote public-RPC diagnostic, 1,000 samples | 7 over 1 ms; worst run p99 1.042916 ms; worst max 1.466167 ms | Deliberately invalid wire; not a builder or acquisition measurement |
| Synthetic concurrent build/sign, 1,000 intents / 8 workers | p99 0.273792 ms; max 3.137834 ms; 5 over 1 ms | Timer starts after dequeue; no real submission/HoL/acquisition; later tests correct dedup/signature claims |
| Controlled dynamic testnet BUY | signal→send 0.433125 ms | Real acquisition; launch/BUY same block N/N+1 |
| Controlled raw P2P/fetch BUY | signal→send 0.535250 ms | Real acquisition; same block indices 0/1 |
| Independent HTTPS-launch/P2P testnet BUY | signal→send 0.617667 ms | Real acquisition; launch 134315024:5, BUY 134315025:1; following block |
| Prepared 48Club diagnostic, 100 samples | p99 0.584291 ms; max 0.602625 ms; 0 over 1 ms | Ledger-recorded prepared transport measurement; linked harness sends preframed padded read-only `eth_chainId`, not a signed BUY/bundle |
| Prepared BlockRazor diagnostic, 100 samples | p99 0.526584 ms; max 0.557 ms; 0 over 1 ms | Same padded read-only harness restriction; no valid BUY/inclusion proof |
| Earlier remote 48Club, 1,000 samples | 22 over 1 ms; worst recorded p99 1.950542 ms | Different setup/boundary; no hard maximum guarantee |
| Earlier remote BlockRazor, 1,000 samples | 19 over 1 ms; worst recorded p99 1.534583 ms | Different setup/boundary; no hard maximum guarantee |
| Current-target 48Club diagnostic, 100 samples | ready p50 0.255541 ms; send p99 1.642791 ms; 5 over 1 ms | Quote/build/sign unnecessarily after signal for a precomputable self-launch; diagnostic only |
| Current-target BlockRazor diagnostic, 100 samples | ready p50 0.274125 ms; send p99 2.216959 ms; 8 over 1 ms | Same construction restriction; not a prepared-path qualification |
| Real prepared full-signal BUY | signal→send 0.103417 ms | Acquisition PASS; following block |
| Real known-hash prepared BUY | signal→send 0.045125 ms | Acquisition PASS; following block; producer T0→BUY UNKNOWN |

Do not merge heterogeneous samples into one distribution or present them as benchmarks of the new library. In the current-target diagnostic, 0.274125 ms is **ready p50**, not full-send p99. Preserve the raw metric names rather than repeating an imprecise number from conversation.

The source-level recheck distinguishes two diagnostic families: `g06a_target_handoff.rs` intentionally corrupts wire transactions; `g06a_presigned_handoff_transport.rs` sends an inertly padded `eth_chainId` request. Its name and `prepared_before_T0` label do not prove signing, real launch timing, bundle-size equivalence or provider bundle processing. Preserve the ledger's measured numbers with their original provenance; qualify the exact production envelope separately. Older `over_1ms` counters use `> 1,000,000`; the strict production miss counter must use `>= 1,000,000`. Never silently reinterpret historical counter names. See §20 SN09–SN13 for code and result links.

The local IPC experiment measured signer p99 34.250 µs versus decode/CREATE2 0.709 µs, quote/minOutput 0.459 µs and framing 1.167 µs in that specific run. These stage percentiles cannot be summed into an end-to-end percentile. Preserve this evidence to avoid repeating basic feasibility work or replacing the checked signer without demonstrated benefit. Prepared paths move signing before the trigger; foreign-launch paths retain the measured cost of necessary final signing.

The most recent known-hash fixture is chain 97:

```text
launch: 0x0d8cbea2ea060158246a51379fb5dd2cf1310218ab796d3302eb2aebfbaa2ee4
BUY:    0xfcab0afaca413182e879e44dd87b7f86ff547e696e5c5d320b81869d55538d0a
token:  0xFC56AC387716B144294a6E896C21E6d523657777
input:  0.0001 tBNB
output: 17669034223232267628301
launch: 134533644:0
BUY:    134533645:0
```

All receipts for both blocks established the recorded Portal BUY rank, including internal router events. This does not turn the 450-ms header separation into execution latency. Earlier same-block successes remain valid evidence too.

Ingress diagnostics included roughly 133.378-ms hash acquisition plus 101.264-ms full fetch; another P2P path roughly 234.642-ms full arrival; latest launch handoff→hash 216.260291 ms and hash→full 148.544500 ms. Preserve each experiment's boundaries. They motivate early native observation and safe preparation; they do not establish an intrinsic P2P/WSS winner. The known-hash BUY did not wait for that later full fetch.

Retain the independent 0.617667-ms BUY's ingress split too: handoff→hash 182.209833 ms, hash→full 87.510417 ms, total actionable arrival 269.720250 ms. The earlier WSS control observed its launch 130.313708 ms before that run's P2P signal. These are separate runs, not interchangeable samples or proof of a universal source winner.

### 3.5 Intent, failure and recovery evidence

Retain the 8,000 signals→1,000 economic intents case, contiguous unique nonce allocation, cross-chain/sender ticket rejection, out-of-order preparation refusal, UNKNOWN-send HALT, no blind retry, permit-before-first-write and ACK≠confirmation.

Retain 256 final signed-envelope binding cases (ticket, sender, nonce, token, Portal and recipient). The eight test sender lanes do not authorize a production multi-sender redesign.

The 25-ms injected head hold establishes nonce head-of-line coupling. Its approximately 30-ms observed resolution interval is an artificial scenario, not measured real BUY latency or proof of a sub-ms violation. **Single sender remains the production baseline.** Measure real waiting/blocked-intent counts; do not hide them or infer an architectural change from the injected duration.

Restart-fence tests cover Reserved, Prepared, Sending and Confirmed quarantine; second owner, corrupt/empty fence and filesystem failures. Quarantine is not automatic recovery and the tests are not a hardware power-loss guarantee.

Preserve failures: no peers, readiness timeouts, disconnects, non-actionable/late hashes, HTTP 429/rate limits, archive missing state, stale/unqualified branches, partial sends, drops, reverts and gaps. Every attempt remains in the denominator.

## 4. Selected production architecture and ownership

Use **pinned bsc-geth + mmap/SPSC + one Rust runtime** as the selected baseline. Selection is an implementation decision; comparative latency superiority still requires measurement.

```text
bsc-geth native hooks ──mmap/SPSC──► Rust single-owner decision core
                                          │
                 bounded recorder ◄───────┤
                                          └──► qualified inclusion adapter(s)
                                                  ├─ local geth
                                                  ├─ 48Club
                                                  └─ BlockRazor
```

Modules do not imply separate processes, async tasks or queues. Do not insert a task/process/serialization hop for each trait or crate.

| Owner | Responsibilities |
|---|---|
| Pinned bsc-geth | EVM/chain protocol, native earliest useful TX observation, txpool/canonical/reorg hooks, peer provenance, local network handoff instrumentation |
| darkbot-wire | Bounded wire/event types and codec primitives, including the existing RLP unit; no transport |
| darkbot-fastlane | mmap/SPSC publication/consumption contract and boundary clocks; no business decisions |
| darkbot-core | Pure event/state/venue/economics/strategy/admission/TX/signing/intent semantics; adapter-neutral submission descriptions |
| darkbot-capture | Bounded capture, journal framing/integrity, offline derivations and replay input |
| darkbot-runner | Ownership/assembly, validated configuration, secrets, source/transport/TLS adapters, background reconciliation and startup fence |

Python may analyze frozen evidence offline. It is never an entry decision, quote, nonce or transport dependency. Historical Reth sensors remain evidence; do not create a second production P2P implementation from them.

Target workspace:

```text
bsc/native/
  Cargo.toml, Cargo.lock, rust-toolchain.toml
  crates/
    darkbot-wire/src/
    darkbot-fastlane/src/
    darkbot-core/src/
      event/ chain_state/ venues/ economics/ strategy/ tx/ signer/ inclusion/
      venues/{flap,four_classic,openfour}/
    darkbot-capture/src/
    darkbot-runner/src/{replay,paper,shadow,live}/
```

Keep one source of truth for decode, update, quote, decision, admission, TX construction, signing and intent transitions. Modes change sources and permitted sinks, not algorithms. Do not add empty placeholder crates/modules merely to match this tree; create concrete owners when their unit is implemented.

## 5. Resident-state and supported-scope contract

Translate the existing G0.2 matrix—17 groups, 277 storage dependencies, 497 classified rows—into an explicit, minimal input/admission contract for each supported execution profile. Do not walk all 497 rows per signal.

Every BUY dependency has:

```text
field / source / independent witness
startup-precomputed | pre-resident | launch-derived | signal-time residual
scope/profile / validity bound / invalidation event / refresh owner
```

At minimum bind:

- Registry, Portal/proxy/implementation code, code hashes and dependency closure.
- Launch mode, salt/deployer/init-code, collision/absence assumptions and token identity.
- Creator purchase and resulting curve reserves r/h/k, not just pre-launch reserves.
- Quote asset/converter, pool reserves, V3 price/liquidity/tick/bitmap window and amount bounds.
- Extension and oracle signature/digest, allowed role, expiry/time branches and admissible deadline margin.
- Caller, exemption, effective tax, tax toggle, protocol fee and exact rounding.
- Sender/recipient, nonce, funding/allowances where relevant, gas/bid/slippage/budget policy.

Publish a coherent immutable snapshot with identity:

```text
canonical_block_hash + ordered_state_cursor + pending_candidate_identity
                    + policy_generation + codehash_generation
```

The cursor represents ordered effects, not merely a block number. Event application, quote and prepared BUY must refer to compatible generations. Reorg, candidate replacement, proxy/registry update, relevant pool/fee/extension change, expiry or wallet/nonce change invalidates affected preparations.

No torn snapshot; no silently guessed missing state; no observed output used to seed the independent calculation. Unknown/stale/unsupported values cause SKIP/UNSUPPORTED with evidence; refresh occurs outside the entry hotpath. Do not assume the future inclusion timestamp or future pool state is known. Unqualified time-dependent branches or conversion paths cannot be admitted.

Code/profile constants may be versioned and compiled when witnessed. Operational gas, budget, slippage, endpoints and eligibility are validated configuration. Neither a verified ABI constant nor a regression fixture is forbidden “hardcoding”; mutable operational defaults pretending to be evidence are forbidden.

## 6. Two fast paths, one semantic implementation

### Prepared path

For a controlled or otherwise fully witnessed candidate, determine every signature-bound field early. Build, quote, reserve, sign and, where possible, frame the exact provider payload before T0. On the trigger, validate binding/generation, select the immutable payload and submit it.

Do not patch signed bytes. Changing nonce, amount, recipient, minOutput, gas or any signature-bound field requires rebuilding/re-signing outside the prepared fast path.

For a controlled self-launch ordered bundle, submission may occur **before launch execution**. Do not wait for the public hash echo when exact valid launch bytes are already known and the qualified provider supports pre-submission. This offers a route to placement at N/N+1 independent of a post-T0 internet reaction.

### External on-signal path

For an unknown third-party launch, prebuild templates and keep code/state/nonce/gas/signing/transport warm. After actionable full information arrives, derive only the genuinely unknown fields, quote locally, finalize and sign through the same functions, then hand off.

An unknown hash alone contains no launch fields. Hash-only may be actionable only when it identifies an already fully verified prepared candidate. Missing sender/nonce/to/value/input must be represented as absent, never zero-filled. Record hash→full delay separately and preserve original first-seen timestamps.

Before send, validate the chain, recovered intended signer, nonce, exact token/Portal, intended recipient and policy identity. Recorder checks after send are an audit, not retroactive permission to trust unverified input.

Benchmark and qualify both paths separately. A prepared self-launch PASS does not qualify the foreign full-launch path.

## 7. Nonce, economic intent and multi-adapter submission

One exclusive owner controls a bounded, preallocated single-sender nonce lane and global economic dedup. Port the Gate-0 transition invariants, not its `BTreeMap::insert` allocation or test Mutex.

Use bounded entry/dedup storage with declared capacities, deterministic assignment and explicit CAPACITY_EXCEEDED fail-closed behavior. Preserve dedup across the required session/recovery horizon; never silently evict an unresolved intent. Measure allocations and contention in the integrated runtime, not just output-buffer capacity.

Separate:

1. **Economic intent:** chain + sender + nonce + token + recipient + policy + exact signed TX hash.
2. **Submission:** that same economic intent through a selected adapter, with adapter-specific envelope and lifecycle.

Consume economic `begin_send` admission once, immediately before authorized egress. Mint bounded once-only adapter permits for the immutable selected set. Each permit is consumed at that adapter's first actual transport write. This must not invoke the old lane's `begin_send()` again for every builder.

Fan-out uses identical signed BUY bytes/nonce; where public launch bundling applies it uses identical ordered signed launch/BUY bytes. Never create a second BUY or reserve a second nonce to achieve redundant submission. Do not wait for adapter A's ACK before initiating the authorized adapter B submission. Adapters' available parallelism must not introduce a blocking hotpath wait.

Maintain per-submission states such as NotStarted, Writing, HandedOff, Acked, Rejected and Unknown, separate from intent states such as Reserved, Prepared, Sending, Included, Reverted, Replaced, Canonical, Orphaned and Halted. HTTP/ACK never means Included/Canonical.

An adapter partial-write/flush failure is UNKNOWN, not “unsent.” Record which permits already started; prohibit blind retry and halt further economic admission until reconciliation. Define and test whether any remaining not-started permit is suppressed on UNKNOWN; default is suppression. A previously started submission cannot be retroactively cancelled by bookkeeping.

Canonical/reorg evidence comes from the **single C2 chain-state/reconciler owner**. Unit 4 consumes its proof/interface and owns intent transitions; it does not implement a second canonical chain engine. Replacement requires an explicit qualified policy, economic identity/budget binding and canonical/pending evidence; no automatic retry loop.

Mandatory tests: original dedup/nonces/order/UNKNOWN cases; identical fan-out bytes and one nonce; reversed ACK order; rejection; partial write; duplicate signal; crash between permits; recipient isolation; capacity overflow; reorg and prepared-state invalidation. Measure actionable→permit waiting separately from build time.

Restart remains fail-closed. The startup runner establishes durable fence/ownership before trading, outside the hotpath. Any unresolved or untrusted recovered lane is quarantined. C2 may resume only after a separately tested canonical/pending/nonce/intent reconciliation proves safety. No filesystem write is introduced on the entry path to pretend that RAM state is crash-durable.

## 8. Inclusion and timing contracts

Support adapter-neutral economic plans with adapter-specific, versioned capability profiles. No provider decides the quote or strategy. API versions/options must be checked against official sources, pinned implementation/reference versions and frozen payload fixtures before qualification. An unverified feature is disabled.

Three distinct qualified cases:

- **Public full launch:** original `[raw_signed_launch, raw_signed_buy]`, without re-signing the third-party launch. For the retained controlled 48Club qualification profile, preserve `noMerge=true`, `positionFirst=true` and narrow `maxBlockNumber` when the verified API supports those exact meanings.
- **Private target:** explicit verified `backrunTarget`/simulation inputs and ordering semantics. Do not invent a public launch raw payload from an unknown private target or do hotpath RPC to resolve it.
- **Controlled self-launch:** qualified ordered pre-submission; do not broadcast the launch publicly first and then claim the private ordering experiment is unchanged.

Validate expiry, chain, funding, launch/BUY gas, bid/tips, total loss budget, signature/recipient and dependency generations before egress. Identical economic transactions may have different provider envelopes. Rate limits, rejects, expiry, reorg, unknown outcomes and ordering failures remain evidence.

### After-handoff validity and late execution

Local preparation invalidation, adapter request expiry and transaction executability are distinct. Removing a local payload, closing a socket, receiving a rejection/cancellation ACK or passing `maxBlockNumber` does not by itself prove that already disclosed signed BUY bytes can no longer execute. A block limit also does not enforce a sub-millisecond execution deadline inside that block.

Before qualifying an adapter set, freeze and test a capability record containing:

- Exact ordered/atomic submission semantics, permitted transaction extraction, rebroadcast/public fallback and downstream recipients of the signed bytes.
- Request target/expiry and its enforcing component; any transaction-level validity bound and its witnessed execution semantics; explicit absence when none exists.
- Invalidation/cancellation behavior before first write, during partial write, after handoff/ACK and after expiry, including delayed and contradictory responses.
- What evidence proves nonce consumption or makes the original BUY unexecutable; provider silence, a missing receipt and elapsed local time are insufficient.
- Residual ability to execute late, the owner of reconciliation and the response to an eventual late or orphaned acquisition.

Unit 5 owns transport capability evidence; Unit 4 owns the unresolved economic intent; C2 owns canonical reconciliation. Test expiry followed by delayed execution, extraction/rebroadcast, one adapter expiring while another remains active, reorg after apparent resolution, and restart with disclosed signed bytes. A simulation proves fault handling, not a provider's actual behavior; qualify the latter separately. All fan-out destinations must satisfy the selected release profile; a weaker public fallback cannot silently bypass it.

Once bytes may have escaped, keep the intent unresolved until sufficient canonical/recovery evidence exists. On invalidation or uncertain cancellation, suppress unstarted permits and halt new economic admission under the existing single-sender policy. Do not free/reuse the nonce, retry, replace or issue a compensating SELL automatically. Preserve eventual acquisition and cost even if it occurs after the campaign window. A late acquisition is a target failure, never a successful SKIP.

No current provider or BUY ABI is presumed to enforce the physical `<1 ms` deadline. Distinguish **measured campaign compliance** from **enforced future execution bounds**. A universal claim that every future BUY must complete within 1 ms requires separately reviewed end-to-end enforcement and timing bounds; missing enforcement is an explicit unresolved claim, not something a benchmark, cancellation option or local deadline guard can establish. Do not add an unwitnessed contract/wrapper or change the economic path merely to claim such enforcement.

### Timestamp compatibility

Preserve existing historical L labels rather than renumbering them:

```text
L0 = EARLY_TX_SEEN
L1 = Fast Lane publish
L2 = Rust receive
L3 = venue decode complete
L4 = decision + local quote complete
L5 = transaction/bundle built
L6 = signed
L7 = inclusion-adapter handoff
L8 = local geth acceptance, if applicable
L9 = first outbound peer handoff, if measurable
```

Named semantic boundaries govern new measurements. Record `boundary_schema_version`, `adapter_id` and `transport_boundary_id`. Legacy `local_send=L7-L0` remains a historical adapter-handoff metric, not automatically the new actual-network T_send metric. Record both if distinct.

For mmap/local geth, SPSC commit is T_queue; txpool acceptance is L8; measure the qualified actual outbound transport handoff separately. If that boundary is unavailable, actual T_send is UNKNOWN; do not substitute the IPC commit. Preserve first-peer versus selected-peer completion semantics.

For TLS/builder, distinguish payload selection, queue commit, write start, full qualified transport handoff and ACK. A buffering writer's `flush()` return is not automatically socket handoff. Instrument the actual specified layer and remaining buffering; do not claim NIC/wire delivery from a userspace write return. Qualification freezes what the adapter measurement does and does not cover.

Report at least:

```text
T_ingress → T_runtime
T_ingress → T_ready
T_ingress → T_send             # authoritative complete local reaction
T_runtime → T_send             # secondary Rust component
T_select → T_send              # prepared-path residual
queue waiting / HoL / framing / sign / transport work
T_send → ACK                  # separate, not part of completion proof
```

When original T_ready precedes ingress, preserve it without fabricating a post-signal ready timestamp. Full local reaction includes IPC, selection, all residual build/sign/framing work and the actual handoff. No moving a timestamp boundary to pass a gate.

The hard engineering target is `<1 ms`, preferred `<0.5 ms`. All deadline misses are recorded. Before first write, expired entry preparation is skipped; after any partial write it is UNKNOWN and reconciled, never reported as a safely cancelled BUY. Freeze the actual deadline clock/start and last-check placement per adapter. A last-check guard alone is not proof of a hard realtime maximum; finite measurements do not prove all future sends meet it.

There is no “1–3 ms acceptable production band.” Optimize misses and qualify the supported workload against the strict target. Report p50/p95/p99/max, full sample count, deadline-miss count/rate, blocked/skip counts and raw samples. p99 PASS cannot conceal tail violations.

## 9. Producer execution measurement and clock contract

To prove actual T0→T_buy, obtain measurements from the **original successful producing execution attempt**, linked to canonical launch/BUY transactions. A follower, archive replay or speculative losing builder simulation cannot recreate the producer's elapsed time. Their durations may be retained under different metrics.

Producer witness schema must exist before consumers are frozen, even if actual producer access is not yet available:

```text
schema_version, node_id, producer_boot_id, client_version, client_commit
clock_id, execution_attempt_id, parent_hash, block_hash
launch_tx_hash/index, buy_tx_hash/index
T0_before, T0_after, T_buy_before, T_buy_after
semantic_hook_ids, original_raw_timestamps
clock_resolution, hook/read_overhead, remaining_uncertainty
launch_status, buy_status, acquisition_evidence_refs
canonical_status, reorg_status
```

Bracket the declared successful completion boundaries, retain discarded attempts separately, and bind acquisition proof without adding synchronous logging/network work between hooks. Producer transaction/transfer timestamps must not use observer receipt time.

Acceptance uses a conservative upper bound:

```text
upper_bound_ns = latest_possible_T_buy - earliest_possible_T0
                 + remaining_non_overlapping_uncertainty_ns
PASS only if upper_bound_ns < 1,000,000
```

Uncertainty already covered by brackets is not counted twice. If hook meaning, canonical attempt identity or error bound is unavailable, the numerical result is UNKNOWN; it cannot pass. An instrumented failing run can disprove the target for that run. The timing schema itself proves no latency.

Every exported timestamp references a concrete **clock_id**, not just a class called MONOTONIC. Clock metadata includes host/node, boot, OS clock source, unit, resolution, uncertainty and source. Store constant metadata once and reference it from records.

Cross-process geth/Rust measurements require a common suitable OS clock or a measured mapping with error bounds. Go monotonic values and Rust `Instant` representations are not automatically comparable. Cross-machine/producer alignment likewise requires bounded mapping. Preserve raw units when normalizing seconds/milliseconds to ns; check overflow and never imply that ns storage creates ns precision.

The product objective concerns original execution after T0. Record producer timing separately from network arrival, local host reaction, pending lead, block order and fee economics. Neither subtract internet latency nor interpolate OHLC to manufacture a producer clock.

## 10. Entry hotpath and latency construction

Before eligibility is enabled:

- Resolve validated immutable config/profiles, ABI/selectors, code hashes and signing keys once.
- Preallocate bounded event, intent, raw/signing/framing and recorder storage; touch pages before qualification.
- Keep warm signing context, locally reserved valid nonce/funding/gas state and conversion windows.
- Establish and warm qualified sender/TCP/TLS connections; refresh outside entry execution.
- Prebuild/sign exact candidates when all necessary fields are valid; preframe provider payloads where possible.

Before actual T_send, do not perform synchronous RPC/HTTP **reads**, REST quote requests, DNS, filesystem/DB access, metadata/ABI/config loading, writer waits, synchronous logging, sleeping, handshake/reconnect or lock contention. Entry transport writes are the intended exception: sending necessarily performs transport work and that work is included in the measurement.

JSON persistence, reflection and general config parsing are forbidden on the entry path. Necessary builder wire framing/hex encoding is not categorically forbidden: precompute it where possible; otherwise implement bounded adapter-specific framing, measure it, and include it in the complete budget. Do not introduce a binary→JSON→binary business-event round trip.

Core remains I/O-free. Startup/reconnect/background tasks perform I/O without blocking the hot owner. Missing data/connection/capacity produces a recorded skip, not synchronous repair. Avoid unnecessary copies and process/task hops; record required borrow/copy lifetimes. Zero heap allocations and no locks are measured properties of the integrated eligible path, not assumptions based on Rust or buffer tests.

Optimize in order: preserve semantics → move work before T0 → resident coherent state → earlier actionable ingress → warm bounded local path → qualified pre-submission/order/arrival → tail and real placement measurement. Escalate kernel/NIC/topology tuning only on measured causes. A faster signer alone cannot fix a late feed or a next-block placement.

## 11. Mandatory raw capture and acquisition proof

Pre-arm bounded capture before each relevant launch/attempt. Preserve successes, skips, late/missed/reverted/dropped/unknown attempts and the full intended **`[T0, T0+15 seconds]`** market window.

### Raw evidence for later acquisition-time calculation

The purpose of this capture includes calculating **when acquisition actually completed relative to T0**, after the run, from original raw evidence. The arithmetic may happen later; the required original observations must be recorded during the actual execution. Retain the pre-launch preparation/submission segment as well as the full 15-second market window and subsequent reconciliation evidence. Fifteen seconds defines market coverage, not a retention/deletion deadline.

For every candidate attempt, preserve the original §9 producer completion brackets for T0 and T_buy, clock/boot/attempt identity and uncertainty metadata, and link them to the launch/BUY hashes, canonical block, receipts, attributable acquisition and costs. Producer records may be collected asynchronously or delivered later, but must originate from the original execution and carry verifiable identity. The BUY hotpath must not wait for their persistence or subsequent analysis. Export a durable journal with integrity manifest; do not overwrite/delete the evidence while qualification, a disputed outcome or reconciliation remains unresolved.

The offline evaluator must reconstruct the acquisition proof, verify the producing attempt/canonical identity and compute the conservative bound from these raw timestamps. Its report links every derived value to the exact raw records and manifest hashes. Preserve raw units/resolution and all failed/unknown attempts. Required recorder tests include delayed producer-witness arrival, missing witness, mismatched execution attempt, corrupt/truncated capture and replay of the derived acceptance verdict.

If original producer timing was never recorded or cannot be obtained, storing more receipts, block headers, trades, OHLC or observer-arrival timestamps cannot recover the missing sub-ms execution interval. Mark `ACQUISITION=PASS` separately where supported and `T0_TO_BUY=UNKNOWN`; the mandatory timing gate remains NOT PASSED. Do not rerun the EVM later and label that duration the historical purchase time. Missing original timing requires a newly instrumented qualification attempt, not a relaxed target or a fabricated retrospective timestamp.

**Unchanged hard rule:** an acquisition meets the requirement only when its conservatively bounded original `T_buy - T0` is strictly below 1,000,000 ns. Exactly 1,000,000 ns, a larger bound, or UNKNOWN never yields timing PASS. The 15-second capture window creates no 15-second, block-sized or percentile-based purchase allowance.

If exact producer T0 is absent, retain a conservative pre-trigger host window and canonical block superset separately, with boundary blocks and canonical backfill through at least the launch header's 15-second horizon plus a boundary block. Declare the basis and `t0_exact=false`; do not mix host-monotonic and header epochs arithmetically. If their alignment/coverage is unbounded, exact-window completeness remains UNKNOWN/INCOMPLETE.

Reconcile after the observation window until canonical status/coverage is established. Fifteen seconds is not the cutoff for receipts, reorg processing or reconciliation.

Raw capture contains, where available:

- Original pending hashes/full bytes, sources/peer/protocol, first-seen/full/actionable times and source health.
- Launch, our intents/preparations, every selected submission and response/error, all timing boundaries.
- Full blocks/TX ordering and complete receipts/logs through the window and boundary blocks.
- TokenCreated/TokenBought/TokenSold/Transfer, including router-internal Portal events.
- Creator launch-internal purchases separately from independent external BUY/SELL rank.
- Atomic input/output amounts, asset identity, proven decimals, fee/tax/converter effects, native debit, gas and builder tips.
- Intended recipient positive acquisition, canonical/reorg results, missed events/gaps and producer witness if available.

Acquisition result requires correct token, recipient, positive attributable received amount, attributable debit/cost and successful canonical execution. Receipt status alone is insufficient. Distinguish gas, transferred value, internal fees and net wallet debit; do not double-count fees already inside the debit. Average fill price and posttrade spot price are different values.

Derive competition from complete receipts/logs, keyed by chain/block/TX/log index, not top-level transaction destinations alone. Derive same block, N/N+1, intervening successful BUY count and external rank independently of timestamp proof.

### Journal and replay

Use a versioned append-only binary journal with bounded frames, source/session/boot identity, raw bytes or lossless fields, checksum and hash manifest. Preserve both source_seq and journal_seq; define per-source/ring gap/overflow semantics. If a missing sequence span cannot be known, record that fact rather than inventing a precise interval.

Recorder publication never blocks the BUY owner. Preallocated ownership/lifetime rules prevent a borrowed event buffer from being overwritten before recorder consumption; no dangling asynchronous references. Overflow/loss is explicit and prevents COMPLETE while allowing the BUY path to continue.

Tests cover wraparound, truncated/corrupt frames, version mismatch, producer/consumer ownership, overflow, restart provenance, reorg and deterministic replay identity. Replay reads the same events into the same Core, with no alternate quote/strategy implementation. JSON/SQLite/Parquet/OHLC are offline derivatives, not a second primary evidence journal.

OHLC/ticks are computed only at the actual underlying event resolution and with recorded aggregation rules. A seconds-only event remains seconds-resolution after aggregation; prices do not uniquely identify elapsed execution time.

## 12. Ingress, C2 state and control-plane counterexamples

Ingress adapters preserve source identity and capabilities; Fast Lane baseline is the earliest useful native geth hook, not merely late txpool-import observation. Record early observation separately from txpool validation/acceptance. Data may arrive early, but cannot become actionable before the required trust/binding check.

Across enabled qualified ingress sources, **first verified actionable information wins** for the same candidate. Do not serially wait for a slower preferred source; preserve per-source first-seen/hash/full times and global economic dedup. A known prepared hash and an unknown foreign hash have different actionability contracts. Record peer provenance before filtering/forwarding can discard it and rank peers by relevant actionable/full arrival, not ping alone. The historical Reth sensor is a diagnostic reference, not a replacement for pinned geth or a ready generic foreign-launch decoder.

Historical capture showed five V6 launches of type 0 and 46 Portal pending transactions (43 type 0, three type 2). That is scope evidence, not a universal legacy-only claim. Decode supported foreign transaction envelopes with explicit type/chain validation; unsupported types produce evidence. Own outgoing legacy encoding remains separate.

SourceHealth includes peers/readiness, disconnect/reconnect, subscription/fetch state, rate limits, gaps and recovery. WSS/private feeds are optional separately measured adapters; they do not silently substitute for or qualify native geth.

Retain the positive G0.4 evidence precisely: the bounded Coverage-v2 census found **4/4 canonical V6 launches pending-covered**, but **zero frozen DOGE-scope launches**, so that scope's coverage remains UNKNOWN. The separate 18,148-full-pending run with zero exact matches establishes neither missing V6 fields nor a venue failure. Do not demand another identical DOGE sample merely to re-prove the already witnessed ABI field completeness; measure the remaining scope/coverage question. See §20 SN07.

C2 is the single canonical/reorg/candidate owner and must port the counterexamples from the Python-retirement and state contracts:

- First-seen is preserved; later full/enrichment never overwrites it.
- Multiple sender/nonce candidates are tracked; observation order alone does not prove replacement.
- Canonical state exists independently of pending observations; late pending cannot rewrite inclusion.
- Receipt/log identity is unique and internally routed events are retained.
- Bounded reorg ancestry invalidates affected quote/preparation/intent provenance.
- Gaps, backfill and dropped/replaced/orphaned outcomes are explicit.
- Restart quarantine precedes any controlled recovery; unknown sends are not automatically resubmitted.

Define bounds/retention, memory pressure and overflow behavior. Unknown state disables affected entries until reconciliation; never authorize with speculative zero values.

## 13. Production phases and their acceptance owners

This is the production sequence **C0–J**. Historical Gate-0 G0.x numbering remains historical; do not equate a production phase with a Gate-0 PASS. C0 units are incremental implementation deliveries inside foundation work, not six independent runtimes.

### C0 — foundations and scoped first-venue units

Deliver reproducible workspace, identities/event/clock/journal contracts, bounded recorder interface, replay foundation and scoped primitives below. Freeze consumer contracts before implementing their consumers. Initial supported profiles are explicit; extension belongs to later phases.

**Unit 1 — RLP, legacy TX, signer.** Preserve the checked Revision-2 logic. Take the normalized format/lock corrections and update the inventory to actual compiler PASS; no algorithm restart. Keep one public validated sign-and-encode path, crate-private foreign-signature encoding, expected-chain recovery, intended-sender checks at admission, low-S policy and cleared output on error. Port golden/malformed/differential/signature/buffer cases. Do not label finite differential cases exhaustive fuzzing. Bound permitted calldata and report remaining arbitrary-untrusted-input coverage honestly. Import only the accepted corrected package, no build targets.

**Unit 2 — Flap protocol.** One ABI/decode/BUY/CREATE2 implementation, with exact deployment/profile hashes, V6 tax-mode fields, provenance and unknown-mode failure. Retain seven CREATE2 vectors, historical real address, wrong-init-code negative case, exact calldata and final token/recipient binding. Typed ingress validation is a separate boundary from this venue decoder. No generalization beyond supported code/mode evidence.

**Unit 3 — scoped Flap economics.** Implement native-BNB Curve-20 and the exact qualified DOGE fixture profile independently, using common witnessed arithmetic where genuinely shared. Port rounding, exemption/tax, quote, creator-state, slippage/overflow, minOutput success/+1-revert/before-launch and independent EVM cases. Publish the input/invalidation contracts of §5 before acceptance. Unsupported conversion/tick/time/caller branches fail closed.

**Unit 4 — intent/nonce runtime.** Implement the pure bounded single-sender economic state machine and adapter-permit contract of §7. Consume canonical proofs through the C2 interface. Add original concurrency, recipient, fail-closed, artificial HoL, startup quarantine and new fan-out/capacity tests. Full chain reconstruction/recovery belongs to C2; do not implement it twice.

**Unit 5 — inclusion adapters.** Implement local-geth, 48Club and BlockRazor only to their verified supported capability scopes. Runner owns transport/TLS/provider framing; Core owns immutable economic/submission descriptions. Port ordered public/private/self-launch cases, expiry/budget, same-bytes fan-out, per-adapter timing, ACK≠confirmation and all failure cases. Missing provider access is an explicit qualification gap, not an invented PASS or reason to rewrite Unit 1.

**Unit 6 — typed startup config/secrets.** Validate chain/profile/code generation, API/endpoints, nonce/funding/gas/bid, amounts/slippage/budgets, clock/capture/capacity and eligible adapters. Resolve a startup authorization capability; do not reread a file before each BUY. Background revocation/invalidation updates immutable generations. No secret-bearing URLs, keys or secret contents in logs/journal/Git. No unfunded/missing-secret default that silently enables live execution.

C0 includes a working minimal recorder/replay foundation and schemas, not just future logger placeholders. Do not require later strategy/producer-access measurements to freeze a scoped RLP unit; record their later owners in the acceptance matrix.

### C1 — pinned bsc-geth Fast Lane

Pin geth source commit/build and implement earliest useful TX, acceptance, canonical/reorg and outbound hooks with peer/source provenance. Define wire version, bounds/framing/endianness/alignment, ownership, acquire/release memory ordering, wraparound, backpressure, crash/restart/shutdown and corruption behavior for both SPSC directions.

Measure native hook→publish→Rust receive and local outbound handoff with compatible clocks. Test data races/torn records, producer/consumer restart, overflow and generation mismatch. Capture/reconciliation must not block the trading consumer. Demonstrate the selected hook is early enough for the intended scope; mmap alone proves nothing about arrival advantage.

### C2 — canonical/pending state and controlled recovery

Implement the single state/reconciler owner from §12; scoped coherent cache publication from §5; quarantine plus explicit safe-resume proof from §7. Port every applicable historical state counterexample with source and test owner. Separate observed candidates, canonical effects and confidence. Freeze usable profiles only when their live dependency refresh and invalidation coverage is proven.

### C3 — integrated local execution qualification

Measure production geth→SPSC→Rust→venue/state/economics/admission→build/sign/framing→selected actual transport boundary. Qualify prepared and foreign on-signal paths separately, each adapter boundary separately and required fan-out behavior. Include cold/stale/reconnect SKIPs, queue/nonce stress, recorder load and failure denominators.

Use dedicated Linux target hardware and preserve raw samples. Report min/mean/p50/p95/p99/p99.9/p99.99/max, sample count, misses, eligible/skipped/failed counts, queue depth, drops, HoL, measured allocations/locks, hardware/client/config/build and evidence hashes. Label extreme quantiles with their sample support; do not pretend a 100-sample p99.99 establishes a reliable extreme tail. Require the strict `<1 ms` complete-local target; optimize toward `<0.5 ms`. No p99-only maximum claim and no exclusion of residual framing/IPC. Preserve measurement overhead/clock bounds. C3 is not a producer-acquisition PASS.

### D — full supported Flap decoder

Extend the qualified V6 subset into explicit supported pending/canonical Flap profiles using the same implementation. Deployment upgrades, event/selector variants and additional modes each need witnessed semantic fixtures, invalidation and unsupported cases. “Full” means the declared supported scope, not invented future protocols.

### E — Four Classic / OpenFour

Separate protocol packages, deployment/token derivation/launch/BUY/economics evidence and qualified profiles. No reuse of Flap or one Four family's assumptions by name alone. Add scopes progressively; failure or missing fixtures in one must not authorize it or destabilize frozen Flap.

### F — production capture and market coverage

Run native passive capture with the actual journal and prearmed 15-second windows. Record canonical supported launches as denominator, seen/missed/actionable full arrivals, timing spread, modes, gaps, source health and replay identity. Freeze sample duration/workload/coverage criteria before running; five seen launches alone is not a coverage guarantee. Preserve complete acquisitions and competing internal Portal events.

Retain the existing Phase-F requirement of **one clean continuous 30-minute native binary journal**, with manifest and exact-core replay parity, in addition to per-attempt 15-second windows. Do not silently replace the 30-minute coverage run with a few launch windows. A longer campaign or required additional scope remains governed by its frozen manifest. Failed captures remain diagnostic evidence. See §20 SN17.

### G — complete supported economics/cost

Complete all supported fee/tax/conversion/quote/gas/builder-cost scopes once in Rust, with independent golden/negative evidence. Dynamic amounts/tick crossings/caller differences require new qualification. Total cost and positive minOutput are risk/admission inputs; no replay-only calculator. Requalify any change to the signature-bound policy or prepared dependencies.

Before Strategy C/production economics acceptance, retain supported BUY **and SELL/exit**, approval/permit, included-revert and migration/graduation cost cases from the Economics Gate. Charge realized gas from receipt `gasUsed * effectiveGasPrice`; do not charge nonexistent on-chain gas to dropped candidates or count one nonce lifecycle twice. Freeze a versioned economics snapshot. Unknown exit/migration costs disable the affected strategy scope; no assumed-zero cost and no strategy tuning inside the economics gate. See §20 SN18.

### H — Strategy C

Port the supported strategy once into Rust after its BSC state/market/economic evidence is established. Maintain deterministic replay parity and explicit risk/admission limits. Solana observation-time replay and lamport costs do not become BSC execution-time or native-fee facts. Fast acquisition does not prove positive expected value.

### I — exact-core Replay / PAPER / SHADOW

Modes use the same complete D/E/G/H semantic pipeline with explicit permitted sources/sinks. Requalify full local latency, allocation/queue/nonce/fan-out/capture stress and canonical/acquisition derivations after strategy integration. No PAPER shortcut qualifies LIVE. Where only Flap is initially enabled, deferred Four scopes remain disabled and cannot claim PASS.

### J — LIVE release gate

Production LIVE requires reviewed supported scope, validated funding/authorization, state/recovery/risk qualification, real inclusion/acquisition evidence and the required original producer-timing qualification. **The strict T0→T_buy target is not waived by a local benchmark or unavailable producer access.** Construction can continue while that track is open; production release cannot claim the objective met.

#### Frozen qualification campaign

Before collecting acceptance samples, Fable must deliver a concrete, versioned campaign manifest for review. Bind its hash to every result. The manifest must contain actual values, units and owners for:

1. Release code/build/config hashes, chain/deployments, venue/amount/caller profiles, strategy/risk policy, hardware, sources and exact adapter/fan-out sets.
2. Separate prepared-self-launch, external-full-launch and private-target cells where enabled. No transfer of a controlled self-launch result to foreign launches, testnet to mainnet, one venue to another or one adapter set to another.
3. Positive minimum attempt count per enabled cell, minimum observation duration, workload/concurrency and failure scenarios, with a written rationale for the chosen sample size and intended reliability claim. A single success is never a production campaign. These values are not inferred from historical diagnostic sample counts.
4. Objective launch eligibility and strategy-selection predicates, canonical launch denominator, source/actionability coverage floor, maximum skip/miss rates and maximum consecutive misses. Express every rate with its exact denominator. Evaluate selection with the same frozen Core and sufficient independent canonical data so a missed feed cannot silently remove a launch from eligibility.
5. Required producer witness access, clocks/error bounds, placement predicate, complete 15-second capture criteria, canonical/finality acceptance rule and reorg/reconciliation horizon. An elapsed observation window alone does not resolve pending outcomes.
6. Funded-test authorization reference, amount and aggregate cost limits, permitted sinks, stop conditions and responsible reviewer. This manifest is not itself authorization to send.

Unfilled fields, unavailable evidence, an unjustified sample-size choice or an unreviewed manifest mean **CAMPAIGN_NOT_READY** and block campaign acceptance/J. Earlier scoped construction may continue. Sample sizes and availability thresholds require an explicit reviewed decision; this document does not invent a statistically justified universal count or silently accept a permissive default.

Freeze the manifest before results are known. Do not end a campaign after a lucky success, drop slow samples, redefine eligibility after observing misses or merge heterogeneous cells. Report planned, observed, selected, admitted, skipped, written, acquired, reverted, dropped, late and unresolved counts. Record skip reasons at the decision boundary; an admitted intent cannot later be relabeled as ineligible because it waited for nonce, transport or state. The declared runtime admission boundary must precede that waiting and be independent of eventual success.

#### Attempt, campaign and release verdicts

- **Attempt PASS:** correct successful acquisition and economics; required canonical status; complete original producer witness with `upper_bound_ns < 1,000,000`; mandatory placement; complete required capture; and all applicable local/state/intent invariants. Preserve separate component verdicts.
- **Attempt FAIL:** a proven revert/drop, wrong acquisition, missed required placement, measured upper bound at or above 1,000,000 ns, violated local deadline or other required invariant. An upper bound crossing the threshold fails conservative acceptance even when the exact physical duration is uncertain. Absence of sufficient evidence is UNKNOWN rather than an invented numerical failure or success.
- **Campaign PASS:** every enabled cell meets its frozen minimum sample/duration/workload and coverage/skip requirements; every admitted attempt is PASS; zero required timing/placement/invariant violations; zero unresolved required evidence or capture gaps. Safe pre-admission skips remain in the coverage denominator and must satisfy the frozen availability limits. Zero admitted attempts cannot pass.
- **Release PASS:** campaign PASS on the exact release scope/build, complete implementation matrix, tested runtime enforcement of the qualified configuration and valid explicit LIVE authorization. Self-launch-only evidence cannot release a product advertised for third-party launches. A deferred capability stays disabled and visibly excluded from the release claim.

One violation prevents that campaign from passing; improving the p99 or adding more fast samples cannot erase it. Correct the cause, rerun affected regressions and execute a new complete frozen campaign. Retain the failed campaign and its provenance. A configuration/scope change creates a separately reviewed qualification, not a retrospective rewrite.

Runtime eligibility must be bound to the qualified profile, build/config generations, source/adapter health and validity/capacity state. Unknown or changed required conditions disable affected entries before egress. A discovered timing/placement violation or missing required acquisition evidence halts new admission and triggers reconciliation; an already disclosed transaction remains governed by §8. Re-enabling requires resolution and the applicable reviewed requalification. This reaction cannot undo a late purchase or guarantee future inclusion timing.

Campaign PASS is a measured result for a frozen scope. It is not proof of an unconditional maximum across all future network/producer behavior. A requested universal hard-bound claim remains BLOCKED unless the distinct enforcement proof in §8 exists; never present a finite campaign as that proof or silently substitute a weaker product promise.

#### Acceptance evaluator regressions

Implement the verdict rules once and test at least: 999,999 ns accepted versus 1,000,000 ns rejected after uncertainty; missing/mismatched producer attempt; successful send without acquisition; same-block acquisition without timing; fast acquisition with wrong placement; one slow attempt among otherwise fast samples; missing capture/reorg resolution; all-skipped/zero-attempt campaign; insufficient per-cell count/duration; post-hoc manifest change; self-launch evidence offered for foreign-launch release; expiry/cancellation ACK followed by a late acquisition. Synthetic cases test verdict logic only and never count as real acquisition samples.

Any narrowly controlled value-bearing qualification experiment is a separate explicit authorization with frozen amount/chain/token/budget, source/sink and measurement plan. It is not an unattended-production waiver. No value-bearing send follows from this document alone.

## 14. Integrated evidence → production acceptance matrix

Maintain this table in the master plan as the human-readable acceptance source. A machine-readable companion may reference these IDs; do not create divergent status reports. Each implemented row gains source commit/hash, scope/sample count, exact test name, raw result, what is excluded and future requalification. Never fill PASS by hand without linked results.

| ID / retained evidence | Owner / required regression or measurement | Current consequence |
|---|---|---|
| W1 — EIP-155, historical 343-byte TX, short r/s, 1,024 signatures | Unit 1: byte/hash/chain/sender/low-S/output/buffer tests | Checked Revision 2 PASS; normalized import pending |
| W2 — malformed RLP and 27,624 actual differential inputs | Unit 1: real Rust differential and bounded decoder error cases | PASS for delivered cases; no exhaustive-fuzz claim |
| P1 — V6 TAXED_V3, real CREATE2, seven vectors, corrected mismatch | Unit 2: deployment/salt/init-code/address golden and wrong-mode rejection | Evidence exists; production port/test pending |
| P2 — 65 TX witness, Router→Portal and direct call, independent output | Units 2/3: independent semantic fixtures tied to actual prestate/caller | Historical scoped PASS; generic live semantics not qualified |
| Q1 — 497 dependency rows, coherent snapshot/invalidators | Units 2/3 + C2: explicit profile inputs; stale/reorg/mode/code/caller negatives | Witness classification exists; production live cache coverage pending |
| Q2 — immediate DOGE BUY, minOutput exact/+1, pre-launch revert | Unit 3: sequential EVM golden and negative tests, no observed-output input | Scoped golden exists; other conversion segments disabled |
| Q3 — native-BNB Curve-20 and exact fee rounding | Unit 3 + G: independent Rust/EVM, tax toggle/exemption/slippage/overflow | Scoped golden exists; operational policy validated separately |
| N1 — 8,000 signals/1,000 intents, cross-chain and ordering failures | Unit 4: bounded dedup/nonces/recipient isolation/UNKNOWN regressions | Historical PASS; new production data structures pending |
| N2 — 256 final-envelope bindings, short signatures | Units 1/2/4: intended chain/signer/nonce/Portal/token/recipient final binding | Eight test senders are not a multi-sender requirement |
| N3 — injected HoL, startup quarantine in all four states | Unit 4 + C2: coupling telemetry, startup failures, controlled recovery proof | Coupling/fence demonstrated; real HoL and automatic resume not proven |
| S1 — permit-before-write, ACK≠confirm, partial-write HALT | Units 4/5: once-economic/per-adapter permits and fault matrix | Historical single-sink PASS; fan-out lifecycle requires new tests |
| S2 — ordered self-launch/public/private inclusion, gas/budget/floor evidence | Unit 5 + Unit 4/C2 + C3/J: exact options/payload bytes, cost/expiry/placement; §8 after-handoff validity and delayed-execution regressions | API acceptance/diagnostic is not real N+1 qualification; request expiry is not transaction revocation |
| T1 — 0.433125/0.535250-ms real same-block BUYs | C3 + inclusion track: retain scope/order/acquisition; new exact-runtime measurement | Historical constructive evidence; producer elapsed time UNKNOWN |
| T2 — prepared remote diagnostics and tails >1 ms | Unit 5 + C3: separate prepared/full path and per-adapter tails/all outcomes | No universal “every send <1 ms” claim |
| T3 — 0.103417/0.045125-ms real following-block BUYs | Unit 5 + C3/J: preserve counterexample, optimize arrival/order | Fast local send does not guarantee first-block acquisition |
| T4 — clock/actual-send/producer witness | C0 schema + C1/C3 + J: bracket original successful execution, conservative bound; frozen per-cell campaign and verdict evaluator regressions | Actual T0→T_buy <1 ms NOT PROVEN; no campaign or universal-bound claim without its distinct proof |
| R1 — no-peer/disconnect/hash/fetch/rate-limit failures | C1/F + runner adapters: source health, actionable binding, full denominator | No feed/P2P coverage winner presumed |
| R2 — pending/canonical/replacement/reorg counterexamples | C2: every applicable retirement-contract counterexample with exact test | No parallel chain-state engine in Unit 4 |
| C1 — first 15 s, complete receipt/internal-event acquisition evidence | C0 recorder + F/I/J: bounded raw capture, explicit window/completeness | Minimal schema alone is not implemented COMPLETE capture |
| C2 — journal/borrow/sequence/checksum/replay | C0 recorder + C1/F/I: lifetime, corruption/gap/restart and identity tests | No hotpath writer wait; derivatives are offline |
| X1 — old Solana/PAPER latency and Research Strategy C scans | H: separate observation semantics, BSC fee/EV validation | Preserve Solana; no BSC-EV/timing inference |

“Pending” here is an implementation/qualification task, not a claim that the historical experiment failed. Attach completed results as units land. Full source details remain in the original ledger/raw files and provenance manifest; do not copy all raw traces into this plan.

## 15. Definition of Done without phase deadlock

For each unit/phase, freeze its declared supported scope and required rows first. A row is one of:

```text
IMPLEMENTED_TESTED   — linked code/test/result matches the retained invariant
QUALIFIED_MEASURED   — linked original measurement, scope and uncertainty
TRACKED_DEFERRED     — named later phase, dependency and disabled affected capability
BLOCKED             — concrete missing input/access/contradiction
UNMAPPED / AMBIGUOUS / UNTESTED — unresolved required work
```

FROZEN is allowed only with no known unresolved defect and no UNMAPPED/AMBIGUOUS/UNTESTED/BLOCKED **required row inside the unit's scope**. Future producer access does not block Unit 1; it is tracked to J and blocks the product-timing qualification. TRACKED_DEFERRED cannot waive a current requirement, enable an unsupported branch or authorize LIVE.

Mapping a test is not passing it; passing a library test is not qualifying the integrated transport; acquiring tokens is not proving sub-ms execution. Keep those gates distinct.

On a defect: root cause → smallest correction → regression → rerun affected required gate. Keep architecture stable unless evidence requires a reviewed change. Do not perform unrelated historical harness migration, mass deletion or speculative optimization.

## 16. Reproducible build and target-machine qualification

Commit Cargo.lock and actually use the pinned toolchain. Use exact dependencies/lock revisions, explicit edition/features/MSRV, documented target/release/panic policy and verified source/license/commit/file references for reused code. Compare LTO/codegen/CPU settings on the actual workload; no unmeasured tuning claim. Dependency update is a reviewed change with applicable reruns, not a silent `cargo update`.

For a reviewed unit workspace:

```bash
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo test --workspace --release --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
```

Offline mode requires available pinned dependencies. A cache miss is not a source-code failure; provision dependencies explicitly and record that step. Do not regenerate a release lock implicitly during tests. No targets, secrets or machine-local generated artifacts in source commits.

Known separate issue: the committed historical Gate-0 Cargo.lock lacks manifest-required reth/tokio packages; a pre-existing local lock differs. Do not replace or stage it as part of ZIP import. If historic harness execution is required later, verify and repair its lock in a separate scoped change.

Final qualification uses dedicated Linux hardware. Record CPU/NUMA/NIC, OS/kernel, geth/Rust commits, compiler/target flags, affinity/SMT policy, governor, queue/capture sizes, clock source and network/provider profile. Evaluate hot core isolation, recorder separation, IRQ/RSS placement, swap, prefault/mlock and locality against measured tails. Mac/shared-VPS evidence remains diagnostic. Do not apply CPU flags or kernel tweaks as unmeasured proof of improvement.

## 17. Phase-end report and documentation discipline

At each delivery provide:

1. Phase/unit, supported scope, code changes and unchanged boundaries.
2. Exact commands, actual exit codes/test counts, toolchain/build/environment.
3. Evidence IDs → source commit/hash → implementation/test mapping.
4. New measurements with boundary IDs, clocks, samples/raw hashes, all failures and included/excluded work.
5. What is proven, what remains UNKNOWN, and exact gate impact.
6. Any contradiction and smallest correction; remaining tasks with owner/phase.
7. Import/commit/push status stated accurately. A local review artifact is not “already in main.”

Update the existing Gate-0 master ledger for new Gate-0 measurements and this production master/matrix for production adoption. Raw evidence remains raw. Do not proliferate near-identical human status/summary files.

Never expose private keys or credential-bearing endpoints in command output, reports, raw evidence or commits. Resolve secrets once through authorized local references; redact provider transport metadata without destroying request/result provenance.

## 18. Immediate execution order for Fable

1. Read this plan, the Revision-2 review and cited scope-specific Git evidence. Use the **already executed** Unit-1 compiler results, normalized formatting and actual lock; do not claim they remain uncompiled.
2. Correct inventory and Event/Clock/Capture proposal to the concrete clock, hash/actionable, producer, adapter boundary, capture/lifetime and fan-out contracts above. Resolve ownership against §4/§7/§12.
3. Return the corrected review package. Import only after acceptance, without targets, historical migration or unrelated Git changes.
4. Implement Unit 2 then Unit 3 with their exact golden and negative cases; fill their matrix rows with real test results. Do not wait for original-producer access to build them.
5. Complete bounded intent/submission/config/minimal journal units, then C1/C2/C3 and the scoped D–I extensions. Every downstream change requalifies affected semantics and integrated latency.
6. Pursue the real inclusion/producer measurement access in parallel with construction. If access is missing, give the exact instrumentation/access requirement; do not pretend more archive data can substitute.
   Deliver the §8 capability records and §13/J campaign manifest before acquisition qualification. Obtain review of concrete sample/workload/availability values and identify any unenforced future-bound claim. Implement the verdict evaluator before accepting campaign results. Do not postpone these acceptance decisions until after a successful BUY.
7. STOP at a phase delivery for the defined review boundary; do not silently enable production LIVE or launch an unapproved funded test.

The fastest correct construction combines early verified information, resident coherent state, safe preparation before T0, warm measured transports and verified ordering at the producer. **A 45-µs local reaction followed by next-block placement must remain visible and drive inclusion work, not a false product PASS.**

## 19. Provenance and source index

This consolidated handoff was written from the following frozen review inputs; no new network or acquisition measurement was made for this document.

The strict acceptance amendment adds mandatory initial birth-entry placement, after-handoff validity/late-execution handling and frozen campaign/release verdicts in response to the user's request for a complete implementation contract. It changes requirements, not historical results. The 22 source-file hashes in the adjacent provenance manifest were rechecked against repository HEAD `eaba1afe5ad5834a92ea0a9fb915792e2473cb15` with zero mismatches before this amendment. No new compiler run, provider qualification, code import or funded test is claimed by this edit.

```text
Git reference HEAD: 24e50ca9685bec27885dee7c551569882d33178a
HEAD/origin-main recheck: eaba1afe5ad5834a92ea0a9fb915792e2473cb15
Latest cited BSC planning commit: 0ab132202e0c71fc69d0adef7acafee34f2dc515
Supplied master SHA256:
cccdfdd573c37e9a58ee80b9c61101bca2576cadb1a93afff83b8947b2fea15b
Original Revision-2 ZIP SHA256:
bb16216b79cd37f21002565ea3c540dd79547e79adea89755b83871ec1e8bcd1
```

The later HEAD contains concurrent Project-0 work. `git diff 24e50ca..eaba1af -- bsc solana/docs` is empty; the cited BSC and Solana documentation inputs remain unchanged. Existing unrelated local modifications were not staged or included.

Review inputs:

- `/Users/sawyer_ma/Downloads/BSC_PRODUCTION_BUILD_MASTER_PLAN.md`
- `/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/c0-review-fable-rev2-2026-10-03/FABLE_REVIEW_REV2.md`
- Adjacent `git_alignment_provenance.json` (22 inspected Git source/evidence files), compiler logs and `command_exit_codes.json`.

Repository root for the paths below: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`.

Primary contracts:

```text
bsc/docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md
bsc/docs/GATE0_TESTPLAN_SHORT.md
bsc/docs/GATE0_FABLE_EXECUTION_CHECKLIST.md
bsc/docs/BSC_BIRTH_ENTRY_ARCHITECTURE.md
bsc/docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md
bsc/docs/BSC_DUAL_VENUE_ROADMAP.md
bsc/docs/BSC_IMPLEMENTATION_HANDOFF_CLAUDE.md
bsc/docs/BSC_PHASE_C_PYTHON_RETIREMENT.md
bsc/docs/BSC_PHASE_D_FLAP_READ_ONLY_CONTRACT.md
bsc/docs/BSC_PHASE_F_DUAL_CAPTURE_CONTRACT.md
solana/docs/RESEARCH_STATUS_2026-09-23.md
solana/docs/STRATEGY_C_SOURCE_OF_TRUTH.md
```

Selected raw/code witnesses:

```text
bsc/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-archive-witness-quicknode-153754/hotpath_dependency_matrix.json
bsc/gates/gate0/src/intent_lane.rs
bsc/gates/gate0/src/restart_fence.rs
bsc/gates/gate0/src/acceptance_guard.rs
bsc/gates/gate0/src/bin/g06a_self_launch_bundle.rs
bsc/gates/gate0/evidence/g03_result.json
bsc/gates/gate0/evidence/g06a_economics_evm_crosscheck_2026-10-02.json
bsc/gates/gate0/evidence/g06a_builder_floor_addendum_2026-10-03.json
bsc/gates/gate0/evidence/g06_single_sender_restart/result.json
bsc/gates/gate0/evidence/g07_hash_trigger_testnet_20261003T002258Z/result.json
```

Stale `docs/bsc/`, `gates/bsc/` or former `gate0/` references in old prose must resolve to the actual source tree and original content hash. Do not move historical sources to make old prose appear current.

**Final invariant:** actual acquisition remains the product objective; local engineering, placement, economic correctness, raw coverage and original producer timing remain separately testable gates. No relevant evidence is lost, no unsupported scope is enabled and no missing measurement becomes an invented latency.


## 20. Binding code, test and measurement adoption map

This section is the concrete implementation index for Fable. GitHub links are immutable links to reviewed commit `eaba1afe5ad5834a92ea0a9fb915792e2473cb15`, not moving `main`. Each row binds source snippets, regression evidence, production owner and required adaptation. The original experiment revision still governs a measurement; current source links do not claim that a newer binary produced an older result.

**Reuse contract:** preserve the demonstrated algorithm/invariant and its exact golden/negative cases; change historical harness layout only as required by the bounded production contracts above. Do not invent a replacement formula, discard a failing case, copy a diagnostic shortcut or restart completed feasibility research. Every delivered unit must identify the SN row, source hash, production symbol, ported test name and actual test result. Required new behavior without an existing source stays explicitly new and unproven until tested.

The complementary [source inventory, metric records and test-function index](</Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/c0-review-fable-rev2-2026-10-03/bsc_source_audit.json>) covers all **1,314 Git-tracked BSC files**, **1,118 JSON artifacts**, **24 Rust source files**, **32 historical Rust test functions** and **52 selected aggregate-result records**. Hashing, JSON parsing and symbol indexing provide mechanical inventory coverage; they do not claim every raw trace was manually interpreted or all experiments rerun. The semantic review is represented by the linked adoption rows below. Reviewed Unit-1 source is separately linked from a byte-identical isolated extraction of the [normalized review ZIP](</Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/c0-review-fable-rev2-2026-10-03/darkbot-c0-einheit1-rev2-reviewed.zip>); these source references are not imported into the production repository. Its 27 compiler-checked tests are a separate snapshot, not 27 additional independent experiments.

Four historical files named `*_ipfs_metadata.json`/`*_sourcify_metadata.json` contain gateway/HTML responses, not valid JSON metadata. Retain them as failed retrieval evidence, never as ABI authority. Two pre-existing working-tree differences (Gate-0 lock and preflight JSON) are recorded separately; committed evidence is not silently replaced by those local files.

### SN01 — Checked RLP / legacy TX / signer

**Matrix:** W1, W2, N2. **Code:** [Signer::sign_legacy](</Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/c0-review-fable-rev2-2026-10-03/unit1-reference/bsc/native/crates/darkbot-core/src/signer/mod.rs:150>); [expected-chain recovery](</Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/c0-review-fable-rev2-2026-10-03/unit1-reference/bsc/native/crates/darkbot-core/src/signer/mod.rs:189>); [legacy encoder/decoder](</Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/c0-review-fable-rev2-2026-10-03/unit1-reference/bsc/native/crates/darkbot-core/src/tx/legacy.rs:201>); [RLP writers and strict decoder](</Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/c0-review-fable-rev2-2026-10-03/unit1-reference/bsc/native/crates/darkbot-wire/src/rlp.rs:103>).

**Tests / evidence:** [27-test compiler log](</Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/c0-review-fable-rev2-2026-10-03/debug_test.log>); [actual alloy-rlp differential tests](</Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/c0-review-fable-rev2-2026-10-03/unit1-reference/bsc/native/crates/darkbot-wire/tests/rlp_differential.rs:93>); [historical envelope reconstruction](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/main.rs#L67); [1,024-envelope regression](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06_concurrency.rs#L444).

**Required adoption:** Unit 1: reuse the reviewed normalized implementation and actual lock after the existing package acceptance. Preserve byte-exact vectors, short r/s, low-S, wrong-chain/sender checks and empty output on error. Do not clone old per-binary RLP helpers or treat buffer-capacity tests as proof of zero allocations in every dependency. The local reference copy is byte-identical to the reviewed ZIP, not a production import.

### SN02 — V6 ABI, CREATE2 and precise old defect

**Matrix:** P1, N2. **Code:** [26-field V6 ABI and roundtrip](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06_flap_pending_decode.rs#L6); [CREATE2 and BUY ABI](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06a_self_launch_bundle.rs#L313).

**Tests / evidence:** [seven EIP-1014 vectors and 97/85-byte regression](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/legacy_gate0/gate0-evidence/verify_flap_g02.py#L24); [frozen corrected result](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/legacy_gate0/gate0-evidence/2026-09-27-g01/flap_g02_verified_correction.json).

**Required adoption:** Unit 2: exact 20-byte deployer, 32-byte salt, witnessed EIP-1167 init code/implementation; source profile must bind all inputs. Port wrong 32-byte deployer padding, wrong implementation, malformed/oversize ABI and unknown-mode rejection. Fixed historical addresses are fixture/profile facts, never a substitute for current deployment qualification.

### SN03 — Independent historical and immediate BUY execution

**Matrix:** P2, Q2. **Code:** [strict EVM replay](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/legacy_gate0/gate0-evidence/replay_flap_evm.cjs#L1); [full semantic verifier](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/legacy_gate0/gate0-evidence/verify_flap_semantic_replay.cjs#L1); [sequential BUY verifier](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/tools/verify_g03.cjs#L1).

**Tests / evidence:** [G0.2 semantic proof](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-archive-witness-quicknode-153754/semantic_replay_proof.json); [G0.3 golden and negative cases](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g03_result.json); [parent code closure](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g03_parent_closure.json).

**Required adoption:** Units 2/3: preserve Router→Portal historical first BUY separately from later direct BUY and immediate post-launch direct BUY. Missing executable code is UNWITNESSED, not zero code or a trading-disabled verdict. Port exact minOutput success/+1 revert and pre-launch revert. These offline scripts are oracle fixtures, not runtime dependencies; old relative paths require explicit relocation if rerun.

### SN04 — State dependencies, invalidators and oracle admission

**Matrix:** Q1, P2. **Code:** [dependency classifier](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/legacy_gate0/gate0-evidence/classify_flap_hotpath.cjs#L1); [ordered intervening-state verifier](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/legacy_gate0/gate0-evidence/verify_flap_intervening.py#L1).

**Tests / evidence:** [17 groups / 277 storage dependencies / 497 rows](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-archive-witness-quicknode-153754/hotpath_dependency_matrix.json); [oracle digest/role/deadline controls](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-archive-witness-quicknode-153754/pending_initial_state_evidence.json).

**Required adoption:** Units 2/3 + C2: convert every relevant row into a named resident/derived input, invalidator, refresh owner and admission guard. Port deadline equality success, deadline+1 rejection, revoked signer role and missing role storage. Do not scan all rows per signal, substitute observed outputs, or fabricate future time/private state.

### SN05 — Exact DOGE and native-BNB arithmetic

**Matrix:** Q2, Q3. **Code:** [DOGE single-segment math](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g03.rs#L19); [Curve-20 quote](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/native_bnb_quote.rs#L70); [native quote CLI binding](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06a_native_bnb_quote.rs#L10); [independent EVM crosscheck](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/tools/g06a_economics_evm_crosscheck.cjs#L1).

**Tests / evidence:** [integer/minOutput tests](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g03.rs#L124); [native golden vectors](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/native_bnb_quote.rs#L153); [EVM crosscheck result](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g06a_economics_evm_crosscheck_2026-10-02.json).

**Required adoption:** Unit 3/G: preserve operation order, combined-fee rounding and exact outputs. Historical code uses assertions and unchecked U256 additions outside its fixture bounds: validate widths/ranges and use checked/widened operations with typed rejection for production inputs. Cover r+reserve, total+h, reserve+net, products, division and subtraction. No panic/wraparound from an admissible external input; no zero minimum. New amount/caller/tick/tax branches require their own proof.

### SN06 — Economic dedup, nonce order and final envelope binding

**Matrix:** N1, N2, N3. **Code:** [IntentLane transitions](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/intent_lane.rs#L59); [canonical short-scalar signing reference](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06_concurrency.rs#L115).

**Tests / evidence:** [lane regression suite](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/intent_lane.rs#L179); [256 final-envelope bindings and counterexamples](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06_concurrency.rs#L515); [8,000 signals / 1,000 intents result](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g06_intent_lane/result.json); [injected HoL evidence](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g06_submission_hol_recovery/result.json).

**Required adoption:** Unit 4: port transition invariants, whole-session dedup and cross-chain/sender rejection into bounded preallocated storage. Do not copy BTreeMap insertion, test Mutex/workers or eight-sender layout into the hotpath. Hash uniqueness is not economic dedup; atomic nonce assignment is not dispatch ordering. Waiting for unresolved head must remain visible in admission/latency/coverage results.

### SN07 — Full/hash ingress, coverage and first-actionable race

**Matrix:** R1, Q1. **Code:** [native-channel diagnostic sensor](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/p2p_ingress.rs#L325); [full pending field decoder](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06_flap_pending_decode.rs#L38); [coverage analyzer](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/tools/analyze_g04_coverage_v2.cjs#L1).

**Tests / evidence:** [bounded source probe](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g04_quicknode_probe/analysis.json); [4/4 V6, zero DOGE denominator](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g04_quicknode_coverage_v2/analysis.json); [exact-scope non-observation](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g06_flap_exact_scope_pending/summary_terminal_capture.json).

**Required adoption:** C1/F: retain hash/full/actionable separation, chain/readiness/peer evidence and first verified actionable wins across enabled sources. The Reth sensor filters a known self-launch hash; it is not generic foreign-launch qualification. Its ignored try_send failures and dropped peer metadata are diagnostic limitations; production must record overflow and provenance. Do not use its polling intervals/channel topology as the production Fast Lane.

### SN08 — Permit-before-write and durable restart quarantine

**Matrix:** S1, N3. **Code:** [guarded_transport_write](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g05_remote_e2e.rs#L357); [FencedLane::arm](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/restart_fence.rs#L30).

**Tests / evidence:** [builder ACK/partial-write/wrong-sender tests](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/builder_lifecycle_tests.rs#L107); [restart state/corrupt-fence tests](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/restart_fence.rs#L106); [26-test historical result](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g06_single_sender_restart/result.json).

**Required adoption:** Units 4/5 + C2: permit before first possible write; ACK never confirms; partial/flush error remains UNKNOWN/HALT. Stable durable fence before admission, no filesystem on BUY path. Adapt single-sink begin_send into one economic admission plus once-only per-adapter permits; do not call begin_send twice or mint a new nonce for builder B. Canonical resume is new C2 work.

### SN09 — Local component and integrated performance evidence

**Matrix:** T2. **Code:** [full datagram benchmark boundary](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g05_local.rs#L193); [signed-TX IPC-only boundary](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g05_ipc.rs#L98); [integrated build/sign/IPC boundary](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g05_e2e_local.rs#L196).

**Tests / evidence:** [50,000 local samples](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g05_local_prequalification/result.json); [500,000 stability samples](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g05_local_stability/summary.json); [100,000 IPC-only samples](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g05_ipc_prequalification/result.json); [50,000 integrated IPC samples](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g05_local_e2e/result.json).

**Required adoption:** C3: retain these as demonstrated local feasibility and component baselines. Do not restart architecture selection or rerun old probes without a concrete need. Benchmark the actual new integrated runtime because these local sink returns are neither geth propagation nor a real BUY. Do not add component percentiles to manufacture an integrated percentile.

### SN10 — Remote transport and all tail failures

**Matrix:** T2, S1. **Code:** [warm remote transport-only probe](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g05_remote_transport.rs#L111); [full remote fixture path](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g05_remote_e2e.rs#L383); [stability runner](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g05_remote_e2e_stability.rs#L269).

**Tests / evidence:** [public RPC stability: 7/1,000 >1 ms](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g05_remote_e2e_stability/summary.json); [48Club: 22/1,000](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g05_48club_builder_stability/summary_terminal_capture.json); [BlockRazor: 19/1,000](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g05_blockrazor_rpc_bundle_stability/summary_terminal_capture.json).

**Required adoption:** Unit 5/C3: retain payload/framing/write/ACK boundaries and all errors. These probes deliberately corrupt transaction bytes and some summaries are user-supplied terminal records. Do not treat method recognition or invalid-transaction rejection as valid bundle acceptance; do not copy per-sample fresh lanes, sleeps, response waits or wire corruption into production.

### SN11 — Prepared transport versus on-signal target diagnostic

**Matrix:** T2, T4. **Code:** [prepared select/write boundary](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06a_presigned_handoff_transport.rs#L135); [actual padded eth_chainId payload](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06a_presigned_handoff_transport.rs#L188); [current-target residual work](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06a_target_handoff.rs#L373).

**Tests / evidence:** [current-target ready/send metrics](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md#L2820); [prepared transport measurements](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md#L2851).

**Required adoption:** Unit 5/C3: preserve the measured preparation advantage and exact numbers. The prepared harness uses padded read-only eth_chainId, not a signed bundle. Its select-only t_ready becomes T_select in production; original T_ready is retained. Current-target 0.274125 ms is ready-p50. Strict misses count >=1,000,000 ns; old > counters are historical fields. Qualify actual production bytes/boundaries separately.

### SN12 — Ordered bundle bytes, separate wallets, budget and authorization

**Matrix:** S2. **Code:** [ordered identical TX projection](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06a_self_launch_bundle.rs#L492); [final signer recovery and DRY binding](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06a_self_launch_bundle.rs#L457); [historical authorization guard](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/acceptance_guard.rs#L100); [floor probe diagnostic](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g06a_48club_floor_probe.rs#L62).

**Tests / evidence:** [DRY-1..4 contract](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g01_mainnet_acceptance_proposal_v2.json); [gas/fee correction](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g06a_builder_floor_addendum_2026-10-03.json); [independent local simulation](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/tools/g06a_local_simulate.cjs#L1).

**Required adoption:** Units 5/6: preserve [original launch, exact BUY], same BUY bytes/nonce across builders and verified provider-specific field encodings. Separate launcher/buyer belongs to controlled self-launch qualification, not a production multi-buyer redesign. Same-wallet diagnostic cannot pass its two-wallet contract. Historical ceilings, 1-gwei policy and environment-fed expected output are not production defaults/oracles. No network capability or existing authorization follows from this DRY source.

### SN13 — Real acquisitions and prepared known-hash restriction

**Matrix:** T1, T3, T4. **Code:** [prepare/sign before launch](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g07_testnet_birth_buy.rs#L775); [known-hash trigger and measured send](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g07_testnet_birth_buy.rs#L875); [postsend evidence verification](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g07_testnet_birth_buy.rs#L896); [earlier direct testnet acquisition](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/src/bin/g07_testnet_buy.rs#L159).

**Tests / evidence:** [same-block historical BUYs](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md#L1991); [0.617667-ms following-block BUY](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md#L2226); [0.103417-ms full-trigger result](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g07_presigned_testnet_20261003T000326Z/result.json); [0.045125-ms hash-trigger result](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g07_hash_trigger_testnet_20261003T002258Z/result.json).

**Required adoption:** C3/J: preserve actual acquisition and placement independently of local timing. Pre-sign only an exactly known valid candidate; do not apply self-launch hash shortcuts to unknown launches. Production binding/admission precedes egress even though historical diagnostics verify additional facts afterwards. Fixed TESTNET_MIN_OUTPUT, pre-send RPC setup and top-level-only competitor scans are not production policies. No receipt-observation duration becomes T_buy.

### SN14 — Complete receipt/event attribution and raw 15-second capture

**Matrix:** C1, T4. **Code:** [complete receipt/event correction](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md#L2943); [15-second task amendment](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md#L2951).

**Tests / evidence:** [raw BUY receipt](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g07_hash_trigger_testnet_20261003T002258Z/buy_receipt.json); [all Portal BUY events in both blocks](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/gates/gate0/evidence/g07_hash_trigger_testnet_20261003T002258Z/portal_buy_logs_launch_through_buy.json).

**Required adoption:** C0 recorder/F/J: capture all receipt-internal events, receipt/block TX-set equality and creator-vs-external identity, plus original producer brackets. The two-block receipt proof is not an implemented 15-second recorder. Original producer timing was absent; subsequent storage or replay cannot retroactively create it. Implement §§9/11 without changing the strict <1-ms acceptance.

### SN15 — Pinned geth/SPSC and exact-core integration

**Matrix:** T4, C2. **Code:** [native ownership and mmap/SPSC contract](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md#L39); [build/sign and host contract](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md#L237).

**Tests / evidence:** [runtime final qualification contract](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md#L391).

**Required adoption:** C0/C1/C3/I: geth owns chain-client duties; one Rust core owns semantics. No existing Gate-0 snippet implements the production mmap/SPSC or original producer hooks. Those are explicit new work, with schema/lifetime/clock/overflow tests before consumers freeze; never mark them inherited PASS. No extra task/serialization hop per module.

### SN16 — Retired Python counterexamples and single C2 owner

**Matrix:** R2, C2. **Code:** [retained Phase-C invariants](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_PHASE_C_PYTHON_RETIREMENT.md#L100).

**Tests / evidence:** [retirement acceptance and forbidden fallbacks](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_PHASE_C_PYTHON_RETIREMENT.md#L143).

**Required adoption:** C2: port first-seen, sender/nonce candidates, late-pending, receipt identity, reorg/backfill/high-water and restart counterexamples as native tests. Do not import the old Python runtime/migrations, add a second state owner in the nonce module or let an old scheduling clause override §1.

### SN17 — 30-minute native capture plus per-attempt windows

**Matrix:** C1, C2, R1. **Code:** [Phase-F duration/artifacts/manifest](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_PHASE_F_DUAL_CAPTURE_CONTRACT.md#L43).

**Tests / evidence:** [capture replay and quality gate](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_PHASE_F_DUAL_CAPTURE_CONTRACT.md#L133).

**Required adoption:** F: preserve clean continuous 30-minute binary capture, canonical denominator and exact-core replay in addition to each 15-second launch window. No Python normalization before primary capture, no first-seen overwrite, no silent overflow. Observer lead is diagnostic and cannot be called original T0.

### SN18 — Full economics and exit-cost acceptance

**Matrix:** Q3, X1. **Code:** [economic ownership and exact arithmetic](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_ECONOMICS_GATE_CONTRACT.md#L68).

**Tests / evidence:** [economic test matrix and snapshot](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_ECONOMICS_GATE_CONTRACT.md#L303).

**Required adoption:** G/H: retain buy/sell/approval/permit/revert/replacement/migration scope and receipt-based realized costs. Bind replay to exact economic snapshot, not a guessed percentage or zero cost. No strategy tuning/EV claim until economics passes. Earlier loose >3-ms prose does not override this master's strict deadline.

### SN19 — Flap full decoder and distinct Four families

**Matrix:** P1, P2, R2. **Code:** [Flap decoder contract](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_PHASE_D_FLAP_READ_ONLY_CONTRACT.md#L33); [Four decoder contract](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_PHASE_E_FOURMEME_READ_ONLY_CONTRACT.md#L35).

**Tests / evidence:** [Flap required tests](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_PHASE_D_FLAP_READ_ONLY_CONTRACT.md#L142); [Four pending-address assumption tests](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/BSC_PHASE_E_FOURMEME_READ_ONLY_CONTRACT.md#L120); [G0.1 frozen deployment evidence](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/GATE0_G01_EVIDENCE_FREEZE_2026-09-27.md#L1).

**Required adoption:** D/E: support only witnessed selectors/code/modes and test duplicate/revert/orphan/malformed/oversize/binary-roundtrip cases. Four Classic and OpenFour stay separate; post-execution TokenCreate never proves a pre-execution address. No hypothetical new protocol or automatic inheritance of Flap proof.

### SN20 — Phase-A isolation and preserved Solana baseline

**Matrix:** X1. **Code:** [Phase-A parity report](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/phase_a/PHASE_A_REPORT.md#L1); [BSC profile fail-fast regression](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/tests/test_bsc_profile_fail_fast.py#L1).

**Tests / evidence:** [post-fix parity evidence](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/phase_a/parity_pre_vs_post_fix.json); [strict replay validation tool](https://github.com/sawyerma/darkbot/blob/eaba1afe5ad5834a92ea0a9fb915792e2473cb15/bsc/docs/phase_a/tools/strict_replay.py#L1).

**Required adoption:** C0/H/I: preserve existing Solana behavior and the non-runnable Python BSC profile boundary. Phase-A synthetic/Solana measurements are not native BSC acquisition evidence. Use relevant parity regressions when touching shared entry points; no wholesale historical runtime migration or generic multichain rewrite.

### Source-level corrections and handoff acceptance

The source audit corrects two descriptions in the earlier consolidated prose: the CREATE2 mismatch was specifically 20→32-byte deployer padding, and the prepared transport harness sends padded read-only `eth_chainId`, whereas the current-target/earlier remote diagnostics use intentionally invalid transaction bytes. It also restores the local 500,000-sample stability series, separate IPC measurements, synthetic build tails, the 0.617667-ms real following-block BUY, the bounded 4/4 V6 coverage result and the 30-minute Phase-F capture requirement. These corrections preserve the prior investigation; they do not change historical raw files or manufacture a new PASS.

At each unit delivery, use §14 as the status source and attach its concrete SN→production-symbol→test→result mapping. Unmapped mandatory source invariants block that unit; later-phase original producer/capture/access work remains with its named owner. Do not demand reruns of unchanged historic network experiments to start a library unit. Re-run applicable production regressions and integrated measurements when code, state scope or measurement boundaries actually change.

The fixed product acceptance remains **actual successful acquisition with conservatively bounded `T_buy - T0 < 1,000,000 ns`**. Preparation, fast local code, ordered submission, raw retention and later calculation must serve that requirement. No linked snippet or historical test relaxes it.