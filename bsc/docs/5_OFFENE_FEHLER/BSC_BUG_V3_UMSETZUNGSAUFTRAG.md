# BSC – VERBINDLICHER UMSETZUNGSAUFTRAG ZUM FEHLERREGISTER V3

**Stand:** 28.10.2026
**Geltungsbereich:** ausschließlich BSC in sawyerma/darkbot; Project_0 und DarkNode nicht verändern.
**Prüfregister:** [BSC_Bug_V3.md](BSC_Bug_V3.md), Revision 14 mit BSC3-001 bis BSC3-070.
**Übergeordnete Autorität:** [PHASENPLAN.md](../1_PLAN/PHASENPLAN.md), besonders die verbindliche Entscheidung vom 28.10.2026; ergänzend [BSC_UMSETZUNGSAUFTRAG_V2.md](BSC_UMSETZUNGSAUFTRAG_V2.md), [STATUS.md](../1_PLAN/STATUS.md) und die bestehenden nativen Architektur-/Wire-/Evidence-Verträge.

## 0. Dein Auftrag

Claude, **behebe die im Register bestätigten BSC-Produktionsfehler Punkt für Punkt im tatsächlich verwendeten Rust-Core**, einschließlich zwingender Integrationskorrekturen, Tests und Nachweisen. Arbeite anschließend die noch ungeprüften Module ab und ergänze nur quellcodebelegte neue Befunde. **Kein erneuter allgemeiner Konzeptentwurf und keine endlose Dokumentationsphase als Ersatz für Code.** Eine sauber abgeschlossene Arbeitseinheit besteht aus reproduziertem Fehler, produktivem Fix, gezielten Regressionstests, Freigabe-/Sicherheitsprüfung und einem überprüfbaren Git-Commit, der auf origin/main gepusht wurde.

Die 70 IDs sind ein **Auditregister, nicht 70 automatisch bestätigte Bugs**. Frühe Aussagen werden durch spätere, ausdrücklich belegte Nachprüfungen übersteuert. Die letzte konsolidierte Präzisierung im Register (N11) gilt vor einer widersprüchlichen historischen Formulierung. Vor jedem Eingriff: tatsächliche Datei am aktuellen Commit lesen, Auswirkung am realen Call-Path prüfen und Status je ID bestimmen.

## 0a. Verbindliche Ausführungspräzisierung (28.10.2026)

- **Start:** BSC3-041/055/024 am aktuellen LIVE-Rust-Pfad prüfen und bestätigte Fehler als erstes P0-Paket reproduzieren, korrigieren, testen, reviewen, committen und auf origin/main pushen. Dann die nächsten P0-IDs. Kein weiterer Audit als Ersatz für produktive Fixes.
- **CI:** Die vorhandene `.github/workflows/bsc.yml` nutzt bereits macOS/Linux, `cargo test --locked --no-fail-fast --workspace --all-features` und prüft sieben `bsc_tools`-Skripte mit `SKIP 0`. BSC3-005/054 gezielt auf fehlende Release-/Werkzeug-Abdeckung prüfen und ergänzen; CI nicht neu erfinden. Test-PASS immer am finalen Commit nachweisen.
- **Ignored:** `--include-ignored` niemals pauschal anwenden. Jeder Test wird zuerst auf externe Voraussetzungen und mögliche wirtschaftliche Wirkung geprüft. Nur sichere lokale Tests ausführen; blockierte Tests ehrlich als SKIP/EXTERNAL_BLOCKED ausweisen.
- **G0.4:** BSC3-032/033/064 vor verbindlicher Kriterienauswertung behandeln. Laufende Kampagne und Rohdaten ohne Betreiberfreigabe nicht verändern, stoppen oder neu starten. Unbelegte Abdeckung bleibt UNKNOWN/NOT_EVALUABLE.
- **BSC3-001:** Menschliche Bestätigung mit realem macOS- und Linux-Sicherheitskonzept; Touch ID ist ein Vorschlag, keine bereits getroffene Entscheidung.
- **BSC3-058:** Vollständigen Not-Aus erhalten; EXIT-ONLY nur separat autorisiert. Nach Ablauf oder Neustart keine automatische wirtschaftliche Transaktion.
- **BSC3-059:** Keine erfundene 10-Minuten-Regel für LIVE-Grants; nötige Maximaldauer vom Betreiber entscheiden lassen.
- **BSC3-068:** Transportfrist vor Owner-Zulassung vorprüfen und unmittelbar vor dem tatsächlichen Write erneut prüfen. Persistente Attempts nur mit crash-/race-sicherem Nicht-Egress-Beweis rückgeben, sonst konservativ zählen.
- **Abnahme:** Je ID Rot/Grün-Regression am Produktionspfad, Integration/Release soweit nichtsendend, unabhängiges Review, Commit, verifizierter origin/main-HEAD und knapper Bericht. Keine Mocks als Produktionsersatz, kein Fake-PASS, keine Legacy-Umgehungen, kein willkürliches Hardcoding, kein zweiter Bot-Core.

