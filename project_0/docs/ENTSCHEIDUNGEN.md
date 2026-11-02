# Project_0 — ENTSCHEIDUNGEN.md

**Version:** 1.5 · **Datum:** 2026-11-01 (DEC-17, DEC-18 und Freigaben C-01, C-04, C-05, C-10, C-11, C-12 vom Nutzer 01.11.2026)
**Art des Dokuments:** Entscheidungsnachweis. Dieses Dokument ist **keine** Status- und **keine** Phasenplan-Autorität. Status: `BUGS.md`. Plan und Gates: `PHASENPLAN.md`.
**Codebasis der Entscheidungsvorbereitung:** Docs `72a99cc2`, Code `865867af`; Gegenprüfung durch eine zweite Instanz an `87c064ba` und `746fdf29`. Vor jeder Umsetzung muss gegen den dann aktuellen HEAD neu gepinnt werden. Dateinamen in diesem Dokument zeigen nur, wo eine Änderung ansetzt; sie sind keine verbindlichen Zeilenangaben.

---

## 0. Regeln zum Lesen dieses Dokuments (verbindlich)

1. **Entschieden heißt nicht umgesetzt.**
   - Die Entscheidungen 1–15 in §2 hat der Nutzer am 2026-10-25 getroffen, 16 am 26.10.2026, 17 und 18 am 01.11.2026.
   - Keine davon war zum Zeitpunkt des Beschlusses implementiert; der Umsetzungsstand steht ausschließlich in `BUGS.md` und `STATUS.md`.
   - Eine Funktion gilt erst dann als umgesetzt, wenn alle Abnahmekriterien ihrer Entscheidung erfüllt sind und `BUGS.md` das mit Commit-SHA und Testnachweis ausweist.
2. **Dieses Dokument erteilt keine Freigabe.** Insbesondere erlaubt es nicht:
   - LIVE, Signieren oder werttragende Submission,
   - Mainnet-Reads, Captures oder Provider-Nutzung,
   - Echtgeld,
   - eine Lockerung von FREE_ONLY, ObserveOnly oder eines Phasenplan-Gates.

   Freigaben stehen ausschließlich in §4, jede einzeln mit Datum und Umfang; was dort nicht **ERTEILT** ist, ist nicht freigegeben.
3. **ACTIVE heißt nicht LIVE.** Wenn eine Capability in diesem Dokument auf ACTIVE gehen darf, betrifft das nur ihre technische Wirkung. Werttragende Ausführung bleibt an FE-06 und P8 gebunden.
4. **Messfragen (§3) sind nicht entschieden.** Bis zur genannten Messung gilt der heutige Wert unverändert. Kein Agent darf ihn vorläufig festschreiben.
5. **Korrekturen nur als neue Entscheidung.** Erzwingt reale Evidence eine Änderung, wird sie hier als neue, datierte Entscheidung eingetragen. Nie wird still im Code abgewichen.
6. **Was kein Agent darf:**
   - Entscheidungen hinzufügen,
   - Entscheidungen ändern,
   - aus Empfehlungen oder Begründungen eine Entscheidung ableiten.
7. **Scope:** Ausschließlich `project_0/`. BSC ist nicht Gegenstand dieses Dokuments.

---

## 1. Grundsätze

### G-1 — Fertig bauen, später nur aktivieren
- **Umfang:** Jede beschlossene Funktion wird jetzt vollständig gebaut. Dazu gehören:
  - produktive Logik,
  - Steuerung über den einen nativen Controller (OFF/OBSERVE/ACTIVE bzw. START/STOP/CANCEL/STATUS),
  - typisierte `.p0j`-Records,
  - Status-Hinweg bis zum bestehenden Reiter und Command-Rückweg bis zum Owner,
  - RED→GREEN-Tests und Loopback-/Restart-/Reconnect-Tests,
  - der Eintrag in `BUGS.md`.
- **Später:** Es wird nur noch qualifiziert und aktiviert, ohne neuen Code. Ausgenommen sind Korrekturen nach Regel 0.5.
- **Standardzustand:** Immer OFF, oder OBSERVE, wo eine Entscheidung das ausdrücklich festlegt. Nie ACTIVE.
- **Kein toter Code:** Jeder gebaute Pfad ist mindestens in OBSERVE produktiv erreichbar.
- **Kein Stub:** Fehlende externe Qualifikation rechtfertigt keinen Stub. Sie blockiert nur die reale Abnahme, und das wird sichtbar als `BLOCKED` bzw. `NOT_RUN` ausgewiesen.
- **Wann `OFFLINE_VERIFIED` gemeldet werden darf:** Erst wenn drei Dinge erfüllt sind:
  1. die Implementierung ist vollständig,
  2. das Offline-Gate ist tatsächlich gelaufen (fmt, Clippy `-D warnings` in der Feature-Matrix, Tests, Release-Build, Python),
  3. die erforderlichen CI-Jobs sind grün (siehe Z-1).

  Externe Qualifikation und LIVE-Freigabe bleiben eigenständige Gates.

### Z-1 — CI-Pflichtjob-Regel (ENTSCHIEDEN, N-4)
- **Nutzerentscheidung vom 25.10.2026:** Verbindlich gilt N-4 in §7 einschließlich fester Pfad→Job-Tabelle aus den jeweils gültigen CI-Workflows, präzisiert durch N-6 (Pflichtjob auf exakt geprüftem SHA) und N-7 (Project_0-CI-Auslöser).
- Bei roten betroffenen Pflichtjobs ist `OFFLINE_VERIFIED` untersagt, außer den **vier** ausdrücklich festgelegten und belegten BSC-Ausnahmekriterien für gemeinsame Jobs; nie für `project0-gate`.
- Keine eigenmächtige BSC-Korrektur, kein übersprungener Test, keine grüne Gesamt-CI bei rotem Lauf; alle Ausnahmen mit `BUGS.md`-ID dokumentieren.

---

## 2. Entscheidungen 1–18

Jede Entscheidung ist gleich aufgebaut: Beschluss · Pflichtanforderungen · Bedeutet nicht · Abnahme · Ansatzpunkte.
**Status jeder Entscheidung:** BESCHLOSSEN (Nutzer: 1–15 am 2026-10-25, 16 am 2026-10-26, 17 und 18 am 2026-11-01); die Code-/Frontend-Abnahmen sind je Ticket gesondert im kanonischen `BUGS.md` zu führen, keine Implementierung allein durch Dokumentation behaupten.

### 1 — V2-09: Blockhash-Frühcheck
- **Beschluss:** Variante B. Frühabbruch mit sicherem Wiederanlauf.
- **Pflichtanforderungen:**
  1. **Frühcheck:** Die Blockhash-Frische wird geprüft, bevor das Kandidaten-RPC-Budget verbraucht und bevor die Planung (`plans_with_health`) aufgerufen wird. Ist der Blockhash veraltet oder fehlt er, gibt es keinen Planungsaufruf und keinen Budgetverbrauch.
  2. **Verlustgrund:** `Build`, mit der eindeutigen Ursache `BLOCKHASH_STALE`. Nicht ausgeführte Routing- oder Planungsprüfungen werden weder als bestanden noch als gescheitert gezählt.
  3. **Refresh anstoßen:** Der Owner weckt den Blockhash-Refresh sofort. Er macht dabei selbst keinen Netzaufruf, und das Provider-Budget bleibt unverändert.
  4. **Parken:** Der Kandidat wird geparkt.
     - Je Account gibt es höchstens einen Eintrag.
     - Bei frischem Blockhash kehrt er in seine ursprüngliche Lane mit seinem ursprünglichen `T_rx` zurück, aber nur, wenn Opportunity-Freshness, Input-Revision und alle Gates noch gültig sind.
     - Andernfalls fällt er regulär als stale heraus.
     - Kein Busy-Loop, keine Doppelverarbeitung.
  5. **Spätcheck:** Der bestehende zweite Check unmittelbar vor dem Build bleibt erhalten.
  6. **Statistik:** Die Loss-Statistik trägt eine Policy-Version. Messungen nach alter und neuer Policy werden nicht vermischt.
- **Bedeutet nicht:** Das maximale Blockhash-Alter ändert sich nicht.
- **Abnahme:** RED→GREEN-Tests für:
  - veralteten Blockhash ohne Planungsaufruf und ohne Budgetverbrauch,
  - Refresh während des Parkens mit Wiederanlauf,
  - Race „Refresh während der Planung“,
  - Liveness: kein Kandidat geht verloren, keiner wird doppelt verarbeitet.

  Ist der Wiederanlauf nicht nachweisbar, bleibt der Frühcheck deaktiviert. Dann gilt nur der bestehende Diagnosezähler.
- **Ansatzpunkte:** `execution/pipeline.rs`, Lanes in `state_owner/candidates.rs`, Blockhash-Refresh in `state_owner/setup.rs`, Verlustgrund-Typ. Register: `bug_v2.md` V2-09.

### 2 — V2-15 / P7-03: Zweite Simulation in der FE-04-Admission
- **Beschluss:** Variante A. Eine vorhandene Simulation wird wiederverwendet, aber nur bei vollständiger Bindung und Aktualität.
- **Pflichtanforderungen:** Wiederverwendung nur, wenn **alle** Bedingungen gelten:
  1. Der Wire-Digest von `SignedPayload` ist identisch (Signaturen, Reihenfolge, Pfad). Ein gleicher Message-Hash reicht nicht.
  2. Der Pfad passt. Ein Bundle braucht eine Bundle-Simulation.
  3. Das Ergebnis ist ein typisiertes `SimResult::Ok`. `Failed`, `Unavailable` und `Deadline` sind nie wiederverwendbar.
  4. Der Kontext-Slot der Simulation ist ≥ dem State-Slot des Intents.
  5. Am Handoff gelten: gleiche State-Generation; seit der Simulation kein Gap, keine Recovery und keine ProgramData-Änderung.
  6. Das Alter der Simulation ist ≤ `max_state_age_ms` der Approval.
  7. Der Nachweis ist das typisierte `SimulatedPayload`-Token aus der echten Simulation. `exact_signed_payload_simulated` wird aus diesem Token abgeleitet; ein Literal ist nicht zulässig.

  Ist eine Bedingung verletzt, wird erneut simuliert, sofern die Deadline das zulässt. Andernfalls wird abgelehnt, ohne dass ein Hold entsteht.
- **Bedeutet nicht:** Keine Freigabe von FE-06.
- **Abnahme:**
  - Je verletzter Bedingung ein Test, der eine Neusimulation oder Ablehnung belegt.
  - Im Wiederverwendungsfall wird nachweislich genau eine Simulation ausgeführt.
- **Ansatzpunkte:** `one_shot/admission.rs`, `one_shot/transport.rs`, `execution/pipeline.rs`. Register: `bug_v2.md` V2-15.

