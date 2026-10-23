# BSC FRONTEND BINDINGS V1 — F0-BSC v9.1, Native↔Protobuf↔Gateway↔React

**Status:** VERBINDLICHE F0-INVENTUR UND IMPLEMENTIERUNGS-SOLL / KEINE LIVE-ODER-WIRTSCHAFTSFREIGABE. Die einzelnen Klassen A/B/C/D und Source-Gates bleiben ausdrücklich maßgebend; kein Anspruch auf vollständig implementierte Projektion.  
**Initialer Audit-Checkpoint:** `4f6d47bc` mit Nachträgen bis `18474739`/D2; **Repository-Ablage 2026-10-23:** spätere D3–D6-Commits sind hinzugekommen. Vor jeder Codeänderung stets das jeweilige aktuelle HEAD/Symbol erneut prüfen. Der historische Audit-Checkpoint ist ausdrücklich kein gültiger Runtime- oder Abnahme-SHA.  
**Origin of truth:** `bsc/docs/1_PLAN/PHASENPLAN.md` v9.1 (§5a, v8/v9/v9.1), `bsc/docs/1_PLAN/GATE0_TESTPLAN_SHORT.md`, `bsc/docs/1_PLAN/BSC_PRODUCTION_BUILD_MASTER_PLAN.md`, `frontend/docs/FRONTEND_PHASENPLAN.md`, `frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md`, `frontend/docs/GOVERNANCE_UND_SICHERHEIT.md`, `protocol/schemas/darkbot/{control,bsc}/v1/*.proto`, native implementation and reporter.

## 0. Vorbemerkung und geltende Grenze

Die Erfassung aller zukünftigen BSC-Sollfelder ist **F0-VAR-Dokumentation**, keine Freigabe der C1/C2-Integration. G0.7/G0.8 und die verbindliche Gate-0-Stoppregel bleiben unangetastet. Die einzige vorgelagerte GUI-Ausnahme ist die *bereits implementierte isolierte offline/shadow SELF_A/SELF_B-Auswahl*. Die v9-FOREIGN_AUTO-Ausführung, fünf Source-LIVE-Schalter, drei Venue-LIVE-Schalter und globale Wirtschaftsausführung bleiben geplante Anforderungen ohne autorisierten aktiven Controller. Nie `LIVE` im GUI gleichsetzen mit `guard approved`, `execution_ready`, `T0`-Witness oder erfolgreichem Token-Erwerb. Der Browser arbeitet nie auf dem BUY-Hotpath.

### 0.1 Tatsächliche vorhandene Inventare

- `frontend/contracts/bsc-fields.json`: **84** konkrete Protobuf-Felder, **13** Schema-Nachrichtengruppen, Dokument `source_commit=8dd7ce20e5d536bfd2343cb4c097b2b9c3c57ed0`. Nicht neuer HEAD; Field-Source-Revalidierung nötig.
- `frontend/contracts/bsc-native-fields.json`: **299** Einträge für G0.6a-, G0.7-, V5-, G0.8-, Kandidaten- und Readiness-DBREC-Schemas. Die Feldeinträge sind keine fertigen UI-Controls und keine Garantien für Datenverfügbarkeit.
- `frontend/docs/SOURCE_VARIABLE_MATRIX.md`: alte BSC-§2-Einträge noch V4-zentriert; aktuell ist **V5** (`FROZEN_APPROVED_V5_SHA256="UNARMED"`) maßgeblich. Historische V4-Belege bleiben unverändert, verlieren jedoch aktive Autorität.
- Zum ursprünglichen Audit-Checkpoint existierte `BSC_FRONTEND_BINDINGS_V1.md` noch nicht; **diese Ablage schafft die ergänzende F0-Spezifikation**, kein separates Runtime-/Control-Modul. `controlRegistry.ts` existiert weiterhin nicht als bestätigter Code-Owner. Solche Dateien nicht als eigenständige Code-/Steuerautorität behaupten. Die gegenwärtige Steuerungsallowlist liegt in `gateway/src/control.rs`, Projekttopics in `frontend/src/app/projectRegistry.ts` und `frontend/src/core/realtime/topicTable.ts`.
- Native öffentliche Quelle: `bsc/engine/crates/qualification/src/status/projection.rs` → `protocol/schemas/darkbot/bsc/v1/status.proto` → Gateway → `frontend/src/features/bsc/api/bscAdapter.ts` → BSC-Zustand und Panels. Keine GUI-Nachbildung der BSC-Safety-Gates.

## 1. Vier Kategorien strikt trennen

| Klasse | Bedeutung | Verhalten |
|---|---|---|
| **A – IST, öffentlich lesbar** | Native/source-gebunden und als Typ im BSC-Statusprojektionsvertrag vorhanden | Nur anzeigen, Quelle und Alter angeben; UNKNOWN ≠ 0 |
| **B – IST, eng kontrollierbar** | Von Gateway und Native explizit erlaubte Offline-/Readiness-Kommandos | Realer Request, erwartete CAS-Generation, Intent-/Receipt-Status, keine positive Execution-Annahme aus ACK |
| **C – SOLL, technisch geplant** | BSC-v6/v8/v9/v9.1 verlangt Felder/Modi, aber kein qualifizierter Gateway-Payload/Controller vorhanden | Als Spec, nicht als aktivierten Schalter behandeln; allenfalls deaktivierte Capability mit Begründung |
| **D – EVIDENCE/REPORT FILTER** | Ausschließlich Anzeige/Abfrage bereits versiegelter Backend-Daten | Kein Einfluss auf Rohaufnahme, Source-Race, wirtschaftliche Admission oder Safety |

## 2. IST-Control-/Topic-Tabelle (nicht neu bauen)

| Kennung | Wire/Datei | Aktuelle Semantik | Quelle/Owner | GUI | Status |
|---|---|---|---|---|---|
| C-01 | `bsc.readiness.start` | Start Read-only-Readiness Job | Gateway allowlist, Native Readiness | global Overview optional, BSC Launch | **B: vorhanden** |
| C-02 | `bsc.readiness.stop` | Stoppt nur Readiness-Job, kein Trading | wie C-01 | global Overview optional, BSC Launch | **B: vorhanden** |
| C-03 | `bsc.candidate.select` | SELF_A/SELF_B Anfrage, `profile_version` = Selection-Generation; Native-Apply erst bei `darkbotctl bsc sync` | `g07_candidate`, `gateway/src/control.rs`, `candidateControl.ts` | Launch / BUY | **B: vorhanden** |
| T-01 | `bsc.status` | `BscStatus`: Guard, Gates, DRY, Readiness, Builder, Witness, Jobs, warnings, source timestamp | `qualification/status/projection.rs` | Overview + Global Overview | **A: vorhanden** |
| T-02 | `bsc.launch.state` | V5 contract read-only; `PublicField`-Liste inkl. `candidate.*` mirror | gleicher Owner | Launch / BUY | **A: vorhanden** |
| T-03 | `bsc.monitoring.state` | aktuell G0.8-Verifikationsstatus/Resultfelder; nicht alle späteren Source-Statistiken | gleicher Owner | Monitoring, Analytics | **A: vorhanden** |
| T-04 | `bsc.wallets.state` | Public Wallet/Funding-Snapshot, nicht automatisch aktueller Kontostand | gleicher Owner | Wallets | **A: vorhanden** |
| T-05 | `bsc.execution.events` | im Topic-Katalog reserviert; kein qualifizierter binärer BSC-EVENT-Decoder/Catch-up im React-Adapter (throws) | BSC-Journal, später Gateway-F4 | Monitoring | **C: nicht verdrahtet** |

Verboten als generische Commands: `BUY`, `SEND`, `ARM`, `SWEEP`, `sign`, `run arbitrary shell`, beliebige ENV-/File-/RPC-/CLI-Parameter. Die vorhandenen 3 BSC-Commands dürfen niemals durch syntaktisch erfundene neue Action-IDs erweitert werden; native Owner/typisierte Payloads, CAS, per-action explizite Freigabe und negative Tests sind Voraussetzung.

## 3. V8/V9/§5a: vollständige neue Modus- und Filterinventur

### 3.1 Kandidaten-Kontext (Dimension 1; nicht Source)

| Feld/ID (logisch, **kein Proto-Tag**) | Werte/Typ/Einheit | Owner / Evidenz | Platz | Status/Gate |
|---|---|---|---|---|
| SCOPE.requested | `SELF_A`, `SELF_B`, später `FOREIGN_AUTO` | Native candidate selection / später native runtime config | Launch | A/B IST, AUTO SOLL nach G0.8 |
| SCOPE.effective | Typisierter wirksamer Scope, mit Version/generation | Native selected candidate + qualified config | Launch | A/B IST über PublicFields, AUTO SOLL |
| SCOPE.candidate_id | unverwechselbarer B256-Hexwert | `g07_candidate` | Launch | IST A/B |
| SCOPE.generation | u64 (exakt JS bigint) | nativer Kandidatenstore CAS | Launch | IST A/B |
| SCOPE.predicted_token | 20-Byte-Adresse (aus Native berechnet) | Candidate/deployment-bound CREATE2 | Launch | IST A/B; Fremdlaunch SOLL |
| SCOPE.deployment | Chain, Portal, Implementierung, Version, Codehash + Proof-Identität | `g07_candidate`/Preflight/V5 | Launch | IST für A/B; volle Fremdvenue-Gates SOLL |
| SCOPE.signed_identity | Launcher, Signatur-/Launch-/BUY-Hashes, Commitment, Salt **niemals** | DRY+Candidate public mirror/V5 | Launch | A/B teilweise IST; Geheimnisse verboten |
| SCOPE.qualification | `UNQUALIFIED`, `QUALIFIED_FOR <version>`, STOP/UNKNOWN mit identifizierter Evidence | Native Qualifier/Readiness | Launch | IST A/B |
| SCOPE.selection_request | submitted/accepted/pending-sync/applied/rejected, request ID und erwartete Generation | Native request receipts + GUI | Launch | IST A/B |
| SCOPE.transition_block | armed/executing/nonce unresolved/intent in flight => Wechsel STOP | LiveGuard/IntentOwner | Launch | native A/B Guards IST; volle Runtime SOLL |

