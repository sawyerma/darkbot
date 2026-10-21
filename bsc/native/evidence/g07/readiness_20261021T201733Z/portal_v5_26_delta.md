# Flap Portal v5.25.0 → v5.26.0 — read-only delta (2026-10-21)

Derived view; the authoritative record of this directory is `current_rpc.readiness.dbrec`.
Method: public endpoints only (`bsc-dataseed.bnbchain.org` for current state, receipts and code;
`bsc-mainnet.public.blastapi.io` for archive `eth_getStorageAt`). No key, no signature, no send.

## Readiness (current RPC)

`PRE_LIVE_READINESS=BLOCKED`, exit 2. `CURRENT_DEPLOYMENT_DRIFT=STOP` with exactly three drift fields:

- `portal_version`: v5.25.0 → v5.26.0
- `portal_implementation`: `0xe13227370996f892a56d2ff4a30f6026887b48fb` → `0x7a94f37b61a82c395f9812cf266b444783cc5e8c`
- `portal_implementation_code_sha256`: `0x42c91172…de29` → `0x68aeff3e…9391`

No other compared field drifted (quote configuration, fees, tax flag, proxy code, wallets, nonces, funding as checked by the readiness preflight). `BUILDER_FRESHNESS=BLOCKED` (sources older than 24 h) is independent of the upgrade.

## Upgrade

- Binary search of the EIP-1967 slot on the archive endpoint: first block with the v5.26.0 implementation **128914179** (`0x025425840e296cff96ca378705da73882429d0b8cff3276c81bec26494e07616`, 2026-10-21T09:29:13Z); block 128914178 still `0xe132…48fb`.
- `Upgraded(address)` log of the Portal in that block: transaction `0x04791db127459ee67718e1ce7610fc87c90483a94411b8c143d97db1bb8cd702`, from `0x705163cf642a975d2f0570463be93aab7993d1c2`, to `0x1f96bc88f0794060433be5f3ec9159a9c4f08a3b`.
- This is the second upgrade after the frozen DRY head 127865760 (the first: v5.24.0 → v5.25.0 at 128103397). Not an R0 scan: header/receipt coverage between the two was not re-verified here.

## Implementation bytecode

- Both 20 347 bytes; 10 356 instructions each; **opcode stream identical** (0 opcode mismatches).
- Only PUSH32 arguments differ: the version string and nine embedded helper addresses (immutables). Selector set unchanged; all six G0.7 functions present (`newTokenV6 8cb5772c`, `swapExactInput ef7ec2e7`, `getQuoteTokenConfiguration 26ef20d5`, `getFeeRate 84e5eed0`, `enableTaxOnBondingCurve 1d31e153`, `version 54fd4d50`).

| Helper v5.25 → v5.26 | Code | References TAX_V3 `0x024f1829…6422` |
|---|---|---|
| `0x60ca21fb…` → `0x3c71df0b…` | same opcodes, 2 push args differ | no |
| `0x6a7be034…` → `0x4f9546a6…` | **different opcodes** (17 456 → 15 608 bytes) | **yes (both)** |
| `0xacc4ff19…` → `0x8240fe9c…` | identical | no |
| `0xdf5f65b2…` → `0xa518065e…` | identical | no |
| `0x6112331c…` → `0xfe55fda1…` | identical | no |
| `0xf4930456…` → `0xec909a58…` | identical | no |
| `0xc871f07a…` → `0x34f6ff6b…` | same opcodes, 1 push arg differs | no |
| `0x2cc2f4ee…` → `0xa626a8cb…` | same opcodes, 1 push arg differs | no |
| `0x59315ebd…` → `0x5064a124…` | same opcodes, 15 push args differ | no |

## Consequence and non-claims

- Under the scope rule the requalified DRY of 2026-10-20 (v5.25.0) and its signed bytes are **not valid** for current Mainnet: G0.7 attempt 1 cannot be armed on it.
- The helper that references the TAX_V3 implementation changed its code, so a change of the launch path cannot be excluded from this analysis. Whether the frozen launch/BUY bytes still execute with the same token, output and gas is **not established** here; that is DRY-2 of a requalification against v5.26.0.
- Nothing here is DRY, R0 or acquisition evidence.
