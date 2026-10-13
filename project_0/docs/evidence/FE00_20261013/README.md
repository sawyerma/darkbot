# FE-00 / FE-01 selection, base 28f6d53 — 2026-10-13

Source trust correction implemented: raw source labels cannot authorize chain evidence. Unknown / legacy full_slot_reconciliation input is UNVERIFIED, never implicitly index/strict. Verified sources require result digest, immutable spec digest, capture/evidence/window/mode binding; strict requires complete spec-bound batch proof and all hashed evidence files. Cached views revalidate these bindings. Historical data is not a current execution authorization.

Offline candidate report reuses existing audit decoder on six archived real transaction fixtures and R122/R124 summaries. It does not scan/replay the five-hour run. Signatures, slots, actual fees, source hashes, missing evidence and archived technical/negative economics are in candidates.json. R122 has two technical passes; all three R124 known RPC edges are negative (-121889 / -118606 / -91691 USD-micro). NO_ECONOMIC_GO. ZERO_PREFUND selected only for CURRENT REQUALIFICATION, not execution: existing atomic exit/floor and prior full simulation. NO_SWAP lacks qualified exit and current inventory. JIT ExactIn remains required if neither initial path qualifies; unsupported ExactOut plus missing funding bank documented, not hidden.

55 Python PASS; workspace txv1 340 PASS/0 FAIL/1 ignored; health 338 PASS/0 FAIL/1 ignored; both full release builds PASS. Clippy baseline executed successfully with warnings; no compiled Rust changes in this package, hence Rust diagnostic delta zero against identical compiled source. Existing ignored F8 test and provider/Mainnet gates remain open. Python compile, shell syntax and git whitespace checks PASS.

FE-00 candidate report delivered; economic GO NOT met. FE-01/current deployment and FE-02/current full simulation NOT_RUN awaiting explicit bounded read-only permission. No automatic LIVE, transfer, 5h replay or new capture. Independent FE-04 offline journal work follows.

Commands from repository root:
```
python3 project_0/scripts/fe_candidate_report.py --output project_0/docs/evidence/FE00_20261013/candidates.json
python3 -m unittest discover -s project_0/scripts -p 'test_*.py' -v
cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health,txv1 --locked --offline
cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health --locked --offline
cargo build --manifest-path project_0/native/Cargo.toml --workspace --features crate_health,txv1 --release --locked --offline
cargo build --manifest-path project_0/native/Cargo.toml --workspace --features crate_health --release --locked --offline
cargo clippy --manifest-path project_0/native/Cargo.toml --workspace --all-targets --features crate_health,txv1 --locked --offline --message-format=json
```

User-owned existing 5h offline replay command remains in `../C0E1_P6_20261013/README.md`; not executed here.
