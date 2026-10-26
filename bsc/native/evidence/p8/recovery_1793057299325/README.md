# Paket 8: release runtime start, crash, restart, stop (27.10.2026, read-only Mainnet, SHADOW)

- Build: `bsc_runtime` release from `ops/darkbotctl build` at `53436297` (manifest PASS), sha256 `f885a83ce73d68e4a26cdaaa6d4799e32a3c49350452f44031661142fb07604d`; config = committed `bsc/config/ingress.public.env` with a fresh unfunded PAPER identity outside the repository (`recovery.sh`).
- Modes: `BSC_INGRESS_PUBLICNODE`, `BSC_STATE_C2`, `BSC_EXEC`, `BSC_STRATEGY_C` = SHADOW (generations 1–4, APPLIED).
- Run `1793057299325`: 150 s, then SIGKILL (a crash). Its journal stays readable (576 records). It has no manifest, so `replay-proof` FAIL ("not sealed"), `capture-quality` NOT_CLEAN, `strategy-replay` INCOMPLETE, census window UNTERMINATED.
- Run `1793057451341`: the restart, 150 s, then the stop file; exit 0. `replay-proof` PASS (manifest match, census COMPLETE, deterministic). `strategy-replay` PASS (6/6). `capture-quality` NOT_CLEAN (no native geth source: external).
- Recovery (`runtime-status-run2.dbrec`): C2 checkpoint RESTORED (64 blocks; 4 writes, 0 errors); C3 `DEPLOYMENT_DRIFT` (Portal v5.27.2, fail-closed, no lane); `sender_paths` C3+STRATEGY_C; Strategy C SHADOW; PUBLICNODE HEALTHY.
- Nothing was signed with a funded key or sent.
