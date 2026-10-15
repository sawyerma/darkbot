# BSC Evidence — Ablageorte, Originalpfade und Migrationsnachweis R1–R8

Historische Nachweise wurden **nicht inhaltlich bearbeitet**. Referenzen in eingefrorenen Dokumenten und Evidence-Dateien (z. B. [ACCEPTANCE_MAP.json](ACCEPTANCE_MAP.json), das Ledger, `native/crates/core/fixtures/flap/MANIFEST.json`, die „Selected raw/code witnesses“ im Masterplan) nennen bewusst weiterhin die **Originalpfade**; die folgende Tabelle bildet sie auf die heutigen Orte ab.

Ausgangscommit aller Umzüge: `7f3c08b` (HEAD vor R1). Jede Datei ist im Git-Verlauf unter ihrem Originalpfad bei diesem Commit auffindbar.

## 1. Umzugstabelle (Originalpfad → heutiger Ort)

| Originalpfad (bei `7f3c08b`) | Klasse | Heutiger Ort | Bindung / Prüfung |
|---|---|---|---|
| `bsc/gates/gate0/evidence/g01_mainnet_acceptance_proposal_v2.json` | ACTIVE | `bsc/config/g01_mainnet_acceptance_proposal_v2.json` | bytegleich, SHA-256 `026b534f4a2e6e76b6eb816a8de84ef71b508aa28b6e24c5c471e9ee6f0b19ff`; `V2_RELATIVE_PATH` umgestellt; Tests `runtime/tests/acceptance_guard.rs` |
| `bsc/gates/gate0/evidence/g06a_builder_floor_addendum_2026-10-11.json` | ACTIVE | `bsc/config/…` | bytegleich, `ca76a93c…bcc87` |
| `bsc/gates/gate0/evidence/g06a_preflight_readonly.json` | ACTIVE | `bsc/config/…` | bytegleich, `ae1ecd5b…61292` (Default-Eingabe der Quote/Preflight/Crosscheck-Tools) |
| `bsc/gates/gate0/spec/producer_execution_witness_v1.schema.json` | ACTIVE | `bsc/config/…` | bytegleich, `8cd9bcac…ee6f` |
| `bsc/gates/gate0/spec/producer_witness_source_v1.schema.json` | ACTIVE | `bsc/config/…` | **eine Zeile geändert**: `required_witness_schema.const` → `bsc/config/producer_execution_witness_v1.schema.json` (gekoppelt an `PRODUCER_WITNESS_SCHEMA_PATH` in G0.7). SHA-256 alt `356583c9…51ac7` → neu `dd29bb95…24e34`; der alte Hash war nirgends gepinnt; es existiert noch keine Attempt-Instanz |
| `bsc/gates/gate0/src/acceptance_guard.rs` | ACTIVE | `bsc/native/crates/runtime/src/acceptance_guard.rs` (Feature `acceptance-guard`) | nur `V2_RELATIVE_PATH` geändert; `FROZEN_APPROVED_V2_SHA256 = "UNARMED"` unverändert; Shell-Guards prüfen die identische Zeile |
| `bsc/gates/gate0/src/qualification_capture.rs` | ACTIVE | `bsc/native/crates/runtime/src/qualification_capture.rs` (Feature `g07-mainnet`) | `#[path]`-Import durch echte `darkbot-capture`-Abhängigkeit ersetzt; Logik unverändert |
| `bsc/gates/gate0/src/bin/g07_mainnet_self_launch_buy.rs` | ACTIVE | `bsc/native/crates/runtime/src/bin/` (`required-features = g07-mainnet`) | Imports → `darkbot_runner`; Schema-/Tool-Pfade umgestellt; Signier-/Kauflogik unverändert |
| `bsc/gates/gate0/src/bin/g08_mainnet_verify.rs`, `g08_producer_timing.rs` | ACTIVE | `bsc/native/crates/qualify/src/bin/` | Imports → `darkbot_capture` / Runtime-Guard; Logik unverändert; neue synthetische Tests |
| `bsc/gates/gate0/src/bin/g06a_self_launch_bundle.rs` | ACTIVE | `bsc/native/crates/qualify/src/bin/` | doppelte v2-Pfadkonstante → Guard-Konstante (gleicher Wert) |
| `bsc/gates/gate0/src/bin/g06a_native_bnb_quote.rs` | ACTIVE | `bsc/native/crates/qualify/src/bin/` | Arithmetik → `core::economics::flap::native_curve20` + `slippage::min_output`; **7/7 gespeicherte DRY-Quotes byte-identisch** |
| `bsc/gates/gate0/tools/g06a_preflight_readonly.cjs`, `g06a_economics_evm_crosscheck.cjs`, `g06a_local_simulate.cjs` | ACTIVE | `bsc/tools/` | nur Pfade (npm-Toolsets, Default-Preflight) |
| `bsc/tests/legacy_gate0/gate0-evidence/{evm-audit-tools,abi-tools}/package*.json` | ACTIVE | `bsc/tools/{evm-audit-tools,abi-tools}/` | bytegleich; `node_modules` lokal mitverschoben (ignoriert, `npm ci` reproduziert) |
| `bsc/native/tools/*` (C1-/Lock-Werkzeuge) | ACTIVE | `bsc/tools/` | Root-Ableitung angepasst; Unit-Tests PASS |
| `bsc/native/ACCEPTANCE_MAP.json` | ACTIVE (Abnahmekarte) | `bsc/docs/ACCEPTANCE_MAP.json` | bytegleich; Inhalt nennt Originalpfade |
| `bsc/native/crates/darkbot-{wire,core,fastlane,runner,capture,qualify}/` | ACTIVE | `bsc/native/crates/{wire,core,fastlane,runtime,capture,qualify}/` | Paketnamen unverändert; `include_*`- und Fixture-Pfade angepasst (siehe §3) |
| `bsc/gates/gate0/` (restlicher Code, G03–G07-Testnet-Binaries, Tools, `evidence/`, `Cargo.toml`, `Cargo.lock`) | PRESTUDY | `prestudy/bsc/experiments/gate0/` | 1.294 Dateien bytegleich (Hash-Abgleich gegen Git-Index); Code + eigene Evidence zusammen, damit relative Includes erhalten bleiben |
| `bsc/gates/README.md` | PRESTUDY | `prestudy/bsc/experiments/README.md` | bytegleich |
| `bsc/native/tools/lock_tls.sh`, `verify_tls_lock.py` | PRESTUDY | `prestudy/bsc/experiments/native-tools/` | benötigen den Gate-0-Lock als Referenz |
| `bsc/tests/` (1.062 Dateien, v. a. Gate-0-Evidence) | PRESTUDY | `prestudy/bsc/evidence/tests/` | bytegleich |
| `bsc/docs/phase_a/`, `bsc/docs/reviews/`, `bsc/docs/BSC_PHASE_B_OPTIONAL_CLEANUP_DRAFT.md` | PRESTUDY | `prestudy/bsc/evidence/docs/` | bytegleich (Phase-B-Draft: Status „ARCHIVED“) |
| `bsc/docs/production-plan-reference/` | PRESTUDY | `prestudy/bsc/references/production-plan-reference/` | bytegleich; Masterplan-Links umgestellt |
| `bsc/native/SHA256SUMS.txt`, `SHA256SUMS.unit5-core-overlay.txt` | PRESTUDY | `prestudy/bsc/evidence/native/` | bytegleich; Basis `./` = damaliges `bsc/native/` mit alten Crate-Ordnern |
| `gates/bsc/gate0/src/p2p_ingress.rs` (Root, außerhalb `bsc/`) | PRESTUDY | `prestudy/bsc/references/root-gates/bsc/gate0/src/p2p_ingress.rs` | bytegleich; **nicht identisch** mit der bsc-Kopie (beide getrennt erhalten) |
| `bsc/src/darkbot/**/__init__.py` | PRESTUDY | `prestudy/bsc/references/bsc-src/darkbot/` | ohne Verbraucher; Paketidentität erhalten |
| `bsc/{config,tools,data,exports,migrations}/.gitkeep` | DELETE | — | leere Platzhalter |

