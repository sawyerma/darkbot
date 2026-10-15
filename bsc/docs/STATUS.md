# BSC Status — Code, Nachweise und LIVE getrennt

Stand: 2026-10-15, nach der Strukturreform R1–R8 (Ausgangscommit `7f3c08b`). Fachliche Status-Aussagen stammen ausschließlich aus den verbindlichen Dokumenten; dieses Dokument verweist nur darauf und bewertet nichts neu.

## 1. Code (Repository-Struktur)

- Ein einziger Rust-Cargo-Workspace: `bsc/engine/` (Rust 1.93.1, `edition 2024`, `resolver 3`, ein `Cargo.lock`). Go-geth-Overlay getrennt unter `bsc/geth/`.
- Crates (R9–R13): `trading`, `tx_encoding`, `geth_ipc`, `execution`, `journal`, `qualification` mit Paketnamen `darkbot-bsc-*`; Zuordnung zu den früheren Namen in [ARCHITECTURE.md](ARCHITECTURE.md).
- `bsc/native/` enthält nur noch `evidence/` (pfadgebundene Q5A-Identität, siehe [EVIDENCE.md](EVIDENCE.md) §5).
- Offline-Regression R9–R13 (alle getrackten, kompilierbaren Testziele, `--features darkbot-bsc-execution/g07-mainnet`): **426 PASS, 0 FAIL, 5 ignored** — identisch zur R1–R8-Baseline. C1-Tool-Unit-Tests: 20/20 PASS. Prüfprotokoll: [EVIDENCE.md](EVIDENCE.md) §6.
- **C1-Integration (2026-10-16, ausdrückliche Nutzeranweisung als Ausnahme von der Roadmap-Stop-Regel):** C1.5–C1.8 aus dem Stash „local-before-p2p-fix-20261011“ (`5753f70`) in `engine/crates/geth_ipc` und `bsc/geth` übernommen; alle Ziele kompilieren. Workspace ohne Filter (alle Ziele + Doctests): **479 PASS, 0 FAIL, 5 ignored**; `geth_ipc` = C1-Rev2-Referenz (Lib 122/0/5, `c15_hot0_alloc` 1, `c18_stress` 8, Doctests 3 inkl. `compile_fail`-Lebensdauerbeweise); Go-Overlay-Tests mit `-race`: 86 PASS, 0 FAIL, 5 SKIP (Interop-Phasen); Rust↔Go-Interop `c13`/`c14`: PASS vor und nach der V2-Zerlegung. **Keine C1-Phasenabnahme.**
- Wire V2 liegt in `protocol_v2/` (Fachdateien, ein Modul); Release-Maschinencode identisch zur Einzeldatei (98/98 Funktionen).
- **Nicht angeschlossen:** `execution` hat keine Abhängigkeit auf `darkbot-bsc-geth-ipc`; `Hot0Poller`/HOT0 wird nur in `geth_ipc`-Tests und den Mess-Beispielen (`c19_consumer`, `c19b_capture`) verwendet, nicht in der produktiven BUY-Pipeline. Anschluss erst nach C1-Qualifikation über die vorgesehenen C1/C2/C3-Schnittstellen, ohne zweiten BUY-Pfad.
- Offen: C1.9/C1.9b-Latenz- und Hook-Nachweise, C1.10-Freeze; Identität des Maschinencodes belegt keine Ende-zu-Ende-Latenz. `T0 → T_buy < 1 ms` und ein erfolgreicher On-Chain-Kauf sind nicht nachgewiesen.
- Bekannt: `geth_ipc` `protocol_v2::tests::role_leases_are_exclusive_per_role_and_coexist_across_roles` war einmal (R1-Baseline) rot, seither in allen Läufen grün → als **flaky** beobachtet.

## 2. Nachweise (fachlich)

Maßgeblich sind:

- G0.1–G0.8: [GATE0_TESTPLAN_SHORT.md](GATE0_TESTPLAN_SHORT.md) §8 und die datierten Status-Freezes (u. a. „G0.6 final dry status — 2026-10-11: PASS“, G0.7-Single-Attempt-Spend-Freeze 2026-10-11).
- Roadmap-Stand: [BSC_DUAL_VENUE_ROADMAP.md](BSC_DUAL_VENUE_ROADMAP.md) §2 und G0.6a–G0.8-Update 2026-10-13.
- C1: [BSC_C1_STATUS_AND_FABLE_HANDOFF_20261008.md](BSC_C1_STATUS_AND_FABLE_HANDOFF_20261008.md), [BSC_C1_REV2_INTEGRATION_REPORT_20261008.md](BSC_C1_REV2_INTEGRATION_REPORT_20261008.md).
- Register: [GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md](GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md); Ablageorte und Umzüge: [EVIDENCE.md](EVIDENCE.md).