### 3 — V2-15 / P7-02: Beweis „nicht gelandet“
- **Beschluss:** B und C prüfen die Non-Landing-Evidence; **N-2 (§7) ist die verbindliche Präzisierung und ersetzt die früheren Pflichtanforderungen 3–6 einschließlich der alten Widerspruchs- und Wallet-Kennzahlen.**
- **B:** Zwei unabhängige FREE_ONLY-Quellen verschiedener Betreiber; je Quelle vier Kriterien: finalisierte Blockhöhe > `lastValidBlockHeight`, `minimumLedgerSlot` ≤ State-Slot, gebundene Signaturen mit History nicht vorhanden und `getTransaction(finalized)=null`. Jede Quelle wird als verwendbar / unbrauchbar / positive Sichtung typisiert eingestuft.
- **C:** Pflichtmäßige lückenlose qualifizierte finalisierte kanonische Block-/Parent-/Skip-/Signatur-Gegenprüfung vom State-Slot bis nach dem Gültigkeitsfenster; niemals allein aus geprunten/null RPC-Responses auf Abwesenheit schließen.
- **Veto/Settlement:** Positive Sichtung durch B oder C blockiert Non-Landing; nur vollständige bestätigte finalisierte Landung geht in FE-04-Reconciliation, Kapitalfreigabe ausschließlich über qualifiziertes Journal-Settlement. Sonst UNKNOWN/Holds bleiben/Alarm/Retry.
- **Abwesenheit:** Keine positive Sichtung, C lückenlos negativ, dazu entweder zwei gültige unabhängige B-Negativnachweise oder bei weniger als zwei brauchbaren B-Quellen der qualifizierte C-Alleinbeweis. Kein manueller Override.
- **Wallets:** exakt fünf Ergebnisklassen nach N-2 (§7), unbrauchbare B-Quellen nach Grund, Hold-Dauer, Calls und Bytes; **kein Schalter**.
- **Abnahme:** N-2-Loopback-Matrix in §7 und echte Phase-/Provider-/Archive-Gates; ohne diese keine externe Freigabe. Keine vorzeitige Behauptung aus geschätzten Sekunden.
- **Ansatzpunkte:** `one_shot/settlement.rs`, `one_shot/exit.rs`, `state_owner/fe04.rs`; Querverweise `BUGS.md` C12 und `bug_v2.md` V2-15.

### 4 — V7: Holds bei ZeroPrefund/FlashFunded
- **Beschluss:** A und B. A gilt verbindlich; B wird vollständig gebaut. Modi OFF/OBSERVE/ACTIVE, Standard OBSERVE.
- **Pflichtanforderungen:**
  1. **Zwei getrennte Arten von Hold:**
     - **Wallet-Reservierung:** nur ausgebbare Einheiten (SOL für Fee, Tip, Rent und Protokollgebühr; Input-Token; Flash-Rückzahlung). Geprüft gegen Saldo minus Fremdreservierungen minus Policy-Reserve.
     - **Inventar-Limit:** der konservative USD-Wert (oberer Preis) des übernommenen Collaterals. Geprüft gegen das Safety-Inventarlimit, **nie gegen den Saldo**.
  2. **Je Asset getrennt:** keine Verrechnung zwischen Assets, keine Doppelreservierung derselben Einheiten.
  3. **Freigabe** nur über eines von beiden:
     - ein finalisiertes Settlement mit vollständigen Token-Deltas,
     - den Beweis aus Entscheidung 3.

     Danach bleibt nur das tatsächliche Restinventar gebunden. Unvollständige Daten ergeben UNKNOWN.
  4. **B umfasst** exakte Flüsse aus dem signierten Plan:
     - Mint-/Amount-Parität,
     - Integrations-Umrechnung (Kamino, Drift, JupLend),
     - Swap-Floor,
     - Token-2022-Gebühren,
     - Flash-Rückzahlung.

     Für unqualifizierte Integrationen gilt automatisch A, und das wird sichtbar angezeigt.
  5. **OBSERVE:** Je Versuch werden A-Hold, B-Hold und die finalisierten Ist-Flüsse als typisierter `.p0j`-Record persistiert.
  6. **B → ACTIVE** nur, wenn alles erfüllt ist:
     - B hat in keinem Fall unterschätzt,
     - es gibt mindestens N echte finalisierte Versuche (N siehe §3, B-8),
     - die Tests sind grün,
     - die Entscheidung ist hier eingetragen.

     B in ACTIVE ist spätestens vor parallelen LIVE-Versuchen Pflicht.
  7. **FlashFunded** bleibt gesperrt, bis die Klasse qualifiziert ist. Danach gilt diese Regel ohne neue Entscheidung.
  8. **Wallets-Reiter:** Modus, A-/B-/Ist-Werte, Abweichungen, gebundenes Kapital.
- **Bedeutet nicht:** Keine Änderung an Limits oder Kapitalbudgets.
- **Abnahme:**
  - Ein ZeroPrefund-Plan wird nie wegen des Collaterals am Saldo abgelehnt (Test).
  - Je Bestandteil von B ein Paritätstest.
  - Der ACTIVE-Pfad ist offline getestet und hinter dem Gate gesperrt.
- **Ansatzpunkte:** `execution/builder.rs`, `liquidation/funding.rs`, `execution/reservation.rs`, `liquidation/safety.rs`, `one_shot/admission.rs`. Register: `BUGS.md` V7/B15, `bugs_b.md` Z7.

### 5 — D11: Zentrale Konfiguration
- **Beschluss:** Variante A. Eine typisierte Registry mit betriebsartabhängigen Profilen.
- **Pflichtanforderungen:**
  1. **Eine Quelle:** Alle 24 Owner-Parameter kommen in die Registry (`runtime_config`). Defaults am Aufrufort (`parsed_or`) entfallen.
  2. **Profilzuordnung:** Jede Betriebsart hat eine ausdrückliche Zuordnung zu einem Profil, ohne implizite Auswahl. Auch econ4h ohne Argumente wird über ein Profil aufgelöst.
  3. **Präzedenz:** Profil < Environment < Konfigurationsdatei < CLI/Controller. Die Herkunft jedes Werts steht im Run-Record.
  4. **Keine Wertänderung:** Die Migration ändert keinen effektiven Wert. Die Profile setzen die heute wirksamen Werte ausdrücklich, einschließlich der heutigen Parallelität je Betriebsart.
  5. **Neustart als Standard:** Hot-Change nur per eigener Entscheidung. Bisher beschlossen: Threshold-Modus (Entscheidung 6) und TTL zwischen Läufen (Entscheidung 7).
  6. **Sichtbarkeit:** Alle Werte sind im Frontend lesbar. Editierbar sind nur die ausdrücklich dafür vorgesehenen. `workers = 1` im Shadow-Profil wird als „vorläufig, Messung offen“ markiert.
- **Bedeutet nicht:** Kein Tuning. Neue Werte erst nach Messung (§3).
- **Abnahme:**
  - Präzedenz-Tests und Tests für ungültige Werte.
  - Ältere `Limits`-Revisionen bleiben lesbar.
  - Der effektive Konfigurations-Record für Shadow und econ4h ist vor und nach der Migration wertgleich.
- **Ansatzpunkte:** `solana/src/runtime_config.rs`, `state_owner/setup.rs`, `orchestration/startup.rs`, `config/profiles/*`. Register: `fehlstellen_v1.md` D11.

### 6 — C08: Threshold-Modus
- **Beschluss:** Variante A. OFF/OBSERVE/ACTIVE mit Hot-Change und parallelem Vergleich.
- **Pflichtanforderungen:**
  1. **Steuerung:** Über den einen nativen Controller, mit CAS, Policy-Version, Generation und `.p0j`-Receipt. Die Umschaltung erfolgt atomar am Owner. Keine zweite Engine.
  2. **OBSERVE:**
     - Der Sweep bleibt maßgeblich. Threshold rechnet parallel, reiht aber nichts ein.
     - **OBSERVE darf den Sweep nie verlangsamen:** Es rechnet auf einem Worker gegen einen unveränderlichen State-Stand oder unter einem harten CPU-Budget auf dem Owner. Fehlt Budget, wird OBSERVE gesampelt.
     - Die Owner-Blockzeit wird angezeigt.
  3. **ACTIVE:**
     - Nur aus OBSERVE erreichbar, und nur mit fertigem, aktuellem Index. Direkt von OFF auf ACTIVE geht nicht.
     - `requested` und `effective` sind getrennt. Bis der Index fertig ist, gilt `effective = SWEEP` mit `block_reason = INDEX_BOOTSTRAP`.
     - Das Gleiche gilt nach Recovery und Neustart: `requested` wird wiederhergestellt, `effective` folgt der Index-Bereitschaft.
  4. **Bei Gap oder unbekanntem State:** keine neue Admission, Rückfall auf den Sweep.
  5. **Übergang:** Gemeinsames Repeat-Gate. Beim Wechsel geht kein Kandidat verloren, keiner wird doppelt eingereiht.
  6. **Gegenprobe in ACTIVE:** Eine Sweep-Stichprobe läuft außerhalb des Owners weiter. Ein einziger Fehltreffer (Sweep findet, Threshold weckt nicht) schaltet automatisch auf den Sweep zurück und löst einen Alarm aus.
  7. **Gate für ACTIVE im realen Betrieb:**
     - 0 Fehltreffer über N echte Oracle-Ticks (N siehe §3, B-8); gezählt werden nur Ticks mit identischer State-Revision.
     - Die Tests zu V2-03 und V2-13 sind grün.
     - Die Entscheidung ist hier eingetragen.
  8. **Liquidator-Reiter zeigt:**
     - Treffer von Sweep und Threshold,
     - Fehltreffer, zusätzliche Treffer,
     - Erkennungszeit p50/p95/p99,
     - CPU- und Health-Aufwand,
     - Fallbacks,
     - `requested`/`effective`/Generation/`block_reason`.
- **Bedeutet nicht:** ACTIVE ist keine LIVE-Freigabe.
- **Abnahme:**
  - Tests für jeden Moduswechsel, ungültige Wechsel (OFF → ACTIVE), Versionskonflikte, Index-Bootstrap, Recovery, Neustart, Reconnect.
  - Nachweis „kein verlorener, kein doppelter Kandidat“.
  - Messung der Owner-Blockzeit in OBSERVE.
- **Ansatzpunkte:** `state_owner/setup.rs` (`P0_USE_THRESHOLD_TRIGGER`), `state_owner/events.rs`, `state_owner/sweep.rs`, `state_owner/detect.rs`, Controller. Register: `fehlstellen_v1.md` C08, `bug_v2.md` V2-03/V2-13.

### 7 — N7: Boot-Audit und TTL
- **Beschluss:** Variante A. Integrierter Ablauf mit konfigurierbarer TTL.
- **Pflichtanforderungen:**
  1. **Ablauf in fünf Schritten:**
     1. Offline-Preflight,
     2. ausdrückliche Netzwerkfreigabe,
     3. Network-Audit (typisierter `.p0j`-Record),
     4. ausdrückliche Startfreigabe,
     5. Start mit interner Token-Übergabe; dabei werden TTL und BOOT-12-Identitäten erneut geprüft. Bei Abweichung: BLOCKED.
  2. **TTL als typisiertes Feld:** Default 60 s, Minimum 10 s, Maximum 120 s. Werte außerhalb dieser Grenzen sind nicht einstellbar.
  3. **Startpunkt der TTL:** Die TTL zählt ab dem **ältesten** Network-Read des Audits.
  4. **Änderung nur zwischen Läufen:**
     - Sie gilt nie für ein bereits ausgestelltes Token.
     - Ein älteres Token wird durch eine Konfigurationsänderung nie gültig.
  5. **Alle heutigen 60-s-Literale** (Descriptor, Audit, Startprüfung, Runtime-Admission, Tests) werden konsistent durch das Feld ersetzt.
  6. **Keine automatische Wiederholung** eines abgelaufenen Audits ohne neue Freigabe.
  7. **Ein Controller** für CLI und Capture-Reiter. Änderungen werden mit Wert, Quelle und Zeitpunkt im Run-Record gespeichert.
- **Bedeutet nicht:** Die Netzwerkfreigabe und die Startfreigabe bleiben getrennt und ausdrücklich.
- **Abnahme:** Tests für:
  - TTL 9/10/120/121 s,
  - langsamen Audit,
  - Konfigurationsdrift,
  - parallele Startversuche,
  - Crash/Restart,
  - „altes Token nach Konfigurationsänderung“.
- **Ansatzpunkte:** `orchestration/boot_network.rs`, `orchestration/boot_audit.rs`, `bin/p0ctl.rs`. Register: `BUGS.md` N7.
- **Nachtrag 26.10.2026 (Nutzer):** Boot-Status auf `project0.analytics.jobs` (15), Anzeige im Capture-Reiter; Rohdaten-Capture ist eigener Boot-Modus `market`; alle Startwege durch denselben nativen Boot-Controller, Autostart ohne Zulassung gesperrt; Provider-Evidence per Record-ID; GUI-Start vorerst nur Chain-Verification `off` (bei DEC-08 wieder aufgreifen).

