# Project 0 — Audit Handoff / Current State — 2026-10-05

## Purpose

This document is the authoritative human-readable handoff after the 2026-10-05 Project 0 production/economic audit.

It exists so that the audit state, evidence, unresolved correctness risks, and exact next work cannot be lost or accidentally replaced by another broad architecture iteration.

The primary product objective remains:

> **turn real Marginfi / Project 0 liquidation opportunities into positive realized net P&L.**

The next work is therefore evidence-driven and deliberately narrow.

---

## 1. Frozen provenance

### Audited runtime source

The Astra audit reviewed the exact DarkBot source snapshot:

```text
799048f8fa95df4c29f2aa6b35bcc3615405fe19
ledger: qualify R127d scheduler admission PASS
```

### Evidence commits added after the audited runtime

The audit evidence and preservation commits are additive; they do not change the audited runtime behavior:

```text
d7dfa01fce911e460354f0b4c312be37f5895025
evidence: freeze Astra Project 0 economic audit corpus

3ab9b9ce07072f8c9788901914b844055cd990de
[Project 0] add audit evidence bundle

c9e112ba8e9f7ce9e6e0b9e0fa60753a97520040
[Project 0] add Cargo.lock
```

The pre-fix audit baseline is also frozen as:

```text
archive/project0-audit-baseline-20261005
```

### Canonical audit evidence root

```text
project_0/docs/evidence/AUDIT_20261005/
```

Human-readable audit records:

```text
AUDIT_FINDINGS.md
BUNDLE_README.md
COMMANDS_AND_RESULTS.md
manifest.json
history_summary.json
history_analysis.json
cache_counterexamples.txt
drift_dispatch.json
```

Full forensic bundle:

```text
project_0/docs/evidence/AUDIT_20261005/darkbot_audit_bundle_20261005.zip
```

The ZIP is the complete preserved audit package. It contains the clean audited DarkBot source archive, pinned public comparison-source snapshots, evidence scripts, raw RPC transaction data, test outputs and checksums. Secrets and local .env files were intentionally excluded.

**Rule:** do not rewrite or "clean up" the Astra evidence files. New conclusions belong in normal Project 0 documentation and new evidence must be additive.

---

## 2. Executive economic verdict

### Current verdict: B

> **The economic mechanism is real, but specific implementation/evidence blockers currently prevent reliable proof of profitable capture.**

What is established:

- real Marginfi receivership liquidations occur;
- DarkBot has a functioning Mainnet Shadow pipeline;
- R127d materially fixed queue/fanout pressure;
- the current workspace test suites are green;
- real recent liquidation transactions are now preserved;
- specific candidate-admission correctness counterexamples are reproduced.

What is **not** established:

- that the current DarkBot would have captured the six real liquidation events;
- that its candidate cache cannot hide economically relevant updates;
- that its route economics are conservative against executable output floors;
- that Drift refresh is currently usable on the deployed Mainnet program;
- complete realized USD P&L for the historical winners;
- positive realized DarkBot P&L;
- competitive Race performance;
- controlled LIVE readiness.

Therefore:

```text
FIRST REALIZED NET PROFIT = NOT PROVEN
CONTROLLED LIVE          = BLOCKED
PRODUCTION LIVE          = BLOCKED
```

---

## 3. What is proven by the audit

### 3.1 Workspace tests

At audited source `799048f...`:

```text
cargo test --workspace --features crate_health,txv1
136 passed / 0 failed

cargo test --workspace --features crate_health
134 passed / 0 failed
```

Evidence:

```text
AUDIT_20261005/tests_txv1.txt
AUDIT_20261005/tests_health.txt
```

Green unit/workspace tests do **not** override the adversarial counterexamples below.

### 3.2 R127d scheduler/fanout result remains measured

R127d Mainnet Shadow run:

```text
run                  r127_20261005T171043Z
dependency_updates   96,816
sweep_checks         12,430,220
healthy              11,316,714
forwarded            151,045
cache_suppressed     962,461
pending_final        835
max_pending          6,285
ingress_backlog_final 0
ingress_overflow     0
sink_overflow        0
```

Compared with R127c:

```text
forwarded       1,113,854 -> 151,045
pending_final       6,154 -> 835
coalesced         986,849 -> 32,342
```

This remains a valid **scheduler/queue/fanout performance PASS** for the measured workload.

It is **not** a complete Candidate Cache correctness PASS after the adversarial audit.

### 3.3 Candidate-cache counterexamples are reproduced

The audit compiled the checked-in admission predicate from `darkbot-runner/src/main.rs` and exercised it adversarially.

Evidence:

```text
project_0/docs/evidence/AUDIT_20261005/cache_counterexamples.py
project_0/docs/evidence/AUDIT_20261005/cache_counterexamples.txt
```

Reproduced:

1. normal observed class transitions and forced Bank re-arm work;
2. a second same-slot Liquidatable evaluation can be suppressed even when capacity exists;
3. the same Error class can be suppressed after changed integration input;
4. with 1,025 persistent candidates, the fixed ordering + 1,024 repeat soft cap can starve the lexicographically last candidate for at least 100 simulated slots while the first 1,024 are re-armed every slot.

These are correctness/economic-opportunity risks, not cosmetic performance issues.

### 3.4 Current mechanism causing the cache risk

At the audited runtime, the cache state is effectively:

```rust
CandidateCacheEntry {
    state,
    last_forwarded_slot,
}
```

The admission decision is based on:

```text
forced
OR health-class transition
OR (new-slot periodic due AND repeat capacity)
```

It does not encode a causal revision/generation for same-slot Bank/Oracle/integration changes.

The account enqueue path also sorts candidate pubkeys deterministically. Under a persistent soft-cap this can create repeat-selection bias.

### 3.5 Drift failure is reproduced

The audit compared the intended Drift refresh discriminator/account ordering against pinned reference source and then performed an unsigned read-only Mainnet simulation.

Observed:

```text
Program: dRiftyHA39MWEi3m9aunc5MzRF1JYuBsbn6VPcn33UH
InstructionFallbackNotFound
Custom(101)
units consumed: 1345
```

Evidence:

```text
AUDIT_20261005/drift_dispatch.json
AUDIT_20261005/probe_drift_dispatch.py
```

Interpretation:

- this is **not** proof that DarkBot's Drift refresh works;
- the audited discriminator/order matching reference source is insufficient against the currently deployed program behavior;
- the exact deployed-version/IDL/instruction-support mismatch must be resolved or Drift-dependent candidates must remain explicitly unsupported/fail-closed.

### 3.6 Six real successful Marginfi liquidation transactions are preserved

The audit scanned 52 signatures, inspected 9 successful transactions and retained six recent successful receivership liquidations.

| Signature | Slot | Receiver | Seized equity USD | Repaid equity USD | Gross equity delta* | Tx fee lamports | Flat protocol fee lamports |
|---|---:|---|---:|---:|---:|---:|---:|
| `59Qt6h...dnwNa` | 453063240 | `qbvAG...mNCV` | 1.633225 | 1.573042 | 0.060183 | 5,000 | 1,000,000 |
| `38ZXvW...Bm7F` | 451704201 | `D9i4V...PqYn` | 0.997934 | 0.466165 | 0.531769 | 19,000 | 1,000,000 |
| `2cebUJ...HJEf` | 451703584 | `D9i4V...PqYn` | 0.350161 | 0.163340 | 0.186822 | 19,000 | 1,000,000 |
| `3ab7mn...ExRB` | 451657746 | `TraceR...sH8` | 2.102967 | 1.917017 | 0.185950 | 5,000 | 1,000,000 |
| `gLVXyf...vkUzB` | 451643526 | `D9i4V...PqYn` | 1.968229 | 1.794029 | 0.174200 | 19,000 | 1,000,000 |
| `C6Jnpz...WRwZ` | 451635082 | `TraceR...sH8` | 2.372690 | 2.162890 | 0.209800 | 5,000 | 1,000,000 |

`*` Gross equity delta is only `seized_equity_usd - repaid_equity_usd`. It is **not realized net P&L**.

The transaction corpus also preserves instruction decoding, payer/receiver lamport deltas, token deltas, Jupiter usage where present, CU information where present and the protocol flat-fee transfer.

