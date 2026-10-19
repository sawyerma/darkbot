# Fable: React-Implementierungsauftrag

**Hauptvertrag:** [FRONTEND_PHASENPLAN.md](FRONTEND_PHASENPLAN.md). Design: [Graphite-V7-Referenz](../design/reference/project0_graphite_v7_fleet.html), [V0-Freeze](V0_GRAPHITE_FLEET.md). Die Dateien können direkt über Git gelesen werden.

Erst F0-VAR (Project_0-/BSC-Feldinventur) gegen wirkliche Rust-/CLI-Owner abschließen; fehlende Backend-Felder als SOLL markieren. F1: bestehende React-V0 fünf Panels visuell erhalten, globale Host-Navigation/BSC-Design, Tokens und visuelle Tests. F2: versioniertes Protobuf und Goldens. F3/F4: echte Source-Liveness und journalgebundene Events. F6: gesicherte Commands, F7: projektweise Integration, F8 deferred. Kein zweites Frontend, keine Engineänderung, kein LIVE, keine erfundenen Profite.

Achtung: Original-GUI-HTML byte-genau mit der bekannten SHA-256 `887aac385b04ca315615844399f42ff611b10de204a5cb9e3bc7ffb2c87eef36` vergleichen. Repository-Kopie ist bei Übergabe noch nicht als identisch verifiziert. Screenshots müssen separat ins Git, bevor ein pixelgenauer visueller Vergleich als nachgewiesen gilt.

Pro Schritt geänderte Dateien, echte Typecheck/Lint/Build/Playwright-Ergebnisse und Source-/Commit-SHA liefern.

## Current priority: F7-OVR Overview / independent project controls (2026-10-19)

**Binding references:** [Master §10 F7-OVR](FRONTEND_PHASENPLAN.md#10-binding-f7-ovr-amendment--independent-project-lifecycle-and-overview-control-2026-10-19) and [actual repository Graphite HTML reference](../design/reference/darkbot_overview_control_v2.html). This task is subordinate to F6 safety, Governance and the canonical Wire. **No commit, push, new branch or worktree.**

Read pinned `main` SHA, `OverviewPage.tsx`, `ProjectEntry.tsx`, `overview.css`, `model/overviewRows.ts`, `projectRegistry.ts`, real Gateway/IPC/engine lifecycle source, `ops/darkbotctl`, and all relevant tests. The real design system is the existing shared graphite tokens and **the existing** Project_0 V7 HTML, not a recolored redesign. Frozen Project_0 Wallets and other Project_0 graphical assets remain byte-identical.

**Exact visual architecture:** 3 cards above (Project 0, BSC, Solana; preserve five fields in each: Readiness, Execution authorization, Bots / jobs / captures, RPC p95, Realized net PnL), 2 equal tiles below (left **System & Control Plane** + `Stop All` and `Show All Logs`; right **Global Finance Aggregate**). No `Start Selected`. A project card adds a separate lifecycle dot and independent Start/Stop/Logs without nested buttons or broken routing. English UI.

**Mandatory engineering questions for return:** show which supervisor/process-group status can prove `RUNNING` for P0 read-only capture and separately for trading, BSC and Solana; prove that feed `Ready` is not process running. Propose safe authenticated same-origin mutating operations, CSRF/Origin/Host, engine/supervisor owner, expected version/idempotent receipts, no duplicate starts, graceful stop and seal, no SIGKILL, reconnect semantics, one project at a time, Stop All partial failure, bounded redacted log viewer. Resolve existing `P0_APP_AUTOSTART_CAPTURE=1` and `until-stopped` behavior against user-manual STOP on reopened Desktop app **without changing local settings or guessing**. If an API lacks qualification, action remains DISABLED with a real reason. Do NOT implement LIVE/BUY or wallet commands.

Finance/RTT integrity remains strict: no fabricated latency; `net_pnl` != necessarily realized; `total_capital` != necessarily equity; no global false aggregate or implied zero. Qualify partial sums by explicit sources or display dashes.

**Deliver back to ChatGPT, not directly to Git:** (1) inspected SHA + source-path/symbol truth matrix, (2) minimal gates/phased file plan plus unknown blockers, (3) exact complete patches or replacement code for the smallest safe independently testable slice, (4) unit, Playwright, Rust and negative tests including double click, source-detach, close/reopen, partial stop, failed journal seal and security, (5) actual executions/exit codes vs NOT_RUN, (6) rollback and open questions. Do not claim a control is functional without evidence. Preserve all existing frontend gates.
