# DarkBot Frontend — gemeinsamer Backend-/Frontend-Vertrag

**VERBINDLICH FÜR PROJECT_0 UND BSC.** Beide Projekte verwenden das **identische F0-VAR-Vorgehen und dieselbe technische Übergabekette**, nicht identische fachliche Felder. Diese README ist der gemeinsame Einstieg für alle Backend-, Gateway- und Frontend-Coding-Agenten. Sie ergänzt, ersetzt aber nicht den einzigen aktiven [Frontend-Master-Phasenplan](docs/FRONTEND_PHASENPLAN.md), den [Wire-Vertrag v1.3](docs/GATEWAY_WIRE_PROTOCOL_V1_3.md) und die [Governance](docs/GOVERNANCE_UND_SICHERHEIT.md). Die jeweiligen nativen Projektphasenpläne bleiben fachlich maßgeblich.

## 1. Ein Frontend, ein Gateway, getrennte native Owners

| | Project_0 | BSC |
| --- | --- | --- |
| Projektkennung / ID | `project_0` / `PROJECT_ID_PROJECT_0 = 1` | `bsc` / `PROJECT_ID_BSC = 2` |
| Native Source | `project_0/native/` | `bsc/engine/` |
| Protobuf-Namespace | `darkbot.project0.v1` | `darkbot.bsc.v1` |
| Topics / Nummernraum | `project0.*` / 10–29 | `bsc.*` / 30–49 |
| React-Domäne | `frontend/src/features/project0/` | `frontend/src/features/bsc/` |
| Bestehende fünf Reiter | Overview, Liquidator, Capture, Analytics, Wallets | Overview, Launch / BUY, Monitoring, Analytics, Wallets |

Die **globale Overview** unter `/` ist getrennt von den Projekt-Overviews und verwendet nur nachweislich verfügbare Aggregate. `/solana` bleibt eigenständig. Keine neue SPA, kein neues Reiterkonzept, keine getrennten Gateways. Die kanonischen Projekt-/Topic-IDs sind in [topics.proto](../protocol/schemas/darkbot/control/v1/topics.proto), [Gateway-Registry](../gateway/src/topics.rs) und [Frontend-Topic-Tabelle](src/core/realtime/topicTable.ts) vorhanden. Keine neue Nummerierung oder zweite Feldautorität.

## 2. Exakt dieselbe technische Übergabekette

```text
Project_0 Rust / BSC Rust  (jeweils alleiniger fachlicher Owner)
    → bestehende native Typen / Record-Schemas / Config / CLI
    → explizit ausgewählte, sanitisierte öffentliche Projektion
    → projektspezifisches Protobuf (darkbot.project0.v1 / darkbot.bsc.v1)
    → nativer Reporter / Engine-Feed
    → gemeinsames Gateway: Topics, Source-Liveness, STATE / EVENT
    → ein same-origin Protobuf-WebSocket
    → projektspezifischer React-Adapter / Zustand-Store
    → fünf bereits vorhandene Reiter je Projekt
```

**Rückweg bei Steuerung:** Reiter → typisierter Command via separatem Gateway-POST → explizite Gateway-Allowlist/Authentisierung/Idempotenz → nativer Controller mit fachlichen Gates → Receipt + neue öffentliche Zustandsprojektion. Ein angenommenes Kommando ist **keine** ausgeführte Transaktion. GUI/REST/WS liegen niemals im Trading-Hot-Path. Der Browser ist kein zweiter Risk-/Signer-/Wallet-/Trading-Owner.

**Wire-Regeln:** vollständige STATE-Snapshots nach (generation, seq), Source-Qualität/Detach/Stale; journalgebundene EVENT-Cursor nach (journal_epoch, durable_cursor) mit Replay/Dedupe. Ein reserviertes Topic oder Protobuf-Feld bedeutet **nicht**, dass ein Producer/Adapter schon existiert. Keine zyklischen Browser-GETs als Ersatz für den gemeinsamen Push-Vertrag; vorhandene Polling-Reste sind ein dokumentierbarer IST-Fehler, keine neue Architektur. History/Reports dürfen gezielt on demand geladen werden.

## 2a. Festgelegte Speicherformate und Protobuf-Grenze