### 3.7 Historical P&L and DarkBot replay remain UNKNOWN

The audit explicitly records:

```text
complete_realized_usd_pnl         = null
historical_darkbot_classification = UNKNOWN
historical_prestate_missing       = true
```

Standard RPC transaction history did not provide the complete pre-liquidation account state needed to execute the current DarkBot health/sizing/routing pipeline exactly as it would have existed before the winning transaction.

No historical USD profit is to be invented from gross equity deltas.

---

## 4. Findings that are important but NOT yet proven

This distinction is mandatory.

### 4.1 Jupiter quote vs executable floor — high-priority economics risk, not yet an executed audit PASS/FAIL

The audit helper `p0-audit-json.rs` contains an intended regression demonstrating a candidate that is positive at a quoted output but negative at an allowed adverse execution floor.

However, `COMMANDS_AND_RESULTS.md` records that this auxiliary executable failed during SDK JSON deserialization **before those assertions executed**.

Therefore the audit does **not** currently prove that exact numeric regression.

Independent code review nevertheless shows that the current Jupiter route object stores `amount_out` from the build response as the route output and the economics layer consumes a single `gross_seized_realizable_usd_micro` value. The next test must establish whether the value passed into economics is the guaranteed executable floor or merely the expected/quoted output.

Classification:

```text
LIKELY CORRECTNESS/ECONOMICS RISK — MUST PROVE OR DISPROVE
severity if confirmed: P1
```

### 4.2 Winner JSON parsing — high-priority evidence/reconciliation risk, not yet proven by the failed helper

The current `winner.rs` reads JSON pointers under:

```text
/transaction/transaction/message/accountKeys
/transaction/transaction/message/instructions
```

The audit helper intended to test the actual serialized transaction shape, but again failed before its assertions due to deserialization.

Therefore do not label the winner parser broken until a direct raw-JSON fixture test executes successfully.

Classification:

```text
UNPROVEN HIGH-PRIORITY REPLAY/RECONCILIATION RISK
```

---

## 5. Reference projects frozen by the audit

The full bundle preserves pinned source snapshots. The audit manifest records:

```text
eva01                 15625d5f57dea2ed0a47e857de814fdd68d2513d
mary                  c1ba4b0e82118e86def2cccb9fb4b32520e88b15
p0-ts-sdk             9860697041532f1a9b19e86a36f9810570cad326
keep-rs               99f94a149f006b2074b7ea7babadb34b07368ea6
solend-public         b3b46ffbc2f5162b6e5680dd3cad1042c2eb3f8e
drift-protocol        13e8e9b8d614f3b62e3a65a8c372c819e6529aeb
solend-liquidator     db9adbf571ba13414117741475331b1ccf17f1fd
keeper-bots-v2        0e4e93be737384f2b03c93c06ada83937c6c8e42
marginfi-v2 pinned    33c67987a661f59c6683a65485ebb4f0ef030b77
marginfi tag          mrgn-0.1.11-rc1
```

These snapshots exist to make future comparisons reproducible. Their existence is not by itself proof that any reference bot is profitable.

---

# 6. Blockers and priority

## P1-A — Candidate revision blindness inside one slot

### Current behavior

Two evaluations with the same health class and same Solana slot can collapse to one downstream admission even if a relevant canonical dependency changed between them.

### Economic consequence

A candidate may remain `Liquidatable` while:

- oracle price changes;
- Bank state changes;
- integration state changes;
- repay/seize sizing changes;
- route feasibility changes downstream.

If the second causal state is more profitable/actionable, suppressing it can miss an opportunity.

### Smallest correct fix

Do **not** remove R127d and do not return to per-update fanout.

Extend candidate admission with a causal revision/generation in addition to health class + slot.

A valid implementation should provide a monotonically changing candidate-input revision whenever a canonical input capable of changing candidate downstream behavior changes.

Minimum candidate-cache semantics:

```text
forward immediately when:
- state class changes; OR
- forced Bank rearm; OR
- relevant input revision changed since last forwarded revision; OR
- periodic recheck is due and fair repeat capacity is available.
```

