# Project_0 — BUGS: verbindliches Fehler-, Härtungs- und Abnahmeregister

**Stand:** 2026-10-18 · **Erstaufnahme / geprüfte Codebasis:** Project_0 aus `e42162e4d45d14c833bee2c1d24a89e5ea9cfee1`; Repository-`main` bei der letzten Code-Lesung `7156d45e68478cd4ee0a04f86d1417c9aff961d7`. Weitere unabhängige Commits betreffen Gateway/Reporter/Market-Taps. **Live-Tests wurden für diese Erstaufnahme nicht ausgeführt.** Die historischen Review-Aussagen und eigene GitHub-Code-Lesung werden ausdrücklich getrennt.

**Verbindliche Dokumente:** [PHASENPLAN.md](PHASENPLAN.md) v4.5.2 (alle fachlichen und Safety-Gates), [STATUS.md](STATUS.md) (Phasenstatus), [MEASUREMENT_LEDGER.md](MEASUREMENT_LEDGER.md) (tatsächliche Beweise), [C0E1_4H_READINESS.md](qualification/C0E1_4H_READINESS.md) (4h-Readiness) und [Frontend BOOT-Vertrag](../../frontend/docs/STARTUP_BACKTEST_OPTIMIZATION.md). Dieses Register ist die **kanonische Arbeits- und Erledigungsliste für die hier dokumentierten Bugs**, ändert aber **keine** Phasen-, FREE_ONLY-, JFI-, PERSIST-, Provider-, LIVE- oder Freigabevorgabe.

> **Freigaben unverändert:** LIVE und werttragende Submission bleiben gesperrt. Keine Mainnet-Reads, kurzen/mehrstündigen Captures, Providerkäufe oder Echtgeldtransaktionen allein aufgrund dieses Registers. Der Nutzer genehmigt Netzwerk-/Laufzeit-/Geldaktionen jeweils gesondert. Codekorrekturen und Offline-Tests sind keine solchen Freigaben.

## 0. So wird dieses Dokument gepflegt

### 0.1 Checklistenvertrag

- `- [ ]` = offen; hierzu gehört auch `CODE_ONLY`, `LOCAL_FAIL`, `BLOCKED` und `NOT_RUN`, solange die definierte Abnahme fehlt.
- `- [x]` = **lokal technisch abgeschlossen**: Fix im zugeordneten Commit nachvollziehbar, Regression im selben Produktionspfad vorhanden, relevante Offline-Tests mit Exit 0 im Ledger dokumentiert oder im Erst-Review ausdrücklich als bestanden belegt. Ein Haken ist **keine Mainnet-, Profit- oder P8-LIVE-Abnahme**.
- Bei erledigten Einträgen bleibt der Wortlaut mit `~~durchgestrichener Problembeschreibung~~` erhalten; Ursache, Fix und Nachweis dürfen nicht verschwinden. Historische Ledger-/Evidence-Dateien werden nicht umgeschrieben oder gelöscht.
- Bei unbestätigten Hypothesen `REVIEW_REQUIRED` vermerken und einen entscheidbaren Reproduktionstest verlangen; **kein spekulativer Fix**. `WAIVED`/`EXTERNAL_BLOCKED` sind keine Haken.
- Für jede Änderung mindestens: **Ticket-ID, Priorität, Phase/Gate, exakte Dateien/Funktion, Reproduktion/Negativtest, Fix, lokale Tests, Commit-SHA, Evidence, Restgefahr**. In `STATUS.md`/`MEASUREMENT_LEDGER.md` mit derselben ID referenzieren.
- Wird ein Fix später regressiv, Haken wieder entfernen, `REGRESSED` plus neue SHA/Testausgabe eintragen.
- Keine Erledigung aufgrund von Diff, Formatierung, `cargo check` allein oder mündlicher Agentenmeldung. Lokale Test-Evidence ist von produktiver Laufzeit-Evidence strikt getrennt.

### 0.2 Git-, Owner- und Ausführungsvertrag

1. Einzig `main`; **niemals Branch, PR, Force-Push, Reset/Clean oder `git add -A`**. Nur beauftragte `project_0/**`-Dateien und nur eigene Änderungen stagen. `bsc/**`, `gateway/**`, `frontend/**` und fremde Reporter-/Agentenänderungen nicht mitcommitten.
2. Vor jedem Ticket aktuelle `origin/main`, Arbeitsverzeichnis, Dirty-Dateien und parallele Agenten berücksichtigen; kein Überschreiben konkurrierender Arbeit. Commit nur nach geprüfter Diff-/Pathspec-Isolation, `git diff --cached --check`, korrektem HEAD und bestandenem Tickettest.
3. Kleine nachvollziehbare Commits `fix(project0): BUG-<ID> <text>` direkt auf `main`. Bei Konflikt STOP und Befund notieren, kein automatischer Gewalt-Merge.
4. Keine neuen Architekturpfade und keine zweiten Finance-/State-/Journal-Engines. `PERSIST-01–05`: ein kanonischer binärer `.p0j`-Vertrag; `JFI-01–05`: intern nur native Typen und Binärdaten. Externes Solana-JSON-RPC/WSS und expliziter Jupiter-Wire-Adapter sind die eng begrenzten Ausnahmen. Historische gepinnte JSON-/JSONL-Evidence bleibt lesbar, nicht neu beschrieben.
5. Sicherheit vor Durchsatz: `UNKNOWN`, unvollständige Replay-/Coverage-Beweise, Lücken, ungeklärte Kosten, Program-Drift oder nicht durable Reservation blockieren den zugehörigen finanziellen Schritt. Bestehende `ObserveOnly`-/`P0_LIVE_ENABLED=false`-/`P0_SUBMISSION_ENABLED=false`-Sperren dürfen nicht gelockert werden.
6. Pro Ticket zuerst gezielte Red-/Green-Tests, dann passende Feature-/Integrationstests. Größere Workspace-Matrix **einmal pro sinnvollem Gate-Abschluss**: `cargo fmt --check`; Clippy inklusive passender Features/Targets mit `-D warnings`; Rust-Tests für `crate_health` und `crate_health,txv1` bzw. tatsächlich unterstützte Feature-Matrix; Release-Build; Python-/Verifier-Parität und relevante Replay-/Fehlerfalltests. `--all-features` ergänzend ausführen, falls im Workspace unterstützt. Nichts als PASS ausgeben, was nicht lief.
7. Evidence nach `project_0/docs/evidence/BUGS_YYYYMMDD/`; Text/Markdown und, falls technisch verlangt, neue native `.p0j`. Kommandos, Exit-Codes, Tested-SHA, Ergebnisse, fehlende Tests und Unterschiede `LOCAL_PASS / BLOCKED / NOT_RUN` festhalten. Keine Secrets/URLs/API-Keys/Wallet-Pfade in Logs.
8. Nach jedem erledigten Ticket **hier die Checkbox, SHA und Evidenz aktualisieren**, danach die bestehenden Statusdokumente. Zurückgestellte Punkte bleiben sichtbar.

### 0.3 Abschlussschranken

| Gate | Definition; die Kriterien sind kumulativ |
|---|---|
| **G-A Codehärtung** | Relevante H-/M-/N-/L-/B-Crash-, Secret-, Deadlock-, Journal-, BOOT- und JFI-Befunde lokal geschlossen; Negativ-/Regressionstests bestanden; keine ungeklärte kritische Warnung. |
| **G-B Read-only Netzwerk-Audit** | G-A für den Netzwerk-/Capture-Pfad; BOOT-01–12 korrekt implementiert, aktuell angeforderte Berechtigungen und Provider-Evidence separat erteilt; echte Auditdaten erst danach. Ein Offline-PASS ist kein Network-READY. |
| **G-C bis 600 s Kalibrierung** | G-B mit realer Quellenliveness, Writer-/RAM-/Disk-/Provider-Budgets; gesonderte Nutzerfreigabe; Capture/Drain/Seal/Verify ohne falschen PASS. |
| **G-D 4h-Readiness** | G-C bestanden; reale Bytes/s, p95/p99-Tails, I/O- und Quota-Reserven, Recovery/Gap/Strict-Modus budgetiert; `READY_FOR_AUTHORIZED_4H` ist noch **kein** Start. |
| **G-E FE-01–05 Pilot-Readiness** | Aktuelle gepinnte Program/Bank/Oracle-State-Parität; wirtschaftlich ausführbarer Kandidat, Funding, Rent, konservativer Exit, vollständige Simulation, Targeted Shadow und FE-04/P8-Durability/UNKNOWN-Replay. Vollständiger C0-E1-Master-PASS ist **nicht allein** Voraussetzung für einen unabhängig vollständig nachgewiesenen engen Pilot (§0.6). |
| **G-F FE-06/07** | Zusätzlich individueller signierter One-Shot mit expliziter Benutzerfreigabe, Single-Flight, persistenten Holds/Spend-Limits und automatischem HALT. Realized Net PnL erst nach finalisierten Chain-Deltas, Gebühren, Exit und kumuliertem Gesamtledger; kein Shadow-Profit als LIVE-Profit. |

## 1. Gesamtübersicht und stabile Ticket-IDs

**Status-Snapshot bei Anlage** (keine Behauptung neuer ausgeführter Tests):

| Block | Technisch lokal geschlossen | Offen / separat nachzuweisen |
|---|---|---|
| `H` hoch aus Review 1 | H1, H2, H3 als Fix + zielgerichtete Tests im Commit berichtet | Gesamtregression / reale Last separat |
| `M` mittel aus Review 1 | M4 und M7 im Commit behoben und reviewt | M1, M2, M3; M5/M6 **nicht als Befund vorgelegt**, deshalb keine erfundene Ticketdefinition |
| `N` neue Astra-Patch-Befunde | — | N1–N8; bei N2 tatsächlicher Clippy-FAIL erst ausführen |
| `L` kleinere Befunde Review 1 | — | L1, L2, L4; L3 wurde nicht als Befund geliefert |
| `B` finanzielle Pipeline-/State-Prüfung | — | B1–B9, mit **B8 als umstrittener Fixvorschlag**; zusätzliche konkrete kleine Befunde B10–B18 |
| `C` Recovery/Owner/Settlement Review 3 | — | C1–C12 offen, C13 weiterer Reviewumfang; C1/C7 vor 4h, C3/C4/C5 vor P8. |
| `D` Market-Evidence/Funding/Sweep Review 4 | — | D1–D13 offen; D1 vor 4h, D3 vor 4h-Stresstest, D4/D5/D8/D9 Capture-Härtung; D2 Effizienz, D10=C6, D11=B11. |
| `X` verbindliche Quervertragslücken | — | X1 BOOT-12, X2 JFI-BOOT, X3 BOOT-01/02; verwandt mit N1–N8, keine doppelte Implementierung |
| Weitere Reviewmodule | — | `winner_tracker.rs`, `chain.rs`, `econ*.rs`, `measurement.rs`, `report.rs`, `index.rs`, `integration.rs`, `projection.rs`, `census.rs`, `planner.rs` und Python-Skripte noch nicht vollständig begutachtet; `sweep.rs`, `state_owner/ingress.rs`, `funding.rs`, `protocol.rs`, `market/{capture,sink,core}.rs` im vierten Durchgang gelesen (C-/D-Tickets bleiben offen) |

Die in diesem Register geschlossenen H- und M-Punkte beruhen auf Fables Review der eingecheckten Fixes und zielgerichteten Testangaben; **eine neue vollständige Workspace-Ausführung durch den Register-Autor wurde nicht vorgenommen**.

## 2. H — hohe Ausgangsbefunde: lokal behoben, Regression sichtbar behalten

### H1 — Torn Capture Tail / Segment-Recovery
- [x] ~~Abgeschnittene letzte Capture-Frames machten ein gesamtes Segment unlesbar.~~
- **Pfad:** `runtime/src/analytics/journal.rs`: `Reader::open_capture`, `next_frame`; `runtime/src/storage/segments.rs`: `ChainReader`, `check_no_orphans`.
- **Fix-Stand:** Commit `e42162e4`; nur nicht versiegelte Capture-Tails mit physischem `UnexpectedEof` werden `tail_truncated / complete=false`. Fehlender letzter Nachfolger eines Rotations-Crashes bleibt INCOMPLETE, Kettenbruch/orphan ist kein PASS. `Reader::open` für Wallet sowie `Writer::resume` bleiben strikt.
- **Beleg:** Fable bestätigte vorhandene Tests zu jeder Abschneideposition; zielgerichteter Patch-Review PASS. **Offener Anschluss:** N3 (Null-Fill-Tail), reale Crash-/Last-Evidence unter PERSIST/SK-07.
- **Regression:** jede Frame-Abschneideposition, Manifest, Rotation, orphan, CRC/Hash, strikter Walletreader. **Gate:** G-A lokal; G-D braucht reale Evidence.
- **Ticketabschluss:** `LOCAL_REVIEW_PASS`; Code-SHA `e42162e4`. Cross-Ref: N3.

### H2 — Secrets in RPC-/Jito-Fehlern
- [x] ~~Provider-Fehlermeldungen konnten URL/API-Key/Response-Payload in Logs und Fehlertexte durchreichen.~~
- **Pfad:** `solana/src/submit/rpc.rs`, `solana/src/submit/jito.rs`, `runtime/src/main.rs` / `orchestration/startup.rs`.
- **Fix-Stand:** `safe_rpc_error`/`safe_http_error`/`safe_remote_error`; konservative `Rejected`-Klasse, unsichere Annahme `Transport/UNKNOWN`; Main-Ausgabe kategorisiert. Fable meldete Secret-/Host-Negativtests PASS.
- **Nachlauf:** N5/B13/B10, Jito-/Simulationsdiagnosen dürfen diese Grenze nicht unterlaufen; redigierter Fehlertext ist nicht automatisch Beweis für sicheren Roh-Response.
- **Ticketabschluss:** `LOCAL_REVIEW_PASS`; Code-SHA `e42162e4`.

