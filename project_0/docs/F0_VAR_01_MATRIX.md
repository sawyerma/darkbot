# F0-VAR — erste Engine ↔ UI Abgleichsmatrix

Commitstand: `6ed654f54a970990636cc12013b8cb03fa49ebb4`. Arbeitsstand **PARTIAL**, weitere Befunde werden nach dem vollständigen Source-Census und Owner-Review ergänzt.

| GUI-Anzeige oder Aktion | Ist-Quelle / Produzent | Runtime-Modus | Heute auf Wire/UI | Offener Vertrag / Qualifikation |
|---|---|---|---|---|
| Run-ID | `market/configuration.rs::MarketConfig.run_id` | Market-Capture | `project0.status` + Health | angezeigt, Gateway-Validität belegt |
| Slot | `MarketStats.last_observed_slot` | Market-Stream | `StatusProjection.last_observed_slot` | bei HTTP 429 fehlend, nicht null erfinden |
| Stream-Alter | `TransportSnapshot.last_frame_age_ns` | Market | `StatusProjection.last_frame_age_ns` | abhängig vom eingehenden Stream |
| Gap-/Reconnect-/Liquidation-Zähler | `MarketStats` | Market | drei Statusfelder | nicht automatisch kanonische Liquidationszahlen |
| Markt-Verdict | `market/core.rs::market_verdict` | Market | `StatusProjection.market_verdict` | NOT_READY / INCOMPLETE semantisch erhalten |
| Writer queue und lag | `storage/writer.rs::StorageStatus` → `StatusTap` | Market, 2-s-Sample | zwei primitive Statusfelder | 10 weitere Writer-Werte fehlen; WriterLane-Entwurf prüfen |
| HTTP 429 | `MarketStats.transport_http_429` | Transportereignis | fehlt | auch vor StreamReady zählen |
| Transport-Reconnect-Versuche | `MarketStats.transport_reconnect_attempts` | Market | fehlt | nicht gleich HTTP-429-Zähler |
| MarketConfig (Quelle, Durations, Host) | `MarketConfig` | Market | nur Run-ID | niemals Auth-Schlüssel oder URL-Query exportieren |
| Laufphase / Stoppgrund | `market/capture.rs` | Market | fehlt | Waiting / Capture / Finalize unterscheiden |
| Boot Audit und Programm-ID | `orchestration/boot_audit.rs::BootAudit` | Boot-Audit | Health „Not checked“ | NOT_RUN von PASS strikt unterscheiden |
| Kapazität | `storage/preflight.rs::CapacityReport` | Preflight | fehlt | Gates / Memory / Disk |
| Oracle / Sweep / Owner-Metriken | `state_owner/metrics.rs::OwnerMetrics` (privat); seit 24.10. P6-MT `lane_wait_ns{L0,L1,L2}`, Lane-Promotions/-Demotions, `route_cap_cache_hits/misses`, `jupiter_requests_per_routed_revision`, Readset-/Ergebnis-/Ownership-Zähler (Owner-Logzeilen `lane_stats`/`route_cap_stats`/`identity_stats`, Binärjournal) | Shadow / Live | fehlt | Besitzerzugriff und Sample-Semantik nötig; p50/p95/p99 nur aus gemessenen Histogrammen |
| Decision-p95 | OwnerMetrics Summen/Max existieren, p95 unbestätigt | Shadow / Live | `UnknownMetric` | p95 nicht aus Durchschnitt/Max vortäuschen |
| RPC-p95 | Quelle nicht nachgewiesen | Runtime unbekannt | `UnknownMetric` | Messung/Population prüfen |
| Kapital / Realized Net PnL / Treasury | **SOLL_MISSING** (Projektion): kanonischer Producer seit 24.10. `execution/accounting.rs::Ledger::totals()` — `realized_net_usd_micro: Option<i128>` (µUSD, kumuliert, nur gesettelte Versuche/Exits), `fee_lamports: Option<u64>`, `open_inventory_mark_usd_micro` (Mark, nie realisiert); aus den FE-04-Journalen (archiviert + aktiv) | Ledger offline (`p0ctl fe04 accounting`), Runtime nicht verdrahtet | `UNCONNECTED_MONITOR`; Ziel `project0.finance.summary` (11) / `project0.wallets.state` (14), Reiter Overview + Wallets | nur finalisierte Werte; UNKNOWN (`None`/Verdict UNKNOWN) nie als 0; withdrawable: `Ledger::withdrawable` read-only (`Withdrawable::Known(u64)`/`Unknown(reason)`, Basiseinheiten je Asset), kein Transfer-Code |
| Versuchshistorie / FE-07-Verdict | **FUTURE_PHASE_BLOCKED** (FE-06/07): `execution/accounting.rs::{AttemptRecord, ExitRecord, StepState, Verdict}` (`NO_TRADE`, `FIRST_REALIZED_PROFIT_EVIDENCED`, `LOSS`, `UNKNOWN`+Gründe) | Ledger offline | fehlt; Ziel `project0.execution.events` (20), Reiter Liquidator + Wallets | reale Versuche NOT_RUN; Verdict nur aus finalisierten Journalen |
| Capture-Start/Stop, Dauer, Quelle, sechs Marker | `CapturePanel` hat nur CLI-Vorschau; `darkbotctl` lokal | Ops / Market | nicht als UI-Command | F6 benötigt Autorisierung, sicheren Control-Path |
| Kandidaten-Weckzeiten (Premium-Grenze, Tag-`t_profitable`) | **SOLL_MISSING** (Projektion): Producer seit 24.10. `state_owner.rs::ShadowOwner.time_schedule` (privat, P2-04/P2-05) und `OpportunityRecord.t_profitable_unix_s` (je Bewertung im Owner-Journal) | Shadow / Live | fehlt; Ziel `project0.liquidator.state` (12) | Chain-Sekunden, Schätzung ≤ echter Zeitpunkt; nie als Entscheidung oder Profit anzeigen |
| Liquidator-Bot-Limits / Strategien | `useFleetDrafts` Browser-Entwürfe | Browser | keine Runtime-Steuerung | LIVE gesperrt, keine stillen Kommandos |
| Analytics Jobs / Reports | Journale/Reports als potentielle Quelle | Post-Capture | kein Topic-Publisher | IDs, Epochen, Status und qualified facts |
| Wallet Send / Sweep | nur UI-Vorschau | Wertetransfer | ausdrücklich deaktiviert | Execution/Authorization und finalisierte Balances fehlen |

## Topics als vorhandener Nummernvertrag

Das Protobuf `protocol/schemas/darkbot/control/v1/topics.proto` reserviert `project0.status` (10), `project0.finance.summary` (11), `project0.liquidator.state` (12), `project0.capture.state` (13), `project0.wallets.state` (14), `project0.analytics.jobs` (15), `project0.execution.events` (20) und `project0.capture.events` (21). Im aktuellen `gateway/src/topics.rs` und `frontend/src/core/realtime/topicTable.ts` ist davon nur `project0.status` als verfügbares Project_0-Payload beschrieben. Ein Enum-Wert allein ist keine funktionsfähige Subscription.

## Noch NICHT F0-VAR-verifiziert

Quellen für finalisiertes Kapital/Wallet-/PnL, `boot_network.rs`- und Provider-Budgets, echte Entscheidungslatenz-Verteilung, Journal- und Replay-Datenmodelle, Engine-Kommandos, Null-Semantik, alle angebundenen Views, Datenfluss zu Analytics sowie Erfüllung der Sicherheitsgates.

**F0-VAR ist erst abgeschlossen**, wenn die Matrix aus dem Rohinventar systematisch für alle relevanten GUI-Felder/Engine-Produzenten ergänzt und reviewed wurde.
