# BSC – VERBINDLICHER UMSETZUNGSAUFTRAG: Fehlstellen V1 + Bug V2

**25.10.2026 · Untergeordneter Ausführungsauftrag, kein neuer Phasenplan und keine wirtschaftliche Freigabe.**

Claude, arbeite sämtliche noch offenen und am aktuellen Code bestätigten Fehler ab: `bsc/docs/5_OFFENE_FEHLER/fehlerliste_bsc_v1.md`, `bsc/docs/5_OFFENE_FEHLER/BSC_Bug_V2.md`, unter Berücksichtigung des älteren, bereits teilweise erledigten `bsc/docs/5_OFFENE_FEHLER/bsc_bug_v1.md`. Ein widerlegter, bereits nachgewiesen geschlossener oder fremder phasenblockierter Punkt ist **kein** erneut zu fixender Bug.

**Höchste technische Autorität:** `bsc/docs/1_PLAN/PHASENPLAN.md`, ergänzend `bsc/docs/1_PLAN/GATE0_TESTPLAN_SHORT.md`, freigegebene V5-/DRY-/Approval-/Original-Evidence-/Record-/Wire-Verträge. Bestehender Frontendvertrag: `frontend/docs/FRONTEND_PHASENPLAN.md` §§12–13, `frontend/docs/BSC_FRONTEND_BINDINGS_V1.md`, `frontend/docs/ASTRA_BSC_FRONTEND_IMPLEMENTATION_ORDER.md`, `frontend/docs/GATEWAY_WIRE_PROTOCOL_V1_3.md`. Project_0 ist methodisches Vorbild, nicht BSC-Autorität.

## 1. Unveränderliche Regeln

Kein produktiver Mock, Fake, Stub, Placeholder, falscher PASS, erfundener Messwert, interne JSON-/JSONL-/Python-/JS-Brücke, doppelte Engine, zweiter Signer/Controller, verdecktes Hardcoding oder toter Produktionspfad. Vollständige echte Rust-/Go-Implementierung entlang bestehender nativer Caller und versionierter binärer Daten-/Evidence-Verträge. Extern zwingendes Builder-/Chain-JSON-RPC nur am Netzrand. Historische signierte/gebundene Beweise unverändert. Ein externes UNKNOWN ist kein 0 und keine Autorisierung. Protokoll- und Approval-Konstanten nicht zu willkürlichen GUI-Tunables machen. Kein Architektur-Neustart oder unkontrollierter Rewrite.

## 2. Phasengrenze

G0.7 CODE_READY ohne Geldautorität → ausdrücklich und separat genehmigter einziger kontrollierter Mainnet-G0.7-BUY → tatsächliche G0.8-Erwerbs-/Ordering-/Kostenprüfung → PASS/ADAPT/STOP → erst danach C1/C2/C3 und weitere Produktion. Die bindende begrenzte Offline-Ausnahme für E5–E8 bleibt erhalten. Kein echter Key-SIGN, ARM, Funding, Paid-Request, BUY, SEND, LIVE oder Nonce-Burn ohne ausdrückliche Nutzerfreigabe. `UNARMED`, `approved=false`, `execution_ready=false` bleiben unberührt. Kein Auto-Retry.

## 3. Konkrete Reihenfolge

**Gate A:** BSC-001–007. BSC-008 (+2/48Club-Flags/Provider/DRY-/V5-Konsequenzen), BSC-011 (G0.8-Verdikt/Evidence-Reihenfolge) und BSC-049 (SELF_A-/SELF_B-Profil-/Risikowahl) **vor** der jeweiligen bindenden SIGN-/V5-/Versuchsfreigabe fest entscheiden, nicht nach dem Versuch. BSC-005 braucht die wirklich maschinen-/checkoutübergreifend dauerhafte One-Attempt-/Egress-Authority; `valid_until`/Disarm allein reicht nicht. BSC-048 Gate-0-JS-/JSON-Grenzen inventarisieren und aktive G0.7/G0.8-JSON-Interna vor Live-Autorisierung mit Byte-/Feldparität minimal in binäre Owner überführen. Kein Neubau aller DRY-Tools als SIGN-Vorbedingung.

**Gate B:** BSC-008–017 am beschlossenen Profil technisch/Provider-/G0.8-seitig fertig qualifizieren. Builder-Gesamtdeadline, Frische, Receipt, UNKNOWN, Chain-/Archive-/Reconcile-/G0.8-Verdikte mit echten Beweisen. Keine stillschweigende neue Approval-/TX-/Gas-/Fee-/Budget-Semantik.

**BSC-050 Vollscan vor Gate C:** Einmal alle produktiven BSC-Rust-/Go- und tatsächlich BSC-relevanten Proto-/Gateway-/Frontend-Caller und Feature-/Release-Pfade nach Mock, Stub, Legacy/V1, internem JSON, Hardcoding, Testnähten, Clock-/UNKNOWN-0, Doppelpfaden, fehlenden Ownern abgleichen. Je Fund Datei::Funktion, SHA, aktiver Pfad, Gefahr, Ticket oder KEIN_BUG. Gate A braucht nur den Gate-0-Teilscan; Gesamtinventur darf die G0.7-Fertigstellung nicht blockieren. BSC-051/053 behandeln geteilten Gateway-Code: Project_0-Kompatibilität und kanonische Idempotenz/Receipts erhalten; keine fremde Project_0-Änderung ohne Owner-Freigabe. BSC-052 ist ein Go-C1-Gap-Clockthema nach Gate-0.