Hinweis aus R1–R8: Der G0.6a-DRY-Report schreibt den v2-Vertragspfad (`contract.path`) in sein Ergebnis. Die vorhandenen DRY-Läufe (lokal unter `bsc/data/g06a_final_dry/`) bleiben als historische Nachweise gültig; **vor einer späteren Scharfschaltung ist ein neuer DRY-Lauf mit dem neuen Pfad `bsc/config/…` erforderlich.**

## 2a. G0.7-Vorbereitung (2026-10-16, read-only, kein Send)

Roadmap-Reihenfolge: G0.7/G0.8 vor jeder weiteren C1/C2-Arbeit.

- **G0.6a-Final-DRY für den eingefrorenen Minimal-Kandidaten: PASS** (DRY1–DRY4, Null-Egress). Signiertes Gas 50 000 000 wei (0,05 gwei), BUY 1 000 000 000 000 wei (0,000001 BNB), Launch-Gaslimit 6 603 270, BUY-Gaslimit 3 000 000; Head 127865760; Nonces 0/0. Quote = unabhängige EVM-Prüfung (176693134169499757962 Roh-Einheiten); lokale Launch+BUY-Simulation erfolgreich (Gas 5 251 225 / 334 654). Aktueller `eth_gasPrice` = 50 000 000 wei = Builder-Floor → dynamischer Gas-Kandidat entspricht den signierten Bytes.
- Evidence (lokal, nicht versioniert): `bsc/data/g06a_final_dry/g06a_final_dry_20261015T222241Z/`.
- **Readiness: `PRE_LIVE_READINESS=PASS_READ_ONLY`, `FUNDING=PASS`**, keine Deployment-/Economics-Drift. Seit 2026-10-16 bewertet `tools/g07_pre_live_readiness.sh` das Funding gegen die exakten signierten Parameter des ausgewählten DRY (`FUNDING_BASIS=EXACT_DRY_PARAMETERS`; Bedarf Launcher 0,0003301635 BNB, Buyer 0,000151 BNB; Bestand je 0,005 BNB). Die v2-Obergrenzen bleiben erzwungen (Preflight verweigert Werte darüber; Guard).
- Bindungs-Hashes (SHA-256): DRY `0efe78a7faf1d817fae3cbfd8893f858a4386a4c2770755465508c668eeded6f`, Preflight `3eb5563d9e0bfea9ce8cc4e3750e3152d81a1b7ea7eff91de78091087bf77e69`, Builder-Policy `ca76a93c432a9cee308b07e59170bac390e801caaa8e0697b0b31d405b9bcc87`, v2-Vorschlag `026b534f4a2e6e76b6eb816a8de84ef71b508aa28b6e24c5c471e9ee6f0b19ff`.
- **Offen vor jeder Scharfschaltung:** (1) ~~Funding-Basis~~ erledigt (exakte DRY-Parameter, Nutzerentscheidung 2026-10-16); (2) Producer-Witness-Quellvertrag für Versuch 1 anlegen (Schema `config/producer_witness_source_v1.schema.json`); (3) Builder-Anforderungen auffrischen (Evidence vom 2026-10-11); (4) DRY-Evidence dauerhaft versioniert sichern; (5) finaler v2-Vertrag mit `approved=true`, `execution_ready=true`, Evidence-Bindungen und eng gefasstem Budget sowie der gesonderte Commit, der `FROZEN_APPROVED_V2_SHA256` setzt — **nur nach ausdrücklicher Nutzerfreigabe**.
- G0.8 setzt das G0.7-Ergebnis voraus (`DARKBOT_G08_G07_EVIDENCE`, `DARKBOT_G08_CHAIN_JOURNAL`); `g08_producer_timing` zusätzlich einen Producer-Witness. Beide Binaries bauen; ohne Eingaben verweigern sie vor jedem RPC.

## 3. LIVE

- **Kein LIVE-Status, keine Freigabe.** `FROZEN_APPROVED_V2_SHA256 = "UNARMED"`, `authorization.approved = false`, `authorization.execution_ready = false` (v2-Vertrag bytegleich, SHA-256 `026b534f…19ff`).
- G0.7 baut nur mit dem Feature `g07-mainnet` und verweigert ohne explizite Autorisierung; bei `UNARMED` endet jeder Versuch vor der ersten Builder-Verbindung (Tests `runtime/tests/g07_no_send.rs`, `runtime/tests/acceptance_guard.rs`).
- Während R1–R8 wurde **keine** Transaktion gesendet und kein Mainnet-/RPC-Lauf ausgeführt.
