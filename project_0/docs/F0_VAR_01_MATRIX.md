# F0-VAR — Engine ↔ UI Abgleichsmatrix (semantisch)

Commitstand: Code `9dbb6607` (Repo-HEAD `30140710`, Rohinventar `F0_VAR_MANIFEST.json` `source_head` `9dbb6607`). Arbeitsstand: **Paket 0 der Project_0-Frontend-Integration (2026-10-24) — vollständige Native→Frontend-Differenzanalyse**, Fortschritt der Folgepakete unten. Normativ: [F0_VAR_00_SCOPE](F0_VAR_00_SCOPE.md), [Frontend-README §3](../../frontend/README.md) (Statusklassen), [Code-Map §10](../../frontend/docs/PROJECT_0_CODE_MAP.md) (Inventar-IDs, F0-P0UI-1…5), [PHASENPLAN](PHASENPLAN.md). Keine zweite Feldautorität: die vier `F0_VAR_*.tsv` bleiben der lexikalische Census, diese Matrix die semantische Zuordnung.

**Statusklassen:** `IST_CONNECTED` Source→Reporter→Proto→Gateway→Adapter/Store bis zum Reiter-Slot nachgewiesen · `IST_BUG` vorhandener Pfad mit belegtem Fehler · `SOLL_MISSING` Owner/Funktion vorhanden oder gewünscht, Pfad fehlt · `FUTURE_PHASE_BLOCKED` erst nach späterem Produkt-Gate · `EXCLUDED` bewusst nie GUI. `IST_CONNECTED` ist weder `QUALIFIED` noch `AUTHORIZED`.

**Zählregel:** (A) jedes Schemablatt der real publizierten Topic-Payloads (Topic 10/13; jeder benannte Market-Counter als eigenes Feld); (B) jede datentragende GUI-Zeile und jedes Control der fünf Reiter, des Health-Drawers und der P0-Karte der globalen Übersicht (reine statische Überschriften/Hinweise nicht gezählt); (C) jede GUI-relevante native Owner-Gruppe ohne Projektion.

## Ergebnis Paket 0

| Gruppe | Anzahl | IST_CONNECTED | IST_BUG | SOLL_MISSING | FUTURE_PHASE_BLOCKED | EXCLUDED |
|---|---|---|---|---|---|---|
| A Wire-Felder Topic 10/13 | 147 (+ 71 Betriebsparameter) | 141 (+71) | 6 | 0 | 0 | 0 |
| B GUI-Zeilen/Controls | 88 | 35 | 10 | 10 | 33 | 0 |
| C Native-Owner ohne Projektion | 33 | 0 | 0 | 11 | 19 | 3 |
| **Summe** | **268** (+71) | **176** (+71) | **16** | **21** | **52** | **3** |

**Fertig verbundene Pfade:** ausschließlich `runtime/src/status_tap.rs` (gefüllt nur vom Market-Capture-Prozess `reporter/src/bin/p0_market_capture_reported.rs:45-59`) → `reporter/src/lib.rs` → `project0.status` (10, `SystemStatus`) und `project0.capture.state` (13, `CaptureState`) → Gateway (`gateway/src/topics.rs`, `available`) → `features/project0/api/project0Adapter.ts` / `project0ViewAdapter.ts` → Store → Overview/Capture/Health/„All values“. Dazu die vorhandene Capture-Prozesssteuerung (`project0.capture.start/stop` → `darkbotctl capture start-configured` / `stop --signal-only`) mit Idempotenz-Ledger, heute nur auf der globalen Übersicht `/` bedient, und `overview.aggregates` `ProjectSummary.live_authorized` (`gateway/src/overview.rs`).

## A. Wire-Felder (Topic 10 und 13)

