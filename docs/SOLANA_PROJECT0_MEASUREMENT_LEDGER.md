# Solana / Project 0 — Measurement & Evidence Ledger

Status: **NOT STARTED / EMPTY BASELINE**  
Updated: 2026-09-30

This is the single human-readable measurement ledger for the new Solana / Project 0 Liquidator.

Primary economic objective:

> **maximize sustainable realized net profit while avoiding unnecessary recurring portal/SaaS costs.**

No Project 0 latency, capture-rate, winner-position, provider-benefit or realized-PnL measurement is currently recorded here. Values remain **N/A** until produced by the actual implementation.

## 0. Rules

- Never write an unmeasured value as 0.
- Separate local processing latency from network/inclusion/landing evidence.
- Submission ACK or Jito bundle ID is not landing evidence.
- Use the same timing vocabulary across replay, shadow and LIVE.
- Every result records source commit, config/source manifest, sample count and raw-evidence path.
- Gate/benchmark code must exercise production components, not duplicate business logic.
- Paid provider cost must be measured alongside its incremental benefit.
- Do not invent p(win), p(land) or competitor-response probabilities.
- Failed-attempt cost is submission-path/outcome dependent.
- Provider convenience is not evidence of economic necessity.

## 1. Timing vocabulary

~~~text
T_rx               update received by Rust ingress
T_detect           candidate classified as liquidatable/actionable
T_ready            complete execution plan/transaction(s) ready
T_sign             signing complete
T_handoff          payload handed to selected submission adapter
T_observed_landed  landing observed/reconciled from chain evidence
~~~

Derived local intervals may include:

~~~text
T_rx → T_detect
T_detect → T_ready
T_ready → T_sign
T_sign → T_handoff
T_rx → T_handoff
~~~

Landing evidence additionally records:

~~~text
signature
landed slot
transaction index where derivable
confirmation status
submission/bundle id
transaction version
serialized bytes
submission path
single tx / bundle
actual fees
actual tips
observed receiver/token deltas
realized PnL
~~~

## 2. Phase evidence table

| Phase | Measurement | Status | Evidence |
|---|---|---|---|
| S0 | timing/telemetry + provider abstraction contract | N/A | not started |
| S1 | ingress throughput/decode/gaps/reconnect/v1 fidelity/provider cost | N/A | not started |
| S2 | state reconciliation/fork/gap recovery | N/A | not started |
| S3 | build/sign/serialize/v0-v1/Jito-RPC capability | N/A | not started |
| S4 | Python↔Rust Pump.fun parity + native baseline | N/A | not started |
| H0 | historical Project 0 liquidation/composition/competition | N/A | not started |
| P2 | affected accounts/update + timed-recheck load | N/A | not started |
| P3 | exact health/eligibility parity | N/A | not started |
| P4A | deterministic economics/ranking | N/A | not started |
| P5 | route/build/size/CU/bundle + path-dependent failure cost | N/A | not started |
| P6 | full hotpath + shadow race | N/A | not started |
| P7 | capture/economics/competition/provider-value evidence | N/A | not started |
| P4B | empirical p(win)/tip/stopping ranking | N/A | blocked on P7 |
| P8 | controlled LIVE reconciliation | N/A | not started |
| P9 | production profit/provider economics | N/A | not started |

## 3. Required record shape

Every measurement entry should state:

~~~text
date/time
phase
repo commit SHA
protocol/source manifest
runtime config
provider/region
provider monthly cost
provider usage cost
hardware/runtime environment
sample count
metric definitions
p50
p95
p99
max
raw evidence path
PASS / ADAPT / STOP
known limitations / non-claims
~~~

Where economic evidence applies also record:

~~~text
gross liquidation value
protocol/flat fees
swap/DEX fees
slippage
priority fees
Jito tips
failed-attempt cost
capital used
realized net PnL
provider cost
incremental opportunities attributable to provider/path
incremental realized/expected profit attributable to provider/path
~~~

## 4. Submission-path failure matrix

Every qualified submission path must eventually populate this matrix from real evidence:

| Path / outcome | Base fee | Priority fee | Tip | Other cost | Evidence |
|---|---:|---:|---:|---:|---|
| RPC not landed | N/A | N/A | N/A | N/A | not measured |
| RPC landed success | N/A | N/A | N/A | N/A | not measured |
| RPC landed revert/fail | N/A | N/A | N/A | N/A | not measured |
| Jito single-tx not landed | N/A | N/A | N/A | N/A | not measured |
| Jito single-tx landed success | N/A | N/A | N/A | N/A | not measured |
| Jito single-tx landed revert/fail | N/A | N/A | N/A | N/A | not measured |
| Jito bundle not landed | N/A | N/A | N/A | N/A | not measured |
| Jito bundle landed success | N/A | N/A | N/A | N/A | not measured |
| Jito bundle landed/reverted | N/A | N/A | N/A | N/A | not measured |

No economics code may replace unknown cells with zero.

## 5. Provider economics matrix

Every paid provider considered for permanent production use must eventually have:

| Provider/path | Monthly cost | Usage cost | Coverage benefit | Latency benefit | Incremental captured profit | Keep/remove |
|---|---:|---:|---|---|---:|---|
| N/A | N/A | N/A | N/A | N/A | N/A | not measured |

Qualification rule:

~~~text
keep paid provider only if:
incremental sustainable net profit
>
full recurring/usage/dependency cost
~~~

## 6. Probability-model gate

Before P7:

~~~text
p(win) = UNKNOWN
p(land) = UNKNOWN unless directly measured for the path
optimal_wait = UNKNOWN
~~~

P4A uses deterministic economics only.

P4B may start only when H0/Shadow/P7 provide a versioned empirical dataset sufficient to estimate and validate:

~~~text
p(win | opportunity, premium, tag_age, tip, submission_path)
~~~

## 7. Raw evidence location

A dedicated raw-evidence directory does not yet exist because Project 0 implementation has not started.

When created, use one canonical location and link it here. Do not scatter Project 0 raw measurements across the older Pump.fun P0_* files.

## 8. Current status

~~~text
Project 0 production code:        NOT STARTED
Project 0 measurements:           N/A
Project 0 LIVE:                   NOT ENABLED
permanent paid provider approved: NONE
probabilistic ranking approved:   NO
~~~
