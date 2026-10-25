# BSC – VERBINDLICHER FINALER IMPLEMENTIERUNGS-, BUGFIX-, INTEGRATIONS- UND BETRIEBSABNAHMEAUFTRAG V2

**Datum:** 25.10.2026  
**Repository:** `sawyerma/darkbot`  
**Ausgangsstand:** mindestens `87c064bad59e`, maßgeblich ist der jeweils aktuelle `origin/main`.

Vorgänger (historisch): [BSC_UMSETZUNGSAUFTRAG_V1.md](BSC_UMSETZUNGSAUFTRAG_V1.md). Fortschritt: Tabelle „Fortschritt Auftrag V2“ in [STATUS.md](../1_PLAN/STATUS.md).

## VERBINDLICHER HAUPTAUFTRAG

Claude, ich erteile dir hiermit den ausdrücklichen Auftrag, das bestehende BSC-Projekt auf Basis seines verbindlichen Phasenplans, sämtlicher Fehlerlisten und des tatsächlichen produktiven Codes **vollständig fertigzustellen, durchgängig zu integrieren, realitätsnah zu testen und technisch bis zur wirtschaftlichen Freigabegrenze startbereit zu machen.**

**Ich akzeptiere keinen weiteren Abschluss mit ausschließlich `LOCAL_PASS`, isoliert grünen Unit-Tests, teilweise angeschlossenen Funktionen oder unverbindlichen Restarbeiten.**

Dein bisheriger Abschlussbericht zum Stand `8b4dc23a` ist ein historischer Zwischenstand der Gate-A-/Gate-B-Nachbesserungen. Er ist ausdrücklich **keine vollständige technische Betriebsabnahme des BSC-Programms**.

Ich verlange:

1. Vollständige echte produktive Rust-/Go-Implementierung.
2. Vollständige Fehlerbehebung gemäß den drei BSC-Fehlerregistern.
3. Vollständige Native-, Wire-, Gateway- und Frontend-Integration.
4. Tatsächlich funktionsfähige ENABLE/DISABLE-Steuerungen statt Mocks.
5. Echte erfolgreiche Release-Builds.
6. Tatsächlich gestartete und geprüfte BSC-Prozesse.
7. Vollständige erlaubte READ_ONLY-, PAPER- und SHADOW-End-to-End-Tests.
8. Nachgewiesene Crash-, Restart-, Recovery-, Nonce-, Reorg- und Sicherheitssemantik.
9. Echte Performance-Messungen des produktiven Datenpfads.
10. Vollständige Dokumentation, reproduzierbare Evidence und unabhängige Phasenabnahme.
11. Einen technisch startbereiten Release-Kandidaten mit vollständigem Operator-Runbook.
12. Eine exakte Trennung zwischen technisch fertiggestellter Software und noch nicht autorisierten wirtschaftlichen Mainnet-Nachweisen.

**Arbeite konsequent weiter, bis sämtliche freigegebenen technischen Anforderungen erfüllt und nachweislich abgenommen sind.**

Keine neue Architektur, keine zweite Engine, keine Grundsatzdiskussion über bereits entschiedene Punkte.

---

## 0. Verbindlicher Phasenplan, Git-Stand und Dokumente

Vor jeder neuen Arbeitssitzung:

- Den tatsächlichen aktuellen `origin/main` ermitteln.
- Lokalen Checkout mittels sicherem Fast-Forward aktualisieren, soweit keine ungesicherten lokalen Änderungen entgegenstehen.
- Bestehende uncommittete Änderungen und unversionierte Evidence erhalten.
- Den verbindlichen Auftrag V2 und den aktuellen Fortschrittsstand lesen.
- Beim ersten tatsächlich offenen Arbeitspaket fortfahren.

**Kein `reset --hard`, kein Force-Push, kein ungeprüftes Überschreiben paralleler Arbeiten.**

Verbindliche Dokumente:

- `bsc/docs/1_PLAN/PHASENPLAN.md`
- `bsc/docs/1_PLAN/GATE0_TESTPLAN_SHORT.md`
- `bsc/docs/1_PLAN/BSC_PRODUCTION_BUILD_MASTER_PLAN.md`
- `bsc/docs/1_PLAN/STATUS.md`
- `bsc/docs/2_REGELN/BSC_BIRTH_ENTRY_ARCHITECTURE.md`
- `bsc/docs/2_REGELN/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md`
- `bsc/docs/5_OFFENE_FEHLER/fehlerliste_bsc_v1.md`
- `bsc/docs/5_OFFENE_FEHLER/BSC_Bug_V2.md`
- `bsc/docs/5_OFFENE_FEHLER/bsc_bug_v1.md`
- `bsc/docs/5_OFFENE_FEHLER/BSC_UMSETZUNGSAUFTRAG_V1.md`
- `frontend/docs/FRONTEND_PHASENPLAN.md`
- `frontend/docs/BSC_FRONTEND_BINDINGS_V1.md`

Zusätzlich sind die tatsächlich verwendeten Wire-, Protobuf-, Gateway-, Release-, Evidence- und Native-Verträge zu berücksichtigen.

### Maßgebliche Nutzerentscheidung

Die aktuelle verbindliche Entscheidung vom **25.10.2026** erlaubt:

**C0, C1, C2, C3, D, F, G, H und I vollständig offline entwickeln, integrieren und technisch abnehmen.**

Phase E ist ebenfalls umzusetzen, sobald die qualifizierten G0.1-Verträge für Four Classic und OpenFour eingefroren sind.

**J, KB und K erhalten dadurch keine wirtschaftliche oder operative Freigabe.**

Ältere widersprechende Entwicklungsstopps sind historisch und dürfen die heute freigegebenen technischen Arbeiten nicht mehr blockieren.