| Message (proto) | Felder | Native Quelle | Reporter | Frontend-Slot | Status |
|---|---|---|---|---|---|
| `OperationalParameter` (system.proto, 6) | field, unit, value, default_value, env, source | `solana/src/runtime_config.rs` `parameters!` (71 Einträge, Werte aus `active().limits`, Quelle aus `EffectiveConfig.sources`) via `status_tap::operational_snapshot_of` | `lib.rs:295-308`, einmal je Capture-Start | Health „Operational profile“ (Anzahl) + All values | 6 IST_CONNECTED (71 Parameter) |
| `EffectiveRecord` (17) | schema … submission_authorized | `runtime_config.rs:987 EffectiveConfig`; Flags 14–17 vertraglich feste Konstanten | `lib.rs:309-331`; ungesetzt bei eingebauten Defaults | Health-Profil; `live_authorized`/`submission_authorized` im View, typisierte Zeilen statisch → B | 17 IST_CONNECTED |
| `RunInfo` (capture.proto, 26) | run_id … process_started_unix_s | `status_tap.rs:25 RunSnapshot` aus `market/configuration.rs MarketConfig` | `lib.rs:146-175` | Capture-Run-Zeilen, Health „Run“, All values | 25 IST_CONNECTED · **1 IST_BUG** `run_dir` = absoluter lokaler Pfad (`configuration.rs:227`) erreicht den Browser |
| `PhaseStatus` (7) | phase … end_result | `status_tap.rs:65 PhaseSnapshot` (`market/capture.rs`) | gefüllt; UNSPECIFIED nie gesendet | Capture-Badge/Result, Health „Phase“ | 7 IST_CONNECTED |
| `TransportStatus` (9) | connected … last_unix_ms | `TransportSnapshot`/`TransportStateSnapshot` (`market/core.rs:836,876`, nur WSS) | Alter beim Senden neu berechnet | Stream-Zeilen, Health „RPC / WS / slot“ | 9 IST_CONNECTED |
| `MarketCounters` (2 + 44 Counter) | observing, stream_ready, 44 Namen aus `MarketStats::counters()` | `market/core.rs:12` | Namensliste, Einheit nur im Suffix | 11 typisiert (Overview/Capture/Health), Rest All values; `provisional_*_events` sind vorläufige Klassenzähler | 46 IST_CONNECTED |
| `MarketAssessment` (18) + `ReasonCount` (2) + `ReceiverTotal` (3) | verdict … top_receivers | `status_tap.rs:120 AssessmentSnapshot` (alle 2 s) | gefüllt; Verdict während Capture nie VALID | Capture/Health „Market verdict“, Gross edge | 18 IST_CONNECTED · **5 IST_BUG** `gross_edge_total/p50/p90/p99_usd` + `ReceiverTotal.gross_edge_total_usd` als `double`-USD (Wire V1.3 §8: Geld nie als Float) |
| `WriterLane` (13) | sampled_unix_ms … disk_exhaustion_s | `storage/writer.rs:107 StorageStatus` (alle 2 s) | 1:1 | Health Writer-Zeilen | 13 IST_CONNECTED |

`StatusProjection` (status.proto, 9 Felder) hat keinen Producer mehr (Altvertrag; nur generierter TS-Typ) — nicht gezählt, nicht angezeigt.

## B. GUI-Zeilen und Controls (fünf Reiter, Health, globale Übersicht)

