# BSC REV.1.0 – VERBINDLICHER GESAMTAUFTRAG ZUR VOLLSTÄNDIGEN FERTIGSTELLUNG

**Stand: 05.11.2026** (Nutzerauftrag, wörtlich übernommen; Umsetzungsstand in `STATUS.md`, Gate-Zeilen im Messplan §2a.5)

**Dieser Auftrag ersetzt die bisherigen einzelnen Ergänzungen und Anweisungen zur Fertigstellung von BSC REV.1.0. Er ist vollständig und als ein zusammenhängender Auftrag umzusetzen.**

Der bestehende BSC-PHASENPLAN bleibt fachlich maßgeblich. Die nachfolgend ausdrücklich geänderte lokale Abnahmeregel hat Vorrang vor der bisherigen GitHub-CI-Pflicht. Alle übrigen bestehenden Projektentscheidungen, Phasenanforderungen und wirtschaftlichen Freigabegrenzen bleiben unverändert.

## 1. Ziel und oberste Priorität

BSC REV.1.0 soll zu einem vollständig funktionsfähigen, schnellen, stabilen und wirtschaftlich korrekt arbeitenden automatischen Handelssystem fertiggestellt werden.

Das System muss auf dem **lokalen Mac** zuverlässig funktionieren. Es soll Launches möglichst früh erkennen, über Strategy C automatisch BUY-Entscheidungen treffen, Transaktionen mit minimaler Latenz ausführen und gehaltene Positionen anhand der vorhandenen Strategy-C-Regeln automatisch verkaufen können.

Die Umsetzung muss insbesondere folgende Eigenschaften besitzen:

- Vollständige Integration aller laut Phasenplan vorgesehenen Komponenten.
- Unterstützung der qualifizierten Flap-, Four-Classic- und OpenFour-Handelspfade.
- Automatische BUY-/SELL-Ausführung über die bestehende Strategy C.
- Korrekte Berechnung sämtlicher Gebühren, Gasreserven, Budgets, Positionen und Nettoergebnisse.
- Minimale Latenz zwischen handelbarer Launch-Erkennung und Beginn des Sendens.
- Zuverlässiger Dauerbetrieb mit korrektem Restart, Recovery und Positionsmanagement.
- Vollständige Bedienbarkeit über die vorgesehenen Terminal- und Frontend-Funktionen.
- Nachweislich bestandene Tests auf der tatsächlichen Betriebsplattform.

**Die wirtschaftliche Funktionsfähigkeit und Performance sind zentrale Entwicklungsziele. Die Software soll nicht lediglich möglichst viele Tests bestehen, sondern im vorgesehenen Betriebsfall korrekt, schnell und zuverlässig arbeiten.**

Keine zusätzlichen Auditprogramme, Architekturprojekte oder Dokumentationsschleifen eröffnen, die nicht zur tatsächlichen Fertigstellung beitragen.

---

## 2. Verbindliche Entwicklungsregeln

### 2.1 Architektur und Implementierung

1. Die bestehende Architektur und vorhandene Komponenten bevorzugt wiederverwenden.
2. Strategy C bleibt die einzige BSC-Handelsstrategie. Keine neuen Strategien, Handelsmodi oder parallelen BUY-/SELL-Ausführungssysteme einführen.
3. C3, Strategy C, Venue-Adapter, LIVE-Owner und bestehende Transaktionsbindungen müssen korrekt zusammenarbeiten.
4. Berechtigte Fehler an ihrer Ursache beheben, nicht lediglich Symptome über zusätzliche Bedingungen verdecken.
5. Refactoring ist ausdrücklich erlaubt, wenn es nachweislich die Korrektheit, Performance, Wartbarkeit oder Integration verbessert.
6. Keine unnötigen Abstraktionen, zusätzlichen Laufzeitebenen oder doppelten Zustandsführungen schaffen.
7. Bestehende Handelsparameter, Defaults, wirtschaftliche Schalter und Strategieregeln nicht eigenmächtig verändern.
8. Keine technisch realisierbaren Anforderungen auf spätere Revisionen verschieben.
9. Maximal zwei Agenten gleichzeitig einsetzen.
10. BSC und Project_0 strikt getrennt halten. Keine fachlich unbeteiligten Project_0-Dateien verändern.

