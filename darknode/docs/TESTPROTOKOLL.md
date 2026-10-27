# DarkNode – ZENTRALES TESTPROTOKOLL

**Version:** 1.0 · **Anlage:** 27.10.2026 · **Status:** Testregister angelegt, DarkNode-Produkt-/Integrationstests nicht ausgeführt · **GATE 1: OPEN / LIVE: GESPERRT**.

**Verbindliche zentrale Übersicht für alle P0–P10-Tests, inklusive M1, P1/P5-Spezialmatrizen, Protobuf, Apple M4, Wallet, Backtest, UI, Betrieb, Fehleranalysen und künftige Regressionstests.** Die Detail-Assertions bleiben in den jeweiligen Phasenplänen; Teststatus, Ergebnis, Quellstand, Evidence und fortlaufender Verlauf werden hier geführt. Keine Eigenentwicklung einer parallelen Trading-/Test-Engine durch diese Dokumentation. Dieses Dokument ist **kein Testlauf**.

## 1. Status – genaue Bedeutung

| Status | Verbindliche Bedeutung |
|---|---|
| **PASS** | Test tatsächlich ausgeführt, alle festgelegten Assertions bestanden; Git-SHA, Test-Command, Exitcode und überprüfbare Evidence vorhanden. |
| **FAIL** | Ausgeführter Test hat eine Assertion verletzt. Fehlerbeobachtung, Reproduktionsweg und Fix-Verweis sind Pflicht. |
| **PARTIAL** | Nachweis ausschließlich für den ausdrücklich beschriebenen Teilumfang. Kein Gesamt-PASS. |
| **OPEN** | Fachliche Frage oder Test-Design noch nicht geschlossen. Kein PASS. |
| **NOT_RUN** | Test spezifiziert, aber im aktuellen DarkNode-Implementierungsstand nicht ausgeführt. Auch wenn die Implementierung fehlt. |
| **BLOCKED** | Test durch **konkreten dokumentierten** technischen/externen Blocker nicht ausführbar; Blocker und Auflösung nennen. Keine Pauschal-Zuordnung. |
| **REUSED_EVIDENCE** | PASS/Testmessung liegt **bei BSC oder Project_0** vor und ist nur für den angegebenen unveränderten Hardware-/Baustein-Scope gültig; keine DarkNode-Funktionsfreigabe. |
| **FUTURE_ONLY** | Erst für spätere Phase/Ereignis relevant, beispielsweise Mac-mini-Migration. |
| **REQUALIFICATION_REQUIRED** | Früherer PASS ist als Historie erhalten; Änderung am relevanten Code/Schema/Host/Policy verlangt neuen Test am aktuellen Stand. |
| **WAIVED** | Ausnahme nur mit dokumentierter, autorisierter Risikoentscheidung; ein zwingendes Safety-/LIVE-Gate darf nicht still umgangen werden. |

**Wichtig:** `SPECIFIED` oder `DESIGN_FIXED` sind Planungsurteile, **keine** bestandenen Tests. `UNKNOWN`-Konto-/Stop-/Orderzustand ist kein erfolgreiches Testergebnis. Die alte öffentliche P1-Python-Zusammenfassung „16 PASS, 23 BLOCKED, 1 NOT_RUN“ darf **nicht** als bestandene Rust-P1-Abnahme oder 23 einzeln überprüfte Blocker gezählt werden. 

## 2. Pflichtfelder und Testlauf-Verlauf

**Eine unveränderliche ID pro Testfall**, niemals ohne dokumentierte Ablösung neu belegen. Ursprüngliche IDs aus den Phasenplänen beibehalten: `T01–T27`, `K01–K15`, `D01–D08` ausschließlich im P1-Namespace; `P5-A01–A28`, `C00–C16` ausschließlich P5; `DN-PROTO-001–008`, `DN-MAC-01–09`. Jeder neu entstehende Test wird hier und im zuständigen Detailplan erfasst, **bevor** er als bestanden bezeichnet werden darf.

**Jede neue Ausführung als zusätzlichen Eintrag in §6 anhängen**, auch bei Wiederholung und Fehler. Alte Resultate niemals überschreiben; in den Matrizen nur `Letztes Ergebnis/Run-ID` aktualisieren. Pflichtfelder:

