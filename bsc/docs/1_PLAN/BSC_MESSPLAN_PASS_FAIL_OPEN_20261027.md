# BSC – Verbindlicher Mess- und Abnahmeplan (PASS / FAIL / OPEN)

**Stand:** 27.10.2026 · **Ausgangs-HEAD:** `8b4721bba41fb5c482976d99590bc2119204df13` · **Geltungsbereich:** ausschließlich BSC, nicht Project_0/Solana.  
**Zweck:** Vorhandene Messbelege von fehlenden Messungen trennen; den kontrollierten ersten Mainnet-BUY, die anschließende G0.8-Prüfung und spätere gezielte BUY→SELL-Versuche ohne pauschale Wartezeit vorbereiten. Dieses Dokument ist ein **Messregister, keine Mainnet-Transaktionsfreigabe**.

## 1. Statuslegende / Regeln zur Beweiskraft

- **PASS** = belegtes Ergebnis **nur innerhalb des ausdrücklich genannten Scopes** (etwa historisch, lokal, PAPER, SHADOW, Testnet oder echte Mainnet-Transaktion). Ein lokaler PASS wird nie stillschweigend zum Produktions-/Erwerbs-PASS.
- **FAIL** = gemessener Fehler bzw. negatives Ergebnis **für das konkret benannte Kriterium**, nicht automatisch für den gesamten Bot.
- **OPEN** = nicht ausgeführt, unvollständig, unzureichende Originalbelege, unqualifizierte Umgebung, in neuer Version nicht wiederholt oder noch kein akzeptierter numerischer Schwellenwert. `UNKNOWN`, `NOT_EVALUABLE`, `EXTERNAL_BLOCKED` und `UNRESOLVED` bleiben zusätzlich als präzise Evidenz-Verdikte erhalten; sie sind **kein PASS**.
- **Ältere Messungen** gelten nicht automatisch nach Portal-/ABI-/Code-/Config-/Provider-Änderungen. `PASS` ohne exakten SHA, Build/Host, Run-ID, Messgrenze, Sample/Nenner, Roh-Evidence und nachvollziehbaren Test ist nicht zulässig.
- **Zwei unterschiedliche Primärfragen:** (A) *G0.7/G0.8* „Wurde der intendierte Token erfolgreich gekauft, wie früh gegenüber Konkurrenz und zu welchen wirklichen Kosten?“ (B) *späterer BUY→SELL-Betrieb* „Wurde die Position real geschlossen und war der Nettoertrag nach sämtlichen Kosten positiv?“ Ein bestandener Kauf ist **kein** Profit-PASS.
- Die 27.10.-Nutzerentscheidung in `PHASENPLAN.md` hat Vorrang vor der alten **7-Tage-Wartepflicht**: explorative G0.4-Sichtung **höchstens 10 Minuten, optional**, keine Mindestzeit, keine Pflicht, vor einem einzelnen G0.7-Test auf einen Census zu warten. Für zielgerichtete Tests zählt die **Anzahl freigegebener Ereignisse/Versuche**, nicht eine vorgegebene Beobachtungsdauer. `max_buy_attempts_per_day=10` ist ein **Beispiel für ein konfigurierbares Maximum**, keine Verpflichtung, zehn BUYs auszuführen.

## 2. Bereits durchgeführte Messungen – bestätigter Dokumentationsstand

**Quellen:** `bsc/docs/1_PLAN/STATUS.md` (aktuelle Übersicht), `bsc/docs/3_BEWEISE/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md` (historische Gate-0-Werte, Stand 16.10.), `bsc/docs/3_BEWEISE/WSS_INGRESS_BEOBACHTUNG_2026-10-22.md`, `bsc/native/evidence/{c1,c3,f,g,g06,h,i,p8}/`, `GATE0_TESTPLAN_SHORT.md`. Es handelt sich hier um einen **read-only Belegabgleich im Repository**, nicht um eine frische Ausführung der Tests auf dem Nutzer-Mac.

| ID | Was wurde gemessen? | Belegtes Ergebnis / Messgrenze | Status | Aussage / was daraus *nicht* folgt |
|---|---|---|---|---|
| M01 | **G0.2/G0.3 Self-Launch**: tokenbezogene Vorbereitung, genaue Launch→BUY-Semantik | Historischer DRY-/Offline-Pfad für qualifizierten Flap-Self-Launch PASS; zuvor Portal v5.24; aktuell v5.27.2 nicht gleicher Freeze | **PASS** (historisch/offline) | Korrektheitsnachweis eines **eigenen** Profil-Scope, **kein** echter Kauf und kein Nachweis für fremde Launches |
| M02 | **G0.5 früher Self-Launch-Transport** bis definiertem Builder-Handoff | Im historischen Gate-0-Ledger 48Club p99 **0,584 ms**, BlockRazor p99 **0,527 ms**, je 0/100 über 1 ms | **PASS** (historische, begrenzte Transportprobe) | Kein tatsächlicher on-chain BUY, keine aktuelle End-to-End-`T_send`-Requalifikation der finalen Produktivruntime, kein `T_buy` |
| M03 | **C1.9 lokal native Ingress→L1** | M4 Max, n=10.000, p50 **2 µs**, p99 **8 µs** (STATUS, Paket 8) | **PASS** (lokal) | Kein Mainnet-geth-P2P-Produktionslauf und keine externe Inclusion |
| M04 | **C3 Rust Ingress→PAPER-Sink** | M4 Max, n=2.000, p50 **214 µs**, p99 **234 µs**, max **260 µs** (Paket 8). Separates G0.6 Basismuster p50 **218 µs**, p99 **258 µs** | **PASS** (lokal, PAPER) | `T_shadow_sink` ist ausdrücklich **nicht `T_send`** |
| M05 | **G0.6 synthetische parallele Launches** | k=2: letzter Platz p99 **521 µs**; k=4: **1.020 µs** (10/500 ≥1 ms); k=8: **2.020 µs** (ab Position 4 alle ≥1 ms) | **FAIL** (nur synthetische <1-ms-Schwelle bei k≥4) | Echte Bursthäufigkeit und LIVE-Nonce-Reihenfolge **nicht gemessen**; kein Beleg, dass diese Last produktiv auftritt |
| M06 | **Strategy C PAPER-BUY-/Exit-Rechenpfad** | Strategy BUY p99 **134 µs**, PAPER-Exit p99 **256 µs** (n=2.000, Phase I); ohne echten Transport | **PASS** (lokale Rechenzeit) | Keine wirkliche SELL-Transaktion und kein realisierter Gewinn |
| M07 | **Quote/Gas/Ingress-Mikromessungen** | Quote p99 **208 ns**, n=100.000; ingest p99 **84 ns**; Egress-Queue p99 **2,9 µs**, n=2.000 | **PASS** (isolierte Diagnostik) | Diese Zahlen dürfen nicht zur tatsächlichen Kauf-/Netzwerklatenz addiert oder als `T_buy` verkauft werden |
| M08 | **WSS-Marktbeobachtung 22.10.** | 30 min: 116 Launches, 48/116 vor **lokalem** Head von irgendeiner Quelle gesehen; PublicNode 397.901 Nachrichten, dRPC im betrachteten Scope 52,6 % connected; 5/5 Full-TX-Objektform-Probe | **PASS** (historische beobachtete Sensor-Diagnostik) | Werte aus Nutzer-Logs ohne im Bericht gebundene Rohartefakt-Hashes, keine vollständige Signed-TX-/Actionable-Qualifikation, **nicht vor T0** |
| M09 | **G0.2 fremde Flap-Profile** | Erste Kampagnenstunden: **0/28** beobachtete fremde Launches im qualifizierten C3-BUY-Profil; 22 andere Quote-Token, 5 Creator-BUY, 1 andere Steuerdauer (STATUS/G0.2, `ce385f95`) | **FAIL** (Fremd-BUY-Profilabdeckung dieser Probe) | Produktionspipeline kann für diese beobachteten Profile noch keinen gültigen pre-block BUY belegen; **kein** Beweis, dass alle Flap-Launches immer ungeeignet sind |
| M10 | **Fremde Konkurrenz-/N+1-Platzierung** | Neuer v10-Census vorhanden (`b3b0fd48`); bisheriger Bericht: 10/11 beobachtete fremde Erst-BUYs früheste externe Stelle N+1, davon keine vollständig vor Block sichtbar auf erfassten freien Feeds | **PASS** (beobachtete Historienklassifikation), **OPEN** (eigene Platzierung) | Fremde BUYs sind nicht unsere BUYs; daraus folgt keine eigene N+1- oder Sub-ms-Fähigkeit |
| M11 | **Native DRY und Kostenprüfungen** | G0.6a DRY-1…DRY-4 PASS für eingefrorenen minimalen Self-Launch, Null-Egress; historisches BUY-Input **0,000001 BNB**, signiertes Gas **0,05 gwei**; V5-Obergrenze **0,0004811635 BNB gesamt** | **PASS** (historisch exakt gebundener DRY) | Werte sind **keine frische** v5.27.2-Versuchsfreigabe; Gas/Nonce/Wallet/Contract müssen vor realem G0.7 erneut stimmen |
| M12 | **Nicht ausführbare Builder-Probe** | 16.10.: 11/11 nicht ausführbare Bundles auswertbar, Methodik/Format angenommen, Flags typisiert erkannt | **PASS** (Parsing/Submission-Probe), **OPEN** (Builder Inclusion) | Keine Garantie, dass Builder tatsächlich das Bundle in gewünschter N+1-Position produziert; kein aktueller Transport-PASS |
| M13 | **H-Strategie über reale eingelesene Marktdaten, PAPER-Bewertung** | 60 min SHADOW, 192 Flap-Launches; 3/192 Eintritte; 3/3 geschlossen, **alle drei negativ**: insgesamt **−0,00389005 BNB**, mittlerer Positions-Return **−12,9 %**; 201/201 Replay-Ausgaben reproduziert (`cec8336a`) | **PASS** (Replay), **FAIL** (Gewinn dieser 3 PAPER-Positionen) | Keine positive Expectancy und keine tatsächliche ausgeführte SELL-Transaktion; 3 Fälle reichen nicht für ein allgemeines Strategieurteil |
| M14 | **Runtime 60-min Betrieb / Speicher** | M4 Max, 720 Stichproben; CPU Ø **16,3 % eines Kerns**; RSS **148 MB (1 min) → 451 MB (60 min)**, Wachstum ab ~30 min abgeflacht, noch nicht ganz flach | **PASS** (60-min-Beobachtung), **OPEN** (24/7-Stabilität) | Kein Langzeitnachweis eines vollständig stabilen Plateaus; nicht Linux-Ziel |
| M15 | **C2/Journal/Crash/Replay** | Paket 8: C2 nach SIGKILL RESTORED, replay-proof PASS auf abgeschlossenem Folgelauf; gekillter Lauf korrekt INCOMPLETE/UNTERMINATED, corrupt checkpoint verweigert | **PASS** (dokumentierter Testscope) | Kein Nachweis über offenen Mainnet-BUY bei Crash |
| M16 | **Native/Gateway/Frontend-Regression auf Release-Kandidat** | An `8f26b02a`: Rust 1.339 PASS/0 FAIL/22 ignored, Gateway 74, Frontend 113; Browser-E2E debug 1×/release 2× PASS; F0-VAR 2.603 Felder PASS | **PASS** (historischer SHA) | Nicht automatisch PASS auf neuem Wallet-/Wirtschafts-Control-Code; später berichtete 2 rote E2E ohne zugeordnete Fehlerlogs bleiben gesondert **OPEN** |
| M17 | **G0.8 Archivzugriff** | 27.10.: Blast `--qualify-history` als QUALIFIED für historische Tiefen 1/128/1200/28800 dokumentiert | **PASS** (damaliger read-only Historienzugriff), **OPEN** (neuer Live-Versuch) | Provider muss zum konkreten Versuch frisch qualifiziert sein; **kein** G0.8-Acquisition-Ergebnis |
| M18 | **G0.4 7-Tage-Kampagne** | `STATUS.md` meldet aktiven, 7 Tage konfigurierten PUBLICNODE+DRPC-SHADOW-Capture; `campaign.sh` hat 604800 s und eine 7-Tage-Deadline; `g04_criteria.env` noch `PROPOSED` | **OPEN** | 28.10.-Entscheidung verlangt **maximal 10 Minuten, optional** und danach geordnetes Beenden. Remote-Git kann nicht verifizieren, ob der Prozess auf dem Mac schon beendet wurde; kein Coverage-Verdikt behaupten |

**Gesamteinordnung:** Zahlreiche **lokale** Teilmessungen PASS; **kein realer G0.7 Mainnet-Kauf**, **kein echter G0.8 Token-Erwerbs-/Ordering-/Kosten-PASS**, **kein realer SELL**, **kein belegter Netto-LIVE-Profit**, **kein original-bezeugtes physisches `T_buy−T0 <1 ms`**.

## 2a. Softwaretest- und Integrationsinventar – nachträglich vollständig nachgetragen (27.10.2026)

**Korrektur zum Erstentwurf:** Die Abschnitte M01–M18 erfassen schwerpunktmäßig *physikalische Messwerte und Markt-/PAPER-Ergebnisse*. Sie sind **kein vollständiges Verzeichnis aller durchgeführten Softwaretests**. Claude hat erheblich mehr getestet. Die folgende Tabelle erfasst die in den bestehenden `STATUS.md`-, `EVIDENCE.md`-, Phase-Gate- und Evidence-Verzeichnissen **konkret dokumentierten Testpakete**. In diesem Dokument bedeutet `LOCAL_PASS` **„auf dem bezeichneten historischen Commit mit PASS protokolliert“**, nicht „von diesem read-only GitHub-Audit erneut auf dem Nutzer-Mac ausgeführt“. `INTEGRATION_PASS` schließt tatsächliche übergreifende Prozesse ein; `RELEASE_PASS` den gebauten Kandidaten. `REVIEW_PASS` ist eine separate Reviewentscheidung. `PASS` ist nie vom historischen Commit auf den neuesten Commit vererbbar.

**Wesentliche Referenzen:**
- [`STATUS.md`](STATUS.md), Fortschritt Pakete 0–9, Gate 0, „Technische Gesamtbetriebsabnahme“ und „Release-Kandidat“.
- [`EVIDENCE.md`](../3_BEWEISE/EVIDENCE.md), Protokolle R1–R8 (§4), R9–R13 (§6), C1.5–C1.9/Go-Rust-Interop (§7).
- [`GATE0_TESTPLAN_SHORT.md`](GATE0_TESTPLAN_SHORT.md), G0.1–G0.8 und historischer DRY.
- [`bsc/native/evidence/c1/`](../../native/evidence/c1/), insbesondere `c1-10-gates-20261026T003339Z/{common-rust.txt,go-common.txt,interop.txt,c1-*.txt}` und `c1-10-freeze-818525b1/`.
- [`bsc/native/evidence/p8/recovery_1793074269302/README.md`](../../native/evidence/p8/recovery_1793074269302/README.md), der tatsächlich gestartete Release-Kandidat `8f26b02a`.
- [`bsc/native/evidence/h/live_strategy_1793049942243/README.md`](../../native/evidence/h/live_strategy_1793049942243/README.md), Replays und PAPER-Wirtschaft.
- [`ops/tests/bsc_control_test.py`](../../../ops/tests/bsc_control_test.py), reale Gateway/Reporter/Browser-E2E-Kette, enthält vor dem Browser einen **ignored, einzeln gestarteten, externen G0.8-RPC-Test**.

### 2a.1 Aktueller dokumentierter Referenz-Release – **SHA `8f26b02a`** (Paket 8/9, 27.10.)

