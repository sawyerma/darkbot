# P0-03 — Reuse matrix (P0.2 fields)

The decisions are the existing ones in `docs/architecture/p0_reuse_matrix.md` (sessions 4/5); this table only adds the
P0.2 fields (version, license, required upgrade, performance concern, LIVE criticality). No decision is changed.
Licenses: `LICENSE_MATRIX.md`. Remote routing and API costs: `README.md`.

| Component | Upstream | Version | License | Purpose | Reuse decision | Required upgrade | Performance concern | LIVE critical |
|---|---|---|---|---|---|---|---|---|
| `marginfi-type-crate` | `0dotxyz/marginfi-v2` | tag `mrgn-0.1.11-rc1` (`33c6798`), crate 0.1.10 | Apache-2.0 | types, PDAs, discriminators, layouts | DIRECT_DEPENDENCY | 0.1.12 (tagging/premium, bank 3904 B) via P1B | none (zero-copy types) | yes |
| `marginfi` program crate | same | 0.1.11 | Apache-2.0 | Health (`get_health_components`, `HealthPriceMode::Client`), receivership instruction/account structs | DIRECT_DEPENDENCY | 0.1.12 via P1B; build with `staging` feature only for staging runs (`STAGING_MODEL.md`) | Health builds `AccountInfo` per evaluation; measured in P3/P6 | yes |
| Kamino/Drift/JupLend mocks (`kamino-mocks`, `drift-mocks`, `juplend-mocks`, …) | same | 0.1.0 | Apache-2.0 | integration account layouts and refresh/withdraw instructions | DIRECT_DEPENDENCY | follows marginfi pin | none | yes |
| `anchor-lang` | crates.io | 1.2.0 | Apache-2.0 | account (de)serialization, discriminators | DIRECT_DEPENDENCY (through marginfi) | follows marginfi pin | none | yes |
| Solana crates (`solana-sdk` 3.0.0, `solana-client` 3.1.12, `solana-message` 3.0.1/4.4.1, …) | crates.io | as pinned | Apache-2.0 | keys, hashes, transactions, RPC types | DIRECT_DEPENDENCY | none in P0 | none | yes |
| `pyth-solana-receiver-sdk`, `pythnet-sdk` | `0dotxyz/pyth-crosschain` rev `f170d205` | 1.2.1 / 2.3.2 | Apache-2.0 | `PriceUpdateV2` decode inside marginfi price adapters | transitive DIRECT_DEPENDENCY | follows marginfi pin | none | yes (price path) |
| `switchboard-on-demand` | `switchboard-xyz/switchboard-sdk` rev `f75b1ef6` | 0.12.1 | MIT | Switchboard pull feed decode inside marginfi | transitive DIRECT_DEPENDENCY | follows marginfi pin | none | yes (price path) |
| eva01 `liquidator.rs`, `cache/*`, `marginfi_ixs.rs`, `inventory.rs`, `wrappers/*` | `0dotxyz/eva01` | `main` as read in session 4 | no LICENSE file → read only | reference for health call, account order, inventory | LERNEN / no code copied (KEEP_DARKBOT implementation) | none | none | no |
| eva01 `geyser.rs` | same | same | read only | Geyser ingest reference | REUSE_ADAPT later (S1-RACE) | none | S1-RACE latency | no (S1-LT is KEEP_DARKBOT) |
| eva01 `utils/pyth_cranker.rs` | same | same | read only | optional pre-TX Pyth crank | REUSE_ADAPT optional, only after measured staleness (`ORACLE_MODEL.md`) | none | adds a pre-TX step | no |
| eva01 `utils/jito.rs` | same | same | read only | Jito bundle/tip handling reference | LERNEN → S3-04 | none | none | later (S3) |
| eva01 `rebalancer.rs` | same | same | read only | inventory recycling | REUSE_ADAPT after P9 | none | none | no |
| eva01 cache/Geyser state layer, patched integration bytes | same | same | read only | — | DROP | — | unbounded channel (reason for DROP) | no |
| `solana-dex-superagg` | `0dotxyz/solana-dex-superagg` | not a dependency | see `README.md` | remote aggregator client (Jupiter/Titan/DFlow) | REMOTE_AGGREGATOR_CLIENT, not linked; DarkBot keeps its own Jupiter Swap API v2 client (`runtime/src/execution/route.rs`) | none | remote HTTP round trip on the route path (P5-04/05) | only for swap funding classes |
| Yellowstone gRPC | `rpcpool/yellowstone-grpc` | not a dependency | see `README.md` | Geyser stream for S1-RACE | later (S1-RACE), not P0 | — | — | no (S1-LT uses WSS/RPC) |
| DarkBot S0/S1-LT/S2/index/schedule/economics/planner/prepared/runner/safety | this repository | workspace | `UNLICENSED` (own code) | foundation, state, economics, execution | KEEP_DARKBOT | — | measured in their phases | yes |

## Gate check "no wheel reinvented"

- Health, account layouts, discriminators, receivership instructions and integration layouts come from the protocol crates;
  DarkBot has no own health or layout formula (Regel 3).
- What DarkBot keeps (state core, economics, scheduler, planner) has no upstream equivalent with the required contracts
  (fail-closed canonical state, deterministic economics, receivership validation); the decisions stand as recorded in
  session 5.
