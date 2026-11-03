# §3b-Register (Messplan §3b/§3b-D, REV.1.0 R11)

Jeder §3b-Fall mit seinem Nachweis auf dem aktuellen Code. PASS heißt: der genannte Test besteht im vollen BSC-Gate und in der CI des Commits, der diese Datei zuletzt geändert hat.

**Ebene:**
- **P:** echter `bsc_runtime`-Prozess (Harness-Build `loopback-harness`: die TLS-Clients vertrauen zusätzlich der Test-CA; nie der Produktions-Build). Er läuft gegen Loopback-Mock-WSS über TLS, der kanonische Endpunkt ist ein geschlossener Port, der Owner wird über die CLI gesteuert.
- **I:** Laufzeit bzw. LIVE-Pfad im Prozess des Tests, mit dem Produktionscode.
- **U:** Owner- oder Entscheidungslogik.

**Abkürzungen:**
- rp = `execution/tests/runtime_process.rs`
- so = `execution/src/sources.rs`
- rt = `execution/src/runtime.rs`
- rx = `execution/src/runtime_exec.rs`
- in = `execution/src/ingress.rs`
- rj = `execution/src/runtime_journal.rs`
- le = `execution/tests/live_e2e.rs`
- ag = `execution/tests/acceptance_guard.rs`
- gn = `execution/tests/g07_no_send.rs`

| ID | Nachweis | Ebene | Status |
|---|---|---|---|
| A01–A04, A09, A11, A15 | rp `the_ingress_runs_exactly_the_sources_their_modes_name` | P | PASS (A11 nur Mock, kein Provider-PASS) |
| A05–A08, A16 | rp `a_failing_source_is_counted_as_its_own_and_never_actionable` (A16: wohlgeformt, nur die Verifikation verweigert Hash/Signer) | P | PASS; volle Queue im Prozess nicht erzwingbar: in `memory_is_bounded_and_counters_cover_every_event`, rj `a_full_queue_loses_visibly_never_silently` (U) |
| A10, A12–A14 | rp `unqualified_and_live_requested_sources_connect_nothing` | P | PASS |
| B01–B06, B08, B10–B13, B16 | rp `the_runtime_mode_and_the_venues_decide_what_runs`; rt `the_runtime_mode_bounds_the_components_as_the_plan_says` | P, U | PASS (B05 im Prozess nur die fail-closed-Hälfte) |
| B05, B07, B09 (C3/Strategie mit C2) | rx `a_runtime_pass_reaches_the_sink_and_the_journal`, `a_strategy_action_reaches_the_sink_only_when_armed`, `the_paper_path_waits_probes_and_fails_closed` | I | PASS; im Prozess EXTERNAL_BLOCKED (s. u.) |
| B14, B15 | rt `the_runtime_mode_bounds_the_components_as_the_plan_says`; so `the_owner_never_makes_a_source_live`; rp A14 | U, P | PASS (seit Plan 28.10. LIVE nur mit verifiziertem Grant: le) |
| 8er-Unterkombination C2/EXEC/STRATEGY_C | rt `the_runtime_mode_bounds_the_components_as_the_plan_says` | U | PASS |
| C01 | so `the_wallet_switches_are_independent_and_disabled_initially`, `the_runtime_mode_is_off_initially_and_never_live` | U | PASS |
| C02–C07 + 8 L/B/S | so `the_sign_guard_holds_for_exactly_one_of_eight_settings` | U | PASS |
| C08, C13, C14 | so `the_daily_buy_limit_counts_every_attempt_once_per_utc_day`, `the_daily_limit_holds_across_days_clocks_and_changes`, `concurrent_buy_attempts_never_exceed_the_limit`, `the_daily_limit_takes_the_runners_attempt_ids` | U | PASS |
| C09, C11 | ag `guard_is_compiled_unarmed`, `unarmed_guard_refuses_a_fully_matching_context_before_reading`, `the_guard_itself_binds_local_simulation_economics_and_portal_upgrade` | U | PASS |
| C10 + 8 A/D/T | so `each_of_the_six_attempt_switches_alone_refuses_the_attempt`, `live_is_effective_only_when_every_condition_holds` (8 A/D/T); gn `the_native_switches_gate_signing_the_authority_and_the_send` | U | PASS |
| C12 | rp A14/C12 (Quellen, Runtime-Modus, Komponenten, Venues LIVE: exakter Grund); so `the_owner_never_makes_a_source_live`, `live_is_effective_only_when_every_condition_holds` | P, U | PASS (seit Plan 28.10. Grund `LIVE_GRANT_MISSING` statt `ECONOMIC_LIVE_NOT_AUTHORIZED`) |
| C15 | rp `a_killed_runtime_restarts_disabled_with_its_limit_and_journal`; so `a_restart_disables_every_switch_and_keeps_the_modes` | P, U | PASS |
| C16 | gn `one_shared_attempt_authority_is_consumed_before_the_latch_and_returned_only_unsent`, `a_crash_anywhere_after_the_authority_is_consumed_never_releases_the_attempt` | U | PASS (Läufer mit echtem Owner tabu: OS-Home-Regel) |
| C17, C18 | rp `owner_races_replays_and_an_unreadable_owner_fail_closed`; so `concurrent_writers_of_one_generation_have_one_winner`, `receipts_are_recovered_only_for_the_same_request` | P, U | PASS |
| C19 | rp (unlesbarer Owner beendet die Runtime mit STOP-Grund); so `a_missing_unreadable_or_tampered_state_refuses_every_switch`; fremdes HOME `bsc/tools/test_g06a_state_pin.cjs` C; veraltete GUI-Projektion `frontend/src/features/bsc/model/sources.test.ts` (STALE) | P, U | PASS |
| C20 | F0-VAR, `frontend/tests/bsc.spec.ts`; Release-Browser-E2E `ops/tests/bsc_control_test.py` zuletzt PASS `4b0139ee`/`0ac597ca` | U | OPEN: Release-E2E liest öffentliche RPCs (verboten) → hermetische Variante R14 |
| Crash/Disk-full | rp `a_killed_runtime_restarts_disabled_with_its_limit_and_journal`, `a_full_disk_stops_the_runtime_and_its_run_reads_incomplete` (macOS-Disk-Image) | P | PASS |

**Offen/extern:**
- **Prozess-Arm (C3-PAPER-Senke und LIVE-Loopback im echten Prozess): EXTERNAL_BLOCKED.** Die Startsonde prüft den SHA-256 der Portal-Implementierung v5.27.2 und von Tax-V3 gegen den eingefrorenen Pin. Deren Laufzeitcode liegt offline nicht vor (die Mitschnitte vom 27.09. sind v5.24.0). Ein einmaliges Read-only-Lesen von einem erlaubten Host entsperrt das; dieses Lesen entscheidet der Betreiber. Im Prozess des Tests ist derselbe Pfad PASS (rx, le).
- **§9.5 (jeder Testfall namentlich gegen seine Anforderung):** R14-Gesamtmatrix.
