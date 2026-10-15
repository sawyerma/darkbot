# BSC Phasenplan — Einstieg

Dieses Dokument ist der **Einstiegspunkt** in den fachlichen BSC-Phasenplan. Es ersetzt **keinen** Vertrag und fasst keine Abnahmebedingung verbindlich neu. Verbindlich sind unverändert die folgenden Dokumente, vollständig und in dieser Rangfolge:

| Rang | Dokument | Inhalt |
|---|---|---|
| 1 | [BSC_DUAL_VENUE_ROADMAP.md](BSC_DUAL_VENUE_ROADMAP.md) | Aktive, autoritative Roadmap: Stop-Regel, G0.6a–G0.8-Update (2026-10-13), Phasenfolge §3, Gate 0 ≠ C0 §4, C0–C3 §8–§11, D/E §12, F §13, G §14, H §15, I §16, J §17, KB §18, K §19, Architektur-Invarianten §20, Entscheidungsregel §21 |
| 2 | [GATE0_TESTPLAN_SHORT.md](GATE0_TESTPLAN_SHORT.md) | Gate-0-Qualifikation G0.1–G0.8, Zeitdefinitionen, G0.7-Spend-Freeze und Status-Freezes |
| 3 | [BSC_PRODUCTION_BUILD_MASTER_PLAN.md](BSC_PRODUCTION_BUILD_MASTER_PLAN.md) | Produktions-Masterplan und Abnahmematrix |
| — | Technische Verträge | [BSC_BIRTH_ENTRY_ARCHITECTURE.md](BSC_BIRTH_ENTRY_ARCHITECTURE.md), [BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md](BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md), [BSC_ECONOMICS_GATE_CONTRACT.md](BSC_ECONOMICS_GATE_CONTRACT.md), [BSC_PHASE_C_PYTHON_RETIREMENT.md](BSC_PHASE_C_PYTHON_RETIREMENT.md), [BSC_PHASE_D_FLAP_READ_ONLY_CONTRACT.md](BSC_PHASE_D_FLAP_READ_ONLY_CONTRACT.md), [BSC_PHASE_E_FOURMEME_READ_ONLY_CONTRACT.md](BSC_PHASE_E_FOURMEME_READ_ONLY_CONTRACT.md), [BSC_PHASE_F_DUAL_CAPTURE_CONTRACT.md](BSC_PHASE_F_DUAL_CAPTURE_CONTRACT.md), [BSC_C1_PARALLEL_FAST_LANE_PLAN.md](BSC_C1_PARALLEL_FAST_LANE_PLAN.md), [ACCEPTANCE_MAP.json](ACCEPTANCE_MAP.json) |
| — | Nachweisregister | [GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md](GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md) (menschenlesbarer Master-Record) |

## Grundsätze (aus den obigen Dokumenten, nicht neu)

- Phasen G0.1–G0.8, C0–C3 und D–K sind **Abnahmeschritte, keine Verzeichnisse**. Keine Phase gilt durch Verschieben von Dateien als PASS.
- Die aktive Roadmap priorisiert **G0.7/G0.8** vor weiterer C1/C2-Produktionsarbeit (Stop-Regel der Roadmap).
- Direkter On-Chain-BUY; erfolgreicher Token-Erwerb ≠ Signatur, Submission, Builder-ACK oder lokaler Handoff. `T0 → T_buy < 1 ms` bleibt eine **nicht belegte** Produktanforderung.
- Die Strukturreform R1–R8 (siehe [../README.md](../README.md)) ist **keine** Freigabe für Mainnet-Sends, Funding oder LIVE.

## Wo die Phasen im Code leben

| Phase | Code / Werkzeug (Stand nach R1–R8) |
|---|---|
| G0.6a DRY | `native/crates/qualify/src/bin/g06a_native_bnb_quote.rs`, `g06a_self_launch_bundle.rs`; `tools/g06a_final_dry.sh`, `tools/g06a_*.cjs` |
| G0.7 | `native/crates/runtime/src/bin/g07_mainnet_self_launch_buy.rs` (Feature `g07-mainnet`), Guard `native/crates/runtime/src/acceptance_guard.rs`, Vertrag `config/g01_mainnet_acceptance_proposal_v2.json`; `tools/g07_*.sh` |
| G0.8 | `native/crates/qualify/src/bin/g08_mainnet_verify.rs`, `g08_producer_timing.rs` |
| C0 | `native/crates/{core,wire,runtime,capture}` |
| C1 | `native/crates/fastlane`, `native/geth/`, `tools/c1*`, `tools/c13*`, `tools/c14*`, `tools/c19*` |