### 2.2 Unveränderte Projektregeln

- **FREE_ONLY:** Keine kostenpflichtigen Provider oder Dienste ohne ausdrückliche Freigabe.
- **NO MOCK / NO STUB / NO FAKE in Produktion.** Test-Harnesses bleiben zulässig, sofern sie nicht Bestandteil des produktiven Ausführungspfades sind.
- **NO LEGACY:** Keine zusätzlichen veralteten oder parallelen Implementierungen.
- **NO JSON als internes Austauschformat:** Die vorgesehenen typisierten/binären Formate verwenden. Erforderliches externes JSON-RPC am Protokollrand bleibt zulässig.
- Kein unnötiges Hardcoding.
- Keine ungeprüften Protokollannahmen.
- Keine unqualifizierten Handelsprofile als funktionsfähig deklarieren.
- Direkte Integration auf `main`, keine dauerhaften zusätzlichen Branches.
- Git-Commits und Pushes nach abgeschlossenen Arbeitspaketen.
- Keine Echtgeld-Transaktion ohne gesonderte ausdrückliche Freigabe gemäß Phasenplan.

### 2.3 Arbeitsweise je Paket

**Implementierung vollständig → gezielte Tests → unabhängiges Review → bestätigte Befunde beheben → finales lokales Gate → Commit/Push → nächstes Paket.**

Bestehende Tests wiederverwenden. Mutationen gezielt für neue oder geänderte kritische Logik einsetzen. Keine vollständigen Mutationskampagnen nach jeder kleinen Änderung.

Ein Paket gilt nicht allein wegen bestandener Unit-Tests als abgeschlossen.

---

## 3. Verbindliche Änderung: Lokale macOS-Abnahme statt GitHub-CI

**GitHub Actions ist ab sofort keine Pflichtvoraussetzung für die technische Fertigstellung oder Abnahme von BSC.**

Der Bot wird auf dem lokalen Mac betrieben. GitHub dient ausschließlich der Versionsverwaltung.

### 3.1 Vollständiges lokales Testsystem

Alle benötigten Prüfungen sind lokal auf dem tatsächlichen Entwicklungs- und Betriebs-Mac auszuführen.

Dazu gehören, soweit fachlich relevant:

- Rust- und Go-Tests.
- Vollständige Native- und Release-Builds.
- `cargo fmt --check`.
- Clippy mit `-D warnings`.
- Kritische Mutationsprüfungen.
- Native-, Protobuf-, Gateway- und Frontend-Integration.
- Browser-E2E.
- Vollständige Prozess-E2E.
- Replay- und Journaltests.
- Crash, Restart, Recovery, Reorg, UNKNOWN und Idempotenz.
- Wallet-, Budget-, Nonce- und Positionsprüfungen.
- Reale read-only Marktdaten und qualifizierte Economics-Proben.
- Performance-, Last- und Dauerbetriebstests.
- Unabhängiges Review.

**Der lokale Testumfang muss mindestens dieselbe fachliche Tiefe besitzen wie das bisher vorgesehene Gesamtsystem-Gate.**

Tests dürfen nicht ausgelassen oder abgeschwächt werden, nur weil GitHub-CI entfällt.

### 3.2 Performance auf dem Mac

Die tatsächliche Betriebsplattform ist maßgeblich.

Insbesondere messen:

- Launch-Erkennung.
- Launch → Strategy-C-Entscheidung.
- Admission und Transaktionsaufbau.
- Entscheidung → Beginn des Sendens.
- BUY-/SELL-Latenzen.
- Queue-Wartezeiten und Drops.
- CPU-/RAM-Auslastung.
- Speicherallokationen.
- Verhalten unter paralleler Last.
- Stabilität bei längerem Betrieb.

Die bestehenden Latenzziele bleiben verbindlich. Lokale Hotpath-Zeiten, Transportzeiten, Builder-Bestätigungen und kanonische Chain-Ergebnisse müssen getrennt ausgewiesen werden.

