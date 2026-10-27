# BSC – Verbindlicher Mess- und Abnahmeplan (PASS / FAIL / OPEN)

**Stand:** 27.10.2026 · **Ausgangs-HEAD:** `8b4721bba41fb5c482976d99590bc2119204df13` · **Geltungsbereich:** ausschließlich BSC, nicht Project_0/Solana.  
**Zweck:** Vorhandene Messbelege von fehlenden Messungen trennen; den kontrollierten ersten Mainnet-BUY, die anschließende G0.8-Prüfung und spätere gezielte BUY→SELL-Versuche ohne pauschale Wartezeit vorbereiten. Dieses Dokument ist ein **Messregister, keine Mainnet-Transaktionsfreigabe**.

## 1. Statuslegende / Regeln zur Beweiskraft

- **PASS** = belegtes Ergebnis **nur innerhalb des ausdrücklich genannten Scopes** (etwa historisch, lokal, PAPER, SHADOW, Testnet oder echte Mainnet-Transaktion). Ein lokaler PASS wird nie stillschweigend zum Produktions-/Erwerbs-PASS.
- **FAIL** = gemessener Fehler bzw. negatives Ergebnis **für das konkret benannte Kriterium**, nicht automatisch für den gesamten Bot.
- **OPEN** = nicht ausgeführt, unvollständig, unzureichende Originalbelege, unqualifizierte Umgebung, in neuer Version nicht wiederholt oder noch kein akzeptierter numerischer Schwellenwert. `UNKNOWN`, `NOT_EVALUABLE`, `EXTERNAL_BLOCKED` und `UNRESOLVED` bleiben zusätzlich als präzise Evidenz-Verdikte erhalten; sie sind **kein PASS**.
- **Ältere Messungen** gelten nicht automatisch nach Portal-/ABI-/Code-/Config-/Provider-Änderungen. `PASS` ohne exakten SHA, Build/Host, Run-ID, Messgrenze, Sample/Nenner, Roh-Evidence und nachvollziehbaren Test ist nicht zulässig.
- **Zwei unterschiedliche Primärfragen:** (A) *G0.7/G0.8* „Wurde der intendierte Token erfolgreich gekauft, wie früh gegenüber Konkurrenz und zu welchen wirklichen Kosten?“ (B) *späterer BUY→SELL-Betrieb* „Wurde die Position real geschlossen und war der Nettoertrag nach sämtlichen Kosten positiv?“ Ein bestandener Kauf ist **kein** Profit-PASS.
- Die 27.10.-Nutzerentscheidung in `PHASENPLAN.md` hat Vorrang vor der alten **7-Tage-Wartepflicht**: explorative G0.4-Sichtung **höchstens 2 h**, keine Mindestzeit, keine Pflicht, vor einem einzelnen G0.7-Test auf einen Census zu warten. Für zielgerichtete Tests zählt die **Anzahl freigegebener Ereignisse/Versuche**, nicht eine vorgegebene Beobachtungsdauer. `max_buy_attempts_per_day=10` ist ein **Beispiel für ein konfigurierbares Maximum**, keine Verpflichtung, zehn BUYs auszuführen.

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
| M18 | **G0.4 7-Tage-Kampagne** | `STATUS.md` meldet aktiven, 7 Tage konfigurierten PUBLICNODE+DRPC-SHADOW-Capture; `campaign.sh` hat 604800 s und eine 7-Tage-Deadline; `g04_criteria.env` noch `PROPOSED` | **OPEN** | 27.10.-Entscheidung verlangt **maximal 2 h** und danach geordnetes Beenden. Remote-Git kann nicht verifizieren, ob der Prozess auf dem Mac schon beendet wurde; kein Coverage-Verdikt behaupten |

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
| T35 | G0.6a Shell-/Secrets-Provenienz | `test_g06a_dry_modes.py`, `test_g06a_key_hygiene.cjs`, `test_g06a_private_mode.cjs`, `test_g06a_shell_scope.py`, `test_g06a_state_pin.cjs` | Testdateien/gezielte Fixes vorhanden; **keine konsolidierte Einzel-PASS-Zahl** in aktuellem STATUS | **OPEN / COUNTS_UNVERIFIED**, bis Claude exakte Exitcodes/SHA dem Paket zuordnet; nicht 0 und nicht erfunden PASS |
| T36 | G0.7 Native Readiness | V5-/Nonce-/Gas-/State-/Deployment-/Clock-/Budget-Check, `PASS_READ_ONLY` / `BLOCKED` / `STOP` getrennt | Historische read-only PASS- und aktuelle BLOCKED/STOP-Zustände dokumentiert | **LOCAL_PASS** für Guard-/Status-Differenzierung; **OPEN** für eine aktuelle grüne Mainnet-Readiness des neuen Versuchs · `STATUS.md` |
| T37 | G0.8 Verifier gegen externe Historie | `g08_binary_records`, `g08_reads_g07_evidence`, `provenance_clock`, `js_dbrec_parity` | Testgruppen **im Repository vorhanden**, G0.8-Archiv-Qualifikation am 27.10. im STATUS `QUALIFIED` | **LOCAL_PASS** für dokumentierte interne Regressionen, **OPEN** für konkreten G0.7-Echtdurchlauf; einmalig `--ignored`-Real-RPC-Test ist **nicht** Teil des pauschalen 1.339-PASS |
| T38 | Gateway-Commands / Source-Modes | Typisierte `bsc.readiness.start/stop`, Candidate und `bsc.source.mode.set`; Generation, Validierung, request/effective, Refusal | Bestandteil Gateway-74 und BSC-E2E, eigene Rust-Tests in `gateway/src/control.rs` vorhanden | **LOCAL_PASS / INTEGRATION_PASS** (bestehender Control-Scope) · Wallet-/Economy-Controls noch **OPEN** |
| T39 | BSC Frontend-QA | BSC Status/Feldrendering, Readiness-/Candidate-/Source-Reiter und No-Polling-Integration | Bestandteil Frontend-113 / E2E und F0-VAR, historisch bestanden | **LOCAL_PASS / INTEGRATION_PASS**; **Wallet-Enable-Schalter noch nicht enthalten**, daher OPEN für neue UI |
| T40 | Phase-Gates C2, C3, D, E | Native-State/Reorg/Recovery, C3 PAPER, Flap-Decoder, Four-Decoder | C2 PASS `f427cfa1`, C3 `cb9813c6`, D `d01ebd44`, E `531ddfcc` | **REVIEW_PASS** für eingefrorene technische Scopes · `STATUS.md` Pakete 4/5; Four verifiziertes ABI für Live weiter OPEN |
| T41 | Phase-Gates F, G, H, I | Journal/Replay, Economics, Strategy SHADOW, PAPER/Transport | F PASS `88cc92b6`; G PASS `5e862699`; H PASS `2d4c5503`; I PASS `3b6275e9` + `bfc239e6` | **REVIEW_PASS** für technische Scopes · `STATUS.md` Pakete 6/7; externe LIVE-Voraussetzungen offen |
| T42 | Phase F native Capture und Journal | Read-only WSS-Replay, Segmentierung, Recovery, Qualitätsklassifikation | E2E laut STATUS `baf5f0fa` PASS; Raw Evidence unter `bsc/native/evidence/f/live_capture_1793023912198/` | **INTEGRATION_PASS** (beobachteter Scope), **EXTERNAL_BLOCKED** Mainnet-geth/30-min native-Capture |
| T43 | Phase H 60-min Strategy SHADOW | Marktdaten, reproduzierbare Strategy-Entscheidungen | **201/201 Replay-Ausgaben PASS**, 192 Launches; 3 geschlossene PAPER-Positionen, **3/3 Verlust** | **LOCAL_PASS** Replay / **FAIL** Gewinn-Scope · `h/live_strategy_1793049942243/README.md` |
| T44 | G0.6 gleichzeitige PAPER-Launches, `BSC_C3_LATENCY_BURST` | k=1,2,4,8 künstlich gleichzeitige Launches, kein echter BUY | p99 letzte Position 258/521/1020/2020 µs; 4er- und 8er-Schwelle verletzt | **LOCAL_PASS** reproduzierbarer Test / **FAIL** synthetisches Ziel bei k≥4 · `g06/README.md` |
| T45 | P8 CPU/RSS 60 min | Read-only SHADOW Release-Runtime 720 Samples | Ø16,3 % eines Kerns; RSS 148→451 MB | **LOCAL_PASS** Beobachtung / **OPEN** echte Dauerlauf- und Produktionshardwarequalifikation · `p8/perf_runtime_60min_20261027/README.md` |
| T46 | BSC P8 Runtime Restart + Browser-/Journal-Verkettung | Launchd restart / neuer Run / C2 RESTORED / Strategy-Replay nach Browser-E2E | PASS dokumentiert am Release-SHA | **INTEGRATION_PASS** · `STATUS.md` + `ops/tests/bsc_control_test.py` |
| T47 | Anschließend berichtete zwei rote Browser-E2E-Läufe | Fehlerort und Fehlerlogs derzeit **nicht** dem Testregister zugeordnet | **nicht nachgewiesen behoben** | **OPEN / REPRODUCE**; der reale RPC-`--ignored`-Prestep kann vor dem Browser scheitern. Kein unbelegtes „RPC-Last“ oder „Browser-Bug“ behaupten |
| T48 | Neue Wallet- und wirtschaftliche Enable-/Disable-Controls + Tageslimit | Neues Auftragspaket 27.10., nicht Bestandteil des Release-Standes `8f26b02a` | **kein neuer Testbericht** | **NOT_RUN / OPEN** bis neuer nativer/CLI/Protobuf/Gateway/Frontend-Release-SHA und Abnahme |
| T49 | **Echter G0.7/Mainnet-BUY, G0.8-Akquisition, reales SELL, Profit und T0-Witness** | Wirtschaftlicher Produktionsnachweis | **kein Mainnet-BUY / keine echte Verkaufskette / kein T0-Witness** | **NOT_RUN / EXTERNAL_BLOCKED**, keinesfalls durch lokale PASS-Zahlen ersetzen |