## 1. Unverhandelbare Qualitätsregeln

1. **Keine Fake-Implementierung:** Kein Platzhalter, TODO, leeres Ok/PASS, stilles Ignorieren, Erfolgs-Flag, fingierter Receipt, simuliertes Guthaben, erfundener Preis, erdichtete On-Chain-Inclusion oder angebliche Produktivverbindung. UNKNOWN bleibt UNKNOWN, UNQUALIFIED bleibt UNQUALIFIED und fehlende Freigaben bleiben BLOCKED.
2. **Keine Mocks als Problemlösung:** Ein Mock, Stub, Loopback oder Fake-Provider darf niemals einen echten Produktionsadapter ersetzen oder einen MAINNET-/LIVE-/BUY-/SELL-PASS begründen. Kontrollierte synthetische Harnesses sind ausschließlich als ausdrücklich so bezeichnete **Unit-, Crash-, Race-, Negativ- oder Nicht-Egress-Integrationstests** zulässig und müssen den unveränderten echten Produktionspfad prüfen. Produktionsverhalten und Testverhalten dürfen nicht heimlich auseinanderlaufen.
3. **Kein neuer Legacy-Code und keine Legacy-Umgehung:** Keine zusätzliche alte API/Engine, kein Altpfad als Ersatz, kein dauerhafter Kompatibilitäts-Shim zur Umgehung eines Fehlers. Bestehende tatsächlich verwendete historische Decoder/Deployments **nicht blind löschen**: erst Daten-/Evidence-Inventur, verlustfreie Migration, Kompatibilitäts- und Reproduktionstests; sonst beibehalten und dokumentieren.
4. **Kein willkürliches Hardcoding:** Keine hineinkopierten RPC-URLs, Schlüssel, Wallet-Adressen, Beträge, Fee-, Gas-, Slippage-, Nonce-, Zeit-, Portal- oder Qualifikationswerte, die einen Test oder LIVE nur scheinbar bestehen lassen. Protokollkonstanten und bewusst eingefrorene Chain-Deployment-Fingerprints sind nur mit versionierter G0.1-/Evidence-Bindung zulässig. Betriebswerte über den bestehenden typisierten Native-Owner/Policy-/Grant-Pfad, fail-closed validiert.
5. **Keine parallele Architektur:** Keine zweite Bot-Engine, kein zweiter Owner, keine konkurrierende Nonce-/Ledger-/BUY-/SELL-Wahrheit und kein zweiter ungebundener Produktions-Sendepfad. Vorhandene verifizierte Bausteine weiterverwenden und ihre Schnittstellen tatsächlich verbinden. Ein größerer Refactor nur bei reproduzierbar unmöglichem Minimalfix, nach Folgenanalyse und Tests.
6. **Keine Risikoverlagerung:** Kein Löschen von Belegen oder Sicherungen, kein Abschwächen von Guards, keine größeren Budgets/Slippage/Gas-Grenzen, keine pauschalen Retries, kein Verschlucken von Fehlern, keine Schein-Quarantäne. Fail-closed auch bei Stromausfall, Prozessabbruch, Dateikorruption, Teil-Writes, Provider-Fehler, Reorg und UNKNOWN.
7. **Keine unzulässige Kosten- oder Echtgeldaktion:** Dieser Auftrag erlaubt Codeentwicklung, READ_ONLY, PAPER, SHADOW, lokale und nichtsendende Tests; **nicht** echte Wallet-Freigabe, SIGN, ARM, Grant, Mainnet-SEND, BUY, SELL oder kostenpflichtige Registrierung/Dienste. G0.7/G0.8 und alle Betreiberentscheidungen bleiben separat ausdrücklich freizugeben. Laufende Beobachtung nicht ohne Auftrag stoppen.
8. **Keine kosmetischen PASS-Berichte:** Kein Erfolg ohne tatsächlich ausgeführten Test samt Befehl, Exitcode, Build/Commit-SHA und Evidence. Ignorierte Tests, fehlende Credentials, unbezeugte Provider-Semantik und ökonomische Unsicherheit offen ausweisen; nie in PASS umbenennen.

