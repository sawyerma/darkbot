# CURRENT_MAINNET — P1A freeze (2026-10-16), promoted to mrgn-0.1.12-rc2 (2026-11-03, C-01 step 7)

Single code source of every value below: `native/crates/liquidation/src/protocol.rs`. Parity with the pinned type crate and
`Cargo.lock` is enforced by `protocol::tests::source_pin_parity_with_type_crate`; the deployment decoder and pin by
`protocol::tests::deployment_freeze_decodes_and_fails_closed`.

| Item | Frozen value | Status |
|---|---|---|
| Program ID | `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` (= `marginfi_type_crate::ID`, feature `mainnet-beta`) | source-pinned, tested |
| ProgramData address | `PDA([program], BPFLoaderUpgradeab1e…)` (`program_data_address`) | derived |
| Deployed program hash | reference `5BrAnWXJFY2Z5sM6PRXWZBYL9ewVEhKKpFjPC8jpXEkm` (SHA-256 `3e3626a9…00de` of the whole ProgramData `4Q8u2ny8…vPQR7`); ELF `eb761836…cdab` (3 860 777 B) = reproducible build of `mrgn-0.1.12-rc2` (`solana-verify`, image `solana-verifiable-build@sha256:1a0ea3dd…`); former reference `H57mqz…` (0.1.11) historical only | qualified: C-01 plan `edf159af0e91fb03` `P1A_PARITY PASS` (BUILD, FE01 plan `1accaed7f43fb811`) |
| Deployment slot, upgrade authority | slot 456 093 551, upgrade authority `J3oBkTkDXU3TcAggJEa3YeBZE5om5yNAdTtLVNXFD47` | read 2026-11-02 (C-01), observed again by FE-01 2026-11-03 |
| Source / release | `mrgn-0.1.12-rc2` (`9fcb729a`); deployment ↔ source proven by the byte-identical build | historical pin `mrgn-0.1.11-rc1` (`33c67987`) |
| IDL | on-chain IDL account stale (91 instructions, `c5c778c6…`) ≠ release IDL `b4f80a6c…` (124); informational once BUILD passes (user decision 2026-11-02) | layouts and discriminators from the type crate |
| Account discriminators | MarginfiAccount `43b2826d7e721c2a`, Bank `8e31a6f2324261bc`, MarginfiGroup `b617adf097ceb643`, FeeState `3fe01055c124ebdc`, LiquidationRecord `5f74178459d2f5a2` | = type crate, tested |
| Bank layout | payload 3904 B (0.1.12, `BANK_PAYLOAD_LEN_PINNED`); 1856 B is the 0.1.11 layout (`OtherVersion`); any other length `UNKNOWN_LAYOUT` | C-01 census 165/165 banks decode |
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

## Gate P1A: PASS (2026-11-03, C-01 step 7)

C-01 plan `edf159af0e91fb03` (7 reads, 17.5 MB): DEPLOYMENT, GROUP, BANKS (165/165), ORACLES (163) PASS; BUILD PASS
(the step's own reproducible build on a clean clone of `9fcb729a`); FE01 PASS (plan `1accaed7f43fb811`, 6 reads, 1.87 MB,
scope closed in round 3, freeze `FROZEN`). The runtime starts on this deployment only (`deployment_pin` = `Reference`); any
other ProgramData halts as before. LIVE and submission stay disabled; P8-CTRL switches stay `DISABLED`; a new network audit
(C-04) follows the promotion.
