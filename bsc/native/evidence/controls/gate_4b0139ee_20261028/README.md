# Gate wallet/economic controls + daily BUY limit — final SHA 4b0139ee (28.10.2026, M4 Max)

No Mainnet action; real owner state afterwards: generation 0, every switch DISABLED, limit 0, runtime OFF.

| File | Command (cwd) | Exit | Result |
|---|---|---|---|
| cargo_workspace_test.txt | `cargo test --offline --no-fail-fast --workspace --all-features` (bsc/engine) | 0 | 86 suites, 1374 passed, 0 failed, 22 ignored |
| clippy.txt | `cargo clippy --offline -p darkbot-bsc-execution -p darkbot-bsc-qualification --all-features --all-targets -- -D warnings` (bsc/engine) | 0 | no warnings |
| gateway_test.txt | `cargo test --offline` (gateway) | 0 | 8 suites, 75 passed, 0 failed, 1 ignored |
| vitest.txt | `npx vitest run` (frontend) | 0 | 29 files, 124 passed |
| check_bsc_fields.txt | `node scripts/check-bsc-fields.mjs` (frontend) | 0 | F0-VAR PASS 2765 native, 116 proto |
| bsc_tools_tests.txt | the 7 `bsc/tools/test_*` (python3 / node) | 0 each | 18+7+3+4+5+5+4 = 46 passed |
| state_pin_FAIL_at_e63f7dcc.txt | `node bsc/tools/test_g06a_state_pin.cjs` before the test fix | 1 | 3/4: red since the simulator's switch check |
| darkbotctl_build.txt, release_manifest.txt, darkbot-release.json | `ops/darkbotctl build`; `python3 ops/release_manifest.py` | 0 / 0 | Coordinated release manifest: PASS |
| e2e_release.txt | `DARKBOT_E2E_PROFILE=release python3 ops/tests/bsc_control_test.py` | 0 | browser chain PASS, crash restart → switch DISABLED (RESTART_RESET) |