### 8 — Native Verifikation: INDEX, CROSSCHECK, STRICT
- **Beschluss:** Variante A. Alle drei Verfahren nativ in Rust. OFF bleibt bestehen.
- **Pflichtanforderungen:**
  1. **Eine Engine:**
     - INDEX, CROSSCHECK und der Winner-Tracker teilen einen Leser für die Signatur-Historie (Pagination, 429, Checkpoint, Resume).
     - Alle nutzen den vorhandenen Decoder und Klassifizierer (`chain.rs`, `market_truth.rs`).
     - Ein zweiter Parser ist verboten.
  2. **Paritäts-Gate:**
     - Der native INDEX liefert auf der vorhandenen historischen Evidence (A4-Paritätsbestand) dieselben Ergebnisse wie der gepinnte Python-Verifier.
     - Erst danach wird der Python-Verifier aus `scripts/` entfernt und archiviert.
     - Historische JSON-Evidence bleibt nur lesend erhalten.
  3. **Budgets:** Requests, Bytes und RPS je Verfahren sind typisierte Felder unter FREE_ONLY. HTTP 429, Lücken oder Pruning ergeben `INCOMPLETE` mit fortsetzbarem Checkpoint.
  4. **Rangfolge STRICT > CROSSCHECK > INDEX:**
     - Es gibt nie eine Hochstufung.
     - Der C0-E1-Master braucht zusätzlich alle übrigen Gates.
  5. **CROSSCHECK** braucht einen zweiten qualifizierten Provider (§4, C-02). Bis dahin: fertig gebaut, offline getestet, real `NOT_RUN`.
  6. **Anzeige:**
     - Capture-Reiter: Verfahren, Status, Fortschritt, Provider, Requests, Bytes, Checkpoint, `block_reason`.
     - Analytics-Reiter: Ergebnisse der drei Verfahren nebeneinander, vier Liquidationsarten, sechs Marker, gefundene und fehlende Ereignisse, Abweichungen, Kosten.
- **Bedeutet nicht:** PHASENPLAN C0-E1 L.2 mit vier Stufen bleibt unverändert gültig. Keine neuen Reiter.
- **Abnahme:**
  - Paritätsnachweis.
  - Loopback-Tests für Pagination, 429, Resume, Lücke, Pruning.
  - Nachweis, dass keine Stufe eine andere hochstuft.
- **Ansatzpunkte:** `market/strict_driver.rs`, `bin/p0ctl.rs`, `market_*`, `market/winner.rs`. Register: `fehlstellen_v1.md` A04; `qualification/C0E1_4H_READINESS.md`.

### 9 — Historisches JSON aus dem Produktionsbinary
- **Beschluss:** Ja.
- **Pflichtanforderungen:**
  - Historische JSON-Leser und -Renders liegen hinter einem Cargo-Feature `historical`.
  - Das Standard-Produktionsbinary enthält keinen historischen JSON-Code.
  - Nur ausdrücklich gestartete Read-only-Werkzeuge (z. B. `p0-journal`, Verifier für alte Läufe) aktivieren das Feature.
  - Es gibt keinen produktiven JSON-Fallback.
- **Bedeutet nicht:**
  - JSON an externen Schnittstellen (Solana-RPC/WSS, Jupiter, Jito) bleibt nach JFI-02 zulässig.
  - Historische Evidence bleibt lesbar.
- **Abnahme:** Der Inventartest `test_jfi04_json_inventory.py` prüft zusätzlich: Ein Standard-Build ohne `historical` enthält keine Aufrufe der Kategorie H.

### 10 — FE-04-Approval-Datei
- **Beschluss:** Ja. Migration von JSON auf natives `key = value`.
- **Pflichtanforderungen:**
  - Der Parser ist streng typisiert: eindeutige Schlüssel, unbekannter Schlüssel ergibt einen Fehler, Einheiten sind im Schlüsselnamen enthalten.
  - Inhalt, Bindung und Einmaligkeit einer Approval bleiben unverändert.
  - Alte JSON-Approvals werden **nicht** still übernommen; eine neue Approval muss ausgestellt werden.
  - Es gibt genau eine Approval-Autorität.
- **Abnahme:** Tests für doppelte Schlüssel, unbekannte Schlüssel, falsche Einheiten, eine abgelaufene Approval und die Ablehnung einer alten JSON-Approval.

### 11 — Bundle- und JIT-Code
- **Beschluss:** Ja. Jetzt vollständig bauen, bis zur Grenze der Bundle-Simulation. Capability standardmäßig OFF.
- **Pflichtanforderungen:**
  - Bundle-Gebührenmodell: Fee je Transaktion plus Tip.
  - SOL-Belastung und Holds über mehrere Transaktionen im Intent.
  - Konstruktion des `JitoAdapter` über die Capability.
  - Bundle-Admission statt der pauschalen Ablehnung `FE04_BUNDLE_FEES_UNQUOTED`.
  - `P0_BUNDLE_SIM_RPC_URL` als typisiertes Konfigurationsfeld.
  - Ohne qualifizierten Bundle-Simulator bleibt `bundle_simulated = false`, und Bundles werden weiterhin abgelehnt.
- **Bedeutet nicht:** Keine reale Jito-Verbindung, kein Senden.
- **Abnahme:** Loopback-Tests für Gebührenmodell, Holds, Admission und Simulationsgrenze. Nachweis, dass mit OFF kein Jito-Aufruf erfolgt.
- **Register:** `fehlstellen_v1.md` B02/B05/F-20.

### 12 — V2-14-Reste
- **Beschluss:** Ja.
- **Pflichtanforderungen:**
  - Typisierte Rejection: Der Text entsteht erst im Writer. Schema-Bump mit Leser für das alte Layout.
  - `message_hash` als nativer Hash-Typ, mit versionierter Migration des gepinnten Layouts.
  - Die Handoff-Deadline wird durch Jito-`submit` gereicht.
  - Bestehende Latenz- und Safety-Verträge bleiben erhalten.
- **Abnahme:** RED→GREEN je Punkt. Alte Records bleiben lesbar.

### 13 — FE-04-Admission produktiv verdrahten
- **Beschluss:** Ja. Gilt für Versuch und Exit.
- **Pflichtanforderungen:**
  - Der Owner erzeugt die Admission produktiv; der Detect-Pfad übergibt den Kandidaten.
  - LIVE läuft über das Controller-Gate mit getrennten Werten `requested` und `effective`. `effective` bleibt `BLOCKED` (FE-06/P8-LT), bis eine Freigabe erteilt ist.
- **Bedeutet nicht:** Keine werttragende Ausführung, kein Signieren für Mainnet ohne gesonderte Freigabe.
- **Abnahme:**
  - Ein zählender Fake-Adapter belegt: Ohne Freigabe erfolgt **kein einziger** Submit-Aufruf.
  - Offline-Ende-zu-Ende mit Recording-Adapter.
  - Die Sperre ist in Code und Test nachgewiesen.
- **Register:** `fehlstellen_v1.md` F-09.

### 14 — Register
- **Beschluss:** Ja, mit Änderung.
- **Pflichtanforderungen:**
  - `BUGS.md` ist die einzige kanonische Statusautorität.
  - Jede ID aus `bugs_b.md`, `fehlstellen_v1.md` und `bug_v2.md` wird in `BUGS.md` mit aktuellem Status und Nachweis referenziert.
  - Widersprüche in `STATUS.md` werden korrigiert.
  - Eine Abgleichliste belegt, dass keine ID verloren ging.
  - Es gibt eine Zählung je Commit-SHA.
  - **N-3:** Die drei Originalberichte bleiben unverändert an Ort und Stelle; **erst ab dem konsolidierten `BUGS.md`-Commit** sind sie als Prüfberichte eingefroren und keine Statusautorität mehr. Status danach ausschließlich in `BUGS.md`; Archivierung erst, wenn sämtliche IDs des jeweiligen Berichts `ERLEDIGT` oder `WIDERLEGT` sind.
- **Bedeutet nicht:** Kein Löschen von Befunden, Evidence oder Originaltext.

### 15 — Git- und Session-Regel
- **Beschluss:** Ja.
- **Pflichtanforderungen:**
  - Eine Session arbeitet in einem eigenen Worktree mit eigenem Index.
  - Nur **ein** verantwortlicher Integrator pusht `main`.
  - Vor jeder Integration: `fetch`, Remote-Stand prüfen, Fast-Forward nachweisen, erforderliche Tests auf dem tatsächlich zu pushenden Stand ausführen.
  - Rebase nur auf eigene Commits. Bei Konflikten: STOP.
  - Keine Branches, kein Force-Push, kein Überschreiben fremder Änderungen.
  - BSC und Project_0 bleiben strikt getrennt.
- **Gilt ab:** sofort.

### 16 — Dauerbetrieb `UntilStopped` (Nutzerentscheidung 26.10.2026)
- **Beschluss:** Dauerbetrieb `UntilStopped` (24/7) jetzt vollständig entwickeln, nach DEC-07 und vor DEC-06; keine neue Architektur, kein späterer Neuaufbau. `RunHorizon::admit()` bleibt der zentrale Admission-Prüfpunkt, aber nicht das einzige Gate; die bestehende Sperre wird nicht pauschal entfernt. Qualifikationen als typisierte, persistente, identitätsgebundene Nachweise: SK-07-Segmentierung und Recovery, rollierende Kapazität, Providerbudgets, verfügbare Provider-Evidence. Ohne vollständige Qualifikation `RUN_HORIZON_UNTIL_STOPPED_GATED`; mit ihr grundsätzlich zulässig, aber nur mit den Netzwerk-, Start- und Betriebsfreigaben. Ressourcen-, Provider- und Integritätsgates bleiben im Betrieb wirksam, Verstoß ⇒ sicherer HALT. Offline-Tests beider Fälle inkl. Crash, Restart, Recovery, Ressourcenerschöpfung; reale Mainnet-Qualifikation `NOT_RUN`/`BLOCKED_EXTERNAL`. Qualifizierter Dauerbetrieb ist **keine** Erlaubnis für unbeaufsichtigte Neustarts und **keine** LIVE-Freigabe.
- **Reihenfolge:** nach 7, vor 6.
- **Nachtrag 26.10.2026 (Nutzer):** Stufen innerhalb von DEC-16 (Nutzer 26.10.2026), je mit vollständigem gate.sh und CI auf dem finalen SHA: 1. modusneutral (admit(), Qualifikations-Records, rollierende Segmente, laufende Kapazitäts- und Budgetprüfung, Recovery); 2. econ4h und market; 3. Shadow-Owner (Owner-Fenster, Drains, Owner-Journale). Alle drei Modi, LIVE bleibt gesperrt; admit() bleibt je Modus GATED, bis dessen Qualifikationsnachweise vorliegen.
- **Nachtrag 26.10.2026 (Nutzer, N-5):** GUI (Capture-Reiter, bestehender Boot-Abschnitt, kein neuer Reiter): Dauer-Auswahl „Bis Stop“ mit Fenster, Runway und Qualifikations-ID, ohne vollständige Qualifikation sichtbar, aber gesperrt mit Grund; Stop laufender UntilStopped-Läufe (Drain, Seal, Abschlussstatus); Anzeige, Erteilen und Widerrufen der Dauerfreigabe für den Autostart, gebunden an Config-Hash, Modus, Provider, Budget, maximale Laufzeit und Ablauf, Bindung vor dem Erteilen angezeigt.
- **Nachtrag 27.10.2026 (Nutzer, Stufe 2):** econ4h-Fenster jetzt, Zulassung bleibt GATED bis Stufe 3 (Shadow-Owner-Fenster); geschlossene Fenster bleiben vorläufig, Nachzügler werden gezählt, nie zurückgerechnet (keine Gnadenfrist); der persistierte u64::MAX-Cutoff der unbegrenzten Rohdaten-Capture wird typisiert ersetzt (angehängte Variante, alte Frames lesbar); eine abgestürzte Rohdaten-Capture setzt beim nächsten zugelassenen Start denselben Lauf fort (SK-07-Recovery, unterbrochenes Fenster erneut geprüft, ein angehaltener Lauf bleibt angehalten).
- **Nachtrag 28.10.2026 (Nutzer, Stufe 3):** D-5 A Autostart nur market, Fortsetzung aller Modi nach freigegebenem Boot-Start; D-3 A derselbe econ4h-Lauf, unterbrochenes Fenster PARTIAL/CRASH_RECOVERED; D-1 A kein Drain an normalen Fenstergrenzen, Erkennung ohne Unterbrechung; D-7 A offene Watches gezählt, geordneter Shutdown mit vollständigem Drain und Journal-Seal, finanzielle UNKNOWN-Zustände bleiben gesperrt; Vorgaben (ohne Widerspruch): D-2 Rotation je Fenster + econ_segment_mib, D-4 Fenster W ab erstem handelbaren READY, D-6 econ4h-Läufe als Kapazitätsbasis, D-8 keine Proto-Änderung.