### H3 — vergifteter Provider-Mutex / Drop-Panic
- [x] ~~Poisoned Mutex und Permit-Drops konnten erneut panicken oder Budgetzustände inkonsistent lassen.~~
- **Pfad:** `solana/src/provider.rs`.
- **Fix-Stand:** `lock_state` übernimmt PoisonError-Inneres und setzt `failed`; `checked_sub`-Drops; Condvar-Poison und Unwind mit gehaltenen Permits getestet. Fail-closed bleibt.
- **Nachlauf:** N4 korrekter Drift-Code, Gesamtregression.
- **Ticketabschluss:** `LOCAL_REVIEW_PASS`; Code-SHA `e42162e4`.

## 3. M — mittlere Ausgangsbefunde

### M1 — Betriebssystem-Sampling unter RAM-Budget-Mutex
- [ ] **OPEN · G-A/G-D · PERSIST-02/SK-03.**
- **Pfad:** `runtime/src/storage/budget.rs`: `MemoryBudget::acquire` → `headroom_allows` → `MemorySample::now`.
- **Ist:** Während `state.lock()` gehalten wird, können `sysctl`/`task_info`/`getrusage`/`/proc`-Abfragen erfolgen; Condvar-Wakeups können das wiederholen.
- **Soll-Fix:** Entscheidungswerte ohne Mutex sampeln; Guard abgeben, Sample holen, Guard erneut nehmen, aktuelle `used/limit/max`-Invarianten erneut prüfen; monotones, begrenztes Samplingintervall; `UNKNOWN`-Headroom erlaubt kein Wachstum. Keine race-bedingte Überreservierung.
- **Tests:** unabhängige Headroom-Tabelle (Memory-Pressure, unbekannte Werte, Grenzwerte), künstliche Concurrent-Acquires mit getriggertem Limitwechsel, Lock-/Deadlock-Probe, Bounded-Wait und Byte-Grenze. Messung: Sampling-/Lockdauer vor/nach.
- **Abnahme:** keine langsamen Syscalls unter `state`-Lock; deterministische Limits unverändert. **Commit/Evidence:** —.

### M2 — Owner-Evidence-Durability und blockierendes stderr
- [ ] **OPEN · G-A/G-D · PERSIST-02/05/SK-04.**
- **Pfad:** `runtime/src/analytics/evidence.rs`: `Sink::store`, `Sink::write`, `run_writer`, `OwnerEvidenceWriter::finish`.
- **Ist:** Owner-Evidence wird regulär beim `seal` synchronisiert, nicht nach belegtem periodischem Intervall; synchrones `eprintln!` auf dem Writer kann bei stderr-Backpressure den Writer verzögern. **Achtung:** separater MarketJournalWorker in `storage/writer.rs` hat bereits einen Sync-Intervallvertrag; hier nicht doppelt implementieren.
- **Soll-Fix:** bounded, fehlersicherer periodischer Sync für Owner-Evidence mit festgelegter und dokumentierter Verlustobergrenze. Diagnostikausgabe entkoppeln, aber niemals durch `join` eines blockierten Konsolenthreads einen Shutdown-Deadlock erzeugen. Journal-Evidence darf nicht still verlorengehen; eine volle Journal-Queue bleibt korrekt fail-closed. Diagnostikverluste gesondert zählen.
- **Tests:** Sync-Deadline auch ohne neue Events, injizierter Sync-Fehler führt sticky HALT, blockiertes Terminal verursacht weder Writer-/Shutdown-Deadlock noch unbegrenzte Queue, EOF/Shutdown/Seal/Evidence-Recovery. Nicht behaupten, ein hart blockierter OS-stderr könne immer sofort beendet werden, ohne die tatsächliche Architektur zu beweisen.
- **Abnahme:** beweisbare Durability-Grenze, keine stille Evidence-Drops, terminierbarer Abschluss. **Commit/Evidence:** —.

### M3 — Blockhash-Refresh-Thread ignoriert allgemeines Shutdown
- [ ] **OPEN · G-A/G-C · S0-Lifecycle/P6.**
- **Pfad:** `runtime/src/orchestration/state_owner/setup.rs`: `thread::Builder::new().name("blockhash")`; Owner-/Shutdown-Struktur `state_owner.rs` und `state_owner/shutdown.rs`.
- **Ist:** Schleife prüft nur `bh_econ.admission_closed()`, schläft 10 s und verwirft den JoinHandle. Ohne Econ-Handle läuft sie bis Prozessende weiter und kann weitere RPC-Reads anstoßen.
- **Soll-Fix:** Shutdown + AdmissionClosed vor jedem Refresh und während der Sleep-Phase prüfen, Handle eindeutig im Owner halten und am geordneten Shutdown joinen. Netzwerk-RPC darf einen Shutdown nicht unbegrenzt überziehen; per-call-Timeout und Joindeadline als reale Grenzen dokumentieren.
- **Tests:** Shutdown ohne Econ-Handle, AdmissionClosed, Request-in-flight + Timeout, wiederholtes Start/Stop ohne verwaiste Threads/RPC nach Stop; negative Cancellation.
- **Abnahme:** keine Refresh-Reads nach Shutdown, kein detached Owner-Thread. **Commit/Evidence:** —.

### M4 — WebSocket ACK und Slot-Progress getrennt
- [x] ~~Ein einziger Timer vermischte ACK und fehlenden Slotfortschritt.~~
- **Pfad:** `solana/src/ingress/ws_rpc.rs`.
- **Fix:** `WSS_ACK_TIMEOUT` eigener Zustand; Slotfortschritt beginnt erst bei `StreamReady`. Fable bestätigte eingecheckten Patch `e42162e4`.
- **Offener Anschluss:** N8 tatsächliches ACK-Budget/Subscription-Mengen; reale Liveness. **Status:** `LOCAL_REVIEW_PASS`.

### M7 — Provider-Budget-Bindung an aktive Konfiguration
- [x] ~~Früh angelegtes Shared-Budget konnte später abweichende aktive Limits unbemerkt nutzen.~~
- **Pfad:** `solana/src/provider.rs`.
- **Fix:** `shared()`/Budget-Konfigurationskontrolle bindet gegen `active()`, Drift führt fail-closed. Fable bestätigte Patch `e42162e4`.
- **Offener Anschluss:** N4 eindeutige Fehlerursache; keine automatische Reinitialisierung. **Status:** `LOCAL_REVIEW_PASS`.

## 4. N — neue Befunde aus der Astra-Patchprüfung

### N1 — BOOT-RPC-Budget ist mit Standardprofil unmöglich
- [ ] **CRITICAL · OPEN · G-A/G-B · BOOT-03/04.**
- **Pfad:** `runtime/src/orchestration/boot_network.rs`: `NetworkReads::new/rpc/cluster`; `solana/src/runtime_config.rs`.
- **Reproduktion:** `fe_mode=smoke` setzt `fe_calls=3`, `fe_seconds=30`. `NetworkReads` verwendet `limit=fe_calls.min(max_requests)`; `cluster` braucht bereits fünf RPC-Reads. Vierter Call ergibt `BOOT_RPC_BUDGET`. Existing Fixtures umgehen das echte Budgetobjekt.
- **Soll-Fix:** Eigenes *explizites* BOOT-Call- und Zeitbudget, getrennt von FE-01-Limits und gebunden an globales Providerbudget und FREE_ONLY. Verbindliches Minimum aus tatsächlichen Cluster-, Programm-/ProgramData-, Scope-Batch- und Oracle-Aufrufen **berechnen, nicht bloß `5+fe_snapshot_rounds` behaupten**. Bei unzureichendem Budget vor Netzwerkzugriff `BOOT_BUDGET_TOO_SMALL`; keine heimliche Quoten-/RPS-Erhöhung.
- **Tests:** Default-Profil plus berechnete Pflicht-Reads, `NetworkReads` mit lokalem HTTP-Stub (nicht nur `Fixture`), Boundary `minimum-1/minimum`, zusätzliche dynamische Scope-Runde, MaxRequests und Deadline, fe_calls-Unabhängigkeit. **Keine echten RPCs.**
- **Abnahme:** Defaults scheitern nicht zwangsläufig bereits beim vierten Cluster-Read; real nicht hinreichendes Budget blockiert **vor** Start. **Commit/Evidence:** —.

### N2 — Formatter/Clippy/Build der Astra-Änderungen nicht nachgewiesen
- [ ] **HIGH · OPEN (konkreter Clippy-Fehler bis Ausführung nicht bewiesen) · G-A.**
- **Pfad:** `runtime/src/orchestration/fe_qualification.rs::collect` (finales `return Ok(...)`), `boot_network.rs::run` (Format), gesamter geänderter Workspace.
- **Soll-Fix:** `cargo fmt` auf tatsächlicher Workspace-Wurzel, finales unnötiges `return` ggf. entfernen; anschließend Clippy mit aktivem Feature-Scope und `-D warnings`, Testmatrix/Release-Build. Keine globalen `#[allow]` zur Kosmetik und keine fremden Module massenhaft formatieren.
- **Tests:** tatsächliche Tool-Ausgaben und Exit-Codes, beide produktiven Featureprofile; Fehler separat mit Dateien/SHA dokumentieren.
- **Abnahme:** formatiert, Clippy/test/release erfolgreich auf exakt getesteter SHA. **Commit/Evidence:** —.

### N3 — rein nullgefülltes Capture-Dateiende nicht als INCOMPLETE erkannt
- [ ] **HIGH · OPEN · G-A/G-D · PERSIST-05/SK-07.**
- **Pfad:** `runtime/src/analytics/journal.rs::read_frame`, `Reader::next_frame`, `storage/segments.rs`, FE-04-Finanzjournal.
- **Ist:** Vier Nullbytes als Frame-Länge führen zu `FRAME_LENGTH`, nicht zu toleriertem `UnexpectedEof`.
- **Soll-Fix:** Ausschließlich **vollständig nachweisbare Null-Suffix-Bytes an einer erlaubten letzten, unversiegelten Capture-Framegrenze** als Torn Tail klassifizieren, Status `INCOMPLETE`. Keine Nullstrecke innerhalb eines korrupten Frames, kein `DATA_AFTER_MANIFEST` oder fremdes/sealed Segment erlauben. Suffix-Prüfung bounded/streaming, ohne O(file_size)-RAM.
- **Tests:** 1/3/4/5/4096/70000 Nullbytes nach gültigen Frames, Fremdbyte am Ende, Nullen mitten im Frame, nach Seal, Segmentrotation, `Reader::open`/`Writer::resume`/`one_shot::Journal` weiterhin strikt; Bytes bleiben unverändert.
- **Abnahme:** nur echter zulässiger Capture-Tail `INCOMPLETE`; beschädigte finanzielle Evidence weiter HALT. **Commit/Evidence:** —.

### N4 — Drift fälschlich als Mutex-Poison gemeldet
- [ ] **MEDIUM · OPEN · G-A.**
- **Pfad:** `solana/src/provider.rs::check_configuration/acquire`.
- **Ist:** `failed=true` nach Config-Drift, Fehlercode `PROVIDER_BUDGET_POISONED` suggeriert Unwind.
- **Soll-Fix:** getrennte, sticky Diagnose `PROVIDER_BUDGET_CONFIG_DRIFT`; Endpunkte nur redigiert/als sichere Feldbezeichner, niemals vollständige URL; Poison-Fehler unverändert fail-closed.
- **Tests:** Drift vs Poison differenziert, einmalige protokollierte Feldklasse, keine Secrets, keine Reinitialisierung.
- **Abnahme:** eindeutiger Code ohne Abschwächung des Budget-HALTs. **Commit/Evidence:** —.

### N5 — lokale Runtime-Fehler durch globale Redaction unbrauchbar
- [ ] **MEDIUM · OPEN · G-A.**
- **Pfad:** `runtime/src/orchestration/startup.rs::shadow_failure_code`; `runtime/src/main.rs`; sichere Config-/Walletfehler.
- **Ist:** Unklassifizierte Fehler werden pauschal `SHADOW_RUNTIME_ERROR`, dadurch fehlen diagnostische Hinweise.
- **Soll-Fix:** ausschließlich explizit allowgelistete eigene, secret-freie Fehlercodes und begrenzte strukturierte Detailfelder. **Kein beliebiges `Display` von unbekannten Error-Typen**; keine Provider-URL, API-Keys, Keypair-Pfade, HTTP-Body oder rohe Logs.
- **Tests:** Missing Env/Mode identifizierbar; ClientError mit geheimem URL-Parameter redigiert; Wrapped/Custom-Fehler mit Secret zeigt nur Kategorie.
- **Abnahme:** nützliche, secret-freie Betriebsdiagnose. **Commit/Evidence:** —.

### N6 — neuer RPC-Client für jeden BOOT-Read
- [ ] **MEDIUM · OPEN · G-B.**
- **Pfad:** `runtime/src/orchestration/boot_network.rs::NetworkReads::rpc`.
- **Ist:** `provider::rpc_client(...)` pro Request; vermeidbare neue Clients/Transport-Runtimes. Die genaue SDK-Runtime-Zahl ist ohne Laufzeitmessung **nicht bewiesen**.
- **Soll-Fix:** Einmaliger Client im `NetworkReads`-Owner, gleiche Provider-Awareness; vor jedem Call absolute Gesamtdeadline prüfen. Kein Client-Caching über veränderte Config-/Clusteridentitäten.
- **Tests:** lokal instrumentierter Client-Factory-Zähler, Budget-/Timeout-Tests, keine erneute Konstruktion bei Wiederholung.
- **Abnahme:** ein Client pro Audit-Instanz, globale Zeitgrenze bleibt hart. **Commit/Evidence:** —.

