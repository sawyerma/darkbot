# DarkBot Global Frontend – verbindliche Architektur v1.0

**Entscheidung:** 2026-10-17 · **Status:** verbindliche SOLL-Architektur; kein zusätzlicher LIVE- oder Backend-Release.  
**Repository:** `sawyerma/darkbot` · **Root:** `frontend/` (das **eine** bestehende React-/Vite-Frontend).  
**Designreferenz:** [Project_0 Graphite V0](V0_GRAPHITE_FLEET.md) · **Phasenplan:** [FRONTEND_PHASENPLAN](FRONTEND_PHASENPLAN.md).  
**Fachliche Autorität bleibt separat:** [Project_0 Phasenplan](../../project_0/docs/PHASENPLAN.md), [Project_0 Status](../../project_0/docs/STATUS.md), [BSC Roadmap](../../bsc/docs/BSC_DUAL_VENUE_ROADMAP.md), [BSC Gate-0-Plan](../../bsc/docs/GATE0_TESTPLAN_SHORT.md), [BSC Status](../../bsc/docs/STATUS.md). Dieser Frontend-Vertrag überschreibt **keines** dieser Gates.

## 1. Verbindliche Produkthierarchie und Navigation

Ein einziges **DarkBot** als Host-Anwendung mit globaler Übersicht und voneinander getrennten Projektbereichen. **Klick auf „DarkBot“ (Logo/Name) führt immer zur globalen Overview**; Auswahl eines Projektreiters oder einer Projektkarte öffnet dessen eigene Projektoberfläche.

```text
DarkBot (ein React-Projekt, ein App-Router, ein globaler Navigationsrahmen)
│
├── /                         Overview / All Projects
│   ├── alle verfügbaren Projekte und ihr tatsächlicher Status
│   ├── aktive Bots / Capture-Runs / Jobs – projektgebunden
│   ├── echte, quellenbelegte Performance / Realized Net PnL
│   ├── Warnungen / Fehler / Health / Latenz / Host-Ressourcen
│   └── Klick auf Projektkarte → jeweilige Projektseite
│
├── /project-0                Project 0 – BESTEHENDE Graphite V0
│   ├── Overview
│   ├── Liquidator
│   ├── Capture
│   ├── Analytics
│   └── Wallets (Balances / Send / Auto-Sweep)
│
├── /bsc                      BSC – EIGENE Fachoberfläche, noch zu entwickeln
│   ├── Overview
│   ├── Launch / BUY          (nur mit G0.7-Gate, keine UI-Entsperrung)
│   ├── Monitoring
│   ├── Analytics
│   └── Wallets              (nach eigenen BSC-Daten- und Freigabegates)
│
├── /solana                   bereits vorhandene Route; nicht ungefragt entfernen
│   └── existierender separater Solana-/Pump.fun-Bereich
│
└── /<future-project>         später registrierbare zusätzliche Projektmodule
```

**Wichtig:** Die globale `Overview` ist **nicht** die `Overview` innerhalb Project_0 oder BSC. Zwei Navigationsebenen sind erlaubt, aber **keine zusätzliche Sidebar**, keine verschachtelten konkurrierenden Hauptmenüs und keine zweite React-Anwendung. Projektübergreifende Navigation benutzt die bestehende `shared/layout/Navigation.tsx`; projektinterne Reiter bleiben in den jeweiligen `features/*`-Modulen.

Die vorhandene `/solana`-Route ist **IST**; die neue Sollansicht mit zunächst besonders sichtbaren Karten für Project_0 und BSC darf `/solana` nicht löschen oder versehentlich mit Project_0 zusammenführen. Neue Projekte erscheinen erst, wenn ausdrücklich registriert und mit passenden Readiness-/Statusquellen versehen.

## 2. Aussehen: ein Designsystem, fachlich unterschiedliche Projektseiten

- Die akzeptierte **graphitgraue V0** ist die gemeinsame optische Richtung: neutrales dunkles Graphit, ruhige Typografie, kompakte Linien und Pill-Navigation; kein Navy/Blue-Dashboard, keine überladenen Marketingkarten oder beliebige Umgestaltung beim nächsten Feature.
- **Project_0 exakt erhalten:** die zuletzt akzeptierte V0-Struktur, die Liquidator-Draft-Liste und insbesondere das Wallet-Layout **nicht** umbauen, nur weil das BSC-Modul ergänzt wird.
- **BSC übernimmt Farben, Layout-Rhythmus und primitives**, aber **nicht** die Marginfi-Funktionalität. BSC bleibt Launch/BUY mit Gate-0, G0.7/G0.8, Portal-/Builder-Requalifikation, Monitoring/Evidence und BSC-spezifischen Wallets; weder ein „Liquidator“-Reiter noch Project_0-Strategieprofile werden nach BSC kopiert.
- Globale Overview: übersichtliche Projektzeilen oder dezente Projektkarten mit klaren Namen, Status und Navigation; keine neuen, frei erfundenen Erfolgsmeldungen oder überzähligen Kacheln. Metrikdetails bei Bedarf aufklappbar.
- Theme-Primitives können langfristig aus Project_0 nach `shared/ui/graphite/` **extrahiert** werden, aber **nur visuell regressionsgetestet und ohne gleichzeitiges Redesign**. Zunächst darf die bestehende scoped `features/project0/styles/graphite.css` unverändert bleiben. Globaler Theme-Umstieg darf BSC und Solana nicht versehentlich verändern.
- Eine konsistente englische UI-Beschriftung für Bedienflächen (`Overview`, `Monitoring`, `Wallets` usw.); Dokumentation/technische Phasen dürfen Deutsch bleiben.