The revision must distinguish multiple relevant writes inside the same Solana slot.

Do not use slot alone as the revision.

### Required tests

1. Healthy -> Liquidatable, same slot: forward.
2. Liquidatable -> Liquidatable after actual Oracle revision, same slot: forward.
3. Liquidatable duplicate with identical revision, same slot: suppress.
4. Error -> Error after changed integration revision, same slot: forward/retry.
5. Error duplicate with identical revision: suppress.
6. Bank forced rearm still forwards.
7. Recovery clears/rebuilds revision state correctly.
8. Direct MarginfiAccount writes remain actionable.
9. Multiple same-slot writes retain the newest revision and never regress.

Gate:

```text
no reproduced same-slot false-negative
```

---

## P1-B — Soft-cap fairness / deterministic starvation

### Current behavior

The affected-account union is sorted by pubkey. Persistent candidates can therefore repeatedly reach the repeat capacity in the same deterministic order.

The audit reproduced:

```text
1025 persistent candidates
repeat soft cap = 1024
100 simulated slots
last candidate forwarded = 0 times
first 1024 forwarded = every slot
```

### Economic consequence

A valid profitable candidate can be systematically excluded without any queue overflow.

### Smallest correct fix

Keep bounded repeat work, but replace static priority with a fair policy.

Acceptable minimal patterns include:

- rotating cursor over the persistent-candidate set;
- round-robin queue;
- ordering by oldest `last_forwarded_slot/revision` with deterministic tie-break;
- another policy with an explicit no-starvation property.

Transitions and forced re-arms must still bypass ordinary repeat throttling.

### Required invariant

Under continuous eligibility and no infinite forced-work flood:

```text
every persistent candidate receives repeat admission within a bounded number of scheduling rounds.
```

For `N` persistent candidates and repeat capacity `C`, the regression test must show that all candidates are serviced within the expected bounded rotation, rather than permanently favoring low pubkeys.

### Required telemetry

Add at least:

```text
candidate_repeat_wait_slots_max
candidate_repeat_wait_slots_p95 (or bucketed equivalent)
candidate_fairness_rotations
candidate_periodic_due_but_deferred
```

Gate:

```text
no-starvation property PASS
queue remains bounded/draining
R127d suppression benefit materially retained
```

---

## P1-C — Economics must use executable worst-case value

### Current uncertainty

The route builder currently keeps an `amount_out` value. It must be proven that economic admission is based on an executable guaranteed/minimum output, not merely an optimistic quote.

### Required fix if the risk is confirmed

Carry separate route fields all the way to economics:

```text
quoted_out
minimum_executable_out / threshold
route fees
price impact / explicit conservative cost
```

For admission:

```text
gross_seized_realizable_value =
value of minimum executable output,
not value of expected quote
```

Unknown threshold/floor => `Evaluation::Unknown`, not zero and not quoted output.

### Required direct regression test

Build the economics regression without historical-JSON deserialization.

The test must demonstrate:

```text
quote-net > min_profit
but
floor-net <= min_profit
=> candidate rejected
```

Also test:

- quote == floor;
- positive floor;
- negative floor;
- missing floor => Unknown;
- no-swap route where no swap floor is needed;
- Token-2022/transfer-fee effects if applicable to the selected route.

Gate:

```text
all admitted candidates have positive deterministic net at the executable floor
```

---

## P1-D — Historical winner/reconciliation parser must be fixture-proven

### Required work

Do not route historical raw RPC JSON through a mismatching SDK deserializer merely to test the JSON pointers.

Create raw JSON fixtures from the six preserved transactions and run the production winner parser or a shared production parsing helper directly.

The parser must handle:

- transaction message shape actually returned/serialized by the selected RPC client;
- versioned transactions;
- loaded addresses where applicable;
- programIdIndex/account index resolution;
- `end_liquidation` discriminator;
- receiver extraction;
- failed transactions ignored;
- same-slot ordering remains UNKNOWN unless transaction index is explicitly available and trusted.

Gate:

```text
all six known successful liquidation fixtures are detected with correct
signature, slot, target and receiver.
```

