# BSC / Birth Entry

Documentation root for the BSC birth-entry project.

## Goal

Earliest technically possible direct on-chain BUY after a supported BSC launch, with measured acquisition/ordering evidence rather than frontend timing.

## Authoritative documents

1. [BSC_DUAL_VENUE_ROADMAP.md](../1_PLAN/BSC_DUAL_VENUE_ROADMAP.md) — active project roadmap.
2. [GATE0_TESTPLAN_SHORT.md](../1_PLAN/GATE0_TESTPLAN_SHORT.md) — current Gate-0 qualification plan.
3. [BSC_BIRTH_ENTRY_ARCHITECTURE.md](../2_REGELN/BSC_BIRTH_ENTRY_ARCHITECTURE.md) — birth-entry architecture.
4. [BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md](../2_REGELN/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md) — target native runtime contract.
5. [BSC_ECONOMICS_GATE_CONTRACT.md](../2_REGELN/BSC_ECONOMICS_GATE_CONTRACT.md) — economics gate.
6. [GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md](../3_BEWEISE/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md) — measurement/evidence ledger.

Entry points: [PHASENPLAN.md](PHASENPLAN.md) (binding plan index), [STATUS.md](../1_PLAN/STATUS.md) (code / evidence / LIVE), [ARCHITECTURE.md](../2_REGELN/ARCHITECTURE.md) (tree, crate ownership), [EVIDENCE.md](../3_BEWEISE/EVIDENCE.md) (evidence locations, original paths, R1–R8 migration record). Additional phase contracts, evidence freezes and the acceptance map ([ACCEPTANCE_MAP.json](../1_PLAN/ACCEPTANCE_MAP.json)) remain in this folder as complete files. Superseded material (Phase-A artifacts, reviews, the Phase-B draft, the Unit-1 production-plan reference) is in [../../prestudy/bsc/](../../../prestudy/bsc/).

## Code and evidence

- [Single Cargo workspace](../../engine/) — crates in [../engine/crates/](../../engine/crates/)
- [G0.7 binary and acceptance guard](../../engine/crates/execution/src/)
- [G0.6a DRY and G0.8 binaries](../../engine/crates/qualification/src/bin/)
- [Pinned bsc-geth overlay](../../geth/)
- [Path-bound C1/Q5A evidence](../../native/evidence/)
- [Active contracts](../../config/)
- [Tools](../../tools/)
- [Historical Gate-0 workspace and evidence](../../../prestudy/bsc/experiments/gate0/)