### 17 — Operator-Auth, Wallet-Modell, Jito, EXIT-WSOL, Journal, Operator-Tools, Safety-HALT (Nutzerentscheidung 01.11.2026)
- **a) Operator-Authentifizierung (P8-CTRL-02):** B, Secure Enclave, Zugriffsregel userPresence (Touch ID, sonst Login-Passwort im Systemdialog). Pflicht für jeden FE-06-Grant und jedes ENABLE von `WALLET:*`, `ECON_ARM`, `ECON_SEND`, `ECON_LIVE`; DISABLE, REVOKE und STOP nie. Die Signatur bindet exakt Request-ID, Generation, Owner-Epoche, Scope-Hash bzw. Schalter und Wert, Zeitstempel; der Owner prüft sie gegen den einmalig registrierten öffentlichen Schlüssel, dessen Hash in der Konfigurationsidentität steckt, sonst `OPERATOR_AUTH_REQUIRED`. Bestätigung nur lokal über `p0ctl` (`p0ctl live grant --confirm <hash>`, `p0ctl capability set … --confirm`); die GUI bereitet vor, zeigt Scope, Hash und „wartet auf Bestätigung“ und kann selbst nicht bestätigen. Grant-Annahme: jede referenzierte Evidence ist ein typisierter Record, per SHA-256 des Inhalts gebunden; der Owner prüft die Hashes beim Grant und bei jeder Nutzung, Abweichung ⇒ Grant BLOCKED mit `EVIDENCE_CHANGED` (L04). Linux-CI: feature-gated Test-Signer, nicht im Produktionsbinary (Negativtest wie L18).
- **b) Wallet-Modell:** ein Owner, mehrere Signer mit Rollen (`LIQUIDATOR`, `FEE_PAYER`, `TREASURY` nur lesend); je Signer eigener Schalter `WALLET:<Pubkey>`, eigene Holds, eigene Limits, eigene Journal-Lane im einen FE-04-Journal. Kein Kapitalfluss zwischen Wallets außer über einen expliziten Rebalance-Plan (P8-INV, bleibt OBSERVE). Wallet-Wahl je Kandidat deterministisch nach verfügbarem Bestand nach Holds und Exit-Reserve, kein Round-Robin. Fehler, UNKNOWN oder HALT einer Wallet sperrt nur diese Wallet, nie global. Der FE-06-Scope nennt die zugelassenen Wallets. Erster Pilot: genau eine Wallet, zugleich Fee-Payer.
- **c) Jito S3-R01:** Block-Engine-Protos gepinnt im Repo vendoren (wie Yellowstone) und mit der vorhandenen Toolchain bauen, kein externes Jito-Crate, Golden-Tests. Standard ohne Auth-Keypair (FREE_ONLY); Keypair nur als optionales Feld nach gemessenem Vorteil, durch dieselben Schalter gedeckt. Jitos aktuelle Policy wird beim realen Zugriff geprüft, nie angenommen.
- **d) EXIT-WSOL:** WSOL-Staub sperrt nur die eigene Position, nie Exits anderer Positionen. Beim vollständigen Exit wird das WSOL-ATA per Unwrap geschlossen; ein Rest unter einer typisierten Schwelle wird als `DUST`-Record gebucht und als Kosten verbucht, ein Rest darüber ist ein eigener Exit mit eigener Admission. Nachweis „Close + Re-Create in einer TX“ offline per litesvm mit dem im Lese-Smoke gelesenen Token-/ATA-Programm-Bytecode. M1/M2 werden diesem Text zugeordnet; weichen sie ab, gilt dieser Text.
- **e) Korruptes Journal:** archivieren, nicht reparieren, nicht löschen: `p0ctl`-Befehl, Manifest mit Pfad, SHA-256, Grund, Datum; zählt nicht zur Qualifikation; Status ARCHIVED.
- **f) Operator-Tools (P8-CTRL-03d: `p0-prewarm-atas`, `p0-probe-*`):** dieselben Permits (WALLET, SIGN, SEND, Operator-Bestätigung), kein zweiter Pfad; kein ATTEMPT/ARM, dafür ein typisiertes Kostenlimit je Werkzeug.
- **g) Safety-HALT:** sperrt neue Versuche, nie Exits. Exits brauchen weiterhin ECON_LIVE, Wallet, SIGN, SEND und Budget; nur `ECON_LIVE=DISABLED` oder STOP sperrt auch Exits.
- **Offen bleiben:** P8-INV ACTIVE (OBSERVE bis nach FE-07), V7-Voll-Hold, B-8 (N).
- **Reihenfolge:** nach dem Lese-Smoke (§4 C-01, C-04, C-05, C-10), der seinerseits erst nach DEC-18 und dem C-11-Messwerkzeug läuft (DEC-18 §6); P8-CTRL-02 zuerst; jedes Paket mit `gate.sh` und CI auf dem exakten SHA.

### 18 — Provider-End-to-End-Verwaltung (DEC-18), Provider-Vormessung (C-11), BOOT-05/P1B→P1A, Jupiter (Nutzerentscheidung 01.11.2026)
- **Quelle:** Nutzerdokument `Project_0_DEC18_C11_BOOT05_Jupiter_REV2_2026-11-01`, geprüft gegen `main` `14ed3a16a622` (01.11.2026, 05:42 UTC), `PHASENPLAN.md` REV.1.0, `ENTSCHEIDUNGEN.md` v1.4, `STATUS.md`, nativen Code und Protobuf. Verbindlicher Umsetzungsauftrag und neue Nutzerentscheidungen; **kein Nachweis einer abgeschlossenen Implementierung oder Mainnet-Qualifikation**. DEC-01…17 und die gesondert erteilten Freigaben bleiben verbindlich. DEC-18 ergänzt sie, ebenso die eng begrenzte Freigabe C-11; C-01, C-04, C-05 und C-10 werden präzisiert (§4). **C-12 ist ausdrücklich NICHT erteilt.** Status- und Gate-Autorität bleiben `BUGS.md` bzw. `PHASENPLAN.md`; bestehende Phasen und Anforderungen werden nicht gestrichen.
- **Verifizierte Codefakten (Ausgangslage, kein Soll-/PASS-Nachweis; vor jedem Commit am dann aktuellen `main` erneut prüfen, PASS nur für wirklich geprüfte Produktionspfade):**
  - Native RPC-/WSS-Endpunktlisten, Fallback, Profil-/Datei-/CLI-Konfiguration und Parametergrenzen existieren (`solana/src/runtime_config.rs`: `RuntimeConfig`, `Resolved`, `ConfigFile`) ⇒ die vorhandene Konfigurationsautorität aus DEC-05 erweitern, keine zweite Registry.
  - Der BOOT-Netzwerkaudit verlangt **ein** RPC-/WSS-Paar, TLS und deaktivierten Fallback (`boot_network.rs`: `NetworkReads::from_config`) ⇒ ein ausdrücklich gewähltes und gebundenes Paar je Audit/Run.
  - Audit und Evidence hashen bisher die **gesamte URL**, ggf. einschließlich `api-key` in der Query (`boot_network.rs::run`; `provider_evidence.rs::validate_provider_evidence`) ⇒ kanonische secretfreie Provider-/Credential-Revisionsbindung einführen und migrieren.
  - BOOT-05 verlangt `DeploymentPin::Reference`, bei Abweichung `PROGRAM_IDENTITY_MISMATCH` (`boot_network.rs::program`) ⇒ kein Hash-Re-Pin ohne fachliche Parität; nach Promotion neuer Audit.
  - BOOT erlaubt derzeit `GetGenesisHash`, `GetVersion`, `GetHealth`, `GetSlot`, `GetMultipleAccounts`; Budget per `boot_calls`, `boot_seconds` (max. 300 s) (`NetworkReads::send`, `boot_budget`) ⇒ das neue Vormesswerkzeug braucht eine getrennte, vor dem Audit erteilte Read-Zulassung.
  - Provider-Record, Validierung und `.p0j`-Store sind vorhanden, ein produzierender nativer Provider-Prober nicht. Das Cargo-Feature `historical` existiert (DEC-09, `bc33567e`); `ProviderEvidence::from_measured_json` ist nicht sichtbar feature-gated ⇒ produktives Messwerkzeug ergänzen; den JSON-Historienleser nach Consumer-Abgleich unter das vorhandene Feature stellen, ohne historischen Datenzugriff zu brechen.
  - Die Jupiter-Produktionsbasis-URL ist konstant (`execution/route.rs::DEFAULT_BASE`); `JupiterV2RouteProvider::new()` liest `Limits.jupiter_timeout_ms` (Registry-Default 1500 ms, `P0_JUPITER_TIMEOUT_MS`) ⇒ URL-Quelle und Provider-Metadaten konsolidieren; Timeout ist verdrahtet, Wertübernahme per Regressionstest schützen.
  - Der native Protobuf-/Frontend-Pfad enthält noch keine vollständig belegte Provider-Konfigurationsbedienung (`protocol/schemas/darkbot/project0/v1/system.proto`; `F0_VAR_01_MATRIX.md` C-CP-3; `frontend/README.md`) ⇒ vollständigen Hin- und Command-Rückweg in **bestehenden** Reitern herstellen.
  - P8-CTRL schreibt initial `DISABLED` für alle Wallet-/ECON-Schalter vor; diese ersetzen BOOT-05 nicht ⇒ Boot-/Deployment-/Provider-Gates und wirtschaftliche Capability-Gates bleiben getrennt.
- **§1 Endpunkt und Provider-Konfiguration (Auswahl OTHER):**
  1. Für die genehmigten Messungen **primär Helius Free**, sofern für das tatsächlich bestehende kostenlose Kontingent RPC-/WSS-Methoden, Provider-Budget und Nutzungsbedingungen vor dem Audit nachweislich geeignet sind. Sekundär darf **Public Solana** (`https://api.mainnet-beta.solana.com`, `wss://api.mainnet-beta.solana.com`) **manuell** als anderes einzelnes Paar gewählt werden. Kein stiller Fallback, kein Paid-Provider, keine implizite Freigabe zusätzlicher Quellen.
  2. Das gewählte Paar wird mit Provider-ID, Endpunkten, Tarif-/Kostenstatus, Limits, Credential-Revision und Run-/Audit-Identität gebunden; für den Boot-Audit genau ein RPC- und ein WSS-Endpunkt, Fallback aus; bei Wechsel neuer Provider-Nachweis und neuer Audit.
  3. Capture-Quelle zunächst **`logs`**. Der frühere HTTP-403-Befund für `transactionSubscribe` belegt nur die damals getestete Quelle, nicht die Unverfügbarkeit bei allen kostenlosen Providern. Andere Methoden nur nach gesondert nachgewiesener Capability und Freigabe; kein ungefragtes Ausprobieren.
  4. Die beschränkte Lesefreigabe C-04/C-05/C-10 gilt für **das ausgewählte, nachweislich kostenlose Helius-Free-Paar** bzw. nach manueller Wahl für das Public-Solana-Paar; exakte Credential-/Endpoint-IDs und Limits werden vor dem jeweiligen Start im Freigaberecord protokolliert. Der frühere Umfangssatz „nur die konfigurierten öffentlichen RPC/WSS-Endpunkte“ (§4) ist durch diese providergebundene Freigabe ersetzt; daraus folgt kein weiter gehender Zugriff.