**Kontrollsumme und Nicht-Doppelzählung:** T01 (**1.339**) umfasst die Rust-Suites seines spezifischen Commits; die älteren 426er/479er, G0.7-Guard-Suiten und C1-Fokus-Tests sind überwiegend **wiederholte/überschneidende Testfälle anderer Revisionen**. Sie dürfen **nicht** zu „mehreren Tausend einzigartigen Tests“ addiert werden. T02 (**74 Gateway**) und T03 (**113 Frontend**) sind zusätzliche getrennte Suiten. T04/T05 sind **drei ganze Browser-Durchläufe**, nicht drei einzelne Assertionen. T06 sind **2.603 Datenfelder**, keine 2.603 Tests. `ignored`, `SKIP`, `NOT_RUN`, fehlende Logs und bisherige negative Testfälle bleiben ausdrücklich sichtbar.

**Was noch fehlt, um dieses Testinventar als wirklich vollständig und aktuell abzunehmen:** Claude muss für die *neuesten* Quelländerungen alle vorhandenen lokalen Test- und CI-Ausgaben einschließlich der 22 `ignored`, externen RPC-Smokes, separaten Python-/Node-Tests, Native-/Gateway-/Frontend-E2E und neuen Wallet-/Limit-Controls gegen den **gleichen finalen Git- und Release-SHA** abgleichen. Pro Testfamilie exakt `command, cwd, exit_code, count_pass, count_fail, count_ignored, run_id, git_sha, binary_sha, config_sha, evidence_path, status` aufnehmen. Auf dem neueren Dokumentations-HEAD ohne solche Läufe **kein automatisch geerbter PASS**.

