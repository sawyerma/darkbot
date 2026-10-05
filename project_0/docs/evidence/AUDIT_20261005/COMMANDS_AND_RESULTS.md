# Commands and results

All commands below were run from `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot` unless a command explicitly changes directory. API keys, private key paths, and endpoint query strings are omitted from this ledger. The raw outputs are copied into this bundle.

## Source freeze and inventory

```sh
git status --short --branch
git archive 799048f | tar -x -C /tmp/darkbot-audit-799048f
git ls-files
```

Result: audited `main` at `799048f8fa95df4c29f2aa6b35bcc3615405fe19`; 2,086 tracked files. Exact inventory and SHA-256 values are in `tracked_inventory.tsv` and `manifest.json`.

## Public comparison source

```sh
git clone --depth 1 https://github.com/0dotxyz/eva01.git /tmp/darkbot-audit-references/eva01
git clone --depth 1 https://github.com/mrgnlabs/mary.git /tmp/darkbot-audit-references/mary
git clone --depth 1 https://github.com/0dotxyz/p0-ts-sdk.git /tmp/darkbot-audit-references/p0-ts-sdk
git clone --depth 1 https://github.com/drift-labs/keep-rs.git /tmp/darkbot-audit-references/keep-rs
git clone --depth 1 https://github.com/drift-labs/keeper-bots-v2.git /tmp/darkbot-audit-references/keeper-bots-v2
git clone --depth 1 https://github.com/solendprotocol/liquidator.git /tmp/darkbot-audit-references/solend-liquidator
git clone --depth 1 https://github.com/drift-labs/protocol-v2.git /tmp/darkbot-audit-references/drift-protocol
git clone --depth 1 https://github.com/solendprotocol/public.git /tmp/darkbot-audit-references/solend-public
```

Result: all clones completed. SHAs and remotes are in `manifest.json`. The pinned Marginfi source was read from the local Cargo checkout at tag `mrgn-0.1.11-rc1`, commit `33c67987a661f59c6683a65485ebb4f0ef030b77`.

## Workspace tests

```sh
cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health,txv1
cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health
```

Result: 136/0 and 134/0 passed/failed, respectively. Full Cargo output is in `tests_txv1.log` and `tests_health.log`; compact result lines are `tests_txv1.txt` and `tests_health.txt`.

## Candidate-cache counterexample

```sh
python3 project_0/docs/evidence/AUDIT_20261005/cache_counterexamples.py .
```

Result: the exact checked-in predicate compiled with `rustc`; class transitions and forced re-arm passed, while same-slot repeated class suppression and fixed-order soft-cap starvation were reproduced. Output: `cache_counterexamples.txt`.

## Read-only Drift dispatch probe

```sh
python3 project_0/docs/evidence/AUDIT_20261005/probe_drift_dispatch.py project_0/docs/evidence/AUDIT_20261005/drift_dispatch.json
```

Result: unsigned `simulateTransaction` returned `InstructionFallbackNotFound` / `Custom(101)`, fee 5,000 lamports, units 1,345. This was a simulation only; no signed transaction was sent.

## Historical Mainnet replay

```sh
P0_RPC_URL=https://api.mainnet-beta.solana.com \
  python3 project_0/docs/evidence/AUDIT_20261005/collect_history.py \
  project_0/docs/evidence/AUDIT_20261005
python3 project_0/docs/evidence/AUDIT_20261005/analyze_history.py \
  project_0/docs/evidence/AUDIT_20261005
```

Result: 6 recent successful Marginfi liquidation transactions were retained, including `59Qt6h...`, `38ZXvW...`, `2cebU...`, `3ab7mn...`, `gLVXyf...`, and `C6Jnpz...`. Fees, protocol flat-fee transfers, instruction amounts, timestamps, and token deltas were decoded. Complete realized USD PnL remains UNKNOWN because prestate and reliable route valuation are unavailable.

## Failed auxiliary probe (retained)

```sh
cargo run --quiet --manifest-path /tmp/darkbot-audit-799048f/project_0/native/Cargo.toml \
  -p darkbot-runner --features crate_health,txv1 --bin p0-audit-json -- <two historical transaction JSON files>
```

Result: failed during `serde_json` deserialization (`invalid type: map, expected f64`). The exact stderr is retained in `winner_economics_repro.txt` and `darkbot-audit-json-build.log`. No claims depend on this failed probe.

## Safety boundary

The runner rejects `P0_MODE=live` before startup qualification. No `sendTransaction`, `sendBundle`, keypair signing, ATA creation, or token transfer was performed by this audit.