| Test-ID | Testgruppe / tatsächlich dokumentierter Umfang | Ergebnis | Status auf `8f26b02a` | Originalquelle / Vorbehalt |
|---|---|---|---|---|
| T01 | Rust **gesamter BSC-Engine-Workspace**, `cargo test --offline --no-fail-fast --workspace --all-features` | **84 Suiten; 1.339 PASS, 0 FAIL, 22 ignored** | **LOCAL_PASS** | STATUS Gesamtbetriebsabnahme §Tests; 22 ignored = **NOT_RUN**, nicht 22 PASS |
| T02 | Gateway `cargo test` | **74 PASS** | **LOCAL_PASS** | STATUS §Tests; zählt getrennt von BSC Rust-Workspace |
| T03 | Frontend/Vitest | **113 PASS** | **LOCAL_PASS** | STATUS §Tests; nicht mit Playwright vermischen |
| T04 | BSC Browser-E2E im Debug-Profil, echte isolierte Native/Reporter/Gateway/React-Pfade | **1 kompletter Durchlauf PASS** | **INTEGRATION_PASS** | STATUS §Integration, [E2E-Harness](../../../ops/tests/bsc_control_test.py), keine globale Mainnet-LIVE-Ausführung |
| T05 | BSC Browser-E2E im Release-Profil mit `darkbotctl build`-Artefakten | **2 komplette Durchläufe PASS** | **INTEGRATION_PASS** | STATUS §Integration; der Harness hat zusätzlich einen read-only G0.8-RPC-Schritt, der separat fehlgehen kann |
| T06 | F0-VAR öffentliches BSC-Feld-/Datenflussinventar | **2.603 geprüfte Felder, PASS** | **INTEGRATION_PASS** | STATUS §Integration. **Felder sind nicht 2.603 einzelne Testfälle** |
| T07 | Release-Build `ops/darkbotctl build`, Frontend, Gateway, BSC-Artefakte | **Build PASS** | **RELEASE_PASS** | STATUS §Release; vollständige Manifest-/Schema-/Binary-SHAs gelistet |
| T08 | `python3 ops/release_manifest.py`: Release-Identität und Koordination | **PASS** | **RELEASE_PASS** | STATUS §Release-Build/Manifest; historischer SHA `8f26b02a`, kein erneuter Build auf Dokumentations-HEAD |
| T09 | Rust `clippy -D warnings` (im tatsächlich bezeichneten Scope: runtime ohne Features) und Build-Checks | **PASS** | **LOCAL_PASS** | STATUS §Tests; ausdrücklich keine Behauptung „alle denkbaren Feature-Kombinationen Clippy-PASS“ |
| T10 | Live-**read-only SHADOW**-Runtime mit echter öffentlicher WSS-Quelle und nativer PAPER-Identity | gestartet, 150 s Run vor kontrolliertem Kill; zweiter 150 s Run geordnet beendet | **INTEGRATION_PASS** (definierter Start-/Stop-Scope) | `p8/recovery_1793074269302/README.md`; nicht Mainnet-LIVE |
| T11 | `SIGKILL` → launchd Restart → C2-Checkpoint/64 Blöcke RESTORED | **PASS** | **INTEGRATION_PASS** | `p8/recovery_1793074269302/`, neuer Run nach Kill, State wiederhergestellt |
| T12 | Erfolgreich beendeter Restart-Run: `replay-proof`, `capture-quality`, `strategy-replay` | Replay PASS, Capture **CLEAN**, Strategy Replay **3/3 PASS** | **INTEGRATION_PASS** | Phase-F-`CLEAN/MET` **nicht** erfüllt, weil native geth-Quelle fehlte |
| T13 | Erwartete Negativfälle im durch `SIGKILL` abgebrochenen ursprünglichen Run | replay-proof **FAIL**, capture **NOT_CLEAN**, Strategy **INCOMPLETE**, Census **UNTERMINATED** | **FAIL_EXPECTED / korrekt erkannt** | Kein Fehler des Neustart-Tests; **den gekillten Run niemals als CLEAN/PASS darstellen** |
| T14 | Four-Drift/Classic im restarted Release-Run | Drift Guard `MATCH`, **3 Classic-Kandidaten**; Token ableitbar/actionable **nicht** behauptet; C3 `DEPLOYMENT_DRIFT` | **INTEGRATION_PASS** für korrektes Fail-closed | `p8/recovery_1793074269302/README.md`; kein Four-BUY |
| T15 | Unabhängiger Gesamt-Review Paket 8 | Durchgang 1 **FAIL**, Durchgang 2 PASS, Durchgang 3 **PASS** (H1/M1/M2/LOW-Funde korrigiert) | **REVIEW_PASS** (nach Fixes) | STATUS §Reviewer; Details bei `b6035809`, behobene Commits in STATUS |
| T16 | Release-Kandidat Paket 9: Status READ_ONLY/SHADOW/PAPER | „TECHNISCH FERTIG — EXTERNAL_BLOCKED“ laut STATUS | **RELEASE_PASS** nur für diesen Betriebs-Scope | Kein `READY_FOR_LIVE` und **keine** G0.7/G0.8/Mainnet-Exekution nachgewiesen |

### 2a.2 Ältere, zusätzliche Testserien – **historisch separat**, keine Summierung mit T01–T16

| Test-ID | Datum/Commit oder Serie | Prüfung | Dokumentiertes Ergebnis | Status und Beleg |
|---|---|---|---|---|
| T17 | R1–R8, 16.10. | BSC-Rust-Workspace mit G0.7-Feature, `--no-fail-fast` | **426 PASS, 0 FAIL, 5 ignored** | **LOCAL_PASS** · `EVIDENCE.md` §4 T6 |
| T18 | R9–R13, 16.10. | Regressionslauf nach Umbenennung und Modulgrenzen, zwei Läufe | jeweils **426 PASS, 0 FAIL, 5 ignored** | **LOCAL_PASS** · `EVIDENCE.md` §6 U1/U3 |
| T19 | R1–R8, Nachtest `cargo fmt` | Runner/Qualifier/Fastlane-Regression | **154 PASS, 0 FAIL** | **LOCAL_PASS** · `EVIDENCE.md` §4 T8 |
| T20 | R1–R8 | Offline-Smoke: Quote, DRY-Konfiguration, G0.7/G0.8-No-Input-Rejection und Shell-Guards | **17/17 PASS** | **LOCAL_PASS** · `EVIDENCE.md` §4 T9 |
| T21 | R1–R8 | Gespeicherte DRY-Quote-Byte-Parität + Default | **8/8 PASS** | **LOCAL_PASS** · `EVIDENCE.md` §4 T3 |
| T22 | R9–R13 | C1-Qualifikationstools `python3 -m unittest test_c1_qualification_tools` | **20/20 PASS** | **LOCAL_PASS** · `EVIDENCE.md` §6 U2 |
| T23 | R1–R8 | G0.7 Guard-/No-Send-Lib-Fokus | 20+6, später 21+6+4 PASS nach jeweiligem Stand | **LOCAL_PASS** · `EVIDENCE.md` §4 T2/T4; historisch unterschiedliche Testversionen |
| T24 | C1 Wire V2 Rev2, 16.10. | `geth_ipc`: Unit/Doctest, C1.5 Allocation, C1.8 Stress | Lib **122 PASS / 5 ignored**, Allocation 1, Stress 8, Doctests 1+2 PASS | **LOCAL_PASS** · `EVIDENCE.md` §7 C1-T1/T5/T8 |
| T25 | C1 Wire V2 Rev2, 16.10. | **Go `-race`** nur DarkBot Fastlane-Overlay, Pin `cdb7548b` | **86 PASS, 0 FAIL, 5 SKIP** | **LOCAL_PASS** · `EVIDENCE.md` §7 C1-T2; SKIP nicht PASS |
| T26 | C1 §7 | Rust↔Go Lifecycle- und Ingress-Interop `c13` / `c14` vor und nach Modulzerlegung | **beide PASS**, vor/nach | **INTEGRATION_PASS** · `EVIDENCE.md` §7 C1-T3/4/6/7; Gate-Protokoll `c1-10-gates-20261026T003339Z/interop.txt` zeigt Exit 0 |
| T27 | C1 §7 | Vollständige frühe Workspace-Regression **nach** C1-Integration | **479 PASS, 0 FAIL, 5 ignored** | **LOCAL_PASS** · `EVIDENCE.md` §7 C1-T9 |
| T28 | C1 Wire V2 §7 | Binäre HOT0-Codegen-/ABI-Vergleichbarkeit bei Modulzerlegung | **98/98 Crate-Funktionen instruktionsidentisch**, 53 API-Elemente unverändert | **LOCAL_PASS** (Maschinencode-Vergleich) · `EVIDENCE.md` §7 |
| T29 | C1.9b Testnet | Gepinnter geth / ETH/68 Hook, `AcceptTxs` Testnet-spezifisch | **1/1 kontrollierter Launch erfasst**, 0 misses, 0 invalid records, 0 lane faults; Analyzer PASS | **INTEGRATION_PASS** auf **Chapel Chain 97** · `GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md` und Gate-0-Testplan, **kein Mainnet-Ingress-PASS** |
| T30 | C1.10, `818525b1` Freeze | Finale C1.5–C1.9 Go/geth-/Rust-/Interop-/fmt-/Clippy-Gates und Build-Identität | **Phase-Gate PASS / FROZEN**; gezielte Prüflogs mit Exitcodes versioniert | **REVIEW_PASS + LOCAL_PASS** · `STATUS.md` Paket 3; `c1-10-gates-20261026T003339Z/` und `c1-10-freeze-818525b1/` |
| T31 | Gepinnter upstream geth Race-Vergleich | Breite Upstream-Race-Suites `./eth`, `./eth/protocols/eth` | **FAIL auf aktuellem und sauberem Pin**, dokumentiert u.a. 55 vs. 51 Race-Warnungen | **FAIL_BASELINE_PRESENT**, **nicht racefrei** und **nicht als behobener DarkBot-Defekt** bewerten · `GATE0_TESTPLAN_SHORT.md`, `EVIDENCE.md`/C1-Review |
| T32 | 17.10. historischer G0.7/G0.8-M1–M8-Abgleich | Binärer Codec/Import, Guard/No-Send, Builder-Probe, Readiness, Verifier, Timing, Binär-Kompatibilität | **129 gezielte PASS, 0 FAIL**, Teile erneut getestet (spätere Berichte ebenfalls) | **LOCAL_PASS** · `STATUS.md` „Current checkpoint“ 17.10.; nicht echter BUY |
| T33 | 16.10. G0.7-Härtung | Runner-Lib, Binary, acceptance guard, g07_no_send, echte G0.8-Binary-Kompatibilität | **21/21 + 20/20 + 6/6 + 10/10 + 2/2 PASS**, unabhängig dokumentierte Regressionen | **LOCAL_PASS** · `STATUS.md` §2a, `g07_no_send.rs`, `acceptance_guard.rs`; **ältere Testversion** |
| T34 | G0.6a DRY-1…DRY-4 | Binding von zwei Wallet-Rollen, Signaturbytes, lokale EVM-Simulation, Economics, Nonces und Null-Egress | **DRY PASS** | **LOCAL_PASS** · `GATE0_TESTPLAN_SHORT.md` und `STATUS.md`; neu qualifizieren bei relevanter Portal-/V5-Änderung |
| T35 | G0.6a Shell-/Secrets-Provenienz | `test_g06a_dry_modes.py`, `test_g06a_key_hygiene.cjs`, `test_g06a_private_mode.cjs`, `test_g06a_shell_scope.py`, `test_g06a_state_pin.cjs` | An `4b0139ee`, je Exit 0: dry_modes **18/18**, shell_scope **7/7**, key_hygiene **3/3**, private_mode **4/4**, state_pin **5/5** (dazu readiness_job 5/5, status_sync 4/4) | **LOCAL_PASS** an `4b0139ee` (2a.3 T56). state_pin war seit der Schalterprüfung des Simulators (`b72bf2ed`) **rot** (FAIL, unbemerkt, da nicht Teil der Cargo-Regression); Test an die Reihenfolge Schalter→Signatur angepasst, BSC-002-Pin-Reihenfolge statisch geprüft |
| T36 | G0.7 Native Readiness | V5-/Nonce-/Gas-/State-/Deployment-/Clock-/Budget-Check, `PASS_READ_ONLY` / `BLOCKED` / `STOP` getrennt | Historische read-only PASS- und aktuelle BLOCKED/STOP-Zustände dokumentiert | **LOCAL_PASS** für Guard-/Status-Differenzierung; **OPEN** für eine aktuelle grüne Mainnet-Readiness des neuen Versuchs · `STATUS.md` |
| T37 | G0.8 Verifier gegen externe Historie | `g08_binary_records`, `g08_reads_g07_evidence`, `provenance_clock`, `js_dbrec_parity` | Testgruppen **im Repository vorhanden**, G0.8-Archiv-Qualifikation am 27.10. im STATUS `QUALIFIED` | **LOCAL_PASS** für dokumentierte interne Regressionen, **OPEN** für konkreten G0.7-Echtdurchlauf; einmalig `--ignored`-Real-RPC-Test ist **nicht** Teil des pauschalen 1.339-PASS |
| T38 | Gateway-Commands / Source-Modes | Typisierte `bsc.readiness.start/stop`, Candidate und `bsc.source.mode.set`; Generation, Validierung, request/effective, Refusal | Bestandteil Gateway-74 und BSC-E2E, eigene Rust-Tests in `gateway/src/control.rs` vorhanden | **LOCAL_PASS / INTEGRATION_PASS** (bestehender Control-Scope) · Wallet-/Economy-/Venue-/Runtime-/Limit-Commands (`bsc.wallet.set`, `bsc.economic.set`, `bsc.venue.mode.set`, `bsc.runtime.mode.set`, `bsc.attempt.limit.set`): **LOCAL_PASS / INTEGRATION_PASS** an `4b0139ee` (2a.3) |
| T39 | BSC Frontend-QA | BSC Status/Feldrendering, Readiness-/Candidate-/Source-Reiter und No-Polling-Integration | Bestandteil Frontend-113 / E2E und F0-VAR, historisch bestanden | **LOCAL_PASS / INTEGRATION_PASS**; Wallet-/Wirtschaftsschalter, Runtime-Modus und Tageslimit in den fünf Reitern: **INTEGRATION_PASS** an `4b0139ee` (2a.3) |
| T40 | Phase-Gates C2, C3, D, E | Native-State/Reorg/Recovery, C3 PAPER, Flap-Decoder, Four-Decoder | C2 PASS `f427cfa1`, C3 `cb9813c6`, D `d01ebd44`, E `531ddfcc` | **REVIEW_PASS** für eingefrorene technische Scopes · `STATUS.md` Pakete 4/5; Four verifiziertes ABI für Live weiter OPEN |
| T41 | Phase-Gates F, G, H, I | Journal/Replay, Economics, Strategy SHADOW, PAPER/Transport | F PASS `88cc92b6`; G PASS `5e862699`; H PASS `2d4c5503`; I PASS `3b6275e9` + `bfc239e6` | **REVIEW_PASS** für technische Scopes · `STATUS.md` Pakete 6/7; externe LIVE-Voraussetzungen offen |
| T42 | Phase F native Capture und Journal | Read-only WSS-Replay, Segmentierung, Recovery, Qualitätsklassifikation | E2E laut STATUS `baf5f0fa` PASS; Raw Evidence unter `bsc/native/evidence/f/live_capture_1793023912198/` | **INTEGRATION_PASS** (beobachteter Scope), **EXTERNAL_BLOCKED** Mainnet-geth/30-min native-Capture |
| T43 | Phase H 60-min Strategy SHADOW | Marktdaten, reproduzierbare Strategy-Entscheidungen | **201/201 Replay-Ausgaben PASS**, 192 Launches; 3 geschlossene PAPER-Positionen, **3/3 Verlust** | **LOCAL_PASS** Replay / **FAIL** Gewinn-Scope · `h/live_strategy_1793049942243/README.md` |
| T44 | G0.6 gleichzeitige PAPER-Launches, `BSC_C3_LATENCY_BURST` | k=1,2,4,8 künstlich gleichzeitige Launches, kein echter BUY | p99 letzte Position 258/521/1020/2020 µs; 4er- und 8er-Schwelle verletzt | **LOCAL_PASS** reproduzierbarer Test / **FAIL** synthetisches Ziel bei k≥4 · `g06/README.md` |
| T45 | P8 CPU/RSS 60 min | Read-only SHADOW Release-Runtime 720 Samples | Ø16,3 % eines Kerns; RSS 148→451 MB | **LOCAL_PASS** Beobachtung / **OPEN** echte Dauerlauf- und Produktionshardwarequalifikation · `p8/perf_runtime_60min_20261027/README.md` |
| T46 | BSC P8 Runtime Restart + Browser-/Journal-Verkettung | Launchd restart / neuer Run / C2 RESTORED / Strategy-Replay nach Browser-E2E | PASS dokumentiert am Release-SHA | **INTEGRATION_PASS** · `STATUS.md` + `ops/tests/bsc_control_test.py` |
| T47 | Anschließend berichtete zwei rote Browser-E2E-Läufe | Fehlerort und Fehlerlogs derzeit **nicht** dem Testregister zugeordnet | **nicht nachgewiesen behoben** | **OPEN** (Ursache nicht reproduziert). Fehlerort belegt: beide Läufe 27.10. ≈09:00Z (`/private/tmp/darkbot-test.xpf4oc0o`, `.m7mzlvfz`, nicht versioniert) endeten **vor dem Browser** (keine Browser-Artefakte) im `--ignored`-G0.8-Echt-RPC-Vorschritt, zeitgleich mit PREPARE (08:27–10:12Z, 1,52 Mio. Header vom selben öffentlichen Endpunkt); Original-Fehlertext nicht gesichert. Seitdem Release-E2E PASS an `c2e1f7da` und `4b0139ee`; der Verifier protokolliert jetzt curl-Exitcode und Ursache |
| T48 | Neue Wallet- und wirtschaftliche Enable-/Disable-Controls + Tageslimit | Auftragspaket 27.10. (`b72bf2ed`…`4b0139ee`) | Gesamtabnahme auf finalem SHA `4b0139ee`, Einzelzeilen T50–T60 in 2a.3 | **LOCAL_PASS / INTEGRATION_PASS / RELEASE_PASS / REVIEW_PASS** an `4b0139ee`; realer Mainnet-Scope bleibt OPEN (O09) |
| T49 | **Echter G0.7/Mainnet-BUY, G0.8-Akquisition, reales SELL, Profit und T0-Witness** | Wirtschaftlicher Produktionsnachweis | **kein Mainnet-BUY / keine echte Verkaufskette / kein T0-Witness** | **NOT_RUN / EXTERNAL_BLOCKED**, keinesfalls durch lokale PASS-Zahlen ersetzen |