## 3. IST-Code – heute vorhanden, heute nicht vorhanden

| Pfad / Symbol | IST | Nicht daraus ableiten |
|---|---|---|
| `frontend/src/app/router.tsx` | Routen `/`, `/bsc`, `/project-0`, `/solana` | Keine dynamische Projekt-Registrierung |
| `frontend/src/shared/layout/Navigation.tsx` | DarkBot-Logo und globale NavLinks | Noch keine projektübergreifende Health-/Statusintegration |
| `frontend/src/features/overview/OverviewPage.tsx` | Statische Karten, allgemeiner WebSocket-Verbindungsstatus | **Keine** echte Aggregation laufender Bots, Gewinne, Jobs, Latenzen |
| `frontend/src/features/project0/Project0Page.tsx` | React Graphite V0 mit fünf Reitern und lokalen Bot-Entwürfen | **Keine** produktive Liquidator-/Fleet-Control-API; Live bleibt gesperrt |
| `frontend/src/features/bsc/BscPage.tsx` | Platzhalter für BSC | **Keine** echte BSC-Bedienoberfläche/BUY-Autorisierung |
| `frontend/src/features/solana/SolanaPage.tsx` | Bestehender Solana-Einstieg | Keine Übertragung der P0-/BSC-Domainlogik |
| `frontend/src/core/api/client.ts::apiGet` | Shared Read-Client, `/api/v1` | Keine fertig implementierten projektspezifischen Control-Endpunkte |
| `frontend/src/core/realtime/RealtimeClient.ts` | WebSocket-Reconnect und Sequence-Gap-Meldung | Keine fertige automatische REST-Resynchronisierung der Projekt-Snapshots |

**Phasengrenze:** Diese Datei dokumentiert SOLL; sie ist **keine Behauptung**, die globale Overview oder BSC-GUI seien schon funktionsfähig.

## 4. Globale Overview: Felder, Status-Semantik, Zuständigkeit

Ein Project-Status-Adapter liefert **nur nachgewiesene, typisierte Lesedaten** in einen gemeinsamen globalen Snapshot:

| Globales Feld (SOLL) | Herkunft | Anzeige / Qualität |
|---|---|---|
| `project_id`, `display_name`, `route`, `kind` | Projektregistrierung | Eindeutige stabile ID, nicht bloß ein Name |
| `health`, `readiness`, `execution_authorization` | Jeweilige Engine/Preflight-Owner | Getrennt ausweisen: `UNKNOWN/STALE/BLOCKED/READY`; `READY` ist keine Echtgeldfreigabe |
| `running_instances`, `active_capture_runs`, `active_jobs` | Autoritativer Projekt-Supervisor | Nur echte Prozesse/Jobs, keine React-Entwürfe als aktive Bots zählen |
| `rpc_latency_p95_ms`, `decision_latency_p95_ms`, `stream_age_ms` | Native Mess-Snapshots je Projekt | Quell-/Zeitfenster/Samplezahl erforderlich, `—` statt Fake-0 |
| `realized_net_pnl_usd_micro` | Finalisierte, projektbezogene Finance-Ledger | `— USD` bei fehlendem FE-07-/G0.8-/Accounting-Nachweis |
| `total_capital_usd_micro` | Projektbezogener Portfolio-Snapshot | Projekt- und Wallet-Ownership eindeutig; nie ohne Preise/Liabilities |
| `warnings`, `last_heartbeat`, `as_of`, `snapshot_id` | Read-only Projektion | Alters-/Stale-Status und Deep-Link zu den Details |

**Keine falschen globalen Summen:** Eine projektübergreifende `Total Capital`- oder `Net PnL`-Summe nur dann anzeigen, wenn alle einbezogenen Projekte die gleiche USD-Bewertungsgrundlage, den definierten Zeitraum, finale Belege und **keine doppelte Wallet-/Transfer-Zählung** haben. Andernfalls getrennte Projektwerte plus `Aggregate unavailable`. Unbekannt ist **nicht** null. Besonders Solana- und BSC-Wallets dürfen nicht vermischt werden; Cross-Chain-Transfers sind zunächst keine Profite.

