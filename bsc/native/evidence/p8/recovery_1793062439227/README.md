# Paket 8: release runtime start, crash, restart, stop at the final build (27.10.2026, read-only Mainnet, SHADOW)

- Build: `bsc_runtime` release from `ops/darkbotctl build` at `73881d51` (manifest PASS), sha256 `e8547e7c82f28b60f631575c928c619004f2c6bbd7128f94c93f3be7e206eaf0`; config `ingress.public.env` here (= `capture-run.dbrec` `config_sha256`; a fresh unfunded PAPER identity outside the repository, `recovery.sh`).
- Modes: `BSC_INGRESS_PUBLICNODE`, `BSC_STATE_C2`, `BSC_EXEC`, `BSC_STRATEGY_C` = SHADOW (generations 1–4, APPLIED).
- Run `1793062439227`: 150 s, then SIGKILL (a crash). Its journal stays readable (789 records); without a manifest: `replay-proof` FAIL ("not sealed"), `capture-quality` NOT_CLEAN, `strategy-replay` INCOMPLETE, census window UNTERMINATED.
- Run `1793062591241`: the restart, 150 s, then the stop file; exit 0. `replay-proof` PASS (3 Flap sightings, census COMPLETE, deterministic); `capture-quality` CLEAN (Phase F acceptance NOT_MET: no native geth source, external); `strategy-replay` PASS 13/13; Four pre-execution tokens derived 3, mismatch 0.
- Recovery (`runtime-status-run2.dbrec`): C2 checkpoint RESTORED (64 blocks); Four drift guard `MATCH at block 129997241`; C3 `DEPLOYMENT_DRIFT` (Portal v5.27.2, fail-closed); `sender_paths` C3+STRATEGY_C; Strategy C SHADOW.
- Nothing was signed with a funded key or sent.