**Gate C nach abgeschlossenem realem Gate-0-Verdikt:** BSC-018–036, 043–047, 050–053 nur in ihren vorhandenen C0/C1/C2/C3–J-Fachphasen. Keine vorgezogenen Source-/Venue-LIVE-Schalter, FOREIGN_AUTO- oder Produktionsfreigaben. **Gate D:** BSC-037–042 erst bei real gemessener vergleichbarer Latenz-/Kostenbasis; keine ausgedachten 10 µs, 0,1 ms, N+1- oder Erfolgsquoten.

## 4. Nur maximal EIN optionaler Prüfagent

Standardmäßig **allein** implementieren. Höchstens ein read-only Agent bei fachlich begründetem SIGN-/Wallet-/Nonce-/Budget-/Crash-/UNKNOWN-/gemeinsamer Gateway-/Schema-/E2E-Risiko. Nur betroffenen Diff/Caller/SHA prüfen, keine Agentenkaskade, keine Voll-Repository-Nachprüfung durch Agenten. Kein Agent bei Routinefixes ist keine Abnahmelücke. Der Hauptimplementierer trägt die Verantwortung.

## 5. Gezielte Tests

Je unabhängiger Ursache möglichst ein RED-Nachweis, kleinster echter Native-Fix, GREEN und relevante positive/negative/Crash-/Restart-/UNKNOWN-/Reorg-Regression. Betroffene fmt/Clippy/Feature/Release/Go-/Proto-/Gateway-/TypeScript-/E2E-Gates einmal je sinnvoll abgeschlossenem Paket, nicht nach jeder Codezeile. Sicherheitskritische Negativfälle nicht aus Einspargründen weglassen. Benchmarks nur bei tatsächlicher Performancekorrektur und gleicher CPU/Clock/Compiler/Sample-/Providerbasis. Kein Offline-Mock als Mainnet-/Provider-/Producer-T0-Nachweis; externe Qualifikation bleibt NOT_RUN/UNQUALIFIED.

## 6. BSC bis in das bestehende Frontend verdrahten

**Status-Hinweg:** BSC-Native-Rust-/Go-Owner → `bsc/engine/crates/qualification/src/status/projection.rs` → `protocol/schemas/darkbot/bsc/v1/status.proto` → BSC-Reporter/Gateway → bestehender Protobuf-WS Topics 30/31/32/33 → `frontend/src/features/bsc/api/bscAdapter.ts` → `frontend/src/features/bsc/store/bscStore.ts` → einer der fünf vorhandenen Reiter Overview / Launch-BUY / Monitoring / Analytics / Wallets. Topic 40 bleibt EVENT-UNAVAILABLE ohne echten Publisher/Decoder.

**Command-Rückweg:** Nur die bereits erlaubten `bsc.readiness.start`, `bsc.readiness.stop`, `bsc.candidate.select`: vorhandener Reiter → `darkbot.control.v1` POST `/api/v1/command` → authentisiertes Gateway/Allowlist/Idempotenz → derselbe native BSC-Policy-/Job-Owner mit CAS/Guard/Durability/Receipt → echte neue STATE-Projektion. Keine BUY-/SIGN-/ARM-/SEND-/SWEEP-Aktion, kein zweites Gateway, kein neuer Store/Reiter/Basic-Advanced. Wenn ein rein interner Fix weder öffentliches Feld noch Command noch Statussemantik betrifft, `UI_NOT_APPLICABLE` belegen und Status-Regression prüfen, statt Proto-/React-Fake hinzuzufügen. Alle echten neuen öffentlichen Felder gehören nur in die bestehende F0-VAR-`frontend/contracts/bsc-fields.json`- und generierte Native-Field-Inventur.

## 7. Abnahme und Git-Disziplin

Vor Ticket: neuesten `main`-SHA, tatsächlichen Aufrufpfad/Featureflag und PHASENPLAN-ID nachweisen. Nach Ticket: reproduzierbarer RED→GREEN, erforderliche native Safety- und ggf. E2E-Rundweg-Prüfung, echte Exitcodes, Commit-SHA, Status `CODE_FIXED/LOCAL_PASS` oder sachlich `OPEN/BLOCKED/NOT_RUN`. Dokumentation nur gesammelt am Paket-Gate. Ausschließlich `main`, keine Branches/Force-Pushes, keine fremden Änderungen überschreiben, nur abgeschlossene und zielgerichtet getestete Pakete als fertig pushen. Historische G0.7/G0.8-Evidence, Signed-Bytes/Approval-Pins nie unbemerkt ändern.

**Priorität: echten Defekt fachlich korrekt beheben, nicht Tests/Audits/Agentenarbeit zum Selbstzweck machen. Nach sauberem Abschluss zur nächsten zulässigen Phase übergehen; niemals ein STOP-/Approval-Gate zur Beschleunigung umgehen.**