**Globale Commands bewusst beschränkt:** Hauptseite navigiert und beobachtet. Sie kann später qualifizierte *projektgebundene* Controls anbieten, führt aber **niemals** durch einen globalen „Start all“ oder generische REST-Aktion Echtgeld-BUYs, Liquidationen, Wallet-Transfers oder Auto-Sweeps ohne die jeweilige separate Authorization aus.

## 5. React-Komponentenbaum und zukünftige Projektregistrierung

```text
frontend/src/
├── app/
│   ├── router.tsx                    IST – ein App-Router, global + Projekt-Routen
│   └── providers.tsx                 IST – gemeinsame React Provider
├── shared/
│   ├── layout/{AppLayout,Navigation}  IST – DarkBot-Logo → /
│   └── ui/                           IST – gemeinsam nutzbare Grundkomponenten
├── core/
│   ├── api/client.ts                 IST – gemeinsamer Read-Client
│   ├── realtime/RealtimeClient.ts    IST – WS, noch ohne Resync der Domain-Hooks
│   └── projects/                     SOLL – typisierte Projektregistrierung
│       ├── registry.ts               SOLL – stable id, route, display, capabilities
│       └── contracts.ts              SOLL – vereinheitlichte *Read*-Statusprojektion
├── features/
│   ├── overview/
│   │   ├── OverviewPage.tsx          IST – später echte globale Aggregation
│   │   ├── components/ProjectEntry.tsx  SOLL – ein einheitlicher Projektlink
│   │   └── hooks/useProjectSummaries.ts SOLL – Snapshot + WS/REST Resync
│   ├── project0/                     IST – Graphite V0, unverändert übernehmen
│   ├── bsc/
│   │   ├── BscPage.tsx               IST – Platzhalter → eigenes Graphite-BSC-Modul
│   │   ├── pages/{Overview,LaunchBuy,Monitoring,Analytics,Wallets}.tsx  SOLL
│   │   ├── api/                      SOLL – nur BSC-Status-/Control-Verträge
│   │   └── model/                    SOLL – BSC-Spezifika, keine P0-Engine-Types
│   └── solana/                       IST – bereits vorhandene, separate Route
```

**Erweiterung weiterer Projekte:** `registry` ist zunächst eine einfache **statisch typisierte Liste aktiv unterstützter UI-Module** (nicht dynamisch von beliebigen Backend-Nachrichten ausführbarer Code). Eine Registrierung benennt `id`, `route`, `label`, `status adapter`, `capabilities`; globale Navigation und Overview werden daraus abgeleitet. Für BSC/P0 bestehen die Routen bereits. Nicht implementierte oder nicht qualifizierte Fähigkeiten bleiben in der GUI deaktiviert. Kein Projekt darf ein anderes Projekt per Import, gemeinsamem Signing-Service oder Wallet-Zugriff steuern.

**Keine zweite Frontend-/Backend-Engine:** Gemeinsam sind ausschließlich Layout, Anzeige-Primitives, Statusadapterschnittstellen und Transport. Programmlogik/State, RPC-Pools, Wallets, Journal, Sicherheitsgate, Kapitalreservierungen und Transaktionssendung bleiben **in ihrem jeweiligen Projekt**; keine gemeinsame generische „Trade Engine“ im Browser.

## 6. Aktualisierung und Speichern

**Heute:** `apiGet<T>` und ein WS-Client existieren. Die Projektkarten in `OverviewPage` sind statisch, P0-Bot-Entwürfe liegen nur im React-Arbeitsspeicher. **Nichts davon beweist einen aktiven Bot oder dauerhafte Profilpersistenz**.

**Ziel:**

1. Beim Öffnen von DarkBot eine versionierte **read-only** Liste der autorisierten Projekt-Snapshots holen; fehlende Domains einzeln als `Not connected` zeigen statt den gesamten Host zu blockieren.
2. Ein gemeinsamer WS-Transport ist erlaubt, aber jede Nachricht trägt eine **harte project_id**, Session/Generation, Sequence, as_of und Version. Jeder Domain-Hook verarbeitet nur eigene Ereignisse. Kein P0-Event kann BSC-Status verändern.
3. WS-Gap, Reconnect, Backend-Neustart, abweichende Version oder veralteter Snapshot → betroffenen Projektadapter `STALE` setzen und komplette autoritative REST-Snapshot-Neuladung; keine stillen Deltas nach Lücke.
4. UI-Telemetrie ressourcenschonend drosseln/coalescen; UI-Refresh darf keine RPC-/WSS-Quota oder Live-Execution-Latenz belasten; Unbekanntes bleibt unbekannt.
5. Laufende Jobs und Profile werden ausschließlich von der jeweiligen Backend-Engine mit eigenen identitätsgebundenen Journal-/Version-/Revisionsregeln gespeichert. Die globale Startseite speichert keine Wallet-Keys und erstellt keine eigenständige Finanz-Wahrheit.
6. Vor jeder Control-Aktion ein projektspezifisches Authority- und Safety-Gate. Eine global geteilte Login-Session wäre **kein** universeller LIVE-Freigabeschlüssel.