- **§1.2 Secretfreie Identitätsbindung (vor der Provider-Vormessung):** die Bindung per `hash(url)` wird durch einen versionierten kanonischen Vertrag ersetzt:
  - Identität über Provider-ID, Provider-/Transporttyp, kanonischen Endpunkt ohne Benutzerinfo oder Secret-Query, Tarif/Plan, unveränderliche Credential-ID und Credential-Revision sowie die relevanten wirksamen Limits/Konfigurationsversionen; binär kanonisiert und domänensepariert gehasht; dieselbe Implementierung für RPC, WSS, Evidence, Audit und Run-Manifest.
  - API-Key-Werte erscheinen nie im gebundenen Record. Jeder Keytausch erzeugt atomar eine neue, nicht wiederverwendbare Credential-Revision, auch bei gleichem Alias. Der Controller verifiziert bei Benutzung, dass die aufgelöste Revision exakt die autorisierte ist; TOCTOU-/Restart-/Rotationstests verpflichtend.
  - Änderungen an Provider, Endpunkt, Plan, Credential oder budgetrelevanter Konfiguration invalidieren die jeweils betroffenen Evidence-, Audit- und FE-06-Bindungen. Aktive Runs behalten ihre unveränderliche Identität; kein Wechsel mitten im Run.
  - Alte `hash(url)`-gebundene Records bleiben für historische Analyse lesbar, werden ohne erneute aktuelle Qualifikation nicht als neue Audit-/LIVE-Evidence anerkannt; keine pauschale Löschung oder Umdeutung.
  - Die URL-Prüfung lehnt Nutzerinfo und geheime URL-Parameter in der persistenten Konfiguration ab bzw. migriert sie kontrolliert in einen Credential-Eintrag. Ein an `localhost` gebundenes Gateway allein ist kein Authentifizierungsnachweis.
- **§1.3 DEC-18 — vollständige Provider-Bedienung vor dem Smoke.** Priorität und Reihenfolge beschlossen: **die vollständige produktive Provider-Konfiguration bis in das bestehende Project_0-Frontend wird vor den genehmigten Mainnet-Smoke-Läufen implementiert und offline abgenommen** (ersetzt die Entwurfsformulierung „DEC-18 nach dem Smoke“). Vorhandene native Komponenten sind Ausgangspunkt; keine Neuarchitektur.
  1. **Native Autorität:** `RuntimeConfig`/`Resolved`, bestehende Provider-/Source-Control, Budgets und den gemeinsamen Project_0-Controller erweitern. Einheitlicher typisierter Provider-Descriptor: ID, Typ (RPC, WSS, Geyser, Jupiter, Jito), Endpoint, Auth-Typ, **Credential-Referenz statt Key**, Priorität, konfigurierte Kapazität, Quelle, Tarif/Entitlement, Qualifikations-ID, Version, `requested/effective`, Fehler-/Blockgrund, Restart-Regel.
  2. **Keine versteckten Produktionsdefaults:** Jupiter-`DEFAULT_BASE` als validierter, im Rust-Registry-Vertrag konfigurierbarer Default statt festem Konstruktorwert. Ist: `JupiterV2RouteProvider::new()` liest bereits `active().limits.jupiter_timeout_ms` (Default 1500 ms in `runtime_config.rs`); diesen Pfad mit einem Test für einen abweichenden konfigurierten Wert absichern, nicht neu implementieren; weicht er auf einem späteren HEAD ab, erst den tatsächlichen Aufrufpfad reparieren. Bestehende Helius-Konstanten nur als nachvollziehbare, versionierte Default-Profile; keine parallele URL-Auflösung im Capture-Prozess. Das P6-MT.7-Quota-Budget bleibt die alleinige Jupiter-Budgetautorität.
  3. **Protobuf:** bestehende `darkbot.project0.v1`-Projektionen und action-spezifische typisierte Commands ergänzen (Schema-IDs, Feldnummern, Verfügbarkeits-/Berechtigungsregeln). Protobuf ist Transport, kein alternativer Config- oder Secret-Speicher; Status transportiert nur redigierte Provider-Identität, Host, Quelle, Qualifikation, Versions-/Gültigkeitsstatus und Limits.
  4. **Bestehende GUI:** RPC/WSS/Geyser in **Capture/Health**; Jupiter/Jito und ihre wirtschaftlich relevanten Quoten/Readiness in **Liquidator**. Anlegen, Bearbeiten, Auswahl, De-/Aktivierung, Priorität und API-Key-Status/Rotation funktionieren. Kein neuer Reiter. UI-Bestätigung oder `ACCEPTED` ist nicht `effective`; erst der vom Owner bestätigte Zustand.
  5. **Ein Controller, ein Vertrag:** CLI (`p0ctl provider …`) und GUI nutzen dieselbe native fachliche Autorität, CAS-/Config-Version, Allowlist, Validatoren, dauerhafte redigierte Receipts und Statusprojektionen. Beide setzen/rotieren/widerrufen Provider-Credentials über den einheitlichen, nichtpersistierenden nativen Credential-Eingang; CLI-Eingaben nie als Klartextargument oder geloggte Umgebungsvariable. Änderungen werden als neue Konfiguration gestaged und gemäß DEC-05 erst beim kontrollierten Neustart aktiv; laufende Audits/Runs werden nicht umkonfiguriert.
  6. **macOS-Credential-Store, CLI-/GUI-Parität:** API-Keys und sonstige Provider-Credentials im **macOS-Schlüsselbund**, nicht in Protobuf-STATE, Journalen, Receipts, Gateway-Ledger, URL-Logs, Frontend-/Browser-Storage oder Testartefakten. Eingabe, Rotation und Widerruf über das Frontend oder `p0ctl provider credential set|rotate|revoke`, über denselben nativen Controller und denselben nichtpersistierenden Credential-Eingang. Die CLI liest Geheimnisse verdeckt aus einer geschützten Eingabe (kein API-Key in Argumenten, Shell-History, Env-Ausgabe oder Logs). Weil das F6-Gateway sein Command-Ledger dauerhaft führt, separiert der authentisierte Gateway-/Engine-Pfad die Secret-Payload **vor jeder dauerhaften Idempotenz-/Command-Protokollierung**; dauerhaft nur Credential-ID/Revision, Request-ID und redigierte Receipts. Session/Origin/CSRF/CAS/Idempotenz bleiben. Kann die Ledger-Pipeline das nicht sicher, wird der **bestehende** Gateway-/Controller-Pfad um einen dedizierten nichtpersistierenden Credential-Eingang ergänzt; kein zweiter fachlicher Controller, keine freie Browser→Signer/RPC-Durchleitung. Positive und negative Leak-/Retry-/Crash-/Replayschutztests verpflichtend.
  7. **`.env`-Migration:** vorhandene Schlüssel dürfen **einmalig kontrolliert** importiert werden (der einzige weitere Credential-Eingang neben Frontend und CLI); danach bezieht der produktive Pfad den Key über die Credential-Referenz. Keine dauerhaft parallelen `.env`- und Keychain-Secret-Autoritäten; Originale nicht ungefragt löschen. Auf Systemen ohne freigegebenen Credential-Store sicher blockieren, kein unsicherer Fallback.
  8. **FREE_ONLY:** Tarife und tatsächliche Quoten als Provider-Evidence qualifizieren; keine bezahlten Anfragen, keine automatischen Upgrades, keine nicht gemessenen Fallbacks.
  9. **Abnahme:** Rust→Protobuf→Reporter→Gateway→WebSocket→Adapter→Store→Reiter und Reiter→typisierter Protobuf-POST→Gateway→Controller→dauerhaftes redigiertes Receipt→STATE. Tests für ungültige Endpunkte, Secret-Exfiltration, Key-Rotation, CAS/Idempotenz, Neustart, Run-Drift, Provider-Fehler, Qualified→Unqualified, verlorenes ACK, Frontend-Detach, Berechtigung und konkurrierende Eingaben. `protocol/scripts/check.sh --check`, Gateway-/Frontend-/Playwright- sowie vollständiges Project_0-Gate und CI auf dem finalen SHA.
  10. **Dokumentationszuordnung:** in `PHASENPLAN.md` (N-5-Control-Inventar) und den F0-VAR-Matrizen verankert, kein zweiter Masterplan; Iststatus erst nach funktionierendem End-to-End-Nachweis `OFFLINE_VERIFIED`.
- **§2 BOOT-05 und P1B → P1A (Auswahl „Promotion jetzt mit freigeben“, bedingt):**
  1. **C-01 Schritte 5–6** sind im read-only-Umfang erteilt. Fehlende native Read-Werkzeuge nutzen/ergänzen: aktuelle ProgramData/Executable/Upgrade-Authority, Chain-/IDL-/Source-Nachweis, Group/Banken einschließlich Resize-Census, Oracles, Integrationen und der erforderliche Token-/ATA-Bytecode.
  2. **Die Parität belegt den real deployten Stand:** Code-/Binary-/IDL-/Instruktionslayout, Bank-Layouts (einschließlich Größenvarianten), Decoder, Health/Sizing, relevante Integration und State-/Oracle-Semantik. Reproduzierbarer/verifizierbarer Build gegen einen belegten, gepinnten veröffentlichten Quellstand, **wenn verfügbar**; fehlt zwingende Quell-/Binary-Parität: `BLOCKED/NOT_RUN`, keine stillschweigende Hash-Umpinnung.
  3. Das RC2-Paket `c9decd7f` ist **kein automatischer Nachweis**, dass die reale Deployment-Version identisch ist oder alle Paritäts-Gates erfüllt; die Gleichheit wird über die gelesenen realen Bytes und Funktions-/Fixture-Prüfungen nachgewiesen.
  4. **C-01 Schritt 7 ist bedingt erteilt:** erst nach vollständig dokumentiertem PASS der Schritte 5–6 und aller für die tatsächliche Version verpflichtenden P1A-/P1B-Paritätsprüfungen dürfen Code-Pins und `CURRENT_MAINNET.md` auf die qualifizierte aktuelle ProgramData/Programmversion promoviert werden; Review, Gate, CI auf dem exakten SHA und **erneuter** Network-Audit wegen geänderter Code-/Binary-/Config-Identität sind Pflicht.
  5. Schlägt die Parität fehl oder ist die Version unbekannt: **keine Promotion, kein künstlicher BOOT-05-PASS, keine Market-Ausnahme**; Ursachen und nötigen P1B-Prüfzyklus mit Ticket/Belegen dokumentieren; unabhängig zulässige Offline-Arbeiten fortsetzen.
  6. **C-12 bleibt NICHT ERTEILT:** eine Sonderzulassung `DEPLOYMENT_NOT_REFERENCE` für Market ist weder beschlossen noch implementierungsseitig zu umgehen; ein späterer Änderungswunsch braucht eine ausdrückliche eigenständige Freigabe; kein automatischer Fallback.
  7. Die Boot-/Deployment-Freigabe ist **keine wirtschaftliche Freigabe.** Initial und nach Restart alle `WALLET:<Pubkey>`, `ECON_SIGN`, `ECON_ARM`, `ECON_SEND`, `ECON_ATTEMPT`, `ECON_LIVE` auf `DISABLED`; LIVE requested bleibt OFF, effective ohne alle erforderlichen Grants/Gates BLOCKED. Die vorhandenen `live_enabled`-/`submission_enabled`-Altprüfungen **nur** nach nachgewiesen mindestens gleichwertiger vollständiger produktiver P8-CTRL-Abdeckung an allen Grenzen ersetzen; nie ersatzlos entfernen.