### N7 — 60-Sekunden-Boot-Audit-TTL operativ knapp
- [ ] **MEDIUM · OPEN / DESIGN_DECISION · G-B.**
- **Pfad:** `runtime/src/orchestration/boot_audit.rs::validate_start`, `boot_network.rs::Spec`, `bin/p0ctl.rs`.
- **Ist:** Doctor und Start erfordern manuelles Tokenhandling in höchstens 60 s; Verspätung blockiert korrekt.
- **Soll-Fix:** sichere kombinierte `doctor + run start`-Bedienung erwägen, **ohne** TTL/Safety still auf 300 s zu erhöhen. Jede etwaige Änderung der TTL bedarf gesonderter Freigabe; BOOT-12 muss ohnehin aktuelle Identitäten prüfen.
- **Tests:** Token gültig/abgelaufen, Konfig-/Binär-/Programmdrift und doppelte Startversuche.
- **Abnahme:** bedienbarer Workflow **ohne** längeres unsicheres Vertrauen in einen alten Token. **Commit/Evidence:** —.

### N8 — WSS-ACK-Deadline ignoriert Subscription-Anzahl
- [ ] **MEDIUM · OPEN · G-C/G-D.**
- **Pfad:** `solana/src/ingress/ws_rpc.rs`: `ack_deadline = now + idle_timeout`.
- **Ist:** ACK-Timeout meldet seit M4 korrekt `WSS_ACK_TIMEOUT`, Budget bleibt unabhängig von tatsächlichen Send-/ACK-Zahlen.
- **Soll-Fix:** ACK-Dauer/-Zahl/-Lag instrumentieren; dynamische aber **harte, konfigurierte** obere Budgetgrenze nach tatsächlichem Subscriptionbedarf prüfen. Keine automatische unbeschränkte Verlängerung, keine Umgehung von Recoverybudget/WSS-Rate.
- **Tests:** kleine/große Subscriptionmenge, einzelne fehlende ACK, 429/Timeout, Shutdown während ACK; reale Profile erst nach Erlaubnis.
- **Abnahme:** falsche Reconnects reduziert, ausbleibende ACK führt weiterhin bounded HALT. **Commit/Evidence:** —.

## 5. L — kleine Restbefunde aus dem ersten Review

### L1 — Drop-Reihenfolge Finanzjournal und File-Lock
- [ ] **MEDIUM vor FE-04 · OPEN.**
- **Pfad:** `runtime/src/execution/one_shot.rs::Journal`.
- **Soll:** Ownership/Drop-Reihenfolge explizit so beweisen, dass Journal/Writer vollständig geschlossen und durable ist, bevor die zugehörige Lock-Freigabe andere Owner zulässt. Keine Lösung allein über Kommentar.
- **Tests:** konkurrierender Open während Drop/Finish, Crash-/Sync-Fehler, Lock bleibt bis korrekt abgeschlossen. **Commit/Evidence:** —.

### L2 — unsicherer `sigs[sigs.len()-2]`-Index im Jito-Bundle
- [ ] **HIGH vor FE-06 · OPEN.**
- **Pfad:** `solana/src/submit/jito.rs::submit_bundle`.
- **Soll:** Signature-/Bundle-Vollständigkeit und erwartete Reihenfolge **vor jedem Netzwerk-Handoff** prüfen, keine Annahme von mindestens zwei Signaturen; leere/unvollständige Bundles lokal ablehnen.
- **Tests:** 0/1/2/mehrere TX, fehlende Signature, nicht signierte TX; kein HTTP-Aufruf bei Ablehnung; UNKNOWN nach unklarer Annahme. **Commit/Evidence:** —.

### L4 — falsch konfigurierte Zero-Prefund-Flags fallen auf `false` zurück
- [ ] **MEDIUM · OPEN · FE-01/P5.**
- **Pfad:** `runtime/src/orchestration/state_owner/setup.rs`: `P0_ZERO_PREFUND_QUALIFIED`, `P0_ZERO_PREFUND_SHADOW_PROBE`.
- **Soll:** Parsefehler explizit als Config-Fehler blockieren, statt `unwrap_or(false)`; korrektes Defaultverhalten nur bei **fehlendem**, nicht malformed Wert.
- **Tests:** missing/true/false/ungültig, kein implizites Einschalten von LIVE. **Commit/Evidence:** —.

## 6. B — finanziell relevante zweite Reviewrunde

### B1 — LIVE-Pipeline umgeht FE-04-Durable-Admission
- [ ] **CRITICAL · FE-06/P8 BLOCKER · G-E/G-F.**
- **Pfad:** `runtime/src/execution/pipeline.rs::evaluate`, `execution/one_shot.rs`, `execution/pending.rs` und StateOwner.
- **Ist:** `Mode::Live` verwendet `safety.check` (RAM) und `submitter.submit/submit_bundle` direkt. `one_shot::Journal`, `Approval` und `SignedPayload` sind nicht an diesen Handoff gebunden. Derzeit über Runtime-/ObserveOnly-Sperren unerreichbar – das ist **keine** finanzielle FE-04-Qualifikation.
- **Soll-Fix:** Direktes finanzielles Senden typisiert unerreichbar machen, sofern keine gültige *persistierte und fsync-bestätigte* Single-Flight-Approval/Intent/Reservation/Signatur-/TX-Payload-/Budget-Bindung vorliegt. Ausschließlich ein bestehender FE-04-One-Shot-Owner führt den tatsächlichen Send aus. Bundles, Tips, Prerequisites, Cost-Floor, Blockhash-Lifetime und UNKNOWN nach Transportfehler vollständig einschließen. Kein zweiter Execution-Core.
- **Tests:** compile-/trait-boundary (kein Live-Submit ohne Approval), Abort vor durable Commit ⇒ 0 sends, Crash an jeder Commit-Grenze, doppelte Aufnahme ⇒ 0 Double-Send, unklare Annahme hält Holds, Finalized-Reconciliation und auto HALT, identischer signierter Payload. Fake Submitter mit gezähltem Call.
- **Abnahme:** ein aktiver Typ-/Laufzeitpfad, der FE-04 unübergehbar bindet; erst dann FE-06 separat prüfbar. **Kein LIVE in diesem Ticket.** **Commit/Evidence:** —.

### B2 — Prefix-Decode von MarginfiAccount, Group und FeeState
- [ ] **HIGH · P1A/P1B/FE-01 BLOCKER · G-E.**
- **Pfad:** `liquidation/src/health.rs::load/health`, `liquidation/src/sizing.rs::load`, `runtime/src/execution/builder.rs::should_init_liquidation_record/active_bank_keys`, `liquidation/src/protocol.rs`.
- **Ist:** Nur `decode_bank` prüft bereits exakten 0.1.11-/0.1.12-Längenvertrag. Allgemeine `load<T>`-Helfer schneiden `data[8..8+size]` ohne Discriminator/Owner/erlaubte Gesamtlänge. `health()` prüft beim Zielaccount keine Owner-/Discriminatorbindung. Builder prüft teilweise Owner/Discriminator, aber nur Mindestlänge.
- **Soll-Fix:** Zentralisierte pinned Versions-Decoder für MarginfiAccount/Group/FeeState mit erlaubten exakten Layoutlängen, Discriminator und vom Aufrufer geprüftem Program-Owner. Bei unbekanntem Upgrade/verlängertem Layout `UNKNOWN_LAYOUT`, **kein Prefix-Cast**; keine stillschweigende Annahme, dass sämtliche 0.1.12-Accounts dieselbe Größe wie 0.1.11 haben. Falls P1B-Längen noch nicht belegt: BLOCKED bis Source-/Fixture-Parität.
- **Tests:** gültige 0.1.11-Fixtures, zu kurze/verlängerte Payload, anderer Discriminator/Owner, absichtlich resized P1B-Fälle, unaligned Bytes; Health/Sizing/Builder müssen identisch fail-closed entscheiden.
- **Abnahme:** gemeinsame Provenienz/Version, keine unqualifizierten Prefix-Leser mehr. **Commit/Evidence:** —.

### B3 — flüchtiger Safety-Verlust-/HALT-/Kapitalzustand
- [ ] **CRITICAL vor P8-LIVE · G-E/G-F · FE-04/P8-RES.**
- **Pfad:** `liquidation/src/safety.rs`, `runtime/src/execution/one_shot.rs`.
- **Ist:** `day`, `capital_in_use`, Fehlversuche und `halted` sind RAM-Zustand. Neustart rekonstruiert sie nicht zuverlässig aus dem Finanzjournal.
- **Soll-Fix:** Einziger versionierter Finanzjournal-/Replay-Automat enthält idempotent Spend/Holds, Exposure, Failed Attempts, finalisierte PnL/Loss, UTC-Day-ID, Inventory und sticky HALT. Nicht einen zweiten Shadow-/Safety-Store bauen. `UNKNOWN` hält Kapital; nichts wird bei Restart automatisch freigegeben oder neu bewaffnet.
- **Tests:** Restart/Crash vor und nach Transition, Mitternacht/UTC-Tageswechsel, Replay-Dedupe, fehlendes Outcome, partielle Fee/Tip, unbekannte Exit-Kosten, Crash/Full-Disk; Portfolio- und Tagesbudget bleiben identisch.
- **Abnahme:** Safety-Status vor jedem werttragenden Handoff aus durable Ledger qualifiziert; kein Auto-Reset. **Commit/Evidence:** —.

### B4 — Rent für neue Liquidation-Records ist Unknown
- [ ] **HIGH · FE-02 BLOCKER für betroffene Kandidaten · G-E.**
- **Pfad:** `runtime/src/execution/builder.rs::plans/should_init_liquidation_record`, `liquidation/src/sizing.rs`, `execution/one_shot.rs`.
- **Ist:** `rent_and_token_usd_micro=None`, falls neuer Record nötig ist. Das hält die Economics bewusst UNKNOWN. Die behauptete „Mehrheit der Accounts“ ist **ohne Marktmessung nicht belegt**.
- **Soll-Fix:** Exakte geplante Accountgröße/Owner/Instruktionen aus gepinntem Marginfi-Source; tatsächliches erforderliches rent-exempt Minimum mit validierter Solana-Rent-Quelle bzw. qualifiziertem zustandstreuen Modell berechnen; daraus SOL-Reserve und USD-Micro mittels gültigem konservativem SOL-Preis. Zusätzlich Return-/Close-/Rent-Recovery nur dann gutgeschrieben, wenn ausführbar und nachgewiesen. Keine Null- oder Pseudo-Konstanten.
- **Tests:** Record existiert/nicht vorhanden, falscher PDA, Rent-Änderung, unbekannter SOL-Preis, verfügbare Fee-SOL, i128/u128/rounding, vollständige Economics inkl. aller Kosten.
- **Abnahme:** betroffene Kandidaten entweder konservativ bewertbar oder weiterhin `UNKNOWN`; keine falschen LIVE-Kandidaten. **Commit/Evidence:** —.

### B5 — Premium-BPS über 100 % kann Unterlauf/Panik auslösen
- [ ] **HIGH · G-A (auch offline untrusted Input) · P4A/T.**
- **Pfad:** `liquidation/src/economics.rs::premium_bps/time_for_premium/t_profitable/max_seizable_for_repay`.
- **Ist:** `base=max(fee_state_max_bps,500)`; `PREMIUM_MAX_BPS-base` kann unterlaufen; `time_for_premium` hat ein mögliches Division-durch-Null-/Underflow-Szenario. `t_profitable` hat zusätzlich zu prüfen, ob negative `min_profit`-Summen sicher in u128 überführt werden (eigener Negativtest; **noch keine bestätigte Production-Auswirkung**).
- **Soll-Fix:** Source-konforme Validierung `fee_state_max_bps <= 10_000` vor Fachberechnung; bei invalider FeeState **UNKNOWN/Unsupported und Fail-Closed**, nicht still „100 %“ als gültig normalisieren, sofern Source das nicht belegt. Sichere Grenzen/checked-Arithmetik für Division, Zeitaddition und Signed→Unsigned.
- **Tests:** 0/500/9999/10000/10001/u64::MAX; vor/nach Grace, growth_blocked, denominator=0, negative MinProfit, Debug/Release-Gleichheit, P1A-Fixture-Parität.
- **Abnahme:** keine Panik, kein Wrap, keine unverdiente positive Economics. **Commit/Evidence:** —.

### B6 — synchrones Fee-/Simulation-/Jupiter-I/O im Owner
- [ ] **HIGH RISK · G-C/G-D/P6-LT; PERFORMANCE + RELIABILITY.**
- **Pfad:** `runtime/src/execution/pipeline.rs::evaluate`, `solana/src/submit/rpc.rs`, `runtime/src/execution/route.rs`.
- **Ist:** `fee_for_message` und `simulate` rufen blockierende RPCs; Route-Build nutzt blockierenden HTTP-Client. Latenz kann den State-/Ingress-Owner aufhalten. Die behaupteten 30 s pro Call und 1,5 s pro Jupiter-Call sind Konfigurations-/Codewerte, keine gemessenen Maximal-Tails.
- **Soll-Fix:** zuerst bereits vorhandenes MT0/`detect_health_ns_*`/Fee-/Simulation-Timing und Queue-/Gap-Halt-Evidence prüfen. Danach Netzwerkjobs auf vorhandene bounded RemoteWorker/Ownership-Schicht auslagern oder nur bei begrenztem Gate einen kürzeren gesamten **Kandidaten**-Deadline verwenden. Einzelne Timeouts blind von 30 s auf 3 s zu setzen ist kein struktureller Fix; niemals RPC neu in den Trading-Hot-Path ziehen.
- **Tests:** künstlich blockierende RPC/Jupiter-Stubber, parallele Ingress-Frames, Queue-Grenzen, Cancellation, Timeout, unbekannte Fee = kein LIVE; deterministische Queue- und Shutdown-Semantik. Vor/nach p95/p99 messen.
- **Abnahme:** keine unbounded Owner-Blockade, Quellenverluste nicht verschleiert. **Commit/Evidence:** —.