| ID | Slot (Datei) | Quelle heute | Status | Ziel / Gate / Owner |
|---|---|---|---|---|
| OV-01 | Stream freshness, Note (OverviewPanel) | Topic 13 `transport.last_frame_age_ns` + Topic-Status | 2 IST_CONNECTED | stale/detached im Hinweis benannt |
| OV-02 | RPC p95, Decision p95 | `UnknownMetric` „— ms“ | 2 FUTURE_PHASE_BLOCKED | P6-MT: keine reale Verteilung (C-OV-4) |
| OV-03 | Realized net PnL, Total capital | `UNCONNECTED_MONITOR` „— USD“ | 2 FUTURE_PHASE_BLOCKED | FE-04/07/P8, `finance_snapshot_id` (C-OV-5) |
| OV-04 | Execution „Disabled · Not authorized“ | statisch, ignoriert `EffectiveRecord.live_authorized` | **1 IST_BUG** | an Topic-10-Wert binden; GUI-Start bleibt gesperrt |
| OV-04b | Available capital | statisch „Not connected“ | 1 FUTURE_PHASE_BLOCKED | C-WL-1 |
| OV-05 | Capture-Badge, Market stream, Unique tx/gaps | Topic 13 | 3 IST_CONNECTED | — |
| OV-06 | Analytics „No stored run loaded“, Replay „Not connected“ | statisch | 1 SOLL_MISSING | kein Run-/Job-Owner für Topic 15 (C-AN-1) |
| OV-NAV | Open Liquidator/Capture/Analytics | lokale Navigation | 3 IST_CONNECTED | — |
| OVG-01 | Globale P0-Karte: Execution authorization | `overview.rs` aus `live_authorized` | 1 IST_CONNECTED | — |
| OVG-02 | Globale P0-Karte: Readiness | `UNKNOWN` | 1 SOLL_MISSING | BootAudit ohne Publisher (C-OV-1) |
| OVG-03 | Globale P0-Karte: Captures, Start/Stop, Logs, Stop All | `captureControl.ts` (HTTP-Polling 2 s) + Command-Ledger | 2 IST_CONNECTED | Polling→Push nur P0-Owner (OPEN) |
| OVG-04 | Globale P0-Karte: RPC p95, Finance | `None` | 1 FUTURE_PHASE_BLOCKED | P6-MT, FE-04/07/P8 |
| LIQ-00 | Running liquidators „0“ | fest kodiert | **1 IST_BUG** | keine native Quelle → „—“ |
| LIQ-00b | Available working capital, Realized net PnL | statisch „—“ | 2 FUTURE_PHASE_BLOCKED | C-WL-1, C-OV-5 |
| LIQ-01…05 | Add/Duplicate/Remove/Select, Name/Origin, Classic/Receivership, vier Limits, Validate draft | `useFleetDrafts` (Browser) | 5 FUTURE_PHASE_BLOCKED | F8-Profile/CAS; Entwurf ≠ Bot |
| LIQ-06 | Start selected bot, Start fleet | disabled | 1 FUTURE_PHASE_BLOCKED | FE-01…06, F8 |
| LIQ-07 | Execution status „Not authorized“ | statisch, ignoriert `live_authorized` | **1 IST_BUG** | an Topic-10-Wert binden |
| LIQ-08 | Parallel operation (4 Zeilen) | statisch „Planned/Unmeasured…“ | 1 FUTURE_PHASE_BLOCKED | P6-MT/F8 |
| LIQ-NAV | Capture → / Analytics → | Navigation | 1 IST_CONNECTED | — |
| CAP-01 | Running-Badge, Feed, Run, Configured duration, Stream, Tx/Liq-Events, Verdict, Result | Topic 13 | 8 IST_CONNECTED | Live-Verdict ≠ Strict-Verifikation |
| CAP-02…04 | Dauer-Presets/Custom, Source, Observation | Browser-State | 3 FUTURE_PHASE_BLOCKED | Gateway-Start nimmt keine Parameter (`start-configured`); typisierter Command + Engine-Bestätigung nötig |
| CAP-05 | Check & start, Stop | nur lokale Notice | **2 IST_BUG** | vorhandenen `captureControl` wiederverwenden (F0-P0UI-2) |
| CAP-06 | Verify Off/Index/Strict/Crosscheck | Index/Crosscheck wählbar | **1 IST_BUG** | `p0ctl run start` verweigert `CHAIN_VERIFY_*_BLOCKED` |
| CAP-07 | Sechs Report-Marker, All/None | Browser-State | 1 SOLL_MISSING | Report-Job-Pfad fehlt (C-AN-1); nur Reportfilter |
| CAP-07b | CLI-Vorschau | `--mode`, index/crosscheck, ohne Marker | **1 IST_BUG** | echte Syntax `--duration`, `--chain-verify off|strict`, `--report-markers` |
| CAP-07c | „Selected report markers N / 6“ | lokal | 1 IST_CONNECTED | — |
| CAP-07d | Hinweis „control binary is not implemented“ | statisch, falsch (`p0ctl run start` existiert) | **1 IST_BUG** | Wahrheitstext |
| CAP-08 | Vier Klassen, vier Related-Types, Provenance/Cost/Time-Filter | fehlt | 1 SOLL_MISSING | `LiquidationType`/`RelatedKind`/`ReportQuery` (C-AN-2) |
| CAP-09 | Boot doctor, Preflight, Capacity, Quota | teils „Not collected“ | 1 SOLL_MISSING | C-OV-1, C-OV-2, C-CP-3 |
| CAP-10 | Quellenmodi OFF/OBSERVE/ACTIVE | fehlt | 1 FUTURE_PHASE_BLOCKED | F0-P0UI-4, S1-02, A23; ACTIVE ≠ LIVE |
| AN-01 | Report/Backtest/Optimize-Tabs | lokal | 1 IST_CONNECTED | — |
| AN-02 | Recorded run, Replay source, Run backtest | disabled | 1 SOLL_MISSING | Run-/Job-Registry fehlt (C-AN-1) |
| AN-03 | Market liquidations, Shadow opportunities, Shadow net edge | „—“ | 3 SOLL_MISSING | Report-Owner offline (`.p0j`), Topic 15 ohne Producer |
| AN-03b | Realized net PnL | „— USD“ | 1 FUTURE_PHASE_BLOCKED | FE-04/07/P8 |
| AN-04 | Capital/Minimum net + Optimize; Add as bot draft | Browser-State / disabled | 2 FUTURE_PHASE_BLOCKED | Optimizer SOLL, F8 |
| WAL-01 | Sieben Finanzzeilen | „— USD“/„—“ | 7 FUTURE_PHASE_BLOCKED | `PortfolioFinancialSnapshot`, FE-04/07/P8; Layout eingefroren |
| WAL-02 | Send (Ziel, Asset, Adresse, Menge, Review) | Vorschau | 1 FUTURE_PHASE_BLOCKED | FEUI-10B, eigene Wertfreigabe |
| WAL-03 | Auto-Sweep (Schwelle, Anteil, Ziel, Status, Preview) | Vorschau | 1 FUTURE_PHASE_BLOCKED | FEUI-10C, W-G01…07 |
| WAL-NAV | Balances/Send/Auto-Sweep | lokal | 1 IST_CONNECTED | — |
| HLTH-01 | Run, Phase, Profil, Programm-ID, RPC/WS/Slot, Transport-Events, Writer (6), Verdict, Transaktionen, Gaps, Gateway-Topics | Topic 10/13 | 12 IST_CONNECTED | — |
| HLTH-02 | Gross edge total/p50/p90 | `double` mit `toFixed(2)` | **1 IST_BUG** | A: `double`-USD; Schemakorrektur P0-Owner |
| HLTH-03 | Wallet/Provider/Capacity | „Not collected“ | 1 SOLL_MISSING | C-CP-3, C-OV-2 |
| HLTH-04 | RPC p50/95/99, Ingress→Decision p50/95/99 | „Not collected“ | 2 FUTURE_PHASE_BLOCKED | P6-MT |
| HLTH-05 | All values | `toJson` beider Messages | **1 IST_BUG** | zeigt absoluten `run_dir` |