Performance-Optimierungen benötigen einen Vorher-/Nachher-Vergleich. Eine Optimierung gilt nur als erfolgreich, wenn der erwartete Nutzen belegt und die bestehende Funktionalität erhalten ist.

### 3.3 GitHub

GitHub wird ausschließlich für Versionsverwaltung verwendet:

1. Finalen lokalen Stand vollständig prüfen.
2. Änderungen committen.
3. Auf `origin/main` pushen.
4. Lokalen und entfernten Commit-SHA vergleichen.
5. Sauberen Arbeitsbaum sicherstellen.

**Keine GitHub-Billing-Arbeiten, keine CI-Reparatur als Pflichtaufgabe und keine Verzögerung der Entwicklung wegen GitHub Actions.**

Die Regeln 9e/9f des PHASENPLANS sowie R14 ausdrücklich auf die vollständige lokale macOS-Abnahme umstellen. GitHub-CI ist optional und darf weder als PASS fingiert noch als zwingender Abschlussblocker geführt werden.

Alle übrigen fachlichen Qualifikations- und Nachweispflichten bleiben bestehen.

---

## 4. Strategy C Four – Stufe c

**Ziel:** Vollständige automatische Strategy-C-PAPER-Ausführung für die qualifizierten Four-Classic- und OpenFour-Profile.

### 4.1 A5 – Gemeinsame typisierte Ausführung

Die vorhandene C3-Venue-Verzweigung als Grundlage verwenden. Keine zweite venue-spezifische Strategy-C-Ausführungsarchitektur schaffen.

Verbindliche Zielstruktur:

- `StrategyCurve` als typisiertes Enum für Flap und Four.
- Gemeinsame Verarbeitung von `Intent + Nonce → ExpectedTx` für C3 und Strategy C.
- Keine zusätzlichen Einstiegspunkte wie `on_strategy_four_*` und `four_curve_of`, die dieselbe Funktion als eigenständigen Parallelpfad implementieren.
- Keine Veränderung der Strategy-C-Handelsentscheidungen.
- Keine zweite Journal- oder Positionsautorität.

Den verhaltensneutralen Enum-/Ausführungsschnitt als getrennte, überprüfbare Änderung vor der Four-c-Feature-Integration abschließen.

**Pflichtnachweise:**

- `strategy-replay` auf dem H-Lauf `live_strategy_1793049942243`.
- I-Lauf.
- G0.4-Kampagnenjournal.
- Jeweils `first_difference = None`, soweit die ursprünglichen Journale verfügbar und lesbar sind.
- Bestehende Four-Fixtures.
- `mut_sc4a.py`: 15/15 Mutanten erkannt.
- `mut_sc4b.py`: 25/25 Mutanten erkannt.

Zusätzlich einen kurzen tatsächlichen, ausschließlich lesenden Four-SHADOW-Lauf mit kostenloser WSS durchführen, ein Four-Journal erzeugen und erfolgreich replayen.

Eine strukturelle Änderung darf keine unbeabsichtigten Änderungen der bisherigen Trading-Entscheidungen oder Replay-Records erzeugen.

### 4.2 A6 – Zentrale Konfigurationsvalidierung

Eine gemeinsame native Strategy-C-Validierung implementieren, die für Runtime, Owner und Frontend-Steuerung maßgeblich ist.

Für die bestehenden Gewinn-/Stop-Multiplikatoren gilt bei `ratio_places`:

`0 ≤ stop < 1× < target`

Ungültige Konfigurationen müssen vor Aktivierung abgelehnt werden.

Die bisherigen Defaults und die bestehende Netto-Gewinnfaktorberechnung bleiben unverändert.

Gateway und Frontend verwenden dieselbe native Entscheidung und zeigen den Ablehnungsgrund an. Keine zweite fachliche Validierungsimplementierung erzeugen.

### 4.3 B2 – Four-Qualifikation auf bewegten Kurven

Die 0,01-BNB-Classic-Proben auf tatsächlich durch Dritthandel bewegten Kurven abschließen.

Die neuen Fixtures müssen innerhalb des nachweisbaren Protokollumfangs enthalten:

