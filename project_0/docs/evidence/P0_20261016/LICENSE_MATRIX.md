# P0-04 — License matrix

Retrieved 2026-10-16. Rust inventory reproducible with
`cargo metadata --locked --offline --format-version 1 --features crate_health,txv1` in `project_0/native` (673 packages:
4 DarkBot workspace crates, 669 third-party packages). No JSON copy of the inventory is kept (JFI-01/JFI-03).

## Reuse components

| Component | Upstream / version | License | Basis | Use |
|---|---|---|---|---|
| `marginfi`, `marginfi-type-crate`, `id-crate`, `kamino-mocks`, `drift-mocks`, `juplend-mocks` (+ `scope-`, `solend-`, `marinade-`, `exponent-mocks` transitively) | `0dotxyz/marginfi-v2` tag `mrgn-0.1.11-rc1` (`33c6798`), crates 0.1.11 / 0.1.10 / 0.1.0 | Apache-2.0 | repository `LICENSE` (Apache License 2.0); the crate manifests carry no `license` field | DIRECT_DEPENDENCY |
| `anchor-lang` | 1.2.0 (crates.io) | Apache-2.0 | manifest | DIRECT_DEPENDENCY (through marginfi) |
| Solana crates (`solana-sdk` 3.0.0, `solana-client` 3.1.12, `solana-message` 3.0.1 and 4.4.1, `solana-transaction-status`, `solana-account-decoder`, …) | crates.io | Apache-2.0 | manifests | DIRECT_DEPENDENCY |
| `spl-associated-token-account` 8.0.0 | crates.io | Apache-2.0 | manifest | DIRECT_DEPENDENCY |
| `pyth-solana-receiver-sdk` 1.2.1, `pythnet-sdk` 2.3.2 | `0dotxyz/pyth-crosschain` rev `f170d205` (through marginfi) | Apache-2.0 | manifests | transitive (protocol price adapters) |
| `switchboard-on-demand` 0.12.1, `switchboard-protos` 0.2.6 | `switchboard-xyz/switchboard-sdk` rev `f75b1ef6` / crates.io (through marginfi) | MIT | manifests | transitive (protocol price adapters) |
| `fixed` 1.28.0 | crates.io | MIT/Apache-2.0 | manifest | DIRECT_DEPENDENCY (I80F48) |
| `tokio` 1.53.2, `tokio-tungstenite` 0.24.0, `reqwest` 0.12.28, `rayon` 1.12.0, `serde`, `bincode`, `bytemuck`, `base64`, `bs58`, `time`, `futures-util`, `async-trait` | crates.io | MIT, MIT OR Apache-2.0, Zlib OR Apache-2.0 OR MIT | manifests | DIRECT_DEPENDENCY |
| `serde_json` 1.0.151 | crates.io | MIT OR Apache-2.0 | manifest | DIRECT_DEPENDENCY; under JFI-02 only allowed at the external Solana JSON-RPC/WSS wire |
| DarkBot crates (`darkbot-common`, `-solana`, `-project0`, `-runner`) | this repository | `UNLICENSED` (proprietary, not published) | manifests | own code |

## All third-party packages by license expression

| Packages | License expression |
|---|---|
| 239 | MIT OR Apache-2.0 |
| 190 | Apache-2.0 |
| 81 | MIT |
| 38 | MIT/Apache-2.0 |
| 36 | Apache-2.0 OR MIT |
| 18 | Unicode-3.0 |
| 10 | Apache-2.0 (marginfi-v2 repository LICENSE) |
| 8 | BSD-3-Clause |
| 5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| 4 | Unlicense OR MIT |
| 3 each | Zlib OR Apache-2.0 OR MIT; Apache-2.0 OR ISC OR MIT; CDLA-Permissive-2.0 |
| 2 each | BSD-2-Clause; Apache-2.0 / MIT; Zlib; BSD-3-Clause OR MIT OR Apache-2.0; MIT OR Apache-2.0 OR LGPL-2.1-or-later (`r-efi`); ISC; Unlicense/MIT; BSD-2-Clause OR Apache-2.0 OR MIT |
| 1 each | 0BSD OR MIT OR Apache-2.0; CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception; BSD-3-Clause AND MIT (`brotli`); BSD-3-Clause/MIT; Apache-2.0/MIT; Zlib OR MIT OR Apache-2.0; CC0-1.0 OR MIT-0 OR Apache-2.0; MIT OR Apache-2.0 OR BSD-1-Clause; MIT OR Apache-2.0 OR Zlib; MIT OR Zlib OR Apache-2.0; Apache-2.0 AND ISC (`ring`); Apache-2.0 OR BSL-1.0; Apache-2.0 (`solana-config-interface`, license file); (MIT OR Apache-2.0) AND Unicode-3.0; MIT / Apache-2.0 |

Verdict for the Rust build: every third-party package is under a permissive license or offers one; LGPL appears only as
an alternative in an `OR` expression (`r-efi`, MIT/Apache-2.0 chosen). No copyleft-only, no unknown license. Attribution
duties (Apache-2.0 NOTICE, MIT/BSD notices) apply if binaries are ever distributed; DarkBot is not distributed.

## References outside the Rust build

| Reference | License | Use |
|---|---|---|
| eva01 (`0dotxyz/eva01`) | no LICENSE file | read only, no code copied |
| `solana-dex-superagg` | MIT | not linked (`README.md`) |
| `jupiter-swap-api-client` (superagg's fork) | Apache-2.0 per crate manifest | not linked |
| `yellowstone-grpc-client` / `-proto` | Apache-2.0 (server plugin AGPL-3.0, not used) | S1-RACE only |
| `jito-sdk-rust` | MIT (crates.io) / Apache-2.0 (repo) | not linked; the crate named `jito-sdk` is GPL-3.0 and must not be used |
| Jupiter, Titan, DFlow, Hermes, Jito, Helius APIs | provider terms of service, not code licenses | `README.md` |