## 7. Reihenfolge und Blocker (verbindlich)

1. **Jetzt:** Project_0 nach eigenem produktiven Phasenplan **als einen** Liquidator qualifizieren (C0-E1/4h, FREE_ONLY, SK/JFI/PERSIST, FE-00…FE-07, gesondertes Pilot-/LIVE-Gate). Bestehendes Frontend V0 erhalten; keine neue GUI-Anforderung auf den 4h-Critical-Path legen.
2. **BSC fachlich unabhängig:** die eigene BSC-Roadmap bleibt autoritativ; insbesondere G0.7/G0.8-Requalifikation und gesonderte Buy-Freigaben. Eine BSC-GUI wird **später** entlang tatsächlicher BSC-Schnittstellen ergänzt und darf weder P0 noch BSC-LIVE indirekt entsperren.
3. **Globale Overview schrittweise:** Layout/Navigationsdesign kann vorab dokumentiert werden; echte Statuswerte erst nach Adapter- und Read-API-Qualifikation. Die bisherige `/solana`-Route bleibt erhalten.
4. **Bot-Schwarm später:** Mehrfach-Liquidatoren und per-Bot-Strategieprofile sind als **spätere** Project_0-Erweiterung vorgesehen ([Fleet-Vertrag](V0_FLEET_IMPLEMENTATION_PLAN.md)), nicht als Voraussetzung für den ersten Liquidator oder 4h-Test. Beim ersten Bot nur saubere Schnittstellen (StateOwner, Strategievertrag, Wallet/Journal/Receipt), **noch keinen Fleet-Scheduler oder Arbiter für N Bots** entwickeln.
5. **Weitere Projekte** erst mit klarer Domain-Ownership, Projektregistrierung, read-only Statusadapter und eigenen Safety-Gates hinzufügen.

## 8. Abnahmekriterien für eine spätere Implementierung

- [ ] DarkBot-Logo sowie Overview im globalen Header öffnen `/`; Project 0 → `/project-0`, BSC → `/bsc`; globale Karten und Nav-Links führen zu derselben Route.
- [ ] Projektinterne Tabs wechseln nur eigenen Projektinhalt; die bestehende P0-Graphite-V0-Wallet-Ansicht wird per visueller Regression unverändert bewahrt.
- [ ] BSC-GUI sieht nach demselben Designsystem aus, besitzt aber **keine** P0-Liquidationsklassen, Captureschalter oder Wallet-Sends.
- [ ] Eine weitere registrierte Domain erscheint in globaler Navigation/Overview ohne Änderung an bestehenden Domain-Seiten; fehlende Domain-API blockiert andere Domains nicht.
- [ ] Status, tatsächliche Prozessanzahl, Latenz und PnL stammen aus echten projektspezifischen Backend-Snapshots; fehlende Werte zeigen `—`/UNKNOWN, keine Null- oder grünen Demo-Zahlen.
- [ ] Projekt-ID, versionierte Snapshots, WS-Gap/Resync und Stale-Verarbeitung verhindern Übersprechen oder widersprüchliche PnL-Werte.
- [ ] Projektübergreifende Summen berücksichtigen zeitgleiche Bewertung, Eigentum/Transfers und belegte Finalität; sonst keine Summe.
- [ ] BSC und Project_0 behalten vollständig separate signer, wallet, RPC/provider quota, journals, launch/liquidation logic and authorization gates.
- [ ] UI funktioniert mit Keyboard/Screenreader, kleinen Displays; `frontend`-Build, Typecheck, Lint und Browser-Smoke erhalten reale Evidence (vorher nicht auf PASS setzen).
- [ ] Global-Frontend-Implementierung oder Fleet-Vorarbeit wird **nicht** zum zusätzlichen Blocker für P0-4h, FE-06/07 oder BSC-G0.7/08.

**Änderungsdisziplin:** Diese Architektur ist eine **Dokumentationsentscheidung**. Design, Navigationsvertrag, Eigentümer und Gates sind nun fixiert; die tatsächlichen Komponenten/Adapter/API müssen separat implementiert und getestet werden. Änderungen an BSC-, P0- oder Solana-Engine-Dateien erfolgen hier ausdrücklich nicht.