- Tatsächliche Storage-Zustände.
- Vergleich gegen `FourCurve::after` ausgehend von der belegten Launch-Kurve.
- `buyTokenAMAP` über `eth_call`.
- `sellToken` über `eth_call`.
- Exakte Übereinstimmung der Tokenmengen und Wei-Beträge.
- Brutto-/Nettoerlöse.
- Fremd-BUYs und Fremd-SELLs.
- Zustände nahe Ausverkauf.
- Zustände innerhalb und außerhalb des belegten Steuerfensters.
- Migration und ausdrücklich verweigerte nicht qualifizierte Zustände.

OpenFour entsprechend anhand nachgewiesener bewegter Kurvenzustände qualifizieren.

**Wichtig:** Keine nicht belegten Zustände künstlich als wirtschaftlich korrekt deklarieren.

Sind externe Proben für einzelne Profile nicht verfügbar, muss deren Admission ausdrücklich verweigert bleiben. Der entsprechende ausführbare technische Pfad ist dennoch vollständig zu implementieren und mit den verfügbaren Daten zu prüfen.

Fehlende externe Evidenz wird exakt benannt, nicht als erfolgreiche Qualifikation dargestellt.

Neue Fixtures in den Economics-Snapshot `bsc-economics-8` aufnehmen und diesen korrekt neu binden.

Eine durch den neuen Snapshot ungültig gewordene Qualifikation ist über den bestehenden Mechanismus `live qualify --replace` zu erneuern, sobald deren Voraussetzungen erfüllt sind.

### 4.4 B1/B3 – Venue-spezifische Gasreserven

Die Exit-Kosten für Flap, Four Classic und OpenFour korrekt und getrennt berechnen.

Pflichtumfang:

- Venue-spezifische APPROVE-/SELL-Gaslimits.
- Entsprechende Ergänzung der bestehenden `BSC_EXEC_FOUR_*`-/`BSC_EXEC_OPENFOUR_*`-Konfiguration, soweit tatsächlich erforderlich.
- Startvalidierung analog BSC3-079.
- Gaslimit mindestens das Klassenmaximum der eingefrorenen tatsächlich belegten Receipts.
- Korrekte Berechnung der SessionCaps.
- Korrekte Funding- und Budgetgrenzen.
- Korrekte Exit-Reserve auch bei mehreren parallelen Positionen.
- Berücksichtigung vorhandener Exit-Repricing-Mechanismen.

Die Kostenführung soll O(1) bleiben, soweit fachlich korrekt möglich.

Fehlen qualifizierte Gasnachweise, ist die betreffende Klasse `UNSUPPORTED`.

Betroffene Entries werden nachvollziehbar durch `Cancel::Cost` abgebrochen.

Den Zähler `cancelled_cost` ergänzen beziehungsweise korrekt anbinden.

Keine frei angenommenen Gaswerte oder zusätzlichen alternativen Kostenmodelle.

### 4.5 B4 – Brutto-/Netto-Abrechnung

Die bestehende Trennung zwischen `gross`, `funds` und `received` beibehalten.

Verifizieren:

- Strategy C bewertet den tatsächlichen Nettoerlös.
- Die Transaktionsbindung verwendet den vom jeweiligen Vertrag verlangten Mindestbetrag.
- Classic-SELL-Parität stimmt mit den vorhandenen ausgeführten SELL-Proben überein.
- Tokenmenge, Gebühren, Slippage und Gas werden korrekt berücksichtigt.

**Bereits korrekt implementierte Funktionen nicht erneut programmieren.**

### 4.6 Abschluss Stufe c

Für qualifizierte Four-Venues müssen die vorhandenen Strategy-C-Regeln im PAPER-Modus tatsächlich funktionieren:

- Automatischer BUY.
- Fill und Positionsübernahme.
- Gewinnziel.
- Stop.
- Timeout.
- Creator-Sell.
- Automatischer SELL.
- Korrekte Gebühren und Nettoergebnisse.
- Korrekte Budget- und Gasreserven.
- Nachvollziehbare Ablehnungen.
- Deterministisches Replay.

Nicht qualifizierte externe Profile bleiben mit korrektem Grund verweigert, ohne die Fertigstellung der belegbaren Funktionen zu verhindern.

