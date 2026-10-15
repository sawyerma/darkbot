# BSC Architektur — Datenfluss, Ownership, Schnittstellen

Einstieg in die Repository-Architektur nach R1–R8. Die fachliche Architektur bleiben [BSC_BIRTH_ENTRY_ARCHITECTURE.md](BSC_BIRTH_ENTRY_ARCHITECTURE.md) und [BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md](BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md); bei Abweichung gelten diese.

## Baum

```text
bsc/
├── README.md            Masterplan R1–R8 und Einstieg
├── .gitignore           /.local/ (Secrets-Ablage, Backups), /data/ (Laufdaten)
├── native/              EINZIGER Cargo-Workspace (cd bsc/native)
│   ├── Cargo.toml · Cargo.lock · rust-toolchain.toml (1.93.1)
│   ├── crates/{core,wire,fastlane,runtime,capture,qualify}
│   ├── geth/            gepinnter bsc-geth: README, Pin, Wire-Spezifikationen, overlay/
│   └── evidence/        pfadgebundene C1-/Q5A-Nachweise (siehe EVIDENCE.md)
├── config/              nichtgeheime aktive Verträge (v2-Acceptance, Builder-Floor, Preflight-Baseline, Witness-Schemas, wallet.public.env)
├── tools/               Shell/Node/Python-Helfer (G0.6a/G0.7, EVM-Crosscheck mit npm-Locks, C1-Interop/-Freeze)
└── docs/                PHASENPLAN, STATUS, ARCHITECTURE, EVIDENCE + vollständige verbindliche Verträge
prestudy/bsc/            historisch, NICHT im Build (experiments/, references/, evidence/)
```

Lokal und nie versioniert: `bsc/.env`, `bsc/secrets.env`, `bsc/.local/`, `bsc/data/`, `bsc/vendor/bsc-geth`, `native/target/`.

## Crate-Verantwortung (eine Implementierung pro Semantik)

| Ordner | Paket | Verantwortung | Abhängigkeiten |
|---|---|---|---|
| `wire` | `darkbot-wire` | RLP / binäre TX-Verträge | — |
| `core` | `darkbot-core` | State, Venues, Quote (`economics::flap::native_curve20`), Economics, Intent, Signer/TX | `wire` |
| `fastlane` | `darkbot-fastlane` | geth↔Rust mmap/SPSC (Wire V1/V2, Ingress) | `core` (Fixtures nur in Tests) |
| `runtime` | `darkbot-runner` | Start, Transport/TLS, Dispatch; Feature `acceptance-guard` (v2-Guard), Feature `g07-mainnet` (Capture-Adapter + G0.7-Binary) | `core`; optional `capture` |
| `capture` | `darkbot-capture` | Journal, bounded Recorder, Replay | — |
| `qualify` | `darkbot-qualify` | read-only Qualifikation: `u5_tls_qualify`, G0.6a-DRY (`g06a_native_bnb_quote`, `g06a_self_launch_bundle`), G0.8 (`g08_mainnet_verify`, `g08_producer_timing`) | `runtime[acceptance-guard]`, `core`, `capture` |

Regeln:

- Die Produktionsbuilds von `runtime` enthalten weder Guard noch G0.7: beide nur über Features. G0.7 ist strikt getrennt vom regulären Runtime-Modus.
- Quote-Arithmetik hat genau einen Eigentümer (`core`); das DRY-Quote-Binary ist byte-identisch zu den früheren Gate-0-Ausgaben (7/7 gespeicherte DRY-Quotes).
- Die unabhängige EVM-Gegenprüfung (`tools/g06a_economics_evm_crosscheck.cjs`, `g06a_local_simulate.cjs`) bleibt eine **separate** Implementierung (npm-Locks in `tools/evm-audit-tools/`, `tools/abi-tools/`; Installation `npm ci`).

## Aufrufkonvention

- Cargo immer aus `bsc/native` (dort greift `rust-toolchain.toml`): `cargo check --locked --workspace`.
- G0.6a/G0.7/G0.8-Binaries lösen Vertragspfade **relativ zum Repository-Root** auf. Die Skripte in `bsc/tools/` bauen daher in `bsc/native` und starten das Binary aus dem Repository-Root (`native_run`).
- G0.7 bauen: `cd bsc/native && cargo build --locked -p darkbot-runner --features g07-mainnet --bin g07_mainnet_self_launch_buy` — Ausführung nur nach expliziter Freigabe gemäß Testplan.