**Kontrollsumme und Nicht-Doppelzählung:** T01 (**1.339**) umfasst die Rust-Suites seines spezifischen Commits; die älteren 426er/479er, G0.7-Guard-Suiten und C1-Fokus-Tests sind überwiegend **wiederholte/überschneidende Testfälle anderer Revisionen**. Sie dürfen **nicht** zu „mehreren Tausend einzigartigen Tests“ addiert werden. T02 (**74 Gateway**) und T03 (**113 Frontend**) sind zusätzliche getrennte Suiten. T04/T05 sind **drei ganze Browser-Durchläufe**, nicht drei einzelne Assertionen. T06 sind **2.603 Datenfelder**, keine 2.603 Tests. `ignored`, `SKIP`, `NOT_RUN`, fehlende Logs und bisherige negative Testfälle bleiben ausdrücklich sichtbar.

**Was noch fehlt, um dieses Testinventar als wirklich vollständig und aktuell abzunehmen:** Claude muss für die *neuesten* Quelländerungen alle vorhandenen lokalen Test- und CI-Ausgaben einschließlich der 22 `ignored`, externen RPC-Smokes, separaten Python-/Node-Tests, Native-/Gateway-/Frontend-E2E und neuen Wallet-/Limit-Controls gegen den **gleichen finalen Git- und Release-SHA** abgleichen. Pro Testfamilie exakt `command, cwd, exit_code, count_pass, count_fail, count_ignored, run_id, git_sha, binary_sha, config_sha, evidence_path, status` aufnehmen. Auf dem neueren Dokumentations-HEAD ohne solche Läufe **kein automatisch geerbter PASS**.

### 2a.3 Abnahme Wallet-/Wirtschaftscontrols und Tageslimit — finaler SHA `4b0139ee` (28.10.2026, Host M4 Max)

Code `b72bf2ed`…`4b0139ee` (Owner `sources.rs`, Projektion, `darkbot.bsc.v1`, Gateway, CLI, fünf Reiter, Durchsetzung in G0.7-Runner/DRY-Signer/DRY-Skript/DRY-Simulator/Builder-Probe/Q5A-Probe). Alle Läufe am selben SHA, ohne Mainnet-Aktion; Evidence `bsc/native/evidence/controls/gate_4b0139ee_20261028/` (Logs, Release-Manifest, Exitcodes).

| Test-ID | Befehl (cwd) | Exit | Ergebnis | Status |
|---|---|---|---|---|
| T50 | `cargo test --offline --no-fail-fast --workspace --all-features` (`bsc/engine`) | 0 | **86 Suites, 1.374 PASS, 0 FAIL, 22 ignored** (= NOT_RUN) | **LOCAL_PASS** |
| T51 | `cargo clippy --offline -p darkbot-bsc-execution -p darkbot-bsc-qualification --all-features --all-targets -- -D warnings` (`bsc/engine`) | 0 | geänderte Crates, alle Features und Targets, keine Warnung | **LOCAL_PASS** |
| T52 | gezielt: `sources::tests` (`--features runtime`), Projektion (`--features status-reporter`) | 0 | **27/27**, **4/4**; neue Tests `a_live_reason_*`, `a_replay_as_an_unchanged_source_*` schlagen ohne Fix fehl (geprüft) | **LOCAL_PASS** |
| T53 | `cargo test --offline` (`gateway`) | 0 | **8 Suites, 75 PASS, 0 FAIL, 1 ignored** (frühere STATUS-Angabe „53“ zählte nur die Control-Tests) | **LOCAL_PASS** |
| T54 | `npx vitest run` (`frontend`) | 0 | **29 Dateien, 124/124** | **LOCAL_PASS** |
| T55 | `node frontend/scripts/check-bsc-fields.mjs` (F0-VAR) | 0 | **2.765 native + 116 Proto-Felder platziert** (IST_CONNECTED 2.133, FUTURE_PHASE_BLOCKED 36, EXCLUDED 596, IST_BUG 0, SOLL_MISSING 0) | **INTEGRATION_PASS** |
| T56 | `bsc/tools`: 5 G0.6a-Tests (T35) + `test_readiness_job.py` + `test_status_sync.py` | 0 | 18+7+3+4+5+5+4 = **46/46** | **LOCAL_PASS** |
| T57 | `ops/darkbotctl build` + `python3 ops/release_manifest.py` | 0 / 0 | `Coordinated release manifest: PASS`; Schema-ID `5e1a11c2…`, `bsc_runtime` `b3660e8b…`, `bsc-status-reporter` `64013eea…`, `g07_pre_live_readiness` `c1ba0412…`, Gateway `ed9454c1…` | **RELEASE_PASS** |
| T58 | `DARKBOT_E2E_PROFILE=release python3 ops/tests/bsc_control_test.py` | 0 | Browser-Kette PASS: Wallets LAUNCHER/BUYER und SEND/LIVE ENABLE→DISABLE per GUI und CLI mit nativer Prüfung auf demselben Zustand, fremdes HOME verweigert, veraltete CAS/falscher Versionstyp verweigert, Venues, Runtime PAPER, Tageslimit 3→0, fünf Reiter (Desktop + 390 px), kein Polling, kein Send; Crash-Neustart: ENABLED → DISABLED (`RESTART_RESET`) | **INTEGRATION_PASS** |
| T59 | Unabhängiger Reviewer (ein Reviewer, 4 Durchgänge) | – | Pass 1 `86de25c9` **FAIL** (M1: Schalter über `$HOME` umlenkbar) → Pass 2 `a8fa15b4` PASS (4 LOW) → Pass 3 `e63f7dcc` **FAIL** (MEDIUM: F0-VAR-Pins nicht erneuert) → Pass 4 `4b0139ee` **PASS** | **REVIEW_PASS** |
| T60 | Realer Owner-Zustand nach dem Gate (`darkbotctl bsc source show`) | 0 | Generation 0 (kein Zustand angelegt): alle sieben Schalter DISABLED, Quellen/Komponenten/Venues OFF, `BSC_RUNTIME_MODE` OFF, Tageslimit 0; `g06a_dry_handoff controls --require` aller sechs Versuchsschalter → `CONTROL_DISABLED` | **PASS** (Default) |

**Offene LOW (dokumentiert, fail-closed):** Linux-Home-Lookup nur `/etc/passwd` (vor Linux-Zielhost `getent`); `test_g06a_state_pin.cjs` übt BSC-002 bei DISABLED-Schaltern nur statisch/Unit; ein verlorener Receipt, mit fremder Nutzlast als andere Quelle/Komponente im selben Modus wiederholt, ist nicht unterscheidbar (Gateway-UUIDs). Nicht ausgeführt: Pixeldiff, 22 ignored, kampagnenweiter 10er-Zähler (O13).

---

**Präzisierung 27.10.:** Die verbindliche aktuelle Nutzerentscheidung lautet **maximal 10 echte BUY-Versuche insgesamt (1 G0.7 + höchstens 9 weitere), nicht 1+10**; jeweils kleinster reproduzierbar qualifizierter BUY-Betrag und kumulatives Verlustbudget. `BSC_BUY_ATTEMPTS_PER_DAY` ist ein tägliches, **kein kampagnenweites** Limit. G0.7-Self-BUY und tatsächlicher LIVE-SELL sind unterschiedliche Fähigkeiten; Strategy-C-Exit und signierter PAPER-SELL beweisen keinen realen Exit. Details im neuesten Abschnitt von `PHASENPLAN.md`.

## 3. Fehlende Messungen – verbindliche PASS/FAIL/OPEN-Matrix