| Feld | Erforderlicher Inhalt |
|---|---|
| `run_id` | Einmalige ID, z. B. `DN-YYYYMMDDTHHMMSSZ-P5-A08-01` |
| `test_id`, `phase/lane` | Ursprüngliche Test-ID, P1 Public, P5 Offline, Demo READ, autorisierte Demo WRITE, P8 usw. |
| `source_sha`, `dirty_diff_hash` | Exakter 40-stelliger Commit und ggf. lokaler Dirty-Zustand |
| `host/env` | Mac-Fingerprint, Build/Toolchain/Features, Power-/Netzprofil, Paper/Demo/LIVE-Umgebung |
| `inputs/fixtures` | Originaldaten-/Capture-IDs, Hash, Zeit-/Zufalls-Seed und Voraussetzungen |
| `command`, `exit_code` | Reproduzierbarer tatsächlicher Aufruf, Resultat des Testprozesses |
| `expected` | Vor Teststart festgelegte Assertionen und Grenzwerte |
| `observed` | Konkrete Beobachtungen, Mengen, Fehler, Messwerte und Abweichungen |
| `result`, `reason` | PASS/FAIL/PARTIAL/BLOCKED/NOT_RUN und nachvollziehbare Begründung |
| `evidence` | Unveränderlicher Artefaktpfad/URL und SHA-256 der Logs/Captures/Reports |
| `owner`, `reviewed_by`, `next_action` | Verantwortliche, Reviewdatum und Maßnahme für offene Ergebnisse |

**Sicherheitsregel:** keine API-Keys, Bitget-Passphrasen, privaten Kontodaten oder ungeschwärzten Secrets in Testevidenz oder Git. Demo-Write/LIVE nur mit entsprechender expliziter Freigabe; reine Dokumentationsupdates erteilen keine Berechtigung.

**Arbeitsregel bei jedem Entwicklungs-/Bugfix-Paket:** betroffene IDs bestimmen → ggf. neue ID + Solltest aufnehmen → Test auf exakt benanntem SHA ausführen → Beleg/Resultat anhängen → Statusfeld/Run-ID synchronisieren → späteren Reviewer und eventuellen Requalifizierungsbedarf eintragen. Ein FAIL wird bei erfolgreicher Regression **nicht gelöscht**. Wenn ein Check nicht gelaufen ist: `NOT_RUN`, niemals `PASS`. Bestehende Quelltests von P0/BSC müssen nicht doppelt ausgeführt werden, aber ihre Übernahme in neue DarkNode-Aufrufpfade erfordert gezielte eigene Integrationstests.

**Spätere Automatisierung:** Runner sollen strukturierte Ergebnisdateien mit Test-IDs, Hashes und Exitcodes erzeugen; ein Validator prüft die Register-/Evidence-Konsistenz. Aktuell existiert dafür **kein fertiger DarkNode-Runner und keine Status-Synchronisierung**. Bis zu dessen Implementierung ist dieses Markdown die verbindliche, manuell versionierte Ergebnisübersicht; keine erfundenen Automations-PASS.

## 3. Phasen-/Gesamtstatus bei Anlage

| Gate/Phase | Status | Ergebnis oder nächster Nachweis |
|---|---|---|
| GATE 1 | **OPEN** | Research und dokumentierte Exchange-/Indikator-/Governance-Nachweise noch nicht vollständig abgenommen |
| P0 | NOT_RUN | Native/Proto-Workspace, Upstream-/Capability- und vollständige Cross-Project-CI-Verträge |
| M1-A | NOT_RUN | Bitget Public Trades → Rust Bar/Indikator/NO_SEND Risk → vorhandenes Gateway → UI |
| M1-B | NOT_RUN | **Nur ausdrücklich autorisierte** Bitget-Demo-Order mit Fill, Exchange-Stop und Wallet-Reconcile |
| P1 | **NOT_RUN** | Public API Research PARTIAL; T01–T27, K01–K15, D01–D08 nicht auf DarkNode-Rust abgenommen |
| P2 | NOT_RUN | History→Live/Warmup-Cutover, keine Phantom-Signale |
| P3 | NOT_RUN | mathematisch eigenständige Rust-Indikatoren/GraphCompiler/Bool3, keine Pine-Referenz |
| P4 | NOT_RUN | gemeinsamer Rust-Kern, Backtest-Fills, Fees, Funding, Lookahead, OOS |
| P5 | **NOT_RUN** | S0–S7, A01–A28, Crash C00–C16, DN-P5-ALL-001 |
| P6 | NOT_RUN | Paper/Demo E2E mit echten Readbacks und bestätigtem FLAT/Rearm |
| P7 | NOT_RUN | genau fünf UI-Reiter, Rust↔Proto↔Gateway↔React, Wallet, Builder |
| P8 | NOT_RUN | integrierte Fault-, Security-, Latenz-, OOS- und 24/7-Abnahmen |
| P9 | **NOT_RUN / LIVE GESPERRT** | separate Echtgeldfreigabe und Pilot-Nachweise fehlen |
| P10 | FUTURE_ONLY | Bot-Fleet unter einem globalen Risk-/Order-Owner |
| DN-PROTO | NOT_RUN | 8 Domain-/Schema-/UI-Prüfungen vorgesehen |
| DN-MAC | **PARTIAL / REUSED_EVIDENCE** | M4-Hardwaretest aus BSC belegt; DarkNode-spezifische Punkte noch nicht getestet |