### B7 — wiederholte Oracle-/Observation-Allokationen
- [ ] **PERFORMANCE · OPEN · P6-MT/S1-LT; Optimierung nur nach Messung.**
- **Pfad:** `liquidation/src/health.rs::components/health`, `liquidation/src/sizing.rs::positions/sizes`.
- **Ist:** `components` kopiert `Account::data.to_vec()` für Maintenance und Equity; `sizes` berechnet `prices()` erneut in Paarschleifen, obwohl `positions` sie bereits erstellt hat.
- **Soll-Fix:** Observation-Buffer je Health-Aufruf wiederverwenden, sicheren AccountInfo-Lifetime-Vertrag wahren; Prices/Floor pro Bank einmal pro konsistentem Slot/Oracle-Snapshot berechnen/cachen, nicht über Versions-/Root-Grenzen wiederverwenden. Keine Semantik-/Rundungsänderungen.
- **Tests:** bitidentische Health/Sizing-Outputs und P1A/P1B-Fixtures, gemessene Allokationszahl/Bytes und p50/p95/p99, Stress mit vielen Banks/Asset-Liability-Paaren.
- **Abnahme:** nachweislich weniger Arbeit bei gleicher finanzmathematischer Entscheidung. **Commit/Evidence:** —.

### B8 — BelowRoot/Confirmed-Reordering: gemeldeter Fixvorschlag gefährlich
- [ ] **REVIEW_REQUIRED · STATE SAFETY · S2/P6; NICHT BLIND ÄNDERN.**
- **Pfad:** `solana/src/state/mod.rs::apply/on_event`.
- **Ist:** Updates `slot < finalized root` werden bewusst als `BelowRoot` verworfen, Counter steigt. Fable vermutet, dass ein nachträglich empfangenes Confirmed-Update neuer als ein einzelner lokaler Account-Stand sein kann.
- **Verbotener Schnellfix:** `if a.slot > existing.slot { apply }` unterhalb Roots ohne Fork-/Finality-/Source-Proof. Das kann die finale State-Konsistenz und gespeicherte Versionen brechen.
- **Soll:** Fall mit konkretem Slot-/Root-/Account-Reordering reproduzieren; unterscheiden, ob Root-Barriere korrekt oder der Snapshot unvollständig war. Bei belegbarer State-Lücke Reconcile/Quarantine mit Provenienz und Counter-/Telemetrieanstieg. Root-Invariante bleibt strikt.
- **Tests:** zwei Forks, Root roll-forward, verspätete confirmed Accounts, fehlende Account-Snapshot-Batches, validiertes Replay, kein stale overwrite, kein falsches READY.
- **Abnahme:** sicherer State gegen Root/Finality; erst nach Beleg ein Codefix. **Commit/Evidence:** —.

### B9 — nicht ausführbare V1-Pläne werden generiert
- [ ] **MEDIUM · P5/FE-02 · G-E.**
- **Pfad:** `runtime/src/execution/builder.rs::plans`; `solana/src/tx/mod.rs::build_v1`.
- **Ist:** Builder fügt `BuildVersion::V1`-Plan hinzu; `build_v1` endet aktuell in `V1Unavailable`; vermeidbare Fehlrecords/Build-Arbeit.
- **Soll:** V1-Plan nur bei **tatsächlich qualifizierter** Build-/Sign-/Simulate-/Submit-Capability generieren, nicht allein aufgrund aktivierter Featureflag oder eines erfolgreichen Wire-Probes. V0 nicht als unqualifizierten Ersatz promoten.
- **Tests:** Capability false/true mit realem Stub, no-V1 bei unavailable, kein Übergehen von V1-Safety-Gate.
- **Abnahme:** nur real baubare Planversionen in Kandidatenliste, beide Pfade bleiben phasentreu. **Commit/Evidence:** —.

### B10 — Keypair-Fehlerpfad und Secret-Zeroization
- [ ] **MEDIUM · G-A vor FE-06.** `solana/src/tx/mod.rs::load_keypair_json`: keine Wallet-Pfade/Secret-Bytes in `Display/Debug`, sensible Zwischenpuffer soweit Rust-/SDK-seitig praktisch möglich sicher droppen/zeroizen; keine unbegründete Garantie über sämtliche Speicher-Kopien. Tests: falscher Pfad, defekter JSON-Inhalt, Secret-Sentinel, Lock-/Test-Fixtures. **Commit/Evidence:** —.

### B11 — Alignment-abhängige bytemuck-POD-Decoder
- [ ] **MEDIUM · G-E zusammen mit B2.** `health.rs`/`sizing.rs` nutzen `bytemuck::try_from_bytes`, `fe_qualification` anderswo unaligned Read. Nur nach Prüfung, ob Referenztypen/Kopien nötig sind, konsistente, formatgerechte Decoder; Alignment darf nicht zufällig vom `Vec<u8>` abhängen. Tests: gezielt unaligned Slice, echte Pinned Fixtures. **Commit/Evidence:** —.

### B12 — Liability-Maintenance-Gewicht aus zwei APIs
- [ ] **REVIEW_REQUIRED · G-E/P3-Parität.** `sizing.rs` `config.liability_weight_maint.into()` vs. `health.rs` `get_weight(Maintenance, Liabilities)`. Echte semantische Gleichheit unter gepinnter Version/EMODE/Integrierter Bank mit Property-/Fixture-Parität beweisen, erst dann ggf. vereinheitlichen. **Commit/Evidence:** —.

### B13 — Jupiter-Fehlerbody unbeschränkt/ungefiltert
- [ ] **HIGH Security · G-A/N5/H2.** `runtime/src/execution/route.rs` schreibt bei Nicht-404-HTTP-Fehlern kompletten `response.text()` in die Fehlermeldung. Nur Status und allowgelisteten, length-bounded Code verwenden; keine Provider-URLs/API-Keys/Body-Freitexte. Tests: 1-MiB-Body, `SECRET`-Marker, Error mit URL/Header. **Commit/Evidence:** —.

### B14 — statische Pubkey-Parse-/LUT-Clone-Arbeit
- [ ] **LOW / PROFILE_REQUIRED · P5/P6.** `builder.rs::receivership`: `Pubkey::from_str(JUP_SWAP_PROGRAM_ID).expect("static")` und `protocol_luts.clone()` je Plan. Nur nach messbarem Effekt und Parität optimieren, keine statischen gültigen Programmpins still umstellen. **Commit/Evidence:** —.

### B15 — Inventarprüfung berücksichtigt neuen Versuch nicht
- [ ] **HIGH vor P8-INV · G-E/G-F.** `liquidation/src/safety.rs::validate_limits`: bereits ausstehendes Inventar gegen **worst-case neues** Collateral/Exposure und globale Holds prüfen, nicht nur aktuellen RAM-Bestand. Tests: Limit genau erreicht, Mehr-Asset, Unknown Exit, gleichzeitige Versuche; niemals Doppelreservierung. **Commit/Evidence:** —.

### B16 — unbekanntes OracleSetup ohne aussagekräftige Dependency-Diagnose
- [ ] **LOW · P1B/S2.** `health.rs::dependency_keys`: fehlende/unsupported Bank-/Oracle-Beziehung in gekapselte UNKNOWN-/Quarantine-Diagnose aufnehmen, aber nicht trotzdem subscribed/qualified vortäuschen. Tests: unknown oracle, missing bank, source identity. **Commit/Evidence:** —.

### B17 — Ingress-Account-Commitment wird als Confirmed gesetzt
- [ ] **REVIEW_REQUIRED · S1/S2.** `solana/src/state/mod.rs::on_event(Account)` setzt `SlotStatus::Confirmed`. Tatsächlichen Subscription-Commitment-/Root-Vertrag untersuchen; Status darf nicht höher sein als belegte Source-Finality. Tests: processed/confirmed/finalized mit Root-Reconciliation. **Commit/Evidence:** —.

### B18 — doppelte Sign-Slot-Zuweisung / toter Pfadcode
- [ ] **LOW · Code-Hygiene.** `runtime/src/execution/pipeline.rs`: `rec.sign_slot` doppelt, `let _ = Path::Rpc`/`SubmissionPath::Rpc`. Aufräumen nur, wenn T_sign-/Timeline-/Telemetry-Parität bleibt; keine Nebenänderung in FE-/Safety-Entscheidungen. Tests: Timeline-Sign-Slot. **Commit/Evidence:** —.


## 7. C — dritter Reviewdurchgang: Recovery, Reconciliation, Owner und Ingress

**Prüfstand:** 2026-10-18, Code auf `main` `6d19e726a028aedd58346bcbaeea96904a7ec868`. Fables Review über `execution/pending.rs`, `liquidation/receivership.rs`, `state_owner/{detect,events,lifecycle}.rs`, `common/lifecycle.rs` und `orchestration/remote_workers.rs` wurde per GitHub-Codeeinsicht abgeglichen. Ergänzend wurden gezielt `state/recovery.rs`, `state/mod.rs`, `submit/rpc.rs` und `state_owner/ingress.rs` gelesen. **Keine Tests lokal ausgeführt; alle Tickets bleiben OPEN.** „Bestätigt“ heißt: der gemeldete Codepfad existiert; ein reproduzierter End-to-End-Langlauffehler ist erst nach Regressionstest bewiesen.

### C1 — Reconnected während Recovery wird als weiterer Recovery-Auftrag verschluckt; nach Exhausted bleibt ein stiller Dauerhalt
- [ ] **HIGH / RECOVERY-LIVENESS · CODE_CONFIRMED / REGRESSION_NOT_RUN · G-A/G-C/G-D · S2-Recovery/P6-LT/C0-E1. Vor Kalibrierung/4h-Readiness verbindlich schließen.**
- **Geprüfter Stand:** `0353badf266d8c06eb5de10cee124df7215b7f23`; `runtime/src/orchestration/state_owner/events.rs::events`, `solana/src/state/recovery.rs::{on_event_cancellable,recover_cancellable}`, `solana/src/state/mod.rs::{on_event,install_snapshot,reconcile,confirm_ready}`, `common/src/lifecycle.rs::reconcile_ok`, `runtime/src/orchestration/state_owner/{ingress,lifecycle}.rs`, `solana/src/ingress/ws_rpc.rs::ready`.
- **Belegte Fehlerursache:** `RecoveryDriver::on_event_cancellable` startet Recovery nur für das **äußere** `IngressEvent::Reconnected`. Während `recover_cancellable` die A/B-Snapshots zieht und auf eine Root-Barriere wartet, führt der Owner-Drain `while let Some(ev) = consumer.try_pop() { s.on_event(ev); }` aus. `Gap` und `Reconnected` werden dadurch zwar in den `StateCore` eingespielt, aber ihre **steuernde Folgeaktion geht verloren**. `StateCore::on_event(Reconnected)` setzt `Halted(FeedGap)`; ein im selben Versuch sauberer Vergleich darf aus `Halted` nicht via `reconcile_ok` zu `Ready` werden. Liegt das neueste `Reconnected` innerhalb des **dritten** Versuchs, endet der Driver mit `RecoveryOutcome::Exhausted { attempts: 3 }`, `events.rs` reagiert abgesehen von Logging nur auf `Ready`, und der Owner wartet in `Halted`. Die WSS-Session kann dabei schon gesund sein; es ist kein weiteres `Reconnected` garantiert. **Die tatsächliche 4h-Häufigkeit ist nicht gemessen, der Kontrollflussfehler aber konkret.**
- **Nicht akzeptable Scheinfixes:** `RunState::reconcile_ok` aus `Halted` freigeben; bei `Exhausted` das Ingress-HALT-Flag löschen; `Ready` allein wegen StreamReady/Root setzen; jedes `Exhausted` mit WSS-Resubscribe beantworten; Recoveries rekursiv aus `drain` aufrufen; endlos bis Fensterablauf auf ein neues Event warten; Snapshot A/B/Root-/ProgramData-Prüfungen überspringen.
- **Implementierung A — Recovery-Control-Signale verlustfrei erfassen:** `DrainFn` bzw. der Owner-Drain erhält einen kleinen **typisierten** `DrainControl`/`RecoverySignal`-Rückgabewert oder eine Owner-owned Pending-Variable. Ein `Reconnected` in der FIFO ist ein **erneut auszuführender Recovery-Auftrag** mit korrekter Reihenfolge/Verbindungs-Epoch (und Original-`attempt`, `t_rx_ns`); `Gap` invalidiert den gerade laufenden Stream; `Fatal` ist terminal. Eventdaten einschließlich `Root/Account/Slot` weiterhin normal in `StateCore::on_event` einspielen. Der Control-Signalstatus wird **nicht** als bloßer bool verfrüht gelöscht; bei `Reconnect→Gap` gibt es kein gültiges StreamReady, bei `Gap→Reconnect` ist das spätere Ready maßgeblich. Duplikate derselben Sessiongeneration deduplizieren; `attempt` allein ist nicht global eindeutig, weil `IngressSessions::spawn` ihn beim Session-Neustart wieder von vorne beginnen lässt (C6/D10).
- **Implementierung B — nichtrekursive, begrenzte Recovery-Wiederaufnahme auf derselben gesunden WSS-Session:** Nach einem Snapshot-/Root-Drain darf ein zwischenzeitlich neu empfangenes `Reconnected` **nicht** als bereits qualifiziert gelten. Den alten Snapshotversuch verwerfen und eine vollständige neue A/B-Snapshot-/Root-/Reconciliation-Runde am **Owner** ausführen, auch wenn das Ereignis aus dem letzten regulären Versuch stammt. Dazu zwei unabhängige Bounds: (1) bestehende `RecoveryDriver::max_attempts=3` je geordnetem Recovery-Zyklus und (2) **höchstens ein zusätzlicher**, ausschließlich durch eine tatsächlich neu eingetroffene und noch nicht verarbeitete StreamReady-Generation belegter Zyklus pro Anlass (maximal zwei Zyklen bzw. sechs A/B-Versuche), plus geltende harte Zeit-/Shutdown-/Root-Catchup-Budgets. Endlos aufeinander folgende `Reconnected` überschreiben den Gesamt-Bound **nicht**. Keine zusätzliche WSS-Verbindung starten, sofern die nachgewiesen wiederhergestellte bestehende Session korrekt liefert.
- **Implementierung C — deterministische terminale Entscheidung:** Wenn `Exhausted` ohne **neuen qualifizierten Pending-Reconnect** auftritt oder Gesamtbudget/Shutdown/`Fatal`/Program-Upgrade/Quellidentität scheitert, **sofort** an Owner-/Econ-/Capture-Controller `STATE_RECOVERY_EXHAUSTED` bzw. `INCOMPLETE_ABORT` übergeben. `IngressHalt` und `RunState::Halted` bleiben; keine Detection oder werttragende Submission. Capture-/Market-Journal einschließlich D1 darf daraus niemals `COMPLETE` machen. Owner und Ingress/Remote/Evidence-Writer geordnet stoppen. Ein WSS-Resubscribe ist ausschließlich bei **eigenem nachgewiesenem Transport-/Subscriptionsfehler**, mit der Owner-Session-/JoinHandle-Qualifikation C6/D10, zulässig; nicht bloß aufgrund des State-`Exhausted`.
- **Implementierung D — Ready ist nur nach vollständigem Beweis zulässig:** Nur `install_snapshot(A)` ⇒ Drain der richtigen Streamgeneration ⇒ Snapshot B ⇒ finale Root-Barriere ⇒ saubere accountbezogene `reconcile` ⇒ `confirm_ready` ⇒ gültiger ProgramData-/Owner-/Oracle-/Dependency-Pin ⇒ `Ready`. Unverarbeitete oder neuere `Gap/Reconnected/Fatal`-Control-Signale verhindern diese Freigabe. `self.halt.clear()` erst **nach** allen Identitäts-/Quiescence-Gates und erfolgreicher Reconciliation, niemals vorzeitig oder bloß für `Exhausted`. Keine Änderung an `common/lifecycle.rs::reconcile_ok`.
- **Zwingende Offline-Regressionen (RED vor Fix, GREEN nach Fix):**
  1. `reconnected_in_third_drain_retries_without_external_event`: A/B des dritten Versuchs empfängt während `drain` ein neues `Reconnected`; danach keine weiteren Queue-Events. Die bestehende WSS-Session bleibt gesund; der Owner beginnt **ohne WSS-Resubscribe** eine weitere vollständige A/B-/Root-Reconciliation und erreicht erst danach `Ready`. Exakt dokumentierte Attempt-/Cycle-Zahl.
  2. `recovery_exhausted_without_fresh_trigger_aborts`: 3 schmutzige Snapshots ohne neues Ready ⇒ einmalig `INCOMPLETE_ABORT`, kein passiver `Halted`-Loop und kein Fake-Ready.
  3. `repeated_reconnected_cannot_extend_budget`: wiederholtes/flatterndes `Reconnected` auch im zusätzlichen Zyklus ⇒ höchstens zwei Zyklen und harter Gesamtdeadline, dann `INCOMPLETE_ABORT`.
  4. `gap_after_reconnected_invalidates_ready` und `gap_then_reconnected_can_recover`: Event-Reihenfolge wird FIFO-genau ausgewertet, Root-/Account-Coverage bleibt zwingend.
  5. `fatal_during_snapshot_or_root_drain_is_terminal`, `program_upgrade_cannot_be_cleared_by_reconnect` und `shutdown_cancels_without_extra_reads`: keine unzulässige neue Recovery oder Submission; C7/Program-Upgrade-Safety bleibt intakt.
  6. `no_duplicate_generation_or_old_session_readiness`: veraltete Events einer abgelösten WSS-Session (C6/D10) können aktuellen State nie zu `Ready` heben; Session-Join-Barriere bleibt.
  7. End-to-End-Owner-Fixture mit Mock-Fetch, echtem `RecoveryDriver` und Fake-Ingress-Queue: `Ready` nur für gültigen neuen Snapshot/Root, ansonsten begrenzter kontrollierter Abbruch; Captures/Market-Journal manifestieren `INCOMPLETE` (D1).