Der tatsächliche wirtschaftliche Mainnet-BUY und seine Qualifikation bleiben separat genehmigungspflichtig.

---

## 1. Verbindliche Definition des Zielzustands

Das BSC-Projekt wird anhand von drei streng getrennten Zuständen bewertet.

### 1.1 TECHNISCH VOLLSTÄNDIG

Alle innerhalb der freigegebenen Fachphasen geforderten Softwarefunktionen sind:

- Im echten produktiven Rust-/Go-Code vorhanden.
- Über ihre tatsächlichen produktiven Aufrufer erreichbar.
- Mit den bestehenden nativen Datenstrukturen verbunden.
- Mit echter Fehler-, Crash- und Recovery-Semantik implementiert.
- Über die vorhandenen Interfaces integriert.
- Soweit öffentlich erforderlich, bis ins bestehende BSC-Frontend verdrahtet.
- Durch reproduzierbare Tests und Builds nachgewiesen.

**Keine Ersatzimplementierung, kein Fake-Status, kein unverdrahtetes Modul.**

### 1.2 BETRIEBLICH OFFLINE ABGENOMMEN

Das tatsächliche BSC-Programm wurde mit seinen vorgesehenen Release-Artefakten gestartet.

Die erlaubten READ_ONLY-, PAPER- und SHADOW-Betriebsarten wurden mit realen Daten beziehungsweise echten gespeicherten Ereignissen durch die produktive Runtime geprüft.

Nachzuweisen sind mindestens:

- Startup und Shutdown.
- Konfiguration und Schema-Identität.
- Daten-Ingress.
- Native Verarbeitung.
- State und Reconciliation.
- Reporter und Gateway.
- Frontend.
- Journaling und Evidence.
- Restart und Recovery.
- Fehlersicherheit.
- Negative Sicherheitsfälle.
- Performance.

**Ein erfolgreicher Build allein ist keine Betriebsabnahme.**

### 1.3 WIRTSCHAFTLICHE QUALIFIKATION SEPARAT

G0.7, G0.8, tatsächlicher Token-Erwerb, Builder-Inclusion, wirtschaftliche Kosten und der physische Producer-Zeitnachweis benötigen teilweise echte externe Ausführung.

Was ohne entsprechende Genehmigung nicht tatsächlich geprüft werden kann, bleibt:

`EXTERNAL_BLOCKED`, `NOT_RUN`, `UNKNOWN` oder `UNQUALIFIED`.

**Niemals einen erfundenen PASS erzeugen.**

Der technisch fertiggestellte Release-Kandidat darf nicht automatisch `READY_FOR_LIVE` oder `REAL_BUY_PASS` heißen.

---

## 2. Gate A/B technisch vollständig abschließen

Arbeite sämtliche aktuell bestätigten und technisch ausführbaren Restfehler aus Gate A/B ab.

Bereits abgeschlossene und nachgewiesene Arbeiten erhalten. Nicht erneut implementieren.

### 2.1 BSC-005 – Attempt-Authority

Verbindlich gilt die T5-konforme Verbrauchsregel.

`return_unsent()` bleibt bestehen.

Eine Remote-Authority darf nur dann mittels CAS zurückgegeben werden, wenn ein kontrollierter Pre-Egress-Abbruch **nachweislich vor jeder möglichen Bundle-Byte-Ausleitung** stattgefunden hat.

Die Rückgabe muss ausgeschlossen sein bei:

- Überschrittener Release-Barriere.
- Crash oder Kill.
- UNKNOWN.
- Vorhandenem RELEASED-Marker.
- Fehlgeschlagenem oder unklarem Latch.
- Unklarer Remote-Authority.
- Fremdem oder veraltetem Claim.
- Nicht beweisbar bytefreiem Abbruch.

**Verbindliche Codeaufgabe:** Die Rückgabeberechtigung durch einen konsumierbaren Rust-Typ beziehungsweise eine gleichwertige statisch erzwungene Capability so kapseln, dass ein Rückgabeaufruf nach der Release-Barriere nicht kompiliert.

Notwendige Tests:

- Sämtliche relevanten Pre-Egress-Abbruchstufen.
- Korrekte Rückgabe einschließlich Journaleintrag.
- RELEASED-Marker verhindert Rückgabe.
- Kill zwischen CAS und Barriere.
- Restart nach Crash.
- Remote-CAS-Mehrhost-Race.
- Stale Claim.
- Zurückgegebener Claim liefert keinen gültigen Guard-Proof.
- Kein automatischer Retry.
- Fail-closed bei Remote-Unklarheit.

Die bestehenden Remote-CAS-Nachweise behalten ihren nachgewiesenen Umfang, ersetzen aber diese zusätzlichen Prüfungen nicht.

### 2.2 Bereits beschlossene technische Entscheidungen

**BSC-008 = Option A**

Bestehende `execution::bundle_wire`-Semantik:

- `send_head + 2`.
- `noMerge=true`.
- `positionFirst=true`.

Nicht real belegte Providerwirkungen bleiben `PROVIDER_UNQUALIFIED`.

**BSC-011 = Option B**

Chain-Ergebnis und Evidence-Vollständigkeit getrennt; bestehendes G0.8-Schema v6 und Gesamtverdikt beibehalten.

**BSC-049 = SELF_A**

SELF_A ist das verbindliche aktuelle Profil.

Keine automatische Umschaltung auf SELF_B oder Neugenerierung bereits eingefrorener wirtschaftlicher Parameter.

**Diese Entscheidungen nicht erneut abfragen.**

### 2.3 G0.8 – UNRESOLVED

Ein nicht aufgenommenes Bundle ohne kanonische Receipts ergibt `UNRESOLVED`.