`SELF_A` und `SELF_B` wählen ausschließlich einen eigenen Testlaunch. `FOREIGN_AUTO` extrahiert den Salt aus unabhängig verifizierter vollständiger fremder Signed-TX-Calldata; er erzeugt **keinen neuen Salt**. Keine A/B-Evidence, Signaturen, Freigaben oder bezahlten Versuche übernehmen.

### 3.2 Fünf unabhängige Ingress-Sources (Dimension 2)

Exakte IDs: `BSC_INGRESS_GETH`, `BSC_INGRESS_PUBLICNODE`, `BSC_INGRESS_DRPC`, `BSC_INGRESS_BLOCKREQ`, `BSC_INGRESS_48CLUB`. **Genau fünf**. Diese Quellen sind kein fünfter/fremder Builder-Submission-Adapter.

Für **jede** der fünf Quellen separat zu inventarisieren:

| Feld (logisch) | Typ / Werte | Owner | GUI / Gate |
|---|---|---|---|
| requested_mode | Enum `OFF / SHADOW / LIVE` | später nativer Konfigurationsowner, Generation-CAS | Monitoring; C, keine aktive Steuerung vor G0.8 |
| effective_mode | aus aktueller Qualifikation abgeleiteter Enum + reason_code | Native Eligibility | Monitoring; C |
| health | `HEALTHY / DEGRADED / RATE_LIMITED / DISCONNECTED / UNQUALIFIED` | Source adapter health | Monitoring; C |
| capability | `HASH_ONLY / FULL_SIGNED_TX / QUALIFIED_PRIVATE_TARGET / UNKNOWN` | Source-specific verified capability | Monitoring; C |
| observed_first_hash_ns | Empfangszeit auf qualifiziertem lokalen Clock, nicht T0 | native ingress source | Monitoring / Analytics; C |
| observed_first_full_ns | vollständige Signed-TX tatsächlich empfangen? bzw. not observed | native ingress | Monitoring / Analytics; C |
| observed_first_actionable_ns | erst nach unabhängiger Verifikation | Native venue/state/identity | Monitoring / Analytics; C |
| first_actionable_proof / last_identity | verified source/venue/version/chain/tx; Evidence hash | Native candidate owner | Monitoring; C |
| gap_count / drop_count / rate_limit_count / reconnect_count | u64 und Messfenster, nie stillschweigend 0 | Source + Journal | Monitoring; C |
| denominator / full_coverage / actionable_coverage | Zeitfenster+Samples; numerator/denominator, keine Phantomquote | G0.4 qualified census | Analytics; C |
| latency p50/p95/p99/max | Ns, klare Zeitgrenzen+Clock+Samples | Native measurements | Analytics; C |
| last_event_time + freshness | Native Eventzeit und Source-Qualität | Publisher | Monitoring; C |
| isolation/queue_pressure | bounded queue, worker budget, WSS degradation may not starve geth | Native source runtime | Monitoring; C |

**48Club-Warnung:** `newPendingAuctions` ist nicht automatisch eine Full-Signed-TX-Quelle; private Target-Semantik, Provider-Zugang und Kosten müssen separat qualifiziert werden. Fehlende Berechtigung ist `UNQUALIFIED`, nicht `DISCONNECTED`. `FREE_ONLY` bleibt der Ausgangspunkt; keine neuen Abos/Kapitalbindungen durch UI.

### 3.3 Drei unabhängige Venue-Gates (Dimension 3)

Venue IDs `FLAP`, `FOUR_CLASSIC`, `OPENFOUR`. Je Venue: `requested_mode=OFF|SHADOW|LIVE`, `effective_mode`, `qualification`, `chain_id`, `deployment/version/codehash`, `launch_selector/mode`, `address_derivation_scheme`, `required_state_evidence`, `quote/economics_validity`, `last_reason_codes` und `source_version`. Native Owner: Flap/Four venue decoder und ReleasePolicy; **alle** sind C / nach G0.7/G0.8, kein aktueller aktivierbarer BSC-Venue-Button.

Zusammenhang: Source `LIVE` + Venue `LIVE` + zulässiger Kandidatenkontext + globaler Wirtschafts-/Safety-Gate. `OFF` in einer zwingenden Schicht verweigert das dortige Trading; `SHADOW` in einer Schicht erlaubt **nur SHADOW**, nicht LIVE. Getrennte Dienste für kanonischen State, Reorg und Safety dürfen durch `INGRESS_GETH=OFF` nicht unbemerkt abgeschaltet werden.

### 3.4 Globale Runtime- und wirtschaftliche Autorität (Dimension 4)

| Feld | Werte/Einheit | Owner | Gate/Anzeige |
|---|---|---|---|
| runtime_mode.requested/effective | `REPLAY / PAPER / SHADOW / LIVE` | native runtime, atomare Konfiguration | Overview read-only, Control erst Phase J |
| config_generation / active_profile_sha | u64/sha256, persisted, rollback safe | Native control owner | Monitoring oder Settings unter BSC, C |
| acceptance_version/hash/approved/execution_ready | aktuell V5, SHA256, zwei unabhängige bools | `acceptance_guard.rs` | Launch read-only, A |
| live_guard_state | `UNARMED` u.a. | Compiled BSC Guard | Overview + Launch, A |
| readiness_verdict / source_evidence | `PASS_READ_ONLY/BLOCKED/STOP/NOT_RUN/UNKNOWN`, datum/version | native readiness | Overview + Launch, A |
| chain/wallet/nonce/budget/funding | exact decimal wei, canonical state+source | native preflight/guard | Launch + Wallets, A falls evidence vorhanden |
| economic_attempt_id / max_attempts / spent/reserved | ID, u64, decimal wei | native attempt owner | Launch + Wallets, A falls record vorhanden |
| inclusion_adapter | Local geth, 48Club, BlockRazor **separat von Ingress** | Qualified submission capability | Monitoring, C (Provider observations teils A) |
| adapter_semantics | `noMerge`, `positionFirst`, expiry, gas floor, pricing, inclusion, proposer coverage, atomicity | per-adapter evidence | Monitoring, UNKNOWN/UNQUALIFIED wo offen |
| producer_witness_status / T0_to_buy | Original-Producer-Witness; ns only with source + bounded uncertainty | G0.8 Producer timing verifier | Monitoring, A/UNKNOWN |

Keine Sicherheitsfreigabe im React-State errechnen. Kein LIVE-BUY-Button ohne eigene native Gate- und Autorisierungsphase. Es gibt **keine** sechs zusätzlichen Provider; Ingress und Inclusion sind getrennte Achsen.

### 3.5 Identitäts- und Konfliktfelder (NEU aus v9)

- `signed_tx_hash`, `recovered_sender`, `chain_id`, `destination`, `selector`, `venue_version`, `deployment_codehash`, `salt_origin`, `predicted_token_address`, `local_prediction_proof`, `token_code_current`, `preloaded_state_generation`.
- `verification`: exakt `VERIFIED / PARTIAL / CONFLICT / UNQUALIFIED / NO_EVENT`; bei `PARTIAL` fehlende Prüffakten und bei `CONFLICT` die widersprechenden Quellen/Launch-IDs **sanitisiert**.
- `identity_collision`: gleicher TX-Hash mit anderen Bytes/Token, **oder** verschiedene TX-Hashes mit derselben Tokenadresse; Konflikt stoppt betroffenen wirtschaftlichen Intent.
- `first_actionable_winner_source`, spätere identische Meldungen je Source, vorangehender Hash-only vs späteres vollständiges verifiziertes Signal, keine Mehrheit/Quorum als Autorität.
- `preparation/readiness`: `T_ready`, `T_queue`, `T_send`, `T_head_local` als **getrennte** Messgrenzen. Weder `T_head_local` noch `T_send` wird `T0`. `T0_TO_BUY` ohne Originalproduzent = UNKNOWN.
- Für das UI aus Native/Journal zu projizieren und source-timestamp, sample count, clock-domain und Evidence-SHA mitzuführen. Keine freie Implementierung eines CREATE2-Rechners in TypeScript.

### 3.6 Echte VIEW-Filter statt wirtschaftlicher Engine-Filter

**Bereits vorhanden (D):** `FilteredEvidence` durchsucht `PublicField.name/value` und ändert nur Sichtbarkeit. `AnalyticsPanel` hat den Report-Selector `Historical DRY / G0.8`.

**Erst mit nachgewiesener nativer Reportquelle (C→D):** Zeitfenster, Venue, Source, Scope (`FOREIGN_AUTO|SELF_A|SELF_B`), requested/effective mode, actual capability, health, `HASH_ONLY/FULL_SIGNED_TX/ACTIONABLE`, Verifikationsstatus, Source-GAP, Reorg, Candidate-ID, Inclusion-Adapter, BUY outcome `PASS/ADAPT/STOP/UNRESOLVED`, finality, builder qualification, Provenienz, nur Erst-Sichtungen vs Duplikate. Der Filter operiert **nur auf vollständigem, qualifiziertem Datenbestand**; `n/total` und Filtergrenzen sichtbar. Filter dürfen **nie** raw journal/capture abschalten oder LIVE-Marktadmission ändern.