- **§3 Provider-Evidence (Auswahl „Natives Messwerkzeug bauen“):**
  1. Über den vorhandenen Rust-Provider-/Budget-/Controller-Pfad `p0ctl provider measure` (oder passender bestehender Command-Name) produktiv ergänzen, providergenerisch entworfen, jetzt nur das konkret manuell ausgewählte RPC-/WSS-Paar messen; für C-02 später erweiterbar, **C-02 selbst bleibt nicht erteilt**.
  2. **Vollständiger, codeabgeleiteter `ProbeSpec` je Modus:** vor dem ersten Netzwerkzugriff die tatsächlich aufgerufenen RPC-Methoden und WSS-Subscriptions der Pfade **BOOT-Audit, 600-s-econ4h-Kalibrierung und 30-min-C0-MO** aus deren realen Callgraphs/Adaptern ermitteln, in den typisierten Probe-/Freigabevertrag übernehmen und auf Abweichungen testen. BOOT-Methoden derzeit `getGenesisHash`, `getVersion`, `getHealth`, `getSlot`, `getMultipleAccounts`; darüber hinaus je nach nachgewiesenem Aufrufpfad insbesondere `getProgramAccounts`, `getAccountInfo`, `getLatestBlockhash`, `getFeeForMessage`, `simulateTransaction`, `getSignaturesForAddress`, `getTransaction`, `getBlock` sowie `logsSubscribe`, `programSubscribe`, `slotSubscribe` — **Prüfkandidatenliste, keine Behauptung, dass jeder Modus alle nutzt**; keine Methode nur wegen ihres Namens ungeprüft freigeben. `sendTransaction`, Signieren und werttragende Endpunkte bleiben ausgeschlossen. Die C-01-Spezialreads haben eine eigene gebundene Teilfreigabe.
  3. **Methoden-/Subscription-Probe vor dem eigentlichen Run:** jede für den gewählten Modus nachgewiesene und freigegebene Methode/Subscription **genau einmal** mit einem typisierten, minimalen, begrenzten, zum echten Call-Schema passenden Read-Test; keine unkontrollierten großen `getProgramAccounts`-/`getBlock`-Antworten; WSS-Subscriptions nötigenfalls innerhalb einer Verbindung sequenziell. Fehlender Provider-Support → `METHOD_UNSUPPORTED`; fehlende geeignete Eingabefixture, Budgeterschöpfung oder unzureichende Berechtigung → jeweils eigener `NOT_PROVEN`/`BLOCKED`-Grund, nicht `METHOD_UNSUPPORTED`. Eine erforderliche, nicht positiv nachgewiesene Methode blockiert ausschließlich den betreffenden Modus. Gemessen: HTTP-/RPC-/WSS-Ergebnisse, ACK/Liveness, Fehler/429/Retry-After, Timeouts, Reconnects, Slots/Gaps, Request-Anzahl, Bytes, Zeitfenster; p50/p95 nur bei ausreichender Stichprobe mit N und Aussagegrenze; aus einem Einzeltest keine Perzentil-/Kapazitätsqualifikation.
  4. **Direkt typisierter, versionierter `.p0j`-`PROVIDER_EVIDENCE`-Record** aus echten Messergebnissen: Bindung nach §1.2, Messplan/-fenster, nach Modus aufgelöste Methoden und Subscriptions mit Ergebnistyp (`SUPPORTED`, `METHOD_UNSUPPORTED`, `NOT_PROVEN`, `BLOCKED`), Provider-/Plan-/Credential-Revisions-ID, FREE_ONLY-/`paid=false`-Nachweis, Budgets, Ergebnisse. Der JSON-Import bleibt ausschließlich für kompatible historische Lesung; `from_measured_json` nach Prüfung seiner realen Consumer unter das **bestehende** Feature `historical` stellen, sofern der historische Lese-/Migrationsvertrag das zulässt, andernfalls die fehlende Feature-Grenze konkret belegen und beheben. **Kein produktiver Messpfad über JSON-Import.**
  5. **C-11 — eng begrenzte Lesefreigabe vor dem Network-Audit:** nur dieses Messwerkzeug; maximal **120 Sekunden**, **1 RPC-Request/s**, **höchstens 120 RPC-Versuche insgesamt**, **16 MiB kumulierte RPC-Response-Bytes**, **eine WSS-Verbindung**, **höchstens zwei gleichzeitig aktive, höchstens drei unterschiedliche im `ProbeSpec` genehmigte Read-Subscriptions insgesamt** (die dritte nur sequenziell nach Abmeldung einer anderen), **genau ein manuell ausgewähltes FREE_ONLY-Paar**, keine Paralleltests/Lastsonden. Diese Zahlen sind Entscheidungsobergrenzen, keine nachgewiesene Providerkapazität; bei kleineren Kontingenten strengere Limits. Vor dem ersten Read ein **modusbezogener typisierter `ProbeSpec`** mit Provider-ID, Code-/Adapter- und Konfigurationsidentität, exakten RPC-Methoden/Subscriptions und sicheren Probeparametern, Tarif, Limits, Run-ID, Dauer, Kostenfreiheit und Config-Hash; **konkrete Nutzerbestätigung des Offline-Tokens über den bestehenden Boot-/Control-Weg** erforderlich. Keine verdeckte Ausweitung durch neu hinzugekommene Adapter oder Methodennamen. Unterstützt der Controller keinen Pre-Audit-Probe, wird dessen gebundener Zulassungspfad ergänzt; **nie den Network-Audit durch einen selbst erklärten PASS umgehen**.
  6. **C-01-Reads** (§2) laufen gesondert über ihre Teilfreigabe und einen eigenen vorab bestätigten Read-Scope/Boot-/Provider-Budget; **nicht pauschal von den C-11-Kontingenten mitautorisiert**. Jeder zusätzliche Read außerhalb des freigegebenen Scopes braucht eine eigene Nutzerfreigabe; kein Endpunkt-/Methoden-Fallback ohne Zustimmung.
  7. Bei PASS wird der Record im vorhandenen Store unter seiner unveränderlichen ID gespeichert; der Audit validiert Alter, Identität, Tarif, gemessene Kapazität und relevanten Scope erneut. `FAILED/BLOCKED/NOT_RUN` werden nicht durch Importe oder zeitliche Wiederholung zu PASS. Status, ausgewählter Provider, Evidence-ID und Ablehnungsgründe sind vollständig im Capture-Reiter sichtbar.
  8. Tests und Evidence: positive/negative Loopbackfälle für Timeout, 429, unpassende Credential-Revision, Drift, fehlerhaften Record, zweifache Messung, Abbruch, Wiederholung und Restart; Gate/CI plus End-to-End Frontend/CLI.
- **§4 Jupiter (Auswahl „Jupiter-Reads zulassen“):**
  1. Nur die für die genehmigten Messungen erforderlichen **Quote-/Build-Reads** über den bestehenden P6-MT.7-Jupiter-Bucket. Provider-URL, Auth-Typ und optionaler Credential-Alias aus derselben nativen DEC-18-Provider-Konfiguration; anonyme Free-Variante bis zur gesondert geprüften Key-/Plan-Qualifikation. Keine `/execute`-/`submit`-Aufrufe, keine echten wirtschaftlichen Transaktionsversuche.
  2. Die verfügbare kostenlose Rate wird vorab aus belegter Tarif-/Provider-Evidence bestimmt und auf `Limits`/Bucket angewendet. 401/403/429, Retry-After, Timeouts und Ausfälle wahrheitsgemäß loggen; nur begrenzte, budgetierte zulässige Wiederholungen; keine Umgehung der Tarife durch Key-/Endpoint-Rotation.
  3. `econ4h`-Kalibrierung und andere rein lesende Messpfade laden **keinen privaten Produktivschlüssel**; für die Routenbewertung den ausdrücklich konfigurierten öffentlichen Taker-/Wallet-Pubkey. Wo unsignierte Simulation erforderlich und zulässig ist, nur `sig_verify=false` mit eigener Kennzeichnung **`UNSIGNED_SIMULATION`**; das ist **kein** Nachweis vollständiger signierter FE-02-/FE-03-Parität. Reale Signaturen ausschließlich in gesondert freigegebenen, exakt gebundenen SIM_ONLY-/LIVE-Pfaden mit WALLET-/ECON_SIGN-Permit.
  4. Jupiter-Kosten, Fee-/Slippage-/MinOut-/Exit-Annahmen sowie fehlende Quote-/Build-Möglichkeiten als Evidence oder `NOT_RUN/UNKNOWN`; keine ökonomische Freigabe aus einem erfolgreichen HTTP-Read.