| ID | Konkrete Messung / Daten je Versuch | Ziel / PASS-Kriterium | Stand | Was genau noch gemacht werden muss |
|---|---|---|---|---|
| O01 | **Wallets und wirtschaftliche Controls** LAUNCHER/BUYER getrennt, persistiertes `DISABLED`, alle CLI-/UI-/Runner-Wege | Beide default DISABLED; bei deaktivierter zuständiger Wallet nachweislich **0 reale SIGN/SEND** auf jedem nativen Pfad, auch Restart/Race; Status und Receipt gleich in CLI und UI | **PASS** (lokal/Integration/Release, finaler SHA `4b0139ee`, 2a.3: Owner-/Projektions-/Gateway-/Prozess-/DRY-/Browser-E2E inkl. Restart/Race/Crash und fremdem HOME; Reviewer Pass 4 PASS); realer Mainnet-Scope **OPEN** | Nur noch realer Scope: bei G0.7 Schalterstand je Versuch im Pro-Versuch-Record (§5) |
| O02 | **Tageslimit/Versuchszähler** pro wirtschaftlichem BUY-Versuch | Konfigurierbares Maximum (Beispiel **10/Tag**), Tagesgrenze und Zeitzone fest, jeder RELEASED/UNKNOWN-Versuch zählt, CAS/Crash kein Double Count, Limit=0 verweigert | **PASS** (lokal/Integration/Release an `4b0139ee`: `BSC_BUY_ATTEMPTS_PER_DAY`, UTC-Tag, create-new-Slots, Retry/Neustart/Race/Tageswechsel-Tests, Attempt-IDs = G0.7-Regel, CLI+GUI-E2E 3→0); realer Scope **OPEN**; kampagnenweiter 10er-Zähler siehe O13 | Ein Maximum ist niemals eine automatische Order |
| O03 | **Aktueller Flap/Four-Contract-Freeze** Chain-ID, Portal, ABI, Version, Runtime-Code, Selector, Tokenableitung | Identische überprüfte aktuelle Implementierung vor qualifizierender Signatur/BUY; Drift → STOP | **OPEN** | Flap v5.27.2 und fehlende Four-Quellen tatsächlich verifizieren; keine alte 5.24/5.27.0-Evidence als aktuell ausgeben |
| O04 | **Fremder Launch→BUY vollständiger Pfad** pro Venue/Quote/Creator-/Taxprofil | Für jeden gewählten Scope full signed Launch → validierte lokale Tokenadresse → gültiges BUY-Calldata/Quote → korrekter eigener pre-block BUY-Plan | **OPEN** | Nach 0/28-Miss qualifiziertes reales Fremdprofil auswählen; reproduzierbar G0.2/G0.3 + Replay + Status-/Reason-Codes nachweisen, ohne unqualifizierte Profile generell zuzulassen |
| O05 | **Native WebSocket-Früherkennung / Ingress-Quellenvergleich** pro identischer signierter Launch-TX | Die **tatsächlich gestartete Release-`bsc_runtime`** messen, nicht allein das historische Terminal-Werkzeug `bsc/tools/br0/wss_race.mjs`: je Quelle/identischer TX-Hash `first_hash` (nur Ankündigung), `first_full` (vollständige signierte TX kryptografisch verifiziert), `first_actionable` (Launch/Token/BUY-Parameter für den gewählten Scope tatsächlich verwertbar) und `T_local_head` (lokaler Blockempfang); Quellabstände und `T_local_head − T_first_*` in ms, auch negative/fehlende Werte; Chain/Venue, Uhrenbasis, Quelle verbunden %, Drops, Fehler, Reconnects, Full-/Hash-only-Anteile und überprüfbare Nenner. **Lokaler Blockempfang ist nicht originales Producer-`T0`.** | **OPEN** für den vollständigen produktiven Codepfad; bisherige Terminal-/SHADOW-Beobachtungen sind nur getrennte Teilbelege (22.10.: 116 Launches, 48 vor lokalem Head gesehen) | Zunächst vorhandene native Journale/Census und frühere Terminal-Evidence abgleichen, fehlende Zeitfelder nötigenfalls im bestehenden Rust-Ingress/Journal ergänzen und mit Mock-WSS/Replay testen. **Nur falls nötig** ergänzende read-only Live-Stichprobe von **maximal 10 Minuten**, ohne Wartepflicht vor SIGN-DRY/V5/G0.7. `HASH_ONLY` niemals als BUY-ready zählen; nicht verifizierbare/fremdprofilbedingt unbrauchbare Launches getrennt ausweisen. |
| O06 | **Native WebSocket→BUY-Latenz** `T_hash→T_full→T_actionable→T_ready→T_queue→T_send` | Für **dieselbe signierte Launch-TX, Candidate-/Attempt-ID** an der echten Rust-Runtime messen: WSS-Frame-Eingang → kryptografische Verifikation und Venue-/Token-Erkennung → C2/C3/Strategieentscheidung/Quote → BUY-Bereitstellung/Signatur → Queue/Adapter → tatsächlich nachweisbare Send-Grenze. Pro Abschnitt monotone Clock/Prozessgrenze, p50/p95/p99/max, Anzahl gültiger Fälle/Fehler, und `T_local_head`-bezogenen verfügbaren Vorsprung nennen; Prebuild/Presign getrennt. Bestehendes lokales Ziel **<1 ms**, bevorzugt **<0,5 ms bis echtem `T_send`**; Werte ≥1 ms untersuchen. **PAPER-Sink, simulierter Send und Builder-ACK sind kein echter `T_send`/on-chain `T_buy`**; physisches `T_buy−T0` bleibt ohne Original-Producer-Witness UNKNOWN. | **OPEN** für vollständige native WebSocket→BUY→echte Send-Grenze; lokale HOT0-/C3-/PAPER-Messungen sind keine Ende-zu-Ende-Live-Latenz | Vor wirtschaftlicher Freigabe deterministischer produktiver Rust-Aufrufpfad mit lokalem Mock-WSS, Replay und nicht ausführbarem Transport prüfen (Full-/Hash-only-/Duplicate-/Late-/Disconnect-/Reorg-Fälle); optional kurze read-only Live-Stichprobe **höchstens 10 Minuten** nur für Früherkennung. Echtes `T_send` erst bei separater Autorisierung und qualifiziertem Adapter ausweisen; **kein neuer Langzeittest und keine Sperre für SIGN-DRY/V5/G0.7**. |
| O07 | **Echte Konkurrenzposition** Launch-N und eigenes BUY-N+1 | **Launch und eigener erfolgreicher BUY im selben kanonischen Block**, BUY-Transaktionsindex = Launchindex+1 und **0 erfolgreiche fremde BUYs dazwischen** im exakt qualifizierten Self-Launch-Scope | **OPEN** | Nach kontrolliertem G0.7 kanonische Receipts + vollständige Block-TXs aus G0.8, Reorg/fehlende Daten ⇒ UNRESOLVED, nicht PASS |
| O08 | **Physische Ursprungslatenz `T_buy−T0`** | **<1.000.000 ns** ausschließlich mit unabhängig qualifiziertem Original-Producer-/Builder-/Validator-Execution-Witness + Unsicherheit | **OPEN / NOT_PROVEN** | Originalzeitzeuge qualifizieren. N+1, Block-Header, lokaler Head, ACK und Papiermessung beweisen es nicht; G0.8 Erwerbs-PASS bleibt davon separat möglich |
| O09 | **Erster echter G0.7 Mainnet-BUY** (eigener Launch) | Eine exakt freigegebene Self-Launch+BUY-Identität, signierte Bytes/Nonces/Builder/Budget korrekt, kanonischer Receipt erfolgreich, intendierter Token-Balance-Delta **>0**, zurechenbare tatsächliche Zahlung | **OPEN** | Vorher G0.1 aktuelle Identität, DRY/H2/V5/Readiness, Funding/Approvals, einmalige Attempt-Authority. Erst nach separater Nutzerentscheidung auf Mainnet ausführen, kein automatischer Retry |
| O10 | **G0.8 Kostenrechnung und technische Erwerbsentscheidung** | Tatsächlich belasteter BUY-Wert, Gas used × effektiver Gaspreis, Launchgas, tips/builder fees/tax und Gesamtsumme; für originalen G0.7 Scope **max. 0,0004811635 BNB** vertraglicher Gesamthöchstbetrag, wenn exakt dieser V5 weiter gilt | **OPEN** | Canonical-Receipts, Token-Balance vor/nach, Builder-Evidence, `chain_outcome` getrennt von `evidence_integrity`; geänderte Werte neu qualifizieren |
| O11 | **Realer SELL/Exit** zu genau der gekauften Positionsidentität | Tatsächlich erfolgreicher SELL/Exit, realer Rückfluss, Token-Abfluss, Revert/Tax/Approval-/Verkaufsgas vollständig erfasst; Position anschließend wirklich geschlossen | **OPEN** | Realen SELL-Pfad/qualifizierte Bedingungen und Walletfreigaben getrennt prüfen. G0.7 Self-Launch-BUY setzt **keinen** erfolgreichen späteren SELL voraus und beweist ihn nicht |
| O12 | **Realisierter Netto-PnL eines vollständigen BUY→SELL** | `Netto-PnL = real erhaltener Erlös − tatsächlich investiertes Kapital − sämtliche BUY-/SELL-/Approval-/Builder-/Gas-/Tax-Kosten`; **>0** nur für einen einzelnen wirtschaftlich positiven Trade; ebenfalls Verluste aufzeichnen | **OPEN** | Nach qualifiziertem realen BUY und SELL mit Chain-Balance-/Fee-Belegen messen. Negative Trades zählen, nie herausfiltern; 3 PAPER-Verluste bleiben separate historisch negative Evidenz |
| O13 | **Maximal 10 echte BUY-Versuche insgesamt einschließlich G0.7** | Je Versuch kleinster aktuell reproduzierbar qualifizierter BUY-Input, separate Freigabe, eigene Kennung und Evidence; **Erfolgsquote = kanonisch erfolgreiche Token-BUYs / alle gestarteten wirtschaftlichen BUY-Versuche**; p50/p95/max lokale Latenz und tatsächliche Position/Kosten; FAIL/UNRESOLVED zählen | **OPEN** (kampagnenweite 10er-Obergrenze noch nicht gesondert nativ nachgewiesen; Tageslimit ist nicht Gesamtlimit) | Nach max. 10 STOP, auch tagübergreifend. Vor Versuch 1 kumulatives Verlust-/Gas-/Builder-/Exitbudget qualifizieren; kein Auto-Retry; nach jedem Versuch auswerten, bei Fehlbefund STOP; SELL nur sofern echter Exit getrennt qualifiziert ist |
| O14 | **Konkurrenz gegen fremde Launches** (spätere, getrennte Scope-Qualifikation) | Bei wirklich fremden Launches **frühester zulässiger externer BUY**; Quote-, Creator-/Presale-/Tax- und Tokenadressprofil jeweils belegt; keiner vor unserem erfolgreichen BUY, falls Ziel N+1 | **OPEN** | Erst nach qualifiziertem Fremdlaunch-Profil+Adapter und kontrollierter Autorisierung; eigener G0.7 Self-Launch-PASS genügt nicht |
| O15 | **Echte Transport-/Builder-Fähigkeit und Abhängigkeit** | Passender inclusion-adapter überträgt korrekte Payload, Fristen/Flags umgesetzt, nachweisbare kanonische Aufnahme, tatsächliche Reihenfolge und Kosten; Failures/Provider-Rejections separat | **OPEN** | G0.5/G0.8/G0.9 erneut aktuell gegen genutzte Dienste qualifizieren, lokale geth-Handoff-Messung nur bei vorhandener Infrastruktur; kein erfundener 48Club-Inclusion-PASS |
| O16 | **Crash/Restart bei offenen Mainnet-Intents** | Kein Duplicate-SEND, falscher Nonce-Reuse, verlorener bereits wirtschaftlicher Versuch, unautorisierter Recovery-Release oder falscher PASS; `UNKNOWN` bleibt ehrlich unbekannt | **OPEN** | Produktionsnahe Fehler-Injektionen mit nichtwertführenden Testidentitäten, CAS-/Latch-/Journal-/Provider-Antwortverlust testen; keine live Probe zum Erzeugen des Fehlers |
| O17 | **Release-/Frontend-E2E auf finalem SHA** | Alle Pflichtprüfungen PASS, sämtliche wirtschaftlichen Controls, Wallets, Status-Gates und G0.8-`UNRESOLVED` identisch durch Native→Proto→Gateway→GUI | **PASS** auf finalem SHA `4b0139ee` (2a.3: Release-Build + Manifest, BSC-Regression 86 Suites/1.374/0 FAIL, Gateway 75, Vitest 124, F0-VAR, Browser-E2E release); rote E2E vom 27.10. Vormittag: Fehlerort vor dem Browser belegt, Ursache **OPEN** (T47) | Nur T47-Ursache (nicht reproduziert) |
| O18 | **Betriebsdatenqualität und Capture** | Pro Versuch lückenlose 15-s- bzw. vertraglich gebundene Capture, segmentierte Journale/Hashes/Provenienz und G0.8-Canonical-Reads; Lücke ⇒ INCOMPLETE/UNRESOLVED | **OPEN** (echter Versuch) | Snapshot-Run-ID, Release-/Config-SHA, Source-Quality, Gap/Drop/Reorg, Chain-Receipts, UTC/monotonen Clock-Kontext, Original Evidence und Fehlversuche versiegeln |
| O19 | **Kosten/Netz-/Hardware-Entscheidung** | Vergleich kostenfreier Ingress-/Inclusion-Wege gegen tatsächliches `T_send`, Datenqualität und Kosten; keine fixe Kaufpflicht für Hardware | **OPEN** | Nur bei belegter Notwendigkeit Mainnet-geth/Provider/Linux qualifizieren; Datum und Kosten getrennt vom BUY-Erfolgsverdikt, vor Ausgaben Nutzerentscheidung |
| O20 | **Gesamtwirtschaftlicher Strategie-Nachweis** | Positive *realisiert-netto* Ergebnisse unter dokumentierter Stichprobe, alle erfolgreichen/gescheiterten/unklaren Trades und reale Kosten. Numerischer Profit-/Max-Drawdown-Grenzwert noch **nicht verbindlich festgelegt** | **OPEN** | Erst nach kontrollierten BUY→SELL-Ergebnissen Strategie anhand echter Nettodaten bewerten; aus drei historischen negativen PAPER-Positionen keinen Produktions-PASS konstruieren |

### 3a. Verbindliche technische Vor-BUY-Abnahme: fünf Quellen einzeln, dann gesamte produktive Kette (Entscheidung 28.10.2026)

**Vor dem ersten echten Mainnet-BUY sind alle ohne Geld ausführbaren Code-, Quellen-, Timing-, E2E- und Negativtests des tatsächlich verwendeten BUY-Pfads durchzuführen und in O04–O06, O15/O16/O18 nachzuweisen.** Dies gilt für die jeweils wirklich verwendete Quelle, Venue, Strategie und den jeweiligen G0.7-Self-/späteren Fremdlaunch-Scope. Bestehende grüne Teiltests auf exakt derselben Code-/Binary-/Config-Identität wiederverwenden; keine neue Bot-Architektur und keinen zweiten Parallel-Runner bauen. Fehlende *nicht benutzte* externe Quellen blockieren nicht pauschal einen technisch anders qualifizierten G0.7-Self-Launch; sie erhalten ein eigenes ehrliches `UNQUALIFIED/EXTERNAL_BLOCKED/NOT_AVAILABLE`, niemals PASS. Diese rein technischen Arbeiten können vollständig **vor** einem wirtschaftlichen BUY durchgeführt werden; keine Mainnet-SIGN-/ARM-/SEND-/BUY-Freigabe durch diesen Abschnitt.

**Fünf einzeln schaltbare native Quellen sind nicht fünf verfügbare WebSockets:** `sources.rs::SourceId::ALL` enthält genau die folgenden fünf IDs. Die derzeitige Runtime verbindet WSS aus `ingress.public.env` nur für tatsächlich konfigurierte `PUBLICNODE/DRPC/BLOCKREQ`-Endpunkte. Für jede Quelle eigene Statuszeile, eigener Zähler/Nenner, konkreter Codepfad und separate Urteilsspalte.

| Native Quelle | Tatsächlicher Typ/aktueller Stand | Vor-BUY-Prüfpflicht und zulässiges Ergebnis |
|---|---|---|
| `BSC_INGRESS_PUBLICNODE` | WSS `wss://bsc-rpc.publicnode.com` konfiguriert | Individueller echter **nativer Rust-WSS-SHADOW**-Lauf: TLS, bestätigtes `eth_subscribe`, tatsächlich empfangene HASH/FULL, Signatur-/TX-Prüfung, Launch-Erkennung, source-spezifische Latenz, Ausfälle und Reconnect; PASS nur für beobachtete Fähigkeiten |
| `BSC_INGRESS_DRPC` | WSS `wss://bsc.drpc.org` konfiguriert | Dasselbe **eigenständig**, getrennte Availability und HASH-only-vs-FULL-Quote; kein PASS durch die PublicNode-Daten |
| `BSC_INGRESS_BLOCKREQ` | WSS-Adapter im Rust-Code vorgesehen, im aktuellen `ingress.public.env` **kein Endpunkt** | Native Mock-WSS-/Replay-Tests des Adapters; echter Anbieter-Test nur bei vorhandenem zulässigem, kostenfreiem Endpoint. Ohne ihn `NOT_CONFIGURED/UNQUALIFIED`, nicht PASS |
| `BSC_INGRESS_GETH` | **Kein öffentlicher WSS-Feed dieser Runtime**; eigener lokaler geth-/HOT0-/IPC-Pfad, derzeit nicht als Laufzeitquelle integriert und kein synchronisierter Mainnet-geth qualifiziert | Vor-BUY offline mit HOT0-/IPC-Fixtures/Interop prüfen und Integrationslücke ausweisen; echter geth-Quellen-/Latenz-PASS erst nach installiertem, integriertem und qualifiziertem Node. Kein Hardwarekauf ohne eigene Freigabe |
| `BSC_INGRESS_48CLUB` | Private/Auction-Feed **zugangsabhängig und für SHADOW im Owner `SOURCE_ACCESS_UNQUALIFIED`** | Konfigurations-, Fail-closed- und offline Mock-Tests; ohne bestätigten Zugang/Protokoll kein echter Subscription-/Latenz-PASS, keine Anmeldung oder kostenpflichtige Buchung ohne Freigabe |

**„Warm laufen“ ist kein festes Minutenritual:** Zu Laufbeginn pro Quelle getrennt `DNS/TCP/TLS → WSS-Handshake → Subscription angefordert → Subscription bestätigt` mit Zeiten und Fehlercodes protokollieren (**COLD_START**). Erst nach einer nachweislich bestätigten Subscription und stabiler laufender Verbindung dürfen Pending-Ereignisse als **CONNECTED_SUBSCRIBED / STEADY_STATE** verglichen werden; kein vorgegebener 2-/5-Minuten-Vorlauf. Reconnects/Backoff, Re-Subscription, verspätete/ausgebliebene Notifications und eventuelle Warm-up-Effekte gesondert ausweisen, nicht aus der Fehlerquote entfernen. `WSS`-Verbindungen für den Quellenvergleich **parallel** auf demselben Host/Uhrkontext betreiben, **maximal 10 Minuten Gesamtzeit** für eine neu erforderliche read-only Live-Sichtung, **nicht 10 Minuten mal fünf Quellen**. Bestehende Rohdaten zuerst auswerten; null passende Launches bedeutet `NOT_EVALUABLE` für Früh-Erkennungsquote, nicht FAIL/PASS. Keine Messwartepflicht vor lokalem SIGN-DRY, V5-Vorschlag oder Readiness.

**Einzel- und Vergleichsmessung im echten Release-Binary:** pro kanonisch identischer TX und Quell-ID `t_first_hash`, `t_first_verified_full`, `t_first_actionable`, `t_local_head` und bei tatsächlich verwendbarem Profil `t_ready/t_queue/t_adapter_handoff`; Quellzeitdifferenzen für **denselben TX-Hash**, `head-relative`-Vorsprung bzw. negative Verspätung, p50/p95/p99/max, Anzahl und Nenner; Connected-% und nutzbare Abdeckungsquote, Full/HASH/Rejected/Dropped/Rate-limited. `first_actionable` erfordert rekonstruierte signierte Bytes + Signer-/Chain-Validierung + gültiges Launch-/Token-/BUY-Profil, kein bloßes JSON-`to/input`. Einen lokalen Head nicht als physisches Validator-`T0` ausgeben. Ein read-only SHADOW-/PAPER-Handoff ist kein echter `T_send` und kein Kauf.