**Strategie/Admission-Filter** wie Budget, Token-Venue, Gas, Slippage, Source/Mode und Risiko wirken ausschließlich in dem einen nativen Rust-Owner nach Qualifikation; sie sind keine UI-„Display-Filter“, keine frei editierbaren JavaScript-Slider und keine zweite Engine.

## 4. BSC-Reiter — verbindliche Informationsarchitektur (bestehendes Graphite)

| Reiter | Bleibt | Neue/umgezogene Gruppen | Entfernen/ausblenden (ohne Evidenzverlust) | aktuell aktive Aktionen |
|---|---|---|---|---|
| **Overview** | Source/As-of, Guard, Readiness, G0.6a/G0.7/G0.8, Portal-Drift, Builder-Qualifikation auf Summary, Witnessstatus | Kompakte **Source CONNECTED ≠ Job RUNNING ≠ LIVE READY**, **V5**-Vertrag/Missing-Reason, ausgewählter Self-A/B-Kandidat, Konfig-/Quality-Alter | Eine von zwei identischen BSC-Readiness-Actions: nur Read-only-Ergebnis; Details in Launch/Monitoring; keine lange rohe Feldliste als Hauptübersicht | Navigation nach Launch/Monitoring; Readiness-Steuerung hier nicht doppeln |
| **Launch / BUY** | G0.7→G0.8, Readiness, Guard, Kandidatenauswahl, Hash-Bindungen, Budget, native technische Checks | **SELF_A/B** native CAS; `FOREIGN_AUTO` als späterer, deaktivierter und begründeter Kontext; Kandidatenidentität/Generation/Qualified-for; V5 + DRY5/DRY6; PREPARE_ONLY ≠ signed DRY; klare drei Autoritätsstufen | Statische **V4**-Überschriften/No-active-v4 ersetzen; keine doppelten Readiness-Abschnitte/Actions, keine Legacy-G07-ENV als frei editierbare Controls | einziges aktives Readiness Start/Stop innerhalb BSC; offline Candidate-Request mit CAS |
| **Monitoring** | G0.8-Reconcile/Receipts, Chain-Evidence, Token-Akquisition, Gebühren, Witness-UNKNOWN | Fünf Source-Zeilen, drei Venue-Zeilen, source×venue Eligibility, native `requested/effective`, capability/health, verified addresses, Konflikte, adapter-spezifische Builderstatus, First-actionable Race, immutable Eventjournal später | Keine JSON-Feld-RPC/latency-Metriken ohne Beleg; kein „first hash wins“; ACK nie Erfolg | aktuell alles read-only; Source-/Venue-Schalter erst qualifiziert und freigegeben |
| **Analytics** | Historische DRY-/G0.8-Berichte und reiner Suchfilter | Native G0.4 Census/Denominator, die fünf Source-Races mit p50/p95/p99/max + Samples, Builder-Inclusion-Bewertung, Kosten, Capture-Completeness, first external BUY, physisches T0 UNKNOWN | Keine doppelte ungefilterte `PublicField`-Vollkopie vom Monitoring; kein Gewinnchart aus Testnet/DRY/shadow | ausschließlich Bericht/Filter/Export nach versiegelter nativer Evidence |
| **Wallets** | Launcher/Buyer, saldo, benötigte Mittel, Quelle/Block/Alter, Evidenzidentität | Separater Wallet-/Nonce-/Funding-/Reserved-/Spent-Budget-Readstatus; Klarstellung Test A/B vs aktuelle wallet-owner-Identität, `UNKNOWN` bei fehlender Finalität; exact decimal wei | Keine P0-Treasury/Auto-Sweep-Controls, keine Secrets, keine leeren irreführenden 0-Wallet-Werte | READ_ONLY, keine SEND/ARM/SWEEP |

**Globale Overview** bleibt separat: 3 Projektkarten über 2 gleich großen Graphite-Flächen, fünf vorhandene KPIs pro Projekt; keine neuen Global-BSC-Konfigurationslisten. BSC „Ready“ ist Source-Verbindung, BSC „Process STOPPED“ beschreibt im aktuellen UI **nur den Readiness-Job**, nicht den nativen Handelsbot. Deutlich als `Readiness job` kennzeichnen; Project0 Capture-/Feedstatus separat halten. Im Screenshot `Pre-Live V4_CONTRACT_MISSING` trotz V5-Code: native Quelle/Record-Generation/Report-Alter prüfen, V4 nicht blind nur per Label in V5 umbenennen.

## 5. Protobuf-/Gateway-Vertragsänderungen: **keine Phantom-Felder**

- Die gegenwärtige gemeinsame `control/v1`-Familie für `Hello/Welcome/Subscribe`, `STATE/EVENT`, `CommandRequest` und `ExpectedVersion` **wiederverwenden**. Keine zweite Wire-Architektur; keine Änderung der 11 WS-Nachrichten. Protobuf `bytes` enthalten keine JSON-Ersatzrepräsentation.
- `BscStatus`, `BscLaunchState`, `BscMonitoringState`, `BscWalletsState` aus `bsc/v1/status.proto` weiterverwenden. Bestehende Proto-Tags dürfen nicht umgedeutet/überschrieben werden; etwaiger neuer typed Domainstatus zunächst F0-VAR/native Quelle, dann additive neue Proto-Tags/Message mit Rust/TS-Generierung und Golden-Tests.
- Für neue Source×Venue-Qualität sind **entweder** klar typisierte neue Domain-Nachrichten auf bestehendem passendem STATE-Topic **oder** eine begründete neue BSC-STATE-Topic-Bindung zu entwerfen. Topic-IDs, Protobuf-Feldnummern und Control-IDs **noch nicht zuweisen**, bevor Source-Owner feststeht; `PublicField`-Wildwuchs ist kein Ersatz für stabile Vertragsprojektionen.
- Der native Control-Owner muss atomare `config_generation`, requested/effective values, Admission-Grund und tatsächliche Request/Effective-CAS-Provenienz veröffentlichen. Kein TS-Inferieren von Backend-Qualifikation, keine React-Simulation eines Native-Settings.
- `bsc.execution.events` bleibt RESERVED/UNAVAILABLE, bis BSC Journal + Gateway/Browser Zwei-Hop-`(journal_epoch,durable_cursor)`-Catch-up, Restore, Dedupe, COMPLETE/INCOMPLETE und Backpressure real ausgeführt/abgenommen sind.
- B-Salt, Walletkeys, Raw-RLP, Provider-Authorization, credentialed RPC-URLs, Builder-Headers und private DBREC-Felder **nie** an Browser/Gateway-State-Frames/Logs geben. Public Hashes/Commitments und nachvollziehbare Evidence-Pfade nur nach Redaction/Sensitivity-Prüfung.

## 6. Reihenfolge und Grenzen der Umsetzung (kein eigenständiger BSC-Phasenplan)

| Schritt | Frontend-Ebene | Konkreter Output | Zuerst nötig |
|---|---|---|---|
| **F0-BSC-1** | F0 | Dieses v9.1-Inventar mit der ursprünglichen 84er-Proto-/299er-Nativmatrix je Feld diffen, Status IST/SOLL und Source-SHA revalidieren | Aktueller HEAD; kein Trading-Gate |
| **F0-BSC-2** | F0 | Vollständige Source→Schema→Topic→Store→Panel→Control-Matrix einschließlich Native-Owner, Typ, Unit, Validator, Quality, Gate, Sensitivity, Tests | F0-BSC-1 |
| **F1-BSC** | F1 | Graphite-UI-Struktur der 5 Reiter, keine zweite Navigation; Screenshots/Visdiff global und BSC, P0 Wallets Freeze | F0 |
| **F2/F3-BSC** | F2/F3 | Verifizierte Public-STATE-Projektion, Generation/Liveness, typed Goldens, No-Fake-State | Native PublicProjection vorhanden |
| **F6-BSC aktuell** | F6 | Nur vorhandene `bsc.readiness.start/stop`, `bsc.candidate.select` nach CI-/Security-/CAS-Negativtests nutzen | Native Job/Candidate CAS vorhanden |
| **F7-BSC-v9-DATA** | F7 | Source/venue/identity/race read-only Daten erst nach belegtem Runtime-Owner, Qualität im GUI | G0.7/G0.8 und nachgelagerte native Phasen |
| **F4/F7-BSC-EVENT** | F4/F7 | Native journal cursors, beide Catch-up-Barrieren, Archivseiten und Event-Reconciliation | Echte journalfähige Backend/Gateway-Projektion |
| **F6/F7-BSC-ACTIVE** | F6/F7 | Atomare Source-/Venue-/Global-Modi, CAS, Stop/Retry/Rollback, eigene Freigaben, UI requested/effective | G0.7/G0.8 Verdict + C1/C2/I/J und ausdrückliche Zulassung |

D1–D6 des BSC-DRY und die V5-Requalifikation sind **BSC-Arbeit**, nicht Frontend-Arbeit. Im GUI höchstens evidenzgebundene read-only Darstellung von `PREPARE_ONLY / NOT_REQUALIFIED`, signiert-qualifiziert und separat freigegeben. D1/D2 Code-Commits ≠ DRY-GO; keine real-key Signatur über das Frontend.

## 7. Präzise Abnahmematrix (vor Implementierungs-PASS)