Abschließend: Tests, Mutationen, Review, vollständiges lokales Paket-Gate, Commit und Push.

---

## 5. Strategy C Four – Stufe d: LIVE

**Ziel:** Die automatische Four-BUY-/SELL-Ausführung vollständig in den bestehenden LIVE-Owner integrieren.

Keine neue LIVE-Autorität und keine parallelen Freigabepfade.

### 5.1 LIVE-Qualifikation

Four LIVE ausschließlich über die vorhandene Qualifikation ermöglichen:

**Venue-Freeze → Economics-/Ausführungsparität → eigene Gas-Receipts → `live qualify` → vorhandene LIVE-Autorität.**

`FOUR_EXECUTION_PATH_MISSING` durch die tatsächlich vorhandene qualifizierte Ausführung ersetzen.

Bei fehlenden Voraussetzungen den exakten Blockgrund ausgeben.

Keine zusätzlichen Handelsmodi oder wirtschaftlichen Schalter erfinden.

### 5.2 A4 – Vollständige Positionsbuchführung

Eine Position darf durch `Input::Hold`, Restart oder Recovery nicht stillschweigend verschwinden oder ohne nachvollziehbaren Abschluss überschrieben werden.

Jeder relevante Zustandsübergang muss korrekt journalisiert und im Positionsstatus nachvollziehbar sein.

Die bestehende Replay- und Recovery-Semantik erhalten.

### 5.3 A1 – Unnötige Token-Kopien

Im Zusammenhang mit A4 die wiederholten vollständigen Token-Klone in `end_of_block` überprüfen und gezielt beseitigen.

Eine Zwei-Pass-Implementierung ist zulässig, wenn sie tatsächlich Kopieraufwand reduziert und deterministisches Verhalten erhält.

Vorher-/Nachher-Messungen und byte-identisches Replay sind Pflicht.

### 5.4 B5 – Abbruchgründe

Abbruch- und Ablehnungsgründe vollständig und konsistent im bestehenden Status bereitstellen.

Insbesondere dürfen Kosten-, Admission- und Qualifikationsfehler nicht nur als undifferenzierte Gesamtzahl erscheinen.

### 5.5 Abschluss Stufe d

Vollständigen technischen Ablauf testen:

**Launch → Entscheidung → BUY → bestätigte Position → Strategy-C-Exit → APPROVE/SELL → Buchführung/Recovery.**

Zusätzlich:

- Wallet- und Nonce-Integrität.
- Grants und wirtschaftliche Autorität.
- Budget- und Verlustgrenzen.
- Restart und Crash.
- Recovery und Reorg.
- Qualifikationswechsel.
- Abbrüche und UNKNOWN-Zustände.
- Vorhandene Frontend-/Gateway-Schnittstellen.

Keine echte Mainnet-Transaktion ohne die gesonderte Nutzerfreigabe des Phasenplans.

Vollständiges lokales Gate, unabhängiges Review, Commit und Push.

---

## 6. R13 – FOREIGN_AUTO

Die Reihenfolge des Phasenplans unverändert einhalten.

1. Den vorhandenen Zensus der Kampagne vom 27.10. bis 03.11. auswerten.
2. Tatsächlich beobachtete fremde Launch-Klassen identifizieren.
3. Für diese Klassen die relevanten On-Chain-Zustände und Eigenschaften belegen.
4. Qualifikation mittels kostenloser RPC-Ausführungsproben.
5. Nur nachgewiesene und qualifizierte Profile in die bestehende Handelslogik integrieren.

Keine angenommenen Decoder und keine unqualifizierten Profile-Fallbacks implementieren.

Ergibt der Zensus keine ausreichend belegte Klasse, ist die betroffene externe Qualifikation als `EXTERNAL_BLOCKED` zu dokumentieren, einschließlich des konkreten Zensusnachweises.

Bereits technisch umsetzbare Funktionen dennoch vollständig programmieren.

Lokales Gate, Review, Commit und Push.

---

## 7. Vollständige Frontend-Integration

Die vorgesehenen Strategy-C-Einstellungen und relevanten LIVE-Funktionen über den bestehenden Steuerungsweg vollständig verbinden:

**Native Owner → Protobuf → Gateway → Store → BSC-LIVE-Reiter → CAS/Receipt → Native Owner**

Pflichtumfang:

- Alle gemäß Phasenplan vorgesehenen Strategy-C-Konfigurationen.
- Bidirektionale Änderung und Rückmeldung.
- Requested-/Effective-Zustände.
- Präzise Ablehnungsgründe.
- Venue- und Source-Zustände.
- Wallets und wirtschaftliche Freigaben.
- BUY-/SELL- und Positionsstatus.
- Kosten, Budget und bestehende Risikogrenzen.
- Persistenz nach Restart.
- Native-/Gateway-/Protobuf-/Frontend-Konsistenz.

Keine zweite Handels- oder Validierungslogik im Frontend.

Alle relevanten Native-, Gateway-, Protobuf- und Browser-E2E lokal testen.

Vollständiges lokales Gate, Review, Commit und Push.

---

## 8. R14 – Performanceabschluss und Gesamtabnahme

R14 ist die letzte technische Abschlussphase.

### 8.1 A2 – Lineare Token-Suchen

Die Token-Suchen bei häufigen Trade-Ereignissen unter realistischer Last messen.

Wenn das lineare Durchsuchen der Tokenliste relevant zur Latenz beiträgt, einen geeigneten direkten Token-Index einführen.

Die bisherige deterministische Reihenfolge beibehalten.

Vorher-/Nachher-Messung und Replay-Parität nachweisen.

### 8.2 A3 – Redundante Speicheroperationen

Unnötige Kopien und Allokationen im Journal-/Runtime-Pfad identifizieren und gezielt beseitigen.

Nur Änderungen umsetzen, die einen nachgewiesenen Nutzen besitzen und keine neue Komplexität ohne Gegenwert erzeugen.

### 8.3 Abschluss verbleibender A1-Arbeiten

Falls nach Stufe d noch relevante unnötige Token-Kopien vorhanden sind, diese gezielt prüfen und optimieren.

### 8.4 Vollständiges lokales Gesamtsystem-Gate

Auf dem finalen Code:

- Rust-/Go-Tests und Release-Builds.
- Formatierung und Clippy.
- Kritische Mutationsprüfungen.
- Native-/Gateway-/Protobuf-/Frontend-E2E.
- Browser-E2E.
- Prozess-E2E.
- Strategy-C-Replay.
- Economics-Parität.
- BUY-/SELL-Budget- und Kostenintegrität.
- Crash/Restart/Recovery/Reorg.
- Nonce- und Idempotenzprüfung.
- Konfigurations- und Qualifikationsprüfung.
- Performance- und Lasttests.
- Stabilitäts- und Dauerbetriebsprüfung.
- Unabhängiges Gesamtreview.

**Keine Pflichtprüfung darf ungeprüft als bestanden gelten.**

### 8.5 Rebaseline

- Economics-Snapshot aktualisieren.
- Konfigurations- und Release-Identitäten prüfen.
- Replay- und Performance-Belege zusammenführen.
- Historische und widersprüchliche STATUS-Angaben bereinigen.
- Aktuellen Betriebsstand eindeutig dokumentieren.
- Vorhandene Messplan- und STATUS-Zeilen finalisieren.

### 8.6 Letzte Gesamtabnahme

Die letzte umfassende Testkampagne erfolgt auf dem tatsächlich finalen Code.

Alle bestätigten abnahmeverhindernden Fehler müssen zuvor behoben sein.

Anschließend:

**Finales lokales Gate PASS → unabhängiges Review PASS → Commit/Push → SHA-Abgleich.**

GitHub Actions gehört ausdrücklich nicht zu diesem Pflicht-Gate.

Nach der Gesamtabnahme keine weiteren strukturellen Umbauten, außer zur Behebung nachgewiesener Fehler mit erneuter betroffener Prüfung.

---

## 9. Verbindliche Abarbeitung sämtlicher Reviewer-Befunde A1–A6 und B1–B5

**Dieser Abschnitt ist eine eigenständige Pflicht innerhalb des Gesamtauftrags. Keiner der elf bereits identifizierten Befunde darf stillschweigend entfallen.**