**Ein Dashboard, zwei unabhängige Rust-Backends, ein gemeinsamer Protobuf-/Gateway-/WebSocket-Transport.** Die selbst entwickelten internen Dateiformate bleiben intern; **Protobuf ist der Transport öffentlicher Projektionen und keine neue Journal- oder Speicherdatei.**

| Format | Internes Projekt / Verwendung | Eigenentwicklung | Tatsächlich verwendete Standardverfahren |
| --- | --- | --- | --- |
| **DBREC001** (DBREC) | BSC: typisierte, versionierte Qualifikations-/Freigabe-/Evidenz-Records | **Ja** | Typisierte Feld-Tags; **SHA-256 im Record-Trailer** und SHA-256 der gesamten Datei zur Bindung; [Quelle](../bsc/engine/crates/record/src/lib.rs) |
| **DBJNL001** (DBJNL) | BSC: binäres append-only Capture-/Transaktionsjournal | **Ja** | **IEEE-CRC32 je Frame**, **SHA-256 über exakte Journalbytes** und Manifestprüfung; [Quelle](../bsc/engine/crates/journal/src/format.rs) |
| **P0J** (Magic: `P0JRNL01`) | Project_0: binäres append-only Runtime-/Evidence-/Analytics-Journal | **Ja** | **IEEE-CRC32 je Frame**, verkettete **SHA-256-Hashes**, **Bincode** für typisierte Payloads ab Schema 2; historische Schema-1-Value-Trees bleiben lesbar; [Journal](../project_0/native/crates/runtime/src/analytics/journal.rs) / [Payload](../project_0/native/crates/runtime/src/analytics/payload.rs) |
| **Protocol Buffers (Protobuf)** | **Beide**: öffentliche Projektzustände, Commands und Wire-Protokoll | **Standardverfahren, mit eigenen DarkBot-Schemas** | Rust `prost`, TypeScript `protobuf-es`, `darkbot.control.v1` + getrennte Fachpakete `darkbot.project0.v1`/`darkbot.bsc.v1`; [protocol/](../protocol/README.md) |

```text
BSC Native Rust                                 Project_0 Native Rust
   |                                                   |
   +-- intern: DBREC001 / DBJNL001                     +-- intern: P0J
   |                                                   |
   +-- validierte öffentliche Rust-Projektion          +-- validierte öffentliche Rust-Projektion
                      \                               /
                       +--- Project-gebundenes Protobuf
                                      |
                            Reporter / Engine-Feed
                                      |
                             EIN gemeinsames Gateway
                                      |
                             Protobuf / EIN WebSocket
                                      |
                        getrennte Projektadapter und Stores
                                      |
                          EINE React-SPA / EIN Dashboard
                            BSC-Tabs | Project_0-Tabs
```

**Feste Grenze:** DBREC, DBJNL und P0J sind keine Browser-/Gateway-Transportformate. Niemals gesamte Journale, rohe DBREC-/P0J-Records, Secrets, Salt, signierte Transaktionen oder private Freigabeverträge als UI-Payloads exportieren. Der native Owner erstellt nur explizit freigegebene, typisierte öffentliche Projektionen. Protobuf transportiert sie über das vorhandene gemeinsame Gateway. Native Ausführung, Wallet-Risiken, Signaturen und wirtschaftliche Gates verbleiben vollständig im jeweiligen Backend. **Keine Formatmigration, keine zweite Feldautorität und kein zweiter Gateway.**

## 2b. Dauerhafte gemeinsame Konsistenzentscheidungen (2026-10-24)

**Diese Entscheidungen präzisieren die bestehenden F3-/F6-/F7- und Wire-V1.3-Verträge; sie schaffen keine neue Architektur oder Phase.** Wo ein Detail technisch noch nicht entschieden oder implementiert ist, bleibt es **OFFEN**, nicht automatisch PASS.

