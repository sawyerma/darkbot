# DarkBot Frontend – Master-Phasenplan V1.3

**Status:** Verbindlicher SOLL-Architektur- und Implementierungsplan, Stand 2026-10-19 (F7-OVR specification appended; historical IST sections remain evidence snapshots). **Keine Aussage, dass Gateway, Commands, Protobuf und BSC-GUI bereits implementiert sind.** Fachliche Projektpläne bleiben unabhängig maßgebend. Dieser Master ersetzt die alten, sich überlappenden Frontend-Gesamtpläne.

## 0. Eine Dokumentationsautorität

- **Dieses Dokument**: einzig aktiver Frontend-Phasenplan, Zuständigkeiten, React-Zielstruktur, F0–F8 und Abnahme.
- [GATEWAY_WIRE_PROTOCOL_V1_3.md](GATEWAY_WIRE_PROTOCOL_V1_3.md): einzig aktiver Wire-Vertrag (Gateway↔Browser **und** Gateway↔Engine).
- [GOVERNANCE_UND_SICHERHEIT.md](GOVERNANCE_UND_SICHERHEIT.md): fachliche Autorisierung, Freigaben und Trust-Grenzen.
- [SOURCE_VARIABLE_MATRIX.md](SOURCE_VARIABLE_MATRIX.md): Source-basierte Project_0-/BSC-Feldinventur, **F0-VAR muss vor Adapterimplementierung abgeschlossen sein**.
- [V0_GRAPHITE_FLEET.md](V0_GRAPHITE_FLEET.md): akzeptiertes V7→V0-Design, fünf Reiter, eingefrorene Wallets.
- Weiterhin aktive Spezialanhänge: [GRAPHITE_V5_UI_FINANCE_CONTRACT.md](GRAPHITE_V5_UI_FINANCE_CONTRACT.md) (Finanz-/UX-Details, nicht neueres Design), [STARTUP_BACKTEST_OPTIMIZATION.md](STARTUP_BACKTEST_OPTIMIZATION.md), [WALLETS_TREASURY.md](WALLETS_TREASURY.md), [PROJECT_0_CODE_MAP.md](PROJECT_0_CODE_MAP.md) (historischer Quellkatalog, nach HEAD zu prüfen).
- **[delete/](delete/README.md)**: unveränderte alte Gesamt-/Wire-/Control-/Fleettexte sowie alter Master sind **archiviert und nicht mehr normativ**. Nicht erneut danach implementieren.

Projekt-Fachautorität: [Project_0 PHASENPLAN](../../project_0/docs/PHASENPLAN.md), [STATUS](../../project_0/docs/STATUS.md), [Ledger](../../project_0/docs/MEASUREMENT_LEDGER.md); [BSC Roadmap](../../bsc/docs/BSC_DUAL_VENUE_ROADMAP.md), [Gate-0](../../bsc/docs/GATE0_TESTPLAN_SHORT.md), [BSC Masterplan](../../bsc/docs/BSC_PRODUCTION_BUILD_MASTER_PLAN.md), [BSC Status](../../bsc/docs/STATUS.md). Keine LIVE-Gates werden durch UI-Dokumente ersetzt.

## 1. Produkt, visuelle Referenz und Navigation

~~~text
DarkBot – EINE React/Vite-App, EINE globale Navigation, EIN Graphite-Design
├── /                  GLOBAL OVERVIEW / All Projects
│   ├── Project 0      → /project-0
│   ├── BSC            → /bsc
│   ├── Solana         → /solana (bestehende Route erhalten)
│   └── weitere Projekte später per Registry
├── /project-0         Project 0 Control / Marginfi
│   ├── Overview       echte Read-only Status- und Finance-Projektionen
│   ├── Liquidator     EIN realer Liquidator zuerst, später Fleet
│   ├── Capture        Rohaufnahme, Verification, Marker-Report-FILTER
│   ├── Analytics      Reports/Replay/Backtest/Optimierung nach Gates
│   └── Wallets        Balances | Send | Auto-Sweep – DESIGN FROZEN
├── /bsc               BSC Birth Entry – eigene fünf Panels
│   ├── Overview
│   ├── Launch / BUY   BSC Gate 0 / G0.7, keine automatische Freigabe
│   ├── Monitoring     Builder/Portal/Evidence/G0.8
│   ├── Analytics
│   └── Wallets        BSC-eigen, keine P0-Treasury
└── /solana            separate bestehende Domäne
~~~

'DarkBot' im gemeinsamen Header führt nach '/'. Globale Overview ist **nicht** identisch mit der Overview in P0 oder BSC. Globalübersicht abonniert nur 'overview.aggregates', nie komplette Wallet- oder Botzustände. Weitere Projekte über statisch typisierte Registry; jeder neue Fachbereich bringt eigene Store-/Adapter-/Route- und Sicherheitsgrenzen mit. Kein zweites Frontend, keine Sidebar, kein globaler Signer oder Trading-Controller.

**Akzeptiertes Design:** Graphite-V7-HTML als Referenz, existierende Project_0 React-V0-Oberfläche; Farben '#1b1b1b' Hintergrund, '#303030' Hauptfläche, '#464646' Trennlinien, kompakte macOS-Typografie, zentrale ~800px-Fläche, dezente Pills, keine blauen Karten. Project_0 besitzt exakt 'Overview/Liquidator/Capture/Analytics/Wallets'. Unter Wallets exakt 'Balances/Send/Auto-Sweep'; Zeilenfolge **Total capital, Realized net PnL, On-chain balance, Available capital, Reserved capital, Withdrawable profit, Transferred to Treasury**. Kein Redesign bei BSC-Integration. Tokens mit visueller Regression extrahieren; Wallets Pixeldiff 0 bei identischen Fonts/Viewport.

## 2. Wirklicher IST-Stand, keine Phantomimplementierung

| Existierende Datei | IST | Für produktive UI fehlt |
|---|---|---|
| 'src/app/router.tsx' | '/', '/bsc', '/project-0', '/solana' | typisierte Registry/Lazy |
| 'src/shared/layout/{AppLayout,Navigation}.tsx' | gemeinsame Header/Links | graphitisches Hostdesign, Registry-nav |
| 'src/features/overview/OverviewPage.tsx' | statische Projektkarten, WS-Status | echte Overview-Aggregate |
| 'src/features/project0/Project0Page.tsx' und fünf Panel-Komponenten | Graphite V0 in React | echte Backend-Snapshots/qualifizierte Commands |
| 'src/features/project0/hooks/useFleetDrafts.ts' | **nur ungespeicherte In-Memory-Drafts** | persistente Profile/echte Botinstanzen |
| 'src/features/bsc/BscPage.tsx' | Platzhalter | BSC-Fachoberfläche |
| 'src/features/solana/SolanaPage.tsx' | bestehende Route | nur projektgebundene spätere Integration |
| 'src/core/realtime/RealtimeClient.ts' | alter JSON-WebSocket mit globaler seq | Topics/Protobuf/Zwei-Hop-Liveness/Journal-Cursor |
| 'src/core/realtime/protocol.ts' | Legacy-Envelope | F7 löschen/re-export aus Codegen |
| 'src/core/api/client.ts' | JSON GET-Client | typed Proto-Read/Command; F7 Legacy ausmustern |
| 'src/app/providers.tsx' | Theme und TanStack Query | Zustand-Livestores, Feature-Boundaries |
| Gateway/Proto/Engine-Adapter | **nicht implementiert/nicht nachgewiesen** | F2–F7 |

