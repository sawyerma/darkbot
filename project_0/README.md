# Project 0

Solana Project 0 liquidation system.

```text
project_0/
├── native/       Rust production workspace
├── src/          Project-0 application/control boundary
├── gates/
├── tests/
├── docs/
├── config/
├── scripts/
├── tools/
├── data/
└── exports/
```

Authoritative plan: `docs/PHASENPLAN.md`.

Rust build:

```bash
cd project_0/native
cargo build --features crate_health
cargo build --features "crate_health txv1"
```

Project-0 frontend views live in `../frontend/src/features/project0/`; the browser never duplicates protocol health/economics/execution logic.
