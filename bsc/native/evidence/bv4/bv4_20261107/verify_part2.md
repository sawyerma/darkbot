# bugs_v4 verification part 2 (agent, read-only), 07.11. ~00:50
CONFIRMED_OPEN: AH1 (runtime_config.rs:108 gas_limits_cover map_or(0); fix missing class Err "no {venue} {action} gas evidence", only Four BuyInLaunchBlock explicitly optional; test GasEvidence::default -> Err; frozen without Flap SELL -> Err; full -> Ok)
  AR1 (=G6, validate twice runtime.rs:2061; bind once with G5)
  AS2 (gate gap: release-only ignored tests runtime_exec.rs:2245, runtime_live_tests.rs:7417/7455 never run: add cargo test --release --offline --locked -p darkbot-bsc-execution --features runtime --lib -- a_runtime_pass_reaches_the_sink_and_the_journal a_live_buy_starts_within_its_deadline_on_the_real_clock a_strategy_buy_starts_within_its_deadline_after_its_admission ; proof "3 passed")
  Z2 (text only: economics/snapshot.rs:167 "no exit cost", :223 "no gas evidence (R12c open)"; fix at next controlled re-pin per bugs_v4)
MEASURE RELEVANT (agent 2): AG1(=F3 QoS; agent1 says unproven -> measure with/without QoS), AP3(=H2 reconcile split per sub-step), AR2 (runtime.rs:1671 synchronous adapter join on source switch-off; stop numbers 24.9/36/177 ms; fix stopping list joined when finished, journal Stopped then, no respawn meanwhile; test adapter exits 200 ms after flag -> pass < 10 ms), AR3(=P24-M1; agent1 NOT_RELEVANT), AR4 (drain order KEEP, cost under P24-M3/G1)
CONDITIONAL: L2 (repro: copy concurrent_admissions_never_exceed_the_grant sources/tests.rs:3822; AFTER_SPEND_READ hook revokes via second Store; expect today Ok; fix apply under spend_lock, try_lock failure = refused revoke), L3 (return_leases concurrently; KEEP if only conservative)
EXTERNAL: S1 (builder DNS/peer IP vs Q5B pins), S2 (READY/STALE/CONNECTING shares on Mac)
OPERATOR: V1 (per-property evidence matrix before C3 LIVE), AE1 (unfiltered LIVE allowed?), AP5 (LIVE probe retry threshold)
ALREADY_FIXED: S4 (A6,B3 85073528), Z3 (bsc-economics-8)
REFUTED: N1, Q2(=H6), S3, U1, W1, Z1(stale text -> Z2), AI5
KEEP: K3, N2, O2, S5, T1, W2, X2, AF1 (reviewer fix refuted), AI1, AI4, AJ2, AK1, AK2, AK3, AL3, AM3, AM4, AO2, AO3, AP1, AQ4, AR5(=D3/E2), AR6, AS3
MEASURE NOT_RELEVANT: O1, P1, X1, Y1, AB1, AC1, AD1, AI2, AI3, AJ1, AL1, AL2, AL4, AM1, AM2, AN3, AS1
MEASURE no R14 number (LIVE off): K1, K2, L1, Q1, AN1, AN2, AN4(=G2), AO4, AP2, AP4, AQ3
DUPLICATES: Y1b=Y1, U2=S4/B3, AO1=K2, AQ1=K2, AQ2=AP4