1. **FRAME_TOO_LARGE – beide Hops:** Der BSC-Reporter→Gateway-Hop und der Gateway→Browser-Hop müssen dieselbe Topic-spezifische STATE-Fehlersemantik erfüllen: übergroße Payload nicht abschneiden, nicht als OK anzeigen, kein unnötiger Verlust aller anderen Topics. **Die konkrete Engine-Frame-Obergrenze einschließlich Protobuf-Envelope-Overhead ist noch OFFEN**; im bestehenden Wire-Vertrag präzisieren, keine BSC-Sonderarchitektur.
2. **Topic-Recovery:** `FRAME_TOO_LARGE` sperrt nur das betroffene Topic. Ein erneutes Abonnieren nach neuem gültigem State muss definiert möglich sein; kein falscher LIVE-Status, keine unbeschränkten Resubscribe- oder Reconnect-Schleifen. **Recovery-Implementierung und Test sind noch OFFEN.**
3. **Idempotenz/Receipts:** **Ein gemeinsamer** Gateway-Ledger (`gateway/src/control.rs`), heute `MAX_RECORDS=1024`; ab dieser Grenze werden neue Commands blockiert. **Eine begrenzte, crashsichere Retention-/Archivierungsregel fehlt** und muss ohne Verlust offener Commands, Idempotenzbindung und gültiger Receipts festgelegt und getestet werden. `request_id` + identische vollständige Request-Bytes (inklusive `issued_at_unix_ns`) bleibt derselbe Intent; gleiche ID + andere Bytes bleibt Konflikt. **Keine willkürliche Löschfrist, kein zweiter Ledger.**
4. **Stop All:** Ein gemeinsamer UI-Command-Verbund für **separat freigegebene** native Project_0-/BSC-Jobs; echte Ergebnisse pro Projekt `SUCCESS`/`FAILED`/`UNKNOWN`, niemals Teil-PASS als Gesamt-PASS. Gateway bleibt verfügbar. Negativtest für Teilausfall **NOT_RUN**; ausschließlich isolierte Testinstanzen, keine produktive P0-Capture unterbrechen.
5. **Gateway-RTT:** Solange nicht tatsächlich gemessen, `—`. Später ausschließlich vorhandenen Gateway↔Browser-Heartbeat/Pong für Messung verwenden; **kein zweiter Browser-Heartbeat, keine P0-Reporter-/Engine-Heartbeat-Änderung allein hierfür**.
6. **Gemeinsame Owner und Phasen:** BSC-Reporter: BSC-Owner; P0-Reporter und P0-Capture-Polling→Push: P0-Owner; Gateway/STATE/WS/Idempotenz: einmalige gemeinsame Infrastruktur. `F4 EVENT`, native Engine-Receipts und weitere fachliche P0-/BSC-Features bleiben in ihren bisherigen Phasen **OFFEN**. Kein BSC-spezifischer Ersatz für fehlende P0-Integration.
7. **Abnahmen:** `npm run budget` ist ohne tatsächlich erfolgreichen Lauf **nicht PASS** (`iltorb`-Fehler offen). Visual-Pixeldiff ohne Baselines und Stop-All-Teilausfall ohne Negativtest **NOT_RUN**. Nur berührte Pfade gezielt prüfen; keine neue Gesamtarchitekturprüfung oder unnötige Volltestserie.

**Dokumentenautorität unverändert:** [Frontend-Master](docs/FRONTEND_PHASENPLAN.md), [Wire V1.3](docs/GATEWAY_WIRE_PROTOCOL_V1_3.md), [Governance](docs/GOVERNANCE_UND_SICHERHEIT.md), die beiden nativen Phasenpläne und die bestehende F0-VAR-Struktur. Keine neuen Topic-IDs, Proto-Feldautoritäten, Reiter oder wirtschaftlichen LIVE-Gates.

## 3. F0-VAR als fortlaufende Backend-Übergabevorbereitung

**Jedes Backend-Codepaket** erfasst relevante neue/geänderte/entfernte Rust-Felder, Konfigurationen, Status, Records, Events und CLI-Parameter für die spätere GUI. **Native Originalnamen und Originaltypen bleiben unverändert.** Projektzuordnung über den Projektpfad und die F0-VAR-Metadaten, nicht durch Umbenennen einzelner Rust-Felder.

**Project_0**: Ausschließlich die vorhandenen [F0_VAR_00_SCOPE.md](../project_0/docs/F0_VAR_00_SCOPE.md), [F0_VAR_01_MATRIX.md](../project_0/docs/F0_VAR_01_MATRIX.md), `project_0/docs/F0_VAR_MANIFEST.json`, die vier existierenden `F0_VAR_*.tsv` und den Generator [f0_var_inventory.py](../project_0/operations/f0_var_inventory.py) verwenden. Das vorhandene Manifest trägt bereits das eindeutige Metadatum `"project": "project_0"`. Der Generator ist aktuell **nur lexikalischer Source-Census**; komplexe Rust-Felder/Makros und semantische Zuordnungen benötigen gezieltes Review. Er beweist weder Wire-Anbindung noch Laufzeitdatenqualität.

