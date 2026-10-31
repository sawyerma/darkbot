# S4-04 core stage baseline — 2026-10-31 (LOCAL, loopback)

Real owner, DEC-13b offline fixture (loopback node + Jupiter, synthetic keys, no network): one liquidatable account's update through the ingress queue → state → detect → plan → sign → handoff, 200 runs, fresh owner each, FE-04 admission open before `T_rx` (steady state). Apple M4 Max, rustc 1.93.1, release.

`P0_S4_BASELINE_RUNS=200 cargo test --release -p darkbot-runner --features crate_health,alloc-count --lib s4_04 -- --ignored --nocapture --test-threads=1`

| stage (§6) | count | p50 ns | p95 ns | p99 ns | max ns | allocations p50/p95/max | queue wait p50/p99 ns |
|---|---|---|---|---|---|---|---|
| ingress_to_state | 200 | 98 303 | 147 455 | 163 839 | 744 042 | NOT_MEASURED (T_rx on the ingress thread) | ingress 10 239 / 18 431; trigger 20 479 / 36 863 |
| state_to_detect | 200 | 2 621 439 | 2 883 583 | 3 145 727 | 3 872 375 | 831 / 850 / 850 | inline |
| detect_to_ready | 200 | 4 607 | 6 143 | 7 167 | 7 958 | 6 / 6 / 6 | inline |
| ready_to_sign | 200 | 114 687 | 163 839 | 196 607 | 235 834 | 36 / 36 / 36 | inline |
| sign_to_handoff | 200 | 393 215 | 655 359 | 720 895 | 807 750 | 505 / 505 / 505 | inline |

Quantiles are MT0 bucket upper bounds (≤ 12.5 %). Loopback remotes inside the stages: Jupiter 1×/run p50 720 895 ns (state_to_detect), fee RPC 2×/run p50 720 895 ns (state_to_detect), simulation 1×/run p50 229 375 ns (sign_to_handoff). One-time per owner (before `T_rx`): FE-04 admission open (journal) p50 5 449 333 ns, max 11 389 541 ns.

Findings: (1) fixed in this commit — the RPC budget overslept a due pacing slot by up to 50 ms (state_to_detect p50 62.9 ms → 2.6 ms); (2) open — the production `fee_rps = 2` paces each further plan variant's fee quote by up to 500 ms (local fee model = P4A-02, parity-gated); (3) open — the first LIVE evaluation of an owner opens the FE-04 journal (~5.5 ms). Mainnet tails: NOT_MEASURED.