If the current JSON pointers are wrong, fix the production parser and add the six fixtures as permanent regression tests.

---

## P1-E — Drift source/deployed-program compatibility

### Required work

Determine why the deployed program rejects the reference refresh discriminator with `Custom(101)`.

Required sequence:

1. identify exact deployed Drift program version/build/IDL relevant to current Mainnet;
2. verify whether the desired refresh operation exists on that deployed version;
3. inspect official current SDK call path for the exact venue/account type being refreshed;
4. construct an unsigned valid-account simulation, not a deliberately account-incomplete generic probe;
5. prove the exact instruction + account list succeeds.

If that cannot be proven quickly:

```text
Drift-dependent candidates = explicit unsupported/fail-closed class
non-Drift candidates       = continue through first-profit path
coverage loss              = measured and reported
```

Do not let an unresolved optional integration block unrelated profitable candidates, but do not silently treat Drift state as refreshed.

Gate:

```text
valid read-only refresh simulation PASS
OR
explicit unsupported classification with zero false claim of coverage
```

---

## P1-F — Historical prestate and exact replay evidence missing

The six winning transactions prove the economic mechanism and actual competitor execution, but not what DarkBot would have done immediately before each event.

### Required prestate per event

At minimum reconstruct:

- target MarginfiAccount immediately before liquidation;
- all referenced Marginfi Banks;
- relevant oracle accounts and prices;
- Marginfi group/config;
- relevant integration accounts;
- token mint/program metadata;
- slot/time context;
- route-relevant asset/debt pair;
- fee state.

Standard transaction RPC alone may be insufficient for arbitrary historical account state. Use an archival source or deterministic reconstruction from preserved state/history. Record the source and limitations.

### Replay path

Each case must be run through the actual production logic:

```text
historical canonical state
-> dependency trigger
-> batch/dedupe
-> corrected Candidate Cache
-> exact Health
-> integration refresh
-> receivership eligibility
-> sizing
-> funding classification
-> route construction
-> executable-floor economics
-> transaction build
-> signing readiness
-> submission readiness
```

Required classification for every event:

```text
WOULD DETECT AND BE READY
WOULD DETECT BUT TOO LATE
WOULD SKIP AS UNECONOMIC
WOULD FAIL HEALTH
WOULD FAIL REFRESH
WOULD FAIL SIZING
WOULD FAIL FUNDING
WOULD FAIL ROUTING
WOULD FAIL BUILD
WOULD MISS COMPLETELY
UNKNOWN
```

No `UNKNOWN` may be silently converted to PASS.

---

# 7. Exact ordered implementation plan

This ordering is mandatory unless new evidence proves a different blocker.

## Step 0 — Preserve baseline — DONE

- audit source frozen;
- raw evidence committed;
- complete audit ZIP committed;
- Cargo.lock committed;
- pre-fix audit archive branch created.

No further packaging work is needed before engineering resumes.

## Step 1 — FIX candidate revision blindness

Implement P1-A only.

Do not mix route/economics changes into the same commit.

Required outputs:

- code;
- adversarial unit tests;
- replay of `cache_counterexamples.py` or replacement production tests;
- short changelog with rollback commit.

STOP if same-slot changed revision can still be suppressed.

## Step 2 — FIX candidate fairness

Implement P1-B.

Required outputs:

- no-starvation unit/property test above 1,024 candidates;
- queue-bound regression;
- fairness telemetry.

STOP if the first candidates can permanently monopolize periodic repeat admission.

## Step 3 — Requalify R127 scheduler/admission correctness + performance

Run short Mainnet Shadow after Steps 1-2.

Compare against R127d baseline:

```text
ingress overflow
sink overflow
pending_final
max_pending
sweep_forwarded
cache suppression
transition forwards
forced forwards
periodic forwards
fairness wait
same-slot revision forwards
```

Acceptance:

- lossless ingress;
- queue bounded and draining;
- suppression remains materially effective;
- no candidate-starvation counterexample;
- same-slot changed revisions are observed/forwardable;
- no regression to R126/R127c fanout explosion.

Do **not** optimize the startup ~6k peak unless evidence shows it causes missed profitable opportunities.

