# BUGS closure B3 / B15 / C3 / C12 — 2026-10-24 (LOCAL_PASS, offline)

Tested SHA: `c9decd7f` (+ the closing commit of this file, which only makes the RAM reconcile
record's `exposure_released = 0` explicit). Gate `project_0/scripts/gate.sh` on `c9decd7f`: fmt,
clippy ×4, test_all 805/0/10, test_default 803/0/10, runner 368/0/3 ×2, release, Python OK —
all exit 0; GitHub Actions `project0-gate` 5/5 PASS. No Mainnet, no LIVE, no submission.

| ID | Closing commits | Regression tests (exit 0 in the gate) |
|---|---|---|
| B3 durable Safety replay | `4bb683d`, `8bfa63f2`, `122e7e9c`, `8c7acbba`, `c7b0e6f1` | `b3_safety_replays_the_durable_attempt_after_restart`, `b3_restored_attempt_halts_and_keeps_its_capital`, `b3_settled_attempt_books_its_day_once`, `c3_settled_open_inventory_is_held_on_any_day`, `c3_settlement_day_is_the_same_in_the_run_and_after_a_restart_across_midnight`, `c3_owner_refused_safety_booking_retains_holds_and_the_restart_books_it`, `fe04_exit_restart_replays_the_attempt_and_its_exit` |
| B15 inventory limit | `e08d3a7`, `07c3f09`, `3e28b1a`, `8bfa63f2`, `8c7acbba` | `b15_inventory_limit_counts_worst_case_new_and_in_flight_holds`, `b15_inventory_hold_is_the_sized_seized_collateral`, `c3_token_inflow_is_open_inventory_never_realized`, `c3_settlement_day_is_the_same_in_the_run_and_after_a_restart_across_midnight` (open inventory booked from finalized deltas, held after restart) |
| C3 settlement / one source of truth | `2605556c`, `d850d478`, `122e7e9c`, `8c7acbba`, `c7b0e6f1`, `cbb0c243`, `760f858c` | settlement tests (landed success/failure, never landed with block-height proof, 429/missing archive UNKNOWN, partial bundle, fees without token deltas, replay/restart, no double booking), `c3_owner_*`, `p8_ram_reconcile_is_telemetry_and_never_books_an_fe04_attempt`, exit tests |
| C12 timeout is no proof | `e08d3a7`, `d850d478`, `122e7e9c` | `c12_timeout_is_unknown_and_releases_nothing`, `c12_non_landing_proof_comes_from_height_history_and_statuses`, `c3_owner_keeps_unknown_holds_and_retries_after_the_backoff` |

Remaining outside these tickets (not closed by this file): real chain reconciliation and exits
on Mainnet (NOT_RUN), FE-06 production wiring of the admissions (B1), the in-run admission's
`check()` reservation released through the journal settlement at that wiring.
