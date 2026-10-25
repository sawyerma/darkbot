# BSC Architektur — Datenfluss, Ownership, Schnittstellen

Repository-Architektur nach R9–R13 (lesbare Benennung nach EVA01, siehe [../README.md](../../README.md) §8). Die fachliche Architektur bleiben [BSC_BIRTH_ENTRY_ARCHITECTURE.md](BSC_BIRTH_ENTRY_ARCHITECTURE.md) und [BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md](BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md); bei Abweichung gelten diese. Ältere Verträge nennen die Crates noch mit ihren Namen vor R10 (Zuordnung unten).

## Baum

```text
bsc/
├── README.md · .gitignore           /.local/ (lokale Secrets-Ablage, Backups), /data/ (Laufdaten)
├── engine/                          EINZIGER Rust-Cargo-Workspace (cd bsc/engine)
│   ├── Cargo.toml · Cargo.lock · rust-toolchain.toml (1.93.1)
│   └── crates/
│       ├── trading/                 chain_state, venues/flap, economics, intent, transactions, signer, submission_policy
│       ├── tx_encoding/             rlp
│       ├── geth_ipc/                protocol_v1, protocol_v2/ (Wire V2: layout, checksum, error, mapping, lanes, records,
│       │                            guards, arena, producer, consumer, lifecycle, peer_pool, role_lease), ingress, hot0, outbound, host
│       ├── execution/               startup, submission (club48, blockrazor, local_geth, frame, transport, tls, response, send),
│       │                            acceptance_guard, qualification_capture, bin/g07_mainnet_self_launch_buy/
│       ├── journal/                 format, recorder, replay
│       └── qualification/           probe, measure, evidence, identity, stats, verdict, bin/ (G0.6a, G0.8, u5)
├── geth/                            gepinnter Go-geth-Overlay, Pin, Wire-V1/V2-Spezifikationen (kein Rust-Workspace)
├── native/evidence/                 pfadgebundene Q5A-/C1-Nachweise (siehe EVIDENCE.md §5)
├── config/                          nichtgeheime aktive Verträge (v2-Acceptance, Builder-Floor, Preflight-Baseline, Witness-Schemas, wallet.public.env)
├── tools/                           Prüf-/Betriebswerkzeuge (Tabelle unten)
└── docs/                            PHASENPLAN, STATUS, ARCHITECTURE, EVIDENCE + vollständige verbindliche Verträge
prestudy/bsc/                        historisch, NICHT im Build
```

Lokal und nie versioniert: `bsc/.env`, `bsc/secrets.env`, `bsc/.local/`, `bsc/data/`, `bsc/vendor/bsc-geth`, `engine/target/`.

## Crates (eine Implementierung pro Verantwortung)

| Ordner | Paket / Rust-Name | Verantwortung | Abhängigkeiten | Vor R10 |
|---|---|---|---|---|
| `trading` | `darkbot-bsc-trading` / `darkbot_bsc_trading` | Fachlogik: Chain-State, Venues, Quote (`economics::flap::native_curve20`), Economics, Intent, Transaktionen, Signer, Submission-Policy (`submission_policy`: Capabilities, Release-Profile, unveränderliche Pläne); keine externen Sends | `tx_encoding` | `darkbot-core` (`core/`, Modul `inclusion`, `tx`) |
| `tx_encoding` | `darkbot-bsc-tx-encoding` | RLP-/binäre Kodierungsprimitive | — | `darkbot-wire` (`wire/`) |
| `geth_ipc` | `darkbot-bsc-geth-ipc` | geth↔Rust Shared Memory: `protocol_v1` (Wire V1, Root-Re-Export), `protocol_v2` (Wire V2; Kompatibilitätspfad `v2`), `ingress` (geth-Ingress-Records), `hot0` (geliehene Receive-Slots, C1.5), `outbound` (local-geth-Outbound-Evidence, C1.7), `host` (C1.6 Host-Profil/QoS) | `libc` (Fixtures aus `trading` nur in Tests) | `darkbot-fastlane` (`fastlane/`) |
| `execution` | `darkbot-bsc-execution` | Startup, `submission` (Provider-Framing, TLS/Transport, Response, Versand; keine Handelsentscheidung); Feature `acceptance-guard` (v2-Guard), Feature `g07-mainnet` (Capture-Adapter + G0.7-Binary) | `trading`; optional `journal` | `darkbot-runner` (`runtime/`, Modul `inclusion`) |
| `journal` | `darkbot-bsc-journal` | Journalformat, bounded Recorder, Replay | — | `darkbot-capture` (`capture/`) |
| `qualification` | `darkbot-bsc-qualification` | read-only Probes, Messungen, Evidenz, Verdikt; Binaries `u5_tls_qualify`, `g06a_native_bnb_quote`, `g06a_self_launch_bundle`, `g08_mainnet_verify`, `g08_producer_timing` | `execution[acceptance-guard]`, `trading`, `journal` | `darkbot-qualify` (`qualify/`) |

Regeln:

- Produktionsbuilds von `execution` enthalten weder Guard noch G0.7 (nur über Features). G0.7 ist vom regulären Modus getrennt; sein Binary gliedert sich in `main.rs` (kontrollierter Ablauf und Guard-Reihenfolge) und `inputs`, `transactions`, `chain_reads`, `builder_submission`. Die G0.7-eigene Legacy-Signatur und Builder-Übertragung sind eingefrorener Qualifikationscode (byte-identisch zur DRY), keine zweite Produktions-Engine.
- Quote-Arithmetik hat genau einen Eigentümer (`trading`); das DRY-Quote-Binary ist byte-identisch zu den Gate-0-Ausgaben.
- Die unabhängige EVM-Gegenprüfung bleibt eine separate Implementierung (`tools/g06a_*.cjs`, npm-Locks in `tools/evm-audit-tools/`, `tools/abi-tools/`; `npm ci`).
- Unveränderte Identitäten: `TLS_TRANSPORT_IMPLEMENTATION = "darkbot-runner/tls-v1 …"` und `Q5A_TLS_EVIDENCE_PATH = "bsc/native/evidence/…"` sind Q5A-Qualifikationsidentitäten und tragen bewusst die alten Namen.
- **Wire V2 ist ein einziges Rust-Modul** (`protocol_v2/mod.rs` setzt die Fachdateien per `include!` zusammen). Grund: Echte Untermodule landen in eigenen Codegen-Einheiten und änderten nachweislich die Release-Inline-Entscheidungen im Hot-Path (z. B. `RecordGuardV2::release`). Mit `include!` ist der Release-Maschinencode aller 98 Crate-Funktionen in den C1-Binaries identisch zur Einzeldatei (siehe EVIDENCE.md §7). Eine Aufteilung in echte Module ist eine spätere, latenz-qualifizierte Entscheidung.
- C1.5–C1.8 (`hot0`, `outbound`, `host`, `BorrowedRecordV2`/`RecordGuardV2`/`LaneSetV2`) sind integriert, kompiliert und regressionsgeprüft — **nicht phasenqualifiziert** (C1-Abnahme gemäß Roadmap/Runbook steht aus).
- `host` bleibt in `geth_ipc`: es nutzt `unsafe`-FFI (`sysctlbyname`, pthread-QoS) und `crc32c` aus Wire V2; `execution` und `qualification` verbieten `unsafe` (`#![forbid(unsafe_code)]`). Option für später: nur den Report-Teil nach `qualification` verlegen.

## Werkzeuge (`bsc/tools/`)

Namen sind stabile CLI (Runbooks, Testplan und Nachweise rufen sie so auf) und bleiben daher; Schema `phase_action_object` (`g06a_final_dry`, `c19_native_latency`).

| Werkzeug | Funktion | Netz |
|---|---|---|
| `g06a_preflight_readonly.cjs` | Mainnet-Preflight (Deployment, Konfiguration, Wallets) | read-only RPC |
| `g06a_economics_evm_crosscheck.cjs`, `g06a_local_simulate.cjs` | unabhängige EVM-Gegenprüfung der Quote / exakte lokale Simulation | read-only RPC-State |
| `g06a_final_dry.sh` | vollständiger G0.6a-DRY (DRY1–DRY4, Null-Egress) → `bsc/data/g06a_final_dry/` | read-only |
| `g07_pre_live_readiness.sh`, `g07_min_spend_probe.sh`, `g07_find_min_executable_buy.sh` | G0.7-Bereitschaft, Spend-Kandidaten, minimale ausführbare BUY-Suche | read-only, kein Send |
| `c1_apply_geth_overlay.py`, `c1_verify_geth_pin.sh` | Overlay auf gepinntes bsc-geth anwenden / Pin prüfen | — |
| `c13_lifecycle_interop.sh`, `c14_ingress_interop.sh` | Go↔Rust-Interop V2-Lifecycle / Ingress | lokal |
| `c14a_early_latency.sh` + `c14a_analyze.py`, `c19_native_latency.sh` (Auswertung: nativer `c19_report`), `c19_prepare_production.py`; C1.9b: nativer `c19b_report` | C1-Latenzmessungen und Auswertung (DBREC/DBTAB, kein JSON) | lokal |
| nativer `c1_freeze` (DBREC-Inputs per CLI), `c1_freeze_geth_patch_evidence.py`, `c1_c1_1_v1_baseline.py` | C1-Freeze-Records und -Nachweise | — |
| `q5a_check_evidence.py` | Q5A-TLS-Evidence prüfen | — |
| `lock_qualify.sh` + `verify_lock_member.py` | Lockfile-Mitgliedsprüfung | offline |

## Aufrufkonvention

- Cargo immer aus `bsc/engine` (dort greift `rust-toolchain.toml`): `cargo check --locked --workspace`.
- G0.6a/G0.7/G0.8-Binaries lösen Vertragspfade **relativ zum Repository-Root** auf; `bsc/tools/*.sh` bauen in `bsc/engine` und starten aus dem Repository-Root (`engine_run`).
- G0.7 bauen: `cd bsc/engine && cargo build --locked -p darkbot-bsc-execution --features g07-mainnet --bin g07_mainnet_self_launch_buy` — Ausführung nur nach expliziter Freigabe gemäß Testplan.