**BSC**: Bestehende [BSC_FRONTEND_BINDINGS_V1.md](docs/BSC_FRONTEND_BINDINGS_V1.md), [SOURCE_VARIABLE_MATRIX.md](docs/SOURCE_VARIABLE_MATRIX.md), `frontend/contracts/bsc-fields.json`, `bsc-native-fields.json` und die native Projektion `bsc/engine/crates/qualification/src/status/projection.rs` verwenden. Deren projektspezifische A/B/C/D-Feldklassen bleiben erhalten. Nicht versuchen, Project_0-Rust-Felder in BSC-DBREC oder umgekehrt zu kopieren.

Für jedes **relevante** Feld beziehungsweise Control in der vorhandenen Matrix nachführen:

1. **Source:** Projekt-ID, Rust-Pfad und `Typ.Feld`, Originaltyp, Owner, Source-Commit, Einheit/Skala, Originalwert oder Ableitung, Sensitivität.
2. **Runtime:** Entstehungsphase, `None`-/`UNKNOWN`-Semantik, Datenqualität und Source-/Run-/Generation-/Attempt-/Evidence-Identität, Aktualität.
3. **Wire:** öffentliche Projektion vorhanden/geplant, Proto-Message und Feldnummer, vorhandenes Topic, Publisher/Reporter, Gateway-Verfügbarkeit, STATE/EVENT- und Reconnect-Verhalten.
4. **Frontend:** bestehender Adapter/Store, einer der fünf Zielreiter, tatsächliche Anzeige/Action; Controls nur mit nativem Controller, Guard, Berechtigung und Idempotenz.
5. **Nachweis:** was wirklich verbunden/getestet ist, was fehlt, was fehlerhaft oder bewusst ausgeschlossen ist. Keine erfundenen Felder, IDs, Topicnamen, Werte, Schemas oder LIVE-PASS-Meldungen.

**Gemeinsame Dokumentationsklassifizierung** beider Projekte (keine zusätzliche Runtime-Enum):

| F0-VAR-Status | Bedeutung |
| --- | --- |
| `IST_CONNECTED` | Source → Publisher → Proto → Gateway → Adapter/Store bis zum konkreten Ziel nachgewiesen |
| `IST_BUG` | Tatsächlich vorhandener Pfad mit belegtem Fehler |
| `SOLL_MISSING` | Gewünschtes Feld/Funktion noch nicht implementiert oder angebunden |
| `FUTURE_PHASE_BLOCKED` | Erst nach einem späteren Produkt-Gate erlaubt |
| `EXCLUDED` | Bewusst nicht für die GUI geeignet, mit Begründung |

**Wichtig:** `IST_CONNECTED` ist weder `QUALIFIED` noch `AUTHORIZED`. Private Secrets, Schlüssel, sensitive Wallet-Daten und ökonomische Guards sind nicht automatisch öffentliche Projektion. Alte Quell-HEADs in Manifesten und Dokumenten sind historische Snapshots; vor einer neuen Zuordnung den aktuellen tatsächlichen Quellstand abgleichen.

## 4. Verbindlicher Arbeitsablauf je Backend-Paket