**Vor-Echtgeld-E2E-Matrix der gesamten Rust-Kette (O04/O06/O15/O16/O18):** `WSS frame → VerifiedTx → Ingress/Dedup/Venue → Token/Quote/Admission/C2/C3 → BUY-Vorbereitung → Nonce/Lane/Queue → PAPER bzw. geblockter/nichtausführbarer Transport`. Mit deterministischen Fixtures/Mock-WSS und aufgezeichnetem Replay mindestens prüfen: korrekte vs manipulierte Signatur/Chain/Selector/Quote/Tax; HASH-only und verspätetes FULL; Full-TX mehrerer Quellen und Quellenrennen; 2/4/8 simultane Launches und Queue-Full/Backpressure; Late/Gap/Disconnect/Reconnect/Rate-limit; Kollision gleicher Tokenadresse (`CONFLICT`, solange produktiv offen kein PASS); Head-Drift/Reorg; Crash/Restart, Idempotenz, Attempt-Limit, Nonce-Reuse/UNKNOWN und kein doppeltes BUY; BUY-ready- und Adapter-Handoff-Messpunkte; Evidence-Vollständigkeit sowie abgesicherter SELL/Exit im bereits vorhandenen, nur simulierten Scope. Die *echte* Sende-/Inclusion-Grenze und der reale SELL bleiben getrennt OPEN bis ihre eigenen autorisierten Nachweise vorliegen.

**Abnahme und Reihenfolge:** In den bestehenden PASS/FAIL/OPEN-Zeilen pro Quelle und E2E-Test `source_id, endpoint_class, requested/effective, connected/subscribed, cold_start_ms, steady_state, capability(HASH_ONLY/VERIFIED_FULL/ACTIONABLE), samples, drops/errors, first/lead/delta p50/p95/p99/max, git_sha, release_binary_sha, config_sha, raw_evidence, verdict` dokumentieren; unabhängiger Review des finalen Codes. Fehlende, unqualifizierte oder nicht beobachtbare Quellen explizit lassen, **keine erfundenen fünf grünen WebSockets**. Die tatsächlich verwendete Kette muss ihre Offline-Sicherheits-/Korrekheitsnachweise vor dem separaten echten G0.7-BUY erfüllen; Ergebnisse anderer späterer Fremdlaunch-/Produktions-Scope bleiben OPEN. Keine neuen Langzeitläufe oder Kosten, kein Blockieren der parallel vorbereiteten lokalen SIGN-DRY/V5-/Readiness-Arbeit.

### 3b. Verbindliche logische Schalter-Kombinationsmatrix (28.10.2026) — Pflicht VOR dem ersten Echtgeld-BUY

**Abnahmegrenze:** Nach vollständiger LIVE-Codeimplementierung, aber vor **S4/G0.7 (erstem realem Mainnet-BUY)** sind die für den tatsächlich genutzten Self-Launch-, Wallet-, Transport- und Guard-Scope zutreffenden Kombinationen **auszuführen und mit PASS zu belegen**, nicht nur als Liste abzuhaken. Bereits gleichwertige Altbelege auf exakt passendem Git-/Binary-/Config-Stand referenzieren statt Tests nutzlos zu wiederholen. Bei einem für den gewählten G0.7-Pfad relevanten FAIL/OPEN bleibt der Geldversuch gesperrt. G0.7-SIGN-DRY, V5-Vorschlag und Readiness dürfen parallel vorbereitet werden; eine neue G0.4-Wartezeit ist nicht verlangt. **Nur Offline-/SHADOW-/PAPER-Tests mit synthetischen Konten, Mock-WS/RPC und garantiert nicht sendendem Transport.** Die echten, finanzierten Operator-Wallets und ihre Schalter sind für diese Tests tabu. Kein Home-Bypass; die kanonische OS-Home-Regel bleibt erhalten.

**Inventar: 20 native Controls, Autorität im Rust-Owner.** Die verbindlichen IDs und zulässigen Modi stammen ausschließlich aus bsc/engine/crates/execution/src/sources.rs (SourceId, ComponentId, VenueId, SwitchId, Entity::accepts/effective). protocol/schemas/darkbot/bsc/v1/status.proto, Gateway und Frontend sind Projektionen, nicht die Freigabeinstanz.

| Bereich | Native IDs (Suffix zu BSC_) | Anzahl | Modi |
|---|---|---:|---|
| Ingress | INGRESS_GETH, INGRESS_PUBLICNODE, INGRESS_DRPC, INGRESS_BLOCKREQ, INGRESS_48CLUB | 5 | OFF/SHADOW/LIVE |
| Komponenten | STATE_C2, EXEC, STRATEGY_C | 3 | OFF/SHADOW/LIVE |
| Venues | VENUE_FLAP, VENUE_FOUR_CLASSIC, VENUE_OPENFOUR | 3 | OFF/SHADOW/LIVE |
| Wallets | WALLET_LAUNCHER, WALLET_BUYER | 2 | DISABLED/ENABLED |
| Wirtschaft | ECON_SIGN, ECON_ARM, ECON_SEND, ECON_ATTEMPT, ECON_LIVE | 5 | DISABLED/ENABLED |
| Runtime | RUNTIME_MODE | 1 | OFF/SHADOW/PAPER/LIVE |
| Tageslimit | BUY_ATTEMPTS_PER_DAY | 1 | nichtnegative Ganzzahl; 0 = kein BUY |
| **Summe** | | **20** | |

**Wichtige Trennung:** Der G0.7-Self-Launch-Runner prüft sechs Pflichtschalter (WALLET_LAUNCHER, WALLET_BUYER, ECON_SIGN, ECON_ARM, ECON_SEND, ECON_ATTEMPT) und zusätzlich Tageslimit, genehmigten V5-/Guard-/Attempt-/Budget-/Nonce-/Builder-Zustand. Ein Wallet-ENABLE alleine kann keinen Echtgeld-BUY auslösen. Der allgemein gewählte RUNTIME_MODE LIVE sowie Source-/Component-/Venue-LIVE bleiben derzeit **auch bei ECON_LIVE ENABLED BLOCKED**, weil der Native-Owner keinen operativen J-/Bot-LIVE-Executor autorisiert. G0.7-BUY und künftiger automatischer LIVE-Betrieb sind getrennt; der Test bestätigt die Sperre, nicht eine fiktive Freigabe.

#### A. Fünf Ingress-Quellen und Umschaltung

S = SHADOW, 0 = OFF. Nur PublicNode und dRPC haben im aktuellen Standardprofil echte WSS-Endpoints. BlockReq ist nicht konfiguriert, Geth ist kein angebundener öffentlicher WSS-Adapter, 48Club ist ohne Feed-Zugang unqualifiziert. Keine fünf grünen WebSockets erfinden.

| ID | Schalterkombination / Fehlerereignis | Verbindliche Erwartung |
|---|---|---|
| A01 | Alle fünf Ingress OFF | Keine Source-WSS-Adapter; kanonischer Follower separat bewerten |
| A02 | Nur PUBLICNODE SHADOW | Nur PublicNode verbunden/subscribed; eigene Ereignisse und Health |
| A03 | Nur DRPC SHADOW | Nur dRPC verbunden/subscribed; eigene Ereignisse und Health |
| A04 | PUBLICNODE + DRPC SHADOW | Beide parallel, gemeinsamer TX-Hash/Clock-Kontext; getrennte Quellzahlen |
| A05 | PublicNode Disconnect, dRPC SHADOW | dRPC unbeeinflusst; PublicNode-Fehler und Recovery erfasst |
| A06 | dRPC Disconnect, PublicNode SHADOW | PublicNode unbeeinflusst; dRPC-Fehler und Recovery erfasst |
| A07 | Beide Down/Rate-limit | Health/Drops ehrlich; keine erfundene volle oder handelbare TX |
| A08 | Früher HASH, spätere verifizierte FULL einer anderen Quelle | FULL gewinnt für actionable; HASH niemals BUY-ready |
| A09 | Identische FULL-TX von beiden Quellen | Nur ein Kandidat, korrekte erste/spätere Zeit, kein Double-BUY |
| A10 | BLOCKREQ SHADOW ohne Endpoint | NOT_CONFIGURED/UNQUALIFIED statt PASS |
| A11 | BLOCKREQ SHADOW mit isoliertem Mock-WSS | Adapter-Handshake/Subscribe/Disconnect/Recovery; kein Live-Provider-PASS |
| A12 | GETH SHADOW ohne native angebundene Quelle | Fehlender IPC-/geth-Adapter als UNQUALIFIED; kein fiktiver Runtime-Empfang |
| A13 | 48CLUB SHADOW ohne Providerberechtigung | BLOCKED SOURCE_ACCESS_UNQUALIFIED, kein Providerkauf |
| A14 | Jede der fünf Quellen einzeln LIVE angefordert | Immer BLOCKED mit richtigem LIVE-Grund, auch bei ECON_LIVE ENABLED |
| A15 | PUBLICNODE und DRPC einzeln OFF → SHADOW → OFF → SHADOW | Start/Stop/Re-Subscribe; keine veraltete Generation, kein Zombie-Adapter |
| A16 | Kaputte FULL/Signer, Queue voll, Disconnect, Reconnect, Rate-Limit | Ablehnung/Backpressure/Drops erfasst, keine falsche Actionability |

Für Quellen-Stabilität und reale Verzögerung gilt §3a: Vorhandene Daten zuerst, optional höchstens **10 Minuten insgesamt** auf demselben Host, parallel für verfügbare Quellen und ohne feste Warm-up-Wartezeit. Es zählt erst die **bestätigte Subscription**; Cold-start, Steady-State und Reconnect getrennt. Keine Wartepflicht vor SIGN-DRY/V5.

#### B. Runtime, C2/C3/Strategie und drei Venues

In jeder Zeile sind nicht genannte Komponenten/Venues OFF, sofern sie nicht ausdrücklich vom referenzierten Zustand übernommen werden. PAPER läuft nur bei tatsächlich qualifiziertem Papierkonto, zutreffendem Portal/Terms und C2/Nonce. Die Tests dürfen keine unqualifizierten Economics als PASS ausgeben.

| ID | Fachlich sinnvolle Komponentenkombination | Verbindliche Erwartung |
|---|---|---|
| B01 | Runtime OFF, alle Quellen/Komponenten/Venues OFF | Keine PAPER-Strategie, kein PAPER-BUY |
| B02 | Runtime OFF, PUBLICNODE SHADOW, C2 SHADOW | Ingress/C2 dürfen lesen; weder C3 noch Strategie-BUY |
| B03 | Runtime SHADOW, FLAP SHADOW, STRATEGY_C SHADOW | Strategy-C-PAPER-Ledger, kein C3-Sink |
| B04 | Runtime SHADOW, FLAP SHADOW, EXEC SHADOW, C2 SHADOW | C3-PAPER-Handoff bleibt OFF |
| B05 | Runtime PAPER, FLAP SHADOW, EXEC SHADOW, C2 SHADOW, STRATEGY_C OFF | C3 bis PAPER-Sink nur bei qualifiziertem PAPER-Sender/Terms/Nonce |
| B06 | Runtime PAPER, FLAP SHADOW, STRATEGY_C SHADOW, EXEC OFF | Strategie-Ledger, kein C3-BUY |
| B07 | Runtime PAPER, FLAP SHADOW, C2/EXEC/STRATEGY_C SHADOW | Beide PAPER-Verarbeitungen; eindeutige Nonce-/Intent-Koordination |
| B08 | Wie B07, aber FLAP OFF | Keine Flap-Kandidatenzulassung zu C3/Strategie |
| B09 | Wie B05, aber C2 OFF | C3 verweigert ohne C2-Snapshot, kein BUY |
| B10 | Wie B05, aber EXEC OFF | Kein C3-BUY |
| B11 | FOUR_CLASSIC SHADOW | BLOCKED FOUR_EXECUTION_PATH_MISSING |
| B12 | OPENFOUR SHADOW | BLOCKED FOUR_EXECUTION_PATH_MISSING |
| B13 | FLAP SHADOW + beide Four-Venues SHADOW | Flap bleibt unabhängig; Four bleibt BLOCKED |
| B14 | Jede Quelle/Komponente/Venue/Runtime einzeln LIVE mit ECON_LIVE DISABLED | BLOCKED mit zutreffendem Grund; C2 ggf. eigener operationaler Grund |
| B15 | Gleiche LIVE-Requests mit ECON_LIVE ENABLED | Weiterhin BLOCKED; kein produktiver LIVE-Executor |
| B16 | Runtime OFF → SHADOW → PAPER → OFF während Replay-Events | Keine hängenden Lanes/Positionen; konsistentes Journal; kein SEND |

**Zusätzliche vollständige Unterkombination:** Bei RUNTIME_MODE PAPER und FLAP SHADOW alle **8 binären OFF/SHADOW-Kombinationen** von STATE_C2/EXEC/STRATEGY_C mit produktiven Callern gegen Mock-/Replay-Inputs durchschalten. Je Venue OFF/SHADOW/LIVE einzeln, mit fachgerechter BLOCKED-Begründung. Keine sinnlosen kartesischen Kombinationen aller 20 Controls.

#### C. Wallet-, ökonomische Freigabe- und Tageslimit-Prüfungen

L = WALLET_LAUNCHER, B = WALLET_BUYER, S = ECON_SIGN, A = ECON_ARM, D = ECON_SEND, T = ECON_ATTEMPT, V = ECON_LIVE; 1 = ENABLED, 0 = DISABLED. In synthetischen Harnesses meint „SIGN-Check PASS“ nur die Erfüllung der nativen Schaltervoraussetzung, niemals eine Erlaubnis zum Signieren realer Wallet-Keys.

| ID | Logische Schalterkombination / Zustandswechsel | Verbindliche Erwartung |
|---|---|---|
| C01 | L/B/S/A/D/T/V alle DISABLED; Tageslimit 0 | Default fail-closed; 0 SIGN/SEND |
| C02 | L=1, B=0, S=1, A/D/T=0 | SIGN-Check verweigert |
| C03 | L=0, B=1, S=1, A/D/T=0 | SIGN-Check verweigert |
| C04 | L=1, B=1, S=0, A/D/T=0 | SIGN-Check verweigert |
| C05 | L=1, B=1, S=1, A/D/T=0 | Nur lokale SIGN-DRY-Schaltervoraussetzung erfüllt; keine reale SIGN-Freigabe, kein SEND |
| C06 | Wie C05, danach L deaktivieren | SIGN-Check verweigert |
| C07 | Wie C05, danach B deaktivieren | SIGN-Check verweigert |
| C08 | L/B/S/A/D/T=1; Tageslimit 0; Testkonto | Wirtschaftlicher Versuch vor SEND verweigert |
| C09 | L/B/S/A/D/T=1; Limit 1, V5/Guard UNARMED | Versuch verweigert; sechs Schalter allein genügen nicht |
| C10 | Alle sechs Pflichtschalter außer jeweils genau einem auf ENABLED | Sechs isolierte Negativfälle; jeder verhindert Signatur/Release |
| C11 | Sechs Pflichtschalter + Limit 1, synthetische V5/Attempt-Grenzen | Produktive Guard-/Barrierenlogik im isolierten Harness, keine echte Egress-Freigabe |
| C12 | ECON_LIVE ENABLED + RUNTIME_MODE LIVE | RUNTIME weiterhin BLOCKED ECONOMIC_LIVE_NOT_AUTHORIZED |
| C13 | Tageslimit 1, erster Attempt gezählt, zweiter angefragt | Zweiter verhindert; kein Double-Count |
| C14 | Limit 3, Race, Restart, UTC-Tageswechsel | Persistenz, Idempotenz und Kapazität korrekt |
| C15 | Alle sieben Wirtschaftsschalter ENABLED, Owner-Restart/Bootwechsel | Wirksam DISABLED/RESET_BY_RESTART; Limit bleibt separat persistent |
| C16 | Pflichtschalter nach Preflight, vor Release-Barriere deaktiviert | Kein unautorisierter neuer Release; nach möglichem Write UNKNOWN statt falsch UNSENT |
| C17 | Zeitgleiche CLI-/GUI-Schaltrequests mit identischer CAS-Generation | Genau ein Apply, anderer stale/conflict; keine verlorene Änderung |
| C18 | Dieselbe request_id und Bytes / dieselbe ID mit anderem Inhalt | Idempotentes Receipt bzw. Konflikt |
| C19 | Owner unlesbar, fremdes HOME, veraltete GUI-Projektion | Fail closed; keine effektive Freigabe |
| C20 | Jeden Wallet-/Wirtschaftsschalter, Runtime und Limit einzeln über CLI und GUI | Native/Protobuf/Gateway/Frontend: requested/effective/reason/generation/receipt identisch |

