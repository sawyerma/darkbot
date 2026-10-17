# DarkBot Gateway Stream & Command Protocol v1 — verbindlicher SOLL-Vertrag

**Stand:** 2026-10-17. **STATUS: SPECIFICATION ONLY / NOT IMPLEMENTED / NO LIVE AUTHORIZATION.** Dieses Dokument präzisiert und übersteuert ältere Aussagen in [DARKBOT_GLOBAL_FRONTEND_ARCHITECTURE.md](DARKBOT_GLOBAL_FRONTEND_ARCHITECTURE.md), [CONTROL_PROTOCOL.md](CONTROL_PROTOCOL.md) und [V0_FLEET_IMPLEMENTATION_PLAN.md), **soweit** diese einen vollständigen globalen Snapshot, eine globale Sequenz oder sofortigen Full-Resync für jede Lücke verlangen. Die finanziellen, Rust-, FREE_ONLY-, JFI- und Sicherheitsverträge behalten Vorrang. Graphite-V0-Design und Wallet-Look bleiben unverändert.

## 1. Fest entschiedener Datenfluss

Ein einzelner **WebSocket-Transport** zwischen Browser und einem **dünnen lokalen Control Gateway**; **separate** Project_0-/BSC-/Solana-Engine-Owner. Die Engines bieten typisierte Projektionen und nach Projekt autorisierte Commands; das Gateway verteilt und puffert **nur Anzeige-/Control-Nachrichten** und führt niemals Trading-, Wallet-, Preis-, Entscheidungs-, Signatur- oder Journal-Logik aus.

