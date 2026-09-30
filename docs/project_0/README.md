# Solana / Project 0 Liquidator

Project 0 is the native Solana liquidation-searcher project.

## Goal

~~~text
native Solana ingress/state
→ exact Project 0 protocol state
→ exact health/liquidation eligibility
→ deterministic economics
→ valid receivership execution
→ Jito/RPC inclusion
→ landing/winner reconciliation
→ realized net profit
~~~

Primary objective: **maximize sustainable realized net profit**. Latency, provider count and technical complexity are only means to that objective.

## Authoritative documents

1. [PHASENPLAN.md](PHASENPLAN.md) — **PHASENPLAN v4**, frozen architecture, phase order, gates and production rules.
2. [MEASUREMENT_LEDGER.md](MEASUREMENT_LEDGER.md) — measurements, evidence and gate results.

## Phase sequence

~~~text
S0 → S1 → S2 → S3 → S4
│
├─ parallel: P0 + P1A + P1B + C0A + H0
│
└→ Gate A
   → P2 → P3 → P4A → P5 → T → P6
   → 0.1.12 deployment: HALT + P1B→P1A + C0B
   → P7 → P4B → P8 → P9 → K
~~~

Key rules from the phase plan:

- one final production implementation; no prototype→rewrite cycle;
- time-critical LIVE path directly in Rust;
- fail closed on unknown protocol/layout/oracle/integration/economics state;
- **Long-Tail first, Race later** until competition evidence justifies escalation;
- Project-0 Core build requires **Gate A = P0 complete + P1A clear + C0A economically plausible**;
- Jupiter or any other route source is an interchangeable `RouteProvider`, not the architecture itself;
- 0.1.12 deployment triggers execution HALT and requalification before further LIVE use;
- no fixed observation duration or start capital without evidence;
- no paid provider without measured positive incremental net value.

## Code and project assets

- [Application boundary](../../darkbot/project_0/)
- [Shared native Solana code](../../darkbot/solana/)
- [Config](../../config/project_0/)
- [Data](../../data/project_0/)
- [Exports](../../exports/project_0/)
- [Gates](../../gates/project_0/)
- [Migrations](../../migrations/project_0/)
- [Tests](../../tests/project_0/)
- [Tools](../../tools/project_0/)

Pump.fun files named `P0_*` do **not** belong to Project 0; they remain under [../solana_pump/](../solana_pump/).