## 4. Zentrale Testmatrix – Einzeltests

Alle Produkttests sind bei Anlage **NOT_RUN**, sofern eine Zeile nicht ausdrücklich eine **Fremdevidenz** nennt. `—` bedeutet, dass **kein DarkNode-Testlauf** dokumentiert ist. Fachliche Detailerwartung jeweils in der verlinkten Originaldatei. Eine Test-ID ist nur innerhalb der genannten Phase eindeutig; Referenz `P1/T01` ≠ `P5/A01`.

### P1 – technische Regressionstests

Quelle: [PHASENPLAN_P1_V1.md](PHASENPLAN_P1_V1.md).

| Test-ID | Verbindlicher Prüffall | Status | Letztes Ergebnis / Run-ID |
|---|---|---|---|
| `T01` | Multi-Trade SBE | NOT_RUN | — |
| `T02` | Unbekanntes Template/Version | NOT_RUN | — |
| `T03` | Depth50 Delta vor Snapshot | NOT_RUN | — |
| `T04` | Sequenz-Lücke / Reset | NOT_RUN | — |
| `T05` | Text-Ping vergessen | NOT_RUN | — |
| `T06` | Replay gleiche ID, gleicher Payload | NOT_RUN | — |
| `T07` | Gleiche ID, anderer Payload | NOT_RUN | — |
| `T08` | 999ms / 1000ms Trade | NOT_RUN | — |
| `T09` | Verspäteter Trade | NOT_RUN | — |
| `T10` | No-Trade-Intervall | NOT_RUN | — |
| `T11` | Crash bei L0-Record/Index/SQLite-Watermark | NOT_RUN | — |
| `T12` | Writer-Ring voll / Disk full | NOT_RUN | — |
| `T13` | Download-Import doppelt | NOT_RUN | — |
| `T14` | Öffentlicher REST liefert keine Historie | NOT_RUN | — |
| `T15` | Leeres API-Fenster vs Timeout/429 | NOT_RUN | — |
| `T16` | Zwei Charts/TFs gleichzeitig | NOT_RUN | — |
| `T17` | Reconnect beim History-Paging | NOT_RUN | — |
| `T18` | Voranstellen bei vorhandenen Live-Bars | NOT_RUN | — |
| `T19` | Browser komplett geschlossen | NOT_RUN | — |
| `T20` | SQLite gegen Binärdateien nach Kill inkonsistent | NOT_RUN | — |
| `T21` | Nicht erlaubter Host/URL | NOT_RUN | — |
| `T22` | Neuer Instrument-Snapshot / Step-Änderung | NOT_RUN | — |
| `T23` | Replay vs Live identischer Capture | NOT_RUN | — |
| `T24` | 7-Tage-Dauerlauf | NOT_RUN | — |
| `T25` | Historische Trades selbst aggregieren | NOT_RUN | — |
| `T26` | Gleichzeitige-ms-Tradereihenfolge | NOT_RUN | — |
| `T27` | Exakte Bar-Komposition | NOT_RUN | — |

### P1 – Qualitäts-/Leistungskriterien

Quelle: [PHASENPLAN_P1_V1.md](PHASENPLAN_P1_V1.md).

| Test-ID | Verbindlicher Prüffall | Status | Letztes Ergebnis / Run-ID |
|---|---|---|---|
| `K01` | Vollständige Frameverarbeitung | NOT_RUN | — |
| `K02` | Dedupe | NOT_RUN | — |
| `K03` | Coverage | NOT_RUN | — |
| `K04` | Replay-Determinismus | NOT_RUN | — |
| `K05` | Datenintegrität | NOT_RUN | — |
| `K06` | Erstes Chart-Bild | NOT_RUN | — |
| `K07` | Rückwärtsseite | NOT_RUN | — |
| `K08` | BTCUSDT-Tag Replay | NOT_RUN | — |
| `K09` | Writer unter Live-Last | NOT_RUN | — |
| `K10` | Storagegröße | NOT_RUN | — |
| `K11` | Historische Rohtrades → eigene 1s-/MTF-Candles | NOT_RUN | — |
| `K12` | Gateway-Integrität | NOT_RUN | — |
| `K13` | 24/7-Betrieb | NOT_RUN | — |
| `K14` | No JSON intern | NOT_RUN | — |
| `K15` | Hardcode-Prüfung | NOT_RUN | — |

### P1 – Definition-of-Done-Gates

Quelle: [PHASENPLAN_P1_V1.md](PHASENPLAN_P1_V1.md).