Ebenso fehlende oder unvollständige Canonical-/Capture-/Acquisition-Evidence.

Es darf niemals daraus abgeleitet werden:

- Erfolgreicher BUY.
- Sicher kein Send.
- Automatisch wiederholbarer Versuch.
- PASS ohne Erwerbsnachweis.
- ADAPT allein wegen fehlender Receipts.

Den Fall durch die tatsächliche native G0.8-Verifikation bis zur bestehenden Frontend-Anzeige testen.

### 2.4 V5.26 und Readiness

Die aktuelle Deployment-Identität, Candidate-Auswahl, DRY-Evidence, Builder-Policy, Approval-Verträge, Nonces, Gas-/Budgetgrenzen und Attempt-Authority müssen konsistent sein.

PREPARE, SIGN/VERIFY, technische Readiness und LIVE-Autorisierung bleiben getrennte Autoritäten.

**Ein keyfreies PREPARE ist kein signierter Qualifikationsnachweis.**

Keine echten Mainnet-Wallet-Keys laden und keine reale SIGN-Ausführung ohne zusätzliche Freigabe.

---

## 3. BSC-050 – vollständige produktive Fehlerinventur

Führe genau einmal eine vollständige, quellcodebasierte Inventur aller produktiv erreichbaren BSC-Pfade durch.

Einzubeziehen sind:

- BSC Rust-Engine.
- Go/geth-Integration.
- C0–I Runtime.
- G0.7/G0.8-Qualification.
- Native Recorder/Journal.
- Konfiguration und Safety.
- Tatsächlich BSC-relevantes Protobuf.
- Reporter.
- Gateway.
- Frontend-Adapter und Stores.
- Produktive Featureflags.
- Release- und Startup-Pfade.

Prüfe auf:

- Mocks, Fakes, Stubs, Platzhalter.
- `todo!`, `unimplemented!` und produktiv erreichbare Panics.
- Tote produktive Aufrufpfade.
- Doppelte Engines, Signer oder Controller.
- Internes JSON/JSONL.
- Unerlaubte Legacy-Fallbacks.
- Hardcodierte dynamische Betriebsparameter.
- Clock-/Timestamp-Fehler.
- UNKNOWN→0.
- Unvollständige Evidence.
- Unsafe Retry/Nonce-Verhalten.
- Fehlende Crash-/Restart-Persistenz.
- Fehlende Frontend-Owner oder Fake-Controls.

Jeden Fund mit Datei, Funktion, Commit, aktivem Caller und konkretem Fehlernachweis erfassen.

**Bestätigte Fehler tatsächlich beheben**, nicht nur auflisten.

Test-Fixtures und ausdrücklich isolierte historische Analyseprogramme sind nicht pauschal produktive Bugs.

Keine wiederholten Inventuren bei jedem Sitzungswechsel.

---

## 4. C0–I vollständig implementieren

### C0 – Produktionsfundament

Vollständige native Startup-, Autorisierungs-, Recorder-, Replay-, Konfigurations- und Sicherheitsgrundlagen.

### C1 einschließlich C1.9/C1.9b/C1.10

Vollständige geth-/P2P-/Fast-Lane-Integration.

Frühester verifizierter actionable Ingress.

HOT0, Source-Race, Timestamp-Identitäten, Build-Identität, Regressionsmessungen und abschließender technischer C1.10-Freeze.

Bestehende gepinnte Upstream-Baseline-Fehler korrekt von eigenen Regressionen unterscheiden.

### C2 – BSC State

Vollständiger produktiver Pending-/Canonical-/Orphan-/Reorg-/Replacement-/Nonce-/UNKNOWN-/Recovery-State.

Ein echter nativer Zustands- und Intent-Owner.

Keine konkurrierenden State-Implementierungen.

### C3 – Lokale Execution

Tatsächlicher Build-/Sign-/Serialize-/Adapter-Datenpfad und korrekte lokale Latenzmessung.

Keine echten Mainnet-Signaturen.

### D – Flap

Vollständiger nativer Flap-Decoder für die tatsächlich verifizierten Deployment-/ABI-/Launch-/BUY-Versionen.

### E – Four Classic und OpenFour

Den erforderlichen G0.1-Vertragsfreeze **selbständig und read-only** durchführen.

Getrennt nach Four Classic und OpenFour nachweisen:

- Vertragsadressen und Chain.
- Aktuelle Implementierung/Proxy-Struktur.
- Deployment und Version.
- Verifizierte ABI.
- Launch-Selektoren und Calldata.
- BUY-Selektoren und Parameter.
- Token-/Adressableitung.
- Quote- und State-Anforderungen.
- Versionierte Evidence.

Jede Eigenschaft muss nachweisbar sein.

**Nicht belegbare Eigenschaften sind unsupported, niemals angenommene Protokollsemantik.**

Nach qualifiziertem Freeze Phase E vollständig nativ implementieren und technisch abnehmen.

### F – Native Market Evidence

Vollständige Capture-, Coverage-, Competitor-, Canonical-, Pending- und Journalverarbeitung.

### G – Economics

Native Quote-, Fee-, Gas-, Slippage-, Budget-, Kosten- und Plausibilitätsprüfung.

### H – Strategie

Native Strategie-, Admission-, Risk- und EV-Verarbeitung inklusive reproduzierbarem Replay.

### I – PAPER/SHADOW und Requalifikation

Vollständiger produktiver Datenpfad bis zur sicheren PAPER-Senke.

Realitätsnahe Source-Races, Nonce-/Replacement-/Reorg-Szenarien und finale lokale HOT0-/Transport-Messungen.

**Keine Phase als TECHNISCH FERTIG deklarieren, wenn erforderliche Module fehlen oder nur isolierte Tests funktionieren.**

---

## 5. ENABLE/DISABLE vollständig und echt implementieren