- **Abnahme/Beweis:** Vorher fehlgeschlagene Regression sowie grüne `crate_health`-/StateOwner-/Recovery-Tests und Release-Build (getesteter SHA + Exit-Codes) in `project_0/docs/evidence/BUGS_YYYYMMDD/`; `STATUS.md`/`MEASUREMENT_LEDGER.md` referenzieren C1. **Keine Mainnet-/4h-Ausführung in dieser Korrektur, kein automatischer LIVE-GO.** Erst danach `- [x]` setzen, ursprüngliche Ursache durchstreichen. **Commit/Evidence:** —.

### C2 — mehrfacher Health-Aufruf pro Kandidat
- [ ] **MEDIUM / PERFORMANCE · CODE_CONFIRMED · P6-LT/P6-MT · Cross-Ref B7/B6.**
- **Pfade:** `runtime/src/orchestration/state_owner/detect.rs` (`engine.health`), `runtime/src/execution/pipeline.rs::evaluate` (`self.health.health`), `liquidation/src/sizing.rs::sizes` (`CrateHealth.health`), `liquidation/src/health.rs::components`.
- **Beobachtung:** Detektor und Pipeline rechnen Health unabhängig, der Sizer erneut; `components` kopiert Observation-Daten für Maintenance und Equity. Mehrfacharbeit und potenziell verschiedene Momentaufnahmen bei nicht streng identischen Eval-Inputs. MT0 `DetectConsumer/Pipeline` und `detect_health_ns_*` vorhanden.
- **Fixvertrag:** Zunächst die wirkliche Zahl/Last anhand MT0/Profiles belegen, dann eine konsistente, slot-/root-/timestamp-/revision-gebundene typisierte `HealthView` berechnen und nur bei identischer Stategeneration an Pipeline/Sizer weiterreichen; Ownership und Error-/UNKNOWN-Semantik der drei Aufrufpfade erhalten. Keine unvalidierte globale Cache-Nutzung, keine Preis-/EMODE-/Rundungsänderung.
- **Tests:** Callcounter bei gleichem Kandidaten (vorher 3, nachher nur berechtigte Recomputes); Read-only Fixture für die vier fachlichen Liquidationsklassen, bitidentische Maintenance/Equity/Sizing/Simulation, Snapshotwechsel erzwingt Recompute, p50/p95/p99 und Alloc-Profile.
- **Abnahme:** nachweisbar weniger CPU/Allokationen **bei gleicher** wirtschaftlicher/Health-Entscheidung. **Commit/Evidence:** —.

### C3 — P8-Reconciliation besitzt keinen vollständigen Settlement-/PnL-/Freigabepfad
- [ ] **CRITICAL vor P8-LIVE · CODE_CONFIRMED · G-E/G-F · FE-04/P8-05 · Cross-Ref B1/B3/B15.**
- **Pfade:** `runtime/src/execution/pending.rs::try_reconcile`, `runtime/src/execution/pipeline.rs::reconcile_outcome`, `liquidation/src/safety.rs::record_outcome`, `runtime/src/orchestration/remote_workers.rs` und FE-04-Journal.
- **Beobachtung:** `try_reconcile` setzt `realized_pnl: None` und `pnl_complete: false` für **jeden** Outcome, auch `LANDED_SUCCESS`. Der Record enthält `exposure_released: p.exposure`, aber das ist bei UNKNOWN **nur ein Vorschlag**, keine tatsächliche Kapitalfreigabe: `Safety::record_outcome` HALT(UnknownEconomics) und gibt vor der `None`-Rückkehr weder Kapital noch Inventar frei. Nach 90 s ohne vollständige Signatur-/Fee-Evidence erzeugt `try_reconcile` `UNRESOLVED` (keinen gesicherten Nicht-Landung-Nachweis). Das ist zunächst korrekt fail-closed, aber kein vollständiger Controlled-LIVE-Pfad.
- **Fixvertrag:** Einziger durable FE-04-/P8-Ledger-Automat: Signaturen, Blockhash-/lastValidBlockHeight, finalized Receipt oder **strenger** Nonlanding-/Expiry-Proof, **alle** Bundle-/Tip-/Prerequisite-Transaktionen, Fees, Rent, Token-/Protocol-/Wallet-Deltas, realisierter Exit und konservativer `i128` Net PnL. Idempotente Settlement-Transitions, Auto-HALT bei UNKNOWN, globale Holds bis belastbarer Abschluss. Feld `exposure_released` erst als **tatsächlich gebucht** ausweisen oder eindeutig `requested_exposure_release` nennen.
- **Tests:** mocked finalized landed-success/failure, never-landed mit Blockheight+historischer Abwesenheitsprüfung, MissingArchive/429 ⇒ UNKNOWN, teilweise bestätigtes Bundle, Gebühren ohne Token-Deltas, mehrfacher Replay und Restart, keine Doppelbuchung; `capital_in_use` sinkt erst nach dauerhaft verbuchtem, vollständig bewiesenem Outcome.
- **Abnahme:** FE-04 und P8 teilen denselben Settlement-/Safety-Source-of-Truth; nicht `pnl_complete=false` als Abschluss verkaufen. **Commit/Evidence:** —.

### C4 — P8-Landing-Reconcile benutzt confirmed statt finalized
- [ ] **CRITICAL vor werttragender Reconciliation · CODE_CONFIRMED · G-E/G-F · P8/FE-04 · Cross-Ref C3.**
- **Pfad:** `solana/src/submit/rpc.rs::reconcile` ruft `transaction_with_meta(..., CommitmentConfig::confirmed())` und nutzt Signaturstatus ohne zwingende finalized-Barriere; `pending.rs::try_reconcile` klassifiziert daraus `LANDED_SUCCESS/FAILED`.
- **Risiko korrekt eingeordnet:** **Heute** führt C3 bei `realized_pnl=None` trotzdem zu HALT und hält Kapital zurück. **Sobald C3 geschlossen wird**, könnte der jetzige confirmed-Landing-Pfad vor finalisiertem Chainbeleg Holdings lösen oder Outcome/PnL buchen. FE-04 hat bereits den strengeren `finalized_transaction`-Einstieg; keine Abschwächung dieses Vertrags.
- **Fixvertrag:** Kritische Finance-Settlement-Funktion verwendet ausschließlich finalisierte Receipt-/Meta-/Signature-/Token-/Rent-Nachweise mit Slot-/Fork-/Source-Konsistenz; `confirmed` darf als **vorläufige Telemetrie** ohne `capital_released`/PnL verwendet werden. Finalization-Reorg/Gap/Proof-unavailable ⇒ UNKNOWN/HALT. Keine bloße Ersetzung einer einzigen Konstante, wenn Status und Metadaten unterschiedliche Commitments/Slots besitzen.
- **Tests:** confirmed→missing/reorg, confirmed-success→finalized-failure, finalized mit vollständigen Gebühren/Deltas, Status finalized aber Meta absent, Fork-Drift, Replay. **Abnahme:** kein Finance-Release/Outcome vor finalized-gebundenem vollständigen Settlement. **Commit/Evidence:** —.

### C5 — finanzielle Reconciliation konkurriert im Winner-Workload-Budget
- [ ] **MEDIUM vor P8-LIVE · CODE_CONFIRMED · G-E/G-F · P8/Provider.**
- **Pfad:** `runtime/src/orchestration/remote_workers.rs::start` und `solana/src/provider.rs`.
- **Ist:** `winner-rpc` und `reconcile-rpc` konstruieren beide Clients mit `Workload::Winner`; Providerlimiter mappt diese Klasse auf `analytics_rps` (Default 2/s) und gemeinsamen maximalen Provider-RPS-/FREE_ONLY-Vertrag.
- **Soll-Fix:** Separater, klar priorisierter Reconcile-Workload **innerhalb** derselben globalen echten Providergrenze, mit Admission-/Deadline-/Burst-Fairness, ohne neues ungeprüftes Requestbudget oder unbegrenzte Retry-Pfade. Darf C3/C4 nicht vortäuschen. Bei Quota-Mangel: UNKNOWN und konservativer HALT; Winner-Analytics darf blockiert werden, Finance-Evidence nicht still degradiert.
- **Tests:** 100 Winner-Jobs + eine Settlement-Aufgabe bei engem lokalem Fake-Quota, Reconcile-Vorrang, Provider Gesamt-RPS nie überschritten, 429/adaptive cooldown, Shutdown und no-spin.
- **Abnahme:** priorisierbare sichere Reconciliation ohne FREE_ONLY-Aufweichung. **Commit/Evidence:** —.

### C6 — WSS-Resubscribe verliert Session-JoinHandle
- [ ] **MEDIUM / REVIEW_REQUIRED für Duplikatwirkung · G-A/G-C · S0/S2.**
- **Pfade:** `runtime/src/orchestration/state_owner/events.rs` Dependency-Change-Resubscribe, `state_owner/ingress.rs::IngressSessions::spawn`.
- **Ist:** `IngressSessions::spawn` startet den Thread, gibt aber nur `Shutdown` zurück; der Thread-JoinHandle wird verworfen. Events ruft `self.ingress_session.request()` und unmittelbar darauf `ingress.spawn` mit **derselben Producer-/IngressHalt-Instanz**. Kurz überlappende Event-Generationen sind möglich; „bis 100 ms“ ist **nicht als harte Laufzeitgrenze bewiesen**.
- **Soll-Fix:** Cancel + geordneter Join (bounded mit nachgewiesener Cancellation) und eindeutige Sessiongeneration vor neuem Spawn; nachgewiesen kein alter Writer in neuer Generation, keine irrtümliche Clearing-/HALT-Bedingung durch alte Session. Shared `SlotWatch` und Root-Beweis bleiben kohärent. Kombiniert mit C1, **keine zweite Restart-Implementierung**.
- **Tests:** Slow old session, Cancel während Blocked-Recv, Duplikate nach Restart, Old-Fatal nach New-Ready, mehrfaches Update, hard stop. **Commit/Evidence:** —.