**Vollständige relevante Unterkombinationen:** Alle **8 Belegungen** von L/B/S als isolierter SIGN-Guard; alle **8 Belegungen** von A/D/T bei sonst gültigen synthetischen Pflichtbedingungen; V=0 und V=1 jeweils mit Source-, Venue- und Runtime-LIVE-Request; Limit 0/1/mehrere/ausgeschöpft/UTC-Tageswechsel. Jede einzelne der sechs G0.7-Bedingungen muss bei DISABLED eine eigene fail-closed-Probe bestehen. Kein wahlloser 128-Zustände-Test ohne Betriebszweck.

#### D. Verbindlicher Nachweis / STOP-Regel

**Für jede Test-ID**: Request wirklich am isolierten nativen Owner stellen und reale Wirkung am zuständigen Fach-Caller prüfen; dazu Native CLI (darkbotctl bsc source request/show), Source-DBREC, Protobuf-Status, Gateway-Receipt und die fünf BSC-Frontend-Reiter auf exakte requested/effective/blocked_reason/generation/request_id abgleichen. Negativfälle verlangen null unautorisierte Signatur/Netzwerksendung, null Doppel-BUY/Nonce- oder Limitumgehung. Auch Crash, Restart, CAS-Race und Frontend-Stale prüfen. Pro Fall Git-/Release-Binary-/Config-SHA, konkrete Eingaben, Test-Scope, Original-Evidence, unabhängiger Review und PASS/FAIL/OPEN. Ein bloßer Quelltexttest oder alternativ akzeptierter Frühabbruch ersetzt keinen echten Verhaltensnachweis (BSC-002-Regressionslehre).

**Kein Mainnet-BUY vor diesem Gate** im tatsächlich benötigten G0.7-Scope. Extern noch nicht zugängliche und *nicht verwendete* Quellen (z. B. 48Club-Ingress, Geth, BlockReq), Four-/Fremdlaunch- oder künftige J-LIVE-Funktion bleiben ehrlich EXTERNAL_BLOCKED/OPEN, dürfen aber keinen anderen bereits qualifizierten Self-Launch-G0.7-Scope künstlich blockieren. Getestete Schalter sind nur Permissions, weiterhin keine V5-/ARM-/Grant-/SEND-/BUY-Freigabe.

### 3c. Neue verbindliche Reihenfolge: LIVE zuerst (28.10.2026)

**Vorrang des aktuellen PHASENPLAN.md:** Zunächst **vollständigen produktiven LIVE-Bot-Pfad inklusive native effective=LIVE, echten BUY- und SELL-Callern, einem BSC-LIVE-Reiter und vollständiger Native→Proto→Gateway→Frontend-Schaltung implementieren**, ohne Echtgeldhandlung. **Erst danach** die umfassende Testkampagne §3a (fünf Quellen/WSS/native E2E) und §3b (20 Controls und 52 Hauptfälle plus relevante Unterkombinationen) beginnen. Einzelne Entwicklungs-Unit-/Buildtests davor sind zulässig. Die Implementierung muss mit synthetischen Freigaben und garantiert nichtsendendem Transport eine fachlich wirksame LIVE-Transition nachweisen können; für reale Wallets bleibt LIVE bis zur gesonderten wirtschaftlichen Freigabe fail-closed. Kein späteres Nachprogrammieren beim Betreiberstart. Fehlende Providerzugänge/Hardware und unqualifizierte, nicht genutzte Quellen separat OPEN statt erfundenem PASS. G0.7/G0.8 bleiben eigenständige reale Qualifikationen.

## 4. Messschritte in richtiger Reihenfolge

| Schritt | Was genau tun? | Erwarteter Output / Fortschritt | Vor echter Mainnet-Aktion nötig? |
|---|---|---|---|
| **S-1 – SOFORT LIVE produktiv implementieren** | Vollständige native wirtschaftliche LIVE-Transitions, BUY-/SELL-Pfad, qualifizierbare Source-/Venue-/Adapter-Integration sowie der neue BSC-LIVE-Reiter mit Wallet-/Freigabe-Workflow und wirksamer Abschaltung. Noch keine Echtgeldaktion. | Synthetisch genehmigter Test kann effective=LIVE mit tatsächlicher BUY-/SELL-Verarbeitung bis zum strikt nichtsendenden Testtransport erreichen; echte Wallets/finanzierter Mainnetbetrieb bleiben gesperrt. | **Zwingend VOR der umfassenden Schalter-/WSS-/E2E-Gesamttestkampagne**, nicht erst vor Echtgeld-BUY |
| **S0 – Iststand sichern** | Aktuellen Code-/Release-SHA und vorhandene G0.1–G0.6/P8/H-Evidence binden; keine Messung doppelt durchführen | Diese M01–M18-Matrix mit belastbarer Pfad-/Run-Identität; ältere PASS/FAIL unverändert | Ja |
| **S1 – Schalter-E2E** | Wallet LAUNCHER/BUYER und alle weiteren wirtschaftlichen Controls implementieren, Defaults DISABLED, CLI/Frontend-Projektion, Fallmatrix | O01/O02/O17 lokal PASS; 0 SIGN/0 SEND bei DISABLED | Ja |
| **S2 – Marktsichtung stoppen/auswerten** | Bestehende G0.4-Aufzeichnung nur geordnet beenden; bereits vorhandene Daten einmal pro Source/Profil auswerten; optional maximal 10 Minuten als Explorationsrahmen (kein Pflichtlauf) | Zeitfenster, tatsächliche Quellenverfügbarkeit, vollständige Signed-TX-Anteile, fremde BUY-Positionen; unzureichender Nenner NOT_EVALUABLE | **Keine Wartepflicht** für einen einzelnen G0.7-Kauf |
| **S2a – Quellen einzeln + native Ketten-E2E (offline vor echtem BUY)** | Die fünf Source-IDs gemäß §3a separat klassifizieren; verfügbare WSS parallel, cold-start/subscribed/steady-state je Quelle messen; Mock-WSS/Replay/Last/Negativfälle der echten Rust-Kette; vorhandene Rohdaten zuerst, bei Bedarf insgesamt höchstens 10 Minuten read-only Live-Sichtung. | O04–O06/O15/O16/O18: pro Quelle begründeter PASS/OPEN/NOT_CONFIGURED/EXTERNAL_BLOCKED mit Hash-/Full-/Actionable-Zeit und nativem BUY-Handoff; keine erfundene echte T_send-Inclusion | **Ja**, Offline-Safety für die tatsächlich zum wirtschaftlichen Versuch verwendete Kette vor S4; nicht benötigte Quellen mit dokumentiertem Nicht-PASS, **keine Wartepflicht** vor SIGN-DRY/V5/Readiness |
| **S2b – Schalter-Kombinationsgate (§3b)** | A01–A16, B01–B16, C01–C20 und definierte 8er-Unterkombinationen mit isoliertem Native-Owner, CLI/Proto/Gateway/Frontend und tatsächlichem Fachverhalten testen; bereits belegt nur per SHA/Evidence anrechnen | Jeder Fall mit PASS/FAIL/OPEN, Gründen, Receipts, Sicherheits- und Wirkungstest; gewählter G0.7-Scope ohne offene Pflichtfälle | **Ja, zwingend VOR S4/erstem Echtgeld-BUY**; unbenutzte nicht qualifizierte Feeds und J/LIVE bleiben separat OPEN |
| **S3 – Current qualification** | Flap-/Four-Verträge und Self-Launch-Economics/BUY-Kontext prüfen; fremde Profile separat; echte Preflight-/G0.6-Identität und Transport/Kosten-Provenienz | O03/O04/O06 soweit ohne Geld testbar; keine Mainnet-Ausführung | Ja, für den jeweils gewählten Scope |
| **S4 – G0.7 Versuch 1** | Separat genehmigter exakt gebundener Self-Launch + BUY mit laufender Capture | Echter Versuch, vollständige G0.7 Evidence, **nicht** nur RPC-ACK | Ja, nur bei ausdrücklicher wirtschaftlicher Nutzerfreigabe |
| **S5 – G0.8 direkt danach** | Receipts, canonical Block, tatsächlichen Token-Zufluss, Position N+1/0 Konkurrenten, Gesamtkosten, Reorg/UNRESOLVED prüfen | Separater Erwerbs-/Ordering-/Kosten-PASS, ADAPT, STOP oder UNRESOLVED; T0-Gap unabhängig UNKNOWN ohne Witness | Ja, nach Versuch |
| **S6 – SELL und Netto** | Erst im dafür tatsächlich qualifizierten Scope reale Position schließen, Gebühren/Steuern/Approval erfassen | Realer SELL-PASS und Netto-PnL/Verlust desselben gekauften Tokens | Nicht als impliziter Teil von S4 behaupten |
| **S7 – weitere gezielte Ereignisse** | Nach erstem Self-Launch-Versuch **höchstens 9 weitere** individuell freigegebene wirtschaftliche BUY-Versuche, **maximal 10 insgesamt**. Keine feste Wartezeit und keine automatisierte Zehnerserie. | Positive Tokenkäufe / alle Versuche, Quell-/SEND-Latenzen, Platzierung, realer Spend, und bei qualifiziertem Exit auch SELL-/Netto-PnL; vollständiger Nenner | Ja, pro Versuch; kein Kollektiv-Grant |
| **S8 – Produktionsurteil** | Getrennte Bewertung: technische Sicherheit, Kauf/Ordering, echte `T0`-Messbarkeit, vollständiger Exit, Netto-Wirtschaftlichkeit, aktuelle Provider-/Runtime-Qualifikation | Kein pauschaler „Bot PASS“ aus lokalem PAPER, einem ersten Kauf oder einer positiven Einzelposition | Ja vor breitem LIVE |

**Abbruch ist immer ereignis-/budgetgetrieben**, nicht an eine künstliche 1-h-/7-Tage-Wartezeit gekoppelt. `max_buy_attempts_per_day` begrenzt die Anzahl neuer Versuche, nicht die laufende Zeit der Quellenüberwachung. Die zwei Wallets und sämtliche wirtschaftlichen Controls bleiben standardmäßig deaktiviert.

## 5. Pro-Versuch-Pflichtdatensatz (ein eindeutiger Record je Attempt)

| Feldgruppe | Mindestinhalt |
|---|---|
| **Identität** | Datum UTC, `run_id`, `attempt_id`, Self/Fremd, Venue/Deployment/ABI/Portal-Codehash, Chain-ID, Candidate-Generation, Walletrollen (nur öffentliche Adressen), Git-/Binary-/Config-/Policy-/V5-SHA, Egress-Adapter |
| **Quellbeobachtung** | Pro Quelle verfügbar/nicht verbunden, health/qualifiziert, Hash-only vs full signed, tx hash/sender/selector, lokal monotone `T_first_hash`, `T_full`, `T_actionable`, Quelle des Winning Event, Gap-/Drop-Zähler |
| **BUY-Berechnung** | unabhängig verifizierte Tokenadresse, Quote-State/-Blockhash, max input, min token output, Creator-/Tax-/Fee-Bedingungen, Gaspreis/Limit, Nonces, erwartete/absolute Worst-Case-Kosten, prebuilt/sign-ready flags |
| **Zeitgrenzen** | `T_ready`, `T_queue`, `T_send` pro Adapter und Definition, `T_head_local`, Producer-T0/T_buy nur mit qualifizierter Originalevidence; sonst `UNKNOWN`; Wall-Clock- und monotone Zeit nicht verwechselt |
| **Submission/Chain** | Alle Builder-/geth-Responses, ACK/Reject/Timeout/UNKNOWN, Release-Barriere, ERC20-Transfer, Receipt-Status, canonical Blocknummer/Hash und Tx-Index, erster konkurrierender externer BUY, Reorg/Finalität |
| **Wirtschaftlich** | Exakter BUY-Input in wei, erhaltene Token-Roheinheiten plus verifizierte Decimals, Launch-/BUY-Gas tatsächlich, effektive Gaspreise, tips/builder fees/tax/slippage, Gesamtabbuchung; bei SELL Tokenabfluss, Verkaufserlös, alle Exitkosten, realisierter Netto-PnL |
| **Safety/Control** | Wallets LAUNCHER/BUYER requested/effective, wirtschaftlicher Mode, V5/Attempt-Authority, CAS-/Receipt-ID, Tageslimit verbraucht/verbleibend, PASS/FAIL/OPEN/UNRESOLVED und begründeter Abbruchcode |
| **Reproduzierbarkeit** | Exakte Evidence-/Journal-/Manifestpfade außerhalb privater Browserdaten, SHA-256, Prüfbefehl, Exitcode, Test-/Verifikation-SHA, Reviewer und source-quality; bei Failure ebenfalls versiegeln |

## 6. Verbindliche Abnahmeformulierungen

- `LOCAL_HOTPATH_PASS`: erst mit direkter Messgrenze; niemals automatisch `REAL_BUY_PASS`.
- `G07_ATTEMPT_EXECUTED`: ausschließlich bei eindeutigem realem Attempt und dessen originaler Evidence.
- `G08_ACQUISITION_PASS`: eigener intended Token positiv erhalten + bestätigte kanonische Receipts/Kosten.
- `G08_ORDERING_PASS`: eigener BUY steht im nachgewiesenen Self-Launch-Scope unmittelbar an N+1, kein erfolgreicher externer BUY dazwischen.
- `PHYSICAL_T0_TO_BUY_PASS`: nur bei qualifiziertem Original-Producer-Witness und **<1 ms**; sonst `UNKNOWN/NOT_PROVEN`.
- `SELL_EXECUTED_PASS`: realer Tokenabfluss und zurechenbarer Verkaufserlös/Exitkosten kanonisch belegt.
- `REALIZED_NET_PROFIT_PASS`: exakt derselbe wirtschaftliche Positionskreis hat nach **allen** Kosten tatsächlich positiven Netto-PnL; Verluste bleiben als FAIL für dieses einzelne Gewinnziel erhalten.
- `BSC_PRODUCTION_READY`: **OPEN**, bis Wallet-/Control-Safety, realer Ein- und Ausstieg, qualifizierte aktuelle Betriebsumgebung und die unabhängigen notwendigen Freigaben belegt sind. Ungetestete Fremdlaunch-/Four-Modi bleiben OPEN, auch wenn ein eigener Flap-G0.7-BUY gelingt.

