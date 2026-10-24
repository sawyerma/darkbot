# CURRENT_MAINNET — P1A freeze (2026-10-16)

Single code source of every value below: `native/crates/liquidation/src/protocol.rs`. Parity with the pinned type crate and
`Cargo.lock` is enforced by `protocol::tests::source_pin_parity_with_type_crate`; the deployment decoder and pin by
`protocol::tests::deployment_freeze_decodes_and_fails_closed`.

| Item | Frozen value | Status |
|---|---|---|
| Program ID | `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` (= `marginfi_type_crate::ID`, feature `mainnet-beta`) | source-pinned, tested |
| ProgramData address | `PDA([program], BPFLoaderUpgradeab1e…)` (`program_data_address`) | derived |
| Deployed program hash | reference `H57mqzeof3mnCmVTPJzmzeTfs7G6zbpaApQP4yNycr3f` (0.1.11, reads 2026-10-08); **observed 2026-10-24 (FE-01 smoke `da8512ad`): SHA-256 `3e3626a9…00de` ≠ reference — upgraded** | observed value is evidence, **not qualified** |
| Deployment slot, upgrade authority | observed slot 456 093 551 (after the reference observation); logged at every Shadow start | upgrade authority not yet recorded |
| Source / release | pinned candidate `mrgn-0.1.12-rc2` (`9fcb729a`, `c9decd7f`, P1B-03); deployment ↔ source mapping **not proven** | historical pin `mrgn-0.1.11-rc1` (`33c67987`) |
| IDL | the pin ships no marginfi IDL (only integration IDLs); layouts and discriminators come from the type crate directly; on-chain IDL account not read | IDL hash NOT_RUN |
| Account discriminators | MarginfiAccount `43b2826d7e721c2a`, Bank `8e31a6f2324261bc`, MarginfiGroup `b617adf097ceb643`, FeeState `3fe01055c124ebdc`, LiquidationRecord `5f74178459d2f5a2` | = type crate, tested |
| Bank layout | payload 1856 B (raw 1864 incl. discriminator) = `size_of::<Bank>()`; 3904 B is the 0.1.12 layout (P1B); any other length `UNKNOWN_LAYOUT` | tested |
| Other account layouts | decoded only through type-crate `size_of` guards (`8 + size_of::<T>()`) | source-pinned |
| Receivership instructions | start, end, init_liquidation_record, withdraw, repay, kamino/drift/juplend withdraw | = type crate `ix_discriminators`, tested |
| Receivership program allowlist | ComputeBudget, marginfi, Kamino, Drift, JupLend, Jupiter v6, Titan (`receivership_allowed_programs`) | source-pinned |
| Refresh allowlist (before start) | Kamino RefreshReserve/RefreshReservesBatch/RefreshObligation, init_liquidation_record, Drift UpdateSpotMarketCumulativeInterest, JupLend UpdateRate | source-pinned (`liquidate_start.rs`) |
| OracleSetup, health rules, flat SOL fee, premium limit | `docs/evidence/P0_20261016/ORACLE_MODEL.md`, `docs/architecture/p0_reuse_matrix.md` (Economics) | source-pinned |
| Asset tags, premium activation | not in 0.1.11 → P1B | — |

## Upgrade watch (P1A-04)

- Startup: the ProgramData account is decoded and compared with the pinned reference (`deployment_pin`). Anything other
  than `Reference` (other bytes, or a program without a qualified deployment) latches `PROGRAM_UPGRADE` HALT before any
  detection work.
- Runtime: every ProgramData account write is hashed against the startup baseline; a change latches the same HALT.
- No automatic resume: a new deployment needs a new source pin, layout/IDL/source requalification and parity (P1B promotion).

## Gate P1A: BLOCKED (2026-10-24: upgrade observed, RC2 candidate pinned, deployment unqualified — runtime halted)

The source-pinned side is complete. Whether the deployment *running today* is still `H57mqz…` needs one finalized read of
the program and ProgramData accounts on Mainnet, which needs separate read-only permission. Unblock step: authorized bounded
read (2 accounts, FREE_ONLY), compare with the table above; on mismatch, P1B promotion before any execution.
