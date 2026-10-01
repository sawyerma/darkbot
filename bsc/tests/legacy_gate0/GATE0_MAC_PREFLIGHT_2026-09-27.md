**Gate-0-Vorbereitung auf dem Mac – 27.09.2026**

Geprüfter Projektstand: `0ea81c48ab0a8dc08b947dd7ec20489904ead236`.
Projekt: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`.
Arbeitsverzeichnis sauber. Änderungen gegenüber `4c5ff58` betreffen ausschließlich fünf Planungsdokumente. Dieses Protokoll ist ein Prüf-/Arbeitsstand, keine zusätzliche Architekturvorgabe.

**Status: Vorbereitung möglich; G0.1 noch nicht abgeschlossen; kein Gate-0-PASS.**

Der Nutzer hat Vorbereitung auf diesem Mac gewählt. Verifiziert: macOS 15.6.1, ARM64; `rustc 1.93.1`, `cargo 1.93.1`. `geth` wurde im PATH nicht gefunden. Docker-CLI vorhanden; ein laufender BSC-Node oder verfügbarer RPC wurde nicht nachgewiesen. `gate0/`, `native/` und Cargo-Manifeste existieren im Projekt noch nicht. Ein ausführbarer Gate-0-Test liegt folglich noch nicht vor.

Die bisherigen Regressionstests wurden nicht erneut ausgeführt: Produktionscode, bestehende Tests, Konfigurationen und Migrationen haben sich seit der letzten Prüfung nicht geändert. Die früheren 287 bestandenen Tests sind kein Gate-0-Nachweis.

**Was jetzt korrekt eingearbeitet ist**

- Erfolg verlangt beabsichtigten Token-Erwerb beim richtigen Empfänger samt Kostenbeleg.
- Launch-interne Käufe sind getrennt vom frühesten externen Kauf.
- Pflichtfelder erhalten Quellen, Vorberechenbarkeit und Invalidierungsregeln.
- Semantische Gültigkeit steht vor Performance-Optimierung.
- Coverage beginnt früh; vollständige Ingress-/Inclusion-Kombinationen werden verglichen.
- Numerische Abnahme und vollständige Ergebnisstatistik sind vorgeschrieben.
- Parallel-Launch-/Nonce-Fälle und die erneute Messung des fertigen Produktionspfads sind vorgeschrieben.

**Verbleibende Korrekturen im Beschreibungstext**

1. [G0.6, Zeile 276](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:276) verlangt echte erfolgreiche Kaufergebnisse, obwohl werttragende Tests erst G0.7 erlaubt sind und jede Phase separat abgeschlossen werden soll. Präzisierung: G0.6 qualifiziert Adapter/Matrix und Konkurrenzfälle trocken; die echte Ausführung dieser Vergleichsmatrix erfolgt in G0.7, ihre Auswertung in G0.8. Vorher kein behaupteter Real-Race-Sieger.

2. [Phase I, Zeile 308](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_DUAL_VENUE_ROADMAP.md:308) unterdrückt die externe Submission, verlangt danach aber eine Messung bis `T_send`. Shadow-Ausgang und echte Transportübergabe brauchen getrennte Kennzeichnungen. PAPER kann bis zum Test-Sink messen; der reale Adapter-/Transportanteil benötigt eine separate geeignete Transportmessung und kontrollierte Produktions-Revalidierung zu Beginn von J, bevor breites LIVE freigegeben wird. Eine simulierte Übergabe ist kein gemessener Mainnet-Sendeweg.

3. [Phase I, Zeile 326](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/BSC_DUAL_VENUE_ROADMAP.md:326) verweist auf das Timing von G0.3. Nach der Umnummerierung steht die lokale Latenzmessung in **G0.5**. Verweis korrigieren.

4. [Timingdefinitionen, Zeile 41](/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_TESTPLAN_SHORT.md:41): `T_pre` ist ausschließlich als Signal vor Handelbarkeit definiert. Für verspätete oder zeitlich nicht einordenbare Signale darf der Startzeitpunkt nicht fehlen. Einen immer messbaren lokalen `T_signal` vor der gemessenen Decode-Arbeit definieren; nachgewiesene Pre-T0-Eigenschaft getrennt speichern. Außerdem pro Adapter eindeutig festlegen, ob `T_send` eine interne Queue, IPC an geth oder die offene Transportverbindung bezeichnet. Queue und echte Transportübergabe getrennt ausweisen; keine verdeckte Verschiebung der Messgrenze.

Diese Präzisierungen erfordern keinen weiteren Architekturwechsel und hindern den Beginn der Quellenprüfung G0.1 nicht.

**Begonnene G0.1-Quellenprüfung – Dokumentationsbelege, noch kein On-chain-Freeze**

| Objekt | Öffentlich dokumentierter Kandidat | Belegstatus |
| --- | --- | --- |
| Flap BNB Portal | `0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0`, v5.23.1 | Aktuelle offizielle Deployment-Seite erneut gelesen; Chaincode/Implementation noch nicht unabhängig geprüft |
| Four TokenManager2 BSC | `0x5c952063c7fc8610FFDB798152D69F0B9550762b` | Offizielle Create-/Trade-Anleitung erneut gelesen; Chaincode/Implementation noch nicht unabhängig geprüft |
| Four Erstellung | `createToken(bytes,bytes)`; Creator-Prebuy kann Teil der Erstellung sein | Dokumentationsbefund; reale ausgewählte Transaktion und State-Transition noch einzufrieren |
| Four BUY | `buyToken` / `buyTokenAMAP` mit dokumentierten Überladungen | Konkreter Selector/Modus noch nicht ausgewählt oder qualifiziert |

Quellen: [Flap Deployment-Tabelle](https://docs.flap.sh/flap/developers/deployed-contract-addresses), [Four Create Guide](https://raw.githubusercontent.com/four-meme-community/fourmeme-docs/main/docs/create-guide.md), [Four Trade Guide](https://raw.githubusercontent.com/four-meme-community/fourmeme-docs/main/docs/trade-guide.md). Diese URLs zeigen bewegliche Dokumentstände; sie ersetzen keine unveränderlichen Quelldateien/Hashes und keinen On-chain-Nachweis.

**Nächste auszufüllende Nachweise – keine Werte erfinden**

| Nachweis | Aktueller Stand |
| --- | --- |
| Lesender BSC-RPC / Chain-ID-Prüfung | OFFEN |
| Referenzblock: Nummer und Hash | OFFEN |
| Runtime-Code, Proxy-/Implementierungsauflösung soweit zutreffend, Codehash | OFFEN |
| Gepinnter ABI-/Quellstand samt Hash und Herkunft | OFFEN |
| Reale Launch-Fixture plus Receipt/Logs/benötigter Vorzustand | OFFEN |
| Exakter primärer Venue-/Deployment-/Launch-/BUY-Modus | NOCH NICHT AUSGEWÄHLT |
| Vollständige G0.2-Pflichtfeldtabelle | OFFEN |
| Numerische Abnahme, Handelsgrößen-/Kostenbereich | VOR DEN ENTSPRECHENDEN VERSUCHEN FESTZULEGEN |

**Gemeinsamer Testablauf**

Auf diesem Mac: G0.1-Evidence zusammenstellen, G0.2-Inputs ableiten und gegen konkrete Launch-Fixtures prüfen; dann den kleinsten nativen G0.3-Test für die exakte Reihenfolge `Launch -> BUY` herstellen. Eine lokale Fork-/EVM-Prüfung benötigt den passenden BSC-Zustand und kompatible Ausführungsregeln. Sie beweist Gültigkeit, aber noch keinen gewonnenen Mainnet-Kaufplatz. Coverage kann mit einem geeigneten Eingang vom Mac aus untersucht werden; der Standort bleibt Teil des Ergebnisses.

Lokale Mac-Messungen dienen Entwicklung und Fehleranalyse. Die endgültige Produktions-Latenzqualifikation verlangt laut aktuellem Vertrag dedizierte Linux-Hardware und den vorgesehenen Ingress-/Inclusion-Pfad.

Kontrollierte Mainnet-Käufe folgen erst nach trockener Qualifikation mit ausdrücklich festgelegtem Testwallet, Kauf-/Gebühren-/Gesamtbudget und Versuchslimit. In dieser Vorbereitung wurden keine Schlüssel gelesen, Transaktionen signiert oder Käufe ausgeführt.
