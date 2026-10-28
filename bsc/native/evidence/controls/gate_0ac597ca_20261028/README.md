# Gate BSC-002 repair + bsc/tools in cargo and CI — final code SHA 0ac597ca (28.10.2026, M4 Max)

Origin of the gap: 86de25c9 (switch check in the DRY simulator before BSC-002; the spawned state-pin test stopped at CONTROL_DISABLED). No Mainnet action; the real owner state stays absent (= every switch DISABLED).

| File | Command (cwd) | Exit | Result |
|---|---|---|---|
| origin_86de25c9_state_pin.txt | original test (c2267886) vs simulator before / at 86de25c9 | 0 / 1 | 4/4 before, 3/4 at 86de25c9 (CONTROL_DISABLED) |
| mutant_real_pin_removed_test_A_red.txt | real simulator with the pin made ineffective, test A | 1 | A red |
| mutant_main_header_hash_test_E_red.txt | main() taking the header hash from the chain, test E | 1 | E red |
| cargo_workspace_test.txt | `cargo test --offline --no-fail-fast --workspace --all-features` (bsc/engine) | 0 | 88 suites, 1382 passed, 0 failed, 22 ignored |
| bsc_tools_counts.txt | `cargo test --all-features -p darkbot-bsc-{execution,qualification} --test bsc_tools -- --nocapture` | 0 / 0 | dry_modes 18, shell_scope 7, key_hygiene 3, private_mode 4, state_pin 9, readiness_job 5, status_sync 4 = 50, fail/error/skip 0 |
| clippy.txt | `cargo clippy -p darkbot-bsc-execution -p darkbot-bsc-qualification --all-features --all-targets -- -D warnings` | 0 | no warnings |
| gateway_test.txt | `cargo test --offline` (gateway) | 0 | 8 suites, 75 passed, 0 failed, 1 ignored |
| vitest.txt | `npx vitest run` (frontend) | 0 | 29 files, 124 passed |
| check_bsc_fields.txt | `node scripts/check-bsc-fields.mjs` (frontend) | 0 | F0-VAR PASS 2765 native, 116 proto |
| darkbotctl_build.txt, release_manifest.txt, darkbot-release.json | `ops/darkbotctl build`; `python3 ops/release_manifest.py` | 0 / 0 | Coordinated release manifest: PASS (binaries identical to 4b0139ee) |
| e2e_release.txt | `DARKBOT_E2E_PROFILE=release python3 ops/tests/bsc_control_test.py` | 0 | browser chain PASS |
| ci_bsc_run_40607601904.txt | GitHub Actions `bsc` on 0ac597ca | success | macos-latest + ubuntu-latest, all steps success |