### 9.1 Vollständiges Befundregister

| ID | Gegenstand | Vorgesehene Bearbeitung |
|---|---|---|
| A1 | Unnötige Token-Kopien pro Block | Stufe d / R14 |
| A2 | Lineare Token-Suchen im Trade-Pfad | R14 |
| A3 | Redundante Journal-/Runtime-Allokationen | R14 |
| A4 | Stilles Überschreiben offener Positionen | Stufe d |
| A5 | Doppelte Four-/Flap-Ausführungsstruktur | Stufe c |
| A6 | Fehlende Ziel-/Stop-Konfigurationsvalidierung | Stufe c |
| B1 | Venue-blinde Exit-Gasreserve | Stufe c |
| B2 | Unzureichender Nachweis bewegter Four-Kurven | Stufe c |
| B3 | Unvollständige Four-Exit-Gasklassen | Stufe c |
| B4 | Prüfung Brutto-/Nettoerlös und SELL-Bindung | Stufe c |
| B5 | Fehlende Differenzierung der Abbruchgründe | Stufe c / d |

### 9.2 Für jeden Befund verpflichtend

**1. Quellcodeprüfung** — Den Befund gegen den aktuellen tatsächlichen Quellcode prüfen. Datei, Zeile und geprüften Commit-SHA festhalten.

**2. Klassifikation** — Jeden Befund eindeutig als `BESTÄTIGT`, `WIDERLEGT` oder `OFFEN` einstufen. Bereits vorhandene korrekte Lösungen nicht erneut programmieren.

**3. Ursachenbehebung** — Bestätigte Fehler vollständig an ihrer Ursache beheben. Keine zusätzlichen Workarounds schaffen, wenn eine saubere Korrektur innerhalb der vorhandenen Architektur möglich ist.

**4. Technischer Nachweis** — Für jede relevante Korrektur: gezielte positive Tests, geeignete Negativtests, kritische Mutationstests, bei Zustandsänderungen Replay-Parität, bei Performanceänderungen Vorher-/Nachher-Messungen, bei wirtschaftlichen Änderungen Kosten-, Wallet- und Budgetprüfungen.

**5. Regression** — Nachweisen, dass bestehende Funktionen nicht beschädigt werden: BUY, SELL, Wallets und Nonces, Budget- und Risikogrenzen, Recovery, Venue-Qualifikation, Gateway/Frontend, Journal und Replay.

**6. Unabhängiges Review** — Die Korrektur unabhängig prüfen lassen. Bestätigte HIGH-/CRITICAL- und sonstige abnahmeverhindernde Befunde vor Abschluss beheben.

**7. Dokumentierter Abschluss** — Für jeden Befund festhalten: Ergebnis, geänderte Dateien und Zeilen, Testbefehle und Exitcodes, Mutationsergebnisse (soweit erforderlich), Replay-/Performance-Nachweise (soweit betroffen), Reviewer-Ergebnis, finaler Commit-SHA. Die Nachweise in den bereits bestehenden STATUS-/Messplan-Unterlagen führen.

### 9.3 Verbindliche Abschlussbedingung

**Alle elf Befunde müssen ein dokumentiertes Prüfergebnis besitzen.**

- Bestätigte Fehler: behoben und erfolgreich getestet.
- Widerlegte Befunde: durch konkreten Gegenbeleg widerlegt.
- Bereits behobene Befunde: bestehende Implementierung und Tests nachgewiesen.
- Nicht technisch abgeschlossene Befunde: ausdrücklich `OFFEN`.
- Tatsächlich extern nicht qualifizierbare Punkte: `EXTERNAL_BLOCKED` mit Beleg.

Ein Fehler darf weder durch Umbenennung noch durch Verschiebung aus dem Abschlussumfang verschwinden.

**Die Bearbeitung erfolgt innerhalb von Four c, Four d und R14. Es wird keine zusätzliche Auditphase eröffnet.**

---

## 10. Verbindliche Abschlussdefinition: TECHNISCH FERTIG

Ein Paket darf nur als **TECHNISCH FERTIG** bezeichnet werden, wenn:

1. Sämtliche technisch realisierbaren bestätigten Anforderungen vollständig im produktiven Code umgesetzt sind.
2. Alle relevanten echten Caller, Runtime-Pfade und fachlichen Schnittstellen integriert sind.
3. Das vollständige betroffene lokale macOS-Gate erfolgreich bestanden wurde.
4. Kritische Logik ausreichend durch positive, negative, Mutations-, Integrations- und Recovery-Tests geprüft ist.
5. Die vorgesehenen Replay- und Performance-Nachweise vorliegen.
6. Das unabhängige Review keine offenen abnahmeverhindernden Befunde enthält.
7. Sämtliche bekannten zugehörigen Reviewer-Befunde abgeschlossen oder korrekt als offen beziehungsweise extern blockiert dokumentiert sind.
8. Evidence, Testbefehle, Exitcodes, Status und finaler Commit-SHA dokumentiert sind.
9. Der abgenommene Stand auf `origin/main` liegt und mit dem geprüften SHA übereinstimmt.

Fehlen ausschließlich externe, derzeit nicht verfügbare Qualifikationsnachweise, darf die implementierte Funktion als **TECHNISCH FERTIG – EXTERNAL_BLOCKED** ausgewiesen werden, mit präziser Beschreibung des nicht qualifizierten Funktionsumfangs.

Fehlen Implementierung, Integration oder notwendige lokale Tests, bleibt das Paket **OFFEN**.

**Ein technischer Abschluss ist keine wirtschaftliche LIVE-Freigabe.** Die bestehenden G0.7-/G0.8-, Mainnet- und Betreiberfreigaben bleiben unverändert.

GitHub-CI ist ausdrücklich kein Abnahmekriterium mehr.

---

## 11. Verbindliche Reihenfolge und Fertigstellungsauftrag

Die verbleibende Entwicklung erfolgt ausschließlich in dieser Reihenfolge:

1. **Strategy C Four c:** PAPER-Ausführung, Architekturvereinheitlichung, Kurven-/Kostenqualifikation und zugehörige Reviewer-Befunde.
2. **Strategy C Four d:** Vollständiger LIVE-Ausführungspfad, Positionsbuchführung, Recovery, Qualifikation und zugehörige Reviewer-Befunde.
3. **R13:** FOREIGN_AUTO gemäß belegtem Zensus und qualifizierten Profilen.
4. **Frontend:** Vollständige bidirektionale Strategy-C-Integration und End-to-End-Funktion.
5. **R14:** Verbleibende Performanceoptimierungen, Rebaseline, vollständiges lokales Gesamtsystem-Gate und abschließende unabhängige Gesamtabnahme.

Innerhalb der Pakete dürfen notwendige abhängige Korrekturen gemeinsam umgesetzt werden. Die Reihenfolge darf nicht durch neue, nicht erforderliche Audit- oder Architekturprogramme verlängert werden.

### Abschließende Anweisung

**Den gesamten hier beschriebenen Funktionsumfang vollständig umsetzen.**

Nicht nur Tests und Sicherheitsmechanismen erweitern, sondern die tatsächliche automatisierte Handelsfunktion bis zum technisch vollständigen BUY-/SELL-Betrieb fertigstellen.

Bereits korrekt funktionierende Komponenten beibehalten. Bestätigte Fehler beheben. Messbar relevante Performanceprobleme optimieren. Alle elf Reviewer-Befunde vollständig nachweisbar abarbeiten.

Qualitätssicherung so umfangreich wie notwendig, aber ohne unnötige Wiederholung oder zusätzliche Entwicklungsphasen.

**Ziel ist ein fertig entwickelter, latenzoptimierter, stabiler und wirtschaftlich korrekt arbeitender BSC-Bot auf meinem lokalen Mac. GitHub ist ausschließlich Versionsverwaltung.**

Die geltenden Phasenplanregeln zur lokalen Abnahme entsprechend aktualisieren, diesen Gesamtauftrag in der bestehenden Projektdokumentation verankern und die Entwicklung ohne weitere allgemeine Planungsrunde mit **Strategy C Four c** fortsetzen.