| Test-ID | Verbindlicher Prüffall | Status | Letztes Ergebnis / Run-ID |
|---|---|---|---|
| `D01` | Öffentliche Bitget-Daten laufen ohne API-Key in einem Rust-Dienst weiter, auch wenn kein Browser verbunden ist. | NOT_RUN | — |
| `D02` | Alle tatsächlich vom Decoder angenommenen Frames/Events sind gezählt, typisiert, per Herkunft verfolgbar und gemäß dokumentierter Persistenzgrenze recoverbar; k | NOT_RUN | — |
| `D03` | L0-L2, Zeitindex, SQLite-Metadaten, Versionen und Crash-Recovery funktionieren konsistent; ein Datenverlustfenster ist begrenzt und sichtbar. | NOT_RUN | — |
| `D04` | 1s-Bars und benötigte höhere Auflösungen sind zeitlich korrekt; Replay desselben fixierten Eingangsdatensatzes mit derselben Policy liefert identische Ergebniss | NOT_RUN | — |
| `D05` | Historienquellen, verfügbare Zeiträume, unbehebbare Lücken und Konflikte sind im Coverage-Ledger nachvollziehbar; kein Datenbereich wird ohne Beleg als vollstän | NOT_RUN | — |
| `D06` | Mehrere Charts laden über das bestehende Gateway ältere Seiten mit exklusivem Cursor, ohne doppelte Bars, Scrollsprung oder zusätzliche Bitget-Lanes. | NOT_RUN | — |
| `D07` | Intern keine JSON-Nachrichten, -Dateien oder -Konfigurationen; unveränderte externe Bitget-JSON-Frames dürfen ausschließlich als opake Rohbytes in L0 liegen. | NOT_RUN | — |
| `D08` | Integrierte Mess-, Fehler- und Dauerlauftests bestanden; Evidenz an einem fixierten Git-Commit; keine Mocks oder Scheinwerte als Laufzeitnachweis. | NOT_RUN | — |

### P5 – Bitget-/Safety-Abnahmetests

Quelle: [PHASENPLAN_P5_V1.md](PHASENPLAN_P5_V1.md).