1. **Quelle und Inventar:** auf neuem HEAD `bsc-fields.json` (84 Baseline) und `bsc-native-fields.json` (299 Baseline) neu erheben; pro Eintrag path, SHA, type/unit, decoder, writer, consumer, Gate, Sensitivity und Zeugenfall. Keine nur per Dokument erfundenen Felder.
2. **Proto-Golden:** Rust/TS bytegleich inkl. u64-Max/Wei-u256/negative signed PnL, Minor-Evolution, inkompatibles Major STOP, unveränderte Tags/Topic-Payloadtyp.
3. **Mode/Authority:** `OFF,SHADOW,LIVE` per Source und Venue separat; requested≠effective; Source-LIVE + Venue-OFF/SHADOW = kein LIVE; global `UNARMED` sperrt immer; Candidate-A/B-Wechsel darf kein alte V5/Signatur/Economics erben.
4. **Identity/Conflict:** full verified beats earlier hash-only; wrong chain/sender/selector/version/deployer/CREATE2 = STOP; conflicting same hash or address collision = STOP; SHADOW must neither block nor acquire LIVE nonce; 48Club target-only fails without capability proof.
5. **Availability:** fehlender Native-Owner => disabled/unavailable, kein injizierter Mock-PASS. Alte fachliche Snapshots werden als `STALE`/historisch gekennzeichnet; `SOURCE_DETACHED` nur bei belegtem Feed-/Supervisor-Disconnect (Gateway-Liveness). Neue Subscriber sehen nie fälschlich Ready; keinerlei Browser-HTTP-Polling.
6. **Commands/Safety:** Gateway bleibt bei drei bestehenden BSC-Action IDs; Request-ID, CSRF, Origin/Host, Engine-ACL, CAS generation, idempotent receipt, ACK≠execution; keine arbitrary args; kein ARM/BUY/Wallet command.
7. **Analytics/EVENT:** View-Filter modifizieren keinerlei Capture/Admission; `(journal_epoch,durable_cursor)` korrekt und Quelle/Run-gebunden; GAP/Reorg/Overflow führt INCOMPLETE; erst nach echten 2-Hop-Tests COMPLETE.
8. **Timing:** ursprüngliche Producer-Witness oder UNKNOWN; p95 ohne Nenner, Window, Clock, Source falsch. Nicht lokale Head-Ankunft als T0 darstellen oder T_send als T_buy.
9. **Snapshot und Graphite:** BSC alle fünf Reiter + Global 3+2 an Desktop und eng, Keyboard/A11y, keine doppelte Readiness-Action innerhalb BSC, P0 Wallets Pixeldiff=0, keine Design-Tokens verändern.
10. **CI-/Dateigates:** `npm run type-check`, `lint`, `test`, `build`, `lint:boundaries`, `lint:css`, `check:freeze`, Protocol Generator/`check --check`, Playwright BSC + Overview; Rust Unit/negative tests der echten nativen Projekte; pro Lauf Exit-Code und git SHA dokumentieren. Auf Grundlage dieses DOKUMENTS keine PASS-Behauptung.

## 8. Konkrete Dokumentationsänderungsvorlage (ohne Code-/Control-Freigabe)

**In `frontend/docs/FRONTEND_PHASENPLAN.md` als neuer §12, nach §11 einfügen:**

> `## 12. F0-BSC-V9.1 – BSC-Inventur und Reiterabgleich (2026-10-23)`  
> Die bestehende Protobuf/Graphite/Gateway-Architektur bleibt bestehen; die verbindliche ergänzende v9.1-Feld-/Modus-/Filtermatrix ist vorläufig `BSC_FRONTEND_V9_1_F0_ABGLEICH_2026-10-23.md` (Review-Stand), bis ein Repository-Dateiname beschlossen und committet wurde. Drei Kandidatenkontexte, fünf Ingressquellen, drei Venues und globale Wirtschaftsautorität sind verschiedene Achsen. Nur A/B Offline-Auswahl und die existierende Readiness-Steuerung haben derzeit einen implementierten Controller. Alle übrigen Source×Venue/FOREIGN_AUTO/LIVE-Controls bleiben PLANNED/DISABLED bis zur gesonderten BSC-Phasenfreigabe nach G0.7/G0.8. Keiner der fünf BSC-Reiter oder der Global Overview erhält erfundene Daten, Profite, Timings oder BUY-/ARM-Controls. `FRONTEND_PHASENPLAN` F0–F8, `GATEWAY_WIRE_PROTOCOL_V1_3`, `GOVERNANCE_UND_SICHERHEIT`, Protobuf-Goldens und Graphite-Freeze bleiben gültig. Historische V4-Texte sind nur Legacy und dürfen die aktuellen V5-Readiness/Contract-Daten nicht überschreiben. Umsetzung und Abnahme gemäß Matrix §6–§7.

**In `frontend/docs/SOURCE_VARIABLE_MATRIX.md` nach bisherigem §4 ergänzen:**

> `## 5. Aktualisierung BSC v9.1`  
> Der alte BSC-§2 ist eine historische F0-Initialextraktion. Aktueller Guard-/Contract-Owner verwendet V5 statt V4. Relevante gegenwärtige Quellen: `acceptance_guard.rs`, `status/projection.rs`, `g07_candidate`, `g06a_requal_check`, `g08_mainnet_verify`, Gateway allowlist und die gemeinsamen Proto-Domain-Schemas. Die alte 84er-Protobuf-Feldliste und 299er-native Liste sind vollständig gegen aktuellen HEAD zu prüfen. Neue dimensionale Felder und Status/Filters sind in der v9.1-Abgleichsmatrix als IST/SOLL getrennt inventarisiert. Geplante Runtime-/Source-/Venue-Controls bleiben kein Teil der aktiven Gateway-Allowlist und benötigen einen neuen nativen Vertrag/eine gesonderte spätere Freigabe.

**Git-/Commitregel:** Erst aktuellen HEAD und gleichzeitige BSC-Arbeit auf `main` beachten; keine Branches/Worktrees und keine unautorisierte GUI-/Trading-Änderung. Dieser Bericht ändert weder Repository noch Mainnet, signiert nichts und wertet historische Evidence nicht um.


---

## 9. Zweiter Quellcode-Abgleich: nachzutragende, bisher nicht vollständige Feldfamilien

**Prüfbasis:** BSC-/Frontend-Code und Contracts auf GitHub 2026-10-23; Ausgangsstand `18474739acfe` (D2). Nachprüfung der neuesten Commits bis `3aedde8153b2`: D3, D4 und D5 sind als Code-/Test-Commits vorhanden; D6 nicht pauschal als abgeschlossen gewertet. Zwischen `18474739` und `3aedde81` wurden keine Gateway-, Protobuf- oder React-Quelldateien geändert; `frontend/contracts/bsc-fields.json` und `bsc-native-fields.json` wurden mit unveränderten 84 bzw. 299 Einträgen geprüft. Der alte Vertrag hat 84 Protobuf-Felder in 13 Messages und das alte `bsc-native-fields.json` inventarisiert 299 selektierte native Felder aus 17 Schemagruppen. **Das ist kein vollständiger fachlicher Frontend-Feldcensus aller künftigen Produktionsphasen.** Die 299 Felder enthalten zahlreiche technische Vertragstexte, die nicht als separate UI-Zeile darzustellen sind. Zusätzliche Felder brauchen erst einen authentischen Native→Proto-Owner.

### 9.1 Konkrete Fehler und Veraltung im vorhandenen IST

| ID | Ist-Befund, exakt zu korrigieren | Code-Owner | Status |
|---|---|---|---|
| COR-01 | Reporter `readiness.verdict` setzt jedes Ergebnis außer `PASS_READ_ONLY` auf `STOP`. Das Readiness-Binary liefert getrennt `BLOCKED` (fehlende Bedingung) und `STOP` (Widerspruch). Muss ENUM-validiert unverändert durchreichen, nicht beliebige Werte akzeptieren. | `bsc/engine/crates/qualification/src/status/projection.rs` Zeilen 439–445; `g07_pre_live_readiness.rs::finish_report` | bestätigter Bug |
| COR-02 | „No active v4 contract“, „Bound v4 plan“, V4-Hinweis im Launch-Panel: Backend ist V5. `status.proto` enthält ebenfalls alte V4-Kommentare (kein Wire-Tag-Wechsel). | `frontend/src/features/bsc/pages/{OverviewPanel,LaunchBuyPanel}.tsx`, `protocol/.../bsc/v1/status.proto` | bestätigter Label-/Dokumentationsdefekt |
| COR-03 | `readiness` wird in BSC Overview **und** Launch dargestellt und steuerbar gemacht; künftig eine zentrale aktive Steuerung in Launch. Overview nur Summary/Link. | `OverviewPanel.tsx`, `LaunchBuyPanel.tsx` | bestätigte Doppelung |
| COR-04 | `bsc.candidate.select` **ist** in der Gateway-Allowlist mit `SELF_A`/`SELF_B` und `profileVersion` enthalten; negativer Testsatz ebenfalls in `gateway/src/control.rs`. | `gateway/src/control.rs`, `candidateControl.ts` | nachgewiesen, Matrix C-03 bleibt IST |
| COR-05 | D1 entfernt `DARKBOT_G06A_SALT` aus der allgemeinen `allowed_environment`-Liste; eigene Scoped-Helpers bestehen. D2-PREPARE ist am Prüf-HEAD als Code-Commit vorhanden; D3–D6 nicht pauschal PASS. | `bsc/tools/g06a_shell_scope.sh`, `g06a_final_dry.sh`, `g06a_dry_handoff.rs` | D1/D2-Code vorhanden; laufender Gesamt-Gate offen |
| COR-06 | BSC-Projektkarte „Ready“ ist Source-Status, „Process“ überwacht den Readiness-Job, NICHT eine produktive Trading-Engine. Quelle, Prozess und LIVE-Guard separat beschriften. | `overview/ProjectEntry.tsx`, `gateway/src/control.rs` | bestätigte Semantik |
| COR-07 | Reporting-/Legacy-Grenze liest aktuell `builder_requirements_refresh_*.json` und `probe.evidence.json`; das JSON-Verbot gilt für den **internen Runtime-HOT0**. Diese eng begrenzte lesende Reporting-Ausnahme dokumentieren. | `qualification/src/status/projection.rs::json_walk` | dokumentierte Ausnahme |
| COR-08 | `source_commit` und `data_as_of_unix_s` sind vorhanden; im Detail-Panel die gemeinsame Provenienz sichtbar und je Verdict die **eigene Evidence-Zeit**. Snapshot-Alter ≠ `SOURCE_DETACHED`: detached nur durch Feed-Liveness, Snapshot-Alt als eigener Indikator. | `status.proto`, `projection.rs`, `bscStore.ts` | bestehende Felder, UX-Mangel |
| COR-09 | Der alte E2-8-Eintrag in `bsc_bug_v1.md` betrifft Reporter-Socket/Label, **nicht** den neuen BLOCKED-vs-STOP-Bug. Neuer eigener Bug-/Testeintrag nötig. | `bsc/docs/5_OFFENE_FEHLER/bsc_bug_v1.md` | Ticketzuordnung korrigieren |
| COR-10 | `BSC_FRONTEND_BINDINGS_V1.md` und `controlRegistry.ts` nicht im geprüften Git-Baum. Keine fiktiven Dateien referenzieren; Namensvergabe nach Commit. | Repository-Tree / F0 | Dokumentationslücke |