Lokal, nicht versioniert (Sicherung unter `bsc/.local/backup/`):

| Original | Heute |
|---|---|
| `bsc/gates/gate0/evidence/g06a_final_dry_*` (11 untracked DRY-Läufe) | `bsc/data/g06a_final_dry/` (Ausgabeort von `tools/g06a_final_dry.sh`, gelesen von `tools/g07_pre_live_readiness.sh`) |
| `bsc/local_diagnostics/` | `bsc/data/local_diagnostics/` |
| `bsc/gates/gate0/.env.save` | `bsc/.local/legacy-gate0.env.save` (Modus 600, Inhalt nicht gelesen) |

## 2. Bewusst nicht verschoben (Pfadidentität)

- `bsc/native/evidence/` bleibt: `Q5A_TLS_EVIDENCE_PATH = "bsc/native/evidence/q5a/q5a-both-20261005T203919Z.json"` und dessen SHA-256 sind als Qualifikationsidentität (Generation 1) in `runtime/src/inclusion/tls/qualification.rs` einkompiliert. Ein Umzug wäre eine Q5A-Neuqualifikation, kein Strukturumbau. Die C1-Tools schreiben ebenfalls dorthin.
- Kommentar-/`#[ignore]`-Hinweistexte mit `bsc/native/tools/…` in `fastlane/src/v2.rs`, `fastlane/src/ingress.rs` und im Go-Overlay (`*_test.go`) sowie im untracked Beispiel `c19_consumer.rs` bleiben unverändert (C1-Hot-Path/Overlay, Kompatibilität mit Stash „local-before-p2p-fix-20261011“ (`5753f70`), keine Funktion). Die Werkzeuge liegen jetzt unter `bsc/tools/`.