| Test-ID | Verbindlicher Prüffall | Status | Letztes Ergebnis / Run-ID |
|---|---|---|---|
| `P5-A01` | Full USDT-Futures-Instruments ohne symbol → valide dynamische Auswahl; stillgelegtes Paar und Formatänderungen blockiert | NOT_RUN | — |
| `P5-A02` | Account/UTAmode/read permissions/holdMode/marginMode/actualLeverage/risktiers aus dem echten Demo-Account mit dokumentierten Rückgabedaten erkannt | NOT_RUN | — |
| `P5-A03` | 5× als Beispiel: pre-set-leverage lesen, autorisiertes set-leverage auf Demo, settings erneut lesen; qualifizierten V1-Margin-/Hold-Modus positiv, nicht fr | NOT_RUN | — |
| `P5-A04` | Mode-Wechsel nur bei Flat+keinen offenen Orders; ablehnender Exchange-Response darf keine Readiness vortäuschen | NOT_RUN | — |
| `P5-A05` | Buy/sell/posSide/reduceOnly nur für den per DN-D08 freigegebenen V1-Mode positiv; andere One-way-/Hedge-Modi ausdrücklich erkennen und ablehnen; Positionsm | NOT_RUN | — |
| `P5-A06` | Zulässige V1-Ordertypen (Market/Limit und nach Capability ggf. Post-only) positiv; nicht zugelassene IOC/FOK/andere Typen fail-closed; Cancel/Modify-ACK ge | NOT_RUN | — |
| `P5-A07` | Zulässiges initiales Exchange-SL/TP positiv; nicht zugelassene OCO/Exchange-Trailing-Varianten ablehnen; Trigger, Stop-Versagen, Partial-Fill, Cancel/Repla | NOT_RUN | — |
| `P5-A08` | Lost ACK, Doppelsignal, reconnect, Gateway-Close, REST/WS Konflikt, Reboot mit Position + ClientOid-Requery → keine Doppelorder | NOT_RUN | — |
| `P5-A09` | Margin/Leverage-Änderungen, Order-Rate-Limits, UID/IP-Limits, Signatur-/Timestamp-/Clock-Fehler und 429; fail-closed | NOT_RUN | — |
| `P5-A10` | Secrets/IP-Allowlist, keine Withdrawal-Rechte, kein Frontend-Key, getrenntes Demo/Live, SBE-Hot-Path ohne interne JSON-Verträge | NOT_RUN | — |
| `P5-A11` | Echtes Risiko-/Kontobudget je Bot, globale UID-Exposures, Liquidations-/Funding-/Fee-Deltas, Kill-/Emergency-Policies | NOT_RUN | — |
| `P5-A12` | Demo-Reconciliation bis bestätigtem FLAT ohne unbestätigten Schutz-/Restorderzustand | NOT_RUN | — |
| `P5-A13` | API-fähiges echtes Bot-Unterkonto, getrennte Mittel, keine automatischen Transfers, Rechte/IP/Keys/Isolated dokumentiert; Verlustgrenzen ohne falsche Margi | NOT_RUN | — |
| `P5-A14` | Nominal-/Mengen-/Bot-/UID-Limits vor jeder Order inklusive offener/UNKNOWN/Partial Exposure und Hebelabweichung – nie nur Margin; Graph/Gateway kann Risk n | NOT_RUN | — |
| `P5-A15` | FLAT ohne offene Orders: pre-set-leverage → set-leverage → settings/Position readback; falscher Hebel/Mode/PosSide blockiert Entry und meldet HALTED | NOT_RUN | — |
| `P5-A16` | Nach jedem Fill Bitget-Liquidationspreis/Markpreis/MMR/Leverage prüfen, konservativen Long-/Short-Stop-Puffer validieren; zu wenig Puffer → sichere Verklei | NOT_RUN | — |
| `P5-A17` | Preset-SL im freigegebenen V1-Mode für aktivierte Entry-Typen real auf Demo testen; ungeprüfte Hedge/Cross-Kombinationen negativ ablehnen. Server-Schutzsta | NOT_RUN | — |
| `P5-A18` | Teilfill: Stopmenge exakt gegen bestätigte Position verifiziert; Zeit bis wirksamen Schutz und mögliches ungeschütztes Intervall real gemessen, keine Sofor | NOT_RUN | — |
| `P5-A19` | Stop fehlt/wird abgelehnt/manuell gelöscht: UNKNOWN → qualifizierter Ersatzstop → wenn nötig kontrollierter Emergency-Close → FLAT/Reconcile oder UNKNOWN+A | NOT_RUN | — |
| `P5-A20` | Periodischer und Reconnect-/Restart-Stop-Audit mit REST+Private WS erkennt falsche Menge/Trigger, externe Löschung/Neuanlage und Positionsänderung | NOT_RUN | — |
| `P5-A21` | Hummingbot-Race-Regression: FAILED Close/Timeout → Late Fill → kein zweiter Close; persistierte clientOid und echter Exchange-Status vor Retry | NOT_RUN | — |
| `P5-A22` | Signalende nach Market-Send bzw. Cancel-Pending bei Limit mit Late Partial Fill: ACK/Fills/Stop bleiben verfolgt; Cancelfehler bleibt UNKNOWN | NOT_RUN | — |
| `P5-A23` | ACTIVE/REDUCING/HALTED plus enger EMERGENCY_RECOVERY-Pfad: HALTED normale Orderdeny; Emergency reduziert nur belegte Restmenge, kein Entry/Flip/Bypass | NOT_RUN | — |
| `P5-A24` | GUI-HALT vs Emergency-Close getrennt; Browser/Gateway down beeinträchtigt autonomen Stop-Audit/Alarm nicht; ACTIVE nur nach vollem Readback | NOT_RUN | — |
| `P5-A25` | Bitget countdown-cancel-all erst bei genehmigtem Zugang und wenn Demo-Test bei Host-/Heartbeat-Ausfall Schutz-TP/SL/Strategy unangetastet belegt; sonst DEF | NOT_RUN | — |
| `P5-A26` | Kaltstart und Account-/App-Drift: Key/Account/Mode/Hebel/Symbol/Orders/Fills/Position/Stops/Clock/WS validiert, jede Abweichung blockiert Entry | NOT_RUN | — |
| `P5-A27` | Crash-/Netzsplit an Persistenz-/Send-/ACK-/Fill-Grenzen inkl. externer Orders und Positionsänderung: kein Phantom-FLAT, kein doppelter Fill/Close | NOT_RUN | — |
| `P5-A28` | Stop-Limit bei Preisgap, Mark/Market-Trigger, Funding, Liquidation, Gebühren/Slippage – fail-closed und keine behauptete Fill-Garantie | NOT_RUN | — |

### DN-PROTO – Rust↔Protobuf↔Frontend

Quelle: [DN_PROTO_CONTRACT_V1.md](DN_PROTO_CONTRACT_V1.md).