**Projektstand aus Status-Evidence:** Project_0 C0-E1 4h NOT_QUALIFIED, PERSIST PARTIAL, FE-00 LOCAL_PASS aber NO_ECONOMIC_GO, FE-06 NOT_AUTHORIZED, FE-07 NOT_RUN, positiver eigener finalisierter Net PnL NOT_PROVEN. BSC G0.7/G0.8 NOT_EXECUTED, Portal-v5.25-Drift requalifizieren, aktiver V4-Guard UNARMED. Kein Frontend-Commit ändert diese Urteile.

## 3. Verbindlicher React-/Repo-Zielbaum – IST/SOLL

~~~text
protocol/ [SOLL]
  schemas/darkbot/control/v1/*.proto        ONE wire envelope/control/commands
  schemas/darkbot/project0/v1/*.proto       typed P0 projections
  schemas/darkbot/bsc/v1/*.proto            typed BSC projections
  codegen/toolchain config, README, Golden-Tests

frontend/src/
  app/
    App.tsx,router.tsx,providers.tsx        [IST]
    projectRegistry.ts                     [SOLL] statisch typisierte Module
  core/
    generated/                             [SOLL] ONLY generated Proto-Types
    realtime/
      RealtimeClient.ts                    [IST→SOLL] 1 Socket, Protobuf
      TopicSubscriptions.ts                [SOLL] refcount, grace
      TopicState.ts                        [SOLL] quality + cursor
      protocol.ts                          [IST→F7 DELETE/REEXPORT]
    api/
      client.ts                            [IST→F7 DELETE] legacy JSON
      commandClient.ts                     [SOLL] action-only POST
      historyClient.ts                     [SOLL] paginierte read-only Reports
    projects/readSummary.ts                [SOLL] shared read-only summary
  shared/
    layout/AppLayout.tsx,Navigation.tsx     [IST]
    ui/graphite/tokens.css                 [SOLL] design freeze
    ui/{Tabs,Rows,Notice,...}.tsx          [SOLL] fachfreie Präsentation
  features/
    overview/OverviewPage.tsx              [IST→SOLL] echte Aggregate
    overview/overviewStore.ts              [SOLL] eigenes Zustand-Store
    project0/Project0Page.tsx              [IST] V0 beibehalten
    project0/pages/{Overview,Liquidator,Capture,Analytics,Wallets}Panel.tsx [IST]
    project0/hooks/useFleetDrafts.ts       [IST] drafts ONLY
    project0/store/project0Store.ts        [SOLL] Zustand mit Selektoren
    project0/api/project0Adapter.ts        [SOLL] Proto→P0-Domain
    project0/styles/graphite.css           [IST] visuell frozen
    bsc/BscPage.tsx                        [IST] placeholder
    bsc/pages/{Overview,LaunchBuy,Monitoring,Analytics,Wallets}Panel.tsx [SOLL]
    bsc/store/bscStore.ts                  [SOLL] separater Zustand-Store
    bsc/api/bscAdapter.ts                  [SOLL] Proto→BSC-Domain
    solana/SolanaPage.tsx                  [IST] erhalten
~~~

**Mechanische Grenzen:** ESLint-boundaries verbieten 'features/bsc' ↔ 'features/project0'-Imports. Generierte Protobuf-Fachtypen dürfen nur von schmalen Domain-Adaptern eingelesen werden (Transport-Codec ausgenommen). CI muss gegen Importverletzungen fehlschlagen. Zustand mit stabilen Selektoren: nur betroffene React-Komponenten rendern. Keine Mega-Context-Subscription, kein Quervermischen der Engines. History/Reports in paginierten TanStack-Query-Reads, nicht im Live-Store. BigInt nur exakt und nie direkt in JSON.stringify-Query-Keys. Je Projekt Lazy Loading und Error Boundary.

## 4. Realtime-/Command-Mechanik (nur ein kanonisches Wire-Dokument)

~~~text
P0 Engine (.p0j)  ─┐
BSC Engine (DBREC) ├─ authenticated source feed + own cursor/liveness ─┐
Solana later        ┘                                                  ▼
                                                       Control Gateway
                                               STATE latest full value/topic
                                               client latest value/topic
                                               EVENT journal catch-up BOTH hops
                                               overview read-only aggregate
                                               same-origin browser session
                                                           │
                                             ONE binary/protobuf WebSocket
                                                           ▼
                                             React separate Zustand stores

User click → same-origin HTTPS Proto POST → Gateway auth proxy → ENGINE auth + CAS
            → persisted intent/receipt → status pending/recent push to UI
~~~

**STATE:** immer kompletter kleiner Topiczustand, kein Delta/Ring/State-Hash. Publisher '(generation,seq)'; neue Generation **akzeptieren** und seq zurücksetzen, abgelöste Generationen nicht erneut anwenden. Gateway-Engine-Disconnect => 'SOURCE_DETACHED' an alte **und neue** Subscribers, alter Cache darf nicht OK heißen. Gateway hält genau einen Latest-Value-Cache pro Topic + einen Latest-Slot pro Client/Topic; kein blockierendes WS-ACK pro Datenframe.

**EVENT:** nur '(journal_epoch,durable_cursor)' (mit Source/Run). Engine bestätigt Durable-Commit vor Push; at-least-once und Client-Dedupe. **Zwei Catch-up-Barrieren:** Client↔Gateway und Gateway↔Engine jeweils zuerst Feed/Puffer anhängen, dann High-Watermark aus Engine lesen, Archiv '(C,H]' paginiert nachziehen, Puffer deduplizieren, 'CatchupComplete(COMPLETE)' erst bei ununterbrochenem Upstream bis Freigabe; sonst INCOMPLETE. Upstream-Cursor ist unabhängig vom Browser-Cursor. Overflow/Catch-up-Limit → expliziter Fehler, max. drei Backoff-Retries mit Cap, dann manuell.

**WS-Nachrichten exakt 11:** Client 'Hello,Subscribe,Unsubscribe,Pong', Server 'Welcome,ControlAck,StateFull,Event,CatchupComplete,Heartbeat,Error'. 'Subscribe' mit optionalem **Paar** '(journal_epoch,durable_cursor)' (nur EVENT), 'StatePart' reserviert ohne Implementierung. Kein Resume-Sonderframe, kein STATE-Patch, kein Hash, keine pro-Frame-ACK. Heartbeat vom Server; Wasserzeichen pro Client für **tatsächlich versendete** STATE-Seq. Bad Proto => stabiler Error, nicht Re-Subscribe-Sturm. Max STATE ~256KiB als noch zu messender Grenzwert.

**Commands:** eigene typisierte HTTPS-POSTs, kein Event-Side-Effect; engine-eigene 'profile_version/config_version' im STATE für CAS, nie WS-Seq. 'pending_commands[]' und bounded 'recent_command_results[]' in der Statusprojektion; ACK 'ACCEPTED' ≠ durchgeführt; verlorene Antwort über State bzw. einmaligen GET auf Command-ID prüfen. Engine persistiert Idempotenz nach Intent-/Receipt-Retention. Keine automatischen Geld-Retries. Produktion SPA vom selben Gateway-Origin; HttpOnly/SameSite Strict Secure Cookie (bei HTTPS), Host-/Origin-Allowlist, CSRF für POST, CSP, autentisiertes Engine-IPC; Engine prüft Fachgates.

**Kein Browser-Polling.** Subscriptions folgen Route/aktivem Panel (kurze unsubscribe grace). Global '/' nur 'overview.aggregates'; Project0 Overview 'project0.status + project0.finance.summary'; Wallets eigenes 'project0.wallets.state'; BSC eigene Themen. Große Reports nur auf explizite Nutzeraktion paginiert holen. Simulation/Replay sichtbar 'OFFLINE/REPLAY'; nicht als Gewinn zählen.

## 5. Pflicht-Themen und Fachprojektionen

| Topic | Klasse | Abonnent; Auszug |
|---|---|---|
| 'overview.aggregates' | STATE | global '/', echte Projektzahl/Bots/Jobs/health/qualifizierte Summen |
| 'project0.status' | STATE | P0 Overview, command pending/results + config_version |
| 'project0.finance.summary' | STATE | Overview; gleiche Backend finance_snapshot_id wie Wallets |
| 'project0.liquidator.state' | STATE | Liquidator; ONE runtime bot first, profile_version, Draft != Running |
| 'project0.capture.state' | STATE | Capture; Provider/Storage/Gaps |
| 'project0.wallets.state' | STATE | Wallets; Balances/Available/Reserved/Withdrawable |
| 'project0.execution.events' | EVENT | echte persistierte P0-Receipt-/Execution-Events |
| 'project0.capture.events' | EVENT | echte Journal-Events, Rohaufnahme ungefiltert |
| 'project0.analytics.jobs' | STATE | Job-Status, Report-Referenzen; History GET |
| 'bsc.status' | STATE | G0.6a/G0.7/G0.8, Portal-/Builder-Drift, config_version |
| 'bsc.launch.state' | STATE | Launch/BUY, plan_version und fachlich gesperrte Actions |
| 'bsc.monitoring.state' | STATE | Witness/Evidence/Chain-Reconcile |
| 'bsc.wallets.state' | STATE | BSC-eigene Wallets, kein P0-Treasury |
| 'bsc.execution.events' | EVENT | BSC Journal-/DBREC-Evidence; Epoch+Cursor |
| 'solana.status' | STATE | späterer eigenständiger Adapter |

## 6. F0-VAR: Vor der Umsetzung vollständige P0- UND BSC-Variablenextraktion

Die erste **belegte** Extraktion steht in [SOURCE_VARIABLE_MATRIX.md](SOURCE_VARIABLE_MATRIX.md), aber **vollständiger Census gegen aktuellen HEAD ist ein F0-Hard-Gate**. Für jedes Frontend-Feld: 'project/feature/source_path/symbol/type/unit/scale/default_min_max/validator/config_or_read/owner/sensitivity/provenance/quality/source_sha/required_gate/topic_or_command/test'. Feld ohne Owner/Unit oder mit ungeklärtem Gate => 'BLOCKED / SOLL_MISSING' und keine freie TS-Berechnung.

- **Project_0:** Native 'RuntimeConfig/Resolved::descriptors()', MeasurementPlan, Flags 'duration,capture-source,chain-verify,report-markers/report-types', alle sechs Marginfi-Marker/vier Klassen/vier Related, MarketStats, StorageStatus/CapacityReport, Journal-Provenance, Receipts/Funding/Economics, Wallet Public Identity und nicht vorhandene Portfolio-Snapshot-Grenze. Capture unfiltered; Marker **nur** Report-Filter. Die zwei Realtime Finance-KPIs stammen **später** aus einem finalisierten Backend-Snapshot; PnL nicht aus Shadow oder historischem Winner.
- **BSC:** 'LiveGuardContext,ContractSnapshot', V4-'UNARMED'-Guard, DRY-/Preflight-/Builder-/Portal-Provenienz, 'g07_mainnet_self_launch_buy' Runner-ENV, G0.8 Canonical-/Receipt-/Fee-Felder und Chainjournal; V4 signierte Limits sind primär immutable Evidence **keine** frei editierbaren Sliders. Key- und Builder-Autorisierungs-Geheimnisse sowie credentialed RPC-URLs niemals ans UI übertragen.
- **Source-Klassifizierung:** 'IST_INTERNAL' ≠ 'EXPOSED_TO_GATEWAY' ≠ 'QUALIFIED' ≠ 'AUTHORIZED'. Backend-Steuerparameter nur durch Engine-DTOs mit 'config_version'; echte Defaults nur aus Rust-Descriptors, nie kopiert. Kontostände nur mit Chain-/Price-/Finality-Provenienz. Wei/u128/u256 als dezimaler String; lamports/slots und positive passende USD-micro als uint64/BigInt; vorzeichenbehafteter oder größerer USD-micro/PnL als signierter Decimal-String.

**Definition F0-VAR DONE:** Inventar UI↔Source bis auf einzelne offene SOLL-Felder komplett; Jede REST/Topic-Projektion hat Owner und Quality; jedes Control eine explizite fachliche CAS-Version, Idempotenz und Gate. Alle Quellfelder erneut gegen Source-HEAD/aktuelles Schema geprüft und diffbar abgenommen. Noch nicht im Backend verfügbare Felder sind separat als **SOLL_TICKET** dokumentiert, kein Fake-Adapter.

## 7. Vollständige Fach-UI (ohne übergriffige Engine)

**Globale Overview:** tatsächliche aktive Bots/Jobs/Captures, Gate-/Health-/Sourcezustand pro Domain, Samplefenster bei Latency, finalisierte Finance je Domain. Gesamtprofit/Capital **nur** bei kompatibler Bewertung, Finality und ohne wallet double count; sonst 'Aggregate unavailable'. Keine 0 für Unknown.

**Project_0:** Overview 'RPC p95, decision p95, stream freshness, Total capital, Realized net PnL' mit gemeinsamer Finance-ID; Liquidator Einzelliquidator + nicht persistente Botdrafts; Capture frei einstellbare Dauer nach Backend-Gate, 'UntilStopped' erst wenn nativ implementiert, 'logs|transactions', Verify 'off/index/strict/crosscheck', alle sechs Marker **nur** Bericht. Analytics immutable Reports/Verifier/Competition/Race, Offline-Backtest/Optimize mit echten versiegelten Runs. Wallets Freeze, Send/Auto-Sweep UI erst nach besonderem Treasury-Gate ausführbar. Keine JavaScript-Economics oder Keys.

**BSC:** Overview G0.6a/G0.7/G0.8/Portal-/Builder-Version/Preflight und Source Witness; Launch/BUY read-only freigegebene Plan-/Budgetwerte und Gate-Verditcts (keine UI-Aktivierung); Monitoring BSC Eventjournal, receipts, chain inclusion, canonical ordering/acquisition/fees; Analytics nur echte Ergebnisse; Wallets BNB/Token und eigene Chain-/Wallet-Gates. BSC 'g07' seit Portal-Drift neu zu qualifizieren, Frontend ersetzt das nicht.

**Bot-Schwarm:** F8 später: N versionierte Strategieprofile innerhalb **P0**, gemeinsamer kanonischer StateOwner/Ingress, globaler nativer P0 Execution-Arbiter mit atomarer Wallet-Reservation; keine N vollständigen RPC-Scanner. Gleiche Schnittstellen heute vorbereiten, Scheduler/Arbiter **nicht** vor erstem realisierten Einzelbot entwickeln.

## 8. Frontend-Arbeitspakete mit Definition of Done (F5 in F3 enthalten)

| Phase | Arbeit | Abnahme/Test |
|---|---|---|
| **F0 – Konsolidierung und F0-VAR-Census** | 1 Master, 1 Wire, 1 Governance, alte Texte nach 'delete/', echte P0-/BSC-Felder aus Rust + CLI extrahieren, Type/Unit/Owner/Gate/source sha beweisen; Ist/Soll aktualisieren | Quelleninventar je geplantem UI-Feld; keine Secrets; fehlende Engine-Projektion ausdrücklich BLOCKED |
| **F1 – Design-Freeze** | Graphite-Tokens, bestehende 5 P0-Tab-Screenshots und Wallet-Baseline, Host Navigation/Registry/Lazy/Error Boundary, Boundary-Lint | P0 Wallets Pixeldiff 0, Cross-Import CI-Fail, kein P0-Layout-Bruch |
| **F2 – Protobuf-/Handshake-Spike** | schema family/prost/protobuf-es, Hello/Welcome/Subscribe, Full-State/Event, Major/Minor, Golden Bytes, Units/BigInt | Uint64::MAX + negative i128 + wei decimal, Schema-Evolution, incompatible-major fail-closed |
| **F3 – Read-only Live UI** | same-origin Gateway, authenticated Engine-STATE feed mit Liveness, latest STATE pro Topic/Client, Heartbeat, Zustand stores/selector, Overview aggregate, P0 & BSC read-only status | Engine-Kill SOURCE_DETACHED auch für NEUEN Subscriber, neues generation/seq akzeptiert, route-scoped subscriptions, keine Polling-GETs |
| **F4 – EVENT-Reconciliation** | Gateway-upstream durable cursor + Browser barrier, journal catch-up, epoch-bound cursors, CatchupComplete, Replay/overflow/backoff, echte Archive | Engine während Catch-up kill, Event nach restart, COMPLETE nur nach Journal-Nachzug; bounded RAM, sonst INCOMPLETE |
| **F6 – Commands & Security** | POST/proto, auth/session/Origin/Host/CSRF/CSP/IPC, native CAS-Versionen, idempotente Commands, pending/recent status, lost ACK | duplicate same key one intent; key+new payload rejected; keine LIVE/Gate-Umgehung |
| **F7 – Fachintegration & Migration** | echte P0 Capture/Verifier/Finance/Analytics/Wallets, BSC Launch/Monitoring/Wallets, paginierte History, Legacy-JSON-WS und JSON-read-client nach Parität entfernen | TS typecheck/lint/build; Rust/TS contract, Playwright E2E/Visual, keine duplicate Decoder/Routes, Source-Proof |
| **F8 – Fleet später** | P0 Profiles+Arbiter+Bot-Attribution/Optimizer | **DEFERRED** nach qualifiziertem einzelnen P0-Liquidator und neuer Freigabe |

**FEUI-Anforderungszuordnung** (keine zusätzliche konkurrierende Phasenfolge): FEUI-00→F0; FEUI-01/02→P0 native Gates und später F3/F6/F7; FEUI-03→F1/F3; FEUI-04/05/06/07→F7 nach nativen Evidence-/Econ-Gates; FEUI-08→F8; FEUI-09→Querschnitt F1–F7; FEUI-10A→F7 read-only; FEUI-10B/C→separate Treasury-Freigabe, nicht automatisch F7. BootAudit/UntilStopped/SegmentReplay/Optimization-Details ausschließlich in den Fachanhängen.

**Projektpriorität:** Project_0 EINZEL-LIQUIDATOR-FIRST, 4h/C0-E1/PERSIST/FE-00…07 bleiben eigene Projektgates. BSC G0.7/G0.8 eigene Reihenfolge. Frontend-Aufgaben dürfen diese nicht blockieren.

## 9. Verbindliche Test- und Dokumentationsgates

1. Source-Feld-Vertrag: Source-SHA, Type/Scale/Gate/owner; unbekannte UI-Werte bleiben '—/UNKNOWN'.
2. Graphite: fünf P0-Tabs, Wallets Pixeldiff 0 (festgelegte Browser-/Fontbasis), Keyboard/Responsive/A11y.
3. Wire: Rust/TS Golden, compatible minor + incompatible major, Decimal exakt ohne JS Float.
4. STATE: Server-/Engine-Neustart, SOURCE_DETACHED auch neuer Subscriber, neues publisher generation akzeptiert, Latest-Client-Slot, No-Poll.
5. EVENT: deterministische zwei Hop-Barrieren bei GAP, Restart und Archive, Cursor Dedupe, Quality INCOMPLETE, Retrycap und Overflow.
6. Safety: SameOrigin/Cookie/Origin/Host/CSRF/IPC, native CAS/Idempotenz, Pending+Recent ohne optionale Eventsub.
7. React: Selector render profiliert, nur benötigte Topics, BSC-Chunk nicht auf /project-0, Fehlergrenzen.
8. Backend/Evidence: keine unechten Profite oder Live-Aktionen durch UI/Offline-Replay; Projektledger bleibt Owner.
9. CI/Evidence: exakter Git-SHA, Tests mit Exitcode + Fixture-/Screenshot-SHA; PASS nur nach realem Lauf, FAIL/NOT_RUN ausgewiesen.
10. Performance: STATE-Frame zunächst <=256 KiB, ~15s heartbeat, ~2s grace, max 3 Retry als **Messhypothese**. Publisher-p95/Engine-Channel-Füllstand bei Slow Client messen, Abnahme-SLO **vor** Implementierungs-PASS festschreiben.

**Umsetzungshygiene:** F-Arbeitspaket erst erledigt, wenn Implementierung + Tests + Source-/Protocol-Identity + offene Risiken dokumentiert. Kein neuer paralleler 'FINAL_PLAN.md'. Jede echte Designänderung zuerst hier bzw. im ausschließlich zuständigen Wire-/Governance-Vertrag, dann Migration.

## 10. Binding F7-OVR amendment – independent project lifecycle and Overview control (2026-10-19)

**Normative status:** this section is part of the **single master** and supersedes older Overview UI wording where inconsistent. **Design target approved for Fable review; operating commands are NOT_IMPLEMENTED / NOT_AUTHORIZED.** Evidence baseline `db238cfdb44991387bb5bd706c838befdbbd2661`. Previous sections describing an earlier IST state are historical snapshots and not proof of present capabilities.

**Visual reference (global Overview only):** [standalone Graphite HTML](../design/reference/darkbot_overview_control_v2.html) · [SVG reference](../design/reference/darkbot_overview_control_v2.svg). Colors, spacing, radii and typography come from the real `src/shared/ui/graphite/{tokens.css,graphite.css}` and `src/features/overview/{overview.css,ProjectEntry.tsx}`. The accepted Project_0 [V7 fleet HTML](../design/reference/project0_graphite_v7_fleet.html), five tabs and wallet freeze remain byte-untouched. **The HTML/SVG are static specimens, not proof of runtime state or working controls.**

![Binding Overview structure: three project cards above two equal Graphite panels](../design/reference/darkbot_overview_control_v2.svg)

### 10.1 Fixed information architecture

- **Top row:** exactly **Project 0**, **BSC**, **Solana** project cards, in registry order. Each preserves its current title/domain, independent source-connection pill and ALL five read-only metrics, in order: `Readiness`, `Execution authorization`, `Bots / jobs / captures`, `RPC p95`, `Realized net PnL`. None is dropped, renamed or replaced by the new controls.
- Each project card adds a **distinct process-status dot with label** plus its own qualified **Start OR Stop** and **Logs** controls. The current entire-card `<button>` must be restructured into valid accessible navigation + **sibling** controls; nested buttons or command-by-card-navigation are forbidden. Retain keyboard navigation and stable project-card test IDs.
- **Bottom row:** exactly **two equal-width, equal-height Graphite panels** (stacked on narrow screens): LEFT **System & Control Plane** with `Gateway`, `Overview Feed`, `Connected Projects`, `Gateway RTT`, `RPC Latency P95`, `Last Overview Update` and actions **Stop All**, **Show All Logs**; RIGHT **Global Finance Aggregate** with `Realized Net PnL`, `Unrealized PnL`, `Total Equity`, `Daily Net PnL`, `PnL Coverage`, `Reporting Currency`. **No Start Selected.** Do not add `Data Age` or `Valuation` rows.
- Global Overview remains read-only as to finance/trading. Project-specific numbers stay at the top; global qualified sums at bottom right. English UI labels throughout, existing restrained monochrome Graphite tokens; no new blue/navy UI or parallel design system.

### 10.2 Source truth and indicator semantics

**Connection state ≠ process lifecycle ≠ engine readiness ≠ LIVE permission.** A currently displayed `Ready` source pill means a validated feed connection, never proof of a RUNNING process. `Gateway OPEN` and `Overview Feed LIVE` are infrastructure states only; `Connected Projects` counts attached sources and **must not** be renamed `Running Projects`.

Project process state requires its OWN supervisor-owned, authenticated, timestamped source with launch/run identity and liveness. `RUNNING` → **green** only with positive healthy process evidence; `STOPPED` → gray only when supervisor positively confirms stopped; `STARTING`/`STOPPING` → amber with witnessed operation; `FAILED` → red with evidenced failure; `UNKNOWN` → neutral gray on missing/stale/untrusted status. Unknown must not be inferred as stopped. P0 read-only Capture running is NOT P0 trading being authorized/running. BSC and Solana retain independent process owners and native safety gates.

### 10.3 Start/Stop/Logs control contract (must pass F6 before enablement)

- One project can run while the others are stopped; lifecycle scopes are **independent**. User-facing Start/Stop targets an explicitly qualified project **process group** (for P0 distinguish Capture/Reporter from trading). No generic Start means `BUY`, submit transaction or enable LIVE.
- Real commands only through authenticated same-origin action channel and native/supervisor authority, with CSRF/Origin/Host checks, replay protection, idempotency, CAS/expected version, durable intent/receipt and separate acknowledged-versus-executed status. Duplicate clicks must never spawn duplicate processes. Never display optimistic `RUNNING`.
- Stop means graceful drain and durable finalization (P0 journal seal), then **verified** stopped state. No ordinary SIGKILL. Timeout/ambiguous Ack/failed seal shows failure/unknown, not success. Closing the browser window does **not** stop capture; reopening reconstructs authoritative status without creating a duplicate.
- **Stop All** explicitly confirms scope and returns **per-project** success/failure/unknown; partial stop must not masquerade as global completion. **Gateway remains running** to keep the GUI and logs available. No `Start Selected`.
- **Show All Logs** and per-project `Logs` use bounded, read-only, authenticated, redacted source logs with project/severity/time filters and archive pagination. No unbounded frontend buffer, raw HTML injection or credentials.
- Existing local `P0_APP_AUTOSTART_CAPTURE=1` and `P0_APP_CAPTURE_SECS=until-stopped` are not to be silently changed. Before binding controls, Fable must give an explicit proposal resolving **manual Stop vs next Desktop-app open/autostart**; a previous explicit user Stop must not be deceptively reversed when merely reopening a window.
- **Missing backend interface => disabled action with honest reason**, not a decorative clickable control. Especially BSC/Solana actions remain gated until their process owner exposes a qualified controller. No LIVE/BUY/Wallet-gate changes.

### 10.4 Finance and measurements remain evidence-based

`Gateway RTT` remains `—` until actually measured. An arithmetic mean of per-project RPC p95 values is NOT global p95; disclose source/worst-of-sources or show unknown. `Last Overview Update` is gateway overview calculation age, not chain-event age. `GlobalFinance.net_pnl` is not automatically validated **Realized Net PnL**, and `total_capital` is not automatically **Total Equity**: do not silently relabel them. Partial sums across a qualified subset of projects are allowed **only** if the backend identifies contributors, comparable price/time/finality, no double counting, and marked partial scope. Until supported, monetary rows are `— USD`, never invented zero. `PnL Coverage` counts finalized verified realized-PnL contributors, not project connections.

### 10.5 Delivery gates nested in F3/F6/F7 (NOT a competing phase sequence)

| Work item | Mandatory evidence and acceptance |
|---|---|
| **F7-OVR-A: visual/reference** | 3 original metric-rich cards + 2 equal tiles + exact names, non-nested controls, keyboard/A11y, 1152px and narrow-width screenshots, Graphite CSS/freeze checks. **SPECIFIED; UI commands not wired.** |
| **F7-OVR-B: read-only process truth** | Exact per-project supervisor/IPC owner, lifecycle source identity/run generation, detach/stale/start/restart tests, no false green. **OPEN** |
| **F7-OVR-C: per-project commands** | P0 read-only Capture first; explicit target; auth/CSRF/Idempotency/CAS, sealed journal, double click, crash/reopen, missing BSC/Solana interfaces disabled. **OPEN, gated by F6** |
| **F7-OVR-D: global control/logs** | Stop All confirmation, verified per-project outcomes and partial-failure evidence, keep Gateway up; bounded redacted Show All Logs + project filters. **OPEN** |
| **F7-OVR-E: acceptance** | `npm run type-check`, `lint`, `test`, `build`, `lint:boundaries`, `lint:css`, `check:freeze`, Playwright desktop/mobile, native integration/negative tests, screenshot/sha + rollback. No `PASS` without real execution. **NOT_RUN for new controls** |

**Implementation order:** visual/accessibility proposal → real read-only process status → P0 capture-only lifecycle → BSC/Solana adapters after owner proof → Stop All / logs and full negative-suite. Fable returns exact source-proven patches and tests for review, **no autonomous Git writes**. Current backend architecture and single Project_0 native execution/trading gates remain authoritative; F8 multi-bot is deferred. Only `main`, never branches/worktrees/force push.
## 11. F0-P0UI – Abgleich vorhandener Project_0-Controls (2026-10-23)

**Keine neue UI- oder Protokollarchitektur.** Dieser Abschnitt aktualisiert veraltete IST-Snapshots der Abschnitte 2/3/7. Die bestehenden fünf Project_0-Graphite-Reiter (Overview, Liquidator, Capture, Analytics, Wallets), Navigation, Typografie, CSS, Wallets-Freeze und F0–F8-Phasen bleiben verbindlich.

**Aktueller vollständiger Panel-/Control-Abgleich:** [PROJECT_0_CODE_MAP.md §10](PROJECT_0_CODE_MAP.md#10-revalidierung-2026-10-23--aktueller-reactprotocolnative-abgleich). Der bindende native Variablen-/Gate-Vertrag steht im [Project_0-Phasenplan – F0-P0UI](../../project_0/docs/PHASENPLAN.md). Die bisherigen F0_VAR_*-TSV-/Manifest-Artefakte sind historische, neu zu erhebende Inventare; ihr älterer HEAD ist keine derzeitige Codequelle.

**Bereits vorhanden:** vollständige React-Page mit fünf Panels und HealthSection, Zustand-Store und typisierte Proto-Adapter; Project0-Systemstatus Topic 10 und CaptureState Topic 13; Gateway-Action project0.capture.start/stop mit Capture-Prozess-/Receipt-Abfrage über den bestehenden captureControl-Client; Rust p0ctl mit doctor, preflight, run start/status/stop, verify, report, export, qualify und post-capture. Nicht erneut erstellen.

**Widersprüche:** CapturePanel zeigt zwar wählbare Dauer, Quelle, Observation, Verify und Marker, doch Check & start / Stop erzeugen bislang nur lokale Notices. Die Gateway-Action startet die vorab konfigurierte Capture und nimmt keine frei gewählten Panel-Parameter entgegen. p0ctl run start sperrt index/crosscheck. React-Drafts sind keine Native-Profile. Die übrigen P0-Topic-IDs für Finanzen, Liquidator, Wallets, Analytics und Events sind derzeit ohne verfügbaren Payload-Typ reserviert. Historische „Page Platzhalter“, „p0ctl fehlt“, „nur ein Topic“ sind überholt.

**F7-Verdrahtung:** bestehende Command-Route für Capture einmalig anschließen, echte Parametrisierung erst bei gebundenem typisierten Backend zulassen. Neue S1-02/S1-08-Quellen- und Providerzustände samt OFF/OBSERVE/ACTIVE/Qualification unter vorhandenem Capture/Health anordnen; Deshred/LocalShred als NOT_IMPLEMENTED. Boot-/Preflight-, FE-/Safety-, PERSIST-/JFI-/P6-MT-Werte nur mit qualifiziertem Reporter/Wire-Pfad. Analysefilter bleiben reine Reportfilter; rohe Capture bleibt ungefiltert. Finanzwerte nur aus derselben finalisierten Portfolio-Finanzsnapshotidentität, keine fiktiven Renditen oder freie LIVE-Controls.

**Abnahme:** Bestehende UI-Slots, keine zweite Registry/Dispatcher/Engine, eindeutiger nativer Owner je Feld/Control, Typ/Unit/Gate/Qualität und bestehende Proto-Feldnummer oder ausdrücklich fehlendes Feld; Rust/TS-Goldens, Security/Idempotenz, Playwright und Design-Pixeldiff. Keine Änderung der anderen Domänen oder des eingefrorenen Wallet-Layouts.


## 12. Binding F0-BSC-v9.1 — native Frontend-Feld-/Control-Verdrahtung (2026-10-23)

**Einzige ergänzende F0-BSC-Spezifikation:** [BSC_FRONTEND_BINDINGS_V1.md](BSC_FRONTEND_BINDINGS_V1.md) (A/B/C/D, G1–G23, Native→Protobuf→Gateway→React). Visueller Referenzentwurf: `frontend/design/reference/bsc_graphite_v9_1_review.html` (nur Design, nicht im Runtime-Bundle). Konkreter umsetzbarer Auftrag: `frontend/docs/ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md`. Diese Unterlagen schaffen **weder einen zweiten Master-Phasenplan noch ein zweites Wire-Protokoll**; `GATEWAY_WIRE_PROTOCOL_V1_3.md` und `GOVERNANCE_UND_SICHERHEIT.md` bleiben verbindlich. BSC-Phasenautorität bleibt `bsc/docs/1_PLAN/PHASENPLAN.md`, einschließlich G0.7→G0.8→PASS/ADAPT/STOP.

**Bestehende Architektur ausschließlich erweitern:** `frontend/src/features/bsc/BscPage.tsx`, die fünf `pages/*Panel.tsx`, `bscAdapter.ts`, `bscStore.ts`, `frontend/src/app/projectRegistry.ts`, vorhandene Graphite-Komponenten, `gateway/src/control.rs`, `protocol/schemas/darkbot/bsc/v1/status.proto` und die native `bsc/engine/crates/qualification/src/status/projection.rs`. Kein paralleles React-Projekt, keine zweite Control-Registry, kein alternatives Store-/Gateway-System, keine doppelte Trading-/Economics-Engine. BSC darf Project_0-Wallet-Design und Solana nicht verändern.

**F0-F7-Verpflichtung:** Von BSC G0.x sowie späteren C0–J/KB/K-Phasen neu geschaffene Daten/Controls werden je Phase im einen F0-VAR-Manifest erfasst und entweder mit Rust-Owner, sanitizierter Public-Projektion, additivem versioniertem Protobuf, realem Gateway-Topic/Allowlist, BSC-Adapter/Store, Graphite-Panel und Rust/TS/E2E-Test vollständig verbunden oder als `SOLL_MISSING`/`BLOCKED` mit Gate und Owner ausgewiesen. Status in der Oberfläche darf die Backend-Qualifikation niemals überholen. Keine neuen LIVE-Steuerbefugnisse ohne separate fachliche Freigabe.

**Reiterbelegung:** BSC Overview = knappe read-only Zusammenfassung; Launch / BUY = Kandidaten, V5, PREPARE, Qualifikation, Guard/Readiness/Attempt; Monitoring = fünf Ingress-Quellen, Venues, Inclusion-Adapter, G0.7/8-Acquisition/Reconciliation; Analytics = historische versiegelte Source-Races, Capture, Timing, Kosten und Evidence; Wallets (Label kann später nach visueller Freigabe `Wallets & Budget` werden) = Launcher/Buyer-Identitäten, Funding/Nonces/Reserven/Provenienz. Bereits existierende A/B-Offline-Auswahl und Readiness-Commands bewahren; die Readiness-Aktion nur einmal im BSC-Fachscreen platzieren. SOURCE_DETACHED ausschließlich aus Transport-Liveness, nicht aus altem Snapshot-Alter ableiten.

**Zunächst belegte Korrekturen:** V4-Text durch V5 ersetzen, native `BLOCKED`/ `STOP`/ `PASS_READ_ONLY` nicht zusammenführen; Readiness-Mode aus der Native-Evidence ergänzen; alte Reporter-Evidence mit Source-Commit + Snapshot-Alter kennzeichnen; JSON-Reporting-Grenze explizit als Ausnahme vom binären HOT0 kennzeichnen. Alle neuen Felder strikt nach Sensitivität und Typ/Feldnummer inventarisieren; B-Privat-Salt, Schlüssel, rohe signierte TX und credentialed URLs niemals ans Frontend. 

**Abnahme:** Keine UI-Phantomimplementierung, kein neues globales LIVE-/BUY-Control, keine Änderung der wirtschaftlichen BSC-Stoppregel. F0-Manifest, Protobuf-Goldens, Rust/TS-Tests, Source-/Lifecycle-/CAS-/Fail-closed-Negativtests, Playwright/Graphite-Pixelvergleiche und Test-Exitcodes je tatsächlich verändertem Paket. Dokumentierter `NOT_RUN`-Status ist kein PASS.

## 13. BSC Presentation Density — Basic / Advanced / Technical Details (binding, 2026-10-23)

**Frontend-Entscheidung:** Die bestehenden **fünf BSC-Reiter** (Overview, Launch / BUY, Monitoring, Analytics, Wallets) erhalten einen **einzigen gemeinsamen Darstellungsmodus-Schalter im BSC-Header**: `Basic | Advanced`. **Basic ist der Standard** nach dem Öffnen von BSC. Der Schalter ist ausschließlich eine UI-Detailstufe und **kein** BSC-Ausführungsmodus (`REPLAY/PAPER/SHADOW/LIVE`), kein Control-Gate und keine Autorisierung. Über `Advanced` sind innerhalb der gleichen Panels **einzeln aufklappbare technische Details** verfügbar: **Basic → Advanced → Details**. Die vorhandene Graphite-Designlinie und Navigation sind unverändert; `frontend/design/reference/bsc_graphite_v9_1_review.html` dient weiter als technische/Advanced-Designreferenz, **nicht** als zweite App.

**Ein Owner für den View-Mode:** Der bestehende `BscPage.tsx`-Host hält den gemeinsamen View-Mode als reinen Präsentationszustand; an die fünf vorhandenen Panels wird er als Prop/Hook aus dem bereits vorhandenen BSC-Feature durchgereicht. **Keine zweite Implementierung der fünf Panels**, kein duplizierter Store mit Backend-Authority, keine neue globale Registry, keine zweite React-App und kein alternatives Wire. Basic/Advanced verwenden identische native Beweisdaten, Protobuf-Messages, Gateway, Zustand-/Adapter-Owner und fachliche Commands. Default Basic nach einem neuen Seitenaufruf; darüber hinausgehende Persistenz nur wenn mit bestehenden UI-Preferences kompatibel und gesondert nachgewiesen. Versteckte Advanced-Sektionen dürfen keine eigenen Hintergrund-Reports/History-Fetches anstoßen, solange nicht geöffnet.

| BSC-Reiter | Basic: betriebsrelevante Kernaussagen | Advanced: ergänzende vollständige Diagnose, mit Details-Disclosure |
|---|---|---|
| **Overview** | Source-/Prozesszustand getrennt, LIVE-Guard, dreiwertige Readiness mit Mode, gewählter Kandidat, G0.7/G0.8, wichtigste aktuelle Sperrgründe | alle Gate-Gründe, Portal-/Builder-/Witness-Drift, Evidence/Commit/Provenienz und technische Checkliste |
| **Launch / BUY** | SELF_A/SELF_B-Auswahl mit tatsächlichem Apply-Status und Tokenadresse, Readiness, V5-Guard/Freigabe getrennt, Versuch/Budget, wichtigster Blocker; AUTO erst wenn qualifiziert und bis dahin ausdrücklich disabled | PREPARE/DRY1–DRY6, R0, Launch-Path, Quote/Simulation, V5-Feldgruppen, gebundene Hashes, Checks, Latch/Nonce, signierte Grenzen als read-only |
| **Monitoring** | Anzahl **nachgewiesen** gesunder/aktionsfähiger Quellen (mit Nenner und Qualifikationskontext), Venues, Inclusion-Status, letzter verifizierter Launch und tatsächliches G0.8-Akquisitions-/Ordering-Ergebnis; bei fehlendem Native-Owner `UNAVAILABLE/UNKNOWN`, kein fiktives 0 | fünf Einzelquellen mit requested/effective/health/capability, drei Venues, Quelle×Venue, first hash/full/actionable, Adress-/Konfliktprüfung, Builder, Receipt/Reorg, Journal/Release-Send |
| **Analytics** | belegte Beobachtungsfenster, eligible/admitted/acquired + jeweilige Nenner, echte Kosten/netto abgeschlossene Ergebnisse nur mit qualifizierter Evidence, höchstens wenige belegte Hauptmetriken/Grafiken | alle Source-Race-Verteilungen (p50/p95/p99 und Tail nur mit Samples), Filter, Journals, vollständige Coverage, Konkurrenz, Tax/Fees, original Producer-Witness und Konfidenz/Unvollständigkeit |
| **Wallets** | Launcher/Buyer-Adresse, Guthaben **mit Snapshot-Alter** und Quelle, verfügbare/reservierte Mittel sofern nativ beweisbar, Finanzierungslücke | getrennte Nonces, Limits, Gas-/Budgetreserve, Exposure, historische Snapshots, Original-Evidence und finalisierte Token-Receipts; keinerlei Wallet-Key-/Send-UI |

**Kritische Zustände in BEIDEN Modi immer sichtbar:** `BLOCKED` vs. `STOP` vs. `PASS_READ_ONLY`, `UNARMED`, Quelle `DETACHED` vs. Stale-Snapshot, Qualifikations-/Version-Drift, fehlende Original-Evidence, `UNKNOWN/UNQUALIFIED/INCOMPLETE`, offene/unklare wirtschaftliche Intents sowie tatsächlich freigegebene Modi. Ein Basic-Screen darf **niemals** positiv/neutral wirken, wenn Advanced einen BLOCKER zeigt. Keine unbekannten Beträge als `0`, keine DRY-Gewinne als Realized PnL, kein N/N+1 als sub-ms-Witness. Eventuelle Quelle-/Venue-LIVE-Schalter bleiben auch in Advanced bis zur nativen BSC-Phasenfreigabe gesperrt. Der Anzeigewechsel ändert **weder** Berechtigungen noch native Konfiguration, Session-Gates oder Audit-Verhalten.

**Umsetzungsabnahme:** Paket 0 der [Astra-Reihenfolge](ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md) inventarisiert für alle vorhandenen UI-Werte `basic_summary / advanced_detail / critical_always_visible / native_source / quality / gate`. Paket 1 fixiert die korrekte Statussemantik, bevor Basic gebaut wird; danach Basic in denselben Komponenten schrittweise parallel zur tatsächlich qualifizierten Advanced-Anzeige. Ein einzelner Header-Schalter muss innerhalb von BSC **reiterübergreifend synchron** sein. Sichtbarkeit, Tastatur/A11y, sehr schmale Breiten, Disclosure, No-Fetch-when-closed, Quelle-Detach, fehlende Werte, BSC-Gate-Fail-closed und Desktop-/Mobil-Graphite-Screenshots in beiden Modi testen. Keine Änderung am Project_0-Wallet-Pixelfreeze, keine neuen Daten- oder Wirtschaftsowner und keine Behauptung eines fertigen Modus vor tatsächlicher Implementierung.

## 14. Paket-1-Korrekturpflicht aus Querschnitts-Audit (2026-10-23)

Die autoritative Reihenfolge des bestehenden Frontend-Plans (§§0–13) bleibt bestehen. Der neue **untergeordnete Prüfbeleg** [BSC_FRONTEND_QUERSCHNITT_AUDIT_2026-10-23.md](BSC_FRONTEND_QUERSCHNITT_AUDIT_2026-10-23.md) erfasst AU-01–AU-20. Die verbindliche Korrekturreihenfolge ist in [ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md §E](ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md) definiert. **Keine neue App, keine zweite Realtime-/Control-Plane, kein Ersatz der bestehenden fünf Panels.** Bereits in §12/§13 beschriebene Basic-/Advanced-Arbeit beginnt nicht vor Status-/Push-/No-Polling-E2E-Gates.

**Systemweite Vertragsverletzung:** Das wiederkehrende Browser-HTTP-GET in `frontend/src/core/realtime/readinessControl.ts` **und** `captureControl.ts` verstößt gegen [GATEWAY_WIRE_PROTOCOL_V1_3.md](GATEWAY_WIRE_PROTOCOL_V1_3.md), §§1/7/9. Prozess-/Controller-/Run- und Command-Receipt-Wahrheit muss über vorhandene Native→Protobuf→Gateway-STATE-Architektur gelangen; BSC-`jobs` ist bereits teilweise vorhanden. **Erst Push-Vertrag/Controller-Proof schließen, danach Polling entfernen.** Kein Browser-Hotpath und keine neuen WS-Clients. Erlaubt bleiben initialer Session-GET, explizite sichere Command-POSTs, ein gezielter Lookup nach verlorener Antwort und ausdrücklich aufgerufene Logs; Native-Polling/launchd-Beobachtung/WS-Heartbeats sind davon unabhängig und nicht pauschal verboten.

**Sperren und Tests:** `BLOCKED` darf weder im nativen Python-Job-Wrapper noch Rust-Projektor zu `STOP` konvertiert werden. Aktuelle Candidate-/Attempt-/Chain-/Version-Evidence muss für gemeinsame Gate-Urteile identisch gebunden sein; fail-closed bei Mismatch. Fehlende Beweise als `UNKNOWN/BLOCKED/HISTORICAL`, niemals als positiver Wert. Testpflicht: No-Polling BSC+P0, korrektes Lifecycle/Command-Reconciliation, SourceDetach/STALE/Reattach, Rust/Protobuf/TS-Goldens, BSC-Evidence-Identität, zeitlich gültige V5-/Portalgültigkeit, Secret-Redaction sowie unveränderte Graphite-/Project0-Wallets-Darstellung. Dokumentation allein ist kein `VERIFIED`.

**Backend-Phasengrenze:** Die Korrektur betrifft F0/Paket 1 (Status-/Transport-/UI-Safety) und keine wirtschaftliche BSC-Ausführung. Source/Venue-LIVE, FOREIGN_AUTO, C1/C2, Mainnet-SEND und produktive EVENT-Catch-up-Phasen bleiben unautorisiert bis zum regulären BSC-Gate. Kein LIVE-Bypass durch Basic oder Advanced.

## 15. Korrigierter F0-VAR-Abnahme-/Paket-1-Handoff (2026-10-24)

**Der Hauptauftrag bleibt F0-VAR:** Bestehende Native-BSC-Variablen-/Protobuf-Inventare felderweise gegen Reporter, Gateway, vorhandenen BSC-Adapter/Store und alle fünf Panels vervollständigen und korrekt verbinden. Die ergänzende Feldmatrix ist keine zweite native Autorität. Technische Defekte AU-01–AU-20 sowie N1/N3/N4/N5 sind ein **begrenzt abzuarbeitendes Paket 1**, nicht der Ersatz für die Variablenverdrahtung. Exakte Korrekturreihenfolge und Acceptance: [ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md §E](ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md). Der Master §§0–14, alle bestehenden Packages und Graphite bleiben erhalten.

**Readiness-Status ist nicht Prozess-Lifecycle:** Native `BLOCKED/STOP/PASS_READ_ONLY` werden von `readiness_job.py` heute über Exit 2 zu `STOP` verdichtet. `publisher.rs::job` **und** `gateway/src/control.rs::observe_project` zählen `BLOCKED` derzeit nicht als regulären Abschluss. DBREC-Originalstatus und `mode` separat erhalten; `STOPPED/FAILED` des Prozesses unabhängig bewerten. G0.7-Klassifikationen/ G0.8-`PASS/ADAPT/STOP/UNRESOLVED` vollständig in Protobuf-Kommentaren/TS-Severity/Goldens berücksichtigen, string-Feldnummern bleiben unverändert. G0.7-/G0.8-Evidenz nur bei belegter gemeinsamer Identity als gemeinsamer Erfolg.

**Keine Phantom-Clock-/RPC-Freigabe:** GUI-`readiness_job.py` isoliert die Umgebung, setzt Public BSC RPC und liefert bislang keine geforderten `DARKBOT_G07_CLOCK_ID`/ `DARKBOT_G07_BOOT_ID`; der aktuelle GUI-Pfad hat damit `CLOCK_CONFIG=BLOCKED` unabhängig von RPC. Vertrauenswürdigen Owner-/Boot-Kontext qualifizieren oder als BLOCKED anzeigen, niemals Sicherheitschecks deaktivieren.

**Das BSC-Job-STATE ist bereits teilweise gepusht:** `projection.rs` erstellt `jobs=[]`, `publisher.rs` fügt vor dem Wire-Send den Readiness-`job()` hinzu. Noch fehlende Controller-Availability/Command-Receipts auf bestehender Gateway-/Native-Eigentümergrenze abgleichen. Danach periodische Browser-HTTP-Prozess- und Receipt-GETs entfernen; reguläre GETs nach erfolgreichem Command-POST sind ebenfalls keine erlaubte Lost-ACK-Reconciliation. `PENDING_RECONCILE`/Kandidaten-Apply bis zum Native-Result offen halten. Globale `/`-Overview nutzt nur `overview.aggregates`; internes BSC-Overview nutzt bei Bedarf bestehende panelgebundene Topics. Keine zweite Verbindung/Engine/Store.

**Abnahme:** echte Rust/Proto/TypeScript/Browser-Tests mit vollständigem Source-SHA, Exitcodes, Start/Stop, validierter Readiness, A→B, Lost-ACK, no Browser-HTTP-Polling, Gate-Identity, Data-age vs. SOURCE_DETACHED und Secret-Redaction. Solange nicht tatsächlich getestet: `NOT_VERIFIED`. Basic/Advanced bleibt reine spätere gemeinsame Ansicht, nicht Voraussetzung für F0-Inventur. Keine C1/C2-/LIVE- oder Mainnet-Freigabe.