## 2. Strikte Abarbeitung je Befund-ID

Führe ein fortlaufendes Register je **BSC3-001 bis BSC3-070** mit Original-ID, Schwere, letzter gültiger Nachprüfung, Quellcode-Beleg, Ergebnis, zugehörigem Test, Fix-Commit und Restblocker. Die Reihenfolge folgt **Abhängigkeit und Schaden**, nicht blind der numerischen ID: erst P0/LIVE-Sicherheitskette, danach SEND/EXIT/Qualifikation, Hot-Path/Latenz, Mess- und Tooling-Fehler, dann Low-Härtung. Innerhalb jeder Kategorie ausdrücklich ID für ID.

Für **jede** ID verbindlich:

1. Aktuellen origin/main-HEAD und den konkreten produktiven Call-Path ermitteln; gegen PHASENPLAN, die späteste Bewertung in BSC_Bug_V3 und vorhandene Tests abgleichen. Historische Dateizeilen/Commits nicht als ungeprüften aktuellen Stand behandeln.
2. Status festhalten: BESTÄTIGT, TEILBESTÄTIGT, WIDERLEGT, KEIN_PFLICHTFIX, EXTERNAL_BLOCKED, FIX_IN_ARBEIT, FIX_VERIFIED. Bei relevanter Unsicherheit erst gezielten nichtwirtschaftlichen Reproduktionstest erzeugen; kein Fix ohne belegte Ursache.
3. **Vorher rot, nachher grün:** Einen gezielten Regressionstest bauen, der vor dem Fix das Fehlverhalten nachweist oder die fehlende Sicherheitsinvariante prüft; danach echten Produktionscode minimal und vollständig korrigieren. Wo ein echter Vorher-Test technisch unmöglich ist, das konkret belegen und eine gleichwertige deterministische Sicherheitsprüfung liefern.
4. Betroffene Caller, Schalter, Scope-/Nonce-/Budget-/Ledger-/Provider-/Position-/Restart-Semantik und Protobuf/Gateway/UI-Verträge mitprüfen. Tests für Erfolgs-, Ablehnungs-, Crash-, Race-, Reorg- und UNKNOWN-Pfade, soweit kausal betroffen.
5. Passende Release-Build-, Unit-, Integration-, Doc-Test-, Lint- und Replay-/CI-Prüfungen **wirklich** ausführen; Performance-Behauptungen nur mit tatsächlich gemessenen T_ingress/T_ready/T_queue/T_send, p50/p99/max und Verlust-/Überlastzählern. Nicht gemessene Zahlen nicht schätzen oder als Gewinn melden.
6. Gezielt unabhängiges Review der geänderten Sicherheitsinvarianten; Regressionen/Findings beheben. Diff eng halten; keine Änderungen an Project_0/DarkNode, Secrets, privaten Walletdateien oder unnötigen Artefakten.
7. **Ein sauberer Commit pro logisch abgeschlossenem Fix-Paket**, dann sicherer Fast-Forward-Push nach origin/main, tatsächlich remote HEAD prüfen. Keine nur lokale Fertigmeldung, kein Force-Push, kein reset --hard, kein Überschreiben fremder oder unversionierter Arbeit.
8. Den Fortschritt in [STATUS.md](../1_PLAN/STATUS.md) und im Register mit exaktem Commit, Test-Evidence, Ergebnis und verbleibendem Rest aktualisieren. Unbewiesene/reine externe Punkte als EXTERNAL_BLOCKED dokumentieren. Erst dann nächstes Paket.

