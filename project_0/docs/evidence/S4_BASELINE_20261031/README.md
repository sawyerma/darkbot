# S4-04 core stage baseline — 2026-10-31 (LOCAL, loopback)

Real owner, DEC-13b offline fixture (loopback node + Jupiter, synthetic keys, no network): one liquidatable account's update through the ingress queue → state → detect → plan → sign → handoff, 200 runs, fresh owner each, FE-04 admission open before `T_rx` (steady state). Apple M4 Max, rustc 1.93.1, release. `T_handoff` is read in the FE-04 journal after its durable RESERVED/HANDOFF_UNKNOWN writes, immediately before the adapter call (time and allocation sample together).

`P0_S4_BASELINE_RUNS=200 cargo test --release -p darkbot-runner --features crate_health,alloc-count --lib s4_04 -- --ignored --nocapture --test-threads=1`

| stage (§6) | count | p50 ns | p95 ns | p99 ns | max ns | allocations p50/p95/max | queue wait p50/p95 ns |
|---|---|---|---|---|---|---|---|
| ingress_to_state | 200 | 81 919 | 131 071 | 262 143 | 461 291 | NOT_MEASURED (T_rx on the ingress thread) | ingress 9 215 / 13 311; trigger 18 431 / 26 623 |
| state_to_detect | 200 | 2 621 439 | 2 883 583 | 3 670 015 | 3 989 458 | 831 / 844 / 844 | RPC admission: fee quote (2×) 0 / 1 048 575 |
| detect_to_ready | 200 | 3 839 | 6 143 | 8 191 | 13 791 | 6 / 6 / 6 | none |
| ready_to_sign | 200 | 81 919 | 163 839 | 180 223 | 245 666 | 36 / 36 / 36 | none |
| sign_to_handoff | 200 | 10 485 759 | 11 534 335 | 12 582 911 | 15 540 333 | 383 / 383 / 387 | RPC admission: simulation, send 0 / 0 |

Quantiles are MT0 bucket upper bounds (≤ 12.5 %). Inside the stages: loopback Jupiter 1×/run p50 655 359 ns and fee RPC 2×/run p50 720 895 ns (state_to_detect); simulation 1×/run p50 196 607 ns and two fsync'd FE-04 writes (Apple F_FULLFSYNC, ~5 ms each) (sign_to_handoff). Once per owner, before `T_rx`: FE-04 admission open (journal) p50 5 291 667 ns, max 12 897 625 ns. Only evaluations that reach both points of a stage are counted; the MT0 stage and admission histograms are in-process (a run's points stay in its records).

Findings: (1) fixed — the RPC budget overslept a due pacing slot by up to 50 ms (state_to_detect p50 62.9 ms → 2.6 ms); (2) fixed — `T_handoff` was read before the durable writes (sign_to_handoff 0.39 ms → 10.5 ms, the real path); (3) open — production `fee_rps = 2` paces each further plan variant's fee quote by up to 500 ms (local fee model = P4A-02, parity-gated); (4) open — an owner's first LIVE evaluation opens the FE-04 journal (~5.3 ms). Mainnet tails: NOT_MEASURED.