Das ist verbindlich.

Eine noch nicht wirtschaftlich freigegebene Funktion soll **fertig entwickelt, aber sicher deaktiviert** sein.

Nicht weglassen. Nicht simulieren.

Erforderlich sind:

- Echter nativer Rust-/Go-Owner.
- Typisierte und validierte Konfiguration.
- Persistente Zustände.
- Getrenntes `requested` und `effective`.
- Generation/CAS.
- Idempotenz und Receipt.
- Crash-/Restart-Verhalten.
- Native Sicherheitsfreigabe.
- Echtes Status-Protobuf.
- Bestehendes Frontend.
- Positive und negative Integrationstests.

Für Source-/Venue-Modi gelten die vorgesehenen `OFF | SHADOW | LIVE`-Zustände.

Ein Nutzerwunsch `requested=LIVE` darf ohne passende fachliche und wirtschaftliche Freigabe **keinen aktiven LIVE-Zustand erzeugen**.

Stattdessen:

`effective=OFF/BLOCKED`

mit konkretem `block_reason`.

Der produktive Guard muss verhindern, dass SIGN-/SEND-/BUY-Egress möglich wird.

**Eine reine React-Sperre ist kein Sicherheitsmechanismus.**

Noch nicht autorisierte wirtschaftliche Aktionen dürfen auch nicht über frei erfundene Gateway-Kommandos erreichbar sein.

---

## 6. BSC-Frontend vollständig integrieren

Die vorhandenen fünf BSC-Reiter bleiben unverändert:

1. Overview.
2. Launch / BUY.
3. Monitoring.
4. Analytics.
5. Wallets.

Keine neue Architektur, kein zweites Dashboard und kein zusätzlicher Store.

Der tatsächliche Datenpfad muss funktionieren:

Native Rust/Go → binäre Statusprojektion → Protobuf → BSC-Reporter → Gateway → bestehender Protobuf-WebSocket → BSC-Adapter → Store → passender Reiter.

Für erlaubte Commands ebenfalls der echte Rückweg:

Frontend → authentisiertes Gateway → gültiger Native-Owner → CAS/Guard/Receipt → neue native Statusprojektion → Frontend.

Jedes neue relevante öffentliche Feld gehört in die vorhandene F0-VAR-Inventur.

Kein Feld erhält einen erfundenen Publisher.

Kein GUI-Schalter erhält einen Mock-Controller.

Alle Komponenten müssen mit der tatsächlich aktuellen gemeinsamen Protobuf-Schema-ID kompatibel sein.

**BSC und Project_0 strikt getrennt halten.**

Geteilten Gateway-/Protobuf-Code nur unter Erhalt aller Project_0-Invarianten und mit erforderlicher Owner-Abstimmung ändern.

---

## 7. Tatsächliche Release- und Betriebs-End-to-End-Tests

**Dieser Abschnitt ist zwingend. Unit-Tests allein reichen nicht.**

### 7.1 Release und Prozessstart

Baue die tatsächlichen vorgesehenen Release-Artefakte.

Starte die erlaubten Komponenten des BSC-Systems über den bestehenden Startup-/Release-Pfad.

Prüfe:

- Startup.
- Shutdown.
- Restart.
- Native Konfiguration.
- Chain-/Schema-/Build-Identitäten.
- Native Engine und Reporter.
- Gateway.
- Frontend.
- Echte Statusaktualisierungen.
- Journaling.
- Persistenz.
- Recovery.
- Fehlende Inputs.
- Disconnect/Reconnect.
- Queue-Overflow.
- Backpressure.
- Error propagation.
- Sicheren Betriebsabbruch.

**Ein gestarteter echter Prozess mit echter Verarbeitung ist Pflicht**, sofern die jeweilige Phase dies fachlich erfordert.

### 7.2 Synchronisierter Mainnet-geth

Vor geth-spezifischen Live-Beobachtungstests prüfen, ob ein tatsächlich synchronisierter und korrekt gepinnter Mainnet-bsc-geth verfügbar ist.

**Keine Umgehung der Mainnet-Sync-Anforderung.**

Eine eventuell vorhandene Ausnahme zur TX-Annahme ohne Sync bleibt ausschließlich auf die dafür vorgesehenen Chapel-Testnet-Fälle beschränkt.

Falls kein geeigneter synchronisierter Mainnet-Knoten existiert:

- Den tatsächlichen Zustand dokumentieren.
- Die fehlenden Hardware-/Speicher-/Sync-Voraussetzungen konkret ermitteln.
- Kapazität, Laufzeit und zusätzliche Kosten nachvollziehbar beziffern.
- Mir eine separate Kostenentscheidung vorlegen.
- Keine Infrastruktur ungefragt kostenpflichtig buchen.
- Keine Fake-geth-Daten als Mainnet-Evidence ausgeben.

Bis dahin mit bereits zulässigen **FREE_ONLY-read-only**-Quellen wie PublicNode, dRPC und BlockReq die tatsächlich möglichen E2E-Prüfungen durchführen.

geth-spezifische offene Nachweise als `EXTERNAL_BLOCKED` führen.

**Ein fehlender eigener Mainnet-Knoten darf andere ausführbare Softwaretests nicht blockieren.**

### 7.3 Echte read-only Mainnet-Verarbeitung

Mit real verfügbaren Quellen tatsächliche BSC-Ereignisse verarbeiten:

- Pending-/Full-/Hash-only-Daten.
- Verifizierung signierter Transaktionen.
- Venue-/Deployment-Erkennung.
- Token-Ableitung.
- Canonical-Status.
- Duplicate-Dedup.
- Konkurrenz und früheste actionable Source.
- Source-Health.
- Capture und Evidence.
- Frontend-Status.