| Test-ID | Verbindlicher Prüffall | Status | Letztes Ergebnis / Run-ID |
|---|---|---|---|
| `DN-PROTO-001` | Eindeutige ProjectId-/TopicId-Registrierung, keine Cross-Project-Owner/Topic- oder Namespace-Vermischung | NOT_RUN | — |
| `DN-PROTO-002` | Generator/Validator aus Proto-Descriptor + Schema-Metadaten; kein unbeschriebener öffentlich verwendeter Parameter/Feld/Command, begründete Ausschlüsse | NOT_RUN | — |
| `DN-PROTO-003` | Bestehendes protocol/scripts/check.sh, buf lint/breaking gegen echten Vorläufer-SHA, Codegen Rust/protobuf-es, Schema-ID/Golden-Versionen reproduzierbar | NOT_RUN | — |
| `DN-PROTO-004` | gateway/src/topics.rs und frontend/src/core/realtime/topicTable.ts projektscharf 1:1 zu Proto-IDs, Klassen, Payload-Typen und Availability; negative Owner- | NOT_RUN | — |
| `DN-PROTO-005` | Rust↔Protobuf↔TS-Goldens für IDs jenseits JS-safe-integer, exakte Geld-/Kontrakt-Einheiten, Presence/Unknown, Node-Parameter, Commands/Receipts | NOT_RUN | — |
| `DN-PROTO-006` | Producer-/Wire-/Adapter-/Store-/UI-Abdeckung durch echte Tests, nicht nur kommentierte Pfade, Status je tatsächlich verifizierter Feature-Gruppe; kein verg | NOT_RUN | — |
| `DN-PROTO-007` | Typed Commands, ExpectedVersion/CAS, Autorisierung, NO_ENTRY/Risk-Owner, Idempotenz/Restart, keine unbekannte Mutation/kein Frontend-Key | NOT_RUN | — |
| `DN-PROTO-008` | End-to-End + Negativfälle: Gateway→UI-Wert, stale/null, fremde ProjectId, Schema-Version, Reconnect, Fill/Stop-Reconcile, DOM-Placement und fehlender Publi | NOT_RUN | — |

### DN-MAC – M4-Host und DarkNode-spezifische Betriebsprüfungen

Quelle: [APPLE_SILICON_M4_HOST_UND_OPTIMIERUNG_V1.md](APPLE_SILICON_M4_HOST_UND_OPTIMIERUNG_V1.md).

| Test-ID | Verbindlicher Prüffall | Status | Letztes Ergebnis / Run-ID |
|---|---|---|---|
| `DN-MAC-01` | Host | REUSED_EVIDENCE (BSC, nur Hardware) | BSC C1.6 11/11 PASS (Mac16,6/M4 Max/128 GiB/12P+4E); DarkNode-Integration nicht getestet |
| `DN-MAC-02` | Rust-Grundtechnik | NOT_RUN | Project_0-/BSC-Code vorhanden, DarkNode-Integration NOT_RUN |
| `DN-MAC-03` | M4 Scheduling | NOT_RUN | P0 P6-MT.12 DESIGN_FIXED; BSC Host-QoS REUSED; DarkNode-Threadpfad NOT_RUN |
| `DN-MAC-04` | APFS | NOT_RUN | P0 M4/APFS-Baseline: Append p99 24,7 µs; fsync p99 5,31 ms; DarkNode-Intent NOT_RUN |
| `DN-MAC-05` | Power/Lid/Netz | NOT_RUN | — |
| `DN-MAC-06` | launchd/Secrets | NOT_RUN | BSC launchd LaunchAgent SIGKILL/Restart PASS, kein DarkNode-Stop-/Secret-Gate |
| `DN-MAC-07` | externe Alarmierung/Restore | NOT_RUN | — |
| `DN-MAC-08` | Bitget-Fill/Stop/Latenz | NOT_RUN | — |
| `DN-MAC-09` | Hostmigration | FUTURE_ONLY | erst bei tatsächlicher Hardwaremigration |

### P5 – Crash- und Fehler-Injektionspunkte

Quelle: [P5_ORDER_STATE_UND_CRASH_MATRIX_V1.md](P5_ORDER_STATE_UND_CRASH_MATRIX_V1.md), §4. Originalbezeichnungen `C00–C16`, im gesamten Register als `P5/C00–P5/C16` eindeutig adressiert.

| Test-ID | Auslösende Crash-/Fehlergrenze | Status | Letztes Ergebnis / Run-ID |
|---|---|---|---|
| `C00` | vor Reservation | NOT_RUN | — |
| `C01` | nach Reservation vor Journalwrite | NOT_RUN | — |
| `C02` | nach Write vor fsync | NOT_RUN | — |
| `C03` | nach fsync vor Netzwerk-Submit | NOT_RUN | — |
| `C04` | während Netzwerk-Write | NOT_RUN | — |
| `C05` | nach Write vor ACK | NOT_RUN | — |
| `C06` | nach ACK vor privatem Fill-Event | NOT_RUN | — |
| `C07` | Teilfill vor Stopbestätigung | NOT_RUN | — |
| `C08` | Modify-Send vor Stop-Readback | NOT_RUN | — |
| `C09` | Cancel-Pending vor Late Fill | NOT_RUN | — |
| `C10` | Emergency-Close vor Readback | NOT_RUN | — |
| `C11` | Exchange-ADL/Fremdtrade ohne eigenen Order-Event | NOT_RUN | — |
| `C12` | Neustart ohne lokales Journal | NOT_RUN | — |
| `C13` | Backup-Restore mit älterem Snapshot | NOT_RUN | — |
| `C14` | Key-Revoke | NOT_RUN | — |
| `C15` | WS/REST-Failover nach unbekanntem Send | NOT_RUN | — |
| `C16` | Rate-Limit-/429-/Time-Drift | NOT_RUN | — |

