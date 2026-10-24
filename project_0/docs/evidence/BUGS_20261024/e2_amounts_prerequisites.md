# BUGS E2 (amounts bound to sizing, prerequisites swap-only) — 2026-10-24, PARTIAL

Offline, no Mainnet, no LIVE, no submission. Code commit `eabd9a7e`. Gate `project_0/scripts/gate.sh` on it: fmt, clippy ×4, test_all 807/0/10,
test_default 805/0/10, runner 369/0/3 ×2, release, Python OK — all exit 0.

- `receivership::builders::built_amounts` decodes the withdraw (plain, Kamino `flags = Some(1)`
  = all, Drift, JupLend) and the repay of the built receivership instructions from their signed
  bytes (`E2_AMOUNTS_SHAPE`, `E2_AMOUNTS_UNDECODABLE`); the builder refuses a plan whose encoded
  amounts differ from the sizing (`E2_AMOUNTS_NOT_SIZED`; the integration withdraw through
  `execution_amount_for_asset`). Test `e2_built_amounts_are_decoded_from_the_signed_bytes`.
- `planner::validate_bundle`: a prerequisite transaction carries pinned swap steps only (the JIT
  buy; `route.rs::exact_in` already refuses non-Jupiter instructions); `Other`, Withdraw, Repay
  outside the receivership ⇒ `prerequisite step is not a pinned swap`. Test `bundle_rules`;
  `v9_technical_plan_rejections_keep_their_own_cause` fixture names its swap program.
- Mutations fail-before: E2a repay encoded off the sizing, E2b Kamino all-flag misread, E2c
  prerequisite `Other` allowed, E2d fixture without swap table.
- Still open: Jupiter swap discriminator classification (BLOCKED, no pinned Jupiter IDL); route
  lookup-table content/deactivation against the on-chain account (needs an RPC read path).