## 3. Geänderte Bindungen und Folgen

| Stelle | Änderung | Folge |
|---|---|---|
| `fastlane/src/ingress.rs` | 3 Test-`include_str!`-Pfade `../../darkbot-core/fixtures/` → `../../core/fixtures/` | Dateihash von `ingress.rs` ändert sich (Test-Code, keine Hot-Path-Semantik) |
| `qualify/tests/support/mod.rs` | `include_bytes!` → `../../../runtime/tests/fixtures/` | — |
| `core/src/inclusion/capability.rs` | Fixture-Pfadtexte → `bsc/native/crates/core/fixtures/…`; `fixture_sha256` unverändert | — |
| `qualify/build.rs` | FROZEN-Pfade → `../core/src`, `../wire/src`, `../runtime/src` | `Q5A_BUILD_FROZEN_UNCHANGED` war bereits vor R1 `false` (8 Dateien seit Baseline `92fa520` geändert); bleibt `false` |
| `bsc/native/Cargo.lock` | +12 Pakete (`alloy-sol-types` 1.7.3 und Makro-Abhängigkeiten, exakt die Gate-0-Versionen) und Abhängigkeitslisten der Workspace-Mitglieder; **kein** bestehendes Paket geändert | einziges produktives Lockfile |
| G0.6a-DRY-Report | `contract.path` lautet künftig `bsc/config/…` | alte DRY-Nachweise bleiben historisch gültig; vor Scharfschaltung neuer DRY-Lauf nötig |

## 4. Prüfprotokoll R1–R8 (10-Test-Budget)

| # | Gruppe | Befehl (gekürzt) | Ergebnis |
|---|---|---|---|
| T1 | R1 | `cargo test --workspace` (getrackte Ziele, Baseline) | capture/core grün; 1 fastlane-Test rot (später ohne Änderung grün → flaky); Abbruch durch fail-fast |
| T2 | R2 | `cargo test -p darkbot-runner --features acceptance-guard --lib --test acceptance_guard` | 20 + 6 PASS |
| T3 | R3 | Quote-Byte-Parität gegen 7 gespeicherte DRY-Quotes + Default | 8/8 PASS |
| T4 | R4 | `cargo test -p darkbot-runner --features g07-mainnet --lib --test acceptance_guard --test g07_no_send` | 21 + 6 + 4 PASS |
| T5 | R5 | `cargo test -p darkbot-qualify --lib --bins --test loopback --test offline` | 3 + 4 + 6 + 26 PASS |
| T6 | R7 | `cargo test --workspace --no-fail-fast --features darkbot-runner/g07-mainnet` (alle getrackten Ziele) | 426 PASS, 0 FAIL, 5 ignored |
| T7 | R7 | `python3 -m unittest test_c1_qualification_tools` (in `bsc/tools`) | PASS |
| T8 | R8 | Nachtest nach `cargo fmt`: `cargo test -p darkbot-runner -p darkbot-qualify -p darkbot-fastlane --features darkbot-runner/g07-mainnet …` | 154 PASS, 0 FAIL |
| T9 | R8 | Offline-Toolketten-Smoke aus dem Repo-Root (Quote-Parität am neuen Datenort, DRY-Bundle liest `bsc/config`-v2, G0.7/G0.8 verweigern ohne Eingaben, Shell-Guards) | 17/17 PASS |

Nicht ausgeführt (benötigen Secrets und/oder Mainnet-RPC, daher **nicht** PASS): voller G0.6a-DRY-Lauf, EVM-Crosscheck gegen RPC-State, `g07_pre_live_readiness.sh`, G0.7-Ausführung, G0.8 gegen echte Blöcke.

Bekannt und unverändert: Compiler-Warnung `last_block` in `qualify/src/bin/g08_mainnet_verify.rs` (aus Gate-0 übernommen); Masterplan-Link auf `production-plan-reference/debug_test.log` war schon vor R1 unauflösbar (Datei nie versioniert).