Nur nachgewiesene Eigenschaften als qualifiziert melden.

### 7.4 PAPER-/SHADOW-Gesamtpfad

Den echten produktiven Native-Pfad ausführen:

Ingress → Decoder → State → Quote/Economics → Strategy/Admission → Build → technisch zulässige Signatur/Serialisierung → PAPER-Senke.

**PAPER-Signaturregel:**

- Nur eine dedizierte lokal erzeugte PAPER-Identität ohne Guthaben.
- Strikte Trennung von G0.7- und Chapel-Keys.
- Außerhalb des Repositorys speichern.
- Niemals produktive Mainnet-Wallets in PAPER/SHADOW laden.
- Die PAPER-Senke muss nachweislich keine Mainnet-Bundle-/Transaktions-Schreibfähigkeit haben.
- Signierte PAPER-Bytes dürfen nicht an Mainnet-Provider gelangen.

Wenn der PAPER-Test Chain-56-formatierte Transaktionen erzeugt, sind sie ausschließlich lokale technische Testartefakte, **keine Freigabe zur Mainnet-Ausleitung**.

Die zulässige PAPER-Senke ist eine echte vorgesehene Betriebsart. Sie darf nicht als Ersatz für fehlenden produktiven Code missbraucht werden.

### 7.5 Chapel-Testnet

Die laut Phasenplan erlaubten C1.7-/C1.9b-Qualifikationen dürfen Chain 97 nutzen.

Voraussetzungen:

- Separierte Testnet-Identitäten.
- Keine echten Mainnet-Keys.
- Prüfung der tatsächlichen Chain-ID.
- Nur zulässige Testnet-Transaktionen.
- Keine Übertragung der Ergebnisse als Mainnet-Kaufnachweis.

### 7.6 Fehler- und Crash-Tests

Relevante Fälle einschließlich Disk-full, Journalfehler, fehlender State, beschädigter Evidence und Recovery prüfen.

**Disk-full- und Journalfehler ausschließlich in einem isolierten Test-Volume oder geeigneten kleinen Test-Disk-Image provozieren.**

Niemals absichtlich das Arbeitsvolume oder das produktive Evidence-Verzeichnis füllen.

---

## 8. G0.4-Marktmessung – verbindlich und unabhängig

Die Rohdatenerfassung G0.4/V10-8 darf sofort beginnen, sofern sie Gate-0-Vorbereitung nicht verzögert.

Vorgesehen ist eine begrenzte read-only-Messung, standardmäßig höchstens sieben Kalendertage.

Erfasse:

- Anzahl fremder Launches pro Tag.
- Vor-Block-Sichtbarkeit.
- Anteil vollständiger verifizierbarer/actionable Ereignisse.
- Anteil fachlich geeigneter Venue-/Token-Profile.
- Source- und Canonical-Coverage.
- Konkurrenz und tatsächliche beobachtbare Platzierung.
- Private/nicht sichtbare Launches als Messgrenze.

**Entscheidungskriterien vor Auswertung:**

Claude schlägt Denominator, Mindeststichprobe und numerische Schwellen für `WEITER / UMSTEUERN / STOPP` vor.

**Ich bestätige diese Kriterien einmal ausdrücklich vor der ersten bewertenden Auswertung.**

Erst danach werden sie versioniert eingefroren.

Keine rückwirkende Schwellenanpassung zur Verbesserung des Ergebnisses.

Die reine Datenerfassung kann unabhängig davon bereits laufen.

Eine ausstehende Bestätigung dieser wirtschaftlich relevanten Kriterien blockiert nicht C1–I-Codearbeiten.

---

## 9. Tatsächliche Performance-Abnahme

Das physische Projektziel bleibt:

**`T_buy − T0 < 1.000.000 ns`**

Dieser Wert ist ohne geeigneten Original-Producer-Witness nicht tatsächlich nachgewiesen.

Keine Umdeutung von:

- Builder-ACK.
- N+1-Position.
- Blockzeit.
- Lokalem Timestamp.
- `T_send`.
- Simuliertem Fill.

Diese Ergebnisse müssen getrennt bleiben.

Zusätzlich die lokal tatsächlich messbaren Produktionsgrenzen vollständig prüfen:

- Erste verifizierte actionable Source.
- Source-Race.
- Decode.
- State.
- Quote.
- Admission.
- Build.
- Signatur und Serialisierung im zulässigen Testmodus.
- Adapter-Queue.
- Transport-Handoff.
- HOT0.
- CPU und Speicher.
- Backpressure und Drops.
- Restart und Recovery.

Berichte p50, p95, p99, Maximum, Sample-Zahl, Messumgebung und Fehlernenner.

Nur echte reproduzierbare Messungen des tatsächlich verwendeten Codes zählen.

Vor wirtschaftlichem Produktions-LIVE (J) bleibt der qualifizierte Original-Witness oder die laut Phasenplan separat zu bestätigende Risikoentscheidung auf Basis tatsächlicher PAPER-/SHADOW-Konkurrenzmessungen erforderlich.

---

## 10. DEFINITION VON FERTIG – ABSOLUT VERBINDLICH

Ein Ticket darf ausschließlich dann **TECHNISCH FERTIG** heißen, wenn:

1. Die zutreffende Anforderung vollständig im echten produktiven Code implementiert ist.
2. Kein produktiver Mock/Stub/Fake/Placeholder an ihre Stelle tritt.
3. Alle erforderlichen produktiven Aufrufer angeschlossen sind.
4. Sämtliche relevanten Native-, Wire-, Safety- und State-Verträge funktionieren.
5. Der erforderliche reale Daten- beziehungsweise Command-End-to-End-Pfad getestet wurde.
6. Öffentliche Funktionen bis in den bestehenden Frontend-Reiter integriert sind.
7. Die relevanten positiven, negativen, Crash-, Restart-, UNKNOWN-, Recovery-, Nonce-, Race- und Reorg-Fälle bestanden wurden.
8. Die erforderlichen Compiler-, Format-, Clippy-, Rust-, Go-, Proto-, Gateway-, Frontend- und Release-Gates erfolgreich sind.
9. Ein erforderlicher tatsächlicher Programmstart erfolgreich geprüft wurde.
10. Evidence, Testbefehle, Exitcodes, Build-Identitäten und Git-SHA dokumentiert sind.
11. Das vollständige Paket nach bestandenem Test auf `origin/main` gepusht und überprüft wurde.

**Ein unabhängiger read-only Reviewer wird pro vollständigem Phasengate eingesetzt, nicht automatisch pro Ticket.**

Ein Ticket kann technisch fertig gemeldet werden, wenn sein eigener Nachweis vollständig ist. Die **Phase** wird erst nach vollständigem Phasengate einschließlich unabhängigem Reviewer abgeschlossen.

HIGH-/CRITICAL-Befunde müssen vorher behoben werden.

### Zulässige Abschlusszustände

**TECHNISCH FERTIG**

Sämtliche zutreffenden technischen Implementierungs-, Integrations-, Betriebs- und Testanforderungen erfüllt.

**TECHNISCH FERTIG — EXTERNAL_BLOCKED: ...**

Sämtliche technischen Anforderungen erfüllt, ausschließlich genau bezeichnete Mainnet-/Producer-/Provider-/wirtschaftlich genehmigungspflichtige Nachweise fehlen.

**OFFEN**

Eine technische Implementierungs-, Integrations- oder Testanforderung fehlt. Mit genauer Datei, Ursache und nächstem Schritt.

**Keine endgültige Abschlussmeldung mit lediglich `CODE_FIXED`, `LOCAL_PASS`, `PARTIAL`, `READY_IN_PRINCIPLE` oder „weitgehend fertig“.**

Der tatsächliche externe LIVE-/Erwerbs-PASS darf niemals durch TECHNISCH FERTIG ersetzt werden.

---

## 11. Operator-Runbook und Startbereitschaft

Erstelle ein vollständiges Runbook aus den tatsächlich vorhandenen Binaries, Konfigurationen und validierten Befehlen.

Es muss enthalten:

- Reproduzierbaren Release-Build.
- Verbindliche Binary-/Schema-Identitäten.
- Tatsächliche Start-/Stop-/Restart-Befehle.
- READ_ONLY-/PAPER-/SHADOW-Konfiguration.
- RPC-/Provider-Voraussetzungen.
- Prozess- und Gateway-Health.
- Frontend-Verbindungsprüfung.
- Log- und Evidence-Standorte.
- Zustands-/Journalprüfung.
- Recovery-Verfahren.
- Fehler- und Abbruchbedingungen.
- Wirtschaftliche Freigabegrenzen.
- G0.7-/G0.8-Operator-Prüfablauf.
- Später erforderliche Disarm-Schritte.

**Runbook und real ausführbarer Code müssen exakt zusammenpassen.**

Für einen später separat freigegebenen Mainnet-Versuch vorbereiten:

Funding → SIGN/VERIFY → V5-Vorschlag → PASS_READ_ONLY → menschliche Prüfung des exakt gebundenen Vertrags → separate wirtschaftliche Freigabe → Arming → Grant → ein Versuch → Disarm nach Runner-Ende → G0.8 mit frisch qualifiziertem Archiv-Endpunkt.

Die konkrete V5-Prüfliste muss Kandidat, Token, signierte Hashes, tatsächliche Nonces, Gas, Budget, Provider, Bundle-Semantik, Policy-Alter und Authority-Identität enthalten.

Der vorbereitete Disarm darf nicht vorzeitig wirksam gepusht werden.

**Keinen dieser wirtschaftlichen Schritte ohne separate Genehmigung tatsächlich ausführen.**

---

## 12. Git, Reviewer und Sitzungsdisziplin

Arbeite in fachlich abgeschlossenen Paketen und pushe nur nachvollziehbar geprüfte Commits.

- Keine zweite Engine.
- Keine parallele Architektur.
- Kein ungeprüfter Legacy-Fallback.
- Kein internes JSON statt nativer Binärdaten.
- Kein unerlaubtes Hardcoding.
- Keine produktiven Mocks.
- Kein Force-Push.
- Keine Secrets im Repository.
- Keine unkontrollierten gemeinsamen Project_0-Änderungen.
- Keine Agentenkaskaden.
- Keine Wiederholung abgeschlossener Arbeit.

**Pro Ticket standardmäßig kein zusätzlicher Reviewer.**

**Pro Phasengate genau ein unabhängiger read-only Reviewer auf dem exakten finalen SHA.**

Das vollständige Phasengate muss zusätzlich reale Tests, Integration, Builds und gegebenenfalls den Runtime-Start enthalten.

Nach bestandener technischer Abnahme ohne neue Nutzerfrage zur nächsten offline freigegebenen Phase.

Nur tatsächlich erforderliche wirtschaftliche Nutzerentscheidungen, Kostenentscheidungen oder fachlich nicht auflösbare technische Blocker dürfen neue Freigaben erfordern.

---

## 13. Abschlussbericht je Paket und Phase

Berichte ausschließlich anhand überprüfter Evidence.

