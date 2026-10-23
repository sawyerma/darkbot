# BSC FRONTEND BINDINGS V1 – dauerhafter Controls-Vertrag

Status: UI_PREPARED_ONLY / NOT_NATIVE_EXECUTION_AUTHORITY. Datum: 2026-10-23.
Quelle: PHASENPLAN.md v8/v9/v9.1, insbesondere §5a, §§C1–KB; Masterplan §5–§18.
Implementierung: frontend/src/features/bsc/model/controlRegistry.ts; Ansicht: pages/ControlsPanel.tsx.

## Regel gegen doppelte Implementierung

Für jede Steuerung gibt es genau eine Registry-ID, einen festen UI-Slot, einen nativen Owner und eine Zielphase. Bestehende Controls (A/B, Readiness, Evidence, Wallet-Snapshots) werden NICHT ein zweites Mal erstellt. Ein künftiges Backend schließt den vorhandenen Slot an und ergänzt echte Binär-Statusfelder und Tests. Keine spätere zweite UI für dieselbe Funktion.

## Bereits echt angebunden; beibehalten

| Element | Aktueller Kanal | Relevante bestehende Dateien |
|---|---|---|
| SELF_A / SELF_B | `bsc.candidate.select` als request mit `profile_version` CAS; native Apply erst nach Desktop-Sync | `CandidateActions.tsx`, `candidateControl.ts`, `g07_candidate`, `gateway/src/control.rs` |
| Readiness Run / Stop | `bsc.readiness.start/stop`, native lifecycle | `ReadinessActions.tsx`, `readinessControl.ts` |
| Guard/G0.7/G0.8/DRY/Builder/Witness | Read-only BSC Status/Launch/Monitoring protobuf snapshots | `LaunchBuyPanel`, `OverviewPanel`, `MonitoringPanel`, `AnalyticsPanel` |
| Wallets | Read-only timestamped account evidence | `WalletsPanel` |

Accepted gateway command is not actual successful native apply. Snapshot status and request/result must be correlated and stale/pending states shown accurately.

## Reservierte künftige Controls – alle sichtbaren Slots zunächst deaktiviert

| Domain | Stabile IDs | Mode / Fields | Native phase |
|---|---|---|---|
| Candidate scope | `SELF_A`, `SELF_B`, `FOREIGN_AUTO` | requested/effective, separately from sources; A/B existing selector only | G0.7 / C1–J |
| Independent ingress | `BSC_INGRESS_GETH`, `BSC_INGRESS_PUBLICNODE`, `BSC_INGRESS_DRPC`, `BSC_INGRESS_BLOCKREQ`, `BSC_INGRESS_48CLUB` | OFF/SHADOW/LIVE; requested/effective, capability, health, qualified, full/actionable timestamps, gaps/drops | G0.4 read-only, C1–J live only after Gate-0 |
| Venues | `FLAP`, `FOUR_CLASSIC`, `OPENFOUR` | OFF/SHADOW/LIVE; requested/effective, deployment/qualification/health | G0.1, D/E, I–J |
| Global runtime | `REPLAY`, `PAPER`, `SHADOW`, `LIVE` | requested/effective separate from ingress and venue; economic safety never inferred from mode | I–J |
| Egress/Inclusion | `LOCAL_GETH`, `48CLUB`, `BLOCKRAZOR`, `OWN_BUILDER` | readiness, qualification, coverage, latency, provider freshness; no automatic LIVE fallback | G0.6, C3, I–J; KB optional |
| Future outcomes | `INGRESS_COVERAGE`, `DECISION_TRACE`, `QUOTE_ECONOMICS`, `PERFORMANCE`, `PRODUCER_WITNESS`, `RECONCILIATION`, `REPLAY_RECOVERY` | typed observations only | G0.4–J |
| Future action placeholders | `CONFIG_UPDATE`, `MODE_REQUEST`, `REQUALIFY`, `DRY`, `APPROVE_EXACT_V4`, `ARM`, `SEND_G07`, `EMERGENCY_STOP` | all disabled; no dispatcher exists | individually gated and approved |

## Reservierte exakte Feldschlüssel

Common keys: `control.contract.version`, `control.backend.status`, `control.config.generation`, `control.scope.requested`, `control.scope.effective`, `control.runtime.requested`, `control.runtime.effective`, `control.safety.guard`, `control.safety.exact_approval`, `control.safety.nonce_intent`, `control.safety.max_total_wei`, `control.witness.execution_gap`.

Per source: `control.ingress.<ID>.requested/.effective/.capability/.health/.qualified/.first_full_ns/.first_actionable_ns/.gaps/.drops` (see typed key functions). Per venue: `control.venue.<ID>.requested/.effective/.deployment_sha256/.qualified/.health`. Per inclusion: `control.inclusion.<ID>.readiness/.qualified/.coverage/.latency_p99_ns/.provider_freshness_ms`.

These are **reserved future projection keys**, NOT fields in the currently deployed protobuf. They MUST NOT be fabricated as active native values, used for sign/arm/send authorization, or added to a fake event transport. On future phase implementation, pin exact native schema IDs/field types, generated protobuf, status reporter, gateway whitelist, frontend adapter, Golden fixtures and test matrix TOGETHER under this stable registry contract.

## Future integration protocol

1. First show current implementation/evidence state for the named phase and native owner. Reuse an existing function and the relevant slot. Do not recreate the UI.
2. Wire a typed generation-CAS command only if its phase and safety authority explicitly allow it; keep requested and effective separate, native decision authoritative. A source or venue LIVE request never implies trade LIVE.
3. Expose immutable observed native values through generated binary BSC status/topic; absence stays UNKNOWN/BACKEND_NOT_IMPLEMENTED, not OFF/false/0.
4. Test unsupported/unqualified backend, stale/gap/reconnect, generation conflict, negative capability/health, protocol mismatch, wrong venue version, no secret in UI, and no unwanted economic dispatch.
5. Only then enable the existing control conditionally. Preserve one Rust engine, one journal, current Gate-0 STOP, no new live action.

## Explicit stop and testability

CURRENT: guard UNARMED, no approved economic V4, no automatic Mainnet send. No LIVE AUTO, source LIVE, venue LIVE, ARM/SEND, funding, nonce-burn or wallet mutations from this UI. Physical `T_buy−T0<1ms` requires a real original producer witness, never local visual inference.

Registry UI checks: exact 3 scope IDs, 5 sources, 3 venues, 4 runtime modes, 4 egress adapters; existing A/B selector one place, readiness controls preserved, all placeholders disabled, no network callbacks or dispatch, future backend field keys stable. CSS and design freeze must remain unchanged.

**Phase discipline:** G0.4 observational controls can later read evidence with no economic operation; no C1/C2 product promotion before the original G0.7/G0.8 gates. Frontend preparation alone does not make a backend ready.