### C7 — Ingress Fatal kann den Owner passiv bis Laufzeitende halted lassen
- [ ] **HIGH vor 4h · CODE_CONFIRMED / TERMINAL-BEHAVIOR-TEST_NOT_RUN · G-C/G-D.**
- **Pfade:** `solana/src/ingress/ws_rpc.rs` emittiert `IngressEvent::Fatal("WSS_RECOVERY_EXHAUSTED")` und beendet Transport-Thread; `solana/src/state/recovery.rs::on_event_cancellable` startet Recovery ausschließlich für `Reconnected`; `runtime/src/orchestration/state_owner/events.rs` besitzt für `Fatal` keinen terminalen Branch; `state_owner/lifecycle.rs` bleibt bis Window-/globalem Stop aktiv.
- **Soll-Fix:** Terminales Feed-Fatal an Owner-/Capture-Window-/Evidence-HALT propagieren, kontrolliert alle Worker/Writer/Segmente drain/seal, Run als `INCOMPLETE_ABORT` mit Ursache `WSS_RECOVERY_EXHAUSTED` beenden, nie `READY`/`PASS`. Bei echter Recovery noch verfügbare Quelle darf nur eine gesondert begrenzte, belegte neue Generation wieder starten; nicht still warten.
- **Tests:** Fatal ohne Folgeevent, Fatal während A/B-Snapshot, Writer-Queue voll, atomarer Window-Abschluss, keine weitere Detection/Submission, Prozess beendet innerhalb Run-Deadline mit Nicht-0/INCOMPLETE. **Commit/Evidence:** —.

### C8 — pro Kandidat kopierte Programm-Allowlisten
- [ ] **LOW / PROFILE_REQUIRED · P6-MT/P6-LT.**
- **Pfad:** `state_owner/detect.rs`: `global_allowed.clone()`, `prereq_allowed.clone()` pro `Pipeline`.
- **Soll:** Borrowed immutable slices/Arc nur bei nachgewiesener Allokations- und Ownership-Parität; keine ungewollte Mutation während Kandidatenbewertung. Test: identical allow/deny and build under concurrent lookups; Profile vor/nach. **Commit/Evidence:** —.

### C9 — Blockhash-Höchstalter dreifach hart kodiert
- [ ] **LOW / CONFIG_CONSISTENCY · P6.**
- **Pfad:** `state_owner/detect.rs` zweimal, `state_owner/events.rs` einmal `30_000_000_000` ns.
- **Soll:** Eine einzige explizite, versionierte Deadline-/Freshness-Policy statt versteckter Zahlen; **keine Erweiterung** der Gültigkeit ohne Evidence/Autorisation. Test: gleiche Grenzwerte an allen drei Gates, 1 ns vor/nach Cutoff. **Commit/Evidence:** —.

### C10 — ReserveStale wird via Fehlermeldungstext reklassifiziert
- [ ] **MEDIUM / ERROR-ROBUSTNESS · P3/P5.**
- **Pfad:** `state_owner/detect.rs`: `rec.reason.contains("ReserveStale")`.
- **Soll:** Typisierte/enum Fehlerursache an Source-/Integration-Adapter erzeugen, ohne String-Contains auf wechselnden Provider-/Crate-Texten. Test: Fehler mit anderer Formulierung ⇒ gleicher `IntegrationRefresh`-LossReason; fremder Text „ReserveStale“ darf nicht falsch klassifizieren. **Commit/Evidence:** —.

### C11 — Available Inventory in Detection ohne in-flight Holds
- [ ] **HIGH vor P8-Multi-Asset/Parallel-LIVE · CODE_CONFIRMED / IMPACT_SCOPE_SINGLE_FLIGHT · Cross-Ref B15/B3.**
- **Pfad:** `state_owner/detect.rs`: `inventory` schätzt ATA-Lamports allein aus RAM-State; Safety/FE-04 holds werden beim Angebot nicht abgezogen.
- **Ist:** Bei aktuellem `max_outstanding_attempts=1` nicht automatisch schädlich, da finanzielle Single-Flight weiterhin separat durchsetzbar bleiben muss. Bei späterer paralleler Inventory-/Fundingplanung kann dasselbe Collateral für zwei Vorhaben reserviert werden.
- **Soll:** konsistente, dauerhafte, walletweite verfügbare Menge = finalisierte/verifizierte Bestände minus **alle** offenen Reservierungen einschließlich UNKNOWN und konkurrierender Bots; dynamische Reconciliation. Ohne gültige State-/Hold-Provenienz ⇒ UNKNOWN/no-plan.
- **Tests:** zwei konkurrierende Kandidaten, andere Bot-Owner, Crash/Restart, UNKNOWN, NO_SWAP-Collateral, intra-slot Deltas, Idempotency; Inventory nie doppelt ausgegeben. **Commit/Evidence:** —.

### C12 — Reconcile-Timeout von 90 s ohne qualifizierte Chain-Abwesenheitsprüfung
- [ ] **MEDIUM als Konfiguration; HIGH für Safety-Settlement · Cross-Ref C3/C4.**
- **Pfad:** `state_owner/remote_workers.rs`: `try_reconcile(..., 90)`.
- **Soll:** Explizite versionierte Deadline-/Provider-/Blockhash-Validity-Policy; bloßer Wallclock-Timeout ist niemals Beweis für `NotLanded`. Timeout führt UNKNOWN, bis finalized Nonlanding/lastValidBlockHeight mit ausreichend vollständiger History nachgewiesen wird.
- **Tests:** 89/90/91 s und langsamer finalized Slot, RPC-429/GAP, nicht bestätigte Bundle-Signaturen, realer lastValidBlockHeight-Cutoff als synthetisches Fixture, kein falsches `EXPOSURE_RELEASED`. **Commit/Evidence:** —.

### C13 — Modulprüfung bewusst unvollständig
- [ ] **REVIEW_REQUIRED / NICHT ALS BEHOBEN MARKIEREN.**
- **Bereits gezielt begutachtet:** `pending.rs`, `receivership.rs`, `detect.rs`, `events.rs`, `lifecycle.rs`, `remote_workers.rs` sowie für C1/C4 relevante `recovery.rs`, `rpc.rs` und `ingress.rs`-Teile. Fable meldete für `receivership.rs` (Anchor-Builder), `measured_attempt_costs`, `RunState` und RemoteWorker-Bounds keinen weiteren spezifischen Defekt; daraus folgt **keine Gesamtqualifikation**.
- **Noch zu prüfen:** gesamtes `sweep.rs` (Rayon-Health im Owner), `winner_tracker.rs`, `integration.rs`/`projection.rs`/`index.rs`, `funding.rs`/`planner.rs`/`protocol.rs`, `census.rs`, Market-Observer/`econ*.rs`/`chain.rs`/`report.rs`/`measurement.rs`, volle `ingress.rs`-Start/Stop-Integration und Python-/Verifier-Skripte. Jede neue harte Regression ⇒ eigene stabile Ticket-ID, nicht C1 umdefinieren.
- **Abnahme:** vollständiger File-/Owner-/Contract-Coverage-Bericht mit überprüften Versionen; Nicht-Befunde nicht als PASS des Gesamtsystems ausweisen.


## 8. D — vierter Reviewdurchgang: Market-Evidence, Funding, Sweep und Ingress

**Review-Stand:** 2026-10-18, betroffene Codepfade auf `main` bei `4e7f4a413faaa158bb1b91d4678a8de0312e0e5c` gelesen. Nachfolgender `main`-Commit `9b250dacc1e97f07e2fff56c8a48d53380528a1a` ändert nur Reporter-Quellen und -Tests, nicht die hier untersuchten Funktionen. Von Fable gelesen: `state_owner/sweep.rs`, `state_owner/ingress.rs`, `liquidation/{protocol,funding}.rs`, `market/{capture,sink,core}.rs`. Zusätzlich wurden `market/summary.rs`, `market/observer_tests.rs`, `execution/{builder,pipeline}.rs` für die betreffenden Pfade gegengeprüft. **Alle D-Tickets bleiben offen: keine neuen Fixes und keine lokal ausgeführten Regressionstests.** Verbindliche Fixvorgaben gelten vorbehaltlich fachlich identischer Paritätstests; keine Trading-/Safety-/Quota-Regel wird stillschweigend verändert.

### D1 — geordneter Market-Capture-Abbruch versiegelt Journal fälschlich COMPLETE
- [ ] **HIGH / EVIDENCE INTEGRITY · CODE_CONFIRMED · G-A/G-C/G-D · PERSIST/SK/C0-E1; VOR 4H.**
- **Pfade:** `runtime/src/market/sink.rs::MarketSink::finish/JournalSink::finish`, `runtime/src/market/summary.rs::finalize`, `runtime/src/market/capture.rs::finish_capture/run`, `analytics/journal.rs::verify_chain`.
- **Ist/Reproduktion:** `JournalSink::finish()` ruft unabhängig vom Outcome `seal(true)`. Bei `stop_reason` = `OPERATOR_STOP`, `STREAM_NOT_READY`, `INGRESS_THREAD_ENDED`, `C0_E1_ABORT` oder bei `drain_complete=false` schreibt `ObserverCore` zwar `INCOMPLETE`-Coverage und die Summary meldet Nichtvollständigkeit; der Writer erhält trotzdem `complete=true`. Dadurch `MarketJournalBinding.status=COMPLETE` und ein struktureller Verifier kann `verify_chain().complete=true` berichten. **Wichtig:** Manifest-COMPLETE bezeichnet einen korrekt abgeschlossenen Capture-Schreibstrom, nicht automatisch die nachgelagerte vollständige kanonische Markt-/Programmpfad-Verifikation. Beide Achsen müssen unabhängig fail-closed bleiben.
- **Verbindlicher Fix:** `MarketSink::finish(&mut self, complete: bool)` beziehungsweise äquivalent explizite typisierte Completeness im gemeinsamen Finishing-Vertrag; `capture::finish_capture`/`summary::finalize` übergibt mindestens `stop_reason == "TIMER" && drain_complete && write_error.is_none()`. Ein persistenter `core.incomplete_reasons`-/Gap-/Ingress-Abort darf nie durch `seal(true)` zu einem behaupteten *vollständigen Capture* hochgestuft werden; Definition der einzelnen Vollständigkeitsachsen mit `C0E1_4H_READINESS.md` abstimmen. `JournalSink` versiegelt entsprechend; `JsonlSink`/Pinned-Offline-Export nur an seiner tatsächlichen Batch-Grenze abschließen (siehe D4), kein vorzeitiger Kompatibilitätsbruch. Schriftliche Summary-/Binding-/Manifest-/Verifier-Konsistenz.
- **Regression RED→GREEN:** `capture_operator_stop_seals_incomplete` (real `finish_capture`, `verify_chain.complete=false`, Binding INCOMPLETE, Coverage INCOMPLETE); Matrix `STREAM_NOT_READY`, `INGRESS_THREAD_ENDED`, `C0_E1_ABORT`, `WRITE_ERROR`, `drain=false`, vollständiger TIMER+Drain; `Writer::seal(false)`/Manifest/Schema-Provenienz; nach `seal(false)` kein stilles `resume` als Complete, keine überschriebenen historischen Journale.
- **Abnahme:** **Alle Abbruchpfade liefern dieselbe nachweisbare unvollständige Evidence-Semantik** in Journal, Binding, Summary und Report; keine falsche FULL_CAPTURE-/FULL_MARKET-Qualifikation; Tests/Commit-SHA/Evidence dokumentiert. **Commit/Evidence:** —.

### D2 — NoSwapInventory und PreheldInventory erzeugen doppelte technische Arbeit
- [ ] **MEDIUM / PERFORMANCE + PROVIDER QUOTA · CODE_CONFIRMED · G-A/G-E · FE-02/P5/P6.**
- **Pfade:** `liquidation/src/funding.rs::is_feasible/feasible_classes/capital_required`, `runtime/src/execution/builder.rs::plans`, `runtime/src/execution/pipeline.rs::evaluate`.
- **Ist:** `NoSwapInventory` und `PreheldInventory` prüfen beide `liability_inventory >= liability_needed`, binden denselben `liability_needed_value`-Betrag. `builder::plans` ruft für beide dieselbe `receivership(..., vec![], true)` auf und erzeugt V0-/NoSwap-Pläne. `pipeline::evaluate` baut die Varianten separat und kann beide über `fee_for_message` bepreisen. **Zwei identische RPC-Calls sind nur dann belegt, wenn beide Varianten den jeweiligen Fee-Branch tatsächlich erreichen; nicht jede Opportunity erzeugt zwangsläufig zwei Calls.**
- **Verbindlicher Fix:** **Keine Fundingklasse aus dem Phasenplan streichen.** Zunächst volle semantische Gleichheit von TX-Bytes, LookupTables, Token-/ATA-/Inventar-Provenienz, Kosten, Plan-Identity und Safety-Holds nachweisen. Falls byte- und entscheidungsgleich: technische Build-/Sign-/Fee-Evidence pro deterministischem Message-/Fee-Cache-Key nur einmal erzeugen und an beide Attributions-/Fundingkandidaten anbinden. Falls Kapitalherkunft, Exit-/Inventorystrategie oder spätere Kosten abweichen, nicht blind deduplizieren. Kein Cross-Slot-/Cross-Blockhash-Reuse, kein Quote-Zero-Default.
- **Tests RED→GREEN:** identische NoSwap/Preheld-Pläne → ein Build/Feecall, **beide** Fundinglabels/Reports bleiben vorhanden; abweichende Plan-/Fee-/Blockhash-/LUT-Inputs → getrennte Arbeit; Mock-RPC-Callcounter 5-rps-Policy unverändert; economics/safety parity und Funding-Class-Schema. 
- **Abnahme:** doppeltes Remote-I/O nachweisbar reduziert, vollständige Entscheidungs- und Finanzattribution unverändert. **Commit/Evidence:** —.