---

**Präzisierung 27.10.:** Die verbindliche aktuelle Nutzerentscheidung lautet **maximal 10 echte BUY-Versuche insgesamt (1 G0.7 + höchstens 9 weitere), nicht 1+10**; jeweils kleinster reproduzierbar qualifizierter BUY-Betrag und kumulatives Verlustbudget. `BSC_BUY_ATTEMPTS_PER_DAY` ist ein tägliches, **kein kampagnenweites** Limit. G0.7-Self-BUY und tatsächlicher LIVE-SELL sind unterschiedliche Fähigkeiten; Strategy-C-Exit und signierter PAPER-SELL beweisen keinen realen Exit. Details im neuesten Abschnitt von `PHASENPLAN.md`.

## 3. Fehlende Messungen – verbindliche PASS/FAIL/OPEN-Matrix

| ID | Konkrete Messung / Daten je Versuch | Ziel / PASS-Kriterium | Stand | Was genau noch gemacht werden muss |
|---|---|---|---|---|
| O01 | **Wallets und wirtschaftliche Controls** LAUNCHER/BUYER getrennt, persistiertes `DISABLED`, alle CLI-/UI-/Runner-Wege | Beide default DISABLED; bei deaktivierter zuständiger Wallet nachweislich **0 reale SIGN/SEND** auf jedem nativen Pfad, auch Restart/Race; Status und Receipt gleich in CLI und UI | **LOCAL PASS** (`b72bf2ed`, `86de25c9`: Owner-/Projektions-/Gateway-/Prozess-/DRY-/Browser-E2E inkl. Restart/Race/Crash; Reviewer offen); realer Scope OPEN | Wallet-Controls sowie fehlende Venue-/Runtime-/Wirtschaftscontrols nativ + Protobuf + Gateway + Frontend implementieren, vollständigen Test auf finalem SHA |
| O02 | **Tageslimit/Versuchszähler** pro wirtschaftlichem BUY-Versuch | Konfigurierbares Maximum (Beispiel **10/Tag**), Tagesgrenze und Zeitzone fest, jeder RELEASED/UNKNOWN-Versuch zählt, CAS/Crash kein Double Count, Limit=0 verweigert | **LOCAL PASS** (`86de25c9`: `BSC_BUY_ATTEMPTS_PER_DAY`, UTC-Tag, create-new-Slots, Retry/Neustart/Race/Tageswechsel-Tests, CLI+GUI-E2E 3→0); realer Scope OPEN | Native Owner/CLI/Wire/UI mit persistierten Tageszählern und Negativtests; ein Maximum ist niemals eine automatische Order |
| O03 | **Aktueller Flap/Four-Contract-Freeze** Chain-ID, Portal, ABI, Version, Runtime-Code, Selector, Tokenableitung | Identische überprüfte aktuelle Implementierung vor qualifizierender Signatur/BUY; Drift → STOP | **OPEN** | Flap v5.27.2 und fehlende Four-Quellen tatsächlich verifizieren; keine alte 5.24/5.27.0-Evidence als aktuell ausgeben |
| O04 | **Fremder Launch→BUY vollständiger Pfad** pro Venue/Quote/Creator-/Taxprofil | Für jeden gewählten Scope full signed Launch → validierte lokale Tokenadresse → gültiges BUY-Calldata/Quote → korrekter eigener pre-block BUY-Plan | **OPEN** | Nach 0/28-Miss qualifiziertes reales Fremdprofil auswählen; reproduzierbar G0.2/G0.3 + Replay + Status-/Reason-Codes nachweisen, ohne unqualifizierte Profile generell zuzulassen |
| O05 | **Ingress-Quellenvergleich** pro identischer Signed-TX | Je Quelle `first_hash`, `first_full`, `first_actionable`, signed identity, Quellenqualität, Drops, Lücken; schnelleres Ereignis erst nach kryptografischer Verifikation | **OPEN** | Maximal 2 h gezielt aus vorhandenen Quellen messen, vorhandene bereits gesammelte Daten auswerten; Quellen einzeln berichten, kein Hash-only-Event als BUY-ready |
| O06 | **Lokale Reaktions- und Submission-Latenz** `T_full→T_actionable→T_ready→T_queue→T_send` | Präzise Uhr-/Adapter-Grenzen, p50/p95/p99/max+Nenner; bisheriges allgemeines lokales Gate **<1 ms**, bevorzugt **<0,5 ms** bis *echtes* `T_send`; alles ≥1 ms untersuchen | **OPEN** | Finale produktive Runtime an echtem geprüften Adapter messen; PAPER-Sink-Zeit ist unzureichend; Prebuild/Presign-Anteile getrennt ausweisen |
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
| O17 | **Release-/Frontend-E2E auf finalem SHA** | Alle Pflichtprüfungen PASS, sämtliche wirtschaftlichen Controls, Wallets, Status-Gates und G0.8-`UNRESOLVED` identisch durch Native→Proto→Gateway→GUI | **LOCAL PASS** (`86de25c9`: Release-Build + Manifest PASS, BSC-Regression 85 Suites/1366/0 Fehler, Browser-E2E debug und release PASS; roter E2E vom Vormittag: G0.8-Schritt durch parallele PREPARE-Last auf demselben `bsc-dataseed`, nach Lastende 6× grün, Fehlerursache wird jetzt mit curl-Exitcode erfasst) | Exakte roten E2E-Logs/Endpoints/Zeiten prüfen; nicht als „nur Netz“ klassifizieren ohne Reproduktion. Nach neuer Control-Implementierung vollständigen Release-/Browser-Gate erneut ausführen |
| O18 | **Betriebsdatenqualität und Capture** | Pro Versuch lückenlose 15-s- bzw. vertraglich gebundene Capture, segmentierte Journale/Hashes/Provenienz und G0.8-Canonical-Reads; Lücke ⇒ INCOMPLETE/UNRESOLVED | **OPEN** (echter Versuch) | Snapshot-Run-ID, Release-/Config-SHA, Source-Quality, Gap/Drop/Reorg, Chain-Receipts, UTC/monotonen Clock-Kontext, Original Evidence und Fehlversuche versiegeln |
| O19 | **Kosten/Netz-/Hardware-Entscheidung** | Vergleich kostenfreier Ingress-/Inclusion-Wege gegen tatsächliches `T_send`, Datenqualität und Kosten; keine fixe Kaufpflicht für Hardware | **OPEN** | Nur bei belegter Notwendigkeit Mainnet-geth/Provider/Linux qualifizieren; Datum und Kosten getrennt vom BUY-Erfolgsverdikt, vor Ausgaben Nutzerentscheidung |
| O20 | **Gesamtwirtschaftlicher Strategie-Nachweis** | Positive *realisiert-netto* Ergebnisse unter dokumentierter Stichprobe, alle erfolgreichen/gescheiterten/unklaren Trades und reale Kosten. Numerischer Profit-/Max-Drawdown-Grenzwert noch **nicht verbindlich festgelegt** | **OPEN** | Erst nach kontrollierten BUY→SELL-Ergebnissen Strategie anhand echter Nettodaten bewerten; aus drei historischen negativen PAPER-Positionen keinen Produktions-PASS konstruieren |