## Step 4 — Prove/fix executable-floor economics

First write the independent direct regression test.

If the current path already uses a valid executable floor, prove it and close the finding.

If not, make the smallest route/economics change needed and rerun all economics tests.

Acceptance:

```text
positive quote + negative executable floor => REJECT
unknown executable floor                  => UNKNOWN/REJECT
positive executable floor above threshold => may qualify
```

## Step 5 — Prove/fix historical winner parser

Use the six preserved real transaction fixtures.

Acceptance:

```text
6/6 known winners parsed correctly
failed transactions ignored
v0/v1 shape handled
loaded addresses handled where required
```

This closes a measurement/reconciliation uncertainty before historical competitive claims are made.

## Step 6 — Resolve Drift scope

Attempt source-exact deployed-program qualification.

Outcome A:

```text
valid refresh simulation PASS
=> enable qualified Drift refresh path
```

Outcome B:

```text
cannot qualify current deployed instruction
=> classify Drift-dependent candidates unsupported/fail-closed
=> measure affected candidate/opportunity share
=> continue non-Drift First-Euro path
```

Do not fake fallback freshness.

## Step 7 — Reconstruct historical prestate

Prioritize the six frozen events.

Start with the events that have the simplest asset/debt route and complete observable winner deltas.

Preserve all acquired historical state under a new immutable evidence subdirectory with:

- provider/source;
- retrieval command;
- retrieval timestamp;
- slot;
- account pubkey;
- data hash;
- parser/source version.

## Step 8 — Run six-case production replay

After Steps 1-7, execute the production replay and fill the classification table.

For every event produce:

- trigger source;
- DarkBot Health;
- receivership eligibility;
- chosen seize/repay sizing;
- route;
- executable floor;
- protocol fee;
- tx fee;
- priority fee;
- tip;
- ancillary cost;
- expected deterministic net;
- readiness slot/time;
- winner slot/index;
- final classification;
- exact reason for any skip/failure.

A known profitable event that DarkBot rejects for the wrong reason is a P1 correctness bug and blocks Controlled LIVE.

## Step 9 — Complete winner realized-PnL attribution

For the six historical winner transactions, derive complete realized P&L only where attribution is defensible.

Required equation:

```text
realized collateral/output value
- debt/repayment value
- protocol flat/variable fees
- base fee
- priority fee
- Jito tip if observable/attributable
- swap/DEX fees
- slippage/price impact
- ATA/rent/token costs
- other attributable costs
= realized net P&L
```

If any material component remains unknown:

```text
complete_realized_usd_pnl = UNKNOWN
```

Do not infer profit only from protocol equity delta.

## Step 10 — Short competitive Mainnet Shadow

Only after correctness/economics replay is clean.

Use a short evidence-focused run first; do not jump to a 72 h run.

Measure:

- real actionable candidates;
- expected deterministic net at floor;
- time from `T_rx -> T_detect -> T_ready -> T_sign`;
- winner signature/slot;
- would-have-landed classification;
- loss reason;
- route failure rate;
- integration coverage;
- candidate-cache fairness.

Goal:

```text
find at least one objectively positive-net opportunity
for which DarkBot is technically ready in time.
```

## Step 11 — Controlled value-bearing execution

This step requires explicit operator approval and all safety gates.

Before enabling:

- exact wallet/capital limit;
- no unresolved P0 fund-loss issue;
- executable-floor economics PASS;
- candidate-cache correctness PASS;
- route/build/simulation PASS;
- supported integration only;
- reconciliation active;
- stop/halt path verified;
- minimum net-profit threshold explicit;
- LIVE remains disabled by default.

Start with the smallest safe exposure permitted by the protocol/opportunity; do not enlarge capital merely to force a test.

Success criterion:

```text
landed real liquidation
AND
fully reconciled
AND
realized net P&L > 0 after all attributable costs
```

This is the first actual First-Euro proof.

## Step 12 — Only then optimize Race/scale

After first positive realized net evidence:

- capture rate;
- Jito/priority/tip optimization;
- provider economics;
- Race path;
- Binary/Geyser improvements;
- prepared candidate/transaction work;
- Direct TPU/SWQoS only if measured;
- longer endurance qualification.