- **§6 Verbindliche Reihenfolge:** 1. **DEC-18 vollständig implementieren und offline abnehmen** (native Provider-/Credential-/Config-Pfade konsolidieren; secretfreie Bindung nach §1.2; Protobuf, Gateway, Frontend, CLI mit beiden gleichwertigen Credential-Eingängen und kontrolliertem einmaligem `.env`-Import; Credential-Rotation; Jupiter-URL in den P6-MT.7-Vertrag, Timeout-Übernahme regressionsgesichert; Safety und Run-Identität erhalten; End-to-End-Tests und CI auf finalem SHA, Push `main`). 2. **C-11-Messwerkzeug** mit demselben Provider-Vertrag, eigenem begrenztem Pre-Audit-Read-Permit und aus den realen Modus-Callgraphs abgeleitetem `ProbeSpec`, typisiertem Record, Offline-Tests, Review, Gate/CI auf `main`; erforderliche, aber unqualifizierte Methoden blockieren den jeweiligen Modus vor dessen Start. 3. **C-01-Schritte 5–6** mit konkreter Token-Freigabe ausführen, Programme/Group/Banken/IDL/Oracles/Bytecode prüfen und dauerhaft belegen. 4. **C-01-Schritt 7** nur bei vollständig positivem Paritätsnachweis regulär promoten (Review/Gate/CI), sonst BOOT-05 BLOCKED und Ursachen abarbeiten. 5. **C-04** neuer Network-Audit für die finale Code-/Binary-/Config-/Provider-Identität; alle BOOT-03…08-Pflichtchecks wirklich PASS. 6. **C-05** maximal 600-s-Kalibrierung mit vollständigem Run-/Provider-/Budget-/Coverage-Ledger, nur nach erfüllten Gates. 7. **C-10** separat die 30-Minuten-C0-MO-Beobachtung über denselben Startpfad; kein automatischer C0-MO-Gesamt-PASS, fehlende Markt-/Provider-Evidence ausdrücklich ausweisen. 8. Nach jedem Paket `BUGS.md`/`STATUS.md`/`MEASUREMENT_LEDGER.md`/F0-VAR und Code-SHA konsistent aktualisieren; danach die verbleibenden unabhängigen offline umsetzbaren REV.1.0-Pakete (darunter DEC-17) entlang des Phasenplans; keine abgeschlossenen Komponenten neu aufbauen.
- **§7 Unveränderte technische Regeln und Definition of Done:** FREE_ONLY; NO MOCK / NO LEGACY / NO HARDCODING in produktiven Pfaden; keine zweite Provider-/Controller-/Journal-/Gateway-/Frontend-Architektur. Keine internen JSON-Handoffs (native typisierte Rust-Werte und versionierte Binärrecords; Protobuf als Wire-Transport; JSON nur an zwingenden externen Solana-/Jupiter-/Jito-Grenzen oder für klar abgegrenzte historische Leseformate). Genau die fünf bestehenden Project_0-Reiter, F0-VAR-Verdrahtung nach `PHASENPLAN.md` P0-PROD-CTRL-05–12 und `frontend/README.md`; keine BSC-Änderungen und keine BSC-Latenzziele. Finanzielle Schalter nach P8-CTRL initial `DISABLED`; BOOT-/Provider-/ProgramData-/Risk-/FE-04-/FE-06-Gates bleiben zusätzlich, nicht alternativ. Echte produktive Consumer und reproduzierbare positive/negative Tests, Crash-/Restart-/Drift-/Secrets-/CAS-/Idempotenz-Gates; optional höchstens ein read-only Reviewer nach P0-PROD-CTRL-10; Gate/CI auf exakt dem gepushten SHA. Ausschließlich `main`, kein Branch, kein Force-Push; keine stillschweigende Freigabe eines Mainnet-Reads, höheren Kontingents oder einer werttragenden Aktion durch einen Dokumenteintrag. Statusstufen strikt getrennt: `DESIGN_APPROVED` → `CODE_COMPLETE` → `OFFLINE_VERIFIED` → `EXTERNAL_QUALIFIED` → ggf. gesondert `LIVE_AUTHORIZED`; ein ungetesteter Teilpfad bleibt `OPEN/PARTIAL/NOT_RUN`.
- **Status 01.11.2026:** DEC-18 `DESIGN_APPROVED`, nicht implementiert; C-11 erteilt, nicht ausgeführt. (Umsetzungsstand: `STATUS.md` DEC-18-Zeile, `BUGS.md` P0-017…P0-024.)
- **Nutzerentscheidungen 01.11.2026 (abends):** **R-PY** — die hash-gepinnten Python-Leser `operations/cli/measurement_contract.py` und `operations/verification/audit_market_quick.py` bleiben unverändert historische Offline-/Paritätsreferenzen: kein Re-Pin, keine Neuimplementierung der vorhandenen nativen INDEX-/CROSSCHECK-Funktionen; produktive Launcher führen ihnen keine `.env`-/`HELIUS_API_KEY`-Credentials zu (Regressionstest); produktive Online-Reads nur über die Provider-Registry und die qualifizierten Rust-Werkzeuge; `c0_census.py` wird durch P0-025 ersetzt. **B-KC** — nach Push und Gate des laufenden DEC-18-Pakets ein lokaler Keychain-Integrationstest mit ausschließlich synthetischem Dummy-Credential über `p0ctl → macOS-Schlüsselbund → Runtime-/Reporter-Binary` einschließlich der Zugriffsrechte und eines Starts ohne interaktiven Zugriff; kein Netz, keine echten Keys, kein Mainnet; einen macOS-Dialog bestätigt der Nutzer selbst; Dummy-Eintrag danach entfernen; Ergebnis mit Binaries, Kommandos, Rückgabecodes und PASS/BLOCKED, kein künstlicher PASS.
- **Nutzerentscheidung 02.11.2026:** DEC-18 auf das tatsächlich Notwendige begrenzt — bestehende Provider-Verwaltung und GUI bleiben, keine zusätzliche GUI-Entwicklung oder Architektur; bestandene Tests nicht wiederholen, nur geänderte Funktionen gezielt prüfen; für C-11 nur die benötigten RPC-/WSS-Funktionen und Credentials; einmalige vollständige DEC-18-Abnahme und CI auf dem finalen Code (`d7a3f504`, CI 41441692744/41441692776 grün; bsc-Workflow rot durch den BSC-Test `live_e2e` (macOS) aus `4a1bae86`), danach direkt C-11 (P0-022). P0-024 bleibt Voraussetzung von C-05.


---

## 3. Messfragen (nicht entschieden; bis zur Messung gilt der heutige Wert)

| ID | Frage | Erforderliche Messung | Bis dahin |
|---|---|---|---|
| B-1 | Recovery-Staging (V2-12) | A/B-Snapshot-Dauer, Events/s, Queue-Peak, Overflow und Gap/Reconnected aus **demselben** qualifizierten Lauf | heutiges Verhalten |
| B-2 | Worker-Zahl Shadow/LIVE | Owner-Block-p99 im Sweep unter realer Last | heutige Werte, als vorläufig markiert |
| B-3 | Sweep-Batch-Werte (Events, Alter, Burst) | Detektionslatenz und Owner-Duty-Cycle | heutige Defaults |
| B-4 | Lane-Gewichte 8:3:1 | MT0 | vorläufig |
| B-5 | Pool-Idle und Keepalive (V2-07) | Idle-A/B 0/30/90/120 s | heutige Defaults aus `Limits` |
| B-6 | MT6–MT10 | nach C0-MO-4h und gemessenem Engpass | gesperrt |
| B-7 | Retention-Byte-Peak (R12) | erster realer Lauf | heutige Grenzen |
| B-8 | Mindestzahl N für die Gates aus Entscheidung 4 und 6 | vom Nutzer festzulegen | ACTIVE gesperrt |
| B-9 | CPU-Budget für OBSERVE (Entscheidung 6) | Owner-Blockzeit in OBSERVE | konservativer Startwert, gesampelt |

---

## 4. Freigaben

Jede Freigabe wird einzeln und ausdrücklich erteilt, mit Umfang, Provider, Budget und Dauer. Eine Freigabe umfasst nie mehr als ihren eingetragenen Umfang.

| ID | Freigabe | Status | Hinweis |
|---|---|---|---|
| C-01 | P1B-Schritte 5–7 (Bank-Resize-Read, FE-01-Beobachtung, Promotion P1B → P1A) | **Schritte 5–6 ERTEILT 01.11.2026** im festgelegten Read-only-Scope mit eigenem, vorab bestätigtem Read-Plan (Bank-Resize-Read, FE-01-Beobachtung, P1A-01 ProgramData/Executable/Upgrade-Authority, Chain-/IDL-/Source-Nachweis, Group/Banken inkl. Resize-Census, Oracles, Integrationen; DEC-18 §2.1); **Schritt 7 BEDINGT ERTEILT 01.11.2026** — ausschließlich nach dokumentiertem vollständigem Paritäts-PASS (DEC-18 §2.2–2.5) | Voraussetzung für FE-01 und damit für den ganzen Profit-Pfad; nicht von C-11 mitautorisiert |
| C-02 | FREE_ONLY-Provider-Qualifikation inkl. zweitem unabhängigen Endpunkt | NICHT ERTEILT | Dient Entscheidung 3 und 8; das C-11-Werkzeug ist dafür später erweiterbar |
| C-03 | S1-08 PublicNode-Capability-Smoke | NICHT ERTEILT | — |
| C-04 | G-B Network-Boot-Audit | **ERTEILT 01.11.2026** über DEC-07, nur nach den nötigen Preflights und gültiger Provider-Evidence; nach einer Promotion erneut | alle BOOT-03…08-Pflichtchecks müssen wirklich PASS sein |
| C-05 | 600-s-Kalibrierung | **ERTEILT 01.11.2026**: eine Kalibrierung, maximal 600 s, ausschließlich read-only und nach vollständig bestandenen Start-/Audit-Gates | ohne privaten Produktivschlüssel (DEC-18 §4.3); eigene Run-ID, Approval-/Audit-/Config-Identität |
| C-06 | C0-E1 4h-Master | NICHT ERTEILT | — |
| C-07 | 72-h-P6-LT-Shadow | NICHT ERTEILT | — |
| C-08 | FE-05 Read-only-Preflight | NICHT ERTEILT | — |
| C-09 | FE-06 One-Shot und FE-07 | NICHT ERTEILT | einzelne, gesonderte Nutzerfreigabe |
| C-10 | eine 30-Minuten-C0-MO-Beobachtung einschließlich der genehmigten Token-/ATA-Bytecode-Reads (DEC-17 d) | **ERTEILT 01.11.2026**, read-only | Kalibrierung, **kein** C0-MO-4h-PASS (C-06); MT6–MT10 bleiben gesperrt (B-6); eigene Run-ID |
| C-11 | Pre-Audit-Provider-Vormessung des ausgewählten RPC-/WSS-Paars und seiner im `ProbeSpec` gebundenen modusrelevanten Methoden/Subscriptions | **ERTEILT 01.11.2026 NUR innerhalb der Grenzen DEC-18 §3.5** (≤120 s, 1 RPC-Request/s, ≤120 RPC-Versuche, 16 MiB kumulierte RPC-Response-Bytes, eine WSS-Verbindung, ≤2 gleichzeitig aktive und ≤3 unterschiedliche Read-Subscriptions, genau ein FREE_ONLY-Paar) **und nach konkreter Offline-Token-Bestätigung** | Messdauer kommt getrennt zur 600-s-Kalibrierung und zur 30-min-Beobachtung hinzu; keine Marktcapture |
| C-12 | Market-Boot mit `DEPLOYMENT_NOT_REFERENCE` | **NICHT ERTEILT** | keine Umsetzung, keine Umgehung, kein Fallback (DEC-18 §2.6) |

**Gesamtscope der Freigaben vom 01.11.2026 (C-01, C-04, C-05, C-10, C-11):** ausschließlich die genau benannten FREE_ONLY-Lesevorgänge mit dem ausdrücklich ausgewählten, nachweislich kostenlosen Helius-Free-Paar oder — nach manueller Wahl — dem Public-Solana-Paar (`https://api.mainnet-beta.solana.com`, `wss://api.mainnet-beta.solana.com`); exakte Credential-/Endpoint-IDs und Limits werden vor dem jeweiligen Start im Freigaberecord protokolliert (DEC-18 §1.1). Keine automatische Erweiterung auf andere Provider, kein Paid-Provider, keine realen Produktivsignaturen, keine Submission, keine wirtschaftliche Transaktion, kein 4h-Lauf. Ablauf über den Boot-Controller (Offline-Preflight → Netzwerkfreigabe → Audit → Startfreigabe → Start); Provider-Evidence als typisierter Record; Ergebnisse in `MEASUREMENT_LEDGER` und `STATUS`. C-05 und C-10 sind unterschiedliche Runs mit eigener Run-ID, Approval-/Audit-/Config-Identität. Jeder reale Netzwerk-Schritt erfordert seine konkrete operatorseitige Freigabe und den zugehörigen gültigen Boot-/Control-Token; kein eigenmächtiges Verlängern oder Self-Approval durch einen Agenten. **Reihenfolge:** erst DEC-18 und das C-11-Werkzeug offline abgenommen auf `main` (DEC-18 §6), dann die Netzwerkschritte. Der frühere Satz „nur die konfigurierten öffentlichen RPC/WSS-Endpunkte“ ist hierdurch ersetzt.

---

## 5. Empfohlene Umsetzungsreihenfolge (Empfehlung, keine Vorgabe)

1. **15** – Git-Regel, sofort.
2. **14** – Register konsolidieren, damit es ab jetzt eine einzige Zählung gibt.
3. **5** – Registry zuerst, weil 1, 6, 7, 8 und 11 neue Konfigurationsfelder brauchen.
4. **9, 10** – JSON-Reste.
5. **1** – Blockhash-Frühcheck.
6. **12** – V2-14-Reste.
7. **2, 13** – Admission-Kette.
8. **3** – Non-Landing-Beweis.
9. **4** – Holds.
10. **11** – Bundle/JIT; baut auf der Admission auf.
11. **7** – Boot-Audit.
11a. **16** – Dauerbetrieb (Nutzerentscheidung 26.10.2026, vor 6).
12. **6** – Threshold.
13. **8** – Verifikation.

Parallel dazu und unabhängig vom Code: Freigabe C-01 beantragen.

---

## 6. Grenzen dieses Dokuments

- Die Entscheidungsvorbereitung beruht auf statischer Code-Lesung und den Registern zu den genannten Commits. Es wurden keine Tests ausgeführt und keine Messungen gemacht.
- Die CI-Ergebnisse auf GitHub konnten nicht selbst geprüft werden.
- Zeilenangaben aus Vorlagen sind nicht übernommen worden. Vor jeder Umsetzung muss gegen den aktuellen HEAD neu gepinnt werden.