| Feld | Inhalt |
|---|---|
| Ticket/Phase | Bestehende Original-ID |
| Git | Exakter geprüfter und gepushter SHA |
| Code | Geänderte Dateien und Funktionen |
| Native Caller | Tatsächlich produktiv erreichbare Aufrufkette |
| Runtime | Wirklich gestarteter Prozess und Betriebsmodus |
| Integration | Native, Reporter, Gateway, Frontend |
| Tests | Tatsächliche Befehle, Resultate und Exitcodes |
| Safety | Guard-/Crash-/Nonce-/Egress-Negativtests |
| Performance | Messwerte und Messgrenzen |
| Reviewer | Finaler SHA und unabhängiges Prüfergebnis je Phase |
| Fehlende Evidence | Konkrete EXTERNAL_BLOCKED-/NOT_RUN-Punkte |
| Endstatus | TECHNISCH FERTIG / EXTERNAL_BLOCKED / OFFEN |

Nach allen freigegebenen Phasen eine **vollständige technische BSC-Gesamtbetriebsabnahme** durchführen.

Dabei müssen tatsächlich gestartete Komponenten, verifizierte Datenwege, funktionsfähige native Funktionen, Frontend-Anbindungen, Guards, Restart-/Recovery-Ergebnisse, Performance und externe Grenzen dokumentiert sein.

**Kein globales `READY_FOR_LIVE` ohne tatsächliche wirtschaftliche und operative Qualifikation.**

---

## 14. Absolute wirtschaftliche Grenze

Dieser Auftrag autorisiert ausdrücklich:

**ERLAUBT**

- Vollständige Offline-Entwicklung C0–I.
- Technisch erforderlicher G0.1-Four-Vertragsfreeze.
- Native Rust-/Go-Implementierung.
- Produktive Integration.
- Release-Builds.
- Echte erlaubte Runtime-Starts.
- Kostenlose read-only Mainnet-Beobachtung.
- Gesondert zulässige Chapel-Testnet-Prüfungen.
- Sichere PAPER-/SHADOW-Verarbeitung.
- Technische Signaturtests mit separaten wertlosen Identitäten ohne Mainnet-Egress.
- Native Sicherheitsguards.
- Fehlerbehebung und Regressionen.
- Technische Gesamtbetriebsabnahme.

**NICHT ERLAUBT**

- Echte Mainnet-Wallet-Signaturen ohne gesonderte Freigabe.
- Funding.
- ARM.
- Mainnet-Nonce-Burn.
- Mainnet-SEND/Broadcast.
- Tatsächlicher Mainnet-BUY.
- Wirtschaftliches LIVE.
- Automatische Wiederholungsversuche.
- Kostenpflichtige Provider oder Infrastruktur ohne ausdrückliche Kostenentscheidung.
- Eigenmächtige Aktivierung von J/KB/K.

Der Guard bleibt:

`FROZEN_APPROVED_V5_SHA256="UNARMED"`

`approved=false`

`execution_ready=false`

**Diese Grenzen dürfen nicht als Begründung dienen, die unabhängig zulässige vollständige Softwareentwicklung liegenzulassen.**

---

## 15. VERBINDLICHE PERSISTENZ UND ARBEITSORGANISATION ÜBER ALLE SITZUNGEN

### 15.1 Diesen Auftrag dauerhaft im Repository verankern

Lege den vollständigen Auftrag als

`bsc/docs/5_OFFENE_FEHLER/BSC_UMSETZUNGSAUFTRAG_V2.md`

im Repository ab.

Die vorhandene V1 bleibt als historischer Vorgänger erhalten und erhält einen klaren Verweis auf V2.

In

`bsc/docs/1_PLAN/STATUS.md`

eine dauerhaft gepflegte Fortschrittstabelle führen.

Mindestspalten:

| Paket | Phase/Tickets | Technischer Status | Geprüfter SHA | Fehlende Nachweise | Nächster Schritt |
|---|---|---|---|---|---|

Die Tabelle wird nach jedem sinnvoll abgeschlossenen Implementierungs-/Testpaket aktualisiert.

**Jede neue Sitzung beginnt mit:**

1. Sicherem Git-Abgleich.
2. Lesen des verbindlichen V2-Auftrags.
3. Lesen der Fortschrittstabelle.
4. Prüfung des aktuellen tatsächlichen HEAD.
5. Fortsetzung des ersten offenen Pakets.

Keine erneute allgemeine Grundsatzprüfung und keine vollständige Neuinventur bei jeder Sitzung.

**Jede Sitzung endet mit:**

- Gepushten abgeschlossenen und geprüften Paketen.
- Aktualisiertem Fortschrittsstand.
- Klarem nächsten technischen Arbeitsschritt.
- Dokumentation unvollständiger lokaler Arbeiten, ohne sie als fertig zu behaupten.

Kein automatischer Push ungetesteter WIP-Implementierungen als abgenommene Pakete.

### 15.2 Synchronisierter Mainnet-Knoten

Die Verfügbarkeit eines korrekt synchronisierten und gepinnten Mainnet-bsc-geth ist vor geth-spezifischen E2E-Qualifikationen zu prüfen.

Fehlt er, sind die tatsächlichen technischen und wirtschaftlichen Voraussetzungen zu ermitteln.

Keine stille Beschaffung, keine ungefragten laufenden Kosten, keine falsche Mainnet-geth-Evidence.

Alternative erlaubte FREE_ONLY-Quellen dürfen für ihren nachweisbaren realen Umfang verwendet werden.

### 15.3 Four-Vertragsfreeze

Der G0.1-Four-Freeze ist Teil dieses Auftrags und wird von dir selbständig read-only durchgeführt.

Kein dauerhaftes Warten auf einen nicht zugewiesenen Owner.

Phase E nach qualifiziertem Freeze vollständig entwickeln.

### 15.4 PAPER-Sicherheitsidentität

PAPER-/SHADOW-Signaturen ausschließlich mit isolierten lokalen Testidentitäten.

Keine produktiven Mainnet-Keys laden.