## C. GUI-relevante Native-Owner ohne Projektion

| ID | Reiter | Owner (Pfad) | Werte / Einheit | Bruchstelle | Status / Gate |
|---|---|---|---|---|---|
| C-OV-1 | Overview | `orchestration/boot_audit.rs` `BootAudit`, `AuditVerdict`, `BootCheck`, `SafetyFlags`, `NetworkAudit` | Ready / OfflineReadyNetworkNotRun / Blocked | kein Tap/Proto; nur `p0ctl doctor` | SOLL_MISSING |
| C-OV-2 | Overview/Capture | `storage/preflight.rs` `CapacityReport`, `Verdict` | Bytes, B/s, ms, `start_allowed` | kein Tap/Proto | SOLL_MISSING |
| C-OV-3 | Overview | `common/src/lifecycle.rs RunState`, `error.rs HaltReason`, `orchestration/control.rs OwnerState` | Zustand | State-Owner-Prozess ohne Reporter; Gateway nur ein Feed je Projekt (`gateway/src/config.rs`) | SOLL_MISSING |
| C-OV-4 | Overview/Health | `common/src/mt0.rs` `Snapshot`/`RemoteSnapshot`/`HistSnapshot`, `histogram.rs`, `timing.rs` | ns p50/p95/p99 | kein Publisher; nur Debug-/Bench-Werte | FUTURE_PHASE_BLOCKED (P6-MT) |
| C-OV-5 | Overview/Wallets | `execution/accounting.rs` `Ledger::totals()` → `realized_net_usd_micro: Option<i128>` (µUSD, kumuliert, nur gesettelte Versuche/Exits), `fee_lamports: Option<u64>`, `open_inventory_mark_usd_micro` (Mark, nie realisiert) | µUSD, Lamports | offline (`p0ctl fe04 accounting`), Runtime nicht verdrahtet; ohne Verdict-Bindung irreführende 0 | FUTURE_PHASE_BLOCKED (FE-04 PARTIAL_LOCAL, FE-06/07 NOT_RUN, P8) |
| C-LQ-1 | Liquidator | `liquidation/src/safety.rs` `Limits`, `Safety`, `Deny`, `DayStats` | USD-e6, Lamports, bps | Limits teils als Betriebsparameter (A); Zustände ohne Projektion | FUTURE_PHASE_BLOCKED (FE) |
| C-LQ-2 | Liquidator | `execution/one_shot.rs` `Approval`, `Intent`, `Phase`; `one_shot/exit.rs ExitPhase`; `one_shot/admission.rs Admission` | — | Owner übergibt `fe04: None` | FUTURE_PHASE_BLOCKED (FE-04…06) |
| C-LQ-3 | Liquidator | `liquidation/src` `HealthView`, `ProtocolSizing`, `FeeSnapshot`, `CandidateInput`, `Evaluation`, `FailedCostMatrix` | I80F48, USD-e6, bps | keine Projektion | FUTURE_PHASE_BLOCKED (read-only nach FE-Qualifikation) |
| C-LQ-4 | Liquidator | `execution/pipeline.rs ReceivershipPlan`; `analytics/record.rs OpportunityRecord`, `Decision`, `LossReason` | — | nur Journal | FUTURE_PHASE_BLOCKED |
| C-LQ-5 | Liquidator | `analytics/payload.rs Fe01Report`, `orchestration/fe_freeze.rs Fe01Verdict`; FE-00…07 nur in STATUS/PHASENPLAN (kein Code-Enum) | Verdict | keine Projektion | FUTURE_PHASE_BLOCKED (FE-01 BLOCKED, FE-05…07 NOT_RUN) |
| C-LQ-6 | Liquidator | `state_owner/metrics.rs OwnerMetrics` (privat): seit 24.10. P6-MT `lane_wait_ns{L0,L1,L2}`, `readset_keys`, `readset_overflows`, `result_actions`, `jupiter_requests_per_routed_revision`; `candidates.rs LaneStats`; `execution/route_cap.rs Stats`; `execution/identity.rs CandidateIdentity`, `ResultActionCounts` | Zähler, ns-Histogramme | nur Owner-Logzeilen (`lane_stats`, `route_cap_stats`, `identity_stats`) und Binärjournal | FUTURE_PHASE_BLOCKED (P6-MT-Qualifikation; p50/p95/p99 nur aus gemessenen Histogrammen) |
| C-LQ-7 | Liquidator | `state_owner.rs ShadowOwner.time_schedule` (P2-04/P2-05), `OpportunityRecord.t_profitable_unix_s` | Chain-Sekunden | kein Topic-12-Producer | SOLL_MISSING (nie als Entscheidung oder Profit zeigen) |
| C-LQ-8 | Liquidator | `solana/src/submit` `SubmitAck`, `LandingEvidence`, `jito.rs BundleStatus`; `execution/pending.rs ReconcileRecord`; `pipeline.rs PipelineTimingSnapshot` | — | Ziel `project0.execution.events` (20); Engine-Feed ohne EVENT-Variante | FUTURE_PHASE_BLOCKED (FE-06/07, F4) |
| C-LQ-9 | Liquidator/Wallets | `execution/accounting.rs` `AttemptRecord`, `ExitRecord`, `StepState`, `Verdict` (`NO_TRADE`, `FIRST_REALIZED_PROFIT_EVIDENCED`, `LOSS`, `UNKNOWN`+Gründe) | — | Ledger offline; Ziel Topic 20 | FUTURE_PHASE_BLOCKED (reale Versuche NOT_RUN) |
| C-CP-1 | Capture | `solana/src/ingress/sources.rs` `SourceUnit`, `OperatingMode{Off,Observe,Active}`, `SourceControl`, `SourceHealth`, `LeadSnapshot`; `state_owner/sources.rs SourceStats` | — | `p0ctl source set/status` nativ; kein Tap/Proto/Command | FUTURE_PHASE_BLOCKED (F0-P0UI-4, S1-02, A23) |
| C-CP-2 | Capture | `ingress/geyser.rs MapperStats` | Zähler | Geyser ohne Health-Events | FUTURE_PHASE_BLOCKED (S1-02/S1-08 Provider NOT_RUN) |
| C-CP-3 | Capture/Health | `solana/src/provider.rs` `ProviderSnapshot`, `CooldownStatus` | requests, in_flight, queued, effective_rps | nur Journalzeilen `CaptureStart`/`MarketSummary`; nicht im Tap | SOLL_MISSING |
| C-CP-4 | Capture | `common/src/channel.rs ChannelStats` | Zähler | nur `ingress_overflow_count` erreicht die GUI | SOLL_MISSING |
| C-CP-5 | Capture | `storage/writer.rs StorageMetrics` (Seal), `storage/segments.rs ChainSummary`, `analytics/evidence.rs OwnerEvidenceWriter`-Fehler | Bytes, Codes | nicht im Tap | SOLL_MISSING |
| C-CP-6 | Capture | `MarketConfig.retention`, `market_gap.rs GapConfig.max_gaps`, `market_reconcile.rs ReconcileConfig` (8 Felder) | s, Anzahl, ms, rps | fehlen in `RunInfo` | SOLL_MISSING |
| C-CP-7 | Capture | `ingress/mod.rs IngressEvent` Gap/Reconnected/Fatal | Ereignisse | Ziel `project0.capture.events` (21) | FUTURE_PHASE_BLOCKED (F4 EVENT) |
| C-CP-8 | Capture/Analytics | Post-Capture-Verifikationsergebnis (`MARKET_DATA_VALID`) | Verdict | `capture.state` endet bei ENDED | SOLL_MISSING |
| C-AN-1 | Analytics | `analytics/report.rs Output`/`DatasetRecord`/`ReportQuery`, `qualification.rs QualificationRecord`, `post_capture.rs Outcome`, `market_verification.rs StrictResult`/`StrictReport`, `market_truth.rs TruthSummary`, `econ_docs.rs SnapshotReport`/`Mt0Telemetry`, `econ_canonical.rs CanonicalSummary`, `shadow_ledger.rs DaySummary` | Berichte | nur `.p0j`-Dateien; kein Job-Registry-/Status-Owner für Topic 15 | SOLL_MISSING |
| C-AN-2 | Capture/Analytics | `market/markers.rs Marker` (6), `market_truth.rs LiquidationType` (4: CLASSIC, RECEIVERSHIP, FORCED_DELEVERAGE, BANKRUPTCY_SETTLEMENT), `market/related.rs RelatedKind` (4) | Reportfilter | Filter-UI fehlt; nie Rohcapture-Filter | SOLL_MISSING |
| C-AN-3 | Analytics | `solana/src/state/mod.rs` `state_epoch`, `recovery_generation` | Epochen | keine Projektion | FUTURE_PHASE_BLOCKED (P6-MT) |
| C-AN-4 | Analytics | rohe `.p0j`-Journale, `p0-journal frames` | — | nie zum Browser | EXCLUDED |
| C-WL-1 | Wallets/Overview | `execution/reservation.rs WalletReservations` (`held`, `available`), `one_shot.rs Current`/`ActiveHold`, `one_shot/settlement.rs Settlement` | Basiseinheiten | kein Produktionsaufrufer | FUTURE_PHASE_BLOCKED (FE-04) |
| C-WL-2 | Wallets | `accounting.rs Withdrawable` (`Known(u64)`/`Unknown`) | Basiseinheiten je Asset | nur Tests | FUTURE_PHASE_BLOCKED (FEUI-10B, W-G) |
| C-WL-3 | Wallets | `execution/inventory.rs Target`, `state_owner/accounts.rs` Inventar | — | keine Projektion | FUTURE_PHASE_BLOCKED |
| C-WL-4 | Wallets/Overview | Treasury-/Wallet-Balance, `finance_snapshot_id` | — | kein Owner, kein Produktionsproduzent | FUTURE_PHASE_BLOCKED (FE-04/07/P8) |
| C-WL-5 | Wallets | Schlüssel, Seeds, Wallet-Pfade, signierte Transaktionsbytes | — | nie | EXCLUDED |
| C-SYS-1 | alle | `topics.proto` verspricht `pending_commands[]`, `recent_command_results[]`, `config_version` in `project0.status` | — | kein Proto-Feld | FUTURE_PHASE_BLOCKED (F6 native Receipts OPEN, CAS) |
| C-SYS-2 | alle | `status.proto StatusProjection` | — | Altvertrag ohne Producer | EXCLUDED |

