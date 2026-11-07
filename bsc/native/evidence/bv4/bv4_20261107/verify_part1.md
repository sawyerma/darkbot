# bugs_v4 verification part 1 (agent, read-only, code = wtEA 6a5584f7 + R14 diff), 07.11. ~00:40
CONFIRMED_OPEN: C1 (runtime_exec.rs WaitingNonce arms on stale validation; fix: before clean_next_nonce/arm, if last_probe older than reprobe drop validated + spawn_probe; test WAITING_NONCE aged past reprobe + upgraded prober -> DEPLOYMENT_DRIFT, + probe-in-flight case)
  C2 LOW (runtime_exec.rs:527 WaitingC2 without disarm when C2 switched OFF while armed; fix self.stop(Phase::WaitingC2) when executor.is_some(); test arm then tick(true,None,..) -> executor None, lanes 0, fence removed)
  C4 (paper_exec.rs:3175 pre-check ignores C3 skip; fix skip = strategy_used==0 && c3_took_nonce; len < used+skip+needed -> LanesExhausted before taking; test 2 lanes, C3 disclosed, unapproved exit -> LanesExhausted lanes unchanged; approved exit lane 1 ok; mutant without skip red)
  D5 (runtime_strategy.rs:1581 GapSpan ignored; verdict() never sees lost; fix count GapSpan into Replay.lost, verdict INCOMPLETE; test input+Y replaced by GapSpan -> INCOMPLETE)
  G5 (runtime.rs:2081 Err/panic keep four_validated; spawn error no v record; fix consecutive failures count, configured BSC_CANONICAL_FOUR_PROBE_FAILURES -> four_validated None, reset on MATCH/DRIFT, journal four_probe_payload(3,0) on spawn error, validate once (G6); test extracted outcome fn)
MEASURE RELEVANT: D2 (status replace on decision thread p99 4-5 ms; fix: second CheckpointWriter latest-wins slot with replace_volatile, encode stays on loop, error-abort via results; test hook in replace_with records thread -> not runtime thread; blocking replace must not delay next event)
CONDITIONAL: D4 (tls_config failure -> double disconnect + restart every 0.5 s; repro inject), L2 (apply without spend_lock vs admit_live paused AFTER_SPEND_READ: LiveGrant OFF APPLIED then admit Ok), L3 (return_leases without spend_lock; worst LIVE_SPEND_UNREADABLE fail-closed)
ALREADY_FIXED: J1(=B2, 85073528 T105)
REFUTED: C5, H1 (tests/tls.rs:853), H6 (runtime_live.rs:3315), J2 (openfour.rs:205), N1 (profile.rs:310)
KEEP: C3, C6, D3, E2, E4, E5, F2, G6 (bind once in G5), H4, H5, K3, P24-M2, N2
MEASURE NOT_RELEVANT (R14): M1, D1, E1, E3, F1, F3 (QoS unproven; split majflt/nivcsw first), G1, G2, G3, G4, G7, J3, P24-M1, P24-M3, P24-M4, O1, O2, P1
MEASURE not covered by R14 (LIVE path, no LiveRuntime ran): H2, H7, I1, I2, K1, K2, L1, Q1
DUPLICATES: H3=C1, Q2=H6
