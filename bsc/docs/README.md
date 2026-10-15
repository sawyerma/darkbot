# BSC / Birth Entry

Documentation root for the BSC birth-entry project.

## Goal

Earliest technically possible direct on-chain BUY after a supported BSC launch, with measured acquisition/ordering evidence rather than frontend timing.

## Authoritative documents

1. [BSC_DUAL_VENUE_ROADMAP.md](BSC_DUAL_VENUE_ROADMAP.md) — active project roadmap.
2. [GATE0_TESTPLAN_SHORT.md](GATE0_TESTPLAN_SHORT.md) — current Gate-0 qualification plan.
3. [BSC_BIRTH_ENTRY_ARCHITECTURE.md](BSC_BIRTH_ENTRY_ARCHITECTURE.md) — birth-entry architecture.
4. [BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md](BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md) — target native runtime contract.
5. [BSC_ECONOMICS_GATE_CONTRACT.md](BSC_ECONOMICS_GATE_CONTRACT.md) — economics gate.
6. [GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md](GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md) — measurement/evidence ledger.

Entry points: [PHASENPLAN.md](PHASENPLAN.md) (binding plan index), [STATUS.md](STATUS.md) (code / evidence / LIVE), [ARCHITECTURE.md](ARCHITECTURE.md) (tree, crate ownership), [EVIDENCE.md](EVIDENCE.md) (evidence locations, original paths, R1–R8 migration record). Additional phase contracts, evidence freezes and the acceptance map ([ACCEPTANCE_MAP.json](ACCEPTANCE_MAP.json)) remain in this folder as complete files. Superseded material (Phase-A artifacts, reviews, the Phase-B draft, the Unit-1 production-plan reference) is in [../../prestudy/bsc/](../../prestudy/bsc/).

## Code and evidence

- [Single Cargo workspace](../native/) — crates in [../native/crates/](../native/crates/)
- [G0.7 binary and acceptance guard](../native/crates/runtime/src/)
- [G0.6a DRY and G0.8 binaries](../native/crates/qualify/src/bin/)
- [Pinned bsc-geth overlay](../native/geth/)
- [Path-bound C1/Q5A evidence](../native/evidence/)
- [Active contracts](../config/)
- [Tools](../tools/)
- [Historical Gate-0 workspace and evidence](../../prestudy/bsc/experiments/gate0/)