### 9.2 Native G0.7-Sende-, Budget-, Capture- und Schutzfelder – bislang unvollständig

Die ursprüngliche Matrix nennt Budget und Journal nur zusammenfassend. Folgende Unterfamilien sind eigenständig zu inventarisieren und dem passenden Reiter zuzuweisen. **Nicht alle sind heute in der `status.proto`-Projektion sichtbar**:

| Feldgruppe / technische Quelle | Darstellung / Semantik | Reiter | Klasse |
|---|---|---|---|
| `g07.evidence`: `classification`, `attempt_id`, `single_use_latch`, `state_dir`, `attempt_number` | Versuch gestartet/abgebrochen/consumed; niemals aus `max_attempts` allein raten | Launch | Native intern, künftige explizite Projektion |
| `g07.evidence`: `launcher_nonce`, `buyer_nonce`, `spent_before_wei`, `reserved_worst_case_wei` | verschiedene Wallet-Nonces; reserviertes vs. tatsächlich ausgegebenes Budget | Launch/Wallets | Native intern |
| `g07.evidence`: `token_code_before_empty`, `launch_tx_hash`, `buy_tx_hash`, `launch_value_wei`, `buy_value_wei` | exakter versuchsgebundener On-chain-Kontext, kein fremder-Full-TX-Beweis | Launch | Native intern/teilw. Vertrag |
| `g07.evidence`: `local_expected_output`, `min_output`, `slippage_bps`, `gas_price_wei`, `launch_gas_estimate`, getrennte Gas-Limits | uneditierbare signierte Grenzwerte vs. beobachtete Ausgaben | Launch/Wallets | Native intern/teilw. DRY-Projektion |
| `g07.evidence::bundle`: `send_head`, `max_block`, `head_age_ms`, `read_round_trip_ns`, `max_head_age_ms`, `max_local_clock_lag_ms`, `expiry_rule` | Frische und zeitliche Transportgrenzen, keine T0-Messung | Launch → technische Details / Monitoring | Native intern |
| `g07.evidence::bundle`: `noMerge`, `positionFirst`, Provider-Body SHA/Länge, exakte TX-Reihenfolge | Einstellungen **versus** echte Builder-Qualifikation trennen | Monitoring | Native intern |
| `g07.evidence::send`: pro Provider `write_started_ns`, `write_flushed_ns`, `bytes_may_have_left`, HTTP/JSON-RPC-Klasse, Response-Hash | ACK, Framing und unbekannte Egress-Zustände; niemals Kauf/PASS | Monitoring | Native intern |
| `g07.evidence::send`: `release_before_ns`, `release_after_ns`, `release_marker`, `both_flush_completed`, `flush_spread_ns` | echte Handoff-Grenzen / Mehrfach-Provider-Fanout | Monitoring/Analytics | Native intern |
| `g07.evidence::journal`: `records`, `first_sequence`, `last_sequence`, `dropped_full`, `rejected_oversize`, `disconnected` | Vollständigkeit, Journal-GAP, fehlendes Capture → kein nachweisbares PASS | Monitoring/Analytics | Native intern |
| `g07.evidence`: `capture_complete`, `capture_status`, `both_receipts_observed_ns`, `coverage_end_ns`, `canonical_window_reconciled_in_g08` | 15-Sekunden-Capture und spätere G0.8-Reconciliation getrennt | Monitoring/Analytics | Native intern |
| `g07` PREPARE/SIGN/VERIFY/LIVE und D1–D6 | PREPARE_ONLY ≠ DRY_PASS; Signaturen ≠ Chain-Submission; keine B-Secrets | Launch/Analytics | D1/D2 teilweise Code, Projektion offen |

**Sicherheitsregel:** Das G0.7-Schema enthält ein internes `SALT`-Feld. Dieses darf bei SELF_B unter keinen Umständen in eine generische `PublicField`-Liste gelangen. Auch signierte Raw-Transaktionen, private Journalpfade, Schlüssel und credentialed URLs nie in Protobuf/Browser/Logs übertragen.

### 9.3 Native G0.8, bislang fehlende Felder

| Feldgruppe | Eigentümer / Ist | Reiter |
|---|---|---|
| `pricing.first_external_buy_scan`, `first_external_buy_tx/block/index/buyer/value/token_amount` | `record/src/schema/g08.rs::result` **heute schon vorhanden** und im generischen `BscMonitoringState.results` potentiell lesbar bei echter Evidence | Monitoring/Analytics |
| `ordering.buy_block_miner` | Schema v5 heute vorhanden; Validator/Builder-Zuordnung **nicht** daraus allein als nachgewiesen behaupten | Monitoring |
| `ordering.successful_external_buys_between_{count,hashes}`, `launch/buy_{block,index,hash}` | N/N+1 und Fremdkonkurrenz getrennt von Timing; nur vollständige kanonische Receipts | Monitoring/Analytics |
| `acquisition.{evidence_complete,token_absent_before_launch_block,token_exists_at_buy_block,buyer_token_balance_at_buy_block,positive_attributable_acquisition,buy_receipt_token_net}` | echter Erwerb, nicht bloßer Status/Block | Monitoring |
| `costs.{launch_gas_cost_wei,buy_gas_cost_wei,buy_value_wei,buyer_total_debit_wei,experiment_total_cost_wei,direct_builder_tip_wei,buy_event_fee_included_in_value}` | aus Receipts abgeleitete Kosten, Event-Fee nicht doppelt summieren | Monitoring/Analytics |
| `capture.{g07_local_journal_complete,chain_window_complete,market_window_seconds,competition_scan}`, `confirmation.{current_head,confirmations_after_launch_block}` | Vollständigkeit und Finalität sind keine Receipt-Momentaufnahme | Monitoring |
| `g08.unresolved.expiry_observation`: head, tx_known, sender/buyer latest/pending nonces, error | getrenntes UNRESOLVED mit Ursache; niemals zu STOP/PASS normalisieren | Monitoring |
| `g08_producer_timing`: Clock-Source, raw resolution, hook overhead, normalization error, remaining uncertainty, lower/conservative upper ns, original producer/attempt identity | physisches T0–T_buy ausschließlich bei Originalwitness | Analytics/Monitoring |

### 9.4 Nachgelagerte native Phasen: zusätzlicher Zukunftsbedarf (nicht jetzt ausführbare Controls)