## 7. Aufgaben für Claude / zukünftige Messung

1. Diesen Plan gegen das jeweilige neueste `origin/main` und echte bereits vorhandene `.dbrec/.dbjnl` aus den genannten Evidence-Verzeichnissen **read-only** verifizieren; M01–M18 anhand tatsächlich lesbarer Records, Testausgaben und SHA bestätigen oder konkret `SOURCE_UNVERIFIED` setzen. Kein pauschales Vermischen alter Releases mit neuem Code.
2. Den **laufenden** G0.4-Capture-Status **lokal** ermitteln und entsprechend Nutzerentscheidung über bestehenden Stop-Mechanismus geordnet beenden, Rohdaten versiegeln. Keine unkontrollierte Änderung der Campaign-Config bei laufender Aufnahme; keine rückwirkend günstigeren Kriterien. Die damals vorgeschlagenen `g04_criteria.env`-Schwellen werden nicht stillschweigend zum heutigen PASS-Test.
3. O01/O02/O17 priorisiert mit tatsächlicher Native-/CLI-/Proto-/Gateway-/React-E2E-Abnahme schließen; kein Realgeldtest allein aus dem Messplan.
4. Vor jedem wirtschaftlichen Versuch genau die dafür nötigen **OPEN**-Zeilen O03/O06/O09/O10 vorbereiten und den Nutzer die exakte signierte Candidate-/V5-/Budget-/Provider-Identität separat prüfen lassen.
5. Nach jedem erlaubten Versuch die Zeilen O07–O13 mit **originaler kanonischer Evidence**, nicht mit Schätzungen, fortschreiben. Echte Verluste, Nullausführungen, fehlende Receipts und zeitliche Unknowns immer mit Nenner behalten.
6. Im `STATUS.md` den jeweils aktuellen Stand und den **Link auf dieses eine Messregister** pflegen. Ein zweites konkurrierendes PASS/FAIL-Register vermeiden.

**Gesamtstatus am 27.10.2026:** **TECHNISCHE TEILNACHWEISE = PASS/FAIL wie oben; ECHTER BUY = OPEN; ECHTER SELL = OPEN; REALISIERTER NETTOERTRAG = OPEN; PHYSISCHES <1-ms-ZIEL = OPEN.** Keine Mainnet-Freigabe durch diesen Plan.


## 8. Vollständigkeitsprüfung – versionierte Testquellen und Evidence-Archive (27.10.2026)

**Git-Tree-Stand:** `b72bf2ed4bdfb12b372d72ed26bb2272663119c2`; `4220` Einträge; `truncated=false`. Diese Inventur deckt alle durch ihre Namenskonvention erkennbaren, **aktuell versionierten** Testdateien und die drei BSC-Evidence-Wurzeln ab, nicht automatisch jede `#[cfg(test)]`-Funktion, jeden historischen ungetrackten Mac-Lauf oder jeden gelöschten älteren Commit. **Quellcode vorhanden ≠ Test ausgeführt ≠ PASS.**

### 8.1 Testdateien (Dateiebene)

**123 erkannte Testquellen.** Klassifizierung: `SOURCE_PRESENT`. Projektspezifische `frontend/project0`-Tests sind als Shared/Projekt_0-Abgrenzung mit aufgeführt, **nicht** als BSC-Fach-PASS.

- `bsc/engine/crates/execution/tests/acceptance_guard.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/c3_paper_latency.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/common/mod.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/dispatch.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/egress_latency.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/f_ingest_latency.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/flap_fastlane_roundtrip.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/four_fastlane_roundtrip.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/framing.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/g06a_ab_parity.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/g06a_dry_handoff.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/g07_attempt_authority.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/g07_builder_probe.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/g07_candidate.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/g07_no_send.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/g07_readiness.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/hotpath_alloc.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/release.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/responses.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/runtime_signal.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/strategy_paper_latency.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/support/requalification.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/tls.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/tls_support/mod.rs` — SOURCE_PRESENT
- `bsc/engine/crates/execution/tests/transport.rs` — SOURCE_PRESENT
- `bsc/engine/crates/geth_ipc/tests/c15_hot0_alloc.rs` — SOURCE_PRESENT
- `bsc/engine/crates/geth_ipc/tests/c18_stress.rs` — SOURCE_PRESENT
- `bsc/engine/crates/qualification/tests/economics_snapshot.rs` — SOURCE_PRESENT
- `bsc/engine/crates/qualification/tests/g08_binary_records.rs` — SOURCE_PRESENT
- `bsc/engine/crates/qualification/tests/g08_reads_g07_evidence.rs` — SOURCE_PRESENT
- `bsc/engine/crates/qualification/tests/js_dbrec_parity.rs` — SOURCE_PRESENT
- `bsc/engine/crates/qualification/tests/loopback.rs` — SOURCE_PRESENT
- `bsc/engine/crates/qualification/tests/offline.rs` — SOURCE_PRESENT
- `bsc/engine/crates/qualification/tests/provenance_clock.rs` — SOURCE_PRESENT
- `bsc/engine/crates/qualification/tests/support/mod.rs` — SOURCE_PRESENT
- `bsc/engine/crates/trading/tests/c14_prefilter_scope.rs` — SOURCE_PRESENT
- `bsc/engine/crates/trading/tests/test_support.rs` — SOURCE_PRESENT
- `bsc/engine/crates/tx_encoding/tests/rlp_differential.rs` — SOURCE_PRESENT
- `bsc/geth/overlay/darkbot/fastlane/chain_v2_test.go` — SOURCE_PRESENT
- `bsc/geth/overlay/darkbot/fastlane/fastlane_test.go` — SOURCE_PRESENT
- `bsc/geth/overlay/darkbot/fastlane/ingress_fixtures_test.go` — SOURCE_PRESENT
- `bsc/geth/overlay/darkbot/fastlane/ingress_test.go` — SOURCE_PRESENT
- `bsc/geth/overlay/darkbot/fastlane/outbound_test.go` — SOURCE_PRESENT
- `bsc/geth/overlay/darkbot/fastlane/stress_test.go` — SOURCE_PRESENT
- `bsc/geth/overlay/darkbot/fastlane/trust_test.go` — SOURCE_PRESENT
- `bsc/geth/overlay/darkbot/fastlane/v2_test.go` — SOURCE_PRESENT
- `bsc/tools/test_g06a_dry_modes.py` — SOURCE_PRESENT
- `bsc/tools/test_g06a_key_hygiene.cjs` — SOURCE_PRESENT
- `bsc/tools/test_g06a_private_mode.cjs` — SOURCE_PRESENT
- `bsc/tools/test_g06a_shell_scope.py` — SOURCE_PRESENT
- `bsc/tools/test_g06a_state_pin.cjs` — SOURCE_PRESENT
- `bsc/tools/test_readiness_job.py` — SOURCE_PRESENT
- `bsc/tools/test_status_sync.py` — SOURCE_PRESENT
- `frontend/src/core/realtime/GatewayConnection.test.ts` — SOURCE_PRESENT
- `frontend/src/core/realtime/TopicState.test.ts` — SOURCE_PRESENT
- `frontend/src/core/realtime/TopicSubscriptions.test.ts` — SOURCE_PRESENT
- `frontend/src/core/realtime/analyticsControl.test.ts` — SOURCE_PRESENT
- `frontend/src/core/realtime/candidateControl.test.ts` — SOURCE_PRESENT
- `frontend/src/core/realtime/captureCommand.test.ts` — SOURCE_PRESENT
- `frontend/src/core/realtime/captureControl.test.ts` — SOURCE_PRESENT
- `frontend/src/core/realtime/control-golden.test.ts` — SOURCE_PRESENT
- `frontend/src/core/realtime/readinessControl.test.ts` — SOURCE_PRESENT
- `frontend/src/features/bsc/api/bsc-golden.test.ts` — SOURCE_PRESENT
- `frontend/src/features/bsc/model/candidate.test.ts` — SOURCE_PRESENT
- `frontend/src/features/bsc/model/checks.test.ts` — SOURCE_PRESENT
- `frontend/src/features/bsc/model/placement.test.ts` — SOURCE_PRESENT
- `frontend/src/features/bsc/model/reportFilters.test.ts` — SOURCE_PRESENT
- `frontend/src/features/bsc/model/sources.test.ts` — SOURCE_PRESENT
- `frontend/src/features/bsc/model/switches.test.ts` — SOURCE_PRESENT
- `frontend/src/features/overview/api/overview-golden.test.ts` — SOURCE_PRESENT
- `frontend/src/features/overview/model/lifecycle.test.ts` — SOURCE_PRESENT
- `frontend/src/features/overview/model/overviewRows.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/api/analyticsAdapter.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/api/ownerAdapter.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/api/panelTopics.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/api/project0Adapter.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/api/project0ViewAdapter.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/model/capturePreview.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/model/contracts.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/model/statusFormat.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/model/statusSelectors.test.ts` — SOURCE_PRESENT
- `frontend/src/features/project0/model/streamFreshness.test.ts` — SOURCE_PRESENT
- `frontend/tests/bsc.spec.ts` — SOURCE_PRESENT
- `frontend/tests/code-split.spec.ts` — SOURCE_PRESENT
- `frontend/tests/overview-f7-ovr.spec.ts` — SOURCE_PRESENT
- `frontend/tests/overview.spec.ts` — SOURCE_PRESENT
- `frontend/tests/project0-backend.spec.ts` — SOURCE_PRESENT
- `frontend/tests/project0-status.spec.ts` — SOURCE_PRESENT
- `frontend/tests/visual/p0-tabs.spec.ts` — SOURCE_PRESENT
- `gateway/tests/control_goldens.rs` — SOURCE_PRESENT
- `gateway/tests/feed_config.rs` — SOURCE_PRESENT
- `gateway/tests/p0_reporter_e2e.rs` — SOURCE_PRESENT
- `gateway/tests/unix_feed.rs` — SOURCE_PRESENT
- `gateway/tests/ws_integration.rs` — SOURCE_PRESENT
- `ops/tests/bsc_control_browser.mjs` — SOURCE_PRESENT
- `ops/tests/bsc_control_test.py` — SOURCE_PRESENT
- `ops/tests/control_browser.mjs` — SOURCE_PRESENT
- `ops/tests/control_test.py` — SOURCE_PRESENT
- `ops/tests/lifecycle_browser.mjs` — SOURCE_PRESENT
- `ops/tests/lifecycle_test.py` — SOURCE_PRESENT
- `ops/tests/stop_all_partial_browser.mjs` — SOURCE_PRESENT
- `ops/tests/stop_all_partial_test.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/__init__.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-27-g01/decode_flap_v6_fixture.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-27-g01/sources/submitted_keccak_under_test.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/audit_existing_rpc_config.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_flap_intervening.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_g02_modules.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_g02_pool_source.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_g02_sources.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_g02_tree.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/keccak_audit.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/probe_flap_archive.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/read_flap_archive_once.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/read_rpc.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/trace_flap_g02.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/update_gate0_master.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/verify_flap_g02.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/verify_flap_g02_economics.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/verify_flap_intervening.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/write_g02_economics_report.py` — SOURCE_PRESENT
- `prestudy/bsc/evidence/tests/test_bsc_profile_fail_fast.py` — SOURCE_PRESENT
- `prestudy/bsc/references/production-plan-reference/unit1-reference/bsc/native/crates/darkbot-wire/tests/rlp_differential.rs` — SOURCE_PRESENT

### 8.2 Archivierte Gate-0-Programme und Messwerkzeuge

**26 historische ausführbare Quellen**, kein aktueller Produktionspass:

- `prestudy/bsc/experiments/gate0/src/bin/g03.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g05_e2e_local.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g05_ipc.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g05_local.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g05_remote_e2e.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g05_remote_e2e_stability.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g05_remote_transport.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g06_concurrency.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g06_flap_pending_decode.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g06a_48club_floor_probe.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g06a_presigned_handoff_transport.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g06a_target_handoff.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g07_testnet_birth_buy.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/src/bin/g07_testnet_buy.rs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/analyze_g04_coverage_v2.cjs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/collect_g03_activation.py` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/g06_capture_flap_actionable_pending.cjs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/g06_capture_flap_exact_scope_pending.cjs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/g06_quicknode_full_pending.cjs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/probe_g04_coverage_v2.cjs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/probe_g04_quicknode.cjs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/probe_sequence.cjs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/run_g05_48club_stability.py` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/run_g05_blockrazor_stability.py` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/verify_g03.cjs` — HISTORICAL_SOURCE
- `prestudy/bsc/experiments/gate0/tools/verify_g04_probe.cjs` — HISTORICAL_SOURCE

### 8.3 Evidence-Quellen je Archivfamilie

**1571 versionierte Roh-/Reportdateien** unter `bsc/native/evidence/`, `prestudy/bsc/evidence/` und `prestudy/bsc/experiments/gate0/evidence/`. Keine Vermischung von Roh-RPC-Responses und tatsächlich bestandenen Testfällen.

| Pfad | Versionierte Dateien | Nachweisstatus |
|---|---:|---|
| `bsc/native/evidence/c1` | 163 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/c3` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/f` | 7 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/g` | 10 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/g01` | 11 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/g04` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/g06` | 5 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/g06a` | 42 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/g07` | 15 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/h` | 22 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/i` | 4 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/p8` | 51 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `bsc/native/evidence/q5a` | 3 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/docs/BSC_PHASE_B_OPTIONAL_CLEANUP_DRAFT.md` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/docs/phase_a` | 32 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/docs/reviews` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/native/SHA256SUMS.txt` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/native/SHA256SUMS.unit5-core-overlay.txt` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/__init__.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0` | 7 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-27-g01` | 259 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-archive-probe` | 5 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-archive-probe-retry` | 21 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-archive-witness` | 5 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-archive-witness-quicknode-153754` | 659 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-economics` | 73 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-28-g02-latency-offline` | 2 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/audit_existing_rpc_config.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/audit_flap_trade_bytecode.cjs` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/classify_flap_hotpath.cjs` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_flap_intervening.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_g02_modules.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_g02_pool_source.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_g02_sources.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/collect_g02_tree.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/decode_flap_economics.cjs` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/GATE0_G01_READONLY_RUN_2026-09-28.md` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/keccak_audit.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/measure_flap_offline_latency.cjs` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/probe_flap_archive.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/read_flap_archive_once.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/read_rpc.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/replay_flap_evm.cjs` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/trace_flap_g02.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/update_gate0_master.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/verify_flap_archive_witness.cjs` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/verify_flap_g02_economics.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/verify_flap_g02.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/verify_flap_intervening.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/verify_flap_semantic_replay.cjs` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/write_g02_economics_report.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/README.md` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/evidence/tests/test_bsc_profile_fail_fast.py` | 1 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |
| `prestudy/bsc/experiments/gate0/evidence` | 142 | TRACKED_EVIDENCE, Einzelresultat nur mit Original-Run/Log |

**Offene Vollständigkeitsgrenze:** Nicht versionierte Laufdaten (`~/.darkbot`, `bsc/.local`, `bsc/data`, `~/Library/Logs/DarkBot`), alte Git-History-/Reflog-Commits und private CI-/Claude-Ausgaben lassen sich aus einem aktuellen GitHub-Tree nicht vollständig rekonstruieren. Bis dahin gilt `ALL_HISTORIC_RUNS_VERIFIED=NO`. `LOCAL_PASS` bleibt an den ursprünglichen SHA/Scope gebunden.


## 9. Normative Einzelabnahmekriterien – ergänzter Pflichtkatalog

**Wichtig:** PRESCRIBED bedeutet vorgeschrieben, aber **nicht automatisch ausgeführt oder bestanden**. Historische Aussagen aus dem Masterplan sind auf ihren damaligen Code-/Testscope begrenzt. Für jeden unten genannten Teilfall sind einzelne `test_name → commit_sha → evidence_path → outcome`-Bindungen auf dem finalen BSC-Release zu prüfen. Neue Nutzerentscheidungen vom 27.10. haben bei reinen Entwicklungs-/G0.4-Zeitvorgaben Vorrang; davon unabhängige fachliche Gates bleiben bestehen.

### 9.1 Vollständige W1–X1-Matrix aus dem Production Master Plan §14

| ID / Beleg | Testpflicht/Owner | dokumentierter früherer Scope | Referenz |
|---|---|---|---|
| W1 — EIP-155, historical 343-byte TX, short r/s, 1,024 signatures | Unit 1: byte/hash/chain/sender/low-S/output/buffer tests | Unit 1 IMPLEMENTED_TESTED and FROZEN for its declared scope | SN01 |
| W2 — malformed RLP and 27,624 actual differential inputs | Unit 1: real Rust differential and bounded decoder error cases | Unit 1 IMPLEMENTED_TESTED/FROZEN; delivered differential set PASS, no exhaustive-fuzz claim | SN01 |
| P1 — V6 TAXED_V3, real CREATE2, seven vectors, corrected mismatch | Unit 2: deployment/salt/init-code/address golden and wrong-mode rejection | Unit 2 IMPLEMENTED_TESTED and FROZEN for its declared scope; wider Flap scope remains Phase D | SN02, SN19 |
| P2 — 65 TX witness, Router→Portal and direct call, independent output | Units 2/3: independent semantic fixtures tied to actual prestate/caller | Scoped Unit 2/3 production fixtures IMPLEMENTED_TESTED/FROZEN; generic live semantics remain unqualified | SN03, SN04, SN19 |
| Q1 — 497 dependency rows, coherent snapshot/invalidators | Units 2/3 + C2: explicit profile inputs; stale/reorg/mode/code/caller negatives | Unit 2/3 input/admission contracts tested; live coherent cache/refresh/invalidation coverage TRACKED_DEFERRED to C2 | SN04, SN07 |
| Q2 — immediate DOGE BUY, minOutput exact/+1, pre-launch revert | Unit 3: sequential EVM golden and negative tests, no observed-output input | Unit 3 exact DOGE fixture scope IMPLEMENTED_TESTED/FROZEN; other conversion segments remain disabled | SN03, SN05 |
| Q3 — native-BNB Curve-20 and exact fee rounding | Unit 3 + G: independent Rust/EVM, tax toggle/exemption/slippage/overflow | Unit 3 qualified Curve-20 scope IMPLEMENTED_TESTED/FROZEN; broader economics/cost remains Phase G and operational policy Unit 6 | SN05, SN18 |
| N1 — 8,000 signals/1,000 intents, cross-chain and ordering failures | Unit 4: bounded dedup/nonces/recipient isolation/UNKNOWN regressions | Unit 4 IMPLEMENTED_TESTED and FROZEN for its declared single-sender scope; integrated timing remains C3 | SN06 |
| N2 — 256 final-envelope bindings, short signatures | Units 1/2/4: intended chain/signer/nonce/Portal/token/recipient final binding | Unit 1/2/4 binding path IMPLEMENTED_TESTED/FROZEN; eight test senders do not authorize multi-sender production | SN01, SN02, SN06 |
| N3 — injected HoL, startup quarantine in all four states | Unit 4 + C2: coupling telemetry, startup failures, controlled recovery proof | Unit 4 quarantine/HoL consumer semantics IMPLEMENTED_TESTED/FROZEN; durable fence Unit 6/runner, recovery proof C2/Unit 5, real HoL timing C3 | SN06, SN08 |
| S1 — permit-before-write, ACK≠confirm, partial-write HALT | Units 4/5: once-economic/per-adapter permits and fault matrix | Unit 4 permit/fan-out state machine IMPLEMENTED_TESTED/FROZEN; actual transport/provider behavior remains Unit 5 | SN08, SN10 |
| S2 — ordered self-launch/public/private inclusion, gas/budget/floor evidence | Unit 5 + Unit 4/C2 + C3/J: exact options/payload bytes, cost/expiry/placement; §8 after-handoff validity and delayed-execution regressions | API acceptance/diagnostic is not real N+1 qualification; request expiry is not transaction revocation | SN12 |
| T1 — 0.433125/0.535250-ms real same-block BUYs | C3 + inclusion track: retain scope/order/acquisition; new exact-runtime measurement | Historical constructive evidence; producer elapsed time UNKNOWN | SN13 |
| T2 — prepared remote diagnostics and tails >1 ms | Unit 5 + C3: separate prepared/full path and per-adapter tails/all outcomes | No universal “every send <1 ms” claim | SN09, SN10, SN11 |
| T3 — 0.103417/0.045125-ms real following-block BUYs | Unit 5 + C3/J: preserve counterexample, optimize arrival/order | Fast local send does not guarantee first-block acquisition | SN13 |
| T4 — clock/actual-send/producer witness | C0 schema + C1/C3 + J: bracket original successful execution, conservative bound; frozen per-cell campaign and verdict evaluator regressions | Actual T0→T_buy <1 ms NOT PROVEN; no campaign or universal-bound claim without its distinct proof | SN11, SN13, SN14, SN15 |
| R1 — no-peer/disconnect/hash/fetch/rate-limit failures | C1/F + runner adapters: source health, actionable binding, full denominator | No feed/P2P coverage winner presumed | SN07, SN17 |
| R2 — pending/canonical/replacement/reorg counterexamples | C2: every applicable retirement-contract counterexample with exact test | Unit 4 proof-consumer transitions IMPLEMENTED_TESTED/FROZEN; proof production and full canonical/reorg ownership remain C2; no parallel chain-state engine | SN16, SN19 |
| C1 — first 15 s, complete receipt/internal-event acquisition evidence | C0 recorder + F/I/J: bounded raw capture, explicit window/completeness | Minimal schema alone is not implemented COMPLETE capture | SN14, SN17 |
| C2 — journal/borrow/sequence/checksum/replay | C0 recorder + C1/F/I: lifetime, corruption/gap/restart and identity tests | No hotpath writer wait; derivatives are offline | SN15, SN16, SN17 |
| X1 — old Solana/PAPER latency and Research Strategy C scans | H: separate observation semantics, BSC fee/EV validation | Preserve Solana; no BSC-EV/timing inference | SN18, SN20 |

### 9.2 Vollständige Economics-Gate-Testmatrix §13

### Quote boundaries
- zero/invalid input rejection;
- minimum meaningful input;
- representative small/medium/large inputs;
- near-boundary state;
- exact rounding edge cases.
### Fees/taxes
- fee-free only if protocol evidence proves it;
- each fee component;
- each supported tax/mode;
- combined components/order.
### Buy/sell
- exact local quote parity;
- exact helper/eth_call parity where available;
- historical executed vector;
- revert vector where available.
### Network cost
- success receipt;
- revert receipt;
- replacement lifecycle;
- approval/permit path where applicable.
### Unknown
- unknown version;
- unknown mode;
- missing economic evidence;
- parity mismatch.

**Belegpflicht:** Alle unterstützten Venue-/Mode-/Version-Kombinationen einzeln, einschließlich realer BUY-/SELL-/Exit-/Approval-/Permit-/Revert-/Replacement-/Migration-Kosten. Ohne explizites Roh-Evidence-Mapping `OPEN`.

### 9.3 C1.5–C1.10, F, I/J, Frontend und Failure-Gates

| Vertrag und Pflichtfall | Status des Nachweises |
|---|---|
| C1 Common: fmt, Clippy, c14 Prefilter, Go fmt/build, C13/C14 Interop | Historisch PASS in C1-Gates; neuer finaler SHA OPEN |
| C1.5: Rust Debug/Release, Compile-fail/Lifetime, HOT0 0 Alloc, Kontrollpfad-Allokation | Historisch C1 PASS; finaler SHA OPEN |
| C1.6: Hostprofile S zweimal bytegleich; Profil P fail-closed; Go race TestC16 | Historisch C1 PASS; finaler SHA OPEN |
| C1.7: outbound Slow-/Fail-/Partial-Peer, Worker Lifecycle, Go race TestC17 | Historisch C1 PASS; finaler SHA OPEN |
| C1.8: Rust Stress Debug/Release, Go race TestC18/TestC15, State/Ownership | Historisch C1 PASS; finaler SHA OPEN |
| C1.9: production-selected-peer, 10.000 Samples/4 Peers, exakte p50/p95/p99/p99.9/p99.99/max, Manifest/Raw | Historisch PASS in C1.9-Scope; reale Mainnet-Fullchain extern OPEN |
| C1.9b: Chain-97-Hook, RLPx, gleiche Clock-/Boot-Identität, Negative Qualifier | Historisch Testnet PASS; Mainnet OPEN |
| C1.10: Frozen source/build/config/hash/pin, dirty tree / stale source refusal / verify Freeze | Historisch C1 FROZEN; neue Codeänderungen nicht abgenommen |
| F: exakt ein **30-minütiges cleanes, natives geth**-Capture, Binärmanifest, Gap/Overflow/Quality/Replay | OPEN / EXTERNAL_BLOCKED laut STATUS; max. 10 Minuten optionales G0.4 ist getrennt |
| F: Peer-Decode→Rust→Recorder p50/p95/p99/p99.9/p99.99/max, Schwellen 0.5/1/3 ms | PARTIAL / finaler echter Ingress-Gate OPEN |
| F/I: Journal Corruption, Capacity, Replay, Restart, Reorg, disk-full nur isolierter Testträger | Historische Teil-PASS, vollständiges Final-SHA-Mapping OPEN |
| G: Quote/fee/tax/rounding/revert/BUY/SELL/approval/permit/replace/migration | Historische Teil-PASS, alle unterstützten Profile einzeln OPEN |
| H: wiederholbarer Strategy-Core, valider Exit und Risk, echte wirtschaftliche Verluste dokumentieren | Replay PASS 201/201; PAPER-Wirtschaft 3/3 Verluste |
| I: gleiche Rust-Core-Pfade in PAPER/SHADOW/LIVE, Egress- und Senderkonkurrenz, finaler Adapter `T_send` | Teil-PASS PAPER; echtes Live-`T_send` OPEN |
| J: 999999 vs. 1000000 ns, Unknown Producer, SEND ohne Erwerb, schlechte Platzierung, fehlende 15-s-Capture, Reorg/Spät-Erwerb, Zero-Attempt und verändertes Manifest | Prescribed, Fall-zu-Test-Mapping OPEN |
| F0-BSC/UI: Native→Protobuf→Gateway→React, Proto Golden U256/signed PnL, CAS/Receipt, Crash, Five Tabs, View Filters, A11y, No Poll, Pixeldiff | Wallet-/Wirtschaftscontrols: F0-VAR + Release-Browser-E2E PASS an `4b0139ee` (2a.3); Pixeldiff NOT_RUN |
| G0.7/G0.8: realer individueller Mainnet-BUY, Erwerb, eigener Tx-Index N+1 und kein externer BUY dazwischen, Gesamtkosten | NOT_RUN (Echtgeld) |
| Original-T0 Witness: Upper-bound < 1000000 ns am selben Produzenten, Fehlergrenzen | UNKNOWN/NOT_PROVEN |

### 9.4 Zusätzliche historisch dokumentierte Prüfungen, bislang ohne Einzelstatus

| Referenz | Historischer Lauf/Ergebnis | Status |
|---|---|---|
| `prestudy/bsc/evidence/docs/reviews/C0_UNIT1_REV2_REVIEW.md` | 27 Rust-Tests im Debug und Release, 27.624 echte RLP-Differentialinputs, Format zuerst rot, isoliert repariert | HISTORICAL_PASS / Scope C0-Unit1 |
| Historisches Unit-1/2/3-Mapping Masterplan | 1.024 Signature-Inputs; sieben CREATE2-Vektoren; 65 historische Tx-Vektoren; 497 State-Abhängigkeiten | HISTORICAL_SCOPED; neueste Anwendung offen |
| Historisches Unit-4-Mapping Masterplan | 8.000 Signale / 1.000 Intents; 256 Envelope-Bindungen; Quarantäne/HoL/Partial-Write | HISTORICAL_SCOPED; produktiver Mainnet-Egress offen |
| `prestudy/bsc/experiments/gate0/evidence/` | archivierte G0.3/G0.5/G0.6/G0.7-Tests, einschließlich vorheriger realer **Testnet**-BUY-Runs | HISTORICAL_TESTNET; keine BSC Chain-56-Akquisition |
| `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/` | historische G0.1/G0.2-RPC-/CREATE2-/Economics-/Archiv- und Fehler-Responses | HISTORICAL_MIXED; nicht alle Rohantworten PASS |
| `prestudy/bsc/evidence/docs/phase_a/PHASE_A_REPORT.md` | 253 PRE / 280 POST / 287 POST_FIX, Parity+Performance | OUT_OF_BSC_SCOPE: Phase-A-Solana/alte gemeinsame Architektur; nicht aktuelle BSC-Testsumme |
| `bsc/docs/5_OFFENE_FEHLER/BSC_Bug_V2.md` | P1–P9 read-only Auditbefunde ohne eigene Dynamiktests | STATIC_REVIEW, NICHT TEST_EXECUTED |
| `frontend/docs/FRONTEND_PHASENPLAN.md` | Gemeinsame Wire-/Gateway-/Frontend-Regression; Einzel-Project_0-Gates nicht als BSC-Fach-PASS werten | SHARED_REGRESSION, BSC-Abgrenzung erforderlich |

### 9.5 Was für den vollständigen Anspruch noch fehlt

1. **Alle jemals durchgeführten Einzeltests** lassen sich nicht allein aus Git `main` beweisen. Nicht versionierte Mac-/Claude-/CI-Logs und historische Git-Commits können zusätzliche Testläufe enthalten. `ALL_HISTORIC_RUNS_VERIFIED=NO`.
2. **Alle tatsächlichen Einzeltestfälle** auch aus Inline-Rust-`#[cfg(test)]`, Go und Vitest benötigen namentliche `--list`-/Discovery-Ausgabe; §8 erfasst Testdateien, nicht jede Testfunktion. `ALL_TEST_CASES_MAPPED=NO`.
3. **Alle bindenden Anforderungen gegen Ergebnisbeleg:** Die Abschnittsanforderungen der Dokumente müssen pro Testfall gegen aktuelle Signaturen und Belege geprüft werden, nicht nur durch PASS auf einer gesamten Suite. `ALL_REQUIREMENT_TO_CASE_MAPPED=NO`.
4. **Endstand Wallet-/Economics-/Daily-BUY-Controls abgenommen:** `LATEST_FINAL_SHA_FULL_GATE=PASS` an `4b0139ee` (2a.3, Evidence `bsc/native/evidence/controls/gate_4b0139ee_20261028/`).
5. Pro Test zwingend: `owner / test_case / required_scope / source_file / command / git_sha / run_id / result / exit_code / sample_count / evidence_sha256 / limitation`. Ohne Originaldaten `SOURCE_UNVERIFIED`, nicht angenommener PASS.

**Fazit:** Dieses Register enthält jetzt einen deutlich vollständigeren versionierten Quell- und Pflichtenindex. Es ist **noch keine nachweislich vollständige Historie sämtlicher jemals gestarteter Testprozesse**. Diese Differenz ist ausdrücklich offen und bei der nächsten lokalen Claude-Übergabe gegen unverversionierte Artefakte zu schließen.