---

# 8. Engineering explicitly paused until first-profit evidence

Do not spend material engineering time on these unless a measured blocker requires them:

- new broad architecture layers;
- another scheduler rewrite unrelated to the reproduced counterexamples;
- ThresholdIndex revival for First-Euro;
- Race-only latency optimization;
- Direct TPU;
- raw shred/deshred work;
- own validator/node;
- kernel/NIC tuning;
- provider subscriptions without measured incremental value;
- startup peak optimization solely because the peak looks large;
- 72 h endurance before correctness/economics replay is complete.

The shortest path is:

```text
Candidate Cache correctness
-> fairness
-> scheduler requalification
-> executable-floor economics
-> winner parser
-> Drift qualification/scoping
-> historical prestate
-> six real-event replay
-> short competitive Shadow
-> controlled real liquidation
-> positive realized net P&L
```

---

# 9. Gate matrix

| Gate | Current state | Blocks first positive-net test? | Blocks Controlled LIVE? |
|---|---|---:|---:|
| R127 queue/fanout performance | PASS measured | No | No |
| Candidate same-slot revision safety | FAIL counterexample | Yes | Yes |
| Candidate repeat fairness | FAIL counterexample | Yes | Yes |
| Exact Health baseline | tests/Mainnet exercised | No known blocker | Must remain PASS |
| Executable-floor economics | UNPROVEN | Yes | Yes |
| Winner/reconciliation fixture parsing | UNPROVEN | Needed for proof | Yes for reliable evidence |
| Drift refresh | FAIL/OPEN | Only Drift-dependent path | Yes for claimed Drift coverage |
| Six historical winner corpus | PASS preserved | No | No |
| Historical prestate | MISSING | Yes for historical proof | Strong gate |
| Historical DarkBot replay | NOT RUN | Yes | Yes |
| Complete winner realized P&L | UNKNOWN | Yes for profit attribution | Yes for economic proof |
| Short competitive Shadow | PENDING | Yes | Yes |
| Controlled value-bearing LIVE | DISABLED | — | — |
| First realized DarkBot net profit | NOT PROVEN | target | target evidence |

---

# 10. Required finding discipline going forward

Every future finding must be one of:

```text
PROVEN CORRECT
LIKELY CORRECT BUT UNPROVEN
INCORRECT
DEAD / OBSOLETE
MISSING
BLOCKED BY EXTERNAL DEPENDENCY
```

Bug severity:

```text
P0 = can lose funds / corrupt state / submit invalidly
P1 = can miss profit / misprice economics / incorrectly block or admit opportunity
P2 = performance / robustness
P3 = cleanup / maintainability
```

Do not upgrade a hypothesis to "proven" because a helper source contains an assertion that never executed.

Do not downgrade an adversarial counterexample because broad workspace tests are green.

---

# 11. Restart instructions

When resuming Project 0 work, read in this order:

1. `project_0/docs/AUDIT_HANDOFF_2026-10-05.md`
2. `project_0/docs/evidence/AUDIT_20261005/AUDIT_FINDINGS.md`
3. `project_0/docs/evidence/AUDIT_20261005/COMMANDS_AND_RESULTS.md`
4. `project_0/docs/evidence/AUDIT_20261005/manifest.json`
5. `project_0/docs/evidence/AUDIT_20261005/cache_counterexamples.txt`
6. `project_0/docs/evidence/AUDIT_20261005/history_summary.json`
7. `project_0/docs/evidence/AUDIT_20261005/history_analysis.json`
8. `project_0/docs/R127D_CHANGELOG_ROLLBACK.md`
9. `project_0/docs/MEASUREMENT_LEDGER.md`

For deep forensic/reference work, use:

```text
project_0/docs/evidence/AUDIT_20261005/darkbot_audit_bundle_20261005.zip
```

Do not restart with another whole-repository redesign.

The immediate implementation ticket is:

```text
P1-A: fix same-slot candidate revision blindness
```

followed immediately by:

```text
P1-B: fix repeat fairness/starvation
```
