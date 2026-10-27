# G0.6 concurrency — head-of-line of simultaneous launches (M4 Max, release, PAPER)

`c3_paper_latency` at `ce385f95` (`BSC_C3_LATENCY_BURST=1|2|4|8`, `BSC_C3_LATENCY_EVIDENCE_DIR`): launches published k at a time, taken one by one by the single C3 executor; per burst position the latency from the burst's first publish to the PAPER sink handoff (`T_shadow_sink`, never `T_send`).

| burst | last position p50 / p99 | ≥ 1 ms |
|---|---|---|
| 1 (`…317086`) | 218 / 258 µs (`T_ingress→T_shadow_sink`) | 0 |
| 2 (`…317992`) | 469 / 521 µs | 0 |
| 4 (`…318875`) | 938 / 1 020 µs | 10 of 500 |
| 8 (`…319751`) | 1 870 / 2 020 µs (position 4 on: every sample ≥ 1 ms) | all from position 4 |

Validity per run: every handoff answers its own launch, distinct tokens in their calldata, PAPER signer. Limits: all C3 PAPER BUYs carry the arm's next nonce (alternatives from one canonical state, never mined) — a LIVE nonce sequence of simultaneous BUYs from one sender is not exercised (the production Unit-4 lane blocks behind an unresolved head); synthetic arrival pattern; dev host with the G0.4 campaign running; no transport, no acquisition, no `T_buy−T0`.
