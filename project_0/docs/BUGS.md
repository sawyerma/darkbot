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
| `X` verbindliche Quervertragslücken | — | X1 BOOT-12, X2 JFI-BOOT, X3 BOOT-01/02; verwandt mit N1–N8, keine doppelte Implementierung |
| Weitere Reviewmodule | — | `events.rs`, `sweep.rs`, `detect.rs`, `receivership.rs`, `integration.rs`, `pending.rs`, Market-Observer/`econ.rs`/`chain.rs` und Python-Skripte noch nicht vollständig begutachtet |

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

## 7. X — explizite Phasenplan-Vertragslücken; mit N-Tickets koordinieren

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

## 8. Reihenfolge und Abhängigkeiten (kein Roadmap-Rewrite)

**Tranche A — laufende Härtung, ausschließlich Offline-Code und Tests:**
1. N1, N2, N3, M3, B5, B13 sowie X1/X2/X3 soweit ohne Mainnet umsetzbar.
2. M1/M2 plus N4/N5/N6 zur Stabilität von Admission, Durability und Diagnose; keine neue Source-of-Truth.
3. N7/N8 nach Mess-/Sicherheitsvertrag, L4 und kleine isolierte L1/L2; B2 soweit für richtige BOOT-/P1A-Scope-/Layoutprüfung erforderlich.
4. Gesamte relevante Offline-Regression, Release-/Formatter-/Clippy-Nachweis, Redaction-Probes, Ledger-/STATUS-/4h-Readiness-Synchronisierung.
5. **Erst dann** getrennt autorisierter G-B-Audit; bei PASS gesondert G-C-Kalibrierung; G-D-vier-Stundenprüfung.

**Tranche B — profitkritische Fertigstellung vor einer eventuell autorisierten FE-06:**
1. B2, B4, B9 und ggf. L4 für die ausgewählte Fundingklasse; echte Candidate-/Source-/Oracle-/Rent-/Exit-Parität (FE-01/02).
2. B6/B7 anhand belastbarer Timingdaten und Kandidaten-Shadow, nicht aufgrund einer geschätzten 4h-Profitrate (FE-03/P6).
3. **B1 + B3 + B15 + L1 + L2 verpflichtend** für finanzielle Single-Flight-/Wallet-/Limit-/Journal-/UNKNOWN-Gates (FE-04/P8). B10/B11/B12/B17 fachlich vor passendem Pilot prüfen; Ticket-Status bleibt sonst OPEN.
4. FE-05 ausschließlich read-only und nur autorisiert; FE-06 gesonderter individueller Geldbeschluss, ein Versuch dann HALT; FE-07 nur mit finalisiertem tatsächlich realisiertem Nettoledger.

**Tranche C — separat beobachtete, nicht gestrichene Produktpflichten:** vollständiger C0-E1-Index/Crosscheck/Strict-Master, H0, Race/Binary-Geyser/Jito-Single/Bundle/gRPC, JIT-ExactIn, P6-MT/M4, P8-RES/Inventory/Rebalancing, K-Untersuchungen, P9 und Frontend bleiben gemäß §0.6/Phasenplan verbindlich. `NO_ECONOMIC_GO` stoppt nur nicht qualifiziertes LIVE, **nicht** die restliche Roadmap; kein Master-PASS oder erster Profit wird erfunden.

## 9. Vorlage für jeden zukünftigen Befund und die Erledigung

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

## 10. Aktuelle Abnahmeentscheidung

- **G-A:** **OPEN**. H1–H3 und M4/M7 sind lokal reviewt; N1/N3/B5/B13/X1/X2 und andere offene Punkte verhindern einen globalen Härtungs-PASS.
- **G-B (Netzwerk-Audit):** **NOT_RUN / NICHT FREIGEGEBEN**. Das vorhandene BOOT-03–08-Modul ist Code, kein Network-READY; BOOT-12 und JFI offen.
- **G-C (600s-Kalibrierung):** **NOT_RUN / NICHT FREIGEGEBEN**.
- **G-D (4h):** **NOT_QUALIFIED / NICHT FREIGEGEBEN**. Historische 4h-/5h-Versuche bleiben INCOMPLETE.
- **G-E (FE-01–05):** **PARTIAL/OPEN**, bisheriger FE-00-Befund `NO_ECONOMIC_GO`.
- **G-F (FE-06/07):** **NOT_AUTHORIZED / NOT_RUN**, kein erster realisierter Gewinn belegt.

**Pflegepflicht:** Jeder künftig gefundene Fehler kommt **zuerst mit stabiler ID in diese Datei** und bleibt bis zur nachgewiesenen Abnahme sichtbar; neue Bugs erzeugen keine stillschweigend neue Phase und keine automatische Verschiebung der bindenden FE-First-Profit-Priorität.