| Bedarf | Werte/Dimensionen | Reiter | Freigabephase |
|---|---|---|---|
| **C1/C2** | Peer-Zahl / früheste Hook-Stufe; WSS vs pinned-geth parallele Quelle; Tx-Typ, Failure/GAP/Txpool accepted; Pending vs Canonical, Reorg-Tiefe, replacement/orphan, Quarantine/Recoverable, State-Generation, Quotes invalidation | Monitoring | nach Gate-0, C1/C2 |
| **C3/Unit5** | Ingress→validate→HOT0→prepare/sign→IPC/handoff/peer; min/mean/p50/p95/p99/p99.9/max + Nenner + Clock; allocations, locks, queues, HoL und Worker-Inhibition | Analytics | C3/I |
| **Phase D/E** | Drei Venue-Familien und Mode-Selektoren; supported/unsupported selector, token tax mode/quote asset/creator internal purchase, deployment/ABI/bytecode/CREATE2 | Monitoring/Analytics | D/E |
| **Phase F** | Recorder-Zustand, segment manifest, Run/Boot/Clock, bytes in RAM, ByteBudget max, Queue-/Writer-HWM, Drops/Gaps, Disk budget/free/writer throughput, projected duration/capacity, Sealing, integrity SHA, crash recovery, replay proof, optional rebuildable SQL index | Analytics, Monitoring bei Source-Gaps | F/I |
| **Phase F Capture** | 30-min qualifying run separat von [T0,T0+15s] je Versuch, 4/24/48h erst gesondert; Records, raw vs filtered, source completeness, gap/disconnect and history cursor | Analytics | F/I |
| **Phase G Economics** | tax/fee/exemption, quote validity, native spend, slippage, revert/replacement, approval/sell/migration, token/wallet deltas, incurred vs reserved costs | Analytics/Launch readonly policies | G |
| **Phase H Strategy C** | Kandidaten/eligible/selected/admitted/skipped/unknown; native strategy thresholds, expected value nach ALLEN Kosten, risk reason, attempt attribution, EV vs realized PnL strikt trennen | Analytics / Launch policies | H/I |
| **I/J** | `REPLAY/PAPER/SHADOW/LIVE` requested/effective + config generation, arm/revocation, per-scope frozen campaign, observed/admitted/reverted/acquired/late/unresolved denominator and per-cell acceptance, audit/rollback | Launch + Monitoring/Analytics | I/J und neue ausdrückliche Freigabe |
| **Frontend F4** | `journal_epoch`, `durable_cursor`, catch-up barrier, `CATCHUP_COMPLETE/INCOMPLETE`, bounded history queries, redaction; keine Ereignisse aus WS-seq rekonstruieren | Monitoring/Analytics | F4/F7 |
| **Wallets** | `launcher` und `buyer` pro Wallet: native chain/pending/latest nonce, safe reserved/spent budget, gas reserve, true balance read timestamp, funding deficit, economic-intent/UNKNOWN reconciliation, asset identity, receipts-driven token acquisition | Wallets | C2/G/J; aktuell historische Snapshots |

### 9.5 Datenqualität, Filtergrenzen und UI-Testfälle

- Zu **jeder Kennzahl**: `source_commit`, snapshot `data_as_of`, **eigene** evidence id/hash/producer time, `quality`, `run_id`, `chain/block/canonical/finality`, window, sample size, unit, scope (Chain/Venue/Version/Candidate/Source/Adapter) und Status `UNKNOWN/INCOMPLETE` falls nicht belegt. `source_commit`/`data_as_of` brauchen nur **einen sichtbaren Panel-Provenienzbalken**, nicht bei jeder Zeile denselben Text; Einzel-Evidence-Zeit bleibt zuordenbar.
- View-only Filter nur auf **nachgewiesenen** historischen Records: Run-ID, Zeitpunkt/Zeitraum, Venue, Token/Candidate, Source, Source-Version, Source-Capability, requested/effective, Full/hash/actionable, Canonical/Reorg, First-External, Owner/Wallet, Probe/Adapter, Launch-/BUY-Status, Decision reason, Window completeness, Fee/Tax mode, Dedupe/First-only, Record integrity. Report-Scope und Nenner sichtbar, keine Änderung des Primary Raw Capture.
- Fehlender Wert ist `—/UNKNOWN`, niemals erfundenes `0`, implizites `PASS`, `READY` oder `SOURCE_DETACHED`. Nachgewiesener SourceDetach folgt vom Gateway-/Source-Lifecycle; unabhängige Stale-/Evidence-Age-Zustände separat.
- Keine Autoritätstransfers: UI-Button → CommandResponse ACCEPTED ist nur durables Intent, Desktop Native Apply entscheidet Kandidat; per-source `LIVE` ist keine wirtschaftliche Freigabe; Builder HTTP ACK ist keine Inclusion, N+1 ist nicht <1ms, DRY-Ertrag ist kein realisierter Profit.
- Tests: `BLOCKED`/`STOP`/`NOT_RUN`/`PASS_READ_ONLY` unverändert bei Rust-Projektor→Proto→TS; V4-Label-Negativtest, alter Reporter/BSC Snapshot; Candidate-CAS, invalid/public mirror mit B salt, source/venue/global fail-closed, Full-TX vs Hash-only, conflicts, F4 Catch-up, G0.8 first-external/miner, unsigned wei u256 String, local provenance/UTC, Graphite desktop/compact/mobile, kein P0-CSS-Diff.

### 9.6 Vier Detailreiter plus BSC Overview: definitive Aufteilung, keine Duplikate

1. **Launch / BUY** — *Preparation & Approval* (Self A/B und später AUTO); `PREPARE`, `SIGN/VERIFY`, `V5`, Bereitschaft, Budget/Attempt, sichere offline Readiness-Controls; **keine** Trading-Webbutton-API. Nicht zur Journal-/Source-Diagnose aufblähen.
2. **Monitoring** — *Sources, Venues, Chain*; fünf Source-Modes/Health/Capability, drei Venue-Gates, Identität/Kollision, Inclusion-Adapter und G0.8-Reconciliation; alles auf einem read-only Überwachungsreiter mit Detailaufklappung.
3. **Analytics** — *Coverage, Race, Costs, Integrity*; historische DRY-/G0.8-, Capture-/Source-Census-, pXX-, Timing-Witness- und Kostenreports. Filter auf sealed Runs, keine falschen Kurven.
4. **Wallets** — *Identity, Funding & Exposure*; Launcher/Buyer getrennt, Snapshot-Herkunft/-Alter, Nonces, reserviert/gespendet, token receipt und wirtschaftliche Exposition. Keine SEND/SWEEP/Keys.

**Namensentscheidung:** Die bestehenden Reiter-Namen `Launch / BUY`, `Monitoring`, `Analytics`, `Wallets` **beibehalten**, um Navigation/Test-/Routenidentität nicht unnötig umzubauen. Deutliche, präzise Unterüberschriften statt weiterer globaler Top-Level-Tabs. Optional später „Monitoring“ → „Sources & Chain“ (länger, mit Nav-/A11y-/Visual-Regressionsfolgen) – kein fachlicher Zwang. „Launch / BUY“ ist eine fachliche Prozessansicht, **kein BUY-Button**.

**Nicht implementiert oder getestet durch diese Matrix.** Vor Merge gegen neueste BSC-Commits und Protobuf-Goldens erneut prüfen; keine Änderungen an privaten Artefakten oder LIVE-Guard.

### 9.7 Letzter GitHub-Abgleich während dieser Arbeit

`main = 3aedde8153b2` (23.10.2026, 21:11 UTC, Zeitpunkt der letzten Repository-Prüfung). Neu gegenüber der ursprünglichen v9.1-F0-Matrix sind die BSC-Implementierungscommits D3 `d7e81aca3b08`, D4 `1d443959da7c`, D5 `3aedde8153b2`. Sie betreffen DRY-Handoff, H2-Frische und A/B-Parität, **nicht** eine Frontend-Freigabe. Beide `frontend/contracts/*.json` tragen weiterhin 84 bzw. 299 Feldeinträge; die BSC-React-Panels und das Proto-Schema wurden zwischen D2 und D5 nicht geändert. **Aktueller Git-HEAD ist zeitabhängig und vor Code-Änderungen erneut zu prüfen.**


---

## 10. Fable-G1–G23: Einzelfeld-Differenz V2→V3 (23.10.2026)

**Charakter:** Ergänzung der ursprünglichen V2, keine bereits implementierte Proto-/React-Änderung. Verglichen wurden 23 vom zweiten Reviewer benannte Felder bzw. Feldgruppen mit dieser Auditmatrix und zusätzlich die nativen Quellen `g07_pre_live_readiness.rs`, `record/src/schema/{g06a,g07,g08,g07_candidate}.rs`, `g06a_requal_check.rs`, `g06a_final_dry.sh` und `qualification/src/status/projection.rs`. Die Verfügbarkeit eines nativen Record-Feldes bedeutet weder, dass dieser Record für den konkreten Versuch existiert, noch dass das Feld bereits zum Browser projiziert wird. Laufende D1–D6-Commits und ihre Testabnahme sind getrennt zu verfolgen.

### 10.1 Einzelfeld-Audit und zugehörige Pflichtaktion