## Bruchstellen (wo der Pfad endet)

1. **Topics 11/12/14/15:** keine Proto-Message in `darkbot.project0.v1`, kein Reporter-Fill, `payload_type ""` → Gateway `available=false`, Hello würde sie abweisen.
2. **Topics 20/21 (EVENT):** zusätzlich nimmt der Engine-Hello nur STATE-Topics an und `EngineFrame` hat keine Event-Variante (F4).
3. **Ein Feed je Projekt** (`gateway/src/config.rs`): ein zweiter P0-Prozess (State-Owner/Liquidator) kann keinen eigenen Feed anmelden; heute speist nur der Market-Capture-Prozess.
4. **Status-Tap nur im Capture-Prozess:** Owner-, FE-, Boot-, Provider-, Quellen- und P6-MT-Daten erreichen keinen Reporter.
5. **P0-Reporter ohne `StateTooLarge`** und ohne Größenprüfung (heute kleine Payloads; nötig für listenlastige Topics) — nur P0-Owner.
6. **Overview-`ProjectSummary` für P0:** Readiness `UNKNOWN`, Latenz/Finance/Jobs leer (`gateway/src/overview.rs`), mangels Engine-Quelle.
7. **Wire-Defekte im Schema:** `double`-USD in `MarketAssessment`/`ReceiverTotal`; absoluter `run_dir` — Schema-/Reporter-Korrektur nur P0-Owner.

## Folgepakete (Fortschritt)

- **Paket 1 (belegte Integrationsfehler, Frontend):** B-Zeilen mit `IST_BUG` außerhalb des Schemas (OV-04, LIQ-00, LIQ-07, CAP-05, CAP-06, CAP-07b, CAP-07d, HLTH-05 Anzeige). Stand: offen.
- **Pakete 2–5:** keine weiteren qualifizierten Native-Werte ohne Publisher; `SOLL_MISSING`-/`FUTURE_PHASE_BLOCKED`-Zeilen bleiben mit Owner und Gate offen.
- **P0-Owner-Abhängigkeiten:** C-*-SOLL_MISSING-Projektionen, Schema-Defekte (A), Reporter-`StateTooLarge`, Capture-Polling→Push, native Receipts (F6).
