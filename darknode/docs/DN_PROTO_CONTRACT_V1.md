# DarkNode – DN_PROTO_CONTRACT_V1: eigene Protobuf-Domäne und automatisierte Feld-Abdeckung

**Stand:** 27.10.2026 · **Revision V1.1** · **Status:** verbindliche Planung, `DN-PROTO-001–008 = NOT_RUN`; weder neue Proto-IDs noch neue Runtime-Topics, CI-Jobs oder produktive Publisher sind implementiert. Referenz: [PHASENPLAN.md](PHASENPLAN.md) P0/P5/P7/P8.

## 1. Domain-Isolation und Namespace (P0, noch keine Vergabe)

| Domäne | Protobuf-Paket | ProjectId | Topic-Raum |
|---|---|---:|---|
| Project_0 | `darkbot.project0.v1` | 1 | 10–29 |
| BSC | `darkbot.bsc.v1` | 2 | 30–49 |
| Solana | im gemeinsamen Control-Protokoll als Platzhalter | 3 | 50–69 |
| **DarkNode (Kandidat)** | **`darkbot.darknode.v1`** | **4 (Vorschlag)** | **70–89 (Vorschlag)** |

`DN-D14=OPEN`. Vor Codierung im P0-Governance-Gate Thema ProjectId/TopicId/Owner/STATE-EVENT/ACL, Publisher, Payloadgröße und Source-/Cursor-/Liveness-Semantik durch versionierte Schemas festlegen. Integrationsorte: `protocol/schemas/`, `gateway/build.rs`, `gateway/src/proto.rs`, `gateway/src/topics.rs`, `gateway/src/engine.rs` und `config.rs`, `gateway/src/overview.rs`, `frontend/src/core/realtime/topicTable.ts`, `frontend/src/app/projectRegistry.ts`, `frontend/src/features/darknode/`. Keine Project_0-/BSC-Fachdateien ändern, kein zweiter Gateway oder UI-WS.

Erforderliche Projektionen fachlich aus P3/P5/P7 ableiten, ohne voreilig alle Nummern zu vergeben: Status/Readiness, Datenqualität, Graph/Parameter, Backtest, Wallet/Equity, Position/Margin, Orders/StopCoverage, Steuerung/Quittungen, durable Execution-Events. Ein Topic ohne realen Publisher bleibt `available=false`.

## 2. **Eine** Feldquelle; automatisch generierte Inventur, nicht manuell kopieren

`*.proto` und ihr generierter Descriptor-Set bilden die einzige Quelle für **Feldname, Feldnummer, Typ, Cardinality und Presence**. Semantische Metadaten (Maßeinheit/Währung/Scale, Status-/Quality-Konvention, gewünschter UI-Pfad oder begründeter interner Ausschluss, Domänenbesitzer) werden **einmal** am Feld oder in einer gegen Descriptor-Feldnummern geprüften minimalen Metadatenquelle gepflegt. **V1-Entscheidung:** kleines versioniertes, **gegen Descriptor-Feldnummern validiertes Metadatenmanifest** neben den `.proto`-Dateien; ein Generator liest die mit vorhandenem `buf`/`protox` erstellten Descriptoren und validiert `Message.FieldNumber`, Presence, Einheiten, Scale, UI-Ort bzw. `EXCLUDED_INTERNAL`. Keine neue `FieldOptions`-Extension in V1 und kein zweiter manueller Feldkatalog. Manifest referenziert nur Semantik, die im Proto-Descriptor selbst nicht ausdrückbar ist. Generator und CI-Test sind **noch nicht implementiert**. Eine UI-Zuordnung wie `wallet.equity` ist ein **Soll**, keine Laufzeitevidenz. `CONNECTED_VERIFIED` darf niemals als statische Schema-Annotation vorgegeben werden.

CI generiert daraus `PROTO_FIELD_CENSUS.md` und eine maschinenlesbare Feldliste. Laufzeit-/Anwendungstests prüfen die **tatsächliche** Producer-/Consumer-Abdeckung. Der Generator muss Protobuf-Importe, Message-Presence, Proto3-Defaultwerte, FieldNumber und reservierte Felder korrekt interpretieren; Drift zwischen generiertem Report und aktuellen Schemas = FAIL. Kein handgeschriebenes 20-Spalten-Duplikat.

**Scope der Abdeckung:** Alle nach außen veröffentlichten, für DarkNode-V1 relevanten fachlichen Felder/Node-Parameter/Commands; reine Rust-Implementierungsvariablen brauchen keine UI-Projektion. Für ein nicht angezeigtes Feld ist `EXCLUDED_INTERNAL` samt Grund möglich; `FUTURE_PHASE_BLOCKED` gilt **nicht** als V1-Feature-PASS. Präzise Statuswerte für echte Source/Frontend-Abdeckung: `SOURCE_ONLY`, `WIRE_NO_PUBLISHER`, `WIRE_NO_ADAPTER`, `UI_UNMAPPED`, `MAPPING_BUG`, `EXCLUDED_INTERNAL`, `FUTURE_PHASE_BLOCKED`, `CONNECTED_VERIFIED`. Nur nach **ausgeführtem** Roundtrip darf `CONNECTED_VERIFIED` vergeben werden.