### D3 — Sweep-Fan-out ist accountseitig unbounded und blockiert Owner-Ingress
- [ ] **HIGH / LIVELESSNESS + CAPTURE INTEGRITY · CODE_CONFIRMED · G-A/G-C/G-D · R127/S1-LT/P6-MT; STRESSTEST VOR 4H.**
- **Pfade:** `runtime/src/orchestration/state_owner/sweep.rs::sweep`, `state_owner/events.rs` und Owner-Loop, `solana/src/runtime_config.rs`.
- **Ist:** `sweep_batch_max_events` begrenzt geänderte Quell-Events, **nicht** die per Reverse-Index zugeordneten Debtor-Accounts. `account_work` wird komplett aufgebaut, anschließend ganz per `into_par_iter()` auf dem Owner evaluiert und sortiert; während des Blocks wird `consumer.pop` nicht bedient. Bei Default `workers=1` arbeitet Rayon faktisch seriell. Ein Bankupdate kann große C0-E1-/R127-Census-/Health-Fan-outs erzeugen; die genaue maximal erreichte Blockierungszeit ist ohne Lastmessung unbekannt.
- **Verbindlicher Fix:** Vor der 4h-Bereitschaft synthetischer deterministischer Lasttest für größte erwartete Dirty-Bank-/Account-Menge (auch 1 Worker). Bei unzulässiger Owner-Stall/Overflow: stabile **gebundene** Account-Work-Queue mit fairen Chunks und wiederholter Ingress-Drain-Opportunity, Snapshot-/Root-/input_revision-/event_slot-Provenienz pro Work-Item, Dedup/Force-Rearm/faire Retryordnung. Keiner der sechs Instruktionsmarker und keiner der Debtor-/Census-Fälle darf still verloren gehen. No O(n²)-Kopie, kein unbounded Aufstauen oder willkürliches Dropping. Optional gesonderter CPU-Worker nur mit eindeutiger Ownership/Backpressure; keine parallelen mutable State-Zugriffe.
- **Tests RED→GREEN:** 1/2/N Rayon-Worker, eine Bank mit 100/10.000/100.000 abhängigen Accounts, Dirty-Event mitten im Lauf, Root-/Oracle-Revision während Chunkgrenze, Shutdown, starvation/oldest-first, voller Queue-/RAM-Budget, Counterparität (unique/forwarded/healthy/liquidatable/pending/census), max Owner-blocking p95/p99 und kein unregistrierter Ingress-Overflow.
- **Abnahme:** realistische Fan-outs unter begrenzter Owner-Blockdauer und ohne Vollständigkeitsverlust; erst echte Kalibrierung misst freie Kapazität. **Commit/Evidence:** —.

### D4 — `on_logs` ruft `flush` pro TX auf und kann Kompatibilitätssink früh versiegeln
- [ ] **MEDIUM / CAPTURE-FAILURE · CODE_CONFIRMED · G-A/G-C · JFI/PERSIST.**
- **Pfade:** `runtime/src/market/core.rs::on_logs`, `runtime/src/market/sink.rs::JournalSink::flush/JsonlSink::flush`.
- **Ist:** Nach jeder zugelassenen Logs-TX `sink.flush()?` mit veraltetem `BufWriter/SIGTERM`-Kommentar. `JournalSink::flush` ist ein No-op (eigener Writer batcht/synct); `JsonlSink::flush` führt einmal `inner.seal(true)` + Legacy-Export durch. Beim selben `on_logs`-Sink würden Folgeframes mit `MARKET_JOURNAL_ALREADY_FINISHED` abbrechen. Strict-Batch-`JsonlSink::flush` ist bewusst ein **Batchabschluss**, kein generischer TX-Flush.
- **Verbindlicher Fix:** Unqualifizierten per-TX-`flush`-Aufruf und Kommentar aus `on_logs` entfernen; Writer-Lane darf weiter gemäß `WriterConfig` flush/sync machen; `JsonlSink` nur explizit an der Verifier-Batch-Grenze abschließen. Bei notwendigem Durability-Checkpoint separate native API statt `finish`-Semantik missbrauchen.
- **Tests:** zwei/drei Logs-TX nacheinander mit `JsonlSink` → alle Rows vor explizitem Batch-Finish; `JournalSink` Batch-/Sync-Policy bleibt unverändert; Strict-Exporter paritätisch, kein JSON-Writer im produktiven Capture.
- **Abnahme:** keine vorzeitige Versiegelung/Verlustfolge durch reguläre TX-Verarbeitung. **Commit/Evidence:** —.

### D5 — unerreichbar angenommener Receivership-Decoderzweig kann Capture panicken
- [ ] **MEDIUM / PANIC HARDENING · CODE_CONFIRMED, TRIGGER_NOT_PROVEN · G-A/G-C · C0-E1.**
- **Pfad:** `runtime/src/market/core.rs::on_logs`, `decoded.additional_events`, `LiquidationType::Receivership`.
- **Ist:** `unreachable!("original receiver decoder handles this type")` in von externen Chain-Logs abgeleitetem Decoderausgang. Ob der gepinnte Decoder diesen Enum-Fall derzeit tatsächlich in `additional_events` liefern kann, ist **noch nicht bewiesen**; ein unerwartetes Verhalten würde den Capture-Thread paniken lassen.
- **Verbindlicher Fix:** Kein `unreachable!/panic!/unwrap` auf untrusted Decodevarianten. Explizit fehlerhaftes/inkonsistentes Decoderresultat zählen und als `UNKNOWN/INCOMPLETE` qualifizieren, keine Liquidation unterschlagen und keine falsche Duplikatzählung. Keine stille `continue`-Lösung, die den Pfad als vollständige Evidence erscheinen lässt.
- **Tests:** direkt injizierter zusätzlicher Receivership-Event, gültige Classic/Deleverage/Bankruptcy, Doppeldekodierung, Truncation, fuzz-/property-Test aller Varianten; kein Panic, keine falsch vollständige Coverage.
- **Abnahme:** Decoderfehler fail-closed in Evidence und Report, Capture bleibt kontrolliert. **Commit/Evidence:** —.

### D6 — MarketStats pro Ingress-Event zweimal geklont und emittiert
- [ ] **LOW / PERFORMANCE · CODE_CONFIRMED · G-C/G-D · MT0/Market-Observer.**
- **Pfad:** `runtime/src/market/core.rs::on_ingress_event`.
- **Ist:** Mit `econ` klont der Code `self.stats` vor dem Event und nachher erneut, erzeugt `Box<MarketStats>` und einen `MarketCounters`-Channel-Event auch bei unveränderten Countern (u. a. Slots). Slot-Evidence selbst muss weiterhin getrennt emittiert werden.
- **Verbindlicher Fix:** Zuverlässige Differenz-/Delta-Erkennung mit stabiler Zeit-/Slot-Provenienz; nur bei Änderung der zu überwachenden Zähler Counter-Evidence erzeugen; notwendige Heartbeat-/Slot-/Cutoff-Evidence unverändert. Option: konkrete changed-bit/typed delta statt jedes Mal Vollkopie, soweit Analyzer-Vertrag es erlaubt.
- **Tests:** unverändertes Event → keine Counter-Kopie/Emission; echte Änderung → exakt einmal, Index-/Ordering-/Timestamp-Parität, High-Rate-Slotstorm, keine Messlücke bei monotonen Countern; allokationsbezogenes Profil.
- **Abnahme:** weniger Byte-/CPU-/Channel-Last ohne Verlust der qualifizierenden Evidence. **Commit/Evidence:** —.

### D7 — String-basierte Market-Keys und Retention-Cap verursachen unnötigen Speicher-/Coverage-Druck
- [ ] **MEDIUM / MEMORY + COVERAGE · CODE_CONFIRMED, CAP_EXHAUSTION_LASTTEST_NOT_RUN · G-C/G-D · SK-03/C0-E1.**
- **Pfade:** `runtime/src/market/core.rs::ObserverCore` (`seen`, `liquidation_signatures`, `targets`, `receivers`, `admit`), `market/decoding.rs` und `market/configuration.rs`/Econ-Kapazität.
- **Ist:** mehrere `HashSet/HashMap<String>`-Keys, Signaturen aus WSS/Logs oft im Stringformat; `admit()` und `admit_identity()` duplizieren Keys. Ohne Econ-Handle arbeitet Teile der Retention mit Fallback-Cap `100_000`, bei Überschreiten `C0_E1_MARKET_RETENTION_OVERFLOW`/Truth `retention_error`. Ob die Cap im konkreten 4h-Markt erreicht wird, ist zu **messen**, nicht zu behaupten. „Halber Speicher“ für typisierte Keys ist **ohne Messung keine gesicherte Zahl**.
- **Verbindlicher Fix:** Quelle/Typen/Subscribers analysieren, native `Signature/Pubkey` nur dann in Hotpath übernehmen, wenn kein JFI-/kompatibles Wire-Schema verletzt wird; Bound/Dedupe/TTL für gesamten Beobachtungszeitraum beweisen. Nicht einfach Cap erhöhen oder alte Signaturen evicten, weil das Markt-Wahrheits-/Dedup-Claims entwertet. Bei Overflow `INCOMPLETE` bleiben, kein stiller Drop.
- **Tests:** gleiche deduplizierte Signature aus zweierlei Quellen und Slots, mehr als 100k einzigartige TX, 4h-Projektionsprofil, p95 RAM/Bytes pro Key, Sort-/Hash-Parität, Cap-Gate, vollständige Receiver-/Target-Zähler.
- **Abnahme:** memory-bounded mit expliziter Coverage-/Retention-Qualifikation, keinerlei fälschliche Vollständigkeit. **Commit/Evidence:** —.

### D8 — Market-Capture-Thread wird nur bei bereits abgeschlossenem Lauf gejoint
- [ ] **MEDIUM / SHUTDOWN LIFECYCLE · CODE_CONFIRMED · G-A/G-C; VOR LANGEN LÄUFEN.**
- **Pfad:** `runtime/src/market/capture.rs::run`.
- **Ist:** Nach `session.request()` und Drain erfolgt `if ingress.is_finished() { ingress.join() }`; wenn der Thread beim Rücksprung noch lebt, wird der JoinHandle verworfen. Consumer-Drop mag später beenden, belegt aber keinen geordneten Shutdown/keine aktuelle Threadquiescence.
- **Verbindlicher Fix:** eigene Session-Cancellation und **nachweisbar bounded** Join-/Shutdown-Barriere mit dokumentiertem RPC/WSS-Timeout, rechtzeitige Abbruch- und Writer-Finalisierung. Da `std::thread::JoinHandle::join` selbst nicht timeout-fähig ist, keine irreführende `join_timeout`-Behauptung ohne entsprechende Architektur. Bei Überschreitung globalen Stop-/Drain-Budgets fail-closed `INCOMPLETE_ABORT`; niemals weiter laufenden Ingress als ruhend deklarieren.
- **Tests:** langsam endender Ingress, blockiertes Receive, StreamReady nicht erreicht, OperatorStop, Writer-Drain und FullQueue; Threadzähler und keine Emission nach Return.
- **Abnahme:** keine detached marktaktive Session nach kontrolliertem Capture-Abschluss. **Commit/Evidence:** —.

### D9 — synchrones stderr auf Capture-Thread kann Ingress-Verarbeitung blockieren
- [ ] **MEDIUM / BACKPRESSURE · CODE_CONFIRMED · G-A/G-C · Cross-Ref M2.**
- **Pfade:** `runtime/src/market/capture.rs` (Status alle etwa 2 s), `runtime/src/market/core.rs::on_logs` (pro erkannter Liquidation).
- **Ist:** Direktes `eprintln!` aus Capture-/Decode-Pfad kann bei vollem Pipe-/langsamen Dateisystem blockieren. Nur Shell-Umleitung auf Datei ist **kein vollständiger Beweis** für nichtblockierendes stderr.
- **Verbindlicher Fix:** bounded/try-send Diagnostik entkoppeln, strukturiertes essentielles Capture-Journal unverändert vollständig, optionale Terminalausgabe darf begrenzt gedroppt/aggregiert werden (Counter). Shutdown darf keinen blockierten Konsolen-Join abwarten (M2).
- **Tests:** blockierte stderr-Pipe, 1000 Liquiderfassungen, Writer-Backpressure, Terminal unavailable, kein erzwungener Ingress-HALT allein wegen optionaler Ausgabe, Summary/Counts identisch.
- **Abnahme:** Terminal-Diagnostik ist kein unbounded Capture-Hotpath. **Commit/Evidence:** —.

### D10 — verworfener JoinHandle beim StateOwner-Resubscribe
- [ ] **MEDIUM / CONFIRMED DUPLICATE · Cross-Ref C6/C1/M3.**
- **Pfad:** `runtime/src/orchestration/state_owner/ingress.rs::IngressSessions::spawn`.
- **Ist:** `spawn()` startet `ingress-ws`, wirft den `JoinHandle` weg und gibt nur `Shutdown` zurück. Beim Dependency-Resubscribe können alte/neue Session den gleichen Producer/Halt kurz gleichzeitig nutzen. Damit ist der zentrale Codebeleg von C6 bestätigt.
- **Verbindlicher Fix:** **C6 implementieren**, nicht zweiten Lifecycle-Mechanismus aufbauen; Session-Owner enthält Cancellation, Generation-ID, JoinHandle und geordneten vollständigen Shutdown. C1-/D8-Koordination ohne Session-Mischung.
- **Tests:** C6-Negativmatrix zusätzlich verifizieren: alte Session endet vor neuer, Altevents nach Neustart verboten, gemeinsame Halt-Generation korrekt, Last-/Repeated Resubscribe, kein detached Ingress. 
- **Abnahme:** ein implementierender Fix/Commit mit C6; beide Tickets erst nach demselben nachgewiesenen Test abschließen. **Commit/Evidence:** —.

### D11 — Alignment des Bank-Decoders wird als UNKNOWN_LAYOUT fehlklassifiziert
- [ ] **MEDIUM / VALIDATION ROBUSTNESS · CODE_CONFIRMED · P1B/FE-01 · Cross-Ref B2/B11 (nicht B10).**
- **Pfad:** `liquidation/src/protocol.rs::decode_bank`.
- **Ist:** Schon korrekt geprüfte Länge/Discriminator werden durch `bytemuck::try_from_bytes(&data[8..])` bei unaligned Input trotzdem abgewiesen und als `UNKNOWN_LAYOUT: bank payload alignment` gemeldet. Die interne Speicheradresse ist keine On-Chain-Formatänderung. Ein blindes `pod_read_unaligned` kann jedoch die API (`&Bank`) verändern; sämtliche Aufrufer/Lifetimes/Crate-Parität müssen berücksichtigt werden.
- **Verbindlicher Fix:** Pinned-ABI-korrekter, alignment-unabhängiger, **strict length/discriminator/owner**-Decoder (owned POD-Read, falls möglich), ohne `0.1.12`-Prefix wieder einzuführen. Fehlerklassen für falschen Layouttyp und lokalen Buffer-Alignment-Fall trennen.
- **Tests:** echte pinned Bank-Fixture 8/16-Byte-offset unterschiedlich, 0.1.12 resized, falsche Gesamtgröße/Owner/Disc, Nachkommabits identisch in Health/Sizer/Dependency-Index, Parser-Panicfrei.
- **Abnahme:** gültige Bytes werden trotz unaligned Storage korrekt decodiert; unqualifizierte Versionen weiterhin UNKNOWN_LAYOUT. **Commit/Evidence:** —.