## 4. Messschritte in richtiger Reihenfolge

| Schritt | Was genau tun? | Erwarteter Output / Fortschritt | Vor echter Mainnet-Aktion nötig? |
|---|---|---|---|
| **S0 – Iststand sichern** | Aktuellen Code-/Release-SHA und vorhandene G0.1–G0.6/P8/H-Evidence binden; keine Messung doppelt durchführen | Diese M01–M18-Matrix mit belastbarer Pfad-/Run-Identität; ältere PASS/FAIL unverändert | Ja |
| **S1 – Schalter-E2E** | Wallet LAUNCHER/BUYER und alle weiteren wirtschaftlichen Controls implementieren, Defaults DISABLED, CLI/Frontend-Projektion, Fallmatrix | O01/O02/O17 lokal PASS; 0 SIGN/0 SEND bei DISABLED | Ja |
| **S2 – Marktsichtung stoppen/auswerten** | Bestehende G0.4-Aufzeichnung nur geordnet beenden; bereits vorhandene Daten einmal pro Source/Profil auswerten; maximal 2 h als neuer Explorationsrahmen | Zeitfenster, tatsächliche Quellenverfügbarkeit, vollständige Signed-TX-Anteile, fremde BUY-Positionen; unzureichender Nenner NOT_EVALUABLE | **Keine Wartepflicht** für einen einzelnen G0.7-Kauf |
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
| F: exakt ein **30-minütiges cleanes, natives geth**-Capture, Binärmanifest, Gap/Overflow/Quality/Replay | OPEN / EXTERNAL_BLOCKED laut STATUS; max. 2h G0.4 ist getrennt |
| F: Peer-Decode→Rust→Recorder p50/p95/p99/p99.9/p99.99/max, Schwellen 0.5/1/3 ms | PARTIAL / finaler echter Ingress-Gate OPEN |
| F/I: Journal Corruption, Capacity, Replay, Restart, Reorg, disk-full nur isolierter Testträger | Historische Teil-PASS, vollständiges Final-SHA-Mapping OPEN |
| G: Quote/fee/tax/rounding/revert/BUY/SELL/approval/permit/replace/migration | Historische Teil-PASS, alle unterstützten Profile einzeln OPEN |
| H: wiederholbarer Strategy-Core, valider Exit und Risk, echte wirtschaftliche Verluste dokumentieren | Replay PASS 201/201; PAPER-Wirtschaft 3/3 Verluste |
| I: gleiche Rust-Core-Pfade in PAPER/SHADOW/LIVE, Egress- und Senderkonkurrenz, finaler Adapter `T_send` | Teil-PASS PAPER; echtes Live-`T_send` OPEN |
| J: 999999 vs. 1000000 ns, Unknown Producer, SEND ohne Erwerb, schlechte Platzierung, fehlende 15-s-Capture, Reorg/Spät-Erwerb, Zero-Attempt und verändertes Manifest | Prescribed, Fall-zu-Test-Mapping OPEN |
| F0-BSC/UI: Native→Protobuf→Gateway→React, Proto Golden U256/signed PnL, CAS/Receipt, Crash, Five Tabs, View Filters, A11y, No Poll, Pixeldiff | Historischer E2E PASS; neue Wallet Controls und Pixeldiff OPEN/NOT_RUN |
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
4. **Endstand noch nicht abgenommen:** Release-PASS bezieht sich auf `8f26b02a`, nicht auf die zukünftige Wallet-/Economics-/Daily-BUY-Control-Version. `LATEST_FINAL_SHA_FULL_GATE=NOT_RUN`.
5. Pro Test zwingend: `owner / test_case / required_scope / source_file / command / git_sha / run_id / result / exit_code / sample_count / evidence_sha256 / limitation`. Ohne Originaldaten `SOURCE_UNVERIFIED`, nicht angenommener PASS.

**Fazit:** Dieses Register enthält jetzt einen deutlich vollständigeren versionierten Quell- und Pflichtenindex. Es ist **noch keine nachweislich vollständige Historie sämtlicher jemals gestarteter Testprozesse**. Diese Differenz ist ausdrücklich offen und bei der nächsten lokalen Claude-Übergabe gegen unverversionierte Artefakte zu schließen.