- **Keine UI-Polling-Timer, keine periodischen REST-Status-GETs, keine WebSocket-Verbindung pro Bot.**
- **Route-scoped Topics statt globalem Vollzustand.** Auf der globalen Overview wird nur \`overview.aggregates\` abonniert; beim Projektwechsel werden nur benötigte Projektthemen aktiviert. Bestehende \`/solana\`-Route bleibt.
- **Topic-spezifische Sequenz und Generation**, serverseitiger begrenzter Replay-Ring. Klare \`SUBSCRIBE\`, \`UNSUBSCRIBE\`, \`RESUME\`, \`SNAPSHOT\`, \`STATE_DELTA\`, \`EVENT\`, \`RESET_REQUIRED\`, \`ACK\`, \`ERROR\`, \`HEARTBEAT\` Frames.
- **State vs. Event sind unterschiedliche Semantikklassen.** Kritische Events stammen aus **dauerhafter projektspezifischer Evidence**; Gateway-Puffer allein reichen nicht als Eigentums-/Verlustschutz.
- **Mutationen laufen nicht über State-/Event-Subscriptions**, sondern über separaten projektgebundenen, autorisierten Command-Endpunkt mit idempotenter Request-ID und Backend-Receipt.
- **Wire-Protokoll: Protobuf mit Schema + Codegen**, Rust \`prost\` und TypeScript \`protobuf-es\` (konkrete Toolversions/Pins erst nach Codegen-Kompatibilitätsprüfung, dann im Lockfile fixieren). **Ein** Gateway-Protokoll für alle Projekte; P0 bleibt produktiv intern native/binär/JSON-frei.
- Das Gateway erstellt keine Common-Trade-Engine, keine zentrale PnL-Wahrheit und keine projektübergreifende Wallet-Schnittstelle.

## 2. Benannte Topics und Route-Bindung (v1-SOLL)

| Topic | Type | Subscriber / projektspezifischer Owner |
|---|---|---|
| \`overview.aggregates\` | STATE | Route \`/\`; ausschließlich **read-only** aggregierte Projektstatus und projektbezogene finanzielle Felder, keine vollständigen Projekt-Snapshots |
| \`project0.status\`, \`project0.health\` | STATE | P0 Overview / Health; echter Rust-Status, Boot/Provider/Slot/WSS/Latency/Writer |
| \`project0.liquidator.state\` | STATE | P0 Liquidator; erst **ein** realer Bot, später mehrere; Frontend-Entwürfe sind keine aktiven Instanzen |
| \`project0.capture.state\` | STATE | P0 Capture; unfiltered read-only capture progress |
| \`project0.capture.events\` | EVENT | P0 Capture/Analytics nach Bedarf; vollständige Ereignisse aus qualifizierter .p0j-/Capture-Quelle, sechs Marker sind Report-Filter und nie Capture-Filter |
| \`project0.execution.events\` | EVENT | P0 Liquidator/Analytics; nur tatsächlich persistierte Finality-/Statusereignisse |
| \`project0.wallets.state\` | STATE | P0 Wallets und derselbe Finance-Snapshot für P0 Overview |
| \`project0.analytics.jobs\` | EVENT | P0 Analytics, Job-Status und Ergebnisreferenzen; vollständige historische Reports paginiert separat |
| \`bsc.status\`, \`bsc.launch.state\`, \`bsc.monitoring.state\`, \`bsc.wallets.state\` | STATE | BSC jeweilige Reiter; BSC-Owner und G0.7/G0.8-Gates |
| \`bsc.execution.events\` | EVENT | BSC Monitoring/Evidence aus eigenem beständigem BSC-Journal |
| \`solana.status\` | STATE | bestehender Solana-Bereich, sobald dafür ein Status-Adapter existiert |

**Project-ID** ist separat und strikt typisiert: \`project_0\` / \`bsc\` / \`solana\`. Topic-Namen sind Wire-Aliase, kein Recht, Daten zwischen Engine-Domains auszutauschen. Unbekanntes Topic, Topic ohne Capability oder unzulässiges Projekt → explizites REJECT, nicht stiller Fallback. Ein künftiger Bot-Schwarm ergänzt neue **Bot-IDs und Daten innerhalb der P0-Domain**; keine neue physische WS-Verbindung pro Bot. Bei hohem Durchsatz Listenstatus als STATE, fachliche Execution-Ereignisse über paginierte EVENT-Cursor.

**Route-Subscription-Regel:** \`/\` → ausschließlich \`overview.aggregates\`; \`/project-0\` → \`project0.status\` plus nur aktuell geöffnetes Panel-Thema; P0 Overview darf \`project0.wallets.state\` für dieselben Total-Capital/Realized-Net-PnL-Zahlen abonnieren, aber **ohne** sämtliche Wallet-Details in die globale Übersicht zu laden. \`/bsc\` → \`bsc.status\` plus aktives Panel. Devtools/Health nur explizit aufgeklappt. Beim Unmount UNSUBSCRIBE und Subscribers ref-counted, damit zwei UI-Komponenten dieselbe Topic-Subscription teilen können. Navigation/Status der separaten Domains bleibt isoliert.

## 3. Envelope, Generation, Sequenz und Snapshot

Schema-\`v1\` verlangt ein eindeutig versioniertes Frame-Header:
\`protocol_major, protocol_minor, connection_id, project_id, topic, topic_generation, topic_seq, frame_kind, snapshot_id?, state_revision?, event_id?, event_cursor?, command_id?, correlation_id?, backend_schema_hash?, as_of_slot?, observed_at_unix_ns?, payload\`. Geld/Slot/Nanosekunden exakt als \`uint64\`/\`sint64\`/Bytes oder deklarierte dezimale Ganzzahldarstellung; **niemals** als JS Number casten. Payload wird durch \`oneof\` pro Frame-Kind/pro Projektdomäne ohne freie dynamische Werte typisiert; Proto Field-IDs dürfen nicht wiederverwendet werden. Major-Mismatch FAIL_CLOSED, minor optional nur mit nachweisbar kompatiblen Feldregeln. Generate Rust-/TS-Clients aus **derselben gepinnten .proto-Spezifikation**; Server- und Browser-Golden-Bytes/Schema-Hash testen.

**Sequence per (topic, generation):** \`topic_seq\` wächst pro **veröffentlichtem** Zustand/Event; Coalescing von State ist nur **vor** dem Zuweisen einer Sequenz zulässig. Pro Verbindung existiert keine globale Sequenz und keine globale Head-of-Line-Blockade auf der Applikationsebene. Weder die reine Transportsequenz noch die Auslieferung bestätigen, dass ein kritischer Event dauerhaft geschrieben wurde. Jede Topic hat eigene vordefinierte Ring-/Byte-/Time-Budgets und eigene Cursor-Fehlerzustände.

**Subscribe racefrei:** Gateway nimmt \`SUBSCRIBE(topic, known_generation?, last_applied_seq?)\` an, bindet eine **atomare** Baseline mit \`snapshot_id/state_revision/through_seq/generation\` und liefert dann nur Deltas \`seq > through_seq\`. Parallel entstehende Frames werden gepuffert, bis der Snapshot am Client installiert ist; kein Delta darf vor seinem Snapshot als gültig angewandt werden. EVENT-Topics erhalten eine explizite \`start_cursor\`/Replay-Policy und Replay-Gap-Information.

**Resume-Fall:** Reconnect sendet \`RESUME(topic,generation,last_applied_seq,last_durable_event_cursor?)\`. Ist der Topic-Ring vollständig, liefert Gateway geordnete Frames ab Cursor (nach ACK/APPLIED auf dem Client); fehlt die Generation/liegt der Cursor vor der Retention, gibt es \`RESET_REQUIRED(reason)\`. STATE → neuer vollständiger **nur Topic**-Snapshot. EVENT → zuerst **durabler, autorisierter Journal-/Event-Cursor-Catch-up**, paginiert, mit ausgewiesener Lücken-/Proof-/Pruning-Qualität. Bei nicht mehr rekonstruierbaren historischen Ereignissen explizit \`INCOMPLETE\`, **niemals** „nichts passiert“ oder erfolgreicher Resync. Keine UI-Echtgeldaktion wegen einem Refresh.

**State-Deltas** referenzieren \`base_revision\` und \`next_revision\` sowie Snapshot/Generation. Unbekannte Basis → Topic als STALE markieren und gezielt neu synchronisieren, keine unvalidierten Patches. EVENT-Cursor sind in der Engine gebunden, run-/source-/journal-spezifisch und im Gateway kein neuer eigenständiger Evidence-Owner. Auf einem Neustart erhöht sich die Topic-Generation; gespeicherte Cursor bleiben nur bei autoritativer Quelle und expliziter Kompatibilität verwendbar.

## 4. Zuverlässigkeit, Backpressure und Heartbeats

| Topicklasse | Puffern / verwerfen | Wiederherstellung |
|---|---|---|
| STATE: Health, p95, Saldoanzeige, Botstatus, Queue-Statistik | **Zwischenstände vor Publikation koaleszieren**, bounded queue; bei langsamer Verbindung Client als STALE kennzeichnen. Nicht blind Drop ohne Sequence- und Revisionskorrektheit | Resume im Ring; falls ungeeignet ausschließlich Topic-Snapshot |
| EVENT: Liquidation, BUY, TX, Journal-Status, kritische Jobtransition | **Quelle zuerst durabel persistieren** (nach dem jeweiligen projektspezifischen Gate); Gateway-Ring bounded, keine unbounded Browser-Queue; bei Überlauf Stream pausieren/degradieren statt stille Events zu überspringen | Reconnect/Replay aus echtem projektspezifischem Journal-Cursor; wenn Evidence fehlt → INCOMPLETE, keine Exactly-once-Zusicherung |
| COMMAND receipt / lifecycle | Backend schreibt idempotent + quittiert; UI darf nicht auf dem Browser allein als erledigt markieren | Correlation/command_id erlaubt verzögerte oder verlorene ACKs eindeutig wiederzufinden |

**Genauigkeit:** „Events gehen nie verloren“ darf nur als **Backend-Durability-Verpflichtung** gelten, nachdem ein qualifizierter Writer die Evidence erfolgreich bestätigt hat. WebSocket-Push ist at-least-once Auslieferung **mit möglichen Duplikaten**, nicht End-to-End exactly once. Dedupe anhand unveränderlicher \`event_id\`/Cursor und \`topic_generation\`, kein Gebühren- oder Transaktionsresultat aus einem temporären WS-Event ableiten.

**Heartbeat:** browserfähiger Application-Level \`HEARTBEAT/PONG\` über den WebSocket, vorgeschlagene Startwerte alle 15s, 2 verpasste Antworten → \`STALE\`, Verbindung neu aufbauen; konkrete Zeiten nach Tests freigeben. Der Browser kann keine WS-Ping-Frame-API benutzen, deswegen Application-Ping. Das ist **kein Markt-/REST-Polling**. Um Verbindungen mit falschem Zustand zu erkennen, Gateway sendet periodisch \`STATE_DIGEST(topic, generation, state_revision, hash)\` als Push (z. B. 60s; Kosten messen). Der Digest muss über **kanonisch deterministische Zustandsprojektionen** von Rust und TS identisch berechenbar sein, mit Golden Tests; Protobuf-Serialisierungsbytes selbst sind ohne spezifizierte deterministische Sortierung kein zuverlässiger plattformübergreifender Hash. Kann Client keinen vereinbarten Hash prüfen, muss er das als \`UNVERIFIED_DIGEST\` ausweisen, nicht „matches“.

**Ressourceninvariante:** Gateway-Reader/Reporter off hotpath, bounded channels; Browser-Drosselung, Offline-Hintergrundtab oder überlastetes Netz **blockieren weder** P0 StateOwner/JournalWriter noch BSC-BUY. Separate Project-/Topic-Quotas, keine zusätzliche kostenpflichtige Feedpflicht. Event-Durchsatz, Ringbudget, Dauer und Rate vor Produktivnutzung benchmarken.

## 5. Separater Command-Weg (nicht WebSocket-Subscribe)

- **Mutation über projektgebundenes HTTP POST/RPC** auf denselben Gateway-Host (kein direkter ungesicherter Browser-RPC gegen Engine, kein WebSocket-Event als impliziter Befehl). Binär typisierter Protobuf-Body; optional dieselben DTOs für native \`p0ctl\`-Controlgrenze; **keine** interne Rust-JSON-Queue/Control-Speicherung. HTTP wird nur bei konkreter Nutzeraktion oder bewusst autorisiertem Job benutzt, nicht pollend.
- Header/Body: \`project_id, action, request_id/idempotency_key, expected_revision, resource_id, auth_context, capability_version, payload\`; validierter \`Origin\`, AuthN/AuthZ, CSRF-/SameSite-/DNS-rebinding-/Localhost-Bindung und durchgesetzte Projekt-/Wallet-/Chain-/Gate-Bindung.
- Backend bestätigt **\`REJECTED(reason)\` oder \`ACCEPTED(command_id, persisted_revision)\`**. \`ACCEPTED\` heißt nur „angenommen“, **nicht** ausgeführt/erfolgreich/on-chain. UI = \`PENDING\` bis \`COMMAND_TERMINAL\`/State-Revision über den **autoritativen Projekt-State-Push** bestätigt wird; \`FAILED\` und \`UNKNOWN\` sind echte terminale/klärungsbedürftige Fälle, keine grünen Anzeigen.
- Um verlorenes ACK oder einen WS-Race zu behandeln: auf Reconnect **einmalige** gezielte Abfrage \`GET /commands/:command_id\` (read-only) bzw. idempotentes Nachreichen mit **demselben** Schlüssel, nicht ein periodischer Browser-Poll. Bei Geldbewegungen \`UNKNOWN\` → nachprüfen, **nicht** erneut senden.
- **Start/Stop** einer Observe-Aufnahme und eigens qualifizierte read-only Jobs dürfen nach vorhandenem Projektgate erlaubt sein. BSC G0.7 BUY, P0 FE-06 Submission und Wallet-Sends benötigen **jeweils explizit ihre fachliche, unverändert geltende separate Freigabe**; das Gateway darf keine solche Autorität erzeugen. Browser darf nie rohe Signer-/TX-Bytes beliebig einspeisen.
- Commands können wegen fehlendem Backend/Capability \`NOT_IMPLEMENTED\` liefern. Weder eine UI-Schaltfläche noch ein protobuf-Schema beweist Ausführbarkeit.

## 6. Typsicherheit, Registry, Store und Komponentenstruktur

\`\`\`text
protocol/                             SOLL, nur das deklarative .proto + codegen config
├── schemas/darkbot/control/v1/*.proto  SOLL - common envelope, topic control, commands
├── schemas/darkbot/project0/v1/*.proto SOLL - P0 typed read DTOs
├── schemas/darkbot/bsc/v1/*.proto      SOLL - BSC typed read DTOs
└── README.md                          SOLL - ownership, codegen pins, version policy
frontend/
├── src/core/realtime/                IST JSON WS client; migrieren, nicht parallel duplizieren
│   ├── RealtimeClient.ts             EIN Browser-WS, Subscription Registry
│   ├── TopicSubscriptions.ts         SOLL scoped subscribe/unsubscribe/ref-count
│   └── TopicResync.ts                SOLL per topic ring resume/snapshot fallback
├── src/core/projects/
│   ├── registry.ts                   SOLL statisch typisierte Projektliste
│   └── contracts.ts                  SOLL nur gemeinsame READ-Projektstatusfelder
├── src/features/overview/
│   └── ...                           SOLL nur overview.aggregates
├── src/features/project0/
│   ├── store/Project0Store.ts         SOLL Domain-Store für P0
│   └── ...                           IST Graphite V0 unverändert
├── src/features/bsc/
│   ├── store/BscStore.ts              SOLL Domain-Store für BSC
│   └── ...                           IST noch Platzhalter
└── src/shared/ui/graphite/
    └── tokens.css                    SOLL aus akzeptierter V0 exakt extrahieren
\`\`\`

**Store-Isolation:** pro Project jeweils **eigene** State- und Event-Typen, Reducer/Selectors, getrennte Version/Generation und \`useSyncExternalStore\` mit stabilen selektierten Werten (oder Zustand-Selektoren). \`core/realtime\` transportiert Byte-Frames und Topic-Routing, berechnet aber keine Project0-Wallets und importiert keine BSC-Trade-Pfade. **Kein globaler All-State-Context**, der bei jedem Tick die ganze App rerendert. Event-History und riesige Reports bleiben **nicht** im Live-Store; sie werden nach expliziter Anforderung paginiert geladen, ggf. über TanStack Query. Lazy Loading + Error Boundary pro Feature; BSC-Darstellungsfehler lässt P0 weiterlaufen.

**Registry:** Compile-time \`ProjectRegistration {id,route,label,capabilities,read_status_adapter,lazy_component,allowed_topics}\` mit typsicherer Konfiguration. Für späteres Solana/Projekt genügt Feature-Modul + Registry-Eintrag (evtl. neue deklarative Route aus Registry). Keine Remote-Code-Plugins, keine externen Dynamic Imports und keine automatische Echtgeldberechtigung durch einen Registry-Eintrag. Die heutigen Routen \`/\`, \`/project-0\`, \`/bsc\`, \`/solana\` bleiben erhalten.

**Design-Freeze technisch:** Ein exakter Satz \`shared/ui/graphite/tokens.css\` mit Farbsystem, Radius, Typografie und Spacing wird aus **akzeptierter** V0 extrahiert; P0 \`graphite.css\` zunächst ohne sichtbare Änderungen belassen und danach visuelle Baselines für P0 Overview/Liquidator/Capture/Analytics/Wallets sowie globale Overview/BSC vergleichen. Stylelint-/ESLint-/CI-Regel verweigert neue unkontrollierte Hardcodes in Feature-CSS (bekannte Bestand-Ausnahmen gezielt und befristet), nicht blind ein Projekt-Bild redesignen. UI-Reuse ist Designsystem, **nicht** gemeinsame fachliche Widgets ohne projektspezifisches Ownership.

## 7. Deterministischer Entwicklungseingang, KEINE simulierte Mainnet-Evidence

Offline-Gateway-**Replay** aus bereits vorhandenen, geprüften **echten** und vollständig quellidentifizierten .p0j-Run-/BSC-Journalen (oder zertifizierten aufgezeichneten Wire-Streams) ist sinnvoll. Nur als \`REPLAY/OFFLINE\` sichtbar, nie als aktuelle Gewinner, LIVE oder monetär ausgeführte Aktionen. Aufnahmequelle, Hash, Run-ID, Verifier-Qualität, Zeitachse, Wiederholbarkeit und Reconnect-/Sequenzfehlereinspielung nachweisen. **Keine generierten Mock-Liquidationen und keine künstlichen profitable LIVE-Evidence.** Unit-/UI-Tests dürfen deterministische Transport-Testvektoren mit explizitem \`TEST_ONLY\` verwenden, die nie in Economics-/Release-Ledger aufsteigen.

## 8. Implementierungspriorität und keine Gate-Ausweitung

1. **JETZT Projekt_0 Einzelliquidator zuerst:** C0-E1/4h, PERSIST/JFI/SK, FE-00…FE-07 und alle bisherigen Sicherheitsgates. **Keine Umsetzung des Multi-Bot-Arbiters als Voraussetzung für 4h.**
2. **Frontend-Protokoll-SPIKE** unabhängig und ohne Engine-Mutation: gemeinsames .proto als ein Interface-Contract, prost/protobuf-es Golden-Test, Browser typed decode + scoped subscriptions, minimaler Replay auf archivierter Evidence. Solange dieser Spike nicht nachgewiesen ist, bleibt der bisherige JSON-WS-Client IST, **nicht** produktiv P0-Protokoll.
3. Control Gateway mit **read-only** Status-Snapshot, Topic-Resume, bounded Ring, Heartbeat, ACK, State/Event-Backpressure; project-by-project qualifizieren. Erst danach Commands mit Auth/Gates.
4. BSC UI in demselben Graphite-System ohne BSC-Engine-Änderung durch Designphase; BSC fachlicher G0.7/G0.8-Freeze bleibt unangetastet. P0 V0 Wallet-Layout unverändert.
5. **Fleet/Schwarm nach qualifiziertem Einzelbot:** Profilpersistenz, parallele Strategiebewertung, zentraler Execution-Arbiter, Portfolio-Optimierung werden später getrennt abgenommen und autorisiert.

**Sperren:** keine erneute Autorisierung von LIVE, BUY, Transfers oder Paid Feeds, keine neuen Server ohne notwendigen JFI-/BSC-Vertragsabgleich, kein unnötiger Hotpath-Kopplungspunkt. *Dieser Protokollvertrag ist noch nicht in Rust/TS implementiert, kompiliert oder qualifiziert.*

## 9. Abnahme und Fehlertests

- [ ] Route-topic-Subscriptions und Refcounts (Overview lädt nie BSC-/P0-Voll-Snapshot).
- [ ] Topic A verzögert/überlastet, Topic B läuft weiter; per-Topic generation/sequence/subscribe-snapshot-Race.
- [ ] 1, 2, N Ereignisse hinter Cursor, Ring-Überlauf, Backend-Neustart, partieller Journal-/Cursor-Verlust -> gezieltes RESUME oder explizites INCOMPLETE; Duplikate dedupliziert.
- [ ] State-Coalescing vor Sequence; state_revision/base_revision und per-Client-Drop führen nie zu still falschem Zustand.
- [ ] Heartbeat/Half-open, Browser suspend/resume, State digest silent divergence, Hash-Cross-Language-Golden, WS-ACK-Ausfall, fallende Verbindung.
- [ ] POST zweimal mit gleicher Request-ID = ein persistenter Command/Result; ACK vor/nach Push; UNKNOWN niemals automatisch nochmals teuer senden.
- [ ] P0 / BSC hard-typed isolation, verbotene Topics/Routen/Auth/Origin/CSRF, keine Wallet-Keys/Projektüberschneidung.
- [ ] Browser parse protobuf bytes aus einzigem gepinnten Schema; inkompatibler Major verweigert Betrieb. Keine unverifizierten Protobuf-Versionen behaupten.
- [ ] Domain-Stores nur selektierte Komponenten, Lazy/Boundary; P0 Wallet visual screenshot pixel-/semantic regression. Neues Projekt nur Registry+Feature.
- [ ] Run Replay aus echter Archiv-Evidence markiert OFFLINE, deterministisch, niemals als Live-Gewinn gezählt; JFI intern ohne JSON.
- [ ] Typcheck, build, lint, Browser-E2E, Rust/native tests mit exakten PASS/FAIL/NOT_RUN-Gates, bevor Implementierung als fertig markiert wird.
