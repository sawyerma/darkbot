# Solana / Project 0 — Measurement & Evidence Ledger

Status: **NOT STARTED / EMPTY BASELINE**  
Updated: 2026-09-30

This is the single human-readable measurement ledger for the new Solana / Project 0 Liquidator.

No Project 0 latency, capture-rate, winner-position or realized-PnL measurement is currently recorded here. Values remain **N/A** until produced by the actual implementation.

## 0. Rules

- Never write an unmeasured value as 0.
- Separate local processing latency from network/inclusion/landing evidence.
- Submission ACK or Jito bundle ID is not landing evidence.
- Use the same timing vocabulary across replay, shadow and LIVE.
- Every result records source commit, config/source manifest, sample count and raw-evidence path.
- Gate/benchmark code must exercise production components, not duplicate business logic.

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
observed receiver/token deltas
actual fees
realized PnL
~~~

## 2. Phase evidence table

| Phase | Measurement | Status | Evidence |
|---|---|---|---|
| S0 | timing/telemetry contract | N/A | not started |
| S1 | ingress throughput/decode/gaps/reconnect | N/A | not started |
| S2 | state reconciliation/fork/gap recovery | N/A | not started |
| S3 | build/sign/serialize/submission adapter | N/A | not started |
| S4 | Python↔Rust Pump.fun parity + native baseline | N/A | not started |
| H0 | historical Project 0 liquidation composition | N/A | not started |
| P2 | affected accounts/update + timed-recheck load | N/A | not started |
| P3 | exact health/eligibility parity | N/A | not started |
| P4 | economics parity | N/A | not started |
| P5 | route/build/size/CU/bundle feasibility | N/A | not started |
| P6 | full hotpath + shadow race | N/A | not started |
| P7 | capture/economics/competition evidence | N/A | not started |
| P8 | controlled LIVE reconciliation | N/A | not started |
| P9 | production metrics | N/A | not started |

## 3. Required record shape

Every measurement entry should state:

~~~text
date/time
phase
repo commit SHA
protocol/source manifest
runtime config
provider/region
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

## 4. Raw evidence location

A dedicated raw-evidence directory does not yet exist because Project 0 implementation has not started.

When created, use one canonical location and link it here. Do not scatter Project 0 raw measurements across the older Pump.fun P0_* files.

## 5. Current status

~~~text
Project 0 production code: NOT STARTED
Project 0 measurements:     N/A
Project 0 LIVE:             NOT ENABLED
~~~
