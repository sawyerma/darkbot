# Runtime-Modularisierung — implementierter Stand

**2026-10-16: im lokalen Produktionscode umgesetzt, offline qualifiziert.** Der frühere reine Plan vom 2026-10-15
ist umgesetzt; die ausdrücklich freigegebene Zielstruktur verwendet `orchestration`, `execution`, `market` und
`analytics` innerhalb derselben Runtime-Crate. `native` und die vier Cargo-Paketnamen bleiben erhalten.

Die vollständige Struktur, jede Alt-/Neupfad-Zuordnung, Abweichungen, Testresultate und Performancegrenzen stehen im
[Abschlussnachweis](runtime_modularization_20261016.md).

| Verantwortung | Tatsächlicher Eigentümer |
|---|---|
| CLI, Moduswahl, kombinierter Capture-Lifecycle | `runtime/src/orchestration/startup.rs` |
| Einziger State-Owner und Reihenfolge | `runtime/src/orchestration/state_owner.rs` |
| Setup, Ingress, Recovery/Events, Sweep, Detection, Worker-Anwendung, Telemetrie, Shutdown | `runtime/src/orchestration/state_owner/` |
| Builder, Pipeline, Routing, Pending, LUT, ObserveOnly, One-Shot | `runtime/src/execution/` |
| Capture, Decoder, Enrichment, Beobachtungsstate, Winner, historische Verifikation | `runtime/src/market/` |
| Econ-Analyse, kanonische Attribution, Records/Ledger, Measurement und Report-Rendering | `runtime/src/analytics/` |
| Kanonischer Chain-/Fork-State | unverändert `solana/src/state/` |
| Binding Health/Sizing/Economics/Funding/Planner | unverändert `liquidation/src/` |

Vier historische Strict-Quellpins bleiben physisch an der Runtime-Wurzel und werden einmal unter `market/observer`
eingebunden. Öffentliche Rust-Pfade bleiben per Reexport verfügbar. Keine weiteren Owner-Funktionszerlegungen oder
Performanceoptimierungen sind Voraussetzung für diese Strukturabnahme. Produktarbeit folgt weiterhin
[PHASENPLAN](../PHASENPLAN.md) und [Ledger](../MEASUREMENT_LEDGER.md); kein wirtschaftliches GO oder LIVE.