1. **Backend zuerst fertigstellen** und entlang seines bestehenden fachlichen Phasenplans prüfen. F0-VAR ist eine **begleitende Nachverfolgung**, keine zusätzliche Entwicklungs- oder LIVE-Sperre.
2. **Nur betroffene native Änderungen inventarisieren**; bei Project_0 den existierenden Generator für den aktuellen Source-Stand nutzen, den Diff prüfen und Matrix/Manifest nachführen. Unveränderte, bereits zugeordnete Felder nicht erneut vollständig auditieren.
3. **Öffentliche Übergabe vorbereitet halten:** klare fachliche Source→Projection→Proto→Gateway→Adapter→Tab-Zuordnung. Bei fehlendem Publisher/Proto/Controller `SOLL_MISSING` bzw. `FUTURE_PHASE_BLOCKED`, nicht `IST_CONNECTED` eintragen.
4. **Keine vorgezogene GUI-Arbeit:** tatsächliche Protobuf-/Gateway-/Frontend-Codeänderungen erst im vorgesehenen Integrationspaket. Bestehende Feldnummern/Topic-IDs nicht umwidmen; generierte Proto-Typen nicht manuell bearbeiten; gebundene Schema-/Codegen-/Golden-/Deployment-Gates respektieren.
5. **Kleiner Handoff:** Kennzeichnung `[Project_0/F0-VAR]` bzw. `[BSC/F0-VAR]` im Commit-/Übergabebericht, betroffene Originalfelder, Quelle, Status und spätere Zuordnung. **Keine** neue zweite Masterliste und keine vollständige KI-Auditkampagne pro Kleinänderung.

## 5. Fachliche Trennung und Sicherheit

- **Genau fünf Reiter pro Projekt bleiben bestehen**, Feinsortierung später. Keine neue UI-Architektur oder Basic/Advanced-Doppelansicht.
- Backend-Originalwerte, Typen und Einheiten bewahren; große/negative Geldwerte niemals unpräzise per JavaScript-Float interpretieren.
- Gemeinsamer Gateway bedeutet **keinen** gemeinsamen BSC-/Project_0-Trading-State, keine projektübergreifenden Wallet-Gates und keine übertragene LIVE-Freigabe.
- Unbekannte/stale/detached finanzielle oder wirtschaftliche Daten nicht als `0`, `PASS` oder verfügbare Quelle darstellen. Global Finance nur bei belastbarer Herkunft und Qualität.
- Native Producer/Hot-Paths durch GUI-/Gateway-Reporter nicht blockieren; keine Browser-Polling-, JSON- oder UI-Verarbeitung im nativen BUY-Hot-Path.
- Für neue Commands gelten die Native-Guard-/Freigabevorschriften; ein UI-Schalter, reserviertes Topic oder CLI-Flag autorisiert keine Werttransaktion.
- Keine neue Architektur, keine zweite Feldautorität, kein paralleler Reporter-/Gateway-/Frontend-Stack und keine ungefragten Änderungen am jeweils anderen Projekt.

## 6. Aktueller struktureller IST-Stand, keine pauschale Freigabe

Gemeinsamer Gateway, protobufbasierter Topic-Vertrag sowie getrennte Project_0-/BSC-Adapter und Stores existieren im Repository. Project_0 bindet `project0.status` und `project0.capture.state`; weitere reservierte Topics können noch unavailable sein. BSC besitzt die fünf Panels und typisierte Status-/Launch-/Monitoring-/Wallet-Projektionen; ein echtes BSC-Execution-EVENT oder eine wirtschaftliche BUY-/ARM-Funktion ist damit nicht automatisch verfügbar. Einzelne ältere Dokumente und Legacy-Dateien beschreiben überholtes IST; **immer aktuelle Quellimplementierung und tatsächliche Nachweise prüfen**. Dateiexistenz ≠ End-to-End-PASS ≠ LIVE.

## 7. Verbindliche Dokumente

- [Frontend Master F0–F8](docs/FRONTEND_PHASENPLAN.md)
- [Gateway Wire v1.3](docs/GATEWAY_WIRE_PROTOCOL_V1_3.md)
- [Governance/Safety](docs/GOVERNANCE_UND_SICHERHEIT.md)
- [Feldmatrix für Project_0 und BSC](docs/SOURCE_VARIABLE_MATRIX.md)
- [BSC Native-/Proto-Bindings](docs/BSC_FRONTEND_BINDINGS_V1.md)
- [Project_0 F0-VAR Scope](../project_0/docs/F0_VAR_00_SCOPE.md) und [Matrix](../project_0/docs/F0_VAR_01_MATRIX.md)
- [Gemeinsame Protobuf-Schemas](../protocol/README.md)
- [Gateway](../gateway/README.md)

## Development

```sh
cd frontend
npm ci
npm run dev
```

Nur phasengebundene Tests/Codegen-/Kompatibilitätsprüfungen für den tatsächlich geänderten Scope ausführen; ein laufender Backend-F0-VAR-Census allein startet keine Frontend-Testserie.