### Weitere Phasen-/Integrationsgates ohne eigene vollständige Einzeltestmatrix

| Gate-ID | Abnahme | Status | Letztes Ergebnis/Quelle |
|---|---|---|---|
| `DN-P5-ALL-001` | Gemeinsamer P5-A01–A28-Runner inkl. Offline/Fault/Public/Demo-Read/autorisiertem Demo-Write/Soak | NOT_RUN | [P5 Gesamttestplan](P5_OPEN_SOURCE_CODE_AUDIT_AND_TESTPLAN_V1.md) |
| `M1-A` | Erster rein lesender Rust→Protobuf→Gateway→Frontend-Durchstich | NOT_RUN | [Gesamtphasenplan](PHASENPLAN.md) |
| `M1-B` | Nach Sicherheitsgates gesondert autorisierter Bitget-Demo-Orderzyklus | NOT_RUN | [Gesamtphasenplan](PHASENPLAN.md) |
| `P1.1–P1.10` | Zehn P1-Paketabnahmen, inklusive P1.10 7-Tage-Betrieb | NOT_RUN | [P1](PHASENPLAN_P1_V1.md), §7.1 |
| `P2` | Warmup/Handover/Restart | NOT_RUN | [P2–P4](P2_P3_P4_DETAILVERTRAG_V1.md) |
| `P3` | Compiler/Nodes/Bool3/Signale mathematisch unabhängig verifiziert | NOT_RUN | [P2–P4](P2_P3_P4_DETAILVERTRAG_V1.md) |
| `P4` | Replay/Fillmodel/Kosten/OOS | NOT_RUN | [P2–P4](P2_P3_P4_DETAILVERTRAG_V1.md) |
| `P6` | Paper-/Demo-E2E, Reconnect, mehrfacher Entry/Exit | NOT_RUN | [P6–P8](P6_P7_P8_QUALIFIKATION_V1.md) |
| `P7` | Builder/Backtest/Live/Wallet/Overview und Binding-Goldens | NOT_RUN | [P6–P8](P6_P7_P8_QUALIFIKATION_V1.md) |
| `P8-R` | Replay | NOT_RUN | [P6–P8](P6_P7_P8_QUALIFIKATION_V1.md) |
| `P8-S` | Safety | NOT_RUN | [P6–P8](P6_P7_P8_QUALIFIKATION_V1.md) |
| `P8-W` | Wallet/Wire | NOT_RUN | [P6–P8](P6_P7_P8_QUALIFIKATION_V1.md) |
| `P8-L` | Latenz und Host-Bursts | NOT_RUN | [P6–P8](P6_P7_P8_QUALIFIKATION_V1.md) |
| `P8-O` | Operations/Restore/Alarme | NOT_RUN | [P6–P8](P6_P7_P8_QUALIFIKATION_V1.md) |
| `P8-E` | Netto-Wirtschaftlichkeit, OOS, Maker/Taker/Funding | NOT_RUN | [P6–P8](P6_P7_P8_QUALIFIKATION_V1.md) |
| `P9` | Formaler Echtgeldpilot mit menschlichem Sign-off | NOT_RUN | [P9/P10](P9_P10_FREIGABE_UND_FLEET_V1.md) |
| `P10` | Multi-Bot-/Exposure-/Owner-Regressionsgates | FUTURE_ONLY | [P9/P10](P9_P10_FREIGABE_UND_FLEET_V1.md) |

**Neue Einzeltests in P2–P4/P6–P10:** erste tatsächliche Assertion erhält eine stabile neue `DN-P2-001`-, `DN-P3-001`-, `DN-P4-001`-, `DN-P6-001`-, `DN-P7-001`-, `DN-P8-001`- oder `DN-OPS-001`-Kennung. **Nicht im Voraus hunderte erfundene Cases vergeben** und die vorhandenen P1-/P5-IDs nicht umnummerieren. Jede weitere Prüfanforderung kommt sowohl in den Phasentestplan als auch hierher.

## 5. Bisherige Research- und Fremdprojekt-Ergebnisse – abgegrenzt von DarkNode-PASS