Zusammenhängende IDs dürfen **in einem atomaren Code-Paket** behoben werden, aber jede ID braucht eine eigene nachprüfbare Abnahmezeile. Ein bloßes Dokumentations-Commit schließt keine Produktions-ID.

## 3. Verbindliche technische Reihenfolge

**P0 — Finanzielles Verhalten vor irgendeinem LIVE-Einsatz:** Menschlich autorisierte Scope-/Wallet-Freigaben und Schlüsselgrenze (001); Crash-/Restart-/Nonce-/Fence-Quarantäne und senderweite Invarianten (041/055, 024); korruptionsfestes Ledger und Positionen (045/047); BUY→ApproVE→SELL sowie abgelaufene, revertierte, unbekannte und reorganisierte Transaktionen (048/030); Ausstiegsgas und Grant-Reserve (027/057); klare Trennung **vollständiger Not-Aus** gegen gesondert bestätigten **BUY-STOP/EXIT-ONLY**-Modus (058); qualifizierte Deployment-/Provider-/Evidence-/Canonical-Quellen (049/056/065/036); Builder-Antwortverarbeitung (051), sichere Versuchszählung ohne vorschnellen Refund (068). Keine automatische Re-ARM- oder SELL-Retry-Abkürzung.

**P1 — Tatsächliche Produktionsintegration:** Owner-Gates und ihre nativen Caller; ein Sender-/Nonce-/Ledger-Wahrheitspfad, echte Receipt-/Positionserfassung und tatsächliche BUY-/SELL-Adapter; vollständige UI/CLI/Native→Protobuf→Gateway→Frontend-Parität. Zusätzlich G0.7-Sonderrunner unverändert als separat streng autorisierten Einzelversuch behandeln; Unterschiede der G0.7- und LIVE-Builder-Wire-Verträge (066) durch exakte Contract-/Golden-Parity-Tests prüfen, nicht ungeprüft die Freigabeautoritäten zusammenlegen.

**P2 — Latenz, Abdeckung, Betrieb:** Owner-fsync/Deadline und Worker-Isolation (009/023/025/068), Launch-Priorisierung (061/063), C2/Checkpoint/Ingress unter Last, G0.4-Sichtungseviction und echte Abdeckung (062/064), Slippage-/Gaspreis- und getrennte EXIT-Grenzen (069), isolierte Git-Trust-Härtung für G0.7 (070) sowie übrige bestätigte IDs. Keinen Hot-Path-Umbau ohne reale Vorher-/Nachher-Messung und Sicherheitsparität.

**P3 — Verbliebene Befunde und Module:** Alle anderen IDs systematisch durchgehen; noch nicht hinreichend gelesene Teile (startup.rs, venue_probe.rs, native_curve20/Admission, Four-Venue, trading::strategy, Replay, Journal, Capture, Reporter/Projection, Gateway/Frontend, bsc/tools, CI) gezielt anhand ihrer Schnittstellen prüfen. Neue Befunde erhalten nur die nächste freie ID ab BSC3-071, mit Reproduktion und Quellbeleg. **Kein neuer generischer Audit-Marathon vor Beginn der Codekorrekturen.**

## 4. Korrekturen früherer Fehlannahmen sind bindend

