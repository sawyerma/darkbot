# DarkBot wallet security

DarkBot uses a different hot wallet for every trading domain. Personal Phantom/MetaMask wallets are treasury wallets only and are never used as automated bot signers.

## Wallet ownership

| Domain | Chain | Public bot wallet | Private-key source |
|---|---|---|---|
| Project_0 | Solana Mainnet | `GqRxBgvH94QKFJQVyDv82fK6jxaKrcyjyjSPkN7ue3zd` | local `P0_KEYPAIR_PATH` |
| Pump.fun | Solana Mainnet | UNPROVISIONED | local `PUMPFUN_KEYPAIR_PATH` |
| BSC | BSC Mainnet | UNPROVISIONED | local `BSC_PRIVATE_KEY_PATH` |

## Mandatory rules

1. Never reuse a private key between Project_0, Pump.fun and BSC.
2. Never use a testnet key on Mainnet.
3. Private keys/keypair JSON files live outside this repository, preferably below `~/.config/darkbot/<domain>/`.
4. Secret directories are owner-only (`chmod 700`); secret files are owner-only (`chmod 600`).
5. Only public wallet addresses may be committed to Git.
6. Never commit or paste a seed phrase, mnemonic, Solana 64-byte keypair JSON, or EVM private key.
7. Each runtime must derive its own public address from its signer and fail closed if it does not match the project's pinned public address.
8. Treasury sweeping is a separate feature: profits may later be swept to an allowlisted Phantom/treasury address while a configured operating reserve remains on the bot wallet. It is not enabled implicitly.
9. Keep only the minimum operating capital needed on each hot wallet.

## Project layout

Public wallet identity belongs inside the owning project:

- `project_0/config/wallet.public.env`
- `solana/config/wallet.public.env` (Pump.fun)
- `bsc/config/wallet.public.env`

Private material never belongs in those files or anywhere else in Git.