| Evidence-ID | Beobachtetes Ergebnis | Originalbeleg | DarkNode-Bewertung |
|---|---|---|---|
| `DN-RESEARCH-P1-PUBLIC-20261027` | `PUBLIC_API_SPIKE=PASS` nur für getesteten öffentlichen Bitget-Scope; Gesamt-Forschung `R2=PARTIAL`. Ein Wrapper zeigte **16 PASS, 23 BLOCKED, 1 NOT_RUN**; die 23 wurden ohne individuelle P1-T-Ausführung pauschal vergeben. | [P1-Gesamtaudit](P1_EVIDENCE/DARKNODE_P1_GESAMTAUDIT_2026-10-27.md) | **PARTIAL**, `P1/T01–T23 = NOT_RUN` auf Rust. Kein LIVE-Netztest in diesem Register durchgeführt. |
| `DN-RESEARCH-V5-20261027` | 60s-Demo-V5-Parser-Sample: **8/62** Depth50-Frames durch experimentellen Python-Parser mit `ValueError('depth group')` verworfen. | [P1 V1.3 §V5](PHASENPLAN_P1_V1.md) | **FAIL im Python-Parser-Scope**, Ursachenklärung OPEN; **nicht** „Bitget hat Frames verloren“. Rust-V5-Decoder NOT_RUN. |
| `REUSE-BSC-C1.6-20261026` | 11 BSC-M4-Hosttests bestanden; Mac16,6/M4 Max/12P+4E/128 GiB, Fingerprint `0xfc5501a8`. | `bsc/native/evidence/c1/c1-10-gates-20261026T003339Z/c1-6.txt` | **REUSED_EVIDENCE**, nur Hosttopologie; `DN-MAC-01` ist kein kompletter DarkNode-Integrations-PASS. |
| `REUSE-BSC-LAUNCHD-20261027` | BSC `gui/UID`-LaunchAgent SIGKILL→Restart→C2 RESTORED, laut Status. | `bsc/docs/1_PLAN/STATUS.md` | **REUSED_EVIDENCE**, kein Mac-System-LaunchDaemon oder Bitget-Stop-/Keychain-Restore. |
| `REUSE-P0-M4-APFS` | Project_0-M4-Append p99 24,7 µs, synthetischer Append+`fsync` p99 5,31 ms laut Ledger. | `project_0/docs/MEASUREMENT_LEDGER.md` | **REUSED_EVIDENCE** als Hardware-/Schreib-Baseline, nicht P5-Order-Intent-Durability-PASS. |
| `REUSE-P0-P6-MT12` | Ein Owner, 1–2 I/O, FAST-Start 2, ein Evidence-Writer, QoS `USER_INITIATED/UTILITY` als festgelegter Architecture-Plan. | `project_0/docs/PHASENPLAN.md` §P6-MT.12; `project_0/docs/STATUS.md` | **DESIGN_FIXED, kein Laufzeit-PASS**; MT6–MT10 historisch OPEN. |

Die Research-Belege sind historische Dokumentationsbefunde: Es liegen in diesem Commit keine dadurch neu durchgeführten DarkNode-Laufzeitprüfungen vor. Ältere Original-Captures, die nicht selbst versioniert sind, sind entsprechend als Nutzer-/Forschungsbeleg gekennzeichnet.

## 6. Fortlaufendes Laufjournal – **append-only ab dem ersten echten DarkNode-Test**

**Erstanlage:** Keine neuen DarkNode-Produkt- oder Bitget-Tests ausgeführt. Erst ein ausgeführter, belegter Test erhält eine echte `run_id`. Eine Planung, ein Stub oder ein Git-Commit zum Dokumentieren ist **kein** PASS.

### Laufdatensatz – Kopiervorlage, ausdrücklich **kein** tatsächlicher Run

```text
run_id: DN-YYYYMMDDTHHMMSSZ-<TEST-ID>-NN
test_id: <ursprüngliche ID>                  phase/lane: <P1/P5/DN-PROTO/...>
started_utc: <ISO-8601>                    ended_utc: <ISO-8601>
source_sha: <exakter 40-Char-Git-SHA>       dirty_diff_hash: NONE | <hash>
host: <fingerprint, OS, locale, power>    mode: OFFLINE | PUBLIC | DEMO_READ | DEMO_WRITE | LIVE_AUTHORIZED
input: <Pfad/ID, Seed, UTC-Fenster>         input_sha256: <64-Char-Hash>
command: <tatsächlich ausgeführter CLI-Aufruf ohne Secrets>
expected: <festgelegte Assertions und Grenzwerte>
observed: <reale Ereignisse, Mengen, Fehlerraten, Messwerte>
exit_code: <echte Exit-Nummer>             result: PASS | FAIL | PARTIAL | BLOCKED | NOT_RUN
evidence: <Artefaktpfad/URL + SHA256>      issue: <Bug-ID/Root Cause oder NONE>
next_action: <konkret und zuständig>        reviewer: <Name/Rolle + Zeitpunkt>
```

**Fortschreibung:** Neue Runs unter §6 mit neuer Überschrift `### RUN <run_id>` **anhängen**, bestehende Einträge unverändert lassen. Bei `FAIL → FIX → PASS` bleiben ursprünglicher FAIL und neuer PASS sichtbar. In §4 Status, Datum, zuletzt gültiger Run-Link und Evidenz aktualisieren. Statuswechsel ohne Evidence als Fehler markieren. Ein endgültiges Phase-PASS benötigt alle erforderlichen Tests am qualifizierten Code-/Schema-/Host-SHA, nicht nur die Anzahl grüner Zeilen.