### D12 — Sweep kopiert Fehler-/Erfolgszweige und berechnet Fairness mehrfach
- [ ] **LOW / MAINTAINABILITY + HOTPATH · CODE_CONFIRMED · P6-MT, Cross-Ref D3.**
- **Pfad:** `state_owner/sweep.rs`: `CandidateHealthState::Liquidatable/Error`, `candidate_fairness_key` in `any()` und in `sort_unstable_by_key()`.
- **Ist:** zwei lange nahezu gleiche Update-/Rearm-Pfade, Fairness-Schlüssel mehrfach pro Kandidat. Risiken: spätere auseinanderlaufende Error-/Liquidatable-Logik, vermeidbare CPU. **Nicht** als bestehender falscher Forwardingentscheid nachgewiesen.
- **Verbindlicher Fix:** zunächst Paritätstests über beide Pfade; gemeisame, rein funktionale Berechnung des Fairness-/Scheduling-Key pro Kandidat, danach optional Schrittausführung refaktorieren; kein stilles Zusammenlegen unterschiedlicher Retry-/Fault-Semantik. Vor-/Nach-Messung.
- **Tests:** Healthy/Liquidatable/Error, forced rearms, prior forward slot, revisions, ties, saturation, startup census, identische Reihenfolge/Counts, keine Starvation.
- **Abnahme:** weniger Duplikation/Schlüsselberechnung mit byte-/countergleichem Outcome. **Commit/Evidence:** —.

### D13 — Rayon-Pool-Initialisierung über RAYON_NUM_THREADS möglicherweise zu spät
- [ ] **MEDIUM / REVIEW_REQUIRED · G-A/G-C · P6-MT, Cross-Ref D3.**
- **Pfad:** `runtime/src/orchestration/startup.rs::run` (`RAYON_NUM_THREADS` per Konfiguration), `state_owner/sweep.rs::into_par_iter`, mögliche frühere `rayon::ThreadPoolBuilder`-/Global-Pool-Nutzung.
- **Ist:** `RAYON_NUM_THREADS` wirkt nur, solange der globale Pool nicht bereits von anderem Code initialisiert wurde. Die bloße Initialisierung von `provider::shared()` ist **kein belegter Rayon-Initialisierer**; möglicher Laufzeitkonflikt muss mit gezieltem Test/Instrumentation verifiziert werden.
- **Verbindlicher Fix:** **ein** explizit früh initialisierter Pool oder ein gebundener eigener Pool passend zum bestehenden Owner-/Worker-Vertrag; keine konkurrierenden Pools/unkontrollierte Parallelisierung; Konfig-/FREE_ONLY-/Bounded-RAM-Regeln erhalten.
- **Tests:** Prozessstart mit `workers=1/2/4`, vorherige bewusste Rayon-Initialisierung, tatsächliche Workerzählung und Threadnamen, deterministischer Lastoutput, Parallelitätsgrenze, Fehler bei widersprüchlicher Initialisierung statt stillem Ignorieren.
- **Abnahme:** angeforderte Poolgröße wirksam und messbar, keine unqualifizierte zusätzliche Parallelität. **Commit/Evidence:** —.

**Keine zusätzlichen D-Tickets für als sauber berichtete Grundlagen:** Pin-/Seed-/Discriminator-/Loader-v3-Funktionen in `protocol.rs`, allgemeine Funding-Klassenlogik außer D2 sowie Capture-Window-/Cutoff-/Drain-Grundlogik. Das schließt B2/D11-Decoderprovenienz, D1-Manifest-Wahrheit oder C1/C7-Ingressabschluss **nicht**.


## 9. X — explizite Phasenplan-Vertragslücken; mit N-Tickets koordinieren

### X1 — BOOT-12 muss aktuelle Identitäten vor Runtime-Admission erneut prüfen
- [ ] **CRITICAL · G-B.**
- **Ist:** `NetworkAudit::validate_start` bindet TTL, lokale Config/Binary/Hash/Token; `p0ctl run start` holt **keine** frischen On-chain-Programm-/Oracle-/Genesisbeweise am tatsächlichen Admission-Punkt. Einzelne Runtime-ProgramData-Rechecks sind noch kein vollständiger BOOT-12-Vertrag.
- **Soll:** *vor Admission* validierte aktuelle Genesis/Cluster-, Program-/ProgramData-/Version-, kontextkonsistente Bank-/Oracle-Beziehungen (frisch/identitätsgebunden) gegen den ursprünglichen Token/Spec; TOCTOU führt `BLOCKED`, keine automatische Re-Pin-/Paid-Fallback-Logik. Keine rückwirkende Akzeptanz eines geänderten Programms. Die genaue Reihenfolge mit BOOT-01/02 und Service-Owner-Locks belegen.
- **Tests offline:** geänderte Genesis, ProgramData-Hash, Upgrade-Slot, Oracle/Bank/Config/Binary, abgelaufener Token, schlagartig geänderter Provider. Netzwerkprüfung **NOT_RUN** bis separate Freigabe.
- **Dedupe:** N1/N6/N7/B2; nur **ein** identitätsgebundener Startvertrag. **Commit/Evidence:** —.

### X2 — JFI-01/02-Verstoß im neuen BOOT-/Provider-Evidence-Pfad
- [ ] **HIGH · G-A/G-B.**
- **Ist:** `boot_network.rs` hat `Reads::rpc(...)->serde_json::Value` bis in `cluster` und Scope-Prüflogik, außerdem Produktiv-Import einer JSON-Provider-Evidence-Datei. Das überschreitet die explizite Solana-Wire-Ausnahme.
- **Soll:** RPC-Responses **unmittelbar im Wire-Adapter** auf native, validierte DTOs abbilden; `Reads` mit typisierten `genesis/version/health/slot/multiple_accounts`; historische JSON-Proofimporte ausschließlich isoliert offline. Neuer produktiver Provider-/Network-Audit-Proof als versioniertes typisiertes `.p0j` im **bestehenden** Journalvertrag, ohne neue parallele Evidenz-Datenbank. FREE_ONLY-Provenienz und tatsächlich beobachtete Quoten separat qualifizieren.
- **Tests:** typed Roundtrip, invalid Owner/Layout/Slot, JSON-Leak-Scan außerhalb Wire/Legacy-Leser, Schema/Version/Corruption/Truncation, historischer Reader bleibt byteidentisch.
- **Dedupe:** N1 und PERSIST/JFI-FIX-04–07; keine neue Umsetzungsphase. **Commit/Evidence:** —.

### X3 — BOOT-01/02 nicht nur als Offline-Token qualifizieren
- [ ] **HIGH · G-A/G-B.**
- **Ist:** Offline-Identitäts-/Ownerprüfungen teilweise vorhanden, aber vollständige Version-/Schema-/Writer-Probe-/Crash-Recovery- und Betriebsbereitschaft sind nicht nachgewiesen.
- **Soll:** gemäß `STARTUP_BACKTEST_OPTIMIZATION.md` alle kritischen lokalen Binary-/Decoder-/Journal-/Owner-/Lock-/Clock-/RAM-/Disk-/Writer-Checks mit dokumentiertem Scope, `PASS/WARN/BLOCKED/NOT_RUN`; externe BOOT-Schritte offline zwingend NOT_RUN. Keine implizite READINESS durch Einzelpass.
- **Tests:** korrupte Journalversion/Writer-Probe, Doppel-Owner, fehlende Binärdatei, Disk-/RAM-Untergrenze, Config-Drift; keine echte Netzwerkaktivität.
- **Dedupe:** M1–M3/N1–N3/PERSIST/SK. **Commit/Evidence:** —.

## 10. Reihenfolge und Abhängigkeiten (kein Roadmap-Rewrite)

**Tranche A — laufende Härtung, ausschließlich Offline-Code und Tests:**
1. **C1/C7 + D1** (Recovery/Fatal, Manifest-Wahrheit vor 4h), N1, N2, N3, M3, B5, B13 sowie X1/X2/X3 soweit ohne Mainnet umsetzbar. **D4/D5/D8/D9** als gezielte Capture-Härtung integrieren.
2. **D3-Stresstest vor 4h-Readiness**; bei unzulässiger Owner-Stall/Ingress-Overflow accountseitig gebundenes Chunking implementieren. M1/M2 plus N4/N5/N6 zur Stabilität von Admission, Durability und Diagnose; keine neue Source-of-Truth.
3. N7/N8 nach Mess-/Sicherheitsvertrag, L4 und kleine isolierte L1/L2; B2 soweit für richtige BOOT-/P1A-Scope-/Layoutprüfung erforderlich.
4. Gesamte relevante Offline-Regression, Release-/Formatter-/Clippy-Nachweis, Redaction-Probes, Ledger-/STATUS-/4h-Readiness-Synchronisierung.
5. **Erst dann** getrennt autorisierter G-B-Audit; bei PASS gesondert G-C-Kalibrierung; G-D-vier-Stundenprüfung.

**Tranche B — profitkritische Fertigstellung vor einer eventuell autorisierten FE-06:**
1. B2, B4, B9 und ggf. L4 für die ausgewählte Fundingklasse; echte Candidate-/Source-/Oracle-/Rent-/Exit-Parität (FE-01/02).
2. B6/B7/C2/C8/C9/D2/D6/D7/D12/D13 anhand belastbarer Timingdaten und Kandidaten-Shadow, nicht aufgrund einer geschätzten 4h-Profitrate (FE-03/P6).
3. **B1 + B3 + B15 + C3 + C4 + C5 + C11 + L1 + L2 verpflichtend** für finanzielle Single-Flight-/Wallet-/Limit-/Journal-/UNKNOWN-Gates (FE-04/P8). B10/B11/B12/B17 fachlich vor passendem Pilot prüfen; Ticket-Status bleibt sonst OPEN. C10/C12 werden im jeweiligen P8-/Error-Vertrag geschlossen.
4. FE-05 ausschließlich read-only und nur autorisiert; FE-06 gesonderter individueller Geldbeschluss, ein Versuch dann HALT; FE-07 nur mit finalisiertem tatsächlich realisiertem Nettoledger.

**Tranche C — separat beobachtete, nicht gestrichene Produktpflichten:** vollständiger C0-E1-Index/Crosscheck/Strict-Master, H0, Race/Binary-Geyser/Jito-Single/Bundle/gRPC, JIT-ExactIn, P6-MT/M4, P8-RES/Inventory/Rebalancing, K-Untersuchungen, P9 und Frontend bleiben gemäß §0.6/Phasenplan verbindlich. `NO_ECONOMIC_GO` stoppt nur nicht qualifiziertes LIVE, **nicht** die restliche Roadmap; kein Master-PASS oder erster Profit wird erfunden.

## 11. Vorlage für jeden zukünftigen Befund und die Erledigung

```text
### ID — kurzer reproduzierbarer Befund
- [ ] PRIORITÄT · OPEN / CODE / BLOCKED · Phase/Gate
- Pfad/Funktion und geprüfter Commit:
- Reproduktion und Auswirkung; was ist Fakt, was Hypothese:
- Sicherheits-/FREE_ONLY-/JFI-/PERSIST-Invariante:
- Minimaler Fix in bestehender Architektur:
- Vorher fehlschlagender Regressionstest / Negativfall:
- Nachher bestandene Tests mit Exit-Code:
- Commit-SHA, Evidence-Pfad, getestete Features:
- Verbleibende externe/Geld-/Mainnet-Gates:
```

Nach bestätigtem Abschluss: `- [x]`, ursprüngliche Problembeschreibung durchstreichen, **Fix und Evidence behalten**. Keine Zusammenlegung unabhängiger Bugs ohne belegte gemeinsame Ursache; bei Doppelbelegen auf eine einzige Implementierung/Regression referenzieren.

## 12. Aktuelle Abnahmeentscheidung

- **G-A:** **OPEN**. C1/C7 sowie D1/D4/D5/D8/D9 sind als Recovery-/Capture-/Evidence-Gates erfasst; D3-Lastqualifikation fehlt. H1–H3 und M4/M7 sind lokal reviewt; N1/N3/B5/B13/X1/X2 und andere offene Punkte verhindern einen globalen Härtungs-PASS.
- **G-B (Netzwerk-Audit):** **NOT_RUN / NICHT FREIGEGEBEN**. Das vorhandene BOOT-03–08-Modul ist Code, kein Network-READY; BOOT-12 und JFI offen.
- **G-C (600s-Kalibrierung):** **NOT_RUN / NICHT FREIGEGEBEN**.
- **G-D (4h):** **NOT_QUALIFIED / NICHT FREIGEGEBEN**. D1-Manifest-Fix und D3-Owner-Fanout-Stresstest fehlen. Historische 4h-/5h-Versuche bleiben INCOMPLETE.
- **G-E (FE-01–05):** **PARTIAL/OPEN**, bisheriger FE-00-Befund `NO_ECONOMIC_GO`.
- **G-F (FE-06/07):** **NOT_AUTHORIZED / NOT_RUN**, kein erster realisierter Gewinn belegt.

**Pflegepflicht:** Jeder künftig gefundene Fehler kommt **zuerst mit stabiler ID in diese Datei** und bleibt bis zur nachgewiesenen Abnahme sichtbar; neue Bugs erzeugen keine stillschweigend neue Phase und keine automatische Verschiebung der bindenden FE-First-Profit-Priorität.
