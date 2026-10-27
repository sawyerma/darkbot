# Paket 9: the release candidate's own runtime: start, crash, restart, stop (27.10.2026, read-only Mainnet, SHADOW)

- Build: `bsc_runtime` release from `ops/darkbotctl build` at `8f26b02a` (manifest PASS, the release candidate's artifact), sha256 `6b3c59bbf6f249f81b5a200093d6b0fc735c0de4868640a96111a2b438d961aa`; config `ingress.public.env` here (= `capture-run.dbrec` `config_sha256`; a fresh unfunded PAPER identity outside the repository, `recovery.sh`).
- Modes: `BSC_INGRESS_PUBLICNODE`, `BSC_STATE_C2`, `BSC_EXEC`, `BSC_STRATEGY_C` = SHADOW (APPLIED).
- Run `1793074269302`: 150 s, then SIGKILL. Readable (776 records); unsealed: `replay-proof` FAIL, `capture-quality` NOT_CLEAN, `strategy-replay` INCOMPLETE, census UNTERMINATED.
- Run `1793074421330`: the restart, 150 s, then the stop file; exit 0. `replay-proof` PASS, `capture-quality` CLEAN (Phase F acceptance NOT_MET: no native geth source, external), `strategy-replay` PASS 3/3.
- Recovery and the pass-2 fixes live (`runtime-status-run2.dbrec`, census): C2 checkpoint RESTORED (64 blocks); Four drift guard `MATCH at block 130023517`; Four Classic launches counted as candidates (`four_pending_candidate`, census `candidate=3`, none derived or actionable); C3 `DEPLOYMENT_DRIFT` (fail-closed); Strategy C SHADOW.
- Nothing was signed with a funded key or sent.