Für Wallet/Order/Risk numerische Typen bewusst festlegen: keine losen Name/Value-Strings für kritische Werte; USDT vs USD, Price, Qty, Notional, Equity, Margin, Funding, Gebühren, signed PnL, Stopmenge und Botbudget mit Fixed-Point/Mantissa/Scale+Currency oder wohldefiniertem Decimal-Wrapper. **Bitget-Exchange-Order-/Trade-IDs als unveränderte dezimale Strings, sofern das API sie als Strings liefert**; `uint64`/int64-Protobuf in JS via `bigint` ohne Number-Konversion. Pflichtgrenzen: `2^53-1`, `2^53+1`, `1498752928271056898`, `u64::MAX`, negative PnL, Überlauf/fehlende Presence, Zero-versus-Unknown und Decimal-Scale/Unit-Mix. TypeScript-Tests müssen unbemerkten Genauigkeitsverlust explizit ablehnen.

## 3. Wiederverwendung der acht geplanten Gates

| Gate | Konkrete spätere Ausführung/Aussage |
|---|---|
| DN-PROTO-001 | Eindeutige ProjectId-/TopicId-Registrierung, keine Cross-Project-Owner/Topic- oder Namespace-Vermischung |
| DN-PROTO-002 | Generator/Validator aus Proto-Descriptor + Schema-Metadaten; kein unbeschriebener **öffentlich verwendeter** Parameter/Feld/Command, begründete Ausschlüsse |
| DN-PROTO-003 | Bestehendes `protocol/scripts/check.sh`, `buf lint/breaking` gegen **echten Vorläufer-SHA**, Codegen Rust/protobuf-es, Schema-ID/Golden-Versionen reproduzierbar |
| DN-PROTO-004 | `gateway/src/topics.rs` und `frontend/src/core/realtime/topicTable.ts` projektscharf 1:1 zu Proto-IDs, Klassen, Payload-Typen und Availability; negative Owner-/Topic-Testfälle |
| DN-PROTO-005 | Rust↔Protobuf↔TS-Goldens für IDs jenseits JS-safe-integer, exakte Geld-/Kontrakt-Einheiten, Presence/Unknown, Node-Parameter, Commands/Receipts |
| DN-PROTO-006 | Producer-/Wire-/Adapter-/Store-/UI-Abdeckung durch **echte Tests, nicht nur kommentierte Pfade**, Status je tatsächlich verifizierter Feature-Gruppe; kein vergessener Feldtransport |
| DN-PROTO-007 | Typed Commands, ExpectedVersion/CAS, Autorisierung, NO_ENTRY/Risk-Owner, Idempotenz/Restart, keine unbekannte Mutation/kein Frontend-Key |
| DN-PROTO-008 | End-to-End + Negativfälle: Gateway→UI-Wert, stale/null, fremde ProjectId, Schema-Version, Reconnect, Fill/Stop-Reconcile, DOM-Placement und fehlender Publisher |

**CI-Festlegung:** bestehende Workflows `.github/workflows/gateway.yml` und `.github/workflows/project0-status.yml` um DarkNode-spezifische Vertragsjobs erweitern bzw. einen **ergänzenden** `darknode-contract`-Job an gleicher Pipeline anbinden, ohne Projekt_0/BSC-Laufzeitregeln zu übernehmen. Erfolgreiche Required-Checks müssen `protocol/scripts/check.sh --check`, Generator-/Descriptor-Census, Gateway-Rust-/Repo-Goldens, TypeScript-Check, Adapter-/UI-Tests und projektübergreifende Ablehnung abdecken. **Trigger-Matrix als Negativtest** für reine `darknode/native/**`-Änderung, Schema, Gateway, Frontend und Workflow; Markdown-only bekommt geeigneten Doc-Check. Bestehende Pipeline/Tests weiterverwenden; künftig die DN-PROTO-Prüfungen auch bei **reinen DarkNode-Rust- und Schema-/UI-/Gateway-Änderungen** als Required Checks auslösen. Änderungen an `darknode/**`, `protocol/**`, `frontend/**`, `gateway/**` und einschlägigen Workflows in der Trigger-/Pfadmatrix testen. Reine Planänderungen brauchen Dokumenten-/Linkchecks statt vorgetäuschter Rust-/UI-PASS. Gute Feldeinträge ohne Runtime-Producer/Consumer sind `NOT_RUN/IN_PROGRESS`.

## 4. Abnahmeregel

Bei V1-relevantem neuem Feld/Parameter müssen Source/IR → Schema → Rust-Publisher → Gateway → TypeScript-Adapter/Store → konkrete UI oder bewusst erlaubter Ausschluss zusammen mit Generator und zugehörigen Tests im selben **fertigen** Änderungspaket überprüfbar sein. Gestufte Entwicklung erlaubt; Zwischenstand nicht als fertiger Commit abnehmen. Jedes Urteil mit SHA, Test-ID, Testlauf, Abdeckung und Gründe; `P8/P9` bleiben bei erforderlichem `FAIL/BLOCKED/NOT_RUN` gesperrt.

**Keine Implementierung durch dieses Dokument.** Keine Änderung an BSC/Project_0/Protobuf/Gateway/Frontend und keine Live-Order-/Key-Aktion.