---

## 7. Nachträge vom 25.10.2026

### N-1 — Scope von G-1
**ENTSCHIEDEN (Nutzer, 2026-10-25).** G-1 gilt ausschließlich für die 15 Entscheidungen DEC-01…15. Die übrige Roadmap bleibt verbindlich, ihre Mess- und Phasengates gelten weiter (MT6–MT10 erst nach C0-MO und gemessenem Engpass). Bereits im Phasenplan-Commit `76d515b4` umgesetzt.

### N-2 — Widersprüche beim Non-Landing-Beweis (Nr. 3 präzisiert)
**ENTSCHIEDEN (Nutzer, 2026-10-25).**

1. **Jede B-Quelle hat genau eine Klasse:**
   - **Verwendbar:** Alle vier B-Kriterien (finalisierte Höhe, History-Untergrenze, Signatur-Status mit History, `getTransaction(finalized)=null`) sind erfüllt.
   - **Unbrauchbar:** Unerreichbar, HTTP 429, geprunt (`minimumLedgerSlot` > State-Slot), finalisierte Höhe noch nicht über `lastValidBlockHeight` oder sonst fehlerhaft. Keine Bestätigung.
   - **Positive Sichtung:** Die Quelle meldet eine gebundene Signatur finalisiert vorhanden.
2. **Jede positive Sichtung von B oder C ist Veto gegen Non-Landing.** Danach Landungsprüfung: finalisierte Transaktion samt vollständigen Metadaten und Signaturzugehörigkeit zum finalisierten Block/Slot, soweit erreichbar von beiden Quellen. Bei **bestätigter Landung** reguläre FE-04-Reconciliation (TX-Ergebnis, Token-/Lamport-Deltas, Gebühren, ggf. Exit); **Kapital ausschließlich über qualifiziertes Journal-Settlement freigeben**. Bei unbestätigter, widersprüchlicher oder unvollständiger Landung: `UNKNOWN`, Holds bleiben, typisierter Widerspruchs-Record, Alarm und Retry.
3. **Non-Landing-Freigabe nur kumulativ:** Keine positive Sichtung; C lückenlos und negativ; zwei verwendbare unabhängige B-Negativquellen oder, bei weniger als zwei verwendbaren B-Quellen, vollständiger qualifizierter C-Alleinbeweis.
4. **Sonst:** Fehlender Block, Call-Fehler, unvollständiges C → `UNKNOWN`, Holds bleiben.
5. **Keine manuelle Auflösung** und kein eigener GUI-Schalter.
6. **Wallets: je Versuch genau eine der fünf Ergebnisklassen** `B+C bestätigt`, `C allein qualifiziert`, `Veto – Landung bestätigt`, `Veto – Widerspruch offen`, `UNKNOWN – unvollständig`; zusätzlich unbrauchbare Quellen je Grund, Hold-Dauer, Calls und Bytes. Die alten Kennzahlen `B allein entschieden`/`C nötig` sind ersetzt.
7. **Abnahme (Loopback, separate Negativ-/Positivtests):**

| Eingabe | Erwartetes Ergebnis |
|---|---|
| Zwei B-Quellen negativ, C negativ | `B+C bestätigt` |
| Eine B-Quelle geprunt, 429 oder unerreichbar, C vollständig negativ | `C allein qualifiziert` |
| Beide B-Quellen unbrauchbar, C vollständig negativ | `C allein qualifiziert` |
| C unvollständig | `UNKNOWN – unvollständig` |
| Eine B-Quelle meldet eine Signatur, C negativ, Landung nicht bestätigt | Keine Freigabe, `Veto – Widerspruch offen` |
| C findet Signatur und Landung bestätigt | FE-04-Reconciliation; Kapital erst nach qualifiziertem Journal-Settlement frei |
| Landung bestätigt, aber Deltas unvollständig | UNKNOWN, Hold bleibt |
| Wiederholtes Lesen ohne neue Evidence | unverändert, kein Doppel-Settlement |

### N-3 — Quellberichte (Nr. 14 präzisiert)
**ENTSCHIEDEN (Nutzer, 2026-10-25).**

- `bug_v2.md`, `fehlstellen_v1.md` und `bugs_b.md` bleiben **unverändert am bisherigen Pfad**.
- Erst **ab dem erfolgreichen Konsolidierungs-Commit** sind sie eingefrorene Prüfberichte: keine Befunde, keine neuen Statusnachträge, keine Statusautorität.
- Ab diesem Commit wird der Status **ausschließlich** in `BUGS.md` gepflegt; sein Kopf benennt die eingefrorenen Quellen.
- Ein Bericht darf erst nach `fertig/`, wenn **alle** seine IDs in `BUGS.md` `ERLEDIGT` oder `WIDERLEGT` sind. Kein pauschaler Umzug und keine Löschung von Ursprungstext oder Evidence.

### N-4 / Z-1 — CI-Pflichtjobs für `OFFLINE_VERIFIED`
**ENTSCHIEDEN (Nutzer, 2026-10-25).**

1. Pflicht sind `project0-gate` laut CI und **alle betroffenen gemeinsamen Protokoll-/Codegen-/Golden-, Gateway-, Reporter- und Frontend-/Playwright-Jobs** gemäß festem Pfad→Job-Mapping der aktuellen Workflow-YAML. Bei unbekanntem geänderten Pfad gelten **alle** gemeinsamen Jobs als Pflicht.
2. Ein roter Pflichtjob blockiert `OFFLINE_VERIFIED`.
3. **Einzige Ausnahme, nur für gemeinsame Jobs** bei gleichzeitigem Nachweis: (a) identischer Test/identische Fehlerursache bereits auf dem Basis-Commit rot; (b) Tests **und** Ursache sind in `bsc/**` und das Arbeitspaket ändert keine `bsc/**`-Datei; (c) alle übrigen Tests des Jobs, vor allem Project_0-relevante, grün; (d) eigenständiger `BUGS.md`-Eintrag mit Test-IDs, Baseline-Lauf, Ursache.
4. Nie erlaubt: BSC-Code aus Project_0-Session ändern, Tests überspringen/deaktivieren, rote CI durch Wiederholung als grün deklarieren. Flaky Tests sind eigenständig zu erfassen.
5. Mit bewilligter Ausnahme: `OFFLINE_VERIFIED (CI-Ausnahme <ID>)`; gemeinsame CI weiterhin als **rot** ausweisen. Für `project0-gate` keine Ausnahme.

**Feste Pfad→Job-Tabelle (Stand der Workflow-YAML vom 25.10.2026 einschließlich N-7; bei Änderung neu ableiten; identisch mit `PHASENPLAN.md` Z-1/N-4):**

| Geänderter Pfad | Pflichtjobs aus den aktuell vorhandenen GitHub-Workflows |
|---|---|
| `project_0/**` außer `project_0/docs/**` (N-7) | `Project 0 status contract`: fünf `project0-gate`-Matrizen, `native`, `contract-and-ui` |
| `frontend/**` | `Project 0 status contract`: fünf `project0-gate`-Matrizen, `native`, `contract-and-ui` |
| `protocol/**`, `gateway/**` | `Project 0 status contract`: fünf `project0-gate`-Matrizen, `native`, `contract-and-ui`; zusätzlich `gateway`-Workflow: `rust`, `protocol` |
| `.github/workflows/project0-status.yml` | alle Jobs des Workflows `Project 0 status contract` |
| `.github/workflows/gateway.yml` | beide Jobs des Workflows `gateway` |
| `project_0/docs/**` allein | keine automatische CI-Auslösung durch diese beiden Workflows (N-7-Ausnahme); Dokumentabgleich gesondert nachweisen, keine erfundene CI-Freigabe |

### N-5 — G-1: Feste Reiter-/Command-Zuordnung
**ENTSCHIEDEN (Nutzer, 2026-10-25).**

Die bisherige Ermessensformel `soweit technisch für die jeweilige Funktion erforderlich` ist ungültig. Für jede Entscheidung mit Laufzeitwirkung ist ein `.p0j`-Record Pflicht. Die folgende Zuordnung ist verbindlich:

| DEC | Status bis zum bestehenden Reiter | Command-Rückweg |
|---|---|---|
| 01 | Liquidator: `BLOCKHASH_STALE`-Verlustgrund, Wiederanläufe | nein |
| 02 | Liquidator: Simulation wiederverwendet / neu / abgelehnt | nein |
| 03 | Wallets: fünf N-2-Ergebnisklassen, Bindungsdauer, Calls/Bytes | nein |
| 04 | Wallets und Liquidator: Modus, A/B/Ist und Abweichungen | ja: Modus mit Nachweis-Gate |
| 05 | Effektive Werte mit Herkunft in den zuständigen Reitern | nur ausdrücklich editierbare Felder (DEC-06, DEC-07) |
| 06 | Liquidator: Threshold-/Fallback-/Latenzstatus gemäß DEC-06 | ja: Modus-Hot-Change |
| 07 | Capture: TTL, Grenzen, Herkunft und Phase | ja: TTL zwischen Läufen; getrennte Netzwerk-/Startfreigabe |
| 08 | Capture und Analytics: INDEX/CROSSCHECK/STRICT und Jobs | ja: START/STOP/CANCEL/STATUS |
| 09 | keine Anzeige (Build-/Inventartest-Entscheidung) | nein |
| 10 | Liquidator/Wallets: Approval-Status ohne Secrets | nein |
| 11 | Liquidator: Capability `requested/effective/block_reason` | ja: Capability-Modus |
| 12 | keine neue Anzeige; typisierte Gründe in vorhandenen Projektionen | nein |
| 13 | Liquidator/Wallets: Admission- und LIVE-Status | ja: LIVE-Anforderung über Controller, `effective=BLOCKED` bis Freigabe |
| 14, 15 | Prozessentscheidungen ohne neue GUI-Funktion | nein |

**Kein neues Bedienelement** bei `nein`. Ein solcher Schalter bräuchte eine neue Nutzerentscheidung. Für jede Entscheidung **mit Laufzeitwirkung** ist der typisierte persistente `.p0j`-Nachweis Pflicht; für reine Build- oder Prozessentscheidungen werden keine künstlichen Laufzeitrecords erzeugt.

**Statusautorität bleibt `BUGS.md`; Plan- und Gate-Autorität bleibt `PHASENPLAN.md`.** Die N-2…N-5-Entscheidungen erlauben keine Mainnet-Reads, Paid-Provider, LIVE, Signing oder Geldtransfers.

### N-6 — CI-Pflichtjob auf exakt geprüftem SHA
**ENTSCHIEDEN (Nutzer, 25.10.2026).**

- Jeder Pflichtjob muss auf dem **exakt geprüften Commit-SHA** erfolgreich abgeschlossen sein.
- Keine Übernahme älterer grüner Ergebnisse.
- `NOT_RUN`, `SKIPPED` und `FAILED` zählen nicht als bestanden.
- Startet ein erforderlicher Workflow nicht automatisch: `workflow_dispatch` auf dem aktuellen `main`; der tatsächlich gestartete SHA muss dem geprüften SHA entsprechen.
- Vor der Integration betroffener Codepakete läuft das vollständige lokale `project_0/scripts/gate.sh`.
- Bei Protobuf-Änderungen zusätzlich `protocol/scripts/check.sh --check` gegen die korrekte Vergleichsbasis.
- N-4 einschließlich der BSC-Ausnahme bleibt unverändert.

### N-7 — Project_0-CI-Auslöser
**ENTSCHIEDEN (Nutzer, 25.10.2026).**

- Änderungen unter `project_0/**` lösen die Project_0-CI aus.
- Ausnahme: Pushes, die ausschließlich `project_0/docs/**` ändern.
- Die bestehenden Auslöser für `frontend/**`, `gateway/**`, `protocol/**` und die Workflow-Dateien bleiben erhalten.
- Die Pfad→Job-Tabelle in N-4 ist entsprechend aktualisiert.