Keine Netzwerk-Schreibfähigkeit der PAPER-Senke.

Echte technische Testsignaturen dürfen keinen Mainnet-Egress ermöglichen.

### 15.5 G0.4-Kriterien

Rohdatenerfassung sofort im freigegebenen READ_ONLY-Umfang möglich.

Numerische WEITER-/UMSTEUERN-/STOPP-Kriterien, Mindeststichprobe und Denominator vor der bewertenden Auswertung vorschlagen.

Einmalige ausdrückliche Nutzerbestätigung einholen, versioniert einfrieren und anschließend nicht ergebnisorientiert verändern.

### 15.6 Reviewer und isolierte Fehlertests

Die unabhängige Review-Pflicht gilt für das vollständige Phasengate, nicht zusätzlich für jedes einzelne Ticket.

Disk-full-/Journal-Fehler ausschließlich in isolierten Test-Volumes erzeugen.

### 15.7 Keine erneute Dokumentationsschleife

Nach Ablage von V2 und Anlage der Fortschrittstabelle liegt der Schwerpunkt auf der **tatsächlichen Codeimplementierung und Betriebsabnahme**.

Dokumentation ist Nachweis der Arbeit, nicht Ersatz für Arbeit.

---

## 16. VERBINDLICHE AUSFÜHRUNGSREIHENFOLGE

**Paket 0 – Dauerhaften Auftrag einrichten**

V2 im Repository ablegen, V1 historisch kennzeichnen, Fortschrittstabelle in STATUS.md anlegen, aktuellen SHA festhalten.

**Paket 1 – Gate A/B final technisch schließen**

Alle offenen echten Code-/Integrationsdefekte beheben, insbesondere BSC-005-Barrier-Typabsicherung, G0.8-UNRESOLVED und aktuelle Schema-/Readiness-Regressionen.

**Paket 2 – BSC-050**

Vollständige einmalige produktive Inventur durchführen. Bestätigte Defekte ihren Phasen zuordnen und beheben.

**Paket 3 – C0/C1**

Sämtliche erforderlichen Native-Funktionen integrieren, C1.9/C1.9b qualifizieren, C1.10 technisch einfrieren und vollständig testen.

**Paket 4 – C2**

Native State-/Nonce-/Pending-/Reorg-/Replacement-/Recovery-Engine vollständig implementieren und technisch abnehmen.

**Paket 5 – C3/D/E**

C3 und D vollständig entwickeln. Parallel den read-only G0.1-Four-Freeze durchführen, anschließend E vollständig integrieren.

**Paket 6 – F/G/H**

Native Marktevidenz, Economics und Strategie bis in die produktiven Aufrufpfade vollständig implementieren.

**Paket 7 – I**

Vollständigen PAPER-/SHADOW-End-to-End-Betrieb mit dem tatsächlich implementierten Produktionscode durchführen.

**Paket 8 – Technische Gesamtbetriebsabnahme**

Release-Build, echte Runtime-Starts, Native-Wire-Gateway-Frontend, Recovery, Sicherheitsguards, Performance, Runbook und unabhängigen Phasen-/Gesamtreview vollständig abschließen.

**Paket 9 – Finaler technischer Release-Kandidat**

Exakten Git-SHA, Build-Artefakte, Konfiguration, Startbefehle, reproduzierbare Evidence, bestandene technische Gates und ausschließlich noch extern notwendige Freigaben dokumentieren.

Die G0.4-Rohdatenerfassung und der G0.1-Four-Freeze können parallel laufen, solange keine wirtschaftliche Grenze verletzt wird und die vereinbarte Gate-0-Priorität erhalten bleibt.

---

## 17. ENDGÜLTIGE ARBEITSANWEISUNG

Claude, **höre nicht nach der nächsten grünen Testsuite oder dem nächsten `LOCAL_PASS` auf.**

Ich will keine erneute Meldung:

- „Weitgehend fertig.“
- „Code implementiert, Integration folgt später.“
- „Der Owner muss irgendwann noch den Publisher bauen.“
- „Unit-Tests grün, daher abgeschlossen.“
- „C1/C2 noch gesperrt.“
- „Warten auf Mainnet-BUY, bevor weiterer Offline-Code entwickelt werden darf.“
- „Frontend-Status vorhanden, Backend später.“
- „Mock funktioniert, reale Implementierung folgt.“

Solche Meldungen sind kein technischer Abschluss, wenn die fehlende Arbeit im freigegebenen Scope jetzt ausführbar ist.

**Ich will das vollständige tatsächlich funktionsfähige BSC-Programm, nicht nur einzelne funktionsfähige Bestandteile.**

Arbeite alle freigegebenen Phasen und bestätigten Bugs vollständig ab, integriere alles in die bestehende Runtime, starte die realen Prozesse, teste den vollständigen zulässigen Betriebsweg und dokumentiere die tatsächlichen Ergebnisse.

**Wenn anschließend ausschließlich ausdrücklich genehmigungspflichtige Mainnet-/Provider-/Producer-/LIVE-Nachweise fehlen, dann ist das der sauber ausgewiesene letzte externe Blocker.**

Diesen Blocker nicht mit einem Mock umgehen, aber auch nicht als Vorwand verwenden, die restliche Softwareentwicklung zu stoppen.

**Verbindliches Endziel: technisch vollständiges, real gestartetes, integriert getestetes und unabhängig abgenommenes BSC-System mit vorbereitetem, gesondert freizugebendem G0.7-/G0.8-Betriebsweg.**

**Beginne jetzt mit Paket 0 und arbeite die gesamte Reihenfolge fortlaufend ab. Keine neue Planungsrunde, keine Wiederholung erledigter Arbeit und keine erneuten Grundsatzfragen zu bereits getroffenen Entscheidungen.**
