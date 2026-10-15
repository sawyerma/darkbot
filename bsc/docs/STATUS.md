# BSC Status — Code, Nachweise und LIVE getrennt

Stand: 2026-10-15, nach der Strukturreform R1–R8 (Ausgangscommit `7f3c08b`). Fachliche Status-Aussagen stammen ausschließlich aus den verbindlichen Dokumenten; dieses Dokument verweist nur darauf und bewertet nichts neu.

## 1. Code (Repository-Struktur)

- Ein einziger Rust-Cargo-Workspace: `bsc/engine/` (Rust 1.93.1, `edition 2024`, `resolver 3`, ein `Cargo.lock`). Go-geth-Overlay getrennt unter `bsc/geth/`.
- Crates (R9–R13): `trading`, `tx_encoding`, `geth_ipc`, `execution`, `journal`, `qualification` mit Paketnamen `darkbot-bsc-*`; Zuordnung zu den früheren Namen in [ARCHITECTURE.md](ARCHITECTURE.md).
- `bsc/native/` enthält nur noch `evidence/` (pfadgebundene Q5A-Identität, siehe [EVIDENCE.md](EVIDENCE.md) §5).
- Offline-Regression R9–R13 (alle getrackten, kompilierbaren Testziele, `--features darkbot-bsc-execution/g07-mainnet`): **426 PASS, 0 FAIL, 5 ignored** — identisch zur R1–R8-Baseline. C1-Tool-Unit-Tests: 20/20 PASS. Prüfprotokoll: [EVIDENCE.md](EVIDENCE.md) §6.
- Bekannt: `geth_ipc` `v2::tests::role_leases_are_exclusive_per_role_and_coexist_across_roles` ist **flaky** (einmal rot in der R1-Baseline).
- Nicht integrierte C1-Vorarbeit ist getrackt, aber **nicht** im Build: `geth_ipc/src/{host,hot0,outbound}.rs`, Testziele `c15_hot0_alloc`, `c18_stress`, Beispiele `c16_host_profile`, `c19_consumer`, `c19b_capture` (kompilieren ohne den Stash „local-before-p2p-fix-20261011“ `5753f70` nicht), Go-Overlay `chain_v2*`, `outbound*`, `qos*`, `stress_test.go`.

## 2. Nachweise (fachlich)

Maßgeblich sind:

- G0.1–G0.8: [GATE0_TESTPLAN_SHORT.md](GATE0_TESTPLAN_SHORT.md) §8 und die datierten Status-Freezes (u. a. „G0.6 final dry status — 2026-10-11: PASS“, G0.7-Single-Attempt-Spend-Freeze 2026-10-11).
- Roadmap-Stand: [BSC_DUAL_VENUE_ROADMAP.md](BSC_DUAL_VENUE_ROADMAP.md) §2 und G0.6a–G0.8-Update 2026-10-13.
- C1: [BSC_C1_STATUS_AND_FABLE_HANDOFF_20261008.md](BSC_C1_STATUS_AND_FABLE_HANDOFF_20261008.md), [BSC_C1_REV2_INTEGRATION_REPORT_20261008.md](BSC_C1_REV2_INTEGRATION_REPORT_20261008.md).
- Register: [GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md](GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md); Ablageorte und Umzüge: [EVIDENCE.md](EVIDENCE.md).

Hinweis aus R1–R8: Der G0.6a-DRY-Report schreibt den v2-Vertragspfad (`contract.path`) in sein Ergebnis. Die vorhandenen DRY-Läufe (lokal unter `bsc/data/g06a_final_dry/`) bleiben als historische Nachweise gültig; **vor einer späteren Scharfschaltung ist ein neuer DRY-Lauf mit dem neuen Pfad `bsc/config/…` erforderlich.**

## 3. LIVE

- **Kein LIVE-Status, keine Freigabe.** `FROZEN_APPROVED_V2_SHA256 = "UNARMED"`, `authorization.approved = false`, `authorization.execution_ready = false` (v2-Vertrag bytegleich, SHA-256 `026b534f…19ff`).
- G0.7 baut nur mit dem Feature `g07-mainnet` und verweigert ohne explizite Autorisierung; bei `UNARMED` endet jeder Versuch vor der ersten Builder-Verbindung (Tests `runtime/tests/g07_no_send.rs`, `runtime/tests/acceptance_guard.rs`).
- Während R1–R8 wurde **keine** Transaktion gesendet und kein Mainnet-/RPC-Lauf ausgeführt.