| ID | V2-Abdeckung | Ergänzungs-/Korrekturanforderung | Reiter; Exposition |
|---|---|---|---|
| G1 | FEHLT | `readiness.mode` als separates Feld: `OFFLINE_ZERO_EGRESS`, `READ_ONLY_RPC`, unbekannt/absent. `PASS_READ_ONLY` setzt die qualifizierte Read-only-Betriebsart voraus; Modus nicht aus dem Verdict rückwärts raten. | Launch / BUY; native A′→typed DTO |
| G2 | JA | `PASS_READ_ONLY`, `BLOCKED`, `STOP`, `NOT_RUN`, `UNKNOWN` unverfälscht transportieren; Projektion korrigieren und Golden/Negativtests ergänzen. | Launch/Overview; bestehendes Proto mit fehlerhafter Producer-Mappinglogik |
| G3 | TEIL | Aktueller verbindlicher technischer Katalog enthält **16** `REQUIRED_TECHNICAL_CHECKS` plus `BUILDER_SEMANTICS` und `PRODUCER_WITNESS`; darüber hinaus Reporter-Checks zulässig. Nicht unbewiesen `22` hardcoden. Statt `"STATUS · detail"` idealerweise typisiertes `check_id`, `status`, `detail`, `source`, `observed` und versionierte erlaubte Werte; unbekannte Namen im Details-Expander, nie als PASS werten. | Launch / BUY; bereits als flache PublicFields A, künftig typed DTO |
| G4 | TEIL | `send_head`, `max_block`, `fortyeightclub_max_timestamp`, `expiry_rule`, `head_age_ms`, `excess_staleness_ms`, `read_round_trip_ns`, `max_head_age_ms`, `max_local_clock_lag_ms`, `probe_head`, `probe_head_age_ms`, `probe_excess_staleness_ms`, `probe_read_round_trip_ns`, `public_mempool_launch`, `automatic_retry`, `noMerge`, `positionFirst`, Builder-Body SHA/Länge. Erforderliche Version/Blockbasis binden. | Launch/Monitoring; native A′ |
| G5 | TEIL | `clock_id`, `boot_id`, `release_before_ns`, `release_after_ns`, `release_marker`, pro Provider `write_started_ns`, `write_flushed_ns`, `bytes_may_have_left`, `http_status_code`, `rpc_result_hash`, `rpc_error_code`, bereinigte `rpc_error_message`, `response_undecodable`; `both_flush_completed`, `flush_spread_ns`, `any_bytes_may_have_left`, `not_t0_to_buy`. Provider-Credentials/unsanitized responses ausblenden. | Monitoring/Analytics; native A′ |
| G6 | TEIL | `journal.records`, `first_sequence`, `last_sequence`, `bytes`, `sha256`, `next_sequence`, `enqueued`, `dropped_full`, `rejected_oversize`, `disconnected` sowie `both_receipts_observed_ns`, `coverage_end_ns`, `canonical_window_reconciled_in_g08`, `launch/buy_receipt_observed`, `post_window_head`. GAP/unknown means not COMPLETE. | Monitoring/Analytics; native A′ |
| G7 | TEIL | `classification`, `critical_rule`, `next_required_gate`, `attempt_number`, `spent_before_wei`, `reserved_worst_case_wei`, `single_use_latch`, `state_dir` mit bewusst **redigiertem** Status statt lokalem privaten Dateipfad. | Launch; native A′ |
| G8 | FEHLT | Portal-R0-Upgraderecord: `scanned_from/to_block`, `headers_verified`, `bloom_candidate_blocks`, `coverage_method`, Upgrade-Einträge (Block/TX/Implementation/Slots), Baseline-/Current-Impl, `first_parent_hash`, `last_block_hash`; Typed-Proof inkl. Kanonizität und Checksum-Gültigkeit. | Launch Qualifikation + Analytics Audit; native A′ |
| G9 | TEIL | Launch-Path-Snapshot mit `block_number/hash`, `launch_return_token`, `code.{address,sha256,bytes,roles}`, `storage.{address,slot,value,selects}` plus `CURRENT_LAUNCH_PATH` Driftgründe. Format nur als redigierte öffentliche Beweisübersicht, kein unbeschränktes Bytearray im Browser. | Launch / Monitoring technische Detailansicht; native A′ |
| G10 | FEHLT | Quote: `curve_id`, `protocol_fee_bps`, `buy_tax_bps`, `tax_on_bonding_curve_enabled`, `protocol_fee_wei`, `bonding_curve_tax_wei`, `combined_fee_wei`, `net_reserve_input_wei`, `reserve_after_wei`, `circulating_before`, `expected_output`, `slippage_bps`, `min_output`, `authority`, ursprüngliche Block-/Deployment-Bindung. Keine Neu-Quote in TS. | Launch (read-only Economics), Analytics (Historie); native A′ |
| G11 | FEHLT | EVM-Simulation: launch/buy execution und intrinsic gas getrennt, jeweiliges total used, `gas_accounting`, `output_difference`, `state_source.block_number/hash`, `limitations[]`. Historisch/simuliert niemals als aktueller Gasverbrauch ausgeben. | Launch/Analytics; native A′ |
| G12 | FEHLT | Economics-Crosscheck: `mainnet_estimate_gas_launch_success`, `mainnet_launch_gas_estimate`, `mainnet_eth_call_launch_success`, `mainnet_eth_call_predicted_token_match`, `limitations[]`. A: ggf. öffentlicher `salt_counter`. B: nur öffentlicher `salt_commitment` und Evidence-Identität, **niemals B-Salt oder private Launch-Calldata**. Vor Exposure field-by-field sensitivity audit. | Launch; native A′ |
| G13 | TEIL | Builder-Policy-Quellen: `sources[].{url,http_status,fetched_unix_ms,body_sha256,finding}`, `minimum_spend_probe`, `non_claims`, `dry3_ready`, `stage1_signed_gas_price_policy_wei`. Authentifizierte/secret-bearing URLs nur als bereinigte Anbieter-ID; Frische aus tatsächlich qualifizierter Evidenz, kein HTTP-Erfolg als Inclusion-PASS. | Monitoring, Launch als Verweis; native A′, teils projiziert |
| G14 | TEIL | Producer witness source: `original_successful_producer_execution_only`, `required_witness_schema`, `delivery_may_be_asynchronous`, `canonical_identity_binding_required`, `clock_and_uncertainty_metadata_required`, `operator_reference`; separates `source_ready` und Original-Witness-Zeit. | Monitoring/Analytics; native A′ |
| G15 | TEIL | Selection `previous_candidate_id`, `invalidated`, `guard_at_switch`, `activated_unix_ms` sowie Result `outcome/detail/decided_unix_ms`. Die im nativen Auswahlrecord invalidierten READINESS/SIGNED-BYTE-/ECONOMIC-APPROVAL-Bindungen anzeigen; kein automatisch wirksamer Request. | Launch; native A′/teilweise PublicFields |
| G16 | TEIL | Stabile Warn-Codes für `INVALID_EVIDENCE`, `V5_CONTRACT_MISSING`, `V5_CONTRACT_INVALID`, `PORTAL_DRIFT`, `WITNESS_SOURCE_NOT_READY`, `BUILDER_REFRESH_EXPIRED_OR_UNQUALIFIED`, `CANDIDATE_MIRROR_INVALID`. Vorerst opaque Strings, sichere Redaction; spätere Typisierung nur versioniert und mit Golden. | Overview/Launch/Monitoring; vorhandene warnings[] |
| G17 | FEHLT | `_import.{legacy_sha256,legacy_path}` vs `_provenance.{tool,observed_unix_ms}`, unterschieden nach `LEGACY_IMPORTED_AUDITED` / `NATIVE_CREATED` / `UNKNOWN` und originärem Hash. Importpfade ggf. redigiert; keine fingierte Vertrauensäquivalenz. | Analytics und Evidence-Drawer aller Reiter; native A′ |
| G18 | TEIL | Versionierte **aus Native gelesene** Konstanten/Policies wie block window Offset, Timestamp-/Builder-Timeout, Capture-Dauer, Receipt-Horizont, Clock-Lag und Frische-Schwellen. Fables Beispielzahlen vor Übernahme je Commit/Profil/Ausführungspfad abgleichen; 2 Blockschritte ≠ beweisbar fixe physikalische 900 ms. Nicht als TS-Hardcode festschreiben. | Launch kleine Window-Sektion; Native policy projection SOLL |
| G19 | JA | V2 §9.3 enthält G0.8 v5 inklusive First External BUY, Miner, N/N+1, Kosten, Canonical, Receipt Attribution, Unresolved, Producer-Witness. Achtung: G0.8 wurde für das tatsächliche Mainnet-Experiment noch nicht ausgeführt. | Monitoring/Analytics; native A/angestrebte strukturierte Anzeige |
| G20 | TEIL | G0.4-Census Source-Daten sind im derzeitigen Browser-Proto **nicht nativ projiziert**. Historische Markdown-Befunde dürfen nur klar als importierte offline Evidence mit eigener Provenienz erscheinen. Produktions-Source-Census bleibt SOLL. | Analytics; C |
| G21 | VORHANDEN (Fable-Zweifel widerlegt) | `g06a_requal_check.rs` enthält `DRY6_B_CANDIDATE`, und `g06a_final_dry.sh` hat B-DRY6-Bindung. Für die GUI getrennt `DRY5_LAUNCH_PATH` und `DRY6_B_CANDIDATE` projizieren; A muss **keinen** DRY6-B-PASS behaupten. | Launch; native A′ / Public DTO noch SOLL |
| G22 | JA, aber Fable-Regel korrigieren | `source_commit`, `evaluated_unix_s`, `data_as_of_unix_s` plus unabhängige pro-Evidence-Identität und `source quality`. Eine gemeinsame Provenienzzeile pro Reiter genügt, ausklappbare Details je Urteil. **Ein alter `data_as_of` ist kein `SOURCE_DETACHED`-Beweis**; Source-Detach nur aus tatsächlicher Feed-/Supervision-Liveness. | alle vier Reiter; A |
| G23 | TEIL | V5 `authorization`, `scope`, `wallet_separation`, `attempts`, `bundle_bytes`, `economics_validation`, `budget_wei`, `pre_arm_checks`, `success`, `retention_observability`, `thresholds`, `coverage`, `send_guard`, `dry_pass`, `candidate` als versionsabhängige Kategorien statt einer flachen Liste. Das sind **15 aufgeführte** fachliche Kategorien (Fable nennt 14); genaue Felder je Kategorie automatisiert aus V5-Schema ableiten. | Launch / BUY; `contract_fields` A, strukturierte Anzeige SOLL |

### 10.2 Prioritätsreihenfolge für eine echte Frontend-Abnahme

