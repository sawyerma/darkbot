# P0-07 — Staging model

Source: `0dotxyz/marginfi-v2` tag `mrgn-0.1.11-rc1` (`33c6798`): `id-crate/src/lib.rs`, `programs/marginfi/Cargo.toml`,
`guides/ADMIN/DEPLOY_GUIDE_STAGING.md`, `guides/ADMIN/DEPLOY_GUIDE.md`, `guides/DEVELOPERS_INTEGRATORS/GETTING_STARTED_INTEGRATOR.md`.
Read locally on 2026-10-16.

## Program IDs

| Feature | Program ID |
|---|---|
| `mainnet-beta` | `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` |
| `staging` | `stag8sTKds2h4KzjUw3zKTsxbqvT4XKHdaR9X9E6Rct` |
| `stagingalt` | `5UDghkpgW1HfYSrmEj2iAApHShqU44H6PKTAar9LL9bY` |
| `devnet` | `neetcne3Ctrrud7vLdt2ypMm21gZHGN2mCmqWaMVcBQ` |
| default (local tests) | `2jGhuVUuy3umdzByFx8sNWUAaf5vaeuDm78RDPEnhrMr` |

## What "network-staging" is

- Staging is the **same program on the Mainnet cluster with a different key** ("Staging is different from mainnet only in
  that it uses a different key", `DEPLOY_GUIDE_STAGING.md:3`). It is built with `--features staging` and gets no verified
  build (`DEPLOY_GUIDE.md:192`).
- When a release is pending ("pre-release" for at least seven days), staging is updated to it, typically on the day the
  pre-release is published (`GETTING_STARTED_INTEGRATOR.md:21-23`). Staging therefore runs the *next* program version
  before Mainnet does — the place to check 0.1.12 compatibility (P1B) before it reaches Mainnet.
- Test groups and banks are listed in `mrgnlabs/mrgn-ts-scripts/scripts/accounts_ref.md`; alternatively an own group can be
  created and Mainnet banks copied into it with `lending_pool_clone_bank`, which only works on the staging program
  (`GETTING_STARTED_INTEGRATOR.md:24-27`, `programs/marginfi/src/lib.rs:93`).
- Oracles, Kamino/Drift/JupLend and swap programs are the real Mainnet programs and accounts (same cluster).
- The deploy log in this checkout lists staging versions up to 0.1.9 and Mainnet up to 0.1.9 (`DEPLOY_GUIDE.md:194-221`);
  the version deployed on staging *today* must be read from chain (P1A/P1B), not from this list.

## Which P5/P8 tests are executable there

| Test | Executable on staging? | Precondition |
|---|---|---|
| Build + simulate the full receivership transaction (P5, FE-02) against a real unhealthy staging account | yes, read-only simulation; no value moves | read-only Mainnet RPC access (separate permission) and an unhealthy account in a staging group |
| Same against an own group with cloned Mainnet banks, made unhealthy by the group admin (risk parameters) | yes | own group creation, deposits/borrows and rent: value-bearing transactions on Mainnet → separate explicit authorization |
| End-to-end receivership with real landing, finalized reconciliation, holds, exit (P8, FE-06 style) | yes, with small own funds | separate LIVE authorization; submission stays disabled in code until then |
| 0.1.12 tagging/premium behaviour (P1B, T) | yes once staging runs the 0.1.12 pre-release | chain read of the deployed staging ProgramData |
| Anything on `devnet` | not representative: no Mainnet oracles, integrations or liquidity | — |

What DarkBot must change to run there (recorded here, implemented with the P5/P8 staging tests, not in P0):

- The receivership builders already take the program as a parameter (`liquidation/src/receivership.rs`, `program_id: *program`),
  and the Shadow runtime reads `P0_PROGRAM_ID`.
- Mainnet-only today: the receivership allowlist constant (`liquidation/src/protocol.rs:33`, `PROGRAM_ID_MAINNET`), the FE-01
  observation (`orchestration/fe_qualification.rs:258`) and the protocol crates, which are compiled with the `mainnet-beta`
  feature (`liquidation/Cargo.toml:29-30`; the crate's `declare_id!` follows the feature). A staging run therefore needs a
  staging build of the protocol crates (`staging` feature) and the configured program in the allowlist, not only a different
  environment variable.

No staging run has been performed; every staging execution above needs the corresponding separate permission (read-only
Mainnet access, or value-bearing transactions).