- **BSC3-042:** Eine unbenutzte Lane hält bei UnknownNonce mit NonceConsumedByOtherTx an. Die Behauptung, sie bliebe scharf und sende ungehemmt weiter, ist widerlegt. Verbleibende Pool-/Fortschrittsprobleme getrennt testen.
- **BSC3-055:** Senderweite LIVE-Invarianten herstellen; ein kompletter Ein-Lane-Neubau ist nicht als einzig erlaubter Fix bewiesen.
- **BSC3-059:** Eine zehnminütige LIVE-Grant-Grenze ist **keine** dokumentierte Pflicht. Die optionale G0.4-Erfassung hat ein Zehn-Minuten-Limit. Ohne neue Betreiberentscheidung kein willkürliches LIVE-Limit einbauen.
- **BSC3-060:** Alte Generation-/Journal-Decoder und Testdeployments sind nicht automatisch fehlerhafter Legacy-Code. Keine Löschung ohne sichere Migration.
- **BSC3-066:** G0.7 und der generische LIVE-Pfad sind nicht dieselbe Freigabeautorität. Keine ungeprüfte Zusammenlegung; Sicherheit und konkrete Wire-Feldsemantik müssen stimmen.
- **BSC3-068:** Ein nicht offengelegter Versuch ist noch kein beliebig erstattbarer Slot. Rückgabe nur mit dauerhaftem, atomarem Nachweis, der Rennen, Absturz und verspäteten Egress ausschließt; andernfalls konservativ zählen.
- **BSC3-001/070:** Ein normaler Git-Push auf main vergibt keinen G0.7-Grant. Der Sonderref-Grant ist separat, aber menschliche Identität, vertrauenswürdiges Git-Binary, wirksame Git-/SSH-Konfiguration und Remote-Identität müssen ausreichend abgesichert sein.
- **BSC3-065:** v5.27.2 stammt aus dem STATUS vom 27.10., nicht aus einer aktuellen erneuten Chain-Abfrage. Vor einem qualifizierten Refreeze die echte aktuelle Chain-Beobachtung und ökonomische EVM-Gegenprüfung einholen; Drift niemals durch Versionswechsel im Code oder gefälschten PASS verdecken.
- **BSC3-051:** poll_release_response existiert bereits. Vorhandenen Parser und den echten Transport-Lifecycle anschließen, keine zweite Antwort-Engine.
- **BSC3-058:** Das globale Abschalten von SIGN/SEND bleibt ein echter vollständiger Stop. EXIT-ONLY ist eine **eigene eng autorisierte Betriebsfunktion**, niemals eine versteckte Umgehung der abgeschalteten Controls.

## 5. Definition of Done und Berichtsformat

Ein Paket ist erst **FIX_VERIFIED**, wenn der echte produktive Pfad geändert und der relevante Fehler reproduzierbar ausgeschlossen wurde; zugehörige Unit-/Integration-/Release-/Replay-/CI-Checks erfolgreich ausgeführt sind; Crash/Restart und ökonomische Sicherheit nicht verschlechtert wurden; eine unabhängige Prüfung erfolgt ist; und der **Commit auf origin/main sichtbar** ist.

Berichte nach jedem Paket **kurz und tabellarisch**, mindestens: ID(s), vorherige Ursache (Datei/Zeilen), konkreter Fix, Testname mit PASS/FAIL/SKIP, verbleibende Risiken, Git-Commit und bestätigter Remote-HEAD. Die einzige offene Arbeit nach einem Paket als nächste Code-Aufgabe nennen. Keine unbewiesenen Zusagen, kein bloßes „alles fertig“ und keine neuen Dokumente ohne Codebedarf.

Die **abschließende Gesamtsystemabnahme** erfolgt erst nach den produktiven Fixes und gemäß dem Phasenplan vom 28.10.: echte Native-/Gateway-/Frontend-Kette, synthetisch autorisiertes effective=LIVE ohne Egress, kompletter BUY-/Approve-/SELL-Durchlauf, Budget-/Nonce-/Provider-/Deployment-/Crash-/Reorg-/Stop-/Recovery-Matrix, Release-CI und getrennte externe Qualifikation. Ein tatsächlicher Mainnet-Erfolg, LIVE-Wirtschaftsfreigabe, Profitabilität oder der G0.7/G0.8-PASS darf **niemals** aus dieser technischen Testkampagne abgeleitet werden.

**Beginne unmittelbar mit dem obersten bestätigten, bislang nicht reparierten P0-Produktionsfehler. Code und Test zuerst, danach Commit und Push.**