1. **Korrektheit vor Darstellungsarbeit:** `readiness.mode`, BLOCKED-vs-STOP, separater Feed-Quality-/Snapshot-Staleness-Vertrag, V5 statt V4, kein abweichendes Frontend-LIVE-Gate.
2. **Native G0.6a/G0.7-Qualifikation:** R0-Scan, Launch-Path, Quote, Sim-Gas, private/öffentliche Crosschecks, DRY5/DRY6, Selection-Invalidierung, Witness und zeitlich gültige Builder-Proofs als explizite DTOs. Verbot: generische Exposition von `SALT`, signierten Raw Bytes, Credentials oder privaten Dateipfaden.
3. **G0.7/G0.8-Verifikation:** Capture-/Journal, Attempts, Provider-Release/ACK, First External BUY, Miner und vollständiges Ordering/Acquisition/Cost/T0-Witness getrennt.
4. **Produktion nach Gate 0:** Source × Venue × Global-Mode, Source Health/Capability/Races, per-adapter Inclusion, Phase-F-Langzeit-Census, Phase-G/H-Economics, J-Release – **erst nach jeweils zulässigem BSC-Gate**, vorher nur SOLL/DISABLED/UNKNOWN.
5. **Visuelle Abnahme:** Graphite erhalten; fünf Reiter BSC inklusive Overview; kein neues sechstes Tab, keine doppelten Readiness-Bedienelemente, kein blindes Durchreichen aller ~123 V5-Kontraktstrings auf die Hauptfläche.

### 10.3 Pflichtabnahme des erweiterten Datenvertrags

Für jedes neu übertragene Feld eine maschinenlesbare Inventarzeile mit `source path + sha/symbol + schema/type/units + owner + provenance/evidence age + status/quality + sensitivity + presentation owner + scope/gate + negative/golden test`. Die Native→Proto→Gateway→TS-Abbildung muss exakte vorzeichenlose Wei-Strings, Optionals, canonical/reorg, stale/unknown und Guard-Fehler nachweislich erhalten. Bei fehlendem Backend-Datenowner gilt `SOLL_MISSING`, nicht ein erfundener Nullwert. Die neu vorgeschlagenen DTOs/Commands sind **nicht bereits implementiert**.

---

## 11. Ablage- und Implementierungsregel (Repository-Vertrag)

Dieses Dokument ergänzt ausschließlich den **einen** `frontend/docs/FRONTEND_PHASENPLAN.md`, ersetzt ihn nicht. Entsprechende BSC-Phasengates stehen ausschließlich in `bsc/docs/1_PLAN/PHASENPLAN.md`. Das Datenprotokoll bleibt `frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md`; `frontend/docs/SOURCE_VARIABLE_MATRIX.md` bleibt die domänenübergreifende Übersicht. Für den ausführbaren Arbeitsauftrag siehe `frontend/docs/ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md`. Die rein visuelle, nicht im App-Bundle verwendete Referenz liegt unter `frontend/design/reference/bsc_graphite_v9_1_review.html`.

**Weder neue React-App noch neue Backend-Engine:** Nur `frontend/src/features/bsc/` mit fünf vorhandenen Panels, vorhandene `frontend/src/app/projectRegistry.ts`, gemeinsame Graphite-Komponenten, `frontend/src/core/realtime/`, `gateway/` und bestehende `protocol/schemas/darkbot/bsc/v1/` ergänzen. Neue protobuf-Felder ausschließlich additiv/versioniert nach Rust-Owner-/Golden-Abnahme. Keine zweite Control-Registry, keine Schatten-DTO-Datenbank, keine UI-Economics, keine JS-Kryptographie und keine Engine-Reimplementierung.

**Vorgehens- und Statusfolge pro Feld:** Native (belegt) → ggf. sanitizierte native Statusprojektion → Proto-Schema/Golden → Gateway-Topic/Version/Quality → Adapter/Store → Graphite-Reiter → Tests/Evidence. Für noch fehlende Native-Owner bleibt der Eintrag `SOLL_MISSING` und der Control disabled. Unqualifiziertes Feld niemals durch hartkodierte UI-Werte ersetzen.

**Priorität:** Bestehende Read-only-Daten, V5-Labels, BLOCKED-vs-STOP und A/B-Auswahl zuerst; danach zusätzliche read-only G0.6a/G0.7/G0.8-Evidence mit Sicherheitsprüfung. C1/C2/FOREIGN_AUTO, Source×Venue LIVE, Strategie/H/J bleiben dokumentierte Phase-Ziele ohne Implementierungs-/Aktivierungsrecht vor den jeweiligen BSC-Gates. Die bestehende G0.7→G0.8→PASS/ADAPT/STOP-Stop-Regel ist unberührt.

## 12. Darstellungskontrakt für alle G1–G23-Felder: Basic / Advanced

Die Feld-/Control-Inventur A/B/C/D und G1–G23 wird **nicht reduziert oder dupliziert**, sondern der bestehenden BSC-React-Komponentenstruktur in **drei Sichtbarkeitsebenen** zugeordnet: `Basic` (default), `Advanced`, `Advanced > Technical details` (aufklappbar). Verbindlicher Master: [FRONTEND_PHASENPLAN.md §13](FRONTEND_PHASENPLAN.md#13-bsc-presentation-density--basic--advanced--technical-details-binding-2026-10-23). Dies ist Präsentationslogik, **nicht** `REPLAY/PAPER/SHADOW/LIVE` und keine neue Native-Control-ID.

**Für jede Feldinventarzeile verpflichtend ergänzen:** `display_tier` (BASIC_SUMMARY | ADVANCED | DETAIL_ONLY), `critical_always_visible` (bool), `summary_source` (identische native Projektion; keine berechnete fiktive Zahl), `frontend_panel`, `visibility_reason` und `quality/evidence`. Bereiche mit G0.7-/G0.8-Sicherheitsblockern oder unkonsistenter Identität dürfen in Basic **nicht** verborgen werden; technische Beweisdetails dürfen dagegen ausklappbar sein. Read-only Report-/History-Fetch nur auf aktive Ansicht/aufgeklappte Details begrenzen, soweit der bestehende Protocol-/F4-Vertrag es erlaubt; kein zusätzlicher WebSocket und keine doppelten topic-subscriptions.

**Inhalt pro Reiter:** Overview = Source, echter Readiness-Prozess, LIVE-Guard, Kandidat und Gates; Launch / BUY = Kandidat, Token, Bereit-/Freigabe-/Versuch-/Budget getrennt; Monitoring = verifizierte Quellen/Venues/Inclusion und tatsächlich belegte Akquisition; Analytics = echte Nenner/Fenster/Kosten und nur bei Evidenz berechnete Ergebnissummen; Wallets = Launcher/Buyer, Snapshot-Provenienz und Funding/Reserve. Die übrigen G1–G23-Details bleiben in Advanced/Disclosure verfügbar. `Wallets` bleibt als technischer Reiter-/Route-Key unverändert, ein Label `Wallets & Budget` bedarf lediglich visueller Abnahme.

**Qualitäts- und Sicherheitsregel:** `UNKNOWN`, `BLOCKED`, `STOP`, `UNARMED`, `SOURCE_DETACHED`, `INCOMPLETE`, `NOT_EXECUTED` und `UNQUALIFIED` werden in Basic nicht glattgebügelt. Als kritische Informationen auch bei geschlossenem Disclosure sichtbar bleiben: fehlende Freigabe, fehlende Producer-Originalzeit, kaputte/alte Evidenz, unbestätigte Inklusion, relevante fehlende Sicherheitsdaten, Quelle/Control nicht verfügbar. Source `DETACHED` basiert auf tatsächlicher Liveness; nur alter Snapshot = `STALE`. Keiner der UI-Modi darf Guard, NONCE, SOURCE/venue requested/effective, Builder-Send oder öffentliche Tokenidentität verändern. Kein weiteres Control, kein Schlüssel, kein gekauftes Provider-Abonnement.

**Tests:** fünf BSC-Reiter × beide Dichten × mindestens die Statusfälle LIVE-BLOCKED, Pre-Live-BLOCKED, STOP, PASS_READ_ONLY, Source DETACHED, Snapshot STALE, MISSING_EVIDENCE/UNKNOWN, Null/Optional/Wei-String, A/B-PENDING_DESKTOP_SYNC. Ein Header-Schalter, geteilte Datenreferenzen, keine zusätzlichen Actions/Protokoll-Aufrufe, Keyboard/Screenreader, responsive Graphite-Abgleich, Basic-vs-Advanced-Safety-Parität. Dashboard-Zahlen nur aus autoritativen Native-Projektionen; vollständige Field-F0-VAR-Tests gelten **für beide Modi**.

## 13. F0-Statusdelta und Fehlerreferenz

Die verbindlichen **Einzelfelder und Datenstufen** stehen in §§0–12 dieses Dokuments. Die bestätigten AU-01–AU-20 sowie N1/N3/N4/N5 sind ausschließlich im [Querschnitt-Audit](BSC_FRONTEND_QUERSCHNITT_AUDIT_2026-10-23.md) als Fehlerregister dokumentiert; N2 (`jobs` dauerhaft leer) wurde widerlegt: `publisher.rs` ergänzt `job()` vor dem STATE-Send. Die tatsächliche **Umsetzungsreihenfolge** steht ausschließlich in [ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md §E](ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md). Kein zusätzliches Feldinventar oder Bugfix-Parallelplan wird hier angelegt.

**Feldstatus:** `IST_CONNECTED | IST_BUG | SOLL_MISSING | FUTURE_PHASE_BLOCKED`. Bestehende `frontend/contracts/bsc-fields.json` und `frontend/contracts/bsc-native-fields.json` bleiben die Inventargrundlage; Source/Owner/Schema/Typ/Einheit/Evidence/Candidate/Attempt/Gate/Sensitivity/Proto-Tag/Topic/Store/UI/Test mit aktuellem SHA abgleichen. Nach Umsetzung von Paket 1 nur echte Proofs als VERIFIED kennzeichnen.
