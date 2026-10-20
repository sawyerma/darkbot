# Project_0 — bugs_b.md: vollständiger 114er-Abgleich und Z/Q-Codebefunde

**Erstellt:** 20.10.2026. **Ablage:** `project_0/docs/bugs_b.md` neben der unveränderten `BUGS.md`. **Registerquelle:** `project_0/docs/BUGS.md` auf GitHub `main`, Datei-Blob `ddc1d9d5e3dcd9abb89e9ae9ddf010b6cbafc038`.

> **Geltungsbereich und Wahrheitsstatus:** Dieses Dokument ist ein ergänzender Prüfbericht, **kein Ersatz** für das kanonische `BUGS.md`. Die 114 Statuszeilen unten sind aus dessen erstem Checkbox-Eintrag pro Ticket abgeleitet (81 abgehakt, 33 nicht abgehakt). Ein Haken ist nur ein *dokumentierter lokaler* Abschluss, keine selbst durchgeführte Compiler-, Performance-, Mainnet- oder LIVE-Abnahme. Die Z/Q-Einträge kennzeichnen ausdrücklich direkt bestätigten Code, berichtete Review-Befunde und Hypothesen. Bei voneinander abweichenden Commitständen sind Zeilenangaben erneut zu prüfen. **Keine neuen Branches, Worktrees, LIVE- oder bezahlten Netzwerktests.**

## A. Alle 114 BUGS.md-Tickets — Statusmatrix

**Legende:** `[x]` = in BUGS.md als lokal abgeschlossen dokumentiert; `[ ]` = dort nicht abgeschlossen. `Rest` bezeichnet den zusätzlich erkannten offenen Teil, nicht zwingend einen falschen Originalfix. Kein Status ist ohne neue Tests automatisch `FINAL VERIFIED`.

| ID | Register-Checkbox | Originalthema | Präzisierte Einordnung / offener Rest |
|---|---|---|---|
| H1 | `[x]` | Torn Capture Tail / Segment-Recovery | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| H2 | `[x]` | Secrets in RPC-/Jito-Fehlern | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| H3 | `[x]` | vergifteter Provider-Mutex / Drop-Panic | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| M1 | `[x]` | Betriebssystem-Sampling unter RAM-Budget-Mutex | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| M2 | `[x]` | Owner-Evidence-Durability und blockierendes stderr | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| M3 | `[x]` | Blockhash-Refresh-Thread ignoriert allgemeines Shutdown | Abgehakt; C0-Census-Join als Review-Rest gemeldet |
| M4 | `[x]` | WebSocket ACK und Slot-Progress getrennt | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| M5 | `[ ]` | WSS-Ingress-Allokationen: Profiling statt unbelegter Bug | Nicht abgeschlossen / OPEN oder REVIEW |
| M6 | `[x]` | One-Shot verschleiert sichere Fehlerursachen | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| M7 | `[x]` | Provider-Budget-Bindung an aktive Konfiguration | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| N1 | `[x]` | BOOT-RPC-Budget ist mit Standardprofil unmöglich | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| N2 | `[x]` | Formatter/Clippy/Build der Astra-Änderungen nicht nachgewiesen | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| N3 | `[x]` | rein nullgefülltes Capture-Dateiende nicht als INCOMPLETE erkannt | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| N4 | `[x]` | Drift fälschlich als Mutex-Poison gemeldet | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| N5 | `[x]` | lokale Runtime-Fehler durch globale Redaction unbrauchbar | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| N6 | `[x]` | neuer RPC-Client für jeden BOOT-Read | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| N7 | `[ ]` | 60-Sekunden-Boot-Audit-TTL operativ knapp | Nicht abgeschlossen / OPEN oder REVIEW |
| N8 | `[x]` | WSS-ACK-Deadline ignoriert Subscription-Anzahl | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| L1 | `[x]` | Drop-Reihenfolge Finanzjournal und File-Lock | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| L2 | `[x]` | unsicherer 'sigs[sigs.len()-2]'-Index im Jito-Bundle | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| L3 | `[ ]` | Writer-/Channel-/Submission-Metriken | Nicht abgeschlossen / PARTIAL |
| L4 | `[x]` | falsch konfigurierte Zero-Prefund-Flags fallen auf 'false' zurück | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| B1 | `[ ]` | LIVE-Pipeline umgeht FE-04-Durable-Admission | Nicht abgeschlossen / PARTIAL |
| B2 | `[x]` | Prefix-Decode von MarginfiAccount, Group und FeeState | Abgehakt; Mainnet-Längen/Versionsfixtures fehlen als Zusatznachweis |
| B3 | `[ ]` | flüchtiger Safety-Verlust-/HALT-/Kapitalzustand | Nicht abgeschlossen / OPEN oder REVIEW |
| B4 | `[ ]` | Rent für neue Liquidation-Records ist Unknown | Offen/PARTIAL; Z10 relevant |
| B5 | `[x]` | Premium-BPS über 100 % kann Unterlauf/Panik auslösen | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| B6 | `[x]` | synchrones Fee-/Simulation-/Jupiter-I/O im Owner | Abgehakt, aber Z2: End-to-End-Deadline nicht erfüllt |
| B7 | `[ ]` | wiederholte Oracle-/Observation-Allokationen | Offen/PARTIAL; Z9 relevant |
| B8 | `[ ]` | BelowRoot/Confirmed-Reordering: gemeldeter Fixvorschlag gefährlich | Nicht abgeschlossen / OPEN oder REVIEW |
| B9 | `[x]` | nicht ausführbare V1-Pläne werden generiert | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| B10 | `[x]` | Keypair-Fehlerpfad und Secret-Zeroization | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| B11 | `[x]` | Alignment-abhängige bytemuck-POD-Decoder | Abgehakt; externe Integration-Layouts nicht voll qualifiziert |
| B12 | `[x]` | Liability-Maintenance-Gewicht aus zwei APIs | Abgehakt; Z8 betrifft zusätzliche EMODE-Semantikprüfung |
| B13 | `[x]` | Jupiter-Fehlerbody unbeschränkt/ungefiltert | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| B14 | `[ ]` | statische Pubkey-Parse-/LUT-Clone-Arbeit | Nicht abgeschlossen / OPEN oder REVIEW |
| B15 | `[ ]` | Inventarprüfung berücksichtigt neuen Versuch nicht | Offen/PARTIAL; Z7 relevant |
| B16 | `[x]` | unbekanntes OracleSetup ohne aussagekräftige Dependency-Diagnose | Abgehakt; Z6 wiederholte dependency_gap-Logs |
| B17 | `[x]` | Ingress-Account-Commitment wird als Confirmed gesetzt | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| B18 | `[x]` | doppelte Sign-Slot-Zuweisung / toter Pfadcode | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| C1 | `[x]` | Reconnected während Recovery wird als weiterer Recovery-Auftrag verschluckt; nach Exhausted bleibt ein stiller Dauerhalt | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| C2 | `[ ]` | mehrfacher Health-Aufruf pro Kandidat | Nicht abgeschlossen / OPEN oder REVIEW |
| C3 | `[ ]` | P8-Reconciliation besitzt keinen vollständigen Settlement-/PnL-/Freigabepfad | Offen; Q2 als latenter Safety-Defekt |
| C4 | `[x]` | P8-Landing-Reconcile benutzt confirmed statt finalized | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| C5 | `[x]` | finanzielle Reconciliation konkurriert im Winner-Workload-Budget | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| C6 | `[x]` | WSS-Resubscribe verliert Session-JoinHandle | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| C7 | `[x]` | Ingress Fatal kann den Owner passiv bis Laufzeitende halted lassen | Abgehakt; Fatal-Queue-/Window-Grenzfall nicht belegt |
| C8 | `[ ]` | pro Kandidat kopierte Programm-Allowlisten | Nicht abgeschlossen / PARTIAL |
| C9 | `[x]` | Blockhash-Höchstalter dreifach hart kodiert | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| C10 | `[x]` | ReserveStale wird via Fehlermeldungstext reklassifiziert | Abgehakt; Q5 String-Vertrag und Q13 SHA-Dokumentation bleiben |
| C11 | `[ ]` | Available Inventory in Detection ohne in-flight Holds | Nicht abgeschlossen / OPEN oder REVIEW |
| C12 | `[ ]` | Reconcile-Timeout von 90 s ohne qualifizierte Chain-Abwesenheitsprüfung | Nicht abgeschlossen / PARTIAL |
| C13 | `[ ]` | aktualisierte Review-Abdeckung | Nicht abgeschlossen / OPEN oder REVIEW |
| D1 | `[x]` | geordneter Market-Capture-Abbruch versiegelt Journal fälschlich COMPLETE | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| D2 | `[x]` | NoSwapInventory und PreheldInventory erzeugen doppelte technische Arbeit | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| D3 | `[x]` | Sweep-Fan-out ist accountseitig unbounded und blockiert Owner-Ingress | Abgehakt; Mainnet-Last nicht gemessen |
| D4 | `[x]` | 'on_logs' ruft 'flush' pro TX auf und kann Kompatibilitätssink früh versiegeln | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| D5 | `[x]` | unerreichbar angenommener Receivership-Decoderzweig kann Capture panicken | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| D6 | `[ ]` | MarketStats pro Ingress-Event zweimal geklont und emittiert | Nicht abgeschlossen / OPEN oder REVIEW |
| D7 | `[ ]` | String-basierte Market-Keys und Retention-Cap verursachen unnötigen Speicher-/Coverage-Druck | Nicht abgeschlossen / OPEN oder REVIEW |
| D8 | `[x]` | Market-Capture-Thread wird nur bei bereits abgeschlossenem Lauf gejoint | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| D9 | `[x]` | synchrones stderr auf Capture-Thread kann Ingress-Verarbeitung blockieren | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| D10 | `[x]` | verworfener JoinHandle beim StateOwner-Resubscribe | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| D11 | `[x]` | Alignment des Bank-Decoders wird als UNKNOWN_LAYOUT fehlklassifiziert | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| D12 | `[ ]` | Sweep kopiert Fehler-/Erfolgszweige und berechnet Fairness mehrfach | Nicht abgeschlossen / PARTIAL |
| D13 | `[x]` | Rayon-Pool-Initialisierung über RAYON_NUM_THREADS möglicherweise zu spät | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| E1 | `[x]` | Integration umgeht den strikten 0.1.12-Bank-Layout-Decoder | Abgehakt; Versionsfixtures fehlen |
| E2 | `[ ]` | Planner validiert handgeschriebene Steps, nicht die reale Receivership-TX | Nicht abgeschlossen / OPEN oder REVIEW |
| E3 | `[x]` | Drift-/JupLend-Refresh-Discriminatoren nicht bytegenau getestet | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| E4 | `[x]` | BOOT-02 hat keinen aktiven Journal-Writer-Probe | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| E5 | `[x]` | BOOT-Doctor scannt große alte p0j-Journale vollständig | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| E6 | `[x]` | BOOT-Diskreserve misst Projekt-data statt tatsächlichen Run-Pfad | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| E7 | `[x]` | owner_state nimmt beim reinen Statuslesen kurz den exklusiven Runtime-Lock | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| E8 | `[ ]` | SIGINT/SIGTERM-Handler ohne SA_RESTART: tatsächliche I/O-Auswirkung prüfen | Nicht abgeschlossen / PARTIAL |
| E9 | `[ ]` | Projection-Cache hat keine vollständige Bank-/Oracle-/Revision-Provenienz | Offen/PARTIAL; Z3 und Z4 relevant |
| E10 | `[x]` | ReverseIndex lässt leere bank_to_accounts-Einträge zurück | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| E11 | `[x]` | Kamino-Discriminator wird pro Refresh-Aufruf neu gehasht | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| E12 | `[ ]` | Kamino-Reserve wird pro Kandidat vollständig Borsh-deserialisiert | Nicht abgeschlossen / OPEN oder REVIEW |
| E13 | `[ ]` | fünfter Review ist kein vollständiger Verifier-/Finance-/Tooling-Audit | Nicht abgeschlossen / OPEN oder REVIEW |
| F1 | `[x]` | fehlende begrenzte Simulationslogs | Abgehakt; Q3 betrifft andere Debug-Datenpfade |
| F2 | `[x]` | FE-04-Wallet-Journal schreibt produktiv noch Legacy-Schema-1-JSON | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| F3 | `[x]` | Jito landed_slot=0 als Ersatz für Unknown | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| F4 | `[x]` | WSS-Slotfortschritt vor Queue-Zustellung | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| F5 | `[x]` | RNG-/UNTESTED-Hygiene | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| F6 | `[x]` | Reporter-Commit '311f9a4' bricht 'cargo fmt --check' (Gate G-A) | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| X1 | `[x]` | BOOT-12 muss aktuelle Identitäten vor Runtime-Admission erneut prüfen | Abgehakt; Z10 neue, noch unbewiesene Rent-Sysvar-Hypothese |
| X2 | `[x]` | JFI-01/02-Verstoß im neuen BOOT-/Provider-Evidence-Pfad | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| X3 | `[x]` | BOOT-01/02 nicht nur als Offline-Token qualifizieren | Abgehakt lokal; Netzwerk-Gate separat offen |
| R1 | `[ ]` | Market-Capture-Drain hängt an Quiescence der WSS-Session | Nicht abgeschlossen / OPEN oder REVIEW |
| R2 | `[x]` | Ingress-Panic wird erst nach Capture-Journal-Seal geprüft | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R3 | `[x]` | jeder Recovery-Ready löst im Sweep-Modus erneuten globalen Bank-/Debtor-Sweep aus | Abgehakt; reale Sweep-Rate nicht gemessen |
| R4 | `[x]` | dynamische BOOT-Abhängigkeiten können das rechnerische Mindestbudget überschreiten | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R5 | `[x]` | potentieller Instant-Overflow bei exzessiver Speicher-Wartezeit | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R6 | `[x]` | Evidence-Drop meldet Fehler direkt über blockierendes stderr | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R7 | `[ ]` | Owner-Evidence-Sync läuft periodisch ohne neue Frames | Nicht abgeschlossen / OPEN oder REVIEW |
| R8 | `[ ]` | Headroom-Sampling wiederholt sich auf neuem Acquire; Hysterese ungeprüft | Nicht abgeschlossen / OPEN oder REVIEW |
| R9 | `[x]` | Capture-Flush-Tick ist für JournalSink derzeit No-op | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R10 | `[x]` | angeforderter WSS-Stop darf keinen falschen Econ-Abort auslösen | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R11 | `[x]` | alte Ingress-Generation: Fatal/Gap-Ereignisse am Resubscribe-Grenzpunkt | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R12 | `[ ]` | K1: ECON-Retention kann den gesamten Vierstundenlauf INCOMPLETE machen | Nicht abgeschlossen / OPEN oder REVIEW |
| R13 | `[x]` | K2: Strict-getBlock-Full-Scan und globaler Validierungsfail können 4h-PASS verhindern | Abgehakt nur A4-Teilfix; Durchsatz/Kalibrierung offen |
| R14 | `[x]` | E-A: Winner-Speedup über finalized Log-RX um Finality-Delay verzerrt | Abgehakt; Z5 verlangt separate technische Race-Statistik |
| R15 | `[x]` | E-B: Bootstrap-Probes Live-vs-Offline-Join unterschiedliche Auswahl | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R16 | `[x]` | E-C: kanonischer Kapitalreport ignoriert konfiguriertes Kapitalraster | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R17 | `[x]` | E-D: 'first_observed_slot' in späteren Stundenfenstern kann globalen ersten Slot wiederholen | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R18 | `[ ]` | F-A: Econ4h-Produktivlauf ohne Reporter-StatusTap-Verbindung | Nicht abgeschlossen / OPEN oder REVIEW |
| R19 | `[ ]` | F-B: Transport-'last_frame_age_ns' wird als veralteter Snapshot erneut ausgesendet | Nicht abgeschlossen / PARTIAL |
| R20 | `[ ]` | F-C: Status-/Shadow-Felder fehlen im Wire-Schema; nur 1 von 8 Topics | Nicht abgeschlossen / OPEN oder REVIEW |
| R21 | `[x]` | F-D: Reporter-Heartbeat ignoriert konfigurierte Periode | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R22 | `[x]` | F-E: stale Reporter-Socket/Lock nach Crash verhindert Neustart | Abgehakt; echte Crash-Reproduktion laut Register offen |
| R23 | `[x]` | F-F: Reporter-Diagnose, Single-Client und leere Heartbeat-Watermarks | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R24 | `[ ]` | Market-only ohne Econ: 'seen'-Population über lange Captures nicht explizit begrenzt | Nicht abgeschlossen / PARTIAL |
| R25 | `[x]` | Analytics-Supervisor: ungeordneter Abort verliert letzten Journalabschluss | Ursprünglicher Fix dokumentiert; unabhängige Vollabnahme nicht durchgeführt |
| R26 | `[ ]` | Folgereview: tatsächliche kanonische Market-Wahrheit noch nicht vollständig gegengelesen | Nicht abgeschlossen / OPEN oder REVIEW |

## B. Originaldetails, exakte Ticketbeschreibungen und vorhandene Evidenz aus BUGS.md

Die folgenden Abschnitte bewahren die **bestehende technische Beschreibung und bereits dokumentierte Commit-/Testangaben im Wortlaut**. Das macht den 114er-Abgleich ohne Sprung in die Originaldatei nachvollziehbar. Bei abgehakten Einträgen können die originalen Resttexte weiterhin offene G-Gates beschreiben.

### H1 — Torn Capture Tail / Segment-Recovery

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] ~~Abgeschnittene letzte Capture-Frames machten ein gesamtes Segment unlesbar.~~
- **Pfad:** `runtime/src/analytics/journal.rs`: `Reader::open_capture`, `next_frame`; `runtime/src/storage/segments.rs`: `ChainReader`, `check_no_orphans`.
- **Fix-Stand:** Commit `e42162e4`; nur nicht versiegelte Capture-Tails mit physischem `UnexpectedEof` werden `tail_truncated / complete=false`. Fehlender letzter Nachfolger eines Rotations-Crashes bleibt INCOMPLETE, Kettenbruch/orphan ist kein PASS. `Reader::open` für Wallet sowie `Writer::resume` bleiben strikt.
- **Beleg:** Fable bestätigte vorhandene Tests zu jeder Abschneideposition; zielgerichteter Patch-Review PASS. **Offener Anschluss:** N3 (Null-Fill-Tail), reale Crash-/Last-Evidence unter PERSIST/SK-07.
- **Regression:** jede Frame-Abschneideposition, Manifest, Rotation, orphan, CRC/Hash, strikter Walletreader. **Gate:** G-A lokal; G-D braucht reale Evidence.
- **Ticketabschluss:** `LOCAL_REVIEW_PASS`; Code-SHA `e42162e4`. Cross-Ref: N3.


### H2 — Secrets in RPC-/Jito-Fehlern

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] ~~Provider-Fehlermeldungen konnten URL/API-Key/Response-Payload in Logs und Fehlertexte durchreichen.~~
- **Pfad:** `solana/src/submit/rpc.rs`, `solana/src/submit/jito.rs`, `runtime/src/main.rs` / `orchestration/startup.rs`.
- **Fix-Stand:** `safe_rpc_error`/`safe_http_error`/`safe_remote_error`; konservative `Rejected`-Klasse, unsichere Annahme `Transport/UNKNOWN`; Main-Ausgabe kategorisiert. Fable meldete Secret-/Host-Negativtests PASS.
- **Nachlauf:** N5/B13/B10, Jito-/Simulationsdiagnosen dürfen diese Grenze nicht unterlaufen; redigierter Fehlertext ist nicht automatisch Beweis für sicheren Roh-Response.
- **Ticketabschluss:** `LOCAL_REVIEW_PASS`; Code-SHA `e42162e4`.


### H3 — vergifteter Provider-Mutex / Drop-Panic

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] ~~Poisoned Mutex und Permit-Drops konnten erneut panicken oder Budgetzustände inkonsistent lassen.~~
- **Pfad:** `solana/src/provider.rs`.
- **Fix-Stand:** `lock_state` übernimmt PoisonError-Inneres und setzt `failed`; `checked_sub`-Drops; Condvar-Poison und Unwind mit gehaltenen Permits getestet. Fail-closed bleibt.
- **Nachlauf:** N4 korrekter Drift-Code, Gesamtregression.
- **Ticketabschluss:** `LOCAL_REVIEW_PASS`; Code-SHA `e42162e4`.

## 3. M — mittlere Ausgangsbefunde


### M1 — Betriebssystem-Sampling unter RAM-Budget-Mutex

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOCAL_PASS · G-A/G-D · PERSIST-02/SK-03.**
- **Pfad:** `runtime/src/storage/budget.rs`: `MemoryBudget::acquire` → `headroom_allows` → `MemorySample::now`.
- **Ist:** Während `state.lock()` gehalten wird, können `sysctl`/`task_info`/`getrusage`/`/proc`-Abfragen erfolgen; Condvar-Wakeups können das wiederholen.
- **Soll-Fix:** Entscheidungswerte ohne Mutex sampeln; Guard abgeben, Sample holen, Guard erneut nehmen, aktuelle `used/limit/max`-Invarianten erneut prüfen; monotones, begrenztes Samplingintervall; `UNKNOWN`-Headroom erlaubt kein Wachstum. Keine race-bedingte Überreservierung.
- **Tests:** unabhängige Headroom-Tabelle (Memory-Pressure, unbekannte Werte, Grenzwerte), künstliche Concurrent-Acquires mit getriggertem Limitwechsel, Lock-/Deadlock-Probe, Bounded-Wait und Byte-Grenze. Messung: Sampling-/Lockdauer vor/nach.
- **Abnahme:** keine langsamen Syscalls unter `state`-Lock; deterministische Limits unverändert. **Commit/Evidence:** `325fa21` (OS-Sample ohne `state`-Lock, max. 1× pro 100 ms je Wartevorgang, Invarianten danach erneut geprüft; `headroom_allows` rein); Tests `headroom_decision_is_pure`, `acquire_samples_memory_at_most_once_per_interval` Exit 0.


### M2 — Owner-Evidence-Durability und blockierendes stderr

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOCAL_PASS · G-A/G-D · PERSIST-02/05/SK-04.**
- **Pfad:** `runtime/src/analytics/evidence.rs`: `Sink::store`, `Sink::write`, `run_writer`, `OwnerEvidenceWriter::finish`.
- **Ist:** Owner-Evidence wird regulär beim `seal` synchronisiert, nicht nach belegtem periodischem Intervall; synchrones `eprintln!` auf dem Writer kann bei stderr-Backpressure den Writer verzögern. **Achtung:** separater MarketJournalWorker in `storage/writer.rs` hat bereits einen Sync-Intervallvertrag; hier nicht doppelt implementieren.
- **Soll-Fix:** bounded, fehlersicherer periodischer Sync für Owner-Evidence mit festgelegter und dokumentierter Verlustobergrenze. Diagnostikausgabe entkoppeln, aber niemals durch `join` eines blockierten Konsolenthreads einen Shutdown-Deadlock erzeugen. Journal-Evidence darf nicht still verlorengehen; eine volle Journal-Queue bleibt korrekt fail-closed. Diagnostikverluste gesondert zählen.
- **Tests:** Sync-Deadline auch ohne neue Events, injizierter Sync-Fehler führt sticky HALT, blockiertes Terminal verursacht weder Writer-/Shutdown-Deadlock noch unbegrenzte Queue, EOF/Shutdown/Seal/Evidence-Recovery. Nicht behaupten, ein hart blockierter OS-stderr könne immer sofort beendet werden, ohne die tatsächliche Architektur zu beweisen.
- **Abnahme:** beweisbare Durability-Grenze, keine stille Evidence-Drops, terminierbarer Abschluss. **Commit/Evidence:** `d41b70f` (periodischer Sync `owner_evidence_sync_ms`, Sync-Fehler sticky; Diagnose-Kopien über `console::line`, kein Join eines Konsolen-Threads; `LIMITS_REVISION` 3); Tests `owner_evidence_syncs_periodically`, `owner_evidence_sync_failure_is_sticky` Exit 0.


### M3 — Blockhash-Refresh-Thread ignoriert allgemeines Shutdown

**Abgleich:** Abgehakt; C0-Census-Join als Review-Rest gemeldet.

- [x] **LOCAL_PASS · G-A/G-C · S0-Lifecycle/P6.**
- **Pfad:** `runtime/src/orchestration/state_owner/setup.rs`: `thread::Builder::new().name("blockhash")`; Owner-/Shutdown-Struktur `state_owner.rs` und `state_owner/shutdown.rs`.
- **Ist:** Schleife prüft nur `bh_econ.admission_closed()`, schläft 10 s und verwirft den JoinHandle. Ohne Econ-Handle läuft sie bis Prozessende weiter und kann weitere RPC-Reads anstoßen.
- **Soll-Fix:** Shutdown + AdmissionClosed vor jedem Refresh und während der Sleep-Phase prüfen, Handle eindeutig im Owner halten und am geordneten Shutdown joinen. Netzwerk-RPC darf einen Shutdown nicht unbegrenzt überziehen; per-call-Timeout und Joindeadline als reale Grenzen dokumentieren.
- **Tests:** Shutdown ohne Econ-Handle, AdmissionClosed, Request-in-flight + Timeout, wiederholtes Start/Stop ohne verwaiste Threads/RPC nach Stop; negative Cancellation.
- **Abnahme:** keine Refresh-Reads nach Shutdown, kein detached Owner-Thread. **Commit/Evidence:** `dc175d2` (`run_blockhash_refresh`, Join in `finish`); Tests `blockhash_refresh_stops_on_shutdown_without_econ`, `…_stops_on_admission_closed`, `…_in_flight_finishes_then_stops` Exit 0. Rest außerhalb M3: `ingress-ws` wird seit `ad39019` (C6/D10) in jedem Modus gestoppt und begrenzt gejoint; `c0-census` wird per Shutdown beendet, aber nicht gejoint; Remote-Worker nur mit Econ/Window gejoint.


### M4 — WebSocket ACK und Slot-Progress getrennt

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] ~~Ein einziger Timer vermischte ACK und fehlenden Slotfortschritt.~~
- **Pfad:** `solana/src/ingress/ws_rpc.rs`.
- **Fix:** `WSS_ACK_TIMEOUT` eigener Zustand; Slotfortschritt beginnt erst bei `StreamReady`. Fable bestätigte eingecheckten Patch `e42162e4`.
- **Offener Anschluss:** N8 tatsächliches ACK-Budget/Subscription-Mengen; reale Liveness. **Status:** `LOCAL_REVIEW_PASS`.


### M5 — WSS-Ingress-Allokationen: Profiling statt unbelegter Bug

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / PROFILE_REQUIRED · kein Funktionsbug ohne Messung · S1/P6-MT.**
- **Ist:** `ws_rpc.rs` verwendet `Message::Text(t) => t.to_string()`, `serde_json::Value` und Base64-Puffer. Dies ist messbare Allokationsarbeit, nicht bereits ein nachgewiesener Overflow.
- **Soll/Tests:** Repräsentative Frame-Last, p50/p95/p99 und Bytes/Allocations erfassen. Nur bei belegtem Nutzen eine JFI-kompatible Parsingoptimierung, gleiche Slot-/Dedupe-/Coverage-Semantik; keine stillen Parseränderungen. **Commit/Evidence:** —.


### M6 — One-Shot verschleiert sichere Fehlerursachen

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH / DIAGNOSTICS · FE-04/06 · CODE_CONFIRMED · Cross-Ref H2/N5/F1.** · Fix `07c3f09` · Tests `m6_journal_failures_carry_typed_secret_free_causes`, `damaged_truncated_foreign_or_illegal_binary_journal_is_a_halt` (exakte Codes) — typisierter `journal::Code` als Fehlerquelle; One-Shot-Fehler `<CODE>:<Klasse>` (FRAME_CRC, FRAME_TRUNCATED, TAIL_ZERO_FILLED, HEADER_TRUNCATED, SCHEMA_ORDER, PERMISSION, NO_SPACE, …), nie Pfad/Text; Submission `SUBMISSION_RESULT_UNKNOWN:REJECTED/TRANSPORT_AMBIGUOUS/SERIALIZE`, Hold bleibt. Provider-Remote-Fehler behalten den JSON-RPC-Code (`RPC_RESPONSE code=`).
- **Ist:** `one_shot.rs` setzt vielfach IO-/Decode-/Replay-Fehler auf `JOURNAL_CORRUPT`/`JOURNAL_IO`; `one_shot/transport.rs::submit_payload` reduziert `Rejected/Transport/Serialize` auf `SUBMISSION_RESULT_UNKNOWN`; `provider.rs` klassifiziert Remote-Fehler pauschal `RPC_REMOTE_ERROR`. N5 betrifft stattdessen Main-/Startup-Ausgabe.
- **Soll:** Typisierte, begrenzte, secretfreie Ursachenklassen (IO_PERMISSION/NO_SPACE, CRC/SCHEMA, REMOTE_REJECTED vs TRANSPORT_AMBIGUOUS etc.), ausschließlich in kontrollierter Evidence, keine URLs, Keys, Bodies oder freie Providertexte in stderr/Display. **Jedes zweifelhafte Handoff bleibt UNKNOWN/HALT; Holds bleiben.** Keine neue Submissionfreigabe oder Retry.
- **Tests:** EACCES/ENOSPC, CRC, unsupported schema, Syncfail, 429/5xx, Reject, Transport-Race, malformed ACK und Secret-Sentinels; identische Replay-/Double-Send-Sperren. **Commit/Evidence:** —.


### M7 — Provider-Budget-Bindung an aktive Konfiguration

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] ~~Früh angelegtes Shared-Budget konnte später abweichende aktive Limits unbemerkt nutzen.~~
- **Pfad:** `solana/src/provider.rs`.
- **Fix:** `shared()`/Budget-Konfigurationskontrolle bindet gegen `active()`, Drift führt fail-closed. Fable bestätigte Patch `e42162e4`.
- **Offener Anschluss:** N4 eindeutige Fehlerursache; keine automatische Reinitialisierung. **Status:** `LOCAL_REVIEW_PASS`.

## 4. N — neue Befunde aus der Astra-Patchprüfung


### N1 — BOOT-RPC-Budget ist mit Standardprofil unmöglich

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **CRITICAL · LOCAL_PASS · G-A/G-B · BOOT-03/04.**
- **Pfad:** `runtime/src/orchestration/boot_network.rs`: `NetworkReads::new/rpc/cluster`; `solana/src/runtime_config.rs`.
- **Reproduktion:** `fe_mode=smoke` setzt `fe_calls=3`, `fe_seconds=30`. `NetworkReads` verwendet `limit=fe_calls.min(max_requests)`; `cluster` braucht bereits fünf RPC-Reads. Vierter Call ergibt `BOOT_RPC_BUDGET`. Existing Fixtures umgehen das echte Budgetobjekt.
- **Soll-Fix:** Eigenes *explizites* BOOT-Call- und Zeitbudget, getrennt von FE-01-Limits und gebunden an globales Providerbudget und FREE_ONLY. Verbindliches Minimum aus tatsächlichen Cluster-, Programm-/ProgramData-, Scope-Batch- und Oracle-Aufrufen **berechnen, nicht bloß `5+fe_snapshot_rounds` behaupten**. Bei unzureichendem Budget vor Netzwerkzugriff `BOOT_BUDGET_TOO_SMALL`; keine heimliche Quoten-/RPS-Erhöhung.
- **Tests:** Default-Profil plus berechnete Pflicht-Reads, `NetworkReads` mit lokalem HTTP-Stub (nicht nur `Fixture`), Boundary `minimum-1/minimum`, zusätzliche dynamische Scope-Runde, MaxRequests und Deadline, fe_calls-Unabhängigkeit. **Keine echten RPCs.**
- **Abnahme:** Defaults scheitern nicht zwangsläufig bereits beim vierten Cluster-Read; real nicht hinreichendes Budget blockiert **vor** Start. **Commit/Evidence:** `342ab2b` (`boot_calls`/`boot_seconds`, `required_reads` aus Cluster-/Programm-/Scope-Pfaden, `BOOT_BUDGET_TOO_SMALL` vor Netzwerkzugriff; `LIMITS_REVISION` 2 → Payload-Schema 3 für `OPERATIONAL_CONFIG`/`RUN_MANIFEST`); Tests `network_reads_refuses_budget_below_minimum`, `network_reads_counts_only_boot_budget` (lokaler HTTP-Stub), `required_scope_reads_match_the_collector`, `limits_layout_is_pinned_to_its_revision` Exit 0.


### N2 — Formatter/Clippy/Build der Astra-Änderungen nicht nachgewiesen

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH · LOCAL_PASS · G-A.**
- **Pfad:** `runtime/src/orchestration/fe_qualification.rs::collect` (finales `return Ok(...)`), `boot_network.rs::run` (Format), gesamter geänderter Workspace.
- **Soll-Fix:** `cargo fmt` auf tatsächlicher Workspace-Wurzel, finales unnötiges `return` ggf. entfernen; anschließend Clippy mit aktivem Feature-Scope und `-D warnings`, Testmatrix/Release-Build. Keine globalen `#[allow]` zur Kosmetik und keine fremden Module massenhaft formatieren.
- **Tests:** tatsächliche Tool-Ausgaben und Exit-Codes, beide produktiven Featureprofile; Fehler separat mit Dateien/SHA dokumentieren.
- **Abnahme:** formatiert, Clippy/test/release erfolgreich auf exakt getesteter SHA. **Commit/Evidence:** `9ce7aa0` (Clippy `-D warnings` workspaceweit, verhaltensgleich; abgelehnter cmp_owned-Autofix im Strict-Cache), `2c9957c` (rustfmt), `e55a5c7` (Export eines abgeschnittenen Journals wieder fail-closed, H1-Regression). Nachweise `evidence/HARDENING_20261018/after_N2_{fmt_check,clippy,test,build_release}.txt` auf `512bb13`, alle Exit 0. Gate auf `d5128ac` (nach N1–N8, M1–M3, D1/D4/D5/D8/D9, B5, B13, C1-Teil, C7, L1/L2/L4, X2): `evidence/HARDENING_20261018/gate_{fmt_check,clippy,test,build_release}.txt` alle Exit 0; Python-Suite 108 (1 übersprungen) nach Entfernen einer veralteten `e42162e`-Assertion grün. **Gate 2 auf `8eccd8f`** (nach C6/D10, E4–E6, D3, B2/E1/D11/B11, E3/E11; sauberer Worktree, `CARGO_BUILD_JOBS=2`): `evidence/HARDENING_20261018/gate2_{clippy,test,build_release}.txt` Exit 0 (`clippy --all-targets --all-features -D warnings`; `test --all-features` 541 ok/0 failed, Runner-Lib 342/5 ignored, Liquidation 72); `gate2_fmt_check.txt` **Exit 1 ausschließlich** wegen der drei fremden Reporter-Dateien aus `311f9a4` (F6) — alle Härtungsdateien formatiert. Python-Suite in Gate 2 nicht erneut gelaufen (keine Python-Änderung). **Gate 3 auf `0de1b9b`** (nach R10/R11/R6/R1-Teil, R2, R3, C1-Test 7, R4, R5, X3, R9, F5; sauberer Worktree, `CARGO_BUILD_JOBS=2`): `gate3_clippy.txt` (`--all-targets --all-features -D warnings`) Exit 0; `gate3_test.txt` (`test --all-features`) Exit 0, **560 bestanden / 0 fehlgeschlagen / 6 ignoriert**; `gate3_build_release.txt` Exit 0; `gate3_python.txt` 108 Tests OK (1 übersprungen) Exit 0; `gate3_fmt_check.txt` **Exit 1 ausschließlich** wegen der vier fremden Reporter-Dateien (F6: `p0_market_capture_reported.rs`, `p0_reporter_fixture.rs`, `env_file.rs`, `tests/config.rs`) — nicht grün, nicht von diesem Auftrag geändert. **Gate 4 auf `c8d2e33`** (nach F6 `3b2e8c0` und X1 `a3ca33c`; sauberer Worktree): `gate4_{fmt_check,clippy,test,build_release}.txt` Exit 0 (`fmt --check` erstmals grün; `test --all-features` 565 bestanden / 0 fehlgeschlagen / 6 ignoriert), **`gate4_python.txt` Exit 1** (4 Errors in den Launcher-E2E-Tests `test_launcher_uses_persisted_plan_and_no_total_capture_timebox`, `test_launcher_end_to_end_real_binary_writes_layout_v1_and_v2`: X1 verweigerte den Start ohne freigegebenen Record schon vor dem lokalen Manifest) ⇒ Folgefix `1d9c6ed` (siehe X1). **Gate 4 Wiederholung auf `1d9c6ed`** (sauberer Worktree, `CARGO_BUILD_JOBS=2`): `gate4r_fmt_check.txt`, `gate4r_clippy.txt`, `gate4r_test.txt` (**565 / 0 / 6**), `gate4r_build_release.txt` und `gate4r_python.txt` (108 OK, 1 übersprungen) **alle Exit 0** — erste vollständig grüne lokale Workspace-Abnahme dieses Auftrags. Keine Netzwerkqualifikation (G-B), keine LIVE-Freigabe.
- **Triage 18.10.2026:** Nur das `return Ok(json!(..))` am Ende von `fe_qualification::collect` ist source-belegt. `needless_return` könnte Clippy `-D warnings` auslösen, **keine** Toolausgabe liegt vor. Vermuteter `rustfmt`-Fehler im `boot_network`-Makro/Einzeiler zurückgenommen bis `cargo fmt --check`.


### N3 — rein nullgefülltes Capture-Dateiende nicht als INCOMPLETE erkannt

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH · LOCAL_PASS · G-A/G-D · PERSIST-05/SK-07.**
- **Pfad:** `runtime/src/analytics/journal.rs::read_frame`, `Reader::next_frame`, `storage/segments.rs`, FE-04-Finanzjournal.
- **Ist:** Vier Nullbytes als Frame-Länge führen zu `FRAME_LENGTH`, nicht zu toleriertem `UnexpectedEof`.
- **Soll-Fix:** Ausschließlich **vollständig nachweisbare Null-Suffix-Bytes an einer erlaubten letzten, unversiegelten Capture-Framegrenze** als Torn Tail klassifizieren, Status `INCOMPLETE`. Keine Nullstrecke innerhalb eines korrupten Frames, kein `DATA_AFTER_MANIFEST` oder fremdes/sealed Segment erlauben. Suffix-Prüfung bounded/streaming, ohne O(file_size)-RAM.
- **Tests:** 1/3/4/5/4096/70000 Nullbytes nach gültigen Frames, Fremdbyte am Ende, Nullen mitten im Frame, nach Seal, Segmentrotation, `Reader::open`/`Writer::resume`/`one_shot::Journal` weiterhin strikt; Bytes bleiben unverändert.
- **Abnahme:** nur echter zulässiger Capture-Tail `INCOMPLETE`; beschädigte finanzielle Evidence weiter HALT. **Commit/Evidence:** `c5e6543` (`read_frame`: Länge 0 nur bei Nullen bis EOF → `UnexpectedEof`/INCOMPLETE, 64-KiB-Blöcke); Tests `capture_zero_filled_tail_is_incomplete_but_strict_readers_refuse`, `zero_filled_tail_of_the_last_segment_is_incomplete_but_not_of_an_intermediate`, `zero_bytes_inside_the_stream_stay_hard_errors`, `wallet_journal_with_zero_tail_is_a_halt` Exit 0.

- **Nachprüfung 18.10.2026:** Claude hat bereits `c5e6543` und dokumentierte RED/GREEN-Testausgaben eingecheckt; das Ticket ist inzwischen von Claude als `LOCAL_PASS` abgehakt. Green: 55 Tests bestanden, 1 ignoriert. Das ist **keine** vollständige Workspace-/4h-Abnahme; vorhandenen Haken und Claudes Evidence nicht überschreiben.


### N4 — Drift fälschlich als Mutex-Poison gemeldet

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · G-A.**
- **Pfad:** `solana/src/provider.rs::check_configuration/acquire`.
- **Ist:** `failed=true` nach Config-Drift, Fehlercode `PROVIDER_BUDGET_POISONED` suggeriert Unwind.
- **Soll-Fix:** getrennte, sticky Diagnose `PROVIDER_BUDGET_CONFIG_DRIFT`; Endpunkte nur redigiert/als sichere Feldbezeichner, niemals vollständige URL; Poison-Fehler unverändert fail-closed.
- **Tests:** Drift vs Poison differenziert, einmalige protokollierte Feldklasse, keine Secrets, keine Reinitialisierung.
- **Abnahme:** eindeutiger Code ohne Abschwächung des Budget-HALTs. **Commit/Evidence:** `e322869` (eigener sticky Zustand `PROVIDER_BUDGET_CONFIG_DRIFT`, einmalige Feldmeldung, Endpunkte nur als Anzahl, Poison unverändert); Test `early_default_budget_cannot_authorize_a_later_profile` Exit 0.


### N5 — lokale Runtime-Fehler durch globale Redaction unbrauchbar

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · G-A.**
- **Pfad:** `runtime/src/orchestration/startup.rs::shadow_failure_code`; `runtime/src/main.rs`; sichere Config-/Walletfehler.
- **Ist:** Unklassifizierte Fehler werden pauschal `SHADOW_RUNTIME_ERROR`, dadurch fehlen diagnostische Hinweise.
- **Soll-Fix:** ausschließlich explizit allowgelistete eigene, secret-freie Fehlercodes und begrenzte strukturierte Detailfelder. **Kein beliebiges `Display` von unbekannten Error-Typen**; keine Provider-URL, API-Keys, Keypair-Pfade, HTTP-Body oder rohe Logs.
- **Tests:** Missing Env/Mode identifizierbar; ClientError mit geheimem URL-Parameter redigiert; Wrapped/Custom-Fehler mit Secret zeigt nur Kategorie.
- **Abnahme:** nützliche, secret-freie Betriebsdiagnose. **Commit/Evidence:** `c83bd46` (`failure_report`: nur klassifizierte Details — Config-Schlüssel ohne Wert, `OperationalConfigError`, Owner-/Evidence-Codes; sonst Kategorie); Test `failure_report_shows_classified_details_only` Exit 0.


### N6 — neuer RPC-Client für jeden BOOT-Read

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · G-B.**
- **Pfad:** `runtime/src/orchestration/boot_network.rs::NetworkReads::rpc`.
- **Ist:** `provider::rpc_client(...)` pro Request; vermeidbare neue Clients/Transport-Runtimes. Die genaue SDK-Runtime-Zahl ist ohne Laufzeitmessung **nicht bewiesen**.
- **Soll-Fix:** Einmaliger Client im `NetworkReads`-Owner, gleiche Provider-Awareness; vor jedem Call absolute Gesamtdeadline prüfen. Kein Client-Caching über veränderte Config-/Clusteridentitäten.
- **Tests:** lokal instrumentierter Client-Factory-Zähler, Budget-/Timeout-Tests, keine erneute Konstruktion bei Wiederholung.
- **Abnahme:** ein Client pro Audit-Instanz, globale Zeitgrenze bleibt hart. **Commit/Evidence:** `e322869` (ein RPC-Client je Audit; Per-Call-Timeout min(rpc_timeout, Budget/4, ≥ 1 s), Call nur bei ausreichender Restzeit → harte Deadline); Test `network_reads_counts_only_boot_budget` Exit 0.


### N7 — 60-Sekunden-Boot-Audit-TTL operativ knapp

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **MEDIUM · OPEN / DESIGN_DECISION · G-B.**
- **Pfad:** `runtime/src/orchestration/boot_audit.rs::validate_start`, `boot_network.rs::Spec`, `bin/p0ctl.rs`.
- **Ist:** Doctor und Start erfordern manuelles Tokenhandling in höchstens 60 s; Verspätung blockiert korrekt.
- **Soll-Fix:** sichere kombinierte `doctor + run start`-Bedienung erwägen, **ohne** TTL/Safety still auf 300 s zu erhöhen. Jede etwaige Änderung der TTL bedarf gesonderter Freigabe; BOOT-12 muss ohnehin aktuelle Identitäten prüfen.
- **Tests:** Token gültig/abgelaufen, Konfig-/Binär-/Programmdrift und doppelte Startversuche.
- **Abnahme:** bedienbarer Workflow **ohne** längeres unsicheres Vertrauen in einen alten Token. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `DESIGN/USABILITY`, kein fehlerhafter TTL-Safety-Check. Keine TTL-Aufweitung als vermeintlichen Bugfix autorisieren; kombinierte Doctor-Start-Bedienung nur separat testen.


### N8 — WSS-ACK-Deadline ignoriert Subscription-Anzahl

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · G-C/G-D.**
- **Pfad:** `solana/src/ingress/ws_rpc.rs`: `ack_deadline = now + idle_timeout`.
- **Ist:** ACK-Timeout meldet seit M4 korrekt `WSS_ACK_TIMEOUT`, Budget bleibt unabhängig von tatsächlichen Send-/ACK-Zahlen.
- **Soll-Fix:** ACK-Dauer/-Zahl/-Lag instrumentieren; dynamische aber **harte, konfigurierte** obere Budgetgrenze nach tatsächlichem Subscriptionbedarf prüfen. Keine automatische unbeschränkte Verlängerung, keine Umgehung von Recoverybudget/WSS-Rate.
- **Tests:** kleine/große Subscriptionmenge, einzelne fehlende ACK, 429/Timeout, Shutdown während ACK; reale Profile erst nach Erlaubnis.
- **Abnahme:** falsche Reconnects reduziert, ausbleibende ACK führt weiterhin bounded HALT. **Commit/Evidence:** `4dc5de2` (ACK-Fenster = idle_timeout + Subscriptions × `wss_ack_ms_per_subscription`, gekappt an der Recovery-Deadline; Messzeile bei StreamReady; `LIMITS_REVISION` 4); Test `ack_window_scales_with_subscriptions_within_recovery_budget` Exit 0. Reale Profile NOT_RUN.

## 5. L — kleine Restbefunde aus dem ersten Review


### L1 — Drop-Reihenfolge Finanzjournal und File-Lock

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM vor FE-04 · LOCAL_PASS.**
- **Pfad:** `runtime/src/execution/one_shot.rs::Journal`.
- **Soll:** Ownership/Drop-Reihenfolge explizit so beweisen, dass Journal/Writer vollständig geschlossen und durable ist, bevor die zugehörige Lock-Freigabe andere Owner zulässt. Keine Lösung allein über Kommentar.
- **Tests:** konkurrierender Open während Drop/Finish, Crash-/Sync-Fehler, Lock bleibt bis korrekt abgeschlossen. **Commit/Evidence:** `5887129` (Lock letztes Feld: Writer schließt vor Lock-Freigabe); Test `next_owner_never_opens_a_journal_still_held_by_its_writer` Exit 0.


### L2 — unsicherer `sigs[sigs.len()-2]`-Index im Jito-Bundle

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH vor FE-06 · LOCAL_PASS.**
- **Pfad:** `solana/src/submit/jito.rs::submit_bundle`.
- **Soll:** Signature-/Bundle-Vollständigkeit und erwartete Reihenfolge **vor jedem Netzwerk-Handoff** prüfen, keine Annahme von mindestens zwei Signaturen; leere/unvollständige Bundles lokal ablehnen.
- **Tests:** 0/1/2/mehrere TX, fehlende Signature, nicht signierte TX; kein HTTP-Aufruf bei Ablehnung; UNKNOWN nach unklarer Annahme. **Commit/Evidence:** `47ebbc6` (alle Fee-Payer-Signaturen vor dem HTTP-Aufruf geprüft, `BUNDLE_SIGNATURE_MISSING`; inflight Landed ohne Slot → Pending); Test `unsigned_bundle_is_rejected_without_network` Exit 0.


### L3 — Writer-/Channel-/Submission-Metriken

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / METRICS · CODE_CONFIRMED für Writer/Channel; Polling separat belegen.** · PARTIAL `9fad87d`+`07c3f09` · Test `l3_failed_attempts_and_dropped_elements_are_separate` — Channel: `overflow` = Fehlversuche (unverändert), `dropped` via `give_up` an allen Full-Stellen (Ingress inkl. Gap/Fatal/Transport/Health, Projection-, Oracle-Queue), Ausgabe `channel_dropped=` (Debug-Layout für R126/R127-Parser unverändert). OFFEN: Writer `deferred_intervals` braucht neues Feld in `StorageMetrics` ⇒ Schema-Evolution `MARKET_SUMMARY` (Bedeutungsänderung von `deferred_syncs` aus `9fad87d` zurückgenommen); `dropped` nicht im persistierten Shutdown-Summary.
- **Ist:** `storage/writer.rs::deferred_syncs` wird während eines Deferrals in Worker-Schleifen erhöht; `common/channel.rs::Producer::try_push` zählt Full bei jedem Retry, nicht unterschiedliche verlorene Events. 1-ms-Submissionpolling aus Review noch am tatsächlichen Aufrufpfad nachzuweisen.
- **Soll/Tests:** `failed_push_attempts`, tatsächliche `unique_dropped`, `deferred_intervals` voneinander trennen, vorhandene Zähler nicht heimlich semantisch ändern. Deterministische Retry-/Backpressure-/Shutdown-Tests, Polling nur bei reproduzierter Last anpassen. **Commit/Evidence:** —.


### L4 — falsch konfigurierte Zero-Prefund-Flags fallen auf `false` zurück

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · FE-01/P5.**
- **Pfad:** `runtime/src/orchestration/state_owner/setup.rs`: `P0_ZERO_PREFUND_QUALIFIED`, `P0_ZERO_PREFUND_SHADOW_PROBE`.
- **Soll:** Parsefehler explizit als Config-Fehler blockieren, statt `unwrap_or(false)`; korrektes Defaultverhalten nur bei **fehlendem**, nicht malformed Wert.
- **Tests:** missing/true/false/ungültig, kein implizites Einschalten von LIVE. **Commit/Evidence:** `c83bd46` (Slippage aus `limits.max_slippage_bps`, Zero-Prefund-Flags strikt vorab geparst, Probe nur im Shadow-Modus); Test `zero_prefund_flags_parse_strictly_and_never_enable_implicitly` Exit 0.

## 6. B — finanziell relevante zweite Reviewrunde


### B1 — LIVE-Pipeline umgeht FE-04-Durable-Admission

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **CRITICAL · FE-06/P8 BLOCKER · G-E/G-F.** · PARTIAL `e08d3a7`+`07c3f09` · Test `b1_live_pipeline_never_sends_and_reserves_nothing` — erreicht: `Pipeline` hat keinen Send-Aufruf, LIVE endet mit `Decision::Unavailable`/`LIVE_SEND_REQUIRES_FE04_ONE_SHOT` ohne Reservierung. OFFEN (Pflicht): echte LIVE→FE-04-Bindung — typisierte Approval/Intent-Übergabe an den One-Shot-Owner (heute ohne Aufrufer/CLI), `submit` nur mit durable Reservation-Token, Crash-/Double-Send-/UNKNOWN-Tests; abhängig von B3/B4/C3.
- **Pfad:** `runtime/src/execution/pipeline.rs::evaluate`, `execution/one_shot.rs`, `execution/pending.rs` und StateOwner.
- **Ist:** `Mode::Live` verwendet `safety.check` (RAM) und `submitter.submit/submit_bundle` direkt. `one_shot::Journal`, `Approval` und `SignedPayload` sind nicht an diesen Handoff gebunden. Derzeit über Runtime-/ObserveOnly-Sperren unerreichbar – das ist **keine** finanzielle FE-04-Qualifikation.
- **Soll-Fix:** Direktes finanzielles Senden typisiert unerreichbar machen, sofern keine gültige *persistierte und fsync-bestätigte* Single-Flight-Approval/Intent/Reservation/Signatur-/TX-Payload-/Budget-Bindung vorliegt. Ausschließlich ein bestehender FE-04-One-Shot-Owner führt den tatsächlichen Send aus. Bundles, Tips, Prerequisites, Cost-Floor, Blockhash-Lifetime und UNKNOWN nach Transportfehler vollständig einschließen. Kein zweiter Execution-Core.
- **Tests:** compile-/trait-boundary (kein Live-Submit ohne Approval), Abort vor durable Commit ⇒ 0 sends, Crash an jeder Commit-Grenze, doppelte Aufnahme ⇒ 0 Double-Send, unklare Annahme hält Holds, Finalized-Reconciliation und auto HALT, identischer signierter Payload. Fake Submitter mit gezähltem Call.
- **Abnahme:** ein aktiver Typ-/Laufzeitpfad, der FE-04 unübergehbar bindet; erst dann FE-06 separat prüfbar. **Kein LIVE in diesem Ticket.** **Commit/Evidence:** —.


### B2 — Prefix-Decode von MarginfiAccount, Group und FeeState

**Abgleich:** Abgehakt; Mainnet-Längen/Versionsfixtures fehlen als Zusatznachweis.

- [x] **HIGH · LOCAL_PASS · P1A/P1B/FE-01 · G-E.** ~~Allgemeine `load<T>`-Helfer schneiden `data[8..8+size]` ohne Discriminator/exakte Länge.~~
- **Pfad:** `liquidation/src/health.rs::load/health`, `liquidation/src/sizing.rs::load`, `runtime/src/execution/builder.rs::should_init_liquidation_record/active_bank_keys`, `liquidation/src/protocol.rs`.
- **Ist:** Nur `decode_bank` prüft bereits exakten 0.1.11-/0.1.12-Längenvertrag. Allgemeine `load<T>`-Helfer schneiden `data[8..8+size]` ohne Discriminator/Owner/erlaubte Gesamtlänge. `health()` prüft beim Zielaccount keine Owner-/Discriminatorbindung. Builder prüft teilweise Owner/Discriminator, aber nur Mindestlänge.
- **Soll-Fix:** Zentralisierte pinned Versions-Decoder für MarginfiAccount/Group/FeeState mit erlaubten exakten Layoutlängen, Discriminator und vom Aufrufer geprüftem Program-Owner. Bei unbekanntem Upgrade/verlängertem Layout `UNKNOWN_LAYOUT`, **kein Prefix-Cast**; keine stillschweigende Annahme, dass sämtliche 0.1.12-Accounts dieselbe Größe wie 0.1.11 haben. Falls P1B-Längen noch nicht belegt: BLOCKED bis Source-/Fixture-Parität.
- **Tests:** gültige 0.1.11-Fixtures, zu kurze/verlängerte Payload, anderer Discriminator/Owner, absichtlich resized P1B-Fälle, unaligned Bytes; Health/Sizing/Builder müssen identisch fail-closed entscheiden.
- **Abnahme:** gemeinsame Provenienz/Version, keine unqualifizierten Prefix-Leser mehr. **Commit/Evidence:** `40a368f` (gemeinsam mit E1/D11/B11): `protocol::decode_marginfi_account/decode_group/decode_fee_state` (Discriminator + exakte Länge `8+size_of::<T>()` des gepinnten 0.1.11-Crates, `LayoutError{kind,found,expected}` ⇒ `UNKNOWN_LAYOUT`); Health (Zielaccount, Group), Sizing (FeeState, Account), Builder (`should_init_liquidation_record`, `active_bank_keys`), Integration, Census und Owner-Mitgliedschaft (`marginfi_account_in_group`, `liability_account_in_group`, `active_banks_in_group_account`, `bootstrap_probe_candidate`, `census_member` ⇒ resized = *undecodable member*) nutzen sie; keine Marginfi-Prefix-Leser mehr (Callsite-Inventur per grep: nur `pod_after_disc` für externe Kamino/Drift/JupLend-Layouts). Test `marginfi_layouts_are_exact_on_every_decoder_path` (Kinds/Pricing/Refresh/Health/Sizing: +1/+8/−2 Byte, Fremd-Discriminator) Exit 0; Liquidation-Lib 71, Runner-Lib 342/5 ignored PASS (`evidence/HARDENING_20261018/after_B2_E1_D11_tests.txt`). RED nur source-belegt (alte Prüfung `len < need` ließ längere Payloads zu), kein separater RED-Lauf. **Rest/NOT_RUN:** Mainnet-Länge von Group/FeeState nicht gemessen (MarginfiAccounts kommen produktiv über den `DataSize`-Filter = exakte Länge; FE-01 verlangt Group/FeeState bereits exakt) — weicht Mainnet ab, wird die Shadow-Health fail-closed `UNKNOWN_LAYOUT` (sichtbar, kein stilles Prefix). `CrateHealth::health` prüft den Program-Owner des Zielaccounts weiterhin nicht selbst (Provenienz: programmgefilterter StateCore); 0.1.12-P1B-Fixtures fehlen.


### B3 — flüchtiger Safety-Verlust-/HALT-/Kapitalzustand

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **CRITICAL vor P8-LIVE · G-E/G-F · FE-04/P8-RES.**
- **Pfad:** `liquidation/src/safety.rs`, `runtime/src/execution/one_shot.rs`.
- **Ist:** `day`, `capital_in_use`, Fehlversuche und `halted` sind RAM-Zustand. Neustart rekonstruiert sie nicht zuverlässig aus dem Finanzjournal.
- **Soll-Fix:** Einziger versionierter Finanzjournal-/Replay-Automat enthält idempotent Spend/Holds, Exposure, Failed Attempts, finalisierte PnL/Loss, UTC-Day-ID, Inventory und sticky HALT. Nicht einen zweiten Shadow-/Safety-Store bauen. `UNKNOWN` hält Kapital; nichts wird bei Restart automatisch freigegeben oder neu bewaffnet.
- **Tests:** Restart/Crash vor und nach Transition, Mitternacht/UTC-Tageswechsel, Replay-Dedupe, fehlendes Outcome, partielle Fee/Tip, unbekannte Exit-Kosten, Crash/Full-Disk; Portfolio- und Tagesbudget bleiben identisch.
- **Abnahme:** Safety-Status vor jedem werttragenden Handoff aus durable Ledger qualifiziert; kein Auto-Reset. **Commit/Evidence:** —.


### B4 — Rent für neue Liquidation-Records ist Unknown

**Abgleich:** Offen/PARTIAL; Z10 relevant.

- [ ] **HIGH · FE-02 BLOCKER für betroffene Kandidaten · G-E.** · **PARTIAL** `119da22` · Tests `b4_liquidation_record_rent_comes_from_the_rent_sysvar`, `b4_conservative_cost_uses_the_upper_price_bound_and_rounds_up`, `b4_plans_add_the_record_rent_only_when_the_record_is_created` (echter `plans()`-Pfad), `b4_record_creating_plans_carry_the_rent_or_stay_unknown`; fail-before per Mutation (Callsite `= None` bzw. entfernt, alte Preisformel) — Record-Rent = Exempt-Minimum des kanonischen Rent-Sysvars (Owner `Sysvar1…`, bincode) für die gepinnte Größe `8 + size_of::<LiquidationRecord>()` = 520 B (Mainnet-Rent: 4.510.080 Lamports), zum konservativen SOL-Preis max(TimeWeighted + Konfidenz, RealTime + Konfidenz) des gepinnten Adapters (PriceUpdateV2-Fixture), aufgerundet (überschreitet die EMA-Konfidenz `oracle_max_confidence`, lehnt der Adapter den oberen Preis ab ⇒ Rent UNKNOWN, fail-closed); in jeden Plan mit neu anzulegendem Record addiert, verlinkter Record ⇒ nichts; fehlender/fremder/undekodierbarer Sysvar, unbekannter Preis oder Overflow ⇒ UNKNOWN (nie 0). `close_liquid_record`-Rückerstattung nicht gutgeschrieben (keine Plan-Instruktion). **Offen:** kein Payer-SOL-Deckungscheck (Rent + Gebühren) vor Ausführung — LIVE bleibt über B1 gesperrt.
- **Pfad:** `runtime/src/execution/builder.rs::plans/should_init_liquidation_record`, `liquidation/src/sizing.rs`, `execution/one_shot.rs`.
- **Ist:** `rent_and_token_usd_micro=None`, falls neuer Record nötig ist. Das hält die Economics bewusst UNKNOWN. Die behauptete „Mehrheit der Accounts“ ist **ohne Marktmessung nicht belegt**.
- **Soll-Fix:** Exakte geplante Accountgröße/Owner/Instruktionen aus gepinntem Marginfi-Source; tatsächliches erforderliches rent-exempt Minimum mit validierter Solana-Rent-Quelle bzw. qualifiziertem zustandstreuen Modell berechnen; daraus SOL-Reserve und USD-Micro mittels gültigem konservativem SOL-Preis. Zusätzlich Return-/Close-/Rent-Recovery nur dann gutgeschrieben, wenn ausführbar und nachgewiesen. Keine Null- oder Pseudo-Konstanten.
- **Tests:** Record existiert/nicht vorhanden, falscher PDA, Rent-Änderung, unbekannter SOL-Preis, verfügbare Fee-SOL, i128/u128/rounding, vollständige Economics inkl. aller Kosten.
- **Abnahme:** betroffene Kandidaten entweder konservativ bewertbar oder weiterhin `UNKNOWN`; keine falschen LIVE-Kandidaten. **Commit/Evidence:** —.


### B5 — Premium-BPS über 100 % kann Unterlauf/Panik auslösen

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH · G-A (auch offline untrusted Input) · P4A/T.**
- **Pfad:** `liquidation/src/economics.rs::premium_bps/time_for_premium` (Aufrufer `t_profitable/max_seizable_for_repay` nur Regression).
- **Ist:** `base=max(fee_state_max_bps,500)`; `PREMIUM_MAX_BPS-base` kann unterlaufen; `time_for_premium` hat ein mögliches Division-durch-Null-/Underflow-Szenario. `t_profitable` hat zusätzlich zu prüfen, ob negative `min_profit`-Summen sicher in u128 überführt werden (eigener Negativtest; **noch keine bestätigte Production-Auswirkung**).
- **Soll-Fix:** Source-konforme Validierung `fee_state_max_bps <= 10_000` vor Fachberechnung; bei invalider FeeState **UNKNOWN/Unsupported und Fail-Closed**, nicht still „100 %“ als gültig normalisieren, sofern Source das nicht belegt. Sichere Grenzen/checked-Arithmetik für Division, Zeitaddition und Signed→Unsigned.
- **Tests:** 0/500/9999/10000/10001/u64::MAX; vor/nach Grace, growth_blocked, denominator=0, negative MinProfit, Debug/Release-Gleichheit, P1A-Fixture-Parität.
- **Abnahme:** keine Panik, kein Wrap, keine unverdiente positive Economics. **Commit/Evidence:** `fe895ba` (FeeState > 10_000 bps → `None` in `premium_bps`/`time_for_premium`/`t_profitable`/`max_seizable_for_repay`, checked Zeit/Produkt, nicht-positive Anforderung sofort, Bedarf > 100 % unerreichbar); Test `invalid_fee_state_and_extreme_inputs_fail_closed_without_panic` Exit 0.
- **Rest aus Zweitreview (18.10.2026), geschlossen:** `t_profitable` wandelte `p_needed − BPS` per `as u64`; Reviewerfall `repay=1, realizable=10_000 bps, min_profit=1_844_674_407_370_956` ⇒ Bedarf `u64::MAX + 8_386` bps, gewrappt auf 8_385, Cap-Check bestanden. Fix `3a4522a` (`u64::try_from(..).ok()?` ⇒ unerreichbar = `None`); derselbe Test enthält genau diesen Fall (RED vorher `Some(1502596)`, GREEN danach). Volle Workspace-Matrix steht für diesen SHA noch aus (siehe N2/F6).
- **Triage 18.10.2026:** `LATENT INPUT`: invalides `fee_state_max_bps` am/über der 10_000-BPS-Grenze als Negativtest; Trigger auf aktueller Chain **nicht** belegt. Trotzdem checked arithmetic/fail-closed zwingend, bevor untrusted State mit diesem Pfad verarbeitet wird.

- **Nachprüfung 18.10.2026:** `max_seizable_for_repay` hat keinen belegten Unterlauf; eigentliche Gefahr bei `premium_bps/time_for_premium`. Aufrufer nur auf Auswirkungen prüfen.


### B6 — synchrones Fee-/Simulation-/Jupiter-I/O im Owner

**Abgleich:** Abgehakt, aber Z2: End-to-End-Deadline nicht erfüllt.

- [x] B6 · Fix `d016fe2` · Test `b6_owner_rpc_block_grows_the_ingress_queue_until_overflow` — Messung: Owner wertet nach je ≤64 Ingress-Events einen Kandidaten synchron aus; 100 ms je RPC ⇒ 211–215 ms Block, 1.000 Events/s in 1.024er-Queue ⇒ Overflow 2.481–2.507; Mainnet-Pipeline gesund max. 2,654 s (kein Overflow), aber je Call bis zur Provider-Deadline 30 s ⇒ ≥60 s/Kandidat, bei 200 Events/s Overflow der 65.536er-Queue nach ~6 langsamen Kandidaten ⇒ erreichbar. Fix: `NetworkBudget` — höchstens `P0_RPC_CANDIDATES_PER_DRAIN` (4) Netz-Kandidaten bis die Ingress-Queue wieder leer ist, Kandidaten-Deadline `P0_CANDIDATE_NETWORK_DEADLINE_MS` (5.000) vor Fee/Simulation; Rest `LossReason::RpcBudget`/`RpcDeadline` (kein Drop, kein HALT); Test mit Guard: max. Tiefe 209, Overflow 0, 18 zurückgestellt. Strukturelle Auslagerung in RemoteWorker bleibt offen.
- [x] **HIGH RISK · G-C/G-D/P6-LT; PERFORMANCE + RELIABILITY.**
- **Pfad:** `runtime/src/execution/pipeline.rs::evaluate`, `solana/src/submit/rpc.rs`, `runtime/src/execution/route.rs`.
- **Ist:** `fee_for_message` und `simulate` rufen blockierende RPCs; Route-Build nutzt blockierenden HTTP-Client. Latenz kann den State-/Ingress-Owner aufhalten. Die behaupteten 30 s pro Call und 1,5 s pro Jupiter-Call sind Konfigurations-/Codewerte, keine gemessenen Maximal-Tails.
- **Soll-Fix:** zuerst bereits vorhandenes MT0/`detect_health_ns_*`/Fee-/Simulation-Timing und Queue-/Gap-Halt-Evidence prüfen. Danach Netzwerkjobs auf vorhandene bounded RemoteWorker/Ownership-Schicht auslagern oder nur bei begrenztem Gate einen kürzeren gesamten **Kandidaten**-Deadline verwenden. Einzelne Timeouts blind von 30 s auf 3 s zu setzen ist kein struktureller Fix; niemals RPC neu in den Trading-Hot-Path ziehen.
- **Tests:** künstlich blockierende RPC/Jupiter-Stubber, parallele Ingress-Frames, Queue-Grenzen, Cancellation, Timeout, unbekannte Fee = kein LIVE; deterministische Queue- und Shutdown-Semantik. Vor/nach p95/p99 messen.
- **Abnahme:** keine unbounded Owner-Blockade, Quellenverluste nicht verschleiert. **Commit/Evidence:** —.


### B7 — wiederholte Oracle-/Observation-Allokationen

**Abgleich:** Offen/PARTIAL; Z9 relevant.

- [ ] **PERFORMANCE · OPEN · P6-MT/S1-LT; Optimierung nur nach Messung.** · **PARTIAL** `119da22` — Health: `components_of` kopiert die Observation-Accounts einmal je Aufruf für Maintenance + Equity (vorher je Typ); Sizing: Preise je Bank einmal in `positions` (`Position.prices`), Paarschleife ohne Neuberechnung. Ausgaben unverändert (gepinnte `get_health_components` liest read-only, Reviewer; alle Health-/Sizing-Tests unverändert grün; kein Fail-before möglich, reine Optimierung). Eine frühere Ad-hoc-Zeitmessung ohne Evidenzdatei gilt nicht als Nachweis. **Offen:** Vor-/Nach-Profil mit Evidenz (Zeit, Allokationszahl/Bytes, p50/p95/p99), Stress mit vielen Bank-Paaren.
- **Pfad:** `liquidation/src/health.rs::components/health`, `liquidation/src/sizing.rs::positions/sizes`.
- **Ist:** `components` kopiert `Account::data.to_vec()` für Maintenance und Equity; `sizes` berechnet `prices()` erneut in Paarschleifen, obwohl `positions` sie bereits erstellt hat.
- **Soll-Fix:** Observation-Buffer je Health-Aufruf wiederverwenden, sicheren AccountInfo-Lifetime-Vertrag wahren; Prices/Floor pro Bank einmal pro konsistentem Slot/Oracle-Snapshot berechnen/cachen, nicht über Versions-/Root-Grenzen wiederverwenden. Keine Semantik-/Rundungsänderungen.
- **Tests:** bitidentische Health/Sizing-Outputs und P1A/P1B-Fixtures, gemessene Allokationszahl/Bytes und p50/p95/p99, Stress mit vielen Banks/Asset-Liability-Paaren.
- **Abnahme:** nachweislich weniger Arbeit bei gleicher finanzmathematischer Entscheidung. **Commit/Evidence:** —.


### B8 — BelowRoot/Confirmed-Reordering: gemeldeter Fixvorschlag gefährlich

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **REVIEW_REQUIRED · STATE SAFETY · S2/P6; NICHT BLIND ÄNDERN.**
- **Pfad:** `solana/src/state/mod.rs::apply/on_event`.
- **Ist:** Updates `slot < finalized root` werden bewusst als `BelowRoot` verworfen, Counter steigt. Fable vermutet, dass ein nachträglich empfangenes Confirmed-Update neuer als ein einzelner lokaler Account-Stand sein kann.
- **Verbotener Schnellfix:** `if a.slot > existing.slot { apply }` unterhalb Roots ohne Fork-/Finality-/Source-Proof. Das kann die finale State-Konsistenz und gespeicherte Versionen brechen.
- **Soll:** Fall mit konkretem Slot-/Root-/Account-Reordering reproduzieren; unterscheiden, ob Root-Barriere korrekt oder der Snapshot unvollständig war. Bei belegbarer State-Lücke Reconcile/Quarantine mit Provenienz und Counter-/Telemetrieanstieg. Root-Invariante bleibt strikt.
- **Tests:** zwei Forks, Root roll-forward, verspätete confirmed Accounts, fehlende Account-Snapshot-Batches, validiertes Replay, kein stale overwrite, kein falsches READY.
- **Abnahme:** sicherer State gegen Root/Finality; erst nach Beleg ein Codefix. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `TELEMETRY_ONLY / NO AUTOMATIC BELOW-ROOT APPLY`. Kein belegter praktischer Provider-Reordering-Fall mit falscher finalisierter Entscheidung. Bis Repro/Root-Audit keine StateCore-Barriere ändern; nicht als 4h-Blocker zählen.


### B9 — nicht ausführbare V1-Pläne werden generiert

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · P5/FE-02 · G-E.** · Fix `07c3f09` (ersetzt Konstante aus `9fad87d`) · Test `b9_v1_capability_flag_matches_the_builder` — Builder bietet V1 nur bei `tx::v1_build_capability()` (einmaliger echter Build+Sign der Implementierung) **und** `supports_tx_version(V1)` des Adapters; `RpcSubmitter` sendet nur SDK-`VersionedTransaction` (kein V1-Wire) und beansprucht V1 nicht mehr. V1-Transport selbst: P5.
- **Pfad:** `runtime/src/execution/builder.rs::plans`; `solana/src/tx/mod.rs::build_v1`.
- **Ist:** Builder fügt `BuildVersion::V1`-Plan hinzu; `build_v1` endet aktuell in `V1Unavailable`; vermeidbare Fehlrecords/Build-Arbeit.
- **Soll:** V1-Plan nur bei **tatsächlich qualifizierter** Build-/Sign-/Simulate-/Submit-Capability generieren, nicht allein aufgrund aktivierter Featureflag oder eines erfolgreichen Wire-Probes. V0 nicht als unqualifizierten Ersatz promoten.
- **Tests:** Capability false/true mit realem Stub, no-V1 bei unavailable, kein Übergehen von V1-Safety-Gate.
- **Abnahme:** nur real baubare Planversionen in Kandidatenliste, beide Pfade bleiben phasentreu. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `CODE_CONFIRMED NOISE`: V1-Plan ohne V1-Buildfähigkeit kostet Arbeit/erzeugt Ablehnungsrecords, impliziert **keinen** ungeprüften V1-Submit; fixen bei P5/FE-02, nicht als 4h-Blocking einstufen.


### B10 — Keypair-Fehlerpfad und Secret-Zeroization

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · G-A vor FE-06.** · Fix `9fad87d`+`07c3f09` · Test `b10_keypair_errors_never_carry_path_or_secret_bytes` — feste Codes `KEYPAIR_READ/FORMAT/INVALID` ohne Pfad/Parsertext; begrenzter, vorab dimensionierter Lesepuffer (≤ 4 KiB, keine Reallokationskopien) und Key-Bytes in `Zeroizing`; nur Ziffern (kein Vorzeichen). Kopien in OS/Allocator/SDK außerhalb der Garantie. `solana/src/tx/mod.rs::load_keypair_json`


### B11 — Alignment-abhängige bytemuck-POD-Decoder

**Abgleich:** Abgehakt; externe Integration-Layouts nicht voll qualifiziert.

- [x] **MEDIUM · LOCAL_PASS · G-E zusammen mit B2.** Geschlossen mit B2/D11 in `40a368f`: alle gepinnten Marginfi-Decoder liefern `Cow` (ausgerichtet geliehen, sonst Kopie), Test `pinned_decoders_are_alignment_independent` (Offsets 0..8) Exit 0. Rest: externe Integrations-Layouts (`pod_after_disc`, Kamino/Drift/JupLend) bleiben `try_from_bytes`. `health.rs`/`sizing.rs` nutzen `bytemuck::try_from_bytes`, `fe_qualification` anderswo unaligned Read. Nur nach Prüfung, ob Referenztypen/Kopien nötig sind, konsistente, formatgerechte Decoder; Alignment darf nicht zufällig vom `Vec<u8>` abhängen. Tests: gezielt unaligned Slice, echte Pinned Fixtures. **Commit/Evidence:** —.


### B12 — Liability-Maintenance-Gewicht aus zwei APIs

**Abgleich:** Abgehakt; Z8 betrifft zusätzliche EMODE-Semantikprüfung.

- [x] **REVIEW_REQUIRED · G-E/P3-Parität.** · Fix `16ec71a` · Test `b12_sizing_emode_and_liability_weight_equal_health` — Gewicht quellgleich (`get_weight(Maintenance, Liabilities)` = `liability_weight_maint` im gepinnten Crate), Sizing nutzt jetzt dieselbe API. Echte Abweichung gefunden und behoben: Sizing ignorierte Same-Asset-EMODE der Gruppe und nicht dimensionierbare Liabilities; jetzt gemeinsame `maintenance_emode_config_of` (Logik unverändert aus Health). `sizing.rs` `config.liability_weight_maint.into()`


### B13 — Jupiter-Fehlerbody unbeschränkt/ungefiltert

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM / UNBOUNDED ERROR BODY · G-A/N5/H2.** `runtime/src/execution/route.rs` schreibt bei Nicht-404-HTTP-Fehlern kompletten `response.text()` in die Fehlermeldung. Nur Status und allowgelisteten, length-bounded Code verwenden; keine Provider-URLs/API-Keys/Body-Freitexte. Tests: 1-MiB-Body, `SECRET`-Marker, Error mit URL/Header. **Commit/Evidence:** `416f515`, `2085b42` (Body max. 4 KiB, nur Status oder allowgelisteter No-Route-Code, Transportfehler klassifiziert); Test `jupiter_error_keeps_only_status_or_allowlisted_code` Exit 0.

- **Nachprüfung 18.10.2026:** Jupiter-Key ist ein `x-api-key`-Header, nicht Teil der URL. Unbegrenzter Remote-Body bestätigt, Credential-Leak nicht belegt; sichere begrenzte Fehlercodes weiterhin nötig.


### B14 — statische Pubkey-Parse-/LUT-Clone-Arbeit

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / PROFILE_REQUIRED · P5/P6.** `builder.rs::receivership`: `Pubkey::from_str(JUP_SWAP_PROGRAM_ID).expect("static")` und `protocol_luts.clone()` je Plan. Nur nach messbarem Effekt und Parität optimieren, keine statischen gültigen Programmpins still umstellen. **Commit/Evidence:** —.


### B15 — Inventarprüfung berücksichtigt neuen Versuch nicht

**Abgleich:** Offen/PARTIAL; Z7 relevant.

- [ ] **HIGH vor P8-INV · G-E/G-F.** · PARTIAL `e08d3a7`+`07c3f09` · Tests `b15_inventory_limit_counts_worst_case_new_and_in_flight_holds`, `b15_inventory_hold_is_the_sized_seized_collateral` — erreicht: Safety-Limit = gehalten + In-flight-Holds + neuer Hold (checked, Hold genau einmal frei, UNKNOWN hält); Hold je Plan aus dessen Sizing (`seized_equity_value` der exakten `withdraw_amount`; ZeroPrefund/Flash 0). OFFEN: ohne B1-Bindung kein Produktionsaufrufer von `check()`; `inventory_outstanding` erst aus finalisierten Wallet-Deltas (C3); konservative Bewertung max(Spot, EMA) statt Equity-Preis; Einheit `P0_MAX_INVENTORY` = USD-Micro. `liquidation/src/safety.rs::validate_limits`


### B16 — unbekanntes OracleSetup ohne aussagekräftige Dependency-Diagnose

**Abgleich:** Abgehakt; Z6 wiederholte dependency_gap-Logs.

- [x] **LOW · P1B/S2.** · Fix `07c3f09` · Test `b16_dependency_gaps_are_reported_not_skipped` — `dependency_scan` liefert Keys + typisierte Lücken (MISSING, FOREIGN_OWNER, NOT_A_BANK, UNKNOWN_LAYOUT, ORACLE_UNSUPPORTED); Setup und Bank-Update melden sie (`dependency_gap:`), Health bleibt fail-closed. `health.rs::dependency_keys`


### B17 — Ingress-Account-Commitment wird als Confirmed gesetzt

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **REVIEW_REQUIRED · S1/S2.** · NO_OVERCLAIM_VERIFIED + Fix `16ec71a` · Test `b17_account_status_is_the_subscription_commitment` — beide Zustandsquellen (Owner-WSS, RPC-Snapshot) lesen `confirmed`; der Status entspricht dem belegten Commitment und wird für keine Entscheidung gelesen. Jetzt ein Vertrag: `STATE_COMMITMENT`/`STATE_SLOT_STATUS` speisen WSS-Default, Snapshot-Commitment und Record-Status. `solana/src/state/mod.rs::on_event(Account)` setzt `SlotStatus::Confirmed`.


### B18 — doppelte Sign-Slot-Zuweisung / toter Pfadcode

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · Code-Hygiene.** · Fix `e08d3a7`+`07c3f09` · Test `b18_single_tx_with_tip_marks_t_sign_once` — T_sign und Sign-Slot genau einmal bei vollständig signiertem Payload (Einzel-TX, auch Tip ohne Bundle-Pfad — vorher kein T_sign; Bundle nach Bundle-Bau); tote `let _ = Path::Rpc`-Zeilen entfernt. `runtime/src/execution/pipeline.rs`: `rec.sign_slot` doppelt


## 7. C — dritter Reviewdurchgang: Recovery, Reconciliation, Owner und Ingress

**Prüfstand:** 2026-10-18, Code auf `main` `6d19e726a028aedd58346bcbaeea96904a7ec868`. Fables Review über `execution/pending.rs`, `liquidation/receivership.rs`, `state_owner/{detect,events,lifecycle}.rs`, `common/lifecycle.rs` und `orchestration/remote_workers.rs` wurde per GitHub-Codeeinsicht abgeglichen. Ergänzend wurden gezielt `state/recovery.rs`, `state/mod.rs`, `submit/rpc.rs` und `state_owner/ingress.rs` gelesen. **Keine Tests lokal ausgeführt; alle Tickets bleiben OPEN.** „Bestätigt“ heißt: der gemeldete Codepfad existiert; ein reproduzierter End-to-End-Langlauffehler ist erst nach Regressionstest bewiesen.


### C1 — Reconnected während Recovery wird als weiterer Recovery-Auftrag verschluckt; nach Exhausted bleibt ein stiller Dauerhalt

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH · LOCAL_PASS (alle sieben Regressionen) · G-A/G-C/G-D · S2-Recovery/P6-LT/C0-E1.** ~~Ein während der Recovery gedraintes `Reconnected` ging verloren; nach `Exhausted` blieb ein stiller Dauerhalt.~~
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
- **Abnahme/Beweis:** Vorher fehlgeschlagene Regression sowie grüne `crate_health`-/StateOwner-/Recovery-Tests und Release-Build (getesteter SHA + Exit-Codes) in `project_0/docs/evidence/BUGS_YYYYMMDD/`; `STATUS.md`/`MEASUREMENT_LEDGER.md` referenzieren C1. **Keine Mainnet-/4h-Ausführung in dieser Korrektur, kein automatischer LIVE-GO.** Erst danach `- [x]` setzen, ursprüngliche Ursache durchstreichen. **Commit/Evidence:** Teil `89a715e`: A (verlustfrei über `StateCore::reconnects_applied`), B (genau ein Zusatzzyklus bei Reconnected im letzten Versuch, max. 6 Versuche), C (Exhausted/Fatal → `OwnerTerminal`, geordneter Stopp, Econ-Abort → INCOMPLETE), D unverändert. Tests 1 `reconnected_in_third_drain_retries_without_external_event`, 2 `recovery_exhausted_without_fresh_trigger_aborts`, 3 `repeated_reconnected_cannot_extend_budget`, 5 `fatal_during_snapshot_or_root_drain_is_terminal` Exit 0. Test 4 `7ac0a31` (`StateCore::gap_open`: ein Gap nach dem letzten StreamReady verhindert Ready; Zusatzzyklus nur bei Reconnect ohne Folge-Gap): `gap_after_reconnected_invalidates_ready` (RED vorher: Ready), `gap_then_reconnected_can_recover` Exit 0. Test 6 `ad39019` (gemeinsam mit C6/D10): `no_duplicate_generation_or_old_session_readiness` Exit 0 — Kontrollpfad ohne C6 im selben Test erreicht Ready aus dem alten StreamReady. Evidence `evidence/HARDENING_20261018/after_C6_D10_tests.txt`. Test 7 `9c3d01e`: echter `ShadowOwner` aus inerten lokalen Teilen (Temp-Evidence/MT0, ungenutzter RPC-Endpunkt, kein Blockhash-Thread, kein Netzwerk-Read) über den produktiven `events()`-Pfad mit Mock-Fetch, echtem `RecoveryDriver` und Fake-Queue, Abschluss über `finish()`: valider A/B-Snapshot mit Start-ProgramData ⇒ Ready + Halt gelöscht, späteres Gap ⇒ wieder Halt; nie saubere Snapshots ⇒ `STATE_RECOVERY_EXHAUSTED` nach genau drei Fetches; geänderte ProgramData ⇒ gelatcht, nie handelbar; Ingress-`Fatal` ⇒ `WSS_RECOVERY_EXHAUSTED` ohne Snapshot-Read, sticky; `record_shadow_end` (aus dem econ-Runner extrahiert, verhaltensgleich): Owner-Fehler ⇒ `shadow_failed+abort` ⇒ Market-Journal INCOMPLETE (D1-Fall „abort“). Tests `owner_end_to_end_recovers_only_with_a_valid_snapshot`, `owner_end_to_end_ends_terminally_and_never_fakes_ready`, `owner_error_aborts_the_econ_run` Exit 0 (`evidence/HARDENING_20261018/C1_test7_owner_e2e.txt`). Release-Nachweis im Gate 3. Reale WSS-/Mainnet-Recovery NOT_RUN.


### C2 — mehrfacher Health-Aufruf pro Kandidat

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **MEDIUM / PERFORMANCE · CODE_CONFIRMED · P6-LT/P6-MT · Cross-Ref B7/B6.**
- **Pfade:** `runtime/src/orchestration/state_owner/detect.rs` (`engine.health`), `runtime/src/execution/pipeline.rs::evaluate` (`self.health.health`), `liquidation/src/sizing.rs::sizes` (`CrateHealth.health`), `liquidation/src/health.rs::components`.
- **Beobachtung:** Detektor und Pipeline rechnen Health unabhängig, der Sizer erneut; `components` kopiert Observation-Daten für Maintenance und Equity. Mehrfacharbeit und potenziell verschiedene Momentaufnahmen bei nicht streng identischen Eval-Inputs. MT0 `DetectConsumer/Pipeline` und `detect_health_ns_*` vorhanden.
- **Fixvertrag:** Zunächst die wirkliche Zahl/Last anhand MT0/Profiles belegen, dann eine konsistente, slot-/root-/timestamp-/revision-gebundene typisierte `HealthView` berechnen und nur bei identischer Stategeneration an Pipeline/Sizer weiterreichen; Ownership und Error-/UNKNOWN-Semantik der drei Aufrufpfade erhalten. Keine unvalidierte globale Cache-Nutzung, keine Preis-/EMODE-/Rundungsänderung.
- **Tests:** Callcounter bei gleichem Kandidaten (vorher 3, nachher nur berechtigte Recomputes); Read-only Fixture für die vier fachlichen Liquidationsklassen, bitidentische Maintenance/Equity/Sizing/Simulation, Snapshotwechsel erzwingt Recompute, p50/p95/p99 und Alloc-Profile.
- **Abnahme:** nachweisbar weniger CPU/Allokationen **bei gleicher** wirtschaftlicher/Health-Entscheidung. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `PROFILE_REQUIRED`. Dreifacher Health-Aufruf per Source bewiesen; Timing-/Allokationseffekt erst durch MT0 messen, keine unqualifizierte Änderung an Health-/Oracle-Snapshotsemantik.


### C3 — P8-Reconciliation besitzt keinen vollständigen Settlement-/PnL-/Freigabepfad

**Abgleich:** Offen; Q2 als latenter Safety-Defekt.

- [ ] **CRITICAL vor P8-LIVE · CODE_CONFIRMED · G-E/G-F · FE-04/P8-05 · Cross-Ref B1/B3/B15.**
- **Pfade:** `runtime/src/execution/pending.rs::try_reconcile`, `runtime/src/execution/pipeline.rs::reconcile_outcome`, `liquidation/src/safety.rs::record_outcome`, `runtime/src/orchestration/remote_workers.rs` und FE-04-Journal.
- **Beobachtung:** `try_reconcile` setzt `realized_pnl: None` und `pnl_complete: false` für **jeden** Outcome, auch `LANDED_SUCCESS`. Der Record enthält `exposure_released: p.exposure`, aber das ist bei UNKNOWN **nur ein Vorschlag**, keine tatsächliche Kapitalfreigabe: `Safety::record_outcome` HALT(UnknownEconomics) und gibt vor der `None`-Rückkehr weder Kapital noch Inventar frei. Nach 90 s ohne vollständige Signatur-/Fee-Evidence erzeugt `try_reconcile` `UNRESOLVED` (keinen gesicherten Nicht-Landung-Nachweis). Das ist zunächst korrekt fail-closed, aber kein vollständiger Controlled-LIVE-Pfad.
- **Fixvertrag:** Einziger durable FE-04-/P8-Ledger-Automat: Signaturen, Blockhash-/lastValidBlockHeight, finalized Receipt oder **strenger** Nonlanding-/Expiry-Proof, **alle** Bundle-/Tip-/Prerequisite-Transaktionen, Fees, Rent, Token-/Protocol-/Wallet-Deltas, realisierter Exit und konservativer `i128` Net PnL. Idempotente Settlement-Transitions, Auto-HALT bei UNKNOWN, globale Holds bis belastbarer Abschluss. Feld `exposure_released` erst als **tatsächlich gebucht** ausweisen oder eindeutig `requested_exposure_release` nennen.
- **Tests:** mocked finalized landed-success/failure, never-landed mit Blockheight+historischer Abwesenheitsprüfung, MissingArchive/429 ⇒ UNKNOWN, teilweise bestätigtes Bundle, Gebühren ohne Token-Deltas, mehrfacher Replay und Restart, keine Doppelbuchung; `capital_in_use` sinkt erst nach dauerhaft verbuchtem, vollständig bewiesenem Outcome.
- **Abnahme:** FE-04 und P8 teilen denselben Settlement-/Safety-Source-of-Truth; nicht `pnl_complete=false` als Abschluss verkaufen. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `DESIGN_GATE P8`. `realized_pnl=None` hält Safety korrekt fail-closed; aktuell kein freigegebener LIVE-Pfad. Nicht als jetzige Kapitalfreigabe oder LIVE-Fehlbuchung behaupten.


### C4 — P8-Landing-Reconcile benutzt confirmed statt finalized

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **CRITICAL vor werttragender Reconciliation · CODE_CONFIRMED · G-E/G-F · P8/FE-04 · Cross-Ref C3.** · Fix `e08d3a7`+`07c3f09` · Test `c4_reconcile_settles_only_on_a_consistent_finalized_receipt` (über `reconcile()`) — confirmed ⇒ `LandedUnfinalized` (Telemetrie, ungelöst); Fee nur bei finalisiertem Status + finalisiertem Receipt desselben Slots mit übereinstimmendem Erfolg/Fehler; Outcome aus `meta.err` des Receipts; Widerspruch/fremder Slot/fehlendes Receipt ⇒ keine Fee. Token-/Rent-Deltas und Freigabe: C3.
- **Pfad:** `solana/src/submit/rpc.rs::reconcile` ruft `transaction_with_meta(..., CommitmentConfig::confirmed())` und nutzt Signaturstatus ohne zwingende finalized-Barriere; `pending.rs::try_reconcile` klassifiziert daraus `LANDED_SUCCESS/FAILED`.
- **Risiko korrekt eingeordnet:** **Heute** führt C3 bei `realized_pnl=None` trotzdem zu HALT und hält Kapital zurück. **Sobald C3 geschlossen wird**, könnte der jetzige confirmed-Landing-Pfad vor finalisiertem Chainbeleg Holdings lösen oder Outcome/PnL buchen. FE-04 hat bereits den strengeren `finalized_transaction`-Einstieg; keine Abschwächung dieses Vertrags.
- **Fixvertrag:** Kritische Finance-Settlement-Funktion verwendet ausschließlich finalisierte Receipt-/Meta-/Signature-/Token-/Rent-Nachweise mit Slot-/Fork-/Source-Konsistenz; `confirmed` darf als **vorläufige Telemetrie** ohne `capital_released`/PnL verwendet werden. Finalization-Reorg/Gap/Proof-unavailable ⇒ UNKNOWN/HALT. Keine bloße Ersetzung einer einzigen Konstante, wenn Status und Metadaten unterschiedliche Commitments/Slots besitzen.
- **Tests:** confirmed→missing/reorg, confirmed-success→finalized-failure, finalized mit vollständigen Gebühren/Deltas, Status finalized aber Meta absent, Fork-Drift, Replay. **Abnahme:** kein Finance-Release/Outcome vor finalized-gebundenem vollständigen Settlement. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `DESIGN_GATE P8`. `confirmed` ist im alten Pending-Reconcile vorhanden; solange C3 Unknown liefert, kommt es zu **keiner** Kapitalfreigabe. Fatal würde die Kombination erst bei P8-PnL-Implementierung ohne Finality-Gate.


### C5 — finanzielle Reconciliation konkurriert im Winner-Workload-Budget

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM vor P8-LIVE · CODE_CONFIRMED · G-E/G-F · P8/Provider.** · Fix `e08d3a7`+`07c3f09` (Regression aus `e08d3a7` behoben) · Tests `c5_reconcile_is_admitted_before_queued_winner_jobs_within_the_budget`, `c5_paced_reconcile_never_blocks_live_reads` — `Workload::Reconcile` höchste Priorität, teilt den Pacing-Slot mit Winner (Gesamtrate bleibt `analytics_rps`); Auswahl nur unter pacing-bereiten Wartenden (kein Head-of-line-Block); gleiche globale Grenzen.
- **Pfad:** `runtime/src/orchestration/remote_workers.rs::start` und `solana/src/provider.rs`.
- **Ist:** `winner-rpc` und `reconcile-rpc` konstruieren beide Clients mit `Workload::Winner`; Providerlimiter mappt diese Klasse auf `analytics_rps` (Default 2/s) und gemeinsamen maximalen Provider-RPS-/FREE_ONLY-Vertrag.
- **Soll-Fix:** Separater, klar priorisierter Reconcile-Workload **innerhalb** derselben globalen echten Providergrenze, mit Admission-/Deadline-/Burst-Fairness, ohne neues ungeprüftes Requestbudget oder unbegrenzte Retry-Pfade. Darf C3/C4 nicht vortäuschen. Bei Quota-Mangel: UNKNOWN und konservativer HALT; Winner-Analytics darf blockiert werden, Finance-Evidence nicht still degradiert.
- **Tests:** 100 Winner-Jobs + eine Settlement-Aufgabe bei engem lokalem Fake-Quota, Reconcile-Vorrang, Provider Gesamt-RPS nie überschritten, 429/adaptive cooldown, Shutdown und no-spin.
- **Abnahme:** priorisierbare sichere Reconciliation ohne FREE_ONLY-Aufweichung. **Commit/Evidence:** —.


### C6 — WSS-Resubscribe verliert Session-JoinHandle

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · G-A/G-C · S0/S2.** ~~`IngressSessions::spawn` verwirft den JoinHandle; alte und neue Session können überlappen.~~
- **Pfade:** `runtime/src/orchestration/state_owner/events.rs` Dependency-Change-Resubscribe, `state_owner/ingress.rs::IngressSessions::spawn`.
- **Ist:** `IngressSessions::spawn` startet den Thread, gibt aber nur `Shutdown` zurück; der Thread-JoinHandle wird verworfen. Events ruft `self.ingress_session.request()` und unmittelbar darauf `ingress.spawn` mit **derselben Producer-/IngressHalt-Instanz**. Kurz überlappende Event-Generationen sind möglich; „bis 100 ms“ ist **nicht als harte Laufzeitgrenze bewiesen**.
- **Soll-Fix:** Cancel + geordneter Join (bounded mit nachgewiesener Cancellation) und eindeutige Sessiongeneration vor neuem Spawn; nachgewiesen kein alter Writer in neuer Generation, keine irrtümliche Clearing-/HALT-Bedingung durch alte Session. Shared `SlotWatch` und Root-Beweis bleiben kohärent. Kombiniert mit C1, **keine zweite Restart-Implementierung**.
- **Tests:** Slow old session, Cancel während Blocked-Recv, Duplikate nach Restart, Old-Fatal nach New-Ready, mehrfaches Update, hard stop. **Commit/Evidence:** —.

- **Nachprüfung 18.10.2026:** Frame-Wait maximal ca. 250 ms pro Loop, Ping-/Pong-`tx.send` jedoch bis 5 s; ein altes WSS-Threadende binnen 100 ms ist unbewiesen. Sessiongeneration, Cancel + Join mit Shutdownbudget testen.
- **Commit/Evidence:** `ad39019`. `IngressSession` hält Stop-Request, JoinHandle und Generation; `stop_and_join` pollt `is_finished` im Shutdownbudget (`limits.shutdown_seconds`, kein falsches `join_timeout`), dann `join`. Resubscribe: HALT → alte Generation stoppen und joinen (sonst `OwnerTerminal("INGRESS_SESSION_NOT_QUIESCENT")`) → die bis dahin eingereihten Events gehören der alten Generation (`RetiredEvents`: deren `Reconnected` wird verworfen und gezählt, Daten normal angewendet) → Ingress-Halt + StateCore-Gap bis zum `Reconnected` der neuen Generation → Spawn Generation n+1. `finish` stoppt und joint die Session in jedem Modus (`INGRESS_NOT_QUIESCENT` bei Überschreitung). Tests `no_duplicate_generation_or_old_session_readiness`, `ingress_session_stop_and_join_is_bounded`, `owner_terminal_codes_are_reported_verbatim`; Regression `state_owner::` 22/1 ignored, `solana state::` 28 — alle Exit 0 (`evidence/HARDENING_20261018/after_C6_D10_tests.txt`). Nicht getestet: echte WSS-Session mit hängendem DNS/`spawn_blocking` (Join-Grenze dann = Shutdownbudget, danach fail-closed); reale Laufzeit NOT_RUN.


### C7 — Ingress Fatal kann den Owner passiv bis Laufzeitende halted lassen

**Abgleich:** Abgehakt; Fatal-Queue-/Window-Grenzfall nicht belegt.

- [x] **HIGH vor 4h · CODE_CONFIRMED / TERMINAL-BEHAVIOR-TEST_NOT_RUN · G-C/G-D.**
- **Pfade:** `solana/src/ingress/ws_rpc.rs` emittiert `IngressEvent::Fatal("WSS_RECOVERY_EXHAUSTED")` und beendet Transport-Thread; `solana/src/state/recovery.rs::on_event_cancellable` startet Recovery ausschließlich für `Reconnected`; `runtime/src/orchestration/state_owner/events.rs` besitzt für `Fatal` keinen terminalen Branch; `state_owner/lifecycle.rs` bleibt bis Window-/globalem Stop aktiv.
- **Soll-Fix:** Terminales Feed-Fatal an Owner-/Capture-Window-/Evidence-HALT propagieren, kontrolliert alle Worker/Writer/Segmente drain/seal, Run als `INCOMPLETE_ABORT` mit Ursache `WSS_RECOVERY_EXHAUSTED` beenden, nie `READY`/`PASS`. Bei echter Recovery noch verfügbare Quelle darf nur eine gesondert begrenzte, belegte neue Generation wieder starten; nicht still warten.
- **Tests:** Fatal ohne Folgeevent, Fatal während A/B-Snapshot, Writer-Queue voll, atomarer Window-Abschluss, keine weitere Detection/Submission, Prozess beendet innerhalb Run-Deadline mit Nicht-0/INCOMPLETE. **Commit/Evidence:** `89a715e` (Fatal sticky im `StateCore`, `RecoveryOutcome::Fatal{code}` auch aus dem Recovery-Drain, Owner endet über `OwnerTerminal` geordnet → Econ-Abort → D1 INCOMPLETE); Tests `fatal_during_snapshot_or_root_drain_is_terminal`, `owner_terminal_codes_are_reported_verbatim` Exit 0. Nicht separat getestet: volle Writer-Queue, Window-Abschluss unter Fatal.


### C8 — pro Kandidat kopierte Programm-Allowlisten

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / PROFILE_REQUIRED · P6-MT/P6-LT.** · **PARTIAL** `119da22` — `Pipeline.global_allowed/prereq_allowed` als `&'a [Pubkey]` vom Owner geliehen statt zwei `Vec`-Clones je Kandidat; während der Evaluation unveränderlich (Borrow-Checker). Typänderung, daher kein Fail-before-Test möglich; bestehende Allowlist-Tests (`e2_pipeline_rejects_plans_by_their_built_instructions`, Econ-Tests) grün. **Offen:** Profil vor/nach, Concurrent-Lookup-Test.
- **Pfad:** `state_owner/detect.rs`: `global_allowed.clone()`, `prereq_allowed.clone()` pro `Pipeline`.
- **Soll:** Borrowed immutable slices/Arc nur bei nachgewiesener Allokations- und Ownership-Parität; keine ungewollte Mutation während Kandidatenbewertung. Test: identical allow/deny and build under concurrent lookups; Profile vor/nach. **Commit/Evidence:** —.


### C9 — Blockhash-Höchstalter dreifach hart kodiert

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · LOCAL_PASS · P6.** ~~dreimal `30_000_000_000` ns.~~ `0ff6704`: eine Konstante `BLOCKHASH_MAX_AGE_NS` (Wert unverändert 30 s, keine Erweiterung) an beiden Detection-Gates und im Pipeline-Build-Limit. Test `blockhash_freshness_is_one_policy` (am Cutoff gültig, 1 ns danach stale, kein Gate mit eigenem Literal) Exit 0. Versionierte Konfigurationsgröße statt Konstante: nicht umgesetzt (keine neue Config ohne Freigabe).
- **Pfad:** `state_owner/detect.rs` zweimal, `state_owner/events.rs` einmal `30_000_000_000` ns.
- **Soll:** Eine einzige explizite, versionierte Deadline-/Freshness-Policy statt versteckter Zahlen; **keine Erweiterung** der Gültigkeit ohne Evidence/Autorisation. Test: gleiche Grenzwerte an allen drei Gates, 1 ns vor/nach Cutoff. **Commit/Evidence:** —.


### C10 — ReserveStale wird via Fehlermeldungstext reklassifiziert

**Abgleich:** Abgehakt; Q5 String-Vertrag und Q13 SHA-Dokumentation bleiben.

- [x] C10 · Fix: dieser Commit `fix(project0): C10 …` (SHA im nächsten Ticket-Commit) · Test `integration_stale_is_classified_by_the_pinned_error_code`, `c10_integration_stale_is_typed_not_text` — Klassifikation an der Quelle über den gepinnten Fehlercode (MarginfiError 6206/6322/6411/6504, Anchor- oder Custom-Programmfehler): `HealthError::IntegrationStale` bzw. Marker `INTEGRATION_STALE:` im Sizing-String ⇒ `LossReason::IntegrationRefresh`; die beiden `contains("…Stale")`-Stellen (detect.rs, pipeline.rs) entfernt; fremder Text mit „ReserveStale“ bleibt Routing.
- [x] **MEDIUM / ERROR-ROBUSTNESS · P3/P5.**
- **Pfad:** `state_owner/detect.rs`: `rec.reason.contains("ReserveStale")`.
- **Soll:** Typisierte/enum Fehlerursache an Source-/Integration-Adapter erzeugen, ohne String-Contains auf wechselnden Provider-/Crate-Texten. Test: Fehler mit anderer Formulierung ⇒ gleicher `IntegrationRefresh`-LossReason; fremder Text „ReserveStale“ darf nicht falsch klassifizieren. **Commit/Evidence:** —.


### C11 — Available Inventory in Detection ohne in-flight Holds

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **HIGH vor P8-Multi-Asset/Parallel-LIVE · CODE_CONFIRMED / IMPACT_SCOPE_SINGLE_FLIGHT · Cross-Ref B15/B3.**
- **Pfad:** `state_owner/detect.rs`: `inventory` schätzt ATA-Lamports allein aus RAM-State; Safety/FE-04 holds werden beim Angebot nicht abgezogen.
- **Ist:** Bei aktuellem `max_outstanding_attempts=1` nicht automatisch schädlich, da finanzielle Single-Flight weiterhin separat durchsetzbar bleiben muss. Bei späterer paralleler Inventory-/Fundingplanung kann dasselbe Collateral für zwei Vorhaben reserviert werden.
- **Soll:** konsistente, dauerhafte, walletweite verfügbare Menge = finalisierte/verifizierte Bestände minus **alle** offenen Reservierungen einschließlich UNKNOWN und konkurrierender Bots; dynamische Reconciliation. Ohne gültige State-/Hold-Provenienz ⇒ UNKNOWN/no-plan.
- **Tests:** zwei konkurrierende Kandidaten, andere Bot-Owner, Crash/Restart, UNKNOWN, NO_SWAP-Collateral, intra-slot Deltas, Idempotency; Inventory nie doppelt ausgegeben. **Commit/Evidence:** —.


### C12 — Reconcile-Timeout von 90 s ohne qualifizierte Chain-Abwesenheitsprüfung

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **MEDIUM als Konfiguration; HIGH für Safety-Settlement · Cross-Ref C3/C4.** · PARTIAL `e08d3a7` · Test `c12_timeout_is_unknown_and_releases_nothing` — erreicht: Timeout ⇒ UNRESOLVED/UNKNOWN, `exposure_released=0`, Holds bleiben. OFFEN: Frist als versionierte Policy, finalisierter Nonlanding-/lastValidBlockHeight-Beweis (Blockhash-Gültigkeit beim Signieren erfassen, Archiv-Vollständigkeit), weitere Reconciliation nach Timeout statt Entfernen aus `pending` (heute ohne Sendepfad, B1).
- **Pfad:** `state_owner/remote_workers.rs`: `try_reconcile(..., 90)`.
- **Soll:** Explizite versionierte Deadline-/Provider-/Blockhash-Validity-Policy; bloßer Wallclock-Timeout ist niemals Beweis für `NotLanded`. Timeout führt UNKNOWN, bis finalized Nonlanding/lastValidBlockHeight mit ausreichend vollständiger History nachgewiesen wird.
- **Tests:** 89/90/91 s und langsamer finalized Slot, RPC-429/GAP, nicht bestätigte Bundle-Signaturen, realer lastValidBlockHeight-Cutoff als synthetisches Fixture, kein falsches `EXPOSURE_RELEASED`. **Commit/Evidence:** —.


### C13 — aktualisierte Review-Abdeckung

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **REVIEW_COVERAGE · kein eigener Produktionsbug · Cross-Ref E13.**
- **Gelesen:** `state_owner/{sweep,ingress,events,detect,lifecycle}.rs`, `liquidation/{protocol,funding,planner,projection,index,integration}.rs`, `orchestration/{control,winner_tracker,boot_audit}.rs`, `market/{capture,sink,core}.rs` und ältere Storage-/Finance-/Recovery-Module. Gezielt zusätzlich `decoding.rs`, `strict_driver.rs`, `market_truth.rs::provisional_event` sowie JournalSink-Aufruf im Verifier.
- **Weiter offen:** `market/winner.rs`, `chain.rs`, `econ*.rs`, `measurement.rs`, `report.rs`, `census.rs`, `lookup_tables.rs`, komplette `market_*`-Verifier, `markers.rs`, `related.rs`, `rows.rs`, `summary.rs` (teilweise), `common/{mt0,histogram,timing,error,config}.rs`, Bins und Python.
- **Abnahme:** eindeutige SHA-/Scope-/Fixture-Matrix; nicht als Gesamtabnahme zählen. **Commit/Evidence:** —.

## 8. D — vierter Reviewdurchgang: Market-Evidence, Funding, Sweep und Ingress

**Review-Stand:** 2026-10-18, betroffene Codepfade auf `main` bei `4e7f4a413faaa158bb1b91d4678a8de0312e0e5c` gelesen. Nachfolgender `main`-Commit `9b250dacc1e97f07e2fff56c8a48d53380528a1a` ändert Reporter-Quellen und -Tests **innerhalb `project_0`**, aber nicht die untersuchten Funktionen. Von Fable gelesen: `state_owner/sweep.rs`, `state_owner/ingress.rs`, `liquidation/{protocol,funding}.rs`, `market/{capture,sink,core}.rs`. Zusätzlich wurden `market/summary.rs`, `market/observer_tests.rs`, `execution/{builder,pipeline}.rs` für die betreffenden Pfade gegengeprüft. **Alle D-Tickets bleiben offen: keine neuen Fixes und keine lokal ausgeführten Regressionstests.** Verbindliche Fixvorgaben gelten vorbehaltlich fachlich identischer Paritätstests; keine Trading-/Safety-/Quota-Regel wird stillschweigend verändert.


### D1 — geordneter Market-Capture-Abbruch versiegelt Journal fälschlich COMPLETE

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH / EVIDENCE INTEGRITY · CODE_CONFIRMED · G-A/G-C/G-D · PERSIST/SK/C0-E1; VOR 4H.**
- **Pfade:** `runtime/src/market/sink.rs::MarketSink::finish/JournalSink::finish`, `runtime/src/market/summary.rs::finalize`, `runtime/src/market/capture.rs::finish_capture/run`, `analytics/journal.rs::verify_chain`.
- **Ist/Reproduktion:** `JournalSink::finish()` ruft unabhängig vom Outcome `seal(true)`. Bei `stop_reason` = `OPERATOR_STOP`, `STREAM_NOT_READY`, `INGRESS_THREAD_ENDED`, `C0_E1_ABORT` oder bei `drain_complete=false` schreibt `ObserverCore` zwar `INCOMPLETE`-Coverage und die Summary meldet Nichtvollständigkeit; der Writer erhält trotzdem `complete=true`. Dadurch `MarketJournalBinding.status=COMPLETE` und ein struktureller Verifier kann `verify_chain().complete=true` berichten. **Wichtig:** Manifest-COMPLETE bezeichnet einen korrekt abgeschlossenen Capture-Schreibstrom, nicht automatisch die nachgelagerte vollständige kanonische Markt-/Programmpfad-Verifikation. Beide Achsen müssen unabhängig fail-closed bleiben.
- **Verbindlicher Fix:** `MarketSink::finish(&mut self, complete: bool)` beziehungsweise äquivalent explizite typisierte Completeness im gemeinsamen Finishing-Vertrag; `capture::finish_capture`/`summary::finalize` übergibt mindestens `stop_reason == "TIMER" && drain_complete && write_error.is_none()`. Ein persistenter `core.incomplete_reasons`-/Gap-/Ingress-Abort darf nie durch `seal(true)` zu einem behaupteten *vollständigen Capture* hochgestuft werden; Definition der einzelnen Vollständigkeitsachsen mit `C0E1_4H_READINESS.md` abstimmen. `JournalSink` versiegelt entsprechend; `JsonlSink`/Pinned-Offline-Export nur an seiner tatsächlichen Batch-Grenze abschließen (siehe D4), kein vorzeitiger Kompatibilitätsbruch. Schriftliche Summary-/Binding-/Manifest-/Verifier-Konsistenz.
- **Regression RED→GREEN:** `capture_operator_stop_seals_incomplete` (real `finish_capture`, `verify_chain.complete=false`, Binding INCOMPLETE, Coverage INCOMPLETE); Matrix `STREAM_NOT_READY`, `INGRESS_THREAD_ENDED`, `C0_E1_ABORT`, `WRITE_ERROR`, `drain=false`, vollständiger TIMER+Drain; `Writer::seal(false)`/Manifest/Schema-Provenienz; nach `seal(false)` kein stilles `resume` als Complete, keine überschriebenen historischen Journale.
- **Abnahme:** **Alle Abbruchpfade liefern dieselbe nachweisbare unvollständige Evidence-Semantik** in Journal, Binding, Summary und Report; keine falsche FULL_CAPTURE-/FULL_MARKET-Qualifikation; Tests/Commit-SHA/Evidence dokumentiert. **Commit/Evidence:** `41f3199` (`MarketSink::finish(complete)`; COMPLETE nur bei TIMER-Fenster, Drain fertig, ohne Schreibfehler und ohne `incomplete_reasons`; Strict-Batch nur bei vollständiger Reconciliation); Test `capture_operator_stop_seals_incomplete` (alle Abbruchpfade, Binding/Summary/`verify_chain` übereinstimmend) Exit 0.
- **Rest aus Zweitreview (18.10.2026), geschlossen:** `JsonlSink` überschrieb `finish` nicht; die Trait-Default rief `flush` ⇒ `seal(true)` unabhängig vom Outcome. Fix `321ac0b`: `JsonlSink::finish(complete)` versiegelt mit genau dieser Vollständigkeit, `flush` bleibt die explizite Verifier-Batchgrenze. Test `jsonl_sink_finish_seals_incomplete_when_asked` (RED vorher: COMPLETE bei `finish(false)`) Exit 0.


### D2 — NoSwapInventory und PreheldInventory erzeugen doppelte technische Arbeit

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM / PERFORMANCE + PROVIDER QUOTA · CODE_CONFIRMED · G-A/G-E · FE-02/P5/P6.** · Fix `119da22` · Test `d2_identical_funding_plans_share_one_fee_quote` (fail-before per Mutation) — Fee-Quote je Evaluation nach Message-Hash der gebauten Bytes (deckt Instruktionen, LUT-Adressen, Blockhash): byte-identische NoSwap-/Preheld-Pläne ⇒ ein RPC-Call, beide Fundinglabels und Kostenrecords bleiben; abweichende Bytes ⇒ eigener Call; kein Reuse über Evaluationen/Blockhashes; H6-/FEE_CALLS-Zähler nur bei echtem Call. Rest (bewusst): Build/Sign laufen weiter je Plan (lokale CPU); ein fehlender Quote (`None`) gilt für die identische Message mit — Typisierung in N2.
- **Pfade:** `liquidation/src/funding.rs::is_feasible/feasible_classes/capital_required`, `runtime/src/execution/builder.rs::plans`, `runtime/src/execution/pipeline.rs::evaluate`.
- **Ist:** `NoSwapInventory` und `PreheldInventory` prüfen beide `liability_inventory >= liability_needed`, binden denselben `liability_needed_value`-Betrag. `builder::plans` ruft für beide dieselbe `receivership(..., vec![], true)` auf und erzeugt V0-/NoSwap-Pläne. `pipeline::evaluate` baut die Varianten separat und kann beide über `fee_for_message` bepreisen. **Zwei identische RPC-Calls sind nur dann belegt, wenn beide Varianten den jeweiligen Fee-Branch tatsächlich erreichen; nicht jede Opportunity erzeugt zwangsläufig zwei Calls.**
- **Verbindlicher Fix:** **Keine Fundingklasse aus dem Phasenplan streichen.** Zunächst volle semantische Gleichheit von TX-Bytes, LookupTables, Token-/ATA-/Inventar-Provenienz, Kosten, Plan-Identity und Safety-Holds nachweisen. Falls byte- und entscheidungsgleich: technische Build-/Sign-/Fee-Evidence pro deterministischem Message-/Fee-Cache-Key nur einmal erzeugen und an beide Attributions-/Fundingkandidaten anbinden. Falls Kapitalherkunft, Exit-/Inventorystrategie oder spätere Kosten abweichen, nicht blind deduplizieren. Kein Cross-Slot-/Cross-Blockhash-Reuse, kein Quote-Zero-Default.
- **Tests RED→GREEN:** identische NoSwap/Preheld-Pläne → ein Build/Feecall, **beide** Fundinglabels/Reports bleiben vorhanden; abweichende Plan-/Fee-/Blockhash-/LUT-Inputs → getrennte Arbeit; Mock-RPC-Callcounter 5-rps-Policy unverändert; economics/safety parity und Funding-Class-Schema. 
- **Abnahme:** doppeltes Remote-I/O nachweisbar reduziert, vollständige Entscheidungs- und Finanzattribution unverändert. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `CODE_CONFIRMED unter Branchbedingungen`. NoSwap/Preheld bauen identische Instruction-Klassen und beide Fee-RPC-Aufrufe erfolgen im Shadow nur, **wenn** beide Kandidaten bis zum `path==Rpc, tip==0, no-prereq`-Fee-Branch gelangen. Nicht jede Opportunity erzeugt garantiert zwei Fee-Calls.


### D3 — Sweep-Fan-out ist accountseitig unbounded und blockiert Owner-Ingress

**Abgleich:** Abgehakt; Mainnet-Last nicht gemessen.

- [x] **HIGH · LOCAL_PASS (gemessen, Overflow-Hypothese offline widerlegt; reale Last G-C) · G-A/G-C/G-D · R127/S1-LT/P6-MT.** ~~Owner-Blockdauer des Fan-outs unbekannt, keine Messung pro Batch.~~
- **Pfade:** `runtime/src/orchestration/state_owner/sweep.rs::sweep`, `state_owner/events.rs` und Owner-Loop, `solana/src/runtime_config.rs`.
- **Ist:** `sweep_batch_max_events` begrenzt geänderte Quell-Events, **nicht** die per Reverse-Index zugeordneten Debtor-Accounts. `account_work` wird komplett aufgebaut, anschließend ganz per `into_par_iter()` auf dem Owner evaluiert und sortiert; während des Blocks wird `consumer.pop` nicht bedient. Bei Default `workers=1` arbeitet Rayon faktisch seriell. Ein Bankupdate kann große C0-E1-/R127-Census-/Health-Fan-outs erzeugen; die genaue maximal erreichte Blockierungszeit ist ohne Lastmessung unbekannt.
- **Verbindlicher Fix:** Vor der 4h-Bereitschaft synthetischer deterministischer Lasttest für größte erwartete Dirty-Bank-/Account-Menge (auch 1 Worker). Bei unzulässiger Owner-Stall/Overflow: stabile **gebundene** Account-Work-Queue mit fairen Chunks und wiederholter Ingress-Drain-Opportunity, Snapshot-/Root-/input_revision-/event_slot-Provenienz pro Work-Item, Dedup/Force-Rearm/faire Retryordnung. Keiner der sechs Instruktionsmarker und keiner der Debtor-/Census-Fälle darf still verloren gehen. No O(n²)-Kopie, kein unbounded Aufstauen oder willkürliches Dropping. Optional gesonderter CPU-Worker nur mit eindeutiger Ownership/Backpressure; keine parallelen mutable State-Zugriffe.
- **Tests RED→GREEN:** 1/2/N Rayon-Worker, eine Bank mit 100/10.000/100.000 abhängigen Accounts, Dirty-Event mitten im Lauf, Root-/Oracle-Revision während Chunkgrenze, Shutdown, starvation/oldest-first, voller Queue-/RAM-Budget, Counterparität (unique/forwarded/healthy/liquidatable/pending/census), max Owner-blocking p95/p99 und kein unregistrierter Ingress-Overflow.
- **Abnahme:** realistische Fan-outs unter begrenzter Owner-Blockdauer und ohne Vollständigkeitsverlust; erst echte Kalibrierung misst freie Kapazität. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `SOURCE_CONFIRMED, IMPACT_UNMEASURED`. Unbounded Account-Fanout/Owner-Hold ist Fakt, Queue-Overflow und verletzte 4h-SLO sind noch Hypothesen. Synthetischen Stress **vor** 4h ausführen; Chunking bei tatsächlich über Budget liegendem Owner-Stall.
- **Commit/Evidence:** `60700aa`. (1) Fehlende Messung behoben: Owner-Blockdauer je Sweep-Batch (Summe, Maximum, Accounts des langsamsten Batches) von Batch-Start bis Ende der Kandidatenplanung, eigene Telemetriezeile `sweep_timing:` (die von R126/R127-Qualify-Skripten geparste `oracle_trigger_stats`-Zeile bleibt unverändert). (2) Synthetischer deterministischer Stresstest `sweep_fanout_owner_block_is_measured` (`#[ignore]`, Release): eine Dirty-Bank mit 100/10.000/100.000 abhängigen Accounts, Fixed-Price-Banks, Sweep-Muster Dedupe → `par_iter` exakte Health → Sortierung, 1/2 Rayon-Worker. Release auf diesem Host: 100k ⇒ **130 ms (1 Worker) / 72 ms (2 Worker)**, ~1,3 µs/Account; 10k ⇒ 12 / 6 ms (`evidence/HARDENING_20261018/D3_sweep_fanout.txt`, Exit 0; `state_owner::` 22 PASS/2 ignored).
- **Einordnung:** Mit den gemessenen Mainnet-Kosten des Detect-Pfads (3,39 µs/Health-Call, Ledger `r127_20261006T130642Z`) blockiert ein 100k-Batch ≈ 0,34 s; Queue-Kapazität 65.536 Events (`P0_INGRESS_BUFFER`). Mainnet R127c/R127d (Max-Batch 78.488 Accounts, 204 Banks): Backlog max 506 bzw. 460, Overflow 0. Ein Overflow bleibt fail-closed (Gap/HALT, nie stiller Drop). **Kein Chunking** — Auftragsbedingung „gemessene unzulässige Owner-Stall/Ingress-Overflow“ nicht erfüllt. **Offen/NOT_MEASURED:** reale Owner-Block-p95/p99 unter Mainnet-Last (neue `sweep_timing`-Zeile, G-C), echte Oracle-Decodierkosten im Sweep (Fixture nutzt Fixed-Price), Census-Batch mit Zeilenerzeugung.


### D4 — `on_logs` ruft `flush` pro TX auf und kann Kompatibilitätssink früh versiegeln

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM / CAPTURE-FAILURE · CODE_CONFIRMED · G-A/G-C · JFI/PERSIST.**
- **Pfade:** `runtime/src/market/core.rs::on_logs`, `runtime/src/market/sink.rs::JournalSink::flush/JsonlSink::flush`.
- **Ist:** Nach jeder zugelassenen Logs-TX `sink.flush()?` mit veraltetem `BufWriter/SIGTERM`-Kommentar. `JournalSink::flush` ist ein No-op (eigener Writer batcht/synct); `JsonlSink::flush` führt einmal `inner.seal(true)` + Legacy-Export durch. Beim selben `on_logs`-Sink würden Folgeframes mit `MARKET_JOURNAL_ALREADY_FINISHED` abbrechen. Strict-Batch-`JsonlSink::flush` ist bewusst ein **Batchabschluss**, kein generischer TX-Flush.
- **Verbindlicher Fix:** Unqualifizierten per-TX-`flush`-Aufruf und Kommentar aus `on_logs` entfernen; Writer-Lane darf weiter gemäß `WriterConfig` flush/sync machen; `JsonlSink` nur explizit an der Verifier-Batch-Grenze abschließen. Bei notwendigem Durability-Checkpoint separate native API statt `finish`-Semantik missbrauchen.
- **Tests:** zwei/drei Logs-TX nacheinander mit `JsonlSink` → alle Rows vor explizitem Batch-Finish; `JournalSink` Batch-/Sync-Policy bleibt unverändert; Strict-Exporter paritätisch, kein JSON-Writer im produktiven Capture.
- **Abnahme:** keine vorzeitige Versiegelung/Verlustfolge durch reguläre TX-Verarbeitung. **Commit/Evidence:** `c256e9d` (kein per-TX-`flush` in `on_logs`); Test `logs_never_finish_a_batch_sink_per_transaction` Exit 0.
- **Triage 18.10.2026:** `LATENT_ONLY after strict-driver inspection`. `strict_driver.rs` nutzt `market_verification::run` → `JournalSink`, genau wie produktiver `capture.rs`. `JsonlSink` ist in den gelesenen Pfaden nur als Kompatibilitäts-/Testtyp sichtbar. Die Fehlkombination ist möglich, aber **kein** nachgewiesener 4h-Capture-Fehler. Gate von pauschal „vor 4h“ auf defensive API-Härtung nach produktiver D1-Gatekorrektur herabstufen.


### D5 — unerreichbar angenommener Receivership-Decoderzweig kann Capture panicken

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM / PANIC HARDENING · CODE_CONFIRMED, TRIGGER_NOT_PROVEN · G-A/G-C · C0-E1.**
- **Pfad:** `runtime/src/market/core.rs::on_logs`, `decoded.additional_events`, `LiquidationType::Receivership`.
- **Ist:** `unreachable!("original receiver decoder handles this type")` in von externen Chain-Logs abgeleitetem Decoderausgang. Ob der gepinnte Decoder diesen Enum-Fall derzeit tatsächlich in `additional_events` liefern kann, ist **noch nicht bewiesen**; ein unerwartetes Verhalten würde den Capture-Thread paniken lassen.
- **Verbindlicher Fix:** Kein `unreachable!/panic!/unwrap` auf untrusted Decodevarianten. Explizit fehlerhaftes/inkonsistentes Decoderresultat zählen und als `UNKNOWN/INCOMPLETE` qualifizieren, keine Liquidation unterschlagen und keine falsche Duplikatzählung. Keine stille `continue`-Lösung, die den Pfad als vollständige Evidence erscheinen lässt.
- **Tests:** direkt injizierter zusätzlicher Receivership-Event, gültige Classic/Deleverage/Bankruptcy, Doppeldekodierung, Truncation, fuzz-/property-Test aller Varianten; kein Panic, keine falsch vollständige Coverage.
- **Abnahme:** Decoderfehler fail-closed in Evidence und Report, Capture bleibt kontrolliert. **Commit/Evidence:** `c256e9d` (`count_provisional`: Receivership in `additional_events` → Decoderfehler, INCOMPLETE, kein Panic); Test `unexpected_receivership_in_additional_events_is_a_decoder_error` Exit 0.
- **Triage 18.10.2026:** `CURRENTLY UNREACHABLE / DEFENSIVE_ONLY`. `market/decoding.rs` führt Receiver-Payloads separat in `events`. `market_truth.rs::provisional_event` akzeptiert nur `CLASSIC_EVENT`, `DELEVERAGE_EVENT`, `BANKRUPTCY_EVENT`; daraus kann `additional_events` aktuell kein Receivership enthalten. Existierendes `unreachable!` für künftige Decoderänderungen ersetzen/auf Enum-Typvertrag testen, aber keinen gegenwärtigen Crash behaupten.


### D6 — MarketStats pro Ingress-Event zweimal geklont und emittiert

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / PERFORMANCE · CODE_CONFIRMED · G-C/G-D · MT0/Market-Observer.**
- **Pfad:** `runtime/src/market/core.rs::on_ingress_event`.
- **Ist:** Mit `econ` klont der Code `self.stats` vor dem Event und nachher erneut, erzeugt `Box<MarketStats>` und einen `MarketCounters`-Channel-Event auch bei unveränderten Countern (u. a. Slots). Slot-Evidence selbst muss weiterhin getrennt emittiert werden.
- **Verbindlicher Fix:** Zuverlässige Differenz-/Delta-Erkennung mit stabiler Zeit-/Slot-Provenienz; nur bei Änderung der zu überwachenden Zähler Counter-Evidence erzeugen; notwendige Heartbeat-/Slot-/Cutoff-Evidence unverändert. Option: konkrete changed-bit/typed delta statt jedes Mal Vollkopie, soweit Analyzer-Vertrag es erlaubt.
- **Tests:** unverändertes Event → keine Counter-Kopie/Emission; echte Änderung → exakt einmal, Index-/Ordering-/Timestamp-Parität, High-Rate-Slotstorm, keine Messlücke bei monotonen Countern; allokationsbezogenes Profil.
- **Abnahme:** weniger Byte-/CPU-/Channel-Last ohne Verlust der qualifizierenden Evidence. **Commit/Evidence:** —.


### D7 — String-basierte Market-Keys und Retention-Cap verursachen unnötigen Speicher-/Coverage-Druck

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **MEDIUM / MEMORY + COVERAGE · CODE_CONFIRMED, CAP_EXHAUSTION_LASTTEST_NOT_RUN · G-C/G-D · SK-03/C0-E1.**
- **Pfade:** `runtime/src/market/core.rs::ObserverCore` (`seen`, `liquidation_signatures`, `targets`, `receivers`, `admit`), `market/decoding.rs` und `market/configuration.rs`/Econ-Kapazität.
- **Ist:** mehrere `HashSet/HashMap<String>`-Keys, Signaturen aus WSS/Logs oft im Stringformat; `admit()` und `admit_identity()` duplizieren Keys. Ohne Econ-Handle arbeitet Teile der Retention mit Fallback-Cap `100_000`, bei Überschreiten `C0_E1_MARKET_RETENTION_OVERFLOW`/Truth `retention_error`. Ob die Cap im konkreten 4h-Markt erreicht wird, ist zu **messen**, nicht zu behaupten. „Halber Speicher“ für typisierte Keys ist **ohne Messung keine gesicherte Zahl**.
- **Verbindlicher Fix:** Quelle/Typen/Subscribers analysieren, native `Signature/Pubkey` nur dann in Hotpath übernehmen, wenn kein JFI-/kompatibles Wire-Schema verletzt wird; Bound/Dedupe/TTL für gesamten Beobachtungszeitraum beweisen. Nicht einfach Cap erhöhen oder alte Signaturen evicten, weil das Markt-Wahrheits-/Dedup-Claims entwertet. Bei Overflow `INCOMPLETE` bleiben, kein stiller Drop.
- **Tests:** gleiche deduplizierte Signature aus zweierlei Quellen und Slots, mehr als 100k einzigartige TX, 4h-Projektionsprofil, p95 RAM/Bytes pro Key, Sort-/Hash-Parität, Cap-Gate, vollständige Receiver-/Target-Zähler.
- **Abnahme:** memory-bounded mit expliziter Coverage-/Retention-Qualifikation, keinerlei fälschliche Vollständigkeit. **Commit/Evidence:** —.
- **A1 (18.10.2026, Zahlen aus vorhandenen Läufen):** max. `unique_transactions` 1.682 (4h `c0e1_4h_20261008T222326Z`), 181 (5h `market5h_20261011T010758Z`); ×1,5 = 2.523 ≪ Cap 262.144 ⇒ `P0_ECON_RETENTION` unverändert. Das `retention_overflow` 293.482 des 10-min-Smokes `c0e1_smoke_20261008T212122Z` stammt aus der damaligen Evaluations-Retention (4h-Lauf danach 0), nicht aus der TX-Menge.


### D8 — Market-Capture-Thread wird nur bei bereits abgeschlossenem Lauf gejoint

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM / SHUTDOWN LIFECYCLE · CODE_CONFIRMED · G-A/G-C; VOR LANGEN LÄUFEN.**
- **Pfad:** `runtime/src/market/capture.rs::run`.
- **Ist:** Nach `session.request()` und Drain erfolgt `if ingress.is_finished() { ingress.join() }`; wenn der Thread beim Rücksprung noch lebt, wird der JoinHandle verworfen. Consumer-Drop mag später beenden, belegt aber keinen geordneten Shutdown/keine aktuelle Threadquiescence.
- **Verbindlicher Fix:** eigene Session-Cancellation und **nachweisbar bounded** Join-/Shutdown-Barriere mit dokumentiertem RPC/WSS-Timeout, rechtzeitige Abbruch- und Writer-Finalisierung. Da `std::thread::JoinHandle::join` selbst nicht timeout-fähig ist, keine irreführende `join_timeout`-Behauptung ohne entsprechende Architektur. Bei Überschreitung globalen Stop-/Drain-Budgets fail-closed `INCOMPLETE_ABORT`; niemals weiter laufenden Ingress als ruhend deklarieren.
- **Tests:** langsam endender Ingress, blockiertes Receive, StreamReady nicht erreicht, OperatorStop, Writer-Drain und FullQueue; Threadzähler und keine Emission nach Return.
- **Abnahme:** keine detached marktaktive Session nach kontrolliertem Capture-Abschluss. **Commit/Evidence:** `c256e9d` (`quiesce`: Ingress-Thread im Drain-Budget gepollt und gejoint, sonst `INGRESS_NOT_QUIESCENT` nach der Summary); Test `ingress_is_joined_or_reported_not_quiescent` Exit 0.


### D9 — synchrones stderr auf Capture-Thread kann Ingress-Verarbeitung blockieren

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM / BACKPRESSURE · CODE_CONFIRMED · G-A/G-C · Cross-Ref M2.**
- **Pfade:** `runtime/src/market/capture.rs` (Status alle etwa 2 s), `runtime/src/market/core.rs::on_logs` (pro erkannter Liquidation).
- **Ist:** Direktes `eprintln!` aus Capture-/Decode-Pfad kann bei vollem Pipe-/langsamen Dateisystem blockieren. Nur Shell-Umleitung auf Datei ist **kein vollständiger Beweis** für nichtblockierendes stderr.
- **Verbindlicher Fix:** bounded/try-send Diagnostik entkoppeln, strukturiertes essentielles Capture-Journal unverändert vollständig, optionale Terminalausgabe darf begrenzt gedroppt/aggregiert werden (Counter). Shutdown darf keinen blockierten Konsolen-Join abwarten (M2).
- **Tests:** blockierte stderr-Pipe, 1000 Liquiderfassungen, Writer-Backpressure, Terminal unavailable, kein erzwungener Ingress-HALT allein wegen optionaler Ausgabe, Summary/Counts identisch.
- **Abnahme:** Terminal-Diagnostik ist kein unbounded Capture-Hotpath. **Commit/Evidence:** `d41b70f` (`console::line`: begrenzte `try_send`-Konsole, detached Thread, Drop-Zähler; Capture-Status und Liquidationszeilen); Test `blocked_console_never_blocks_the_caller` Exit 0.


### D10 — verworfener JoinHandle beim StateOwner-Resubscribe

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · Duplikat von C6 · Cross-Ref C6/C1/M3.** ~~`spawn()` wirft den `JoinHandle` weg.~~ Gemeinsam mit C6 in `ad39019` geschlossen (eine Implementierung, dieselben Tests; siehe C6).
- **Pfad:** `runtime/src/orchestration/state_owner/ingress.rs::IngressSessions::spawn`.
- **Ist:** `spawn()` startet `ingress-ws`, wirft den `JoinHandle` weg und gibt nur `Shutdown` zurück. Beim Dependency-Resubscribe können alte/neue Session den gleichen Producer/Halt kurz gleichzeitig nutzen. Damit ist der zentrale Codebeleg von C6 bestätigt.
- **Verbindlicher Fix:** **C6 implementieren**, nicht zweiten Lifecycle-Mechanismus aufbauen; Session-Owner enthält Cancellation, Generation-ID, JoinHandle und geordneten vollständigen Shutdown. C1-/D8-Koordination ohne Session-Mischung.
- **Tests:** C6-Negativmatrix zusätzlich verifizieren: alte Session endet vor neuer, Altevents nach Neustart verboten, gemeinsame Halt-Generation korrekt, Last-/Repeated Resubscribe, kein detached Ingress. 
- **Abnahme:** ein implementierender Fix/Commit mit C6; beide Tickets erst nach demselben nachgewiesenen Test abschließen. **Commit/Evidence:** —.


### D11 — Alignment des Bank-Decoders wird als UNKNOWN_LAYOUT fehlklassifiziert

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · P1B/FE-01 · Cross-Ref B2/B11 (nicht B10).** ~~`try_from_bytes` weist unaligned Bytes als `UNKNOWN_LAYOUT: bank payload alignment` ab.~~
- **Pfad:** `liquidation/src/protocol.rs::decode_bank`.
- **Ist:** Schon korrekt geprüfte Länge/Discriminator werden durch `bytemuck::try_from_bytes(&data[8..])` bei unaligned Input trotzdem abgewiesen und als `UNKNOWN_LAYOUT: bank payload alignment` gemeldet. Die interne Speicheradresse ist keine On-Chain-Formatänderung. Ein blindes `pod_read_unaligned` kann jedoch die API (`&Bank`) verändern; sämtliche Aufrufer/Lifetimes/Crate-Parität müssen berücksichtigt werden.
- **Verbindlicher Fix:** Pinned-ABI-korrekter, alignment-unabhängiger, **strict length/discriminator/owner**-Decoder (owned POD-Read, falls möglich), ohne `0.1.12`-Prefix wieder einzuführen. Fehlerklassen für falschen Layouttyp und lokalen Buffer-Alignment-Fall trennen.
- **Tests:** echte pinned Bank-Fixture 8/16-Byte-offset unterschiedlich, 0.1.12 resized, falsche Gesamtgröße/Owner/Disc, Nachkommabits identisch in Health/Sizer/Dependency-Index, Parser-Panicfrei.
- **Abnahme:** gültige Bytes werden trotz unaligned Storage korrekt decodiert; unqualifizierte Versionen weiterhin UNKNOWN_LAYOUT. **Commit/Evidence:** `40a368f`: `decode_bank` → `Cow<Bank>` über `pod_exact` (ausgerichtet: Referenz, sonst `pod_read_unaligned`-Kopie); `BankLayoutError::Alignment` entfernt — Alignment ist Eigenschaft des lokalen Puffers, nie des On-Chain-Layouts. Längen-/Discriminator-/0.1.12-Klassifikation unverändert (`dual_size_bank_decoder_fails_closed`). Test `pinned_decoders_are_alignment_independent` Exit 0.


### D12 — Sweep kopiert Fehler-/Erfolgszweige und berechnet Fairness mehrfach

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / MAINTAINABILITY + HOTPATH · CODE_CONFIRMED · P6-MT, Cross-Ref D3.** · **PARTIAL** `119da22` · Test `d12_fairness_keys_are_computed_once_with_identical_order` (fail-before: Key je Vergleich neu ⇒ Aufrufzahl > n) — `order_by_fairness` berechnet den Fairness-Key genau einmal je Kandidat (vorher in `any()` und je Sortiervergleich); Reihenfolge identisch (eindeutige Keys mit Pubkey-Tiebreak), Rotations-Flag gleich. **Offen:** Paritätstests über Liquidatable-/Error-Pfad und deren Zusammenführung, Vor-/Nach-Zeitmessung.
- **Pfad:** `state_owner/sweep.rs`: `CandidateHealthState::Liquidatable/Error`, `candidate_fairness_key` in `any()` und in `sort_unstable_by_key()`.
- **Ist:** zwei lange nahezu gleiche Update-/Rearm-Pfade, Fairness-Schlüssel mehrfach pro Kandidat. Risiken: spätere auseinanderlaufende Error-/Liquidatable-Logik, vermeidbare CPU. **Nicht** als bestehender falscher Forwardingentscheid nachgewiesen.
- **Verbindlicher Fix:** zunächst Paritätstests über beide Pfade; gemeisame, rein funktionale Berechnung des Fairness-/Scheduling-Key pro Kandidat, danach optional Schrittausführung refaktorieren; kein stilles Zusammenlegen unterschiedlicher Retry-/Fault-Semantik. Vor-/Nach-Messung.
- **Tests:** Healthy/Liquidatable/Error, forced rearms, prior forward slot, revisions, ties, saturation, startup census, identische Reihenfolge/Counts, keine Starvation.
- **Abnahme:** weniger Duplikation/Schlüsselberechnung mit byte-/countergleichem Outcome. **Commit/Evidence:** —.


### D13 — Rayon-Pool-Initialisierung über RAYON_NUM_THREADS möglicherweise zu spät

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM / REVIEW_REQUIRED · G-A/G-C · P6-MT, Cross-Ref D3.** · NO_BUG_VERIFIED `16ec71a` · Test `d13_configured_workers_size_the_global_rayon_pool` (frischer Prozess je 1/2/4 Worker) — `startup::run` setzt `RAYON_NUM_THREADS` vor jeder Rayon-Nutzung (einziger Nutzer: Owner-Sweep); konfigurierte Workerzahl = globale Poolgröße. Kein eigener Pool. · Ergänzt `119da22`: Test `d13_rayon_is_used_only_by_the_state_owner` (Quellwächter: Rayon nur in `state_owner.rs`, `state_owner/sweep.rs`, `state_owner/tests.rs`, `startup.rs`). Scope: nur `runtime/src` gescannt; `solana-client`/`-perf`/`-tpu-client`/`-connection-cache` hängen von rayon ab, `liquidation` wird nicht gescannt.
- **Pfad:** `runtime/src/orchestration/startup.rs::run` (`RAYON_NUM_THREADS` per Konfiguration), `state_owner/sweep.rs::into_par_iter`, mögliche frühere `rayon::ThreadPoolBuilder`-/Global-Pool-Nutzung.
- **Ist:** `RAYON_NUM_THREADS` wirkt nur, solange der globale Pool nicht bereits von anderem Code initialisiert wurde. Die bloße Initialisierung von `provider::shared()` ist **kein belegter Rayon-Initialisierer**; möglicher Laufzeitkonflikt muss mit gezieltem Test/Instrumentation verifiziert werden.
- **Korrigierter Auftrag:** Nur Reihenfolge und echte Poolgröße in separatem Prozess messen; kein eigener Pool ohne nachgewiesenen Fehler.
- **Tests:** Prozessstart mit `workers=1/2/4`, vorherige bewusste Rayon-Initialisierung, tatsächliche Workerzählung und Threadnamen, deterministischer Lastoutput, Parallelitätsgrenze, Fehler bei widersprüchlicher Initialisierung statt stillem Ignorieren.
- **Abnahme:** angeforderte Poolgröße wirksam und messbar, keine unqualifizierte zusätzliche Parallelität. **Commit/Evidence:** —.

**Keine zusätzlichen D-Tickets für als sauber berichtete Grundlagen:** Pin-/Seed-/Discriminator-/Loader-v3-Funktionen in `protocol.rs`, allgemeine Funding-Klassenlogik außer D2 sowie Capture-Window-/Cutoff-/Drain-Grundlogik. Das schließt B2/D11-Decoderprovenienz, D1-Manifest-Wahrheit oder C1/C7-Ingressabschluss **nicht**.
- **Triage 18.10.2026:** `WITHDRAWN / NO_BUG_EVIDENCE_IN_CURRENT_STARTUP`. Fable zieht die Hypothese zurück: die operationalen Env-Werte werden in `startup::run` vor `run_shadow` angewendet. Kein belegter früher Rayon-Pool-Init. **Keine** globale/zweite Poolumstellung ohne Gegenbeispiel. Falls echte Workerzahl relevant: nur Mess-/Prooftest; kein Codefix als Pflicht.

- **Nachprüfung 18.10.2026:** `NO_BUG_CANDIDATE`: Konfigurationsstart setzt `RAYON_NUM_THREADS` vor `run_shadow`/Sweep. Legacy-Shadow ohne CLI verwendet Shell-Variable. **Kein neuer Pool** ohne reproduzierten Fehler; separater Prozess mit workers=1/2 und tatsächlicher Rayon-Zahl als Abschluss-Evidence.

## 9. E — fünfter Reviewdurchgang: Decoder, tatsächliche TX-Gates, BOOT und Projektionen

**Review-Stand:** 2026-10-18 · GitHub-`main` `81b667a14d68d0418e03e846c79ae8c698b31c37`. Fables Befunde zu `liquidation/{planner,projection,index,integration}.rs` und `orchestration/{control,winner_tracker,boot_audit}.rs` wurden mit GitHub-Quellcode, `runtime/execution/{builder,pipeline}.rs` und `state_owner/detect.rs` abgeglichen. **Keine neuen Regressionstests ausgeführt.** Hier dokumentierte `REVIEW_REQUIRED`-Fälle sind Prüfpflichten, kein behaupteter reproduzierter Laufzeitfehler. E1/E2 sind belegte Source-Invarianten, nicht bloß Performanceideen. Keine Änderung an LIVE-, Provider-, FE- oder Quota-Freigaben.


### E1 — Integration umgeht den strikten 0.1.12-Bank-Layout-Decoder

**Abgleich:** Abgehakt; Versionsfixtures fehlen.

- [x] **HIGH · LOCAL_PASS · G-A/G-E · P1A/P1B/FE-01; Cross-Ref B2/B11/D11.** ~~`kinds/pricing_kinds/refresh_plan_for_account` decodieren Banks per `pod_after_disc::<Bank>` (Prefix).~~
- **Pfade:** `liquidation/src/integration.rs::{pod_after_disc,kinds_for_account,pricing_kinds_for_account,refresh_plan_for_account}`; `liquidation/src/protocol.rs::decode_bank`.
- **Ist:** `pod_after_disc<T>` verlangt nur `data.len() >= 8+size_of::<T>()` und decodiert den **Prefix** `data[8..need]`. `kinds_for_account`, `pricing_kinds_for_account` und `refresh_plan_for_account` prüfen den Bank-Owner und Discriminator, benutzen aber danach dennoch `pod_after_disc::<Bank>` statt `protocol::decode_bank`. Daher kann eine verlängerte/auf 0.1.12 resized Bank **entgegen P1B-02** akzeptiert und als gültige Integration/Refresh-Quelle ausgewertet werden. Weitere MarginfiAccount-Eingänge derselben Funktionen verwenden ebenfalls `len < need` statt exakter Länge; derselbe Verstoß gegenüber B2. **Das ist source-belegt; ob ein bestimmter Mainnet-Bankdatensatz betroffen ist, bedarf qualifizierter Chain-Evidence.**
- **Soll-Fix:** **Einen einzigen strikten, versionsgepinnten Bank-Decoder** aus `protocol.rs` an sämtlichen Marginfi-Bank-Callsites verwenden; Owner, 8-Byte-Discriminator und die **exakte** für den aktiv gepinnten Crate zugelassene Länge sichern. `pod_after_disc<T>` nur für speziell geprüfte **externe** Integrationstypen mit je eigenem Layoutvertrag benutzen, nie für `Bank`/`MarginfiAccount`/`MarginfiGroup`/`FeeState`. Betroffene MarginfiAccount-Decoder im selben B2-Fix umstellen. Unaligned Buffer korrekt unterscheiden (D11); nicht durch Löschen des Strict-Checks „reparieren“. Unknown version/layout ⇒ explizit `UNKNOWN_LAYOUT`/no-plan/Stop, niemals stilles Prefix oder scheinbar valider Preis.
- **Tests RED→GREEN:** 0.1.11-Bank-Fixture + 0.1.12-resized Bank-Prefix + falscher Owner/Discriminator + zwei Byte weniger/eins mehr + unaligned Slice; durch **alle drei** Integration-API-Pfade (Kinds, Pricing, Refresh) sowie Health/Sizer/Builder/Index; unbekannte Version überall gleicher fail-closed Verdict, identische Source-/Pin-Provenienz. Kein RPC und keine LIVE-Aktion.
- **Abnahme:** `grep`-/Callsite-Inventur enthält keinen Bank-Prefix-Decoder mehr; B2/P1B-Rundung/Health-/Oracle-/Refresh-Parität nachgewiesen. **Commit/Evidence:** `40a368f`: Bank in allen drei Integrationspfaden über `protocol::decode_bank` (`INTEGRATION_BANK_LAYOUT:<bank>:<Grund>`), MarginfiAccount in Kinds/Pricing/Refresh/Projection über `decode_marginfi_account` (`*_LAYOUT`); `pod_after_disc` dokumentiert nur für externe Integrationstypen. Test `marginfi_layouts_are_exact_on_every_decoder_path`: resized 0.1.12-Bank ⇒ `INTEGRATION_BANK_LAYOUT … 0.1.12`, Bank +1 Byte, Account +1/+8/−2 Byte auf allen drei Pfaden UNKNOWN_LAYOUT; exakte Layouts unverändert OK. **Rest:** P1B-Rundungs-/Oracle-Parität mit echten 0.1.12-Fixtures NOT_RUN.


### E2 — Planner validiert handgeschriebene Steps, nicht die reale Receivership-TX

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **HIGH / TX-ADMISSION · CODE_CONFIRMED · G-A/G-E/G-F · P5/FE-04/FE-06; kein LIVE vor Schluss.** · Fix `16ec71a` · Tests `e2_steps_are_derived_from_built_instructions`, `e2_built_receivership_derives_its_true_steps`, `e2_pipeline_rejects_plans_by_their_built_instructions` — Gate klassifiziert die tatsächlich gebauten Instruktionen (`planner::steps_of`: Programm + 8-Byte-Discriminator, gepinnte marginfi-IDL- und Integrations-Refresh-Discriminatoren in einer `InstructionTable`); `ReceivershipPlan.steps` entfernt; unbekannter Discriminator auf Receivership-/Integrationsprogramm ⇒ `UnknownInstruction`; Pre-Refreshes und InitRecord jetzt geprüft; Prerequisites über `validate_bundle`; 0 Build/Sim/Send bei Verstoß. Jupiter-Instruktionen selbst nur programmgebunden (kein gepinntes Jupiter-IDL). · **Wieder offen (Review 20.10.), PARTIAL** — `119da22` ergänzt: `Step::InitLiquidationRecord` nur vor Start, Repay nur nach Withdraw, `compiled_matches` prüft Programm + Daten der signierten Message gegen die validierten Instruktionen (ComputeBudget-Präfix) nach jedem Receivership- und Prerequisite-Build; Tests `e2_steps_are_derived_from_built_instructions`, `e2_compiled_message_is_checked_against_the_validated_instructions`, beide fail-before per Mutation. **Offen:** Swap-Programme ohne Discriminator-Klassifikation (`planner.rs:114`, kein gepinntes Jupiter-IDL); Prerequisite-`Other` (System/Token/ATA/ComputeBudget) nur programmgebunden (`planner.rs:116`); `compiled_matches` vergleicht keine Account-Metas und keine LUT-Auflösung; ComputeBudget-Präfixdaten ungeprüft.
- **Pfade:** `liquidation/src/planner.rs::validate_receivership/validate_bundle`, `runtime/src/execution/builder.rs::receivership`, `liquidation/src/receivership.rs::build_receivership`, `runtime/src/execution/pipeline.rs::evaluate`.
- **Ist:** Builder erzeugt echte `Vec<Instruction>` über `build_receivership(ReceivershipParams { init_record, pre_refreshes, withdraws, swaps, repays, ... })`, erstellt aber **danach unabhängig** `steps=[StartLiquidation, Withdraw, route_steps…, Repay, EndLiquidation]`. Pre-Start-Refreshes und Initialisierung des LiquidationRecords fehlen darin; die `Step`-Regeln für erlaubte Prefixe, End als letzte Instruktion und Instruktionssequenz werden so **nicht über den tatsächlichen Input** geprüft. `Pipeline` ruft `validate_receivership(plan.steps, ...)` vor dem TX-Build auf. Die zusätzliche tatsächliche Program-ID-Allowlist hilft, prüft aber nicht die **zeitliche Reihenfolge, Instruktionsdiscriminator und Start/End-Anzahl**. Dass der derzeitige `build_receivership`-Output tatsächlich regelwidrig ist, ist **nicht belegt**; die unabhängige Gate-Kette ist nicht wirksam.
- **Soll-Fix:** Sequenz/Discriminator/Programm aus den **realen und vollständig gebauten** `Instruction`-Daten der Receivership-TX sowie aller Prerequisite-TXs/Bundleelemente ableiten und eine **einzige** Gateprüfung unmittelbar vor der signierten Payload durchführen; Program+Instruction-Daten **gemeinsam** klassifizieren, sonst fail-closed. Erkennt genau eine Start-/End-Kombination in genau einer TX, End als tatsächlich letzte erlaubte Instruktion (nur explizit gepinnte Ausnahmen, falls On-Chain nachgewiesen), richtige Prefix-Refresh-/Oracle-/InitRecord-Positionen, gültige Swap-/Withdraw-/Repay-Reihenfolge, Top-Level-/Prerequisite-Allowlist, LUT-program resolution und geplanten SubmissionPath. Für unbekannte Diskriminatoren/Programme kein generischer „Other allowed“ als Umgehung der Struktur. `Step` bleibt höchstens abgeleitete Diagnose, **nicht** unabhängige Eingangsquelle. FE-04 SignedPayload-/Approval-Bindung (B1) darf dieses Gate nicht umgehen.
- **Tests RED→GREEN:** tatsächlich gebaute Instruktionsliste mit `End` nicht zuletzt; Doppel-Start, fehlendes End, unzulässige PreStart-/AfterEnd-Instruktion, fremder ProgramID mit vermeintlich korrektem Discriminator, unbekannter Discriminator auf erlaubter ProgramID, verbotener Swap, falsches Bundle-Splitting, LUT-Auflösung, legitime gepinnte Kamino/Drift/JupLend-Refreshes, init_record yes/no, Token-2022 und repay_all, Prerequisite separat. Mock-Signer/Submitter beweist **0 Sendeaufrufe** bei Verstoß. Shadow-/Fixture-Parität.
- **Abnahme:** das Gate entscheidet ausschließlich auf tatsächlichem Build-/Sign-/Submit-Payload; keine zweite manuell zusammengesetzte „immer gültige“ Schrittfolge. **Commit/Evidence:** —.


### E3 — Drift-/JupLend-Refresh-Discriminatoren nicht bytegenau getestet

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · G-A/G-E · P1A/P1B/FE-01.** ~~Drift-/JupLend-Discriminatoren werden nur auf Länge 8 geprüft.~~
- **Pfad:** `liquidation/src/integration.rs::{drift_refresh,juplend_refresh,tests::pinned_refresh_discriminators_match_expected}`.
- **Ist:** Die fest kodierten Drift- und JupLend-Discriminatoren werden im Test **nur auf Länge acht** geprüft. Kamino wird per `global:refresh_reserve/refresh_obligation`-Hash gegen feste Bytes getestet. Drift-/JupLend-Fehlbytes könnten daher den Test bestehen und erst als fehlgeschlagene Simulation/IntegrationRefresh auffallen. Die jeweiligen Fremdprotokolle müssen **nicht** denselben Hash-Präfixalgorithmus wie Anchor-Kamino verwenden.
- **Soll-Fix:** Fest kodierte Discriminatoren gegen **gepinnten echten Instruction-Source/IDL/Mock-Crate** byteweise testen; nur bei belegtem Hash-Algorithmus berechnen, sonst geprüfte Konstanten exakt vergleichen und Source-Commit referenzieren. Live-Refresh-Build und Builder-/Instruction-Provenienz in denselben Test einbeziehen; bei P1B-Upgrade Fingerprint/Pin prüfen.
- **Tests:** exakte erwartete acht Bytes pro Protokoll und Instruktionsart, mutiertes Byte muss FAIL erzeugen, inkompatible Deployment-/IDL-Version ⇒ fail-closed, kein bloßer `len()==8`-Test.
- **Abnahme:** signierbarer Refresh ist durch Source/Fixture begründet, nicht durch eine 8-Byte-Länge. **Commit/Evidence:** `1a25c5f` (gemeinsam mit E11): alle vier Refresh-Discriminatoren (Kamino `refresh_reserve`/`refresh_obligation`, Drift `update_spot_market_cumulative_interest`, JupLend `update_rate`) stammen aus den IDLs des gepinnten marginfi-Quell-Commits `33c67987` (`idls/{kamino_lending,drift,juplend_earn}.json`, über `declare_program!` der gepinnten Mock-Crates als `anchor_lang::Discriminator`-Konstanten); Bytes unverändert, gleich IDL-Array und Anchor-Regel (Algorithmus damit belegt). Tests `pinned_refresh_discriminators_match_expected` (exakte IDL-Bytes, gekipptes Byte ⇒ FAIL) und `refresh_builders_emit_the_pinned_idl_instructions` (Live-Builder Drift/JupLend: Programm, Daten, Account-Reihenfolge und Writable-Flags wie IDL) Exit 0 (`evidence/HARDENING_20261018/after_E3_E11_tests.txt`). **Grenze:** Parität zur *deployten* Mainnet-Version der Fremdprogramme NOT_RUN (BOOT/FE-01).


### E4 — BOOT-02 hat keinen aktiven Journal-Writer-Probe

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH · LOCAL_PASS · G-A/G-B · BOOT-02/PERSIST/SK; Cross-Ref X3/M2.** ~~BOOT-02 prüft nur freie Bytes, schreibt nichts.~~
- **Pfad:** `runtime/src/orchestration/boot_audit.rs::offline`.
- **Ist:** BOOT-02 kontrolliert Runtime-Owner, unsealed Vorjournale, RAM und freie Diskkapazität, führt aber **keinen** isolierten Create→Frame-Write→Sync→Readback→Seal→Verify-Test des *tatsächlichen Run-Dateisystems* aus. Eine freie Diskzahl beweist weder aktuelle Schreibbarkeit noch Flush-/Sync-/Journal-Parität.
- **Soll-Fix:** Nur nach sauberer BOOT-Owner-/Pfadprüfung in einer **einmaligen eindeutigen Probe-Datei** auf dem tatsächlich konfigurierten Ziel-Volume eine native `.p0j`-Framekette erstellen, fsync/sync_data plus Verzeichnismetadaten und CRC/Hash/Längen-/Manifest-Readback/Seal entsprechend bestehendem Journalvertrag überprüfen; Probe-Datei nach Erfolg geordnet entfernen, bei Fehler Ursache und Artefakt für Diagnose sicher behandeln, niemals fremde/historische Journale verändern. Kosten/Sync-Semantik explizit messen und eine lokal belastbare Schreibfähigkeit statt globaler Garantie behaupten. Keine Mainnet-Reads, keine Keys.
- **Tests:** temp-volume writable/read-only/full/quota/permission, injected fsync/short-write/readback/manifest-failure, concurrent Owner und stale probe; BOOT bleibt `BLOCKED/UNKNOWN` bei unsicherem I/O.
- **Abnahme:** aktive Writer-Probe auf **richtigem Zielpfad** PASS; sonst BOOT-02 nicht PASS. **Commit/Evidence:** `af8050b`: `journal_writer_probe` (kritisch) erzeugt auf dem Volume des tatsächlichen Run-Roots (E6) eine eindeutige `.p0-boot-probe-<pid>-<ns>.p0j`, schreibt zwei typisierte Frames, `sync`, `seal(true)`, liest mit dem strikten `Reader` zurück (sealed+complete+Payload-Gleichheit), entfernt die Datei und synct das Verzeichnis; jeder Fehlschritt (CREATE/WRITE/SYNC/SEAL/READBACK/REMOVE) ⇒ BLOCKED, Probe-Datei auch nach Fehler entfernt (sonst Pfad im Grund). Dauer wird im Grund ausgewiesen. Test `writer_probe_passes_and_fails_closed` (beschreibbar ⇒ PASS ohne Rest, injizierter Sync-Fehler ⇒ `WRITER_PROBE_SYNC`, Read-only-Volume ⇒ `WRITER_PROBE_CREATE` und Audit BLOCKED) Exit 0 (`evidence/HARDENING_20261018/after_E4_E5_E6_tests.txt`). **NOT_RUN:** volle Platte/Quota, Short-Write, konkurrierender Owner während der Probe; die Probe belegt Schreib-/Sync-Fähigkeit *jetzt*, keine Kapazität für den ganzen Lauf (SK-06-Preflight).


### E5 — BOOT-Doctor scannt große alte p0j-Journale vollständig

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · G-A/G-B · BOOT-02.** ~~`unsealed_journals` liest jedes Journal bis zum Ende.~~
- **Pfad:** `runtime/src/orchestration/boot_audit.rs::unsealed_journals`.
- **Ist:** Für jedes `.p0j` der drei neuesten Runs pro Root wird `Reader::open` ausgeführt und in `while let Ok(Some(_)) = r.next_frame() {}` **bis zum Ende** gelesen. Ein langes Marktjournal kann Doctor/Admission deutlich verzögern; konkrete Minutenangabe ist erst mit realen Bytes nachzuweisen. Zudem klassifiziert die Schleife Lesefehler nicht getrennt, sondern beendet den Scan.
- **Soll-Fix:** Bounded *strukturelles* Seal-/Tail-/Manifest-Screening mit expliziter Max-Arbeits-/Zeitgrenze und Erkennung von beschädigtem/undecodierbarem Tail; **nicht** einen unverifizierten Header/Footer als vollständige Chain-Verifikation verkaufen. Volle CRC/Hash-/Frame-Parität gehört weiterhin zum separaten strikten Verifier oder zu gefordertem tieferen BOOT-Gate. Bei unklarem Ergebnis `UNKNOWN/WARN/BLOCKED`, keine unbesehene Annahme `SEALED`. Kein Löschen älterer Runs.
- **Tests:** 0-Byte/TornTail/corrupt sealed/unsealed, zig große Segmente, max drei Runroots, Budgetablauf und falscher Footer, vollständiger Checker findet absichtlich manipuliertes Middle-Frame. p50/p99-Doctor-Dauer und read bytes.
- **Abnahme:** begrenzte Doctor-Laufzeit **ohne** Falsch-PASS der Journal-Integrität. **Commit/Evidence:** `af8050b`: `journal::screen_seal` liest Header + höchstens 64 KiB Tail; „sealed“ nur, wenn der letzte Frame ein CRC/SHA-gültiges Manifest ist, dessen Frame-Zahl und Root zu eigener Sequenz und Vorgänger-Hash passen; sonst `Unsealed` bzw. `Undecodable(<Klasse>)` (OPEN, HEADER_TRUNCATED, JOURNAL_VERSION_OR_MAGIC, TAIL_READ, TAIL_UNDECODABLE, MANIFEST_MISMATCH). Bericht ausdrücklich „structural seal screening (no chain verification)“; Cap 4096 Journale pro Audit, Erreichen ⇒ WARN mit Hinweis. Test `seal_screening_is_bounded_and_structural_only` (sealed complete/incomplete, offen, leer, 64×60 KiB groß, letzter Frame > Fenster ⇒ nie sealed, Torn-Manifest, Null-Tail, Fremddatei, zu kurz; manipuliertes Middle-Frame: Screening „sealed“, voller `Reader` ⇒ Fehler) Exit 0. **NOT_RUN:** reale Doctor-Dauer p50/p99 auf Mainnet-Run-Daten.


### E6 — BOOT-Diskreserve misst Projekt-data statt tatsächlichen Run-Pfad

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · G-A/G-B/G-C · BOOT-02/SK.** ~~Speicherprüfung auf `project/data` statt auf dem Run-Root.~~
- **Pfad:** `runtime/src/orchestration/boot_audit.rs::offline` (`let data=project.join("data")` und `free_disk_bytes(data|project)`); `OfflineInputs::for_project`.
- **Ist:** Speicherprüfung bezieht sich auf `project/data` (oder Projektwurzel), ohne jeden konfigurierten Output-Runroot auf getrennte Mounts/Symlinks/Volumes aufzulösen. Bei abweichendem `P0_DATA_DIR`/Runpfad können Reserve und späterer Capture-Writer ein anderes Dateisystem nutzen.
- **Soll-Fix:** Zielverzeichnis des tatsächlich gewählten Capture-/Journal-/Market-Output-Runroots gegen Config/CanonicalPath/Persist-Owner auflösen; Volume-ID und freie Bytes + Reserve/Quota am korrekten Ziel; optional alle getrennten Write-Volumes prüfen. Noch nicht vorhandenen Run-Unterordner über nächste existierende Elternstruktur desselben Volumes behandeln, keine symlink-/path-TOCTOU-Umgehung. Zusammen mit E4 Writer-Probe und Storage-/Capacity-Preflight.
- **Tests:** Projekt und Run auf verschiedenen Mounts/Mock-filesystem IDs, Symlink/RelativePath, fehlendes Verzeichnis, FreeSpace knapp, Writer-Fehler trotz viel freien Bytes; P0_FREE_ONLY unverändert.
- **Abnahme:** BOOT-Prüfung qualifiziert das Volume, auf das tatsächlich geschrieben wird. **Commit/Evidence:** `af8050b`: `econ_run_root` löst den Run-Root wie der gepinnte Launcher-`run_store.resolve_store` auf (`P0_ECON_OUT_DIR` mit `~/`, sonst `data/econ4h` bei `P0_RUN_STORE=legacy`, sonst `data/runs`; ungültiger Store ⇒ Fehler); `disk_reserve` misst per `statvfs` am kanonisierten nächsten existierenden Vorfahren (Symlink aufgelöst) und nennt das Volume; unauflösbarer Root ⇒ `disk_reserve`/`journal_writer_probe` UNKNOWN ⇒ Audit BLOCKED. Test `run_volume_resolves_like_the_launcher` Exit 0. `run_store.py` unverändert (gepinnt). **Grenze:** Market-Runs (`P0_MARKET_OUT_DIR`) laufen nicht über `p0ctl run start` und sind nicht Teil dieser Prüfung; getrennte Mounts nur über Symlink-Fixture getestet, keine echte zweite Partition.


### E7 — owner_state nimmt beim reinen Statuslesen kurz den exklusiven Runtime-Lock

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW / REVIEW_REQUIRED: RACE-BEHAVIOR · G-A/G-B · BOOT-02/Control.** · Fix `07c3f09` · Test `e7_status_probes_never_fail_a_concurrent_start` (1000 Übergaben bei Dauer-Probe; ohne Fix rot) — kurzer blockierender Probe-Guard serialisiert Statusprobe und Owner-Erwerb: kein falsches `RUNTIME_OWNER_LOCKED`, kein halb geschriebener Record; Owner-Lock bleibt einzige Runtime-Exklusion.
- **Pfad:** `runtime/src/orchestration/control.rs::owner_state/acquire_owner`.
- **Ist:** `owner_state` öffnet das Lockfile und testet Owner-Anwesenheit durch kurzzeitiges `flock(LOCK_EX|LOCK_NB)`; ein parallel gestarteter `acquire_owner` kann genau währenddessen `RUNTIME_OWNER_LOCKED` melden, obwohl nur der Doctor kurz prüft. Das ist eine **kleine echte Konkurrenzmöglichkeit**, keine belegte Datenbeschädigung und darf nicht durch Entfernen des exklusiven Runtime-Locks „repariert“ werden.
- **Soll-Fix:** konkurrierende offline Doctor/Run-Starts gezielt reproduzieren; falls relevant, Statusprobes so kapseln/ordnen, dass weder falsche Live-Owner-Angaben noch unberechtigter Run-Fehlschlag auftreten. Strikte einzelne aktive Runtime beibehalten; ein retry/zweiter read-only Statuscheck darf keinesfalls zwei Owner zulassen.
- **Tests:** 1.000 deterministische Lock-Handovers, Parallel-Doctor und Start, zwei Runner dürfen nie gleichzeitig Lock besitzen, Drop/Crash-Freigabe.
- **Abnahme:** keine falsche Owner-Freigabe, Race-Befund gemessen/behoben oder belegbar unter kontrollierter Benutzerausgabe tolerierbar; kein Haken ohne Reproduktion/Evidence. **Commit/Evidence:** —.


### E8 — SIGINT/SIGTERM-Handler ohne SA_RESTART: tatsächliche I/O-Auswirkung prüfen

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / REVIEW_REQUIRED, NICHT automatisch Bug · G-A · S0/SK.** · **PARTIAL (Evidenz)** `119da22` · Test `e8_stop_signals_never_break_blocking_reads_or_journal_writes` — eigener Prozess, 50 × SIGTERM per `pthread_kill` an einen blockierten `read_exact` und an laufendes Journal-Append + fsync + Seal: Lesen liefert die vollständigen Daten, Kette versiegelt + vollständig, Stop-Flag gesetzt (std wiederholt EINTR). Kein Bug in diesen Pfaden. **Offen:** WSS-Recv (tokio/mio) unter Signal.
- **Pfad:** `runtime/src/orchestration/control.rs::install_stop_handler`.
- **Ist:** `libc::sigaction` wird mit Standard-Flags Null installiert, `SA_RESTART` fehlt. Auf Unix kann dies blockierende Syscalls mit `EINTR` beenden. Ob das **hier** schlecht ist, hängt von gewünschten Shutdown-Interrupts und der konkreten I/O-Retry-Logik ab; blindes Setzen von `SA_RESTART` könnte einen Shutdown sogar verlängern.
- **Soll-Fix:** Mit realer synthetischer Signal-/EINTR-Injektion Writer-/WSS-/Lock-/Reader-/Shutdownreaktion beweisen und dokumentieren; nur bei nachgewiesenem Fehler targeted EINTR-Retry oder angepassten Handlerflags, mit bounded Shutdown.
- **Tests:** SIGTERM während blocked read/write, WSS recv und Journal-Seal; Prozess exit/INCOMPLETE/Durability, keinerlei Endlosblockade.
- **Abnahme:** Signalstrategie nachgewiesen sicher; sofern kein Fehler, E8 als `NO_BUG_WITH_EVIDENCE` dokumentieren, nicht still zur Implementierungsaufgabe aufblasen. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `WITHDRAWN_AS_BUG`. Abwesenheit von `SA_RESTART` ist kein Defektnachweis; mögliche EINTR-Weiterleitung kann beabsichtigtes Stop-Verhalten sein. Keine Signalhandleränderung ohne dedizierten Fehlerfall und Shutdown-Parität.


### E9 — Projection-Cache hat keine vollständige Bank-/Oracle-/Revision-Provenienz

**Abgleich:** Offen/PARTIAL; Z3 und Z4 relevant.

- [ ] **MEDIUM / STALE-SHADOW-EVALUATION RISK · CODE_CONFIRMED, KONKRETER FEHLENTSCHEID NICHT REPRODUZIERT · G-A/G-E · P3/P6/FE-01.** · Fix `16ec71a` · Test `e9_dependency_fingerprint_tracks_every_dependency_revision` — Projektion an Fingerprint (Präsenz, Slot, Daten-Hash) aller Abhängigkeiten gebunden (Refresh-Instruktionskonten + Health-Observation-Konten), geprüft beim Ergebnis und bei Wiederverwendung; Änderung ⇒ stale, neue Projektion beim nächsten Trigger (`projection_dependencies_changed:`). NOT_MEASURED: Anteil verworfener Projektionen bei realer Oracle-/Bank-Kadenz (G-C). · **Wieder offen (Review 20.10.), PARTIAL** — Fehler von `observation_accounts` wird zur leeren Menge (`state_owner/detect.rs`, `unwrap_or_default`, → N3); Group, FeeState und Clock nicht im Fingerprint, obwohl Sizing Group-EMODE/FeeState liest; Root/ContextSlot-, ProgramData- und Chain-Clock-Bindung sowie bounded Requeue fehlen; Verwerfungsquote unter realer Oracle-Kadenz NOT_MEASURED (→ N4). Ein Snapshot über Simulations-Post-Accounts wurde verworfen: agave `simulateTransaction` lehnt mehr Post-Account-Adressen als Message-Account-Keys ab (Reviewer-Hinweis, lokal nicht verifizierbar; die Projektions-TX enthält nur die Refresh-Konten).
- **Pfade:** `runtime/src/orchestration/state_owner/detect.rs`: `projection_cache.remove(&acct).filter(|c| c.base_account_hash == base_account_hash && base_record.slot == slot)`; `liquidation/src/projection.rs::ProjectedState::account`; Integration-/Oracle-/Bank-Reverse-Index.
- **Ist:** Der Cache-Reuse validiert nur Hash und Trigger-Slot des **Kandidatenaccounts**. Andere Banken/Oracles und Integrations-/Reserve-Accounts können zwischen Projection-Worker-Snapshot und späterem Consume aktualisiert worden sein, ohne `base_account_hash` oder `base_record.slot` zu ändern. Das `ProjectedState`-Overlay fällt für nicht überschriebenen State auf den **heutigen** Base-State zurück, kann also Werte verschiedener Quellversionen zu einem gemeinsamen `projection_slot` kombinieren. Eine spätere Simulation ist keine verlässliche Korrektur für falsche Profit-/Ranking-/Telemetry-Entscheidungen.
- **Soll-Fix:** Projektionsresultat typisiert an **alle** für Health/Sizing/Refresh erforderlichen Bank-/Oracle-/Integration-Key-Revisionen, Root/ContextSlot, ProgramData-Pin und Chain-Clock binden; Reuse ausschließlich, wenn abhängige Revisions-IDs und Sourceidentität passen oder in einen konsistenten State-Snapshot re-projiziert wurde. Änderungsereignisse während laufender Remote-Projektion invalidieren den Cache, verwerfen veraltete Workerresultate und planen bounded neue Arbeit. Kein neuer Hotpath-RPC oder stilles Akzeptieren von gemischten Slot-Ständen.
- **Tests:** Candidate unverändert bei geänderter Bank/Oracle/Reserve/Root, Projection-Worker-Eingang gegen späteres Resultat, StaleOutOfOrder mit gleicher candidate_slot, Canary-PnL-/Health-Abweichung, Snapshot-/reorg-/ProgramUpgrade, idempotentes bounded Requeue. Beobachtetes `evaluation_slot` entspricht qualifizierter Gesamtevaluation.
- **Abnahme:** kein veraltetes Remote-Overlay mit neuen Dependency-/Oracle-Daten als eine konsistente Projektion bewertet; identische Parität bei stabilen Eingängen. **Commit/Evidence:** —.
- **Triage 18.10.2026:** `LATENT/RECORD-PROVENANCE, DESIGN_GATE`. Basisaccount-Hash+Slot validieren Dependency-/Oracle-Revisionen nicht. Eine spätere Simulation kann Stale/Drift erkennen, ist aber **kein** Beweis für ein korrektes `evaluation_slot` oder Ranking. Vor FE-Integrationsfreigabe Snapshotprovenienz testen; keine Aussage über erlittene Fehltrades.


### E10 — ReverseIndex lässt leere bank_to_accounts-Einträge zurück

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · LOCAL_PASS · P2/S1-LT.** ~~geleerte Bank-Sets bleiben als Keys stehen.~~ `6d03b18`: geleerte Sets werden entfernt; kein Verbraucher zählt Bank-Einträge. Test `emptied_bank_entries_are_removed` (Move, Remove, 1000 Wechsel ohne Wachstum, Lookup-Parität) Exit 0.
- **Pfad:** `liquidation/src/index.rs::ReverseIndex::set_account_banks/remove_account`.
- **Ist:** Wenn ein Account aus einer Bank-Set-Zuordnung entfernt wird, wird das `HashSet` leer, aber der HashMap-Key der Bank nicht gelöscht. Viele ehemalige Banks können Speicher-/Indexgröße aufblasen; funktionale Fehlweiterleitung ist **nicht** belegt.
- **Soll-Fix:** Nach Entfernen nur vollständig leere Reverse-Sets löschen; dabei Borrowing/Iterator-Lifetimes korrekt halten, keine aktive Referenz verlieren.
- **Tests:** add→move→remove, dieselbe Bank unter mehreren Accounts, leere Sets verschwinden, Lookup-/Iteration-Parität, langes Alternieren ohne wachsende leere Keys.
- **Abnahme:** bounded Index-Speicher ohne Routing-/Fanout-Verhaltensänderung. **Commit/Evidence:** —.


### E11 — Kamino-Discriminator wird pro Refresh-Aufruf neu gehasht

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · LOCAL_PASS · P6-MT.** ~~`ix_discriminator(...)` hasht pro Plan.~~ Geschlossen mit E3 in `1a25c5f`: gepinnte IDL-Konstanten statt Laufzeit-Hash, bitidentische Instruktionsdaten (Tests siehe E3). Allokations-/Timing-Profil nicht gemessen (nicht nötig: kein Hash mehr).
- **Pfad:** `liquidation/src/integration.rs::{ix_discriminator,kamino_refreshes}`.
- **Ist:** `ix_discriminator("refresh_reserve"/"refresh_obligation")` erzeugt zur Laufzeit Hash + `Vec<u8>` bei jedem Plan. Ob dies gegenüber Reserve-Decode/RPC relevant ist, ist zu profilieren.
- **Soll-Fix:** bereits byteweise getestete, gepinnte 8-Byte-Konstanten anstelle wiederholtem dynamischem Hash, sofern Quell-/IDL-Parität sichergestellt ist; keine ungetesteten Fremdprotokolldiscriminatoren aus Kamino-Hash ableiten.
- **Tests:** exakte Bytes, Source-Pin/Upgrade, Build-Instruction/Accounts bitidentisch, Allocation-Timing.
- **Abnahme:** weniger dynamische Arbeit, identische signierbare Instruktionen. **Commit/Evidence:** —.


### E12 — Kamino-Reserve wird pro Kandidat vollständig Borsh-deserialisiert

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / PERFORMANCE + PROVENANCE · CODE_CONFIRMED · P6-MT · Cross-Ref B7/C2.**
- **Pfad:** `liquidation/src/integration.rs::kamino_reserve/refresh_plan_for_bank/dependency_keys_for_bank`.
- **Ist:** Vollständige `Kamino Reserve::try_deserialize` für Oracle/Refresh-Planung plus weitere `MinimalReserve`-POD-Zugriffe auf dieselben Accountbytes. Kann bei vielen Kandidaten wiederholt erfolgen. Der Aufwand ist ohne Bench nicht quantifiziert.
- **Soll-Fix:** konservativer, versionierter Cache/Parser pro **identischem Reserve-Snapshot und Slot-/Write-Revision**, falls messbar sinnvoll; nur korrekt geprüfte Felder extrahieren, keine veralteten Oracle-/Farm-Keys. Ein universeller `Pod<Reserve>`-Cast ohne Version-/Owner-/Disc-Check ist ausgeschlossen.
- **Tests:** duplicate candidate batches, Oracle-/Reserve-Revision, verschiedene Kamino-Mocksource-Versionen, vollständige Dependency- und Byteparität, p50/p95 Allokationsprofil.
- **Abnahme:** identische Freshness-/Oracle-Provenienz und besseres gemessenes Profil. **Commit/Evidence:** —.


### E13 — fünfter Review ist kein vollständiger Verifier-/Finance-/Tooling-Audit

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **REVIEW_REQUIRED / COVERAGE-GATE · bleibt offen bis restliche Dateien geprüft.**
- **Gemäß fünftem Durchgang inhaltlich ohne neuen gesonderten Befund:** `winner_tracker.rs` Kapazität/TTL/revisionierte Watch-Verarbeitung; `projection.rs` Overlay-Mutation; `index.rs` Threshold-Bereiche; `control.rs` eigentliche `flock`-Owner-Mutual-Exclusion; `boot_audit.rs` existierende Token-Bindung/Offline-TOCTOU-Rechecks. Keine dieser Aussagen qualifiziert das ganze System.
- **Weiterhin ungelesen oder nur partiell:** `market/winner.rs`, `winner_tracker.rs` außerhalb der gelesenen Pfade, `chain.rs`, `econ*.rs`, `measurement.rs`, `report.rs`, `census.rs`, `lookup_tables.rs`, `market_*`-Verifier, `markers.rs`, `related.rs`, `rows.rs`, `summary.rs` (teilweise), `common/{mt0,histogram,timing,error,config}.rs`, `bin/{p0ctl,p0-journal,p0-strict-verify}` und Python-Verifier. Phase-/Risk-basierte künftige Prüfung; neue bestätigte Befunde als F-Serie, nicht hier unterstellen.
- **Abnahme:** vollständiger Scope-/Code-SHA-/Test-/Evidence-Bericht; relevante ökonomische Winner-/Census-/Verifier-Outputs gegen echte kanonische Chain-Truth-Fakten qualifizieren. **Commit/Evidence:** —.


## 10. F — weitere bestätigte oder prüfpflichtige Vertragslücken


### F1 — fehlende begrenzte Simulationslogs

**Abgleich:** Abgehakt; Q3 betrifft andere Debug-Datenpfade.

- [x] **MEDIUM / FE-02-DIAGNOSTIK · CODE_CONFIRMED.** · Fix `9fad87d` · Tests `f1_simulation_logs_are_bounded_redacted_and_never_in_debug`, `f1_simulation_failure_is_a_typed_owner_frame` — `SimResult::Failed(SimulationFailure)` mit Code, CU, ≤ 64 Zeilen à 256 B / 16 KiB, Redaction, Truncation; eigener Journal-Kind `OWNER_SIMULATION_FAILURE` (105); `Debug`/Record-Text nur der Code. Jito-Bundle-Simulation weiterhin nur Code.
- **Ist:** `submit/rpc.rs::simulate` speichert bei Fehler nur `SimResult::Failed("RPC_SIMULATION_FAILED ...")`, nicht `r.value.logs`.
- **Soll:** Typisierte `SimulationFailureEvidence` mit kategorisiertem Fehler, optionalen CU, redigierten Logs (z. B. höchstens 64 Zeilen à 256 Bytes, maximal 16 KiB) samt Truncation, nur im nativen Journal, nie im ungeschützten Display/stderr. Failed/Unavailable bleiben no-send.
- **Tests:** 0/65/1000 Zeilen, oversized, Secret-/URL-Sentinel, fehlende Logs, Backward Reader, keine unqualifizierte Send-Freigabe. **Commit/Evidence:** —.


### F2 — FE-04-Wallet-Journal schreibt produktiv noch Legacy-Schema-1-JSON

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH / CONTRACT_OPEN · JFI-01/02, FE-04/PERSIST.** · Fix `07c3f09` · Tests `f2_wallet_journal_writes_typed_frames_and_reads_historical_json`, `f2_wallet_transition_layout_is_pinned` — Wallet-Journal schreibt typisierte `Version`/`LegacyImport`-Frames (Schema 2, `append_typed`); FE04-Kinds nicht mehr legacy-schreibbar; Schema-1-JSON nur als lesbares Präfix alter Journale (danach `SCHEMA_ORDER`); bincode-Layout per Hash gepinnt.
- **Ist:** `one_shot.rs::Journal::persist` nutzt `serde_json::to_value`; `append` ruft `writer.append_legacy` auf. `.p0j`-Container allein macht Frames nicht zum typed JFI-Format.
- **Soll:** Bestehendes Wallet-`.p0j` auf versionierte native typed Transitions migrieren, historischen Schema-1-JSON-Leser beibehalten, keine zweite Ledger-Quelle, keine Störung von Fsync/UNKNOWN/Hold/Lock/Finality.
- **Tests:** historische Frames, Legacy-Import, Binary Roundtrip, truncated/CRC/UnknownKind, Crash/Replay, no-double-send, WalletLock und Sync-Failure. **Commit/Evidence:** —.


### F3 — Jito landed_slot=0 als Ersatz für Unknown

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · LOCAL_PASS · P8.** ~~`inflight_status` benutzt für `landed_slot` `unwrap_or(0)`.~~ Kern bereits in `47ebbc6` (L2) behoben: Landed ohne Slot ⇒ Pending. Rest `c85bb17`: auch ein gemeldeter Slot 0 (Genesis) ist keine Landung; Parsing als `inflight_entry_status` testbar. Test `inflight_landed_needs_a_real_slot` (fehlend/null/0/String/negativ ⇒ Pending, 321 ⇒ Landed, Failed, NotFound) Exit 0 (`evidence/HARDENING_20261018/after_E10_C9_F3_tests.txt`). Finalisierte Ledger-Beweise bleiben unabhängig davon Pflicht (C3/C4).
- **Ist:** `submit/jito.rs::inflight_status` benutzt für `landed_slot` `unwrap_or(0)`.
- **Soll:** `Option<slot>` oder unqualifizierter LandedStatus; finalisierte Ledgerbeweise bleiben erforderlich. Missing/null/0/realer Slot testen. **Commit/Evidence:** siehe oben (`47ebbc6`, `c85bb17`).


### F4 — WSS-Slotfortschritt vor Queue-Zustellung

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · S2/C0-E1.** ~~`last_slot`/`slots.observe()` vor `sink.try_push(ev)`.~~
- **Ist:** `ws_rpc.rs` aktualisiert `last_slot` und `slots.observe()` vor `sink.try_push(ev)`; nach Queue-Full beschreibt Gap.from_slot womöglich nie zugestellte Daten. HALT ist korrekt.
- **Soll:** Wire-observed und tatsächlich Owner-delivered getrennt führen; Gap vom letzten belegten zugestellten Slot; kein falscher Root/Ready-Fortschritt. Queue-Full/Slot/Account/Reconnect mit bounded Fake-Ingress testen. **Commit/Evidence:** `52e20c2`: `deliver()` schreibt `last_slot` (Gap-Ursprung, auch für das Session-Fehler-Gap) und den geteilten `SlotWatch` erst nach erfolgreicher Zustellung fort; Queue-Full bleibt HALT + Gap-Versuch (fail-closed), getrennter Owner beendet die Session. Test `slot_progress_follows_queue_delivery` (Queue-Kapazität 2: abgewiesener Slot 20 bewegt weder `last_slot` noch `SlotWatch`, Halt gesetzt; Disconnect) Exit 0; `solana ingress::` 20 PASS, Clippy `-D warnings` (`evidence/HARDENING_20261018/after_F4_tests.txt`). Session-Ready-Fortschritt (`progress_slot`) ist session-lokal und endet mit dem Overflow-Fehler; Reconnect-Pfad mit echtem WSS NOT_RUN.


### F5 — RNG-/UNTESTED-Hygiene

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · LOCAL_PASS · `3d0ef10`.** ~~doppeltes `random16`, pauschale `UNTESTED/VERIFY`-Kommentare.~~ `storage/segments.rs` nutzt `journal::random16` (bytegleiche Kopie entfernt: 16 Byte `/dev/urandom`, gleiche Fehlersemantik). Statuszeilen in `submit/{mod,rpc,jito}.rs` und `ingress/ws_rpc.rs` nennen jetzt die tatsächlich vorhandenen Offline-/Loopback-Tests und markieren reale Provider-/Endpoint-Antworten weiterhin als VERIFY/NOT_RUN — keine Produktivqualifikation behauptet. `storage::segments`/`analytics::journal` PASS, Clippy `-D warnings` (`evidence/HARDENING_20261018/F5_tests.txt`).
- **Ist:** mehrfach `random16` laut Review, Kommentar `UNTESTED/VERIFY` in RPC/Jito/WSS, obwohl einzelne Teiltests existieren. Kein pauschaler funktionaler Fehlernachweis.
- **Soll:** Duplikate nur bei identischer Sicherheits-/Entropy-Semantik zusammenführen; Kommentare an tatsächliche Evidence anpassen. Keine fiktiven PASS-Bezeichnungen. **Commit/Evidence:** —.


### F6 — Reporter-Commit `311f9a4` bricht `cargo fmt --check` (Gate G-A)

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · LOCAL_PASS · `3b2e8c0` (mit ausdrücklicher Nutzerfreigabe 18.10.2026, hebt „nur der Autor“ auf).** ~~Vier fremde Reporter-Dateien unformatiert.~~ Nur `rustfmt` auf genau `reporter/src/bin/p0_market_capture_reported.rs`, `reporter/src/bin/p0_reporter_fixture.rs`, `reporter/src/env_file.rs`, `reporter/tests/config.rs`; whitespace-bereinigter Diff zeigt ausschließlich rustfmt-Normalisierungen (Match-Arm in Block, Import-Reihenfolge, Trailing-Kommas, `;` nach `continue`, Closure-Block). `cargo fmt --all --check` Exit 0, Reporter-Tests und Clippy `-D warnings` Exit 0 (`evidence/HARDENING_20261018/F6_format.txt`).
- **Nachtrag 18.10.2026:** seit `b462ba2` (fremder Gateway-/Reporter-Commit) ist zusätzlich `reporter/src/bin/p0_reporter_fixture.rs` unformatiert — damit **vier** fremde Reporter-Dateien; ebenfalls nicht von diesem Härtungsauftrag geändert.
- **Ist:** `cargo fmt --all --check` auf `ad39019` und erneut im Gate 2 auf `8eccd8f` (`gate2_fmt_check.txt`) meldet Diffs ausschließlich in `reporter/src/bin/p0_market_capture_reported.rs`, `reporter/src/env_file.rs`, `reporter/tests/config.rs` (eingeführt mit `311f9a4` „opt-in env-file loading“, anderer Autor). Alle Härtungsdateien sind formatiert. Der Formatter-Teil der Gate-Matrix ist damit bis zu einer Formatierung dieser drei Dateien **rot**; die übrigen Gate-Teile sind davon unabhängig.
- **Soll:** Der Autor/Owner von `311f9a4` formatiert die drei Dateien (`rustfmt`, keine Verhaltensänderung) oder der Nutzer gibt die Formatierung fremder Reporter-Dateien ausdrücklich frei. **Commit/Evidence:** —.

## 11. X — explizite Phasenplan-Vertragslücken; mit N-Tickets koordinieren


### X1 — BOOT-12 muss aktuelle Identitäten vor Runtime-Admission erneut prüfen

**Abgleich:** Abgehakt; Z10 neue, noch unbewiesene Rent-Sysvar-Hypothese.

- [x] **CRITICAL · LOCAL_PASS (Offline-Vertrag mit Fixtures; Netzwerkfall NOT_RUN, G-B bleibt offen) · `a3ca33c`.**
- **Ist:** `NetworkAudit::validate_start` bindet TTL, lokale Config/Binary/Hash/Token; `p0ctl run start` holt **keine** frischen On-chain-Programm-/Oracle-/Genesisbeweise am tatsächlichen Admission-Punkt. Einzelne Runtime-ProgramData-Rechecks sind noch kein vollständiger BOOT-12-Vertrag.
- **Soll:** *vor Admission* validierte aktuelle Genesis/Cluster-, Program-/ProgramData-/Version-, kontextkonsistente Bank-/Oracle-Beziehungen (frisch/identitätsgebunden) gegen den ursprünglichen Token/Spec; TOCTOU führt `BLOCKED`, keine automatische Re-Pin-/Paid-Fallback-Logik. Keine rückwirkende Akzeptanz eines geänderten Programms. Die genaue Reihenfolge mit BOOT-01/02 und Service-Owner-Locks belegen.
- **Tests offline:** geänderte Genesis, ProgramData-Hash, Upgrade-Slot, Oracle/Bank/Config/Binary, abgelaufener Token, schlagartig geänderter Provider. Netzwerkprüfung **NOT_RUN** bis separate Freigabe.
- **Dedupe:** N1/N6/N7/B2; nur **ein** identitätsgebundener Startvertrag. **Commit/Evidence:** Teil: `028c662` Genesis-Pin `GENESIS_HASH_MAINNET` vor ProgramData-Read/Admission (`BOOT_CLUSTER_MISMATCH`) und `--genesis-hash` = Pin (`BOOT_GENESIS_PIN_MISMATCH`); Tests `cluster_check_accepts_only_the_mainnet_genesis_pin`, `spec_rejects_genesis_that_differs_from_the_pin` Exit 0. Reale Prüfung NOT_RUN; frische Oracle-/State-Identitäten bei Admission offen.

- **Nachprüfung 18.10.2026 (Härtung, nach `652a91d`):** Am Startpunkt bereits geprüft und getestet: Token-Ablauf/-Manipulation, Binär-, Quell- und Capture-Source-Drift (`validate_start`, `native_network_record_and_start_token_reject_expiry_drift_and_tampering`), lokale Identität (`recheck`), Genesis-Pin (`setup::check_cluster`) und ProgramData-Pin (Runtime-Start, P1A-04). **Offen und Designentscheidung:** frische Bank-/Oracle-*Identitäts*bindung (Keys/Owner/Layout, nicht Preisdaten) der Runtime gegen die `NetworkAudit.accounts` — erfordert Übergabe des Audit-Ergebnisses über `p0ctl` → Launcher → `p0-longtail` und zusätzliche Start-Reads (Mainnet-Autorisierung). Nicht ohne Freigabe umgesetzt.
- **Nachprüfung 18.10.2026:** `setup::prepare` liest ProgramData, aber ursprünglich kein `getGenesisHash`; `boot_network::Spec::validate` prüfte nur Parseformat des Operator-Hashs statt gepinnten Erwartungswert. Claude hat in `028c662` bereits einen **lokalen Genesis-Pin-Fixkandidaten** mit BOOT12 Red/Green-Evidence geliefert. Nicht als vollständige BOOT-12-Acceptance abhaken: restliche dynamische Account-/Oracle-Proofs und verifizierter Admission-Pfad bleiben offen.
- **Rest geschlossen 18.10.2026 (`a3ca33c`, Design vom Nutzer freigegeben, LOCAL_PASS):** keine zusätzlichen Netzwerk-Reads; die Runtime bindet gegen den Zustand, den sie ohnehin liest.
  1. *Übergabe:* `p0ctl run start` gibt dem Launcher den kanonischen Pfad des freigegebenen `NetworkAudit`-Records, das 16-stellige Token-Präfix und die Freigabezeit (`P0_NETWORK_AUDIT`, `P0_NETWORK_AUDIT_TOKEN`, `P0_NETWORK_AUDIT_APPROVED_AT`).
  2. *Start-Gate (`startup::run`, vor jedem Thread und jedem Read; im econ4h-Prozess also auch vor dem Market-Observer):* Record fehlt/unlesbar ⇒ `BOOT_NETWORK_AUDIT_MISSING`/`BOOT_NETWORK_EVIDENCE_INVALID`; nicht READY, BOOT-03..08 fehlt oder manipuliert ⇒ `BOOT_NETWORK_NOT_READY`/`_CHECK_MISSING`/`_TOKEN`; Freigabe außerhalb der 60-s-TTL ⇒ `BOOT_NETWORK_EXPIRED` (die Runtime prüft die Wanduhr nicht erneut: Build, Preflight und StreamReady liegen zwischen Freigabe und Bootstrap); Drift von Quelle, Operational-Config, Safety-Flags, Schema, Binaries (p0ctl + p0-longtail), Capture-Source oder Genesis-Pin ⇒ `BOOT_IDENTITY_DRIFT: <Felder>`. Jeder Fall beendet den Start mit festem Code (`shadow_failure_code`), keine Re-Pin-/Fallback-Logik. p0ctl und Runtime lösen die auditierte Config mit derselben Regel auf (`boot_audit::start_config`: Operational-Args + `--duration`).
  3. *Bindung bei Admission (erstes `Ready`, vor `halt.clear()`):* ProgramData mit Owner, Länge, Data-Hash und `executable` (aus dem Start-Read); Runtime-Pflichtabhängigkeiten (`required_dependency_keys`: Group, Fee-State, Clock, Oracles, Mints, Integrationskonten) und auditierte Banks mit Owner und Länge (Daten dürfen sich innerhalb der TTL ändern); Abweichung oder Fehlen ⇒ `BOOT_IDENTITY_DRIFT`, HALT bleibt, Lauf endet (econ: `shadow_failed` + `abort`). **Außerhalb des Runtime-Scopes** (gemeldet, nie als gebunden gezählt): Programmkonto, Liquidity-Vaults, Ziele ohne Liabilities; liegen sie doch im Zustand, werden sie gebunden.
  4. TTL, FREE_ONLY, ObserveOnly und Provider-Budget unverändert.
  - **Tests (offline, nur Fixtures):** `x1_admission_refuses_expiry_tampering_and_every_identity_drift`, `x1_admission_record_from_the_launcher_environment`, `x1_bind_state_binds_runtime_scope_and_reports_drift_per_field_class`, `x1_start_config_is_identical_in_p0ctl_and_the_launched_runtime` (Profile standard/smoke/performance, Launcher-Exports), Owner-E2E `owner_binds_the_approved_audit_before_admission` (gebunden / ProgramData-Drift / fehlende Abhängigkeit); Mutationsprobe: ohne Bindung schlägt der E2E-Test fehl. 9 gezielt PASS, Runner-Crate 362 passed / 0 failed / 6 ignored, Clippy `-D warnings` 0 (`evidence/HARDENING_20261018/X1_tests.txt`).
  - **Betriebshinweis:** Das Netzwerk-Audit bindet den Digest von p0ctl **und** p0-longtail. Baut der Launcher nach dem Audit ein anderes p0-longtail, endet der Start mit `BOOT_IDENTITY_DRIFT: binary` — vor dem Netzwerk-Audit alle Binaries bauen.
  - **Folgefix `1d9c6ed` (aus Gate 4):** econ4h verweigert nach dem lokalen Manifest/BOOTSTRAP-Eintrag und **vor** beiden Owner-Threads (`refuse_before_owners`: kein Netzwerkkontakt, Analytics endet geordnet, Phasenjournal INCOMPLETE, BOOT-Code wörtlich); Shadow-only weiterhin vor `prepare`. Die zwei Launcher-E2E-Tests prüfen jetzt den X1-Vertrag (Verweigerung mit `BOOT_NETWORK_AUDIT_MISSING`, kein Market-Journal, kein Warmup) statt des alten Starts ohne Record; alle übrigen Assertions unverändert (`evidence/HARDENING_20261018/X1_launcher_refusal.txt`). Damit offline **nicht mehr** abgedeckt: der echte Owner-Warmup-/Market-Journal-Pfad über den Launcher — er setzt einen freigegebenen Record voraus (G-B); Market-INCOMPLETE bleibt durch die Rust-Tests (D1/R2) belegt.
  - **NOT_RUN:** realer Record gegen Mainnet-Zustand (G-B, keine Freigabe).


### X2 — JFI-01/02-Verstoß im neuen BOOT-/Provider-Evidence-Pfad

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **HIGH · G-A/G-B.**
- **Ist:** `boot_network.rs` hat `Reads::rpc(...)->serde_json::Value` bis in `cluster` und Scope-Prüflogik, außerdem Produktiv-Import einer JSON-Provider-Evidence-Datei. Das überschreitet die explizite Solana-Wire-Ausnahme.
- **Soll:** RPC-Responses **unmittelbar im Wire-Adapter** auf native, validierte DTOs abbilden; `Reads` mit typisierten `genesis/version/health/slot/multiple_accounts`; historische JSON-Proofimporte ausschließlich isoliert offline. Neuer produktiver Provider-/Network-Audit-Proof als versioniertes typisiertes `.p0j` im **bestehenden** Journalvertrag, ohne neue parallele Evidenz-Datenbank. FREE_ONLY-Provenienz und tatsächlich beobachtete Quoten separat qualifizieren.
- **Tests:** typed Roundtrip, invalid Owner/Layout/Slot, JSON-Leak-Scan außerhalb Wire/Legacy-Leser, Schema/Version/Corruption/Truncation, historischer Reader bleibt byteidentisch.
- **Dedupe:** N1 und PERSIST/JFI-FIX-04–07; keine neue Umsetzungsphase. **Commit/Evidence:** `2599f01` (typisierter `Reads`, JSON nur in `NetworkReads`-Wire-Kante; `PROVIDER_EVIDENCE` Kind 711 über `p0-journal import-provider-evidence`); Tests `provider_evidence_round_trips_as_typed_journal`, `network_reads_decode_at_the_wire_edge`, BOOT-Fixture-Tests Exit 0.


### X3 — BOOT-01/02 nicht nur als Offline-Token qualifizieren

**Abgleich:** Abgehakt lokal; Netzwerk-Gate separat offen.

- [x] **HIGH · LOCAL_PASS (Offline-Matrix; Netzwerkteile NOT_RUN) · G-A/G-B.**
- **Ist:** Offline-Identitäts-/Ownerprüfungen teilweise vorhanden, aber vollständige Version-/Schema-/Writer-Probe-/Crash-Recovery- und Betriebsbereitschaft sind nicht nachgewiesen.
- **Soll:** gemäß `STARTUP_BACKTEST_OPTIMIZATION.md` alle kritischen lokalen Binary-/Decoder-/Journal-/Owner-/Lock-/Clock-/RAM-/Disk-/Writer-Checks mit dokumentiertem Scope, `PASS/WARN/BLOCKED/NOT_RUN`; externe BOOT-Schritte offline zwingend NOT_RUN. Keine implizite READINESS durch Einzelpass.
- **Tests:** korrupte Journalversion/Writer-Probe, Doppel-Owner, fehlende Binärdatei, Disk-/RAM-Untergrenze, Config-Drift; keine echte Netzwerkaktivität.
- **Dedupe:** M1–M3/N1–N3/PERSIST/SK. **Commit/Evidence:** Teil `af8050b` (E4 Writer-Probe, E5 Seal-Screening, E6 Run-Volume in BOOT-02), Rest `652a91d`: neuer kritischer BOOT-01-Check `runtime_binary` (Digest von p0ctl + p0-longtail über `binary_identity`; fehlt ⇒ BLOCKED, bisher erst bei `run start`), Test `boot_01_02_offline_matrix`: gesunder Host ⇒ alle lokalen Checks PASS, sechs Netzwerkchecks NOT_RUN, Urteil `OFFLINE_READY_NETWORK_NOT_RUN` (nie READY); fehlende Runtime-Binary, lebender Runtime-Owner, Disk-Reserve über freiem Run-Volume, Capture-Max + Systemreserve über physischem RAM, aktivierte LIVE-Flags ⇒ BLOCKED; korruptes Journal eines früheren Laufs ⇒ WARN (Crash-Evidence bleibt). Writer-Probe/Read-only (E4), Run-Volume (E6), Seal-Screening (E5), Read-Budget (R4) und Token-Drift (`recheck`) mit eigenen Tests. `boot_` 15 PASS, Clippy `-D warnings` (`evidence/HARDENING_20261018/X3_tests.txt`). **NOT_RUN:** echte Platte voll/Quota, Netzwerkschritte BOOT-03–08.

## 12. Reihenfolge und Abhängigkeiten (kein Roadmap-Rewrite)

**AUFTRAG 1 – verbindliche Codehärtung und Evidence-Wahrheit vor 4h (Offline-Code, keine Mainnet-Läufe):**

**Nachprüfung 18.10.2026 / zusätzliche 4h-Regressionen:** R1 (WSS Shutdown→Drain innerhalb 120s), **R2 (Panic vor COMPLETE-Manifest erkennen)**, **R3 (Recovery-Ready→FullSweep inklusive Ingress-Backpressure)** und R4 (dynamische BOOT-Budgets offline; Live-Netzaudit bleibt gesperrt) zu den vorhandenen Gates aufnehmen. R5–R9 sind LOW/PROFILE/CLEANUP ohne automatische 4h-Sperre. **R10 (Cancel darf Econ nicht falsch aborten), R11 (altgenerationales Fatal differenzieren)** vor 4h per Offline-Regression absichern. R1 umfasst explizit das Shadow-Join-Budget; R6 auch Ingress-Thread-Exit-Logging. **Gate 3 auf neuem finalem HEAD ist Pflicht:** `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`, `cargo build --release --all-features`, notwendige Python-Suite und Featureprofile, SHA + Exitcodes. `fmt` bleibt mit F6 formal BLOCKED, bis **der Autor** die drei `reporter`-Dateien formatiert hat; dieser Auftrag verändert sie nicht. Kein `cargo fmt` über fremde Dateien. F5 umfasst Jito-VERIFY-Kommentare, F6 fremder Reporter-FMT-Befund. 

1. **C1/C7 + D1 + E4/E5/E6** (Recovery/Fatal, Market-Manifest, BOOT-Writer/Doctor/Run-Volume vor 4h), N1, N2, N3, M3, B5, B13 sowie X1/X2/X3 soweit ohne Mainnet umsetzbar. **D8/D9** als gezielte Capture-Härtung integrieren. **D4/D5 nur defensiv/nicht blockierend** nach den neuen Decoder-/Strict-Pfad-Erkenntnissen.
2. **D3-Stresstest vor 4h-Readiness**; bei **gemessener** unzulässiger Owner-Stall/Ingress-Overflow accountseitig gebundenes Chunking implementieren. Kein unbewiesener Architekturumbau. M1/M2 plus N4/N5/N6 zur Stabilität von Admission, Durability und Diagnose; keine neue Source-of-Truth.
3. N7/N8 nach Mess-/Sicherheitsvertrag, L4 und kleine isolierte L1/L2/L3; M5 nur profilieren; **B2/E1/D11** mit einheitlichem striktem Bank-Decoder für BOOT-/P1A-/P1B-Scopes; **E3** bytegenaue gepinnte Fremdinstruktionsdiscriminatoren.
4. Gesamte relevante Offline-Regression, Release-/Formatter-/Clippy-Nachweis, Redaction-Probes, Ledger-/STATUS-/4h-Readiness-Synchronisierung.
5. **Erst dann** getrennt autorisierter G-B-Audit; bei PASS gesondert G-C-Kalibrierung; G-D-vier-Stundenprüfung.

**Review 8 / 4h-Economics und Reporter (18.10.2026):** Vor G-C/G-D und der FE-00-Auswertung erst R12 (historische Retention-Zahlen), R14/R15 (Commitment-Metriken und Live-/Offline-Parität), R13 (Strict-Scan-Budget mit 600-s-Kalibrierung und konservativer Hochrechnung) nachweisen. R18/R19 sind relevant, bevor ein laufender 4h-Run über Frontend/Gateway als aussagekräftig dargestellt wird; sie ersetzen keine 4h-Backend-Admission. R16/R17 sowie R20–R25 bleiben vollständig erfasst, aber ihre Gatewirkung ist differenziert. R26 ist ein noch ausstehender Quellen-/Wahrheitsreview, kein bestätigter Fehler. Kein neues RUN-/LIVE-GO aus dieser Dokumentation.

**AUFTRAG 2 – Design-/Finanz-Gates vor Pilot, vor FE-06 und vor P8-LIVE:**
1. **B2/E1/D11, E2, E9, F1**, B4, B9 und ggf. L4 für die ausgewählte Fundingklasse; echte Candidate-/Source-/Oracle-/Rent-/Exit-/Plan-Instruktions-Parität (FE-01/02/04).
2. B6/B7/C2/C8/C9/D2/D6/D7/D12/E10/E11/E12 anhand belastbarer Timingdaten und Kandidaten-Shadow, nicht aufgrund einer geschätzten 4h-Profitrate (FE-03/P6).
3. **B1 + B3 + B15 + C3 + C4 + C5 + C11 + M6 + F2 + L1 + L2 verpflichtend** für finanzielle Single-Flight-/Wallet-/Limit-/Journal-/UNKNOWN-Gates (FE-04/P8). B10/B11/B12/B17 fachlich vor passendem Pilot prüfen; Ticket-Status bleibt sonst OPEN. C10/C12 werden im jeweiligen P8-/Error-Vertrag geschlossen.
4. FE-05 ausschließlich read-only und nur autorisiert; FE-06 gesonderter individueller Geldbeschluss, ein Versuch dann HALT; FE-07 nur mit finalisiertem tatsächlich realisiertem Nettoledger.

**Tranche C — separat beobachtete, nicht gestrichene Produktpflichten:** vollständiger C0-E1-Index/Crosscheck/Strict-Master, H0, Race/Binary-Geyser/Jito-Single/Bundle/gRPC, JIT-ExactIn, P6-MT/M4, P8-RES/Inventory/Rebalancing, K-Untersuchungen, P9 und Frontend bleiben gemäß §0.6/Phasenplan verbindlich. `NO_ECONOMIC_GO` stoppt nur nicht qualifiziertes LIVE, **nicht** die restliche Roadmap; kein Master-PASS oder erster Profit wird erfunden.

## 13. Vorlage für jeden zukünftigen Befund und die Erledigung

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

## 14. Aktuelle Abnahmeentscheidung

- **G-A:** **OPEN**. Lokal geschlossen (LOCAL_PASS mit Evidence): H1–H3, M1–M4/M7, N1–N6/N8, L1/L2/L4, B2/B5/B11/B13, C1/C6/C7/C9, D1/D3/D4/D5/D8–D11, E1/E3–E6/E10/E11, F3–F6, X1 (Offline-Vertrag, `a3ca33c`)/X2/X3, R2–R6/R9 sowie R10/R11 als NO_BUG_VERIFIED; Gate 3 auf `0de1b9b` grün bis auf fmt (F6, seit `3b2e8c0` behoben); **Gate 4 auf `1d9c6ed` vollständig grün** (fmt, Clippy, 565/0/6 Tests, Release, Python 108). Weiter offen u. a.: E2/E9 (Plan-/Projection-Gates), R1-Rest (Consumer-Buffer voll/halbvoll, E2E-Capture-Drain; Ping/Pong-Blockade und TLS-Stall seit `1ac2630` belegt), L3, M6, F1/F2 sowie die P8-/FE-Designgates.
- **G-B (Netzwerk-Audit):** **NOT_RUN / NICHT FREIGEGEBEN**. Das vorhandene BOOT-03–08-Modul ist Code, kein Network-READY; E4/E5/E6 lokal geschlossen (`af8050b`, LOCAL_PASS, keine reale Laufzeit); X1 (Admission-Bindung an den freigegebenen Record, `a3ca33c`) und X3 nur offline belegt — der reale Netzwerkfall bleibt NOT_RUN.
- **G-C (600s-Kalibrierung):** **NOT_RUN / NICHT FREIGEGEBEN**.
- **G-D (4h):** **NOT_QUALIFIED / NICHT FREIGEGEBEN**. D1, D3, R3 (Offline-Messungen), C1 (alle sieben Tests) und R2 lokal geschlossen; reale Kalibrierung (G-C) fehlt. D4/D5/D13/E8 sind **keine** ohne neuen Beweis geltenden 4h-Blocker. Historische 4h-/5h-Versuche bleiben INCOMPLETE.
- **G-E (FE-01–05):** **PARTIAL/OPEN**, bisheriger FE-00-Befund `NO_ECONOMIC_GO`.
- **G-F (FE-06/07):** **NOT_AUTHORIZED / NOT_RUN**, kein erster realisierter Gewinn belegt.

**Pflegepflicht:** Jeder künftig gefundene Fehler kommt **zuerst mit stabiler ID in diese Datei** und bleibt bis zur nachgewiesenen Abnahme sichtbar; neue Bugs erzeugen keine stillschweigend neue Phase und keine automatische Verschiebung der bindenden FE-First-Profit-Priorität.


## 15. R — sechste Review-Nachprüfung: 4h-Capture-Härtung gegen neueren Main-Stand

**Quelle:** Fables Nachprüfung der zehn Hotpath-/Härtungsdateien auf `84ebd225`, am 2026-10-18 gegen `a7ca11914ea38c706f66717ebe7baf68094b1e5a` und ergänzend `market/configuration.rs`, `solana/ingress/ws_rpc.rs`, `state_owner/sweep.rs` verifiziert. **Codeeinsicht, keine neuen Tests ausgeführt.** Alle R1–R9 bleiben OPEN, bis Tests und – wo nötig – Implementierung mit SHA nachgewiesen sind. Ein bestätigter Source-Pfad ist nicht automatisch eine beobachtete 4h-Fehlfunktion. **Keine ungefragten Mainnet-/LIVE-Aktionen.**


### R1 — Market-Capture-Drain hängt an Quiescence der WSS-Session

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **MEDIUM / OFFLINE NEGATIVE TEST REQUIRED vor G-C/G-D · Cross-Ref D1/D8/C6/C7.**
- **Ist:** `market/capture.rs` markiert `drain_complete=true` nur, wenn `ingress.is_finished()` und die letzten Queue-Frames fehlerfrei abgearbeitet sind; andernfalls führt der Deadline-Ablauf zu `drain_complete=false` und korrekter, aber eventuell vermeidbarer `INCOMPLETE`-Versiegelung. `market/configuration.rs::DEFAULT_DRAIN_TIMEOUT_SECS=120`. In `ws_rpc.rs` gibt es `tokio::select!` mit `wait_shutdown` (100-ms-Poll), bounded Ping/Pong-Sends (5 s), Frame-Wait ≤250 ms und unterbrechbaren Reconnect-Backoff. Der von Fable befürchtete Regelfall „5 s Send ⇒ 4h immer INCOMPLETE“ ist **nicht belegt**; der bestehende 120-s-Default ist bereits länger als die genannten Einzeltimeouts. TLS-/Runtime-Abbau und tatsächliche Quiescence unter Fehlern sind noch nicht End-to-End getestet.
- **Soll:** Kein blindes Verlängern auf unbegrenzte Drain-Zeit. Deadline aus nachgewiesenem maximalem Shutdown-/Drain-Pfad und tatsächlicher Queue-Last ableiten; reale `thread_terminated`-, `frames_delivered`-, `drain_complete`- und `journal_complete`-Begriffe klar trennen. Wenn Deadline bei noch lebendem Thread erreicht wird: Incomplete erhalten, niemals fail-open. Fast-Shutdown ohne Eventverlust soll bei 120 s erfolgreich bleiben.
- **Tests:** Offline Fake-WSS: Shutdown während Backoff, zwischen Connect/ACK, während Ping-/Pong-Send, idle Read, langsamer TLS-Abbau; volles/halbvolles Consumer-Buffer, physisches 120-s-Deadline-/konfiguriertes kürzeres Testbudget, keine Live-Reads. Bei nachgewiesener Überschreitung minimal bounded konfigurieren/refaktorieren; Exit und Manifest-/Coverage-Konsistenz gegen D1 prüfen.
- **Abnahme:** kein vermeidbares INCOMPLETE bei nachweislich leerem und beendetem Stream; jedes echte Timeout bleibt INCOMPLETE. **Commit/Evidence:** **PARTIAL** `648e031`: Stop-Latenz des echten WSS-Adapters (eigene Runtime inkl. Drop) gemessen ≤ 104 ms vor Start/Backoff/Handshake/ACK-Wait (R10-Test); Shadow-Seite `shadow_session_stop_and_join_is_far_below_the_shutdown_budget`: `IngressSession::stop_and_join` ≤ 113 ms in Backoff, hängendem Handshake und Provider-Admission-Wartephase, zweimal hintereinander (2× Resubscribe), ohne `shadow_failed/abort`, gegen das Default-Budget 180 s; echtes Hängen ⇒ `false` am Budget (`ingress_session_stop_and_join_is_bounded`). Damit kein Beleg für vermeidbares INCOMPLETE durch Quiescence. **Rest (vom Reviewer als ein nicht blockierender Test beauftragt) `1ac2630`, LOCAL_PASS:** `requested_stop_is_bounded_when_the_peer_stops_reading_or_tls_stalls` über den echten `run()`-Pfad, nur Loopback: (a) Peer bestätigt die Subscription, flutet Pings und liest nie wieder — die Pong-Sends des Adapters füllen die Socketpuffer, er liest nicht mehr (der Server sieht seinen eigenen Send stehen); Stop ⇒ `Ok`, kein Fatal, kein Gap, 80–91 ms (3 Läufe); (b) TLS-Handshake hängt nach dem ClientHello des produktiven rustls-Connectors ⇒ Stop `Ok`, kein Fatal, 97–100 ms. Mutationsprobe: ohne den Stop-Zweig in `run_async` wartet der Stop den 5-s-Pong-Timeout ab (4498 ms) und der Test schlägt fehl — er trifft also den blockierten Send (`evidence/HARDENING_20261018/R1_rest_tests.txt`). Befund: kein vermeidbares INCOMPLETE durch Ping/Pong-Blockade oder TLS-Stall. **NOT_RUN:** Abbau einer etablierten TLS-Sitzung (offline nicht baubar: der Connector vertraut nur den webpki-Roots; strukturell: kein Stop-Pfad wartet auf WebSocket-Close oder TLS-Shutdown, die Session-Future wird verworfen). **Weiter offen, nicht blockierend:** Consumer-Buffer voll/halbvoll und End-to-End-Capture-Drain durch `market/capture.rs::run` mit 120-s-Deadline — deshalb bleibt der Haken offen. **`119da22` (Evidenz, keine Codeänderung):** `r1_full_and_half_full_queue_drain_completely_before_the_seal` — produktives `capture::drain_after_stop` + `finish_capture` mit echtem `JournalSink` und 120-s-Deadline: volle (64) und halbvolle Queue sowie während des Drains nachgelieferte Frames ⇒ jeder Frame verarbeitet, COMPLETE, Kette versiegelt + vollständig, < 10 s; ein bei voller Queue verlorener Frame ⇒ INCOMPLETE. **Weiter offen:** Deadline-Fall (Ingress lebt über die Deadline) in diesem Pfad, Abbau einer etablierten TLS-Sitzung (NOT_RUN).
- **Zusatzprüfung 18.10.2026 / Shadow-C6:** Neben dem Market-Capture-Drain von 120 s braucht der ¤ShadowOwner¤ dieselbe Negativmatrix für ¤IngressSession::stop_and_join¤. ¤runtime_config::shutdown_seconds¤ ist im Default **180 s** (konfigurierbar), ¤stop_and_join¤ pollt Thread-Ende alle 10 ms; ¤ws_rpc::run_async¤ cancellt mit ¤tokio::select!¤ und ¤wait_shutdown¤ auch während Reconnect-Backoff. **Nicht** nur nominales ¤tx.send¤-Timeout addieren: reale blockierende DNS-/TLS-/Runtime-Abschnitte und konfigurierte ¤P0_SHUTDOWN_SECS¤ gegen Worst-Case messen. Ein unter Budget sauber terminierender Stop darf nicht ¤INGRESS_SESSION_NOT_QUIESCENT¤ erzeugen; echter Timeout bleibt fail-closed. Tests: Retry-Backoff, Connect/Handshake, 5-s-Send, blockierter Fake, 2× Resubscribe, Timeoutgrenze (budget-1/budget) und ¤shadow_failed/abort¤-Parität. Keine unbounded Joins, keine automatische Verlängerung ohne Beleg.


### R2 — Ingress-Panic wird erst nach Capture-Journal-Seal geprüft

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS · `1e62423` · Cross-Ref D1/D8.** ~~`drain_complete` aus `is_finished()` allein; Join erst nach Seal.~~
- **Ist:** `capture.rs` wertet in der Drain-Schleife `None if ingress.is_finished()` als `drain_complete` aus, verarbeitet letzte Events, führt **danach** `finish_capture()` (Manifest/Binding/Reports) aus und ruft erst anschließend `quiesce(ingress, ...)` auf. Ein bereits panikbedingt terminierter Ingress-Thread kann daher vorher als sauber drain-complete gelten und ein `COMPLETE`-Manifest erzeugen, obwohl `quiesce` später `INGRESS_THREAD_PANIC` meldet. Ein solches Rennen muss in einem deterministischen Fake nachgewiesen werden; die Code-Reihenfolge selbst ist eindeutig.
- **Soll:** Join-Ergebnis **vor** finalem `finish_capture/JournalSink::finish` für die Vollständigkeitsentscheidung berücksichtigen, jedenfalls bei bereits `is_finished()`; geordnetes erfolgreiches Threadende vs. Panic unterscheiden. `drain_complete` niemals aus `is_finished()` allein; bereits empfangene Events vollständig verarbeiten und kein Frame wegen vorgezogenem Join verlieren. Die endgültige Fehlerklasse `Panic/Incomplete` ist in Manifest, Binding, Coverage und Resultat konsistent.
- **Tests RED→GREEN:** erfolgreich beendeter Fake-Ingress mit 0/N ausstehenden Frames ⇒ COMPLETE nur bei TIMER und gültigem Drain; Fake-Ingress mit sofortiger Panic nach letztem Event ⇒ `verify_chain().complete=false`, `MarketJournalBinding.status=INCOMPLETE`, Ergebnis `INGRESS_THREAD_PANIC`; Panic während Drain, Deadline/Backpressure, Fehler im Writer und Doppelabschlüsse.
- **Abnahme:** niemals COMPLETE bei panischem Ingress; geordneter Fall paritätisch. **Commit/Evidence:** `1e62423`: Phase 2 ist `capture::drain_after_stop`; ein bereits beendeter Ingress-Thread wird **vor** `finish_capture`/Seal gejoint: `Ok` (angeforderter Stop) ⇒ Drain vollständig wie bisher; Panic ⇒ `INGRESS_THREAD_PANIC`, Fehler-Return ⇒ feste Klasse (z. B. `WSS_RECOVERY_EXHAUSTED`), jeweils `core.mark_incomplete`, `drain_complete=false`, Lauf endet `MARKET_CAPTURE_INCOMPLETE: <Klasse>`. Der Thread gibt sein Ergebnis zurück statt es nur zu drucken (R6: `console::line`, feste Klasse). `quiesce` joint weiterhin nur einen am Drain-Deadline noch lebenden Thread. RED source-belegt (alte Schleife: `drain_complete = write_error.is_none()` bei `is_finished()` ⇒ COMPLETE wie D1-Fall „timer“). Test `ingress_panic_after_last_event_never_seals_complete` (sauber ⇒ COMPLETE; Panic/Fehler nach letztem Event ⇒ INCOMPLETE in Summary-Binding und `verify_chain`), D1/D8 unverändert; `market::` 121 PASS, Clippy `-D warnings` (`evidence/HARDENING_20261018/R2_tests.txt`). Nicht separat: Panic mitten im Drain mit Backpressure (gleicher Pfad: Join erst nach leerer Queue).


### R3 — jeder Recovery-Ready löst im Sweep-Modus erneuten globalen Bank-/Debtor-Sweep aus

**Abgleich:** Abgehakt; reale Sweep-Rate nicht gemessen.

- [x] **HIGH · LOCAL_PASS (Offline-Messung; reale Rate NOT_MEASURED) · `d9e98c5` · Cross-Ref D3/C1/B6/C7.**
- **Ist:** `state_owner/events.rs` verarbeitet jedes erfolgreiche `RecoveryOutcome::Ready` mit ReverseIndex-Rebuild, löscht Dirty-/Candidate-Cache und setzt bei `!self.use_threshold_trigger` sämtliche Gruppenbanken in `sweep_dirty_banks`; `sweep_batch_events=sweep_batch_max_events` erzwingt unmittelbar den ganzen Sweep. `state_owner/sweep.rs` expandiert über `accounts_for_bank` und arbeitet `account_work.into_par_iter()` vollständig auf dem Owner ab. Bei wiederholten WSS-Reconnects läuft derselbe komplette Debtor-Scan mehrfach. D3 maß 100k Fixed-Oracle-Kandidaten synthetisch 135 ms (1 Worker) bzw. ~71 ms (2 Worker), R127 historisch bis 78.488 Accounts/Batch, aber **nicht** den kombinierten Reconnect→Rebuild→Ingress-Fill→Sweep→nächste Gap-Zyklus.
- **Soll:** Reproduzierbarer Test dieses echten Zustandsübergangs gegen Queuegröße/Rate und Fairness-Latenz; die komplette Population nach Recovery **nicht** als vermeintlich redundant streichen (bereits unterwasser befindliche Debtors und Census-Vollständigkeit). Nur wenn der gemessene Owner-Block Queue-/Liveness-Budget überschreitet: accountseitig bounded Chunks/Owner-Yields mit revisions- und root-konsistenter vollständiger Abarbeitung, ohne Eventverlust, Vergessen von Debtors oder unbounded RAM.
- **Tests:** 1 Worker, historische Max-Batch- und 100k-Debtor-Fixture, zwei bis vier Reconnects in kurzem Abstand, Ingress simuliert verschiedene Eventraten (inklusive gemessener Peak), 65.536-Queue, Root/Gap/ProgramUpgrade während Rebuild und Sweep; überprüfte Input-Generation, vollständige Health-/Census-Counter, kein unbemerkter Overflow, Sweep p95/p99 und Owner-HALT/INCOMPLETE korrekt.
- **Abnahme:** Verbindliche Rate-vs-Owner-Block-Reserve gemessen und dokumentiert; bei Überschreitung Fix RED→GREEN. D3 bleibt für den **isolierten** Sweep-Fix LOCAL_PASS, R3 ist die ausstehende zusammengesetzte Recovery-Abnahme. **Commit/Evidence:** `d9e98c5`, Test `recovery_ready_rebuild_and_full_sweep_is_measured` (`#[ignore]`, Release): 100k Debtors im StateCore-Snapshot, je Zyklus `rebuild_reverse_index` über den ganzen State + alle Gruppenbanken dirty + Voll-Sweep (Dedupe → exakte Health → Sortierung), 6 Zyklen direkt hintereinander (wiederholte Reconnects), 1/2 Rayon-Worker, parallel füllt ein Produzent die 65.536er-Queue mit 50.000 Events/s. Release auf diesem Host: Owner-Block **227–234 ms (1 Worker) / 143–147 ms (2 Worker)**, höchstens 11.708 Events gequeued, **Overflow 0**; Break-even ≈ 280k Events/s (1 Worker) (`evidence/HARDENING_20261018/R3_recovery_sweep.txt`, Exit 0, Clippy `-D warnings`). Mit den Mainnet-Healthkosten des Detect-Pfads (3,39 µs statt ~1,3 µs synthetisch) Schätzung ≈ 0,45 s Block ⇒ Break-even weiterhin > 100k Events/s; Mainnet R127c/d: Backlog max. 506. **Kein Chunking** (Auftragsbedingung „gemessene Überschreitung“ nicht erfüllt). **NOT_MEASURED/NOT_RUN:** reale Mainnet-Ingress-Spitzenrate, Census-Zeilen im Zyklus, echte Oracle-Decodierung; Root/Gap/ProgramUpgrade *während* Rebuild/Sweep können nicht interleaven (beides synchron auf dem Owner, Events warten FIFO in der Queue).


### R4 — dynamische BOOT-Abhängigkeiten können das rechnerische Mindestbudget überschreiten

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **MEDIUM · LOCAL_PASS (Offline-Vertrag; Netzwerk NOT_RUN) · `c1c78e8` · Cross-Ref N1/X1/X3.**
- **Ist:** `boot_network.rs::required_reads` = `CLUSTER_READS + fe::required_scope_reads`. Der Kommentar nennt das **necessary, not sufficient**, weil BOOT-08 zusätzliche Oracle-/Dependency-Batches erst aus realen Accountdaten erkennt. `NetworkReads` stoppt korrekt mit `BOOT_RPC_BUDGET` beim Limit. N1 hat den alten Smoke-Defaultbug behoben, nicht die Bedarfsprognose bei wechselnder dynamischer Population.
- **Soll:** In Offline-Fixtures einen dynamischen Scope mit mehr Required Accounts/Oracle-Batches bilden; dadurch den tatsächlichen Worst-Case-/Budgetreservevertrag (unter `max_requests`, FREE_ONLY und Provider-RPS) dokumentieren und vor dem **separat autorisierten** ersten Netzwerk-Audit explizit qualifizieren. `required + 2×fe_snapshot_rounds` ist nur Fables Vorschlag, kein garantiert ausreichender Wert. Nie automatisch Limits überziehen oder zusätzlichen Mainnet-Traffic starten.
- **Tests:** `minimum`, `minimum+reserve`, dynamisches Wachstum bis Provider-Cap, Budget-Exhaustion ⇒ BLOCKED ohne Retry-Sturm, verschiedene Daten-/Oracle-Layouts, aktive FREE_ONLY-Configdrift, Zeitbudget und 429.
- **Abnahme:** Budgetgrenzen nachvollziehbar, sowohl legitime dynamische Reads als auch Fail-closed-Abbruch reproduziert. **Commit/Evidence:** `c1c78e8`: `fe::worst_case_scope_reads` / `boot_network::worst_case_reads` — der Scope schließt nach höchstens zwei Erweiterungen (Targets ⇒ Banken ⇒ je Bank Vault, Mint, ≤ 4 Observation-Keys, ≤ 3 Integration-Accounts), also ≤ `FIXED + T·(1 + 16·10)` Keys pro Runde, solange sich die Bank-Mengen der Targets während des Audits nicht ändern; darüber greift das harte Limit (fail-closed). Doctor-Zeile `boot_read_budget` (nicht kritisch): PASS ≥ Worst-Case, WARN zwischen Minimum und Worst-Case bzw. unter dem Minimum (Netzwerk-Audit verweigert). Test `dynamic_boot_scope_is_bounded_and_blocks_at_the_budget` (Maximal-Scope 327 Keys: kohärent 8 Scope-Reads — **unter** dem bisherigen „Minimum“ 10, weil es alle Runden annimmt —, nie kohärent 28 > 10 und ≤ Worst-Case 34; Minimum-Budget ⇒ `BOOT_RPC_BUDGET` nach genau 10 Reads, kein Retry; Default `boot_calls` 64 ≥ 34+5), `boot_read_budget_is_reported_against_the_worst_case` Exit 0 (`evidence/HARDENING_20261018/R4_tests.txt`). **Kein** Limit automatisch erhöht. **NOT_RUN:** reale Mainnet-Scope-Größe/Provider-RPS/429 (separat autorisierter G-B-Audit).


### R5 — potentieller Instant-Overflow bei exzessiver Speicher-Wartezeit

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · LOCAL_PASS · `825b9c8` · Cross-Ref M1.** ~~`started + wait` panickt bei nicht darstellbarer Wartezeit.~~
- **Ist:** `storage/budget.rs::MemoryBudget::acquire` enthält `let deadline = started + wait;`. Ein extrem großer `Duration`-Wert kann bei Instant-Addition panicken, falls ein Aufrufer ihn tatsächlich zulässt. Für reguläre Capture-Werte ist das kein nachgewiesener Fehler.
- **Soll/Tests:** Bounded Wait-Contract und `Instant::checked_add` mit explizit konservativem `Err` (oder dokumentiertem gekapptem Wait), keinesfalls infinite wait. Test `Duration::MAX`, `ZERO`, reale Limits, Hold-/Release- und Mutex-Parität.
- **Abnahme:** keine Panic durch Warteparameter, keine längeren unqualifizierten Admission-Wartezeiten. **Commit/Evidence:** `825b9c8`. RED reproduziert: `Duration::MAX` ⇒ Panic „overflow when adding duration to instant“ — auch ohne Wartebedarf (Deadline wurde vorab berechnet). Zweite Fundstelle: `MarketJournalWorker::submit` (`Instant::now() + stall_budget`, aus `P0_MARKET_WRITER_STALL_MS` ohne Obergrenze). Fix: `checked_add` an beiden Stellen; Budget ⇒ `Err(CAPTURE_MEMORY_WAIT_INVALID)` (als refused gezählt), Writer ⇒ `MARKET_WRITER_STALL_INVALID`; normale Waits/Bounded-Wait-Vertrag unverändert, nie unendliches Warten. Tests `unrepresentable_wait_is_refused_without_panic`, `unrepresentable_stall_budget_is_an_error_not_a_panic` Exit 0; `storage::` 21 PASS, Clippy `-D warnings` (`evidence/HARDENING_20261018/R5_tests.txt`). Keine neue Config-Obergrenze eingeführt (würde Limits-Revision ändern).


### R6 — Evidence-Drop meldet Fehler direkt über blockierendes stderr

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · LOCAL_PASS · `648e031` · Cross-Ref M2/D9.** ~~`OwnerEvidenceWriter::drop` und `ingress ended` schreiben per `eprintln!`.~~ Beide über `console::line` (begrenzt, nie blockierend); Ingress-Ende nur als feste, secretfreie Fehlerklasse (`ingress_error_class`); die Evidence-Fehler bleiben sticky im Writer, der Abort/HALT unverändert. Tests `ingress_end_and_evidence_drop_never_write_stderr_directly` (kein `eprintln!` in beiden Pfaden), `blocked_console_never_blocks_the_caller` (Konsole blockiert nie) Exit 0. Ein separater Test mit physisch blockierter Prozess-stderr ist nicht gelaufen (globaler stderr-Handle).
- **Ist:** `analytics/evidence.rs::impl Drop for OwnerEvidenceWriter` ruft bei offenem Worker `finish(None)` und bei Fehler `eprintln!("{e}")`. Reguläre Writer- und Capture-Logs nutzen inzwischen `console::line`. Der `Drop`-Pfad ist selten, kann mit blockierter stderr-Pipe aber weiter verzögern.
- **Soll/Tests:** sichere nonblocking `console::line` nur für optionale Drop-Diagnostik, eigentliche persistente Evidence-Fehler weiter sticky und im Rückgabepfad. Offline WriterDrop mit fehlerhaftem Finish und blockierter stderr; kein Deadlock, Incomplete bei Fehler bleibt sichtbar.
- **Abnahme:** auch im Drop-Pfad kein ungeordnetes unbounded stderr-Warten. **Commit/Evidence:** —.
- **Zusatzquelle 18.10.2026 / Ingress-C6:** ¤state_owner/ingress.rs::IngressSessions::spawn¤ loggt bei ¤src.run¤-Fehler im Threadabschluss weiterhin synchron über ¤eprintln!("ingress ended: {}", e)¤. Selbst wenn der Fehler nur im Error-Pfad auftritt, kann eine blockierte stderr-Pipe den Threadabschluss und dadurch ¤stop_and_join¤ verzögern. Optionalen Text über ¤console::line¤ oder typisierte sichere Fehlerklasse ausgeben; keine Provider-URL/Key-Bestände und keine echten Terminal-Abhängigkeiten für sicheren Shutdown. RED/GREEN-Test mit blockierter stderr-Konsole und auslösendem ¤src.run¤-Fehler; Owner-/Evidence-HALT bleibt konservativ. Nicht als eigenes Duplikat erfassen.


### R7 — Owner-Evidence-Sync läuft periodisch ohne neue Frames

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / PROFILE_REQUIRED · OPTIMIZATION, KEIN DURABILITY-BUG · Cross-Ref M2.**
- **Ist:** `analytics/evidence.rs::run_writer` ruft bei abgelaufenem Intervall immer `Sink::sync()`, auch wenn seit dem letzten Sync kein weiterer Frame geschrieben wurde. Das erhält korrekt den periodischen Durability-Vertrag, kann aber unnötige fsync-Systemcalls verursachen.
- **Soll:** nur bei tatsächlichem IO-Profilgewinn optional einen last-synced-Position-/Dirty-Zustand einführen; bei jedem neu geschriebenen Frame und bei Finish/Seal gelten mindestens die bestehenden Sync-/Fsync-/StickyFailure-Barrieren. Nicht durch bloßes „kein neuer Frame“ echte Daten ungesynct lassen.
- **Tests:** Sync ohne Events, danach erster/neuer Frame, Race zwischen Deadline und Writer, ErrorInjection, Shutdown/Seal, maximale Durability-Lücke.
- **Abnahme:** keine Sync-Regressionsverschlechterung; reduzierte Fsync-Last nur bei Messbeleg. **Commit/Evidence:** —.


### R8 — Headroom-Sampling wiederholt sich auf neuem Acquire; Hysterese ungeprüft

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / PROFILE_REQUIRED · Cross-Ref M1/PERSIST.**
- **Ist:** M1 cached `MemorySample` höchstens 100 ms **innerhalb eines einzelnen** `MemoryBudget::acquire`. `BytePermit::drop` setzt bei `used <= initial` das Limit sofort auf `initial` zurück. Bei fortlaufenden neuen Frames mit Grenzüberschreitung können daher neue Syscalls anfallen; wie oft das reale 4h-Lastprofil betrifft, ist ungemessen.
- **Soll:** unter synthetischer Grenzlast messen. Shared Cache/Hysterese nur mit geprüftem Freshness-Maximum und fail-closed Pressure-Policy; besonders keine Verwendung eines alten positiven Headroom-Samples als freie unbeschränkte zusätzliche Reservierung. Fables Schwelle `initial/2` ist nur Optimierungsoption, kein Freigabekriterium.
- **Tests:** oszillierende Used-Bytes, 1/2/N Concurrent-Acquires, MemoryPressure/Unknown, MaxBytes, stale sample und abrupter Druck; Peak-RAM, Syscallzählung und Admission-Parität.
- **Abnahme:** keine stillschweigende Schwächung von M1 oder Memory-Safety bei Performancegewinn. **Commit/Evidence:** —.


### R9 — Capture-Flush-Tick ist für JournalSink derzeit No-op

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW · LOCAL_PASS · `2b7c5f2` · Cross-Ref D4/M2.** ~~Tick ruft ein `Ok(())`-No-op.~~
- **Ist:** `market/capture.rs` ruft bei `last_flush.elapsed() >= 5s` `sink.flush()`, jedoch ist `JournalSink::flush` ein No-op; tatsächliche Writer-Durability wird über die eigene Journal-Writer-Lane garantiert. Der Tick beeinflusst damit nur den Fehlerbehandlungspfad, nicht die persistierte Bytefolge.
- **Soll:** Tick entfernen oder als tatsächliche Metrik/Writer-Statuskontrolle bewusst qualifizieren; `MarketSink`-Vertrag und den expliziten `JsonlSink`-Batchabschluss nicht verwechseln. Keine Änderung der Write-/Seal-/Drop-Semantik.
- **Tests:** produktiver Capture-Journal-Stream über mehrere Tickintervalle, keine Truncation, WriterSync/Failure, Jsonl Strict-Batch, Report-Parität.
- **Abnahme:** redundanter Code entfernt oder sinnvoll verwendet; kein Verhalten/Safety regressiert. **Commit/Evidence:** `2b7c5f2`, „sinnvoll verwendet“: Der 5-s-Tick blieb, `JournalSink::flush` liefert jetzt den sticky Fehler der Writer-Lane (`MarketJournalWorker::check`) — ein im Hintergrund gescheiterter Write/Sync wird so auch ohne neue Zeile binnen eines Ticks `WRITE_ERROR` statt erst beim nächsten Frame. `flush` flusht/versiegelt/beendet weiterhin nichts; JsonlSink-Batchgrenze, Write-/Seal-/Drop-Semantik unverändert. RED vor dem Fix: derselbe Test sah 5 s lang keinen Fehler. Test `journal_sink_flush_reports_a_failed_writer_without_a_new_row`; `market::`, `storage::`, `market_verification` 143 PASS, Clippy `-D warnings` (`evidence/HARDENING_20261018/R9_tests.txt`).


### R10 — angeforderter WSS-Stop darf keinen falschen Econ-Abort auslösen

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **NO_BUG_VERIFIED · LOCAL_PASS · `648e031` · Cross-Ref C6/D10/C7/R1.**
- **Quelle, überprüft auf `b462ba2`:** `state_owner/ingress.rs::IngressSessions::spawn` setzt bei jedem `Err(e)` von `src.run(...)` `shadow_failed=true` und `abort=true`. Der **heutige** WSS-Loop `solana/ingress/ws_rpc.rs::run_async` gibt jedoch bei gesetztem `shutdown.is_requested()` bereits explizit `Ok(())` zurück, sowohl beim `tokio::select!`-Shutdown als auch bei einem während des Stops eintreffenden Session-Fehler und während Reconnect-Backoff. Ein normaler Cancel ⇒ Err ⇒ Abort ist deshalb **nicht nachgewiesen**. Race am Rand der Cancel-/Fatal-Klassifikation bleibt prüfpflichtig.
- **Soll:** Eindeutige terminale Klassifikation `RequestedStop/NormalEnd` versus `ActualFatal/RecoveryExhausted`; normaler Dependency-Resubscribe darf bei geordnetem Stop niemals das geteilte `shadow_failed/abort` setzen. Reales `Fatal`, ein unerwartetes Err **ohne** Stop oder ein nicht beendbarer Thread müssen fail-closed zu `INCOMPLETE/abort` führen. Kein pauschales `if shutdown { ignore all errors }` nach bereits beobachtetem echten Fatal; C7-State/Owner bleibt authoritative.
- **Tests RED→GREEN bzw. VERIFY-PASS:** Fake-Ingress `stop_and_join_is_bounded` mit `econ Handle`/Status-Flags; Stop vor Connect, während Connect/ACK, während Async-Send, während Timeout-/Reconnect-Backoff; `abort==false && shadow_failed==false` nur bei **reinem** RequestedStop. Echtes Err vor/nach Stop, `WSS_RECOVERY_EXHAUSTED` und `Fatal` separat ⇒ abort und INCOMPLETE; reproduzierbare Ordering-Matrix ohne Mainnet.
- **Abnahme:** falls alles schon korrekt ist, Ticket **als NO_BUG_VERIFIED** mit Test-Exit 0, SHA und Evidence schließen; andernfalls minimalen Fix samt RED/GREEN-Test implementieren. Kein C6-Rewrite ohne Reproduktion. **Commit/Evidence:** `648e031`, kein Fehler reproduzierbar. Produktiver Pfad `WsRpcControlSource::run` (eigene Runtime + Drop) gegen Loopback-Fakes: Stop vor Start, im Reconnect-Backoff, bei hängendem WS-Handshake und während des ACK-Wartens auf einem Fake-WS-Server ⇒ jeweils `Ok(())`, **kein** `Fatal`, Stop-Latenz ≤ 104 ms; echte Recovery-Erschöpfung ohne Stop ⇒ `Err(WSS_RECOVERY_EXHAUSTED)` + genau ein `Fatal` (`requested_stop_is_ok_without_fatal_in_every_session_phase`). Der Thread-Body von `IngressSessions::spawn` ist jetzt `run_ingress` (verhaltensgleich); `requested_ingress_stop_never_aborts_the_measurement` fährt ihn mit den echten `econ::Shared`-Flags: Stop im Backoff ⇒ `shadow_failed=false, abort=false`; Erschöpfung ⇒ beide `true`. Evidence `evidence/HARDENING_20261018/R10_R11_R6_R1_tests.txt` (Exit 0, Clippy `-D warnings`). **NOT_RUN:** blockierter 5-s-Ping-/Pong-Send (Puffer lässt sich lokal nicht füllen; Abbruch über `tokio::select!` an jedem `await`), echter TLS-Abbau.


### R11 — alte Ingress-Generation: Fatal/Gap-Ereignisse am Resubscribe-Grenzpunkt

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **NO_BUG_VERIFIED · LOCAL_PASS · `648e031` · Cross-Ref C6/C7/C1/F4.**
- **Ist auf `b462ba2`:** `state_owner/ingress.rs::RetiredEvents::admit` verwirft nur `Reconnected` einer abgeschlossenen Generation und zählt diese; `Account/Slot/Root/Gap/Fatal` gehen unverändert an `StateCore`. Altes `Gap` hält die bereits geöffnete State-Lücke konservativ aufrecht. Ein **retired Fatal** wird seit C7 sticky und kann den Lauf terminal beenden, selbst wenn die alte Generation als Teil eines legitimen Dependency-Resubscribe bereits gestoppt wurde. Risiko bedingt: ein echtes Fatal **vor** Stop kann ebenfalls einen realen Datenverlust signalisieren und darf nicht blind weggedrückt werden.
- **Soll:** Semantik und Provenienz von Cancel-induziertem `Fatal` vs. tatsächlichem `WSS_RECOVERY_EXHAUSTED`/unerwartetem Fatal beweisen. Nur eindeutig mit **alter Generation und angefordertem geordnetem Stop** verknüpfte, fachlich irrelevante terminale Kontrollsignale dürfen gezielt unterdrückt und gezählt werden. Daten-/Root-/Gap-Events nicht ohne nachweisliche vollständige Reconciliation wegwerfen; echter altgenerationaler Datenverlust bleibt HALT/INCOMPLETE, bis neue Generation vollständig `StreamReady + A/B-Recovery + Root` qualifiziert. Kein Early READY durch neue Generation.
- **Tests:** Queue `Account, Reconnected, Gap, Fatal` vor/nach Resubscribe; Cancel-Fatal vs. echter vorheriger Fatal; `Fatal` vor `join`, nach `retire`, bei neuer Generation vor/ nach StreamReady; Producer-Written-Count/FIFO, fehlende/volle Queue, wiederholte Dependency-Änderung. Erwartungswerte unterscheiden `dropped_cancel_control` von `real_fatal_terminal`. Negative Tests belegen **keine** Fehlpromotion zu READY, **keine** verdeckte echte Fatal-Evidence.
- **Abnahme:** provenienzgebundene Generationstrennung und korrektes Outcome (geordneter Stop bleibt testbar recoverbar; echtes Fatal bleibt INCOMPLETE). Minimaler Fix nur falls RED/fehlender Contract. **Commit/Evidence:** `648e031`, kein Fix nötig. Provenienz: der einzige `Fatal`-Erzeuger ist `ws_rpc::run_async` bei `WSS_RECOVERY_EXHAUSTED`; ein angeforderter Stop erzeugt in keiner Phase `Fatal` (R10-Test). Ein `Fatal` der alten Generation ist daher immer eine echte Erschöpfung **vor** dem Stop — und deren Thread hat den Lauf über `run_ingress` bereits abgebrochen; Unterdrücken würde eine echte Ursache verdecken. Test `retired_generation_controls_are_classified_by_provenance`: alte Folge `Account, Reconnected, Gap` + neues `Reconnected` ⇒ genau ein `Ready` erst nach neuer A/B-Recovery, `stream_ready_dropped=1` (= *dropped_cancel_control*); ohne neues StreamReady kein Ready; altes `Fatal` ⇒ `RecoveryOutcome::Fatal{WSS_RECOVERY_EXHAUSTED}` (= *real_fatal_terminal*) trotz folgendem neuen StreamReady, kein Ready; `Fatal` der neuen Generation vor StreamReady ⇒ terminal. Ergänzt `no_duplicate_generation_or_old_session_readiness`.

**Bereits belegte Korrekturen aus Fables Review (NICHT erneut offenlegen):** B5 (`economics.rs::t_profitable` verwendet aktuell `u64::try_from(...).ok()?` und enthält negative Tests), D1 (`MarketSink::finish(complete)` + `JsonlSink::finish`-Override), D8 (`capture::quiesce` statt discarded JoinHandle), M1 (OS-Sample außerhalb Mutex), M2 (bounded periodic sync und optional `console::line`), D9 (asynchrone Konsolenausgabe), C6/D10 (WSS Generation/Join), N1 (separates BOOT-Budget). **Die bestehenden `- [x]` bleiben, neue Regression-Lücken tragen eigene IDs.** F5 deckt Jito-`UNTESTED/VERIFY`-Kommentare bereits ab: weiterhin offen, nicht doppelt als R10 anlegen. F6 Reporter-Formatierung nur durch den zuständigen Autor.


## 16. Aktueller Claude-Codeauftrag — Hotpath-Gates aus Review 7 (nur offline)

**Auftragsstand:** geprüft gegen GitHub-`main` `b462ba2330b4149aaa4e3c2471f8fa07bdfe657e`. Dieser Abschnitt ist ein verbindlicher **Arbeitsauftrag zur Umsetzung**, kein behaupteter neuer Test/PASS. Änderungen anderer parallel arbeitender Agenten nicht überschreiben. Arbeite direkt auf `main` (kein Branch/PR, kein Force-Push, kein Reset), vor jedem Commit Remote nachziehen, nur eigene Pfade stage/committen, `bsc/**` und `gateway/**` unangetastet lassen. Kein LIVE, kein Geld, keine Mainnet-Reads, keine kostenpflichtigen Provider, keine globalen Limits-Bypässe. `BUGS.md` und `STATUS.md` nur nach nachprüfbarer Umsetzung/Evidence fortschreiben.

**Priorität 1 — zunächst Reproduktion und Quellklassifikation:** `ws_rpc.rs::run_async/session`, `state_owner/ingress.rs::{spawn,stop_and_join,RetiredEvents}`, `state_owner/events.rs`, `state_owner/shutdown.rs` und `runtime_config.rs` end-to-end lesen. (a) **R10:** bereits vorhandene `shutdown => Ok(())`-Zweige beachten; deterministischen Cancel-vs-Fatal-Test über echten Spawn-Closure/Econ-Abort-Flags bauen; nur bei RED echten Fehler korrigieren, sonst mit `NO_BUG_VERIFIED`/Evidence schließen. (b) **R11:** Fake-Eventfolge alter/neuer Generation inkl. `Fatal/Gap/Reconnected`; Stale-Control nur bei belegter alter Session/Cancel herausfiltern, niemals echte terminale Ursache verstecken. Fehlende Tests ergänzen, C1/C7-Fail-closed erhalten. (c) **R1:** 120-s Market-Drain und konfiguriertes Shadow-`shutdown_seconds` (Default 180) gegen alle WSS-Loop-Blockier-/Reconnect-Zustände messen; Abbruch < Budget, echtes Hängen ⇒ INCOMPLETE. Nicht pauschal Timeouts erhöhen.

**Priorität 2 — verbleibende harte 4h-Gates:** **R2** Fake-Ingress-Panic **vor** `finish_capture/Journal-Seal` erkennen und Evidence INCOMPLETE halten; **R3** Recovery→full index rebuild→100k Sweep + volle/teilvolle Queue (1 und 2 Worker, wiederholte Reconnects) p95/p99/Overflow verifizieren; **C1 Test 7** echtes offline aufbaubares Owner-E2E; **R4/X1/X3** dynamische BOOT-Budgets/Identitäts- und Crashmatrix offline – Netzwerkaudit bleibt nicht autorisiert. Nur bei gemessener Überschreitung bounded Chunking mit Vollständigkeitsinvarianten implementieren. D3/C6/D10/B5 nicht wieder ohne Reproduktion anfassen.

**Neu Review 8 – ECON/STRICT/REPORTER:** Beziehe zusätzlich die Tickets **R12–R26 aus §17** in den bestehenden Auftrag ein. Erste Offline-Arbeiten sind die historischen 4h-/5h-Retention-Zähler (R12), die nachweisbare Finality-/Bootstrap-Report-Parität (R14/R15) und die Vorbereitung einer budgetierten 600-s-Strict-Kalibrierung (R13). **Keinen** selbstständigen Netzwerk-Audit oder 600-s-/4h-Lauf starten: G-B/G-C/G-D brauchen weiterhin separate Freigabe. Reporter-Anbindung R18/R19 vor tatsächlich angezeigtem Econ4h-Frontend; R20–R23 nicht als vorzeitige 4h-Backend-Blocker deklarieren. Bei R26 als Nächstes ¤market_truth.rs¤ lesen und Source-Truth/Nicht-Relevanz belegen; erst bei echtem Defekt neue ID. Bereits laufende R1–R11/C1-Gates nicht überspringen.

**Priorität 3 — isolierte Niedrig-Risiko-Pfade:** **R6** optionales `eprintln!("ingress ended: {}", e)` auf bounded `console::line` und sichere Fehlerklasse umstellen (zusammen mit Evidence-Drop), ohne den echten Error/HALT zu unterdrücken; **R5** defensives `Instant::checked_add`; **R7–R9** erst bei belegtem Profilgewinn, Durability/Admission/Batch-Semantik unverändert. **F5** Jito-VERIFY ohne erfundene Produktivqualifikation dokumentieren. **F6:** die drei Reporter-Dateien aus `311f9a4` **nicht** formatieren; dem Autor überlassen.

**Abnahme pro Ticket:** stabiler Bug-ID-Eintrag; ursprünglicher RED-Test oder belegtes `NO_BUG_VERIFIED`, GREEN inklusive Panic-/Overflow-/Fake-WSS-Negativmatrix, getestete Features, `cargo fmt --check`, Clippy `-D warnings`, Workspace-Rust-/erforderliche Python-Tests, Release-Build auf einer identischen SHA. Evidence in `project_0/docs/evidence/HARDENING_20261018/` (testiger Output mit Exitcode, SHA und Limitierung), `BUGS.md` Haken nur bei tatsächlich bestandener Abnahme, `STATUS.md`-Gates bleiben separat NOT_RUN/BLOCKED. Für Formatierungsausnahme F6 explizit EXIT 1 plus **genau** diese drei Dateien dokumentieren; nicht grün behaupten. Bei R10/R11 keine riskanten Änderungen ohne reproduzierbare Notwendigkeit.

**Übergabe:** geänderte Pfade, pro Ticket RED/GREEN, Commit-SHAs, PASS/NOT_RUN, tatsächlich getestete Commits, offene Risiken und eindeutige Empfehlung zum **nächsten offline** Gate. Auf `main` nur Fast-Forward-Push. Nach jedem Push lokal/remote Status verifizieren.

## 17. R — Review 8: 4h-Economics, Strict-Reconciliation und Frontend-Reporter

**Review-Quelle:** Fables SHA-gebundener Lesedurchgang auf `e9872cb`: `econ.rs`, `econ_canonical.rs`, `measurement.rs`, `startup.rs` (Econ4h-Supervisor), `market/core.rs`, `market/configuration.rs`, `market/winner.rs`, `market_reconcile.rs`, `status_tap.rs`, Reporter `lib.rs`/Composition Root sowie `status.proto` und Gateway-Wire-Vertrag. Am 18.10.2026 sind relevante Codepfade zusätzlich auf GitHub-`main` `1973b97ee5c66ff7e3897433a77aa1a26fc69be3` anhand festgepinnter Dateien stichprobenartig nachgelesen worden. **Keine neuen Rust-/Python-Tests, alten 4h-Evidence-Läufe oder Netzwerkzugriffe in dieser Nachprüfung ausgeführt.** SOURCE_CONFIRMED bedeutet überprüfter Codepfad, **nicht** bewiesener reproduzierter Betriebsfehler oder Gate-PASS. Bei Änderungen seit `e9872cb` vor Fix/Schließung immer auf dem dann aktuellen SHA neu verifizieren. Cross-References verhindern doppelte Implementierungen mit bestehenden FE-/C0-/JFI-/Frontend-Gates.


### R12 — K1: ECON-Retention kann den gesamten Vierstundenlauf INCOMPLETE machen

**Abgleich:** Nicht abgehakt in Quellregister.

- A1: historische Zahlen siehe D7 (max. 1.682 TX in 4h, Cap 262.144 unverändert); Grenzfall-Tests offen.
- [ ] **HIGH / G-C/G-D KAPAZITÄTSGATE · CODE_CONFIRMED / AUSLÖSER NOT_MEASURED · Cross-Ref C0-E1, PERSIST/SK, R13.** · Fix `16ec71a` · Test `r12_live_retention_cap_is_exact_and_fail_closed` — Cap exakt (262.144 zugelassen, 262.145. ⇒ INCOMPLETE + Zähler, keine Verdrängung), Duplikat am Cap kein Overflow, fehlgeschlagene relevante TX zählen, Strict-Receipts gleich fail-closed. Reserve aus A1: max. 1.682 TX/4h ≪ 262.144; Cap unverändert. · Ergänzt `119da22`: `r12_strict_scan_retention_is_the_configured_cap` — Strict-Scan nutzt `retention_cap()` (Econ-Kapazität bzw. Market-Retention), kein 100.000-Literal mehr. · **Wieder offen (Review 20.10.), PARTIAL:** der im Ticket geforderte Test mit knappem Bytebudget (`econ_retention_mib`) fehlt.
- **Ist:** `analytics/econ.rs::Config::from_lookup`: `P0_ECON_RETENTION` default **262.144**, zulässig höchstens **2.000.000**, mit separater Bytegrenze `econ_retention_mib`. Reviewer nennt drei Verbraucher des Limits: Live-Capture `seen`, `truth.live_receipts`, `truth.signatures` im Strict-Scan. Einmalige relevante Marginfi-Transaktionen können auch fehlgeschlagene fremde Versuche enthalten; nicht nur Liquidationsereignisse zählen. Rein rechnerisch entspricht 262.144/14.400 s ≈ **18,2 eindeutigen Transaktionen/s**, sofern die jeweilige Datenstruktur diesen Cap eins-zu-eins belegt. `C0_E1_MARKET_RETENTION_OVERFLOW` / `CANONICAL_MARKET_RETENTION` führen konservativ zu INCOMPLETE; eine reale Auslösung ist **nicht** belegt.
- **Soll:** Aus vorhandenen 4h-/5h-Läufen read-only `unique_transactions`, `marginfi_transactions`, Peaks, Retention-/Byte-Maximum, Frame-/Truth-Kardinalitäten und C0-E1-Evidence mit Run-ID ermitteln. Mehrfachzähler, Signaturen und RAM-Budget sauber differenzieren. Nur bei begründetem Bedarf `P0_ECON_RETENTION` innerhalb harter Limits **und** `econ_retention_mib` / Diskreserve / Admission konsistent dimensionieren; keine pauschale Hochsetzung.
- **Tests:** 262.144/262.145 Grenzfall (Fake-TX inkl. gescheiterter relevanter TX), identische Signatur mehrfach, Strict-vs-Live-Retention, Bytebudget knapp, großer gültiger Upper-Bound, UNKNOWN/HALT statt stiller Verdrängung; 600-s-Kalibrierung als separater freizugebender Schritt.
- **Abnahme:** dokumentierte historische Zahlen und begründete 4h-Kapazitätsreserve; Overflow bleibt fail-closed. **Code-Commit/Evidence:** —.


### R13 — K2: Strict-getBlock-Full-Scan und globaler Validierungsfail können 4h-PASS verhindern

**Abgleich:** Abgehakt nur A4-Teilfix; Durchsatz/Kalibrierung offen.

- [x] A4 (K2a, nur der Validierungsabbruch; Durchsatz/Kalibrierung bleibt offen) · Fix `4dd9be9` · Test `full_slot_invalid_transaction_is_counted_and_the_scan_goes_on` — Decode-/Validierungs-/Key-Fehler einer einzelnen TX: `transactions_invalid` + erste (Slot, Index, Code), `error=TRANSACTIONS_INVALID`, complete=false, Scan läuft weiter; Strukturfehler brechen weiter sofort ab; Row-Layout unverändert (Zahl und erste TX im persistierten `error`-Text).
- [ ] **HIGH / G-C KALIBRIERUNG / G-D STRICT COMPLETENESS · CODE_CONFIRMED, REAL THROUGHPUT NOT_MEASURED · Cross-Ref market_reconcile, strict_driver, PERSIST/SK/JFI.**
- **Ist:** `market_reconcile.rs`: `getBlocks` über das Fenster, `getBlock` mit `transactionDetails:"full"` je produzierten Slot, komplette Transaktionsdecode und `validate_full_transaction` auch bevor Marginfi-Relevanz entschieden wird. Fehler `INNER_INSTRUCTIONS_UNAVAILABLE`/`LOADED_ADDRESSES_MISSING`/ungültige Indices ⇒ gesamter Strict-Scan fail-closed. `max_slots` default/harter Config-Max **100.000**; der 4h-Window-Umfang liegt grob bei 36.000 Slots (abhängig von tatsächlicher Produktion/Finality). **Nur als Untergrenze bei 5 req/s und 36.000 Blöcken:** 7.200 s = 2 h, ohne Block-/Header-/Retry-Overhead; freie Providerquota und Datenrate nicht nachgewiesen. JSON-Blöcke können viel RAM/Netz belegen, zweistellige GB sind Reviewer-Szenario, **keine Messung**.
- **Soll:** G-C-600s-Probe mit Messung `produced_blocks/s`, Header/JSON compressed/uncompressed bytes/block, Memory peak, validate-errors nach Klasse, 429-Rate, backoff, effective provider RPS, disk occupancy, P50/P95/P99 und projected strict seconds für 4h; Quote auch gegen FE/RPC-Sharing und FREE_ONLY prüfen. Full-chain/inner/ALT-Validierung NICHT aufweichen, um scheinbare PASS-Rate zu erhöhen. Bei echter Grenzüberschreitung bounded read-/scan-Optimierung nur mit gleicher kanonischer Completeness, typed model, Resume/Checkpoint und JFI.
- **Tests:** Offline getBlock-Fixtures mit Vote-/Legacy-/v0/v1-/ALT-/inner-Fehlern, große Blöcke, 429, Deadline, Budget Exhaustion, 100.000/100.001 Slots, erzeugte vs. fehlende Slots, strict restart/resume, max RAM/Disk/Calls. Keine Mainnet-Aufrufe ohne eigene Freigabe.
- **Abnahme:** replizierbare strikt-konservative Durchsatzhochrechnung + realistische Guardrails; unqualifizierbare TX bleiben UNKNOWN/INCOMPLETE. **Code-Commit/Evidence:** —.


### R14 — E-A: Winner-Speedup über finalized Log-RX um Finality-Delay verzerrt

**Abgleich:** Abgehakt; Z5 verlangt separate technische Race-Statistik.

- [x] A2 · Fix `8fc5a55` · Test `a2_slot_margin_is_identical_live_and_offline_and_rx_only_when_confirmed`, `a2_rx_margins_only_for_a_confirmed_receipt`, `a2_earlier_schema_frames_stay_readable`, `a2_without_receipts_or_commitment_nothing_is_claimed` — rx-Margen nur bei `confirmed` (sonst None + Grund); Hauptkennzahl = kanonische Marge zum ersten In-Window-Slot-Empfang (live Worker-`slots`, offline `SlotBoundary` aus `market.p0j`, gleiche Funktion); Schema 3 für ECON_OPPORTUNITY/ECON_CANONICAL/ECON_SNAPSHOT/ECON_FINAL, Schema 2 lesbar; Renderer `NativeReport/2`; `late_stage_*` unter finalized nur noch über POST-Klasse; Replay ohne Journal: Slot-Marge NOT_AVAILABLE, rx NOT_COMPUTED; Mutationsprobe 4/4 rot.
- [x] **HIGH / FE-00 ANALYTIK-VALIDITÄT vor 4h-Auswertung · SOURCE_CONFIRMED, MAGNITUDE_NOT_MEASURED · Cross-Ref R13/C0-E1/FE-00.**
- **Ist:** `market/configuration.rs` default `P0_MARKET_COMMITMENT=finalized`; `analytics/econ.rs::race` berechnet `margin_to_winner_rx` und `required_speedup_to_winner_rx` aus lokalem Sign-Zeitstempel vs. Empfang der Live-Winner-Logs. Bei finalisierten Logs ist RX die **Finality-/Provider-bedingte Beobachtung**, nicht die konkurrierende Submission/Landung. Reviewer schätzt ca. 32 Slots / rund 13 s Bias; das ist **keine in diesem Review gemessene Konstante**. Kanonisch auch `local_margin_ms`, `required_local_speedup_ms`, `speedup_sensitivity` betroffen. Slot-Race-Klassen `PreWinnerSlotProven/SameSlotUnknown/PostWinnerSlotProvenLate` hängen nicht von dieser Wandzeitdifferenz ab.
- **Soll:** `margin_to_winner_slot_seen` / `required_speedup_to_winner_slot_seen` als klar qualifizierte Beobachtungsalternativen abgleichen; jede RX-Kennzahl sichtbar mit `commitment=finalized/confirmed`, Quelle und zeitlicher Definition versehen, niemals „echte Konkurrenz-Submission-Deadline“ behaupten. Option `confirmed` nur unter Source-/Capture-/Gap-Vertrag, kein Default-Flip ohne Test. Finalisierte kanonische Wahrheit/Identity bleibt separat.
- **Tests:** gleiche Winner-Identity mit identischen Slots aber künstlichem Delay 0/1/13 s in finalisierten Logs; Speedup-Anzeige darf den Delay nicht als Wettbewerbsvorsprung werten. `confirmed`-Fixture, SlotSeen-Missing, Uhrendomain, no winner, never fake negative speedup, Report-Parität.
- **Abnahme:** Zeitmargen und FE-00-Wettbewerbsschlussfolgerungen lassen sich aus Replay reproduzieren, keinerlei durch Finality geschönte Edge. **Code-Commit/Evidence:** —.


### R15 — E-B: Bootstrap-Probes Live-vs-Offline-Join unterschiedliche Auswahl

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] A3 · Fix `108ee5f` · Test `a3_offline_join_admits_only_the_live_window` — `join()` (nativer Report, Replay) lässt nur Evaluationen zu, die die Live-Chain zulässt (`live_hour`, gemeinsam mit `Worker::hour`; monotones Fenster aus dem CAPTURE-Frame, im Replay optional 7. Argument `CAPTURE_START_NS`, sonst `evaluation_window: NOT_APPLIED`); Mutationsprobe: ohne Filter wird eine live NOT_DETECTED-Evaluation offline EXACT_PLAN_MATCH (199,99 USD). Nicht nachgebildet: Kapazitätsabweisung der Live-Chain (`shadow_heads` voll).
- [x] **MEDIUM / FE-00 BERICHTSPARITÄT vor 4h-Auswertung · SOURCE_CONFIRMED, REPRO_NOT_RUN · Cross-Ref PERSIST/JFI/C0-E1.**
- **Ist:** `analytics/econ.rs::Worker` hängt Evaluations-Frames immer an, verlinkt den pro-Target-`shadow_heads`-Chain aber **nur bei `hour.is_some()`** (in-window). `econ_canonical.rs::finalize_canonical` replayt diese Chain; Legacy/Offline-`join()` kann bei Scan über alle Frame-Records frühere Bootstrap-Probes berücksichtigen. Startup-Bedingung `ready <= capture_start` macht Pre-Window-Probes erwartbar. Reviewer berichtet dadurch Live-`NOT_DETECTED` vs. Offline-`target_seen` für Targets ausschließlich aus Bootstrap.
- **Soll:** verbindliche, identische Window-/Bootstrap-Zuordnungsregel für kanonischen Live- und Offline-/Reporter-Pfad; Pre-Window-Probes fachlich korrekt getrennt berichten, nicht rückwirkend als gemessene 4h-Detektion deklarieren. Änderungen dürfen keine echte In-Window-Evaluation unterschlagen.
- **Tests:** ein Target nur vor Fenster, eines vor+innerhalb, nur nach Cutoff, mehrfache Winner, same-slot Ambiguity; native Report, Live-Finalisierung, Offline-Replay müssen pro Klasse denselben `target_seen`/`NOT_DETECTED`-Status belegen. Run-/Schema-Provenienz unverändert.
- **Abnahme:** vollständige Replay-Parität mit festgeschriebener Fensterregel. **Code-Commit/Evidence:** —.


### R16 — E-C: kanonischer Kapitalreport ignoriert konfiguriertes Kapitalraster

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] A6 · Fix `13c0b1c` · Test `a6_canonical_capital_uses_the_configured_list_live_and_offline` — `summarize()`, `joined_value()` und Replay nutzen `cfg.capital` (Replay: optionales 8. Argument `CAPITAL_USD`, sonst Standardliste; `parse_capital` gemeinsam mit `P0_ECON_CAPITAL_USD`); kanonische Zeile trägt die Liste (ECON_CANONICAL Schema 4, Schema 2/3 lesbar mit der damals genutzten Standardliste); live == Replay für fundable/blocked/unknown.
- [x] **LOW / REPORT-CONFIG PARITÄT · CODE_CONFIRMED · Cross-Ref FE-00/P6/C0-E1.**
- **Ist:** `econ.rs::Config` liest `P0_ECON_CAPITAL_USD`, während `econ_canonical.rs::summarize` aktuell direkt `CAPITAL_USD` = [200,500,1000,2500,5000,10000] verwendet; Legacy-Pfad nutzt `cfg.capital`. Bei benutzerdefiniertem Raster weichen beide Reports ab.
- **Soll:** kanonische Zusammenfassung erhält das validierte Konfigurationsraster ausdrücklich, Schema/Provenienz eindeutig; keine still geänderten Funding-Simulationen.
- **Tests:** Default/Custom/invalid Capital, Live-/Offline-Paarvergleich, fundable/blocked/unknown identisch. **Commit/Evidence:** —.


### R17 — E-D: `first_observed_slot` in späteren Stundenfenstern kann globalen ersten Slot wiederholen

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- E-D laut Auftrag 18.10.2026 nur eingetragen, nicht gefixt.
- [x] **LOW / HOURLY-METRICS · CODE_CONFIRMED · Cross-Ref C0-E1/FE-00.** · Fix `07c3f09` · Test `r17_hourly_observed_slots_are_per_hour` — je Ingress-Event Slot-Spanne der zugelassenen Transaktionen (`observed_slots`); Stunde = Min/Max ihrer Events, leere Stunden ohne Wert; globale Werte unverändert.
- **Ist:** `analytics/econ.rs` übernimmt bei `Evidence::MarketCounters` den `first_observed_slot` per `or_insert(n)`, während `MarketStats` einen globalen Minimumslot ausgibt. Stunde 2–4 können deshalb Slot 1 des ganzen Runs melden statt des stündlichen Minimums.
- **Soll:** pro Fenster ein tatsächliches Slot-Minimum aus im Fenster beobachteten Slot-Ereignissen oder einer eindeutig deklarierten neuen Stundenmetrik führen; aus bloßem Counterdelta ohne Slot-Identität ist ein Minimum **nicht** sicher rekonstruierbar. Keine Schätzung aus `last_observed_slot`.
- **Tests:** 4 Stunden, Lücken/Reconnect, fehlende Slots, Root-Reorg/Gaps, Hours mit 0 Frames; globale und stündliche Grenzen getrennt. **Commit/Evidence:** —.


### R18 — F-A: Econ4h-Produktivlauf ohne Reporter-StatusTap-Verbindung

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **MEDIUM / FRONTEND-VISIBILITY GATE · SOURCE_CONFIRMED, E2E_NOT_RUN · Cross-Ref F6/FEUI/F3-Gateway.**
- **Ist (Reviewer-Stand `e9872cb`):** `run_econ4h` verwendet `run_embedded(market, clock, Some(handle))`, damit `status_tap=None`; Reporter-Publikation nur bei Market-only `run_embedded_with(...status_tap)`. Der neuere `market/capture.rs` besitzt bereits beide APIs, aber der tatsächliche Econ4h-Composition-Root ist im Rahmen dieser Nachprüfung noch nicht Ende-zu-Ende instrumentiert getestet. Kein StatusTap ⇒ kein `project0.status`-Stream des 4h-Produkts.
- **Soll:** opt-in read-only Reporter-Anbindung über klaren Owner/Lifecycle bei Econ4h und Shadow, ohne Blockierung des Writer-/Owner-Hotpaths; dieselben Run-/Generation-/Topic-/Quality-Identitäten und Journal-Source-of-Truth. Ein Frontend-Snapshot ist niemals ein 4h-PASS-Beleg.
- **Tests:** Fake-4h-Econ startet Reporter und sendet Status; Reporter aus ⇒ Capture unverändert; Gateway-E2E, Reset, Crash, send-queue overload, 4h-Stopp, keine Secrets, keine Socket-Neuarchitektur ohne Quelle.
- **Abnahme:** P0-Econ4h sichtbar, Status/Schadensfall konsistent, keine negative Hotpath-Wirkung. **Commit/Evidence:** —.


### R19 — F-B: Transport-`last_frame_age_ns` wird als veralteter Snapshot erneut ausgesendet

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **MEDIUM / FRONTEND-FEED-HEALTH · CODE_CONFIRMED, FAILURE_FIXTURE_NOT_RUN · Cross-Ref R18/FEUI.** · PARTIAL `07c3f09` · Test `r19_frame_age_is_advanced_to_the_send_time` — erreicht: Reporter sendet das Frame-Alter zum Sendezeitpunkt (Snapshot mit Publikationszeit), kein altes relatives Alter mehr als frisch. OFFEN: Alterung bei eingefrorenem Feed ohne neues Capture-Frame (Anzeige now − source_timestamp), `transport_alive` vs. `publisher_alive`, ggf. absoluter Zeitpunkt im Wire — Gateway/Frontend/Protocol-Scope.
- **Ist:** `status_tap.rs::TransportSnapshot` speichert relativen `last_frame_age_ns`, `reporter/lib.rs::projection` gibt diesen gespeicherten Wert wieder; `state_frame` versieht die gesamte State-Nachricht mit neuem `source_timestamp_unix_ns=now_ns()`. Bei anderen State-Updates oder Heartbeats kann das relative Frame-Alter fälschlich als frisch erscheinen, obwohl seit dem letzten Transport-Health-Sample der Feed tot ist.
- **Soll:** zuverlässigen absoluten letzten-Frame-Epochzeitpunkt bzw. monotone Zeitdomäne mit expliziter Sender-Uhrbindung publizieren und Alter bei Darstellung als `now - last_frame_time` berechnen; für Backward-Compatibility neue Wire-/Schema-Version und UNKNOWN bei fehlendem Timepoint. Heartbeat-Publikation darf Feed-Liveness nicht vortäuschen.
- **Tests:** eingefrorener Transport + steigender Sender-Timestamp, Replay, Reconnect, Clock-Skew, None/0/unknown, Gateway-/Frontend-Verbraucher; nach 10s Tod muss Alter ≈ 10s steigen oder UNKNOWN werden.
- **Abnahme:** UI zeigt Tote-Feed-Alter korrekt und unterscheidet `transport_alive` von `publisher_alive`. **Commit/Evidence:** —.


### R20 — F-C: Status-/Shadow-Felder fehlen im Wire-Schema; nur 1 von 8 Topics

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / FRONTEND SCOPE-GATE · SOURCE_CONFIRMED, SPEZIFIKATION VORHANDEN · Cross-Ref FEUI/F3-Gateway.**
- **Ist:** `StatusTap::MarketSnapshot` enthält `observing` und `stream_ready`, Transport enthält `connected`/`last_slot`; Reporter-`projection` publiziert davon nur einen Teil. Shadow Ready/HALT/Recovery/Candidate-/Detection-Status fehlt; `hello_frame` annonciert derzeit nur `TopicId::Project0Status`, während der Review einen acht-Topic-Gateway-Vertrag beschreibt. **Keine Behauptung**, dass die übrigen sieben Topics bereits produktiv gefordert sind, bevor Gateway-/Frontend-Vertrag nachgelesen ist.
- **Soll:** Topic-/Feld-Matrix aus gepinntem Proto/Wire-Vertrag mit Version/Owner/Quality erstellen, vorhandene Felder nur semantisch korrekt übernehmen und fehlende Topics gemäß F0–F7-Plan priorisieren; keine Phantom-Stati, kein Polling des Trading-Owners.
- **Tests:** Schema-/Proto-Kompatibilität, Snapshot/Delta/Heartbeat, Ready vs Blocked vs Detached, Shadow-Lifecycle, Gateway-Reconnect, Anfangszustand leer.
- **Abnahme:** belegte Wire-Parität je vereinbartem Topic; offene Scope-Themen bleiben eigene Frontend-Gates. **Commit/Evidence:** —.


### R21 — F-D: Reporter-Heartbeat ignoriert konfigurierte Periode

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW / CONFIG-EFFECTIVENESS · CODE_CONFIRMED · Cross-Ref R20/F3-Gateway.** · Fix `9fad87d` · Test `r21_heartbeat_period_is_the_configured_interval` — Wait bis zur nächsten Heartbeat-Deadline (≤ 500-ms-Stop-Poll); 700 ms kam vorher alle ~1000 ms.
- **Ist:** `reporter/lib.rs::run_session` wartet mit `reader.wait_changed(cfg.heartbeat.min(Duration::from_millis(500)))`, also effektiv maximal 500 ms statt des aus `P0_REPORT_HEARTBEAT_MS` geparsten Intervalls. Ob jede Wait-Iteration tatsächlich einen Wire-Heartbeat sendet, im Test qualifizieren; die Konfiguration verliert jedenfalls ihre beabsichtigte Taktwirkung.
- **Soll:** Heartbeat-Deadline und LatestValue-State-Änderungen getrennt behandeln, konfigurierten Intervall innerhalb Wire-/Gateway-Liveness-Budgets einhalten, keine busy loop und keine Rückstau-/Sleep-Schleife bei fehlendem Client.
- **Tests:** Fake-Clock 500/5.000/15.000 ms, bursts vs idle, Backpressure und Disconnection; Anzahl gesendeter Heartbeats und Zeitabstände prüfen. **Commit/Evidence:** —.


### R22 — F-E: stale Reporter-Socket/Lock nach Crash verhindert Neustart

**Abgleich:** Abgehakt; echte Crash-Reproduktion laut Register offen.

- [x] **MEDIUM / RESTARTABILITY · CODE_CONFIRMED, CRASH_REPRO_NOT_RUN · Cross-Ref R18/FEUI.** · Fix `9fad87d`+`07c3f09` · Tests `r22_crash_leftovers_restart_but_live_owners_and_foreign_files_refuse`, `r22_second_shutdown_never_removes_a_successors_files` — prozessgebundene `flock` für Generation **und** Socket-Pfad (O_NOFOLLOW, Inode-Prüfung); verwaister eigener unbedienter Socket/Temp-Datei entfernt; Pfade genau einmal und nur unter Lock entfernt; lebender Owner/Server, Fremddatei, Symlink ⇒ Start verweigert, nichts gelöscht.
- **Ist:** `reporter/lib.rs::bind` erzeugt einen Lock mit `create_new(true)` und bindet UNIX-Socket ohne Recovery vorhandener verwaister Socket-/Lock-Artefakte; `Drop` entfernt sie nur bei geordnetem Ende. Nach Crash/kill -9 können verwaiste Pfade einen Neustart blockieren.
- **Soll:** exklusive, prozessgebundene Ownership (z. B. Advisory `flock`), sichere Live-Server-Probe vor Entfernen nur **eigener** stale Socket-Datei, generation-gebundene Umstellung; niemals fremdes lebendes Socket löschen oder zweiter Writer werden. Permission- und symlink-sichere Pfadidentität. Kein Force-Unlink ohne belegte Inaktivität.
- **Tests:** Crash/restart, aktiver fremder Owner, races zwischen 2 Starts, stale lock+socket, symlink attack, falsche Permission, Gateways während Rebind, idempotentes geordnetes Stop.
- **Abnahme:** geordneter und Crash-Neustart ohne Duplikat-Reporter, keine Daten-/Client-Übernahme. **Commit/Evidence:** —.


### R23 — F-F: Reporter-Diagnose, Single-Client und leere Heartbeat-Watermarks

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] **LOW / OBSERVABILITY-CONTRACT · SOURCE_CONFIRMED, GATEWAY_EXPECTATION_VERIFIED · Cross-Ref D9/R6/R20.** · Fix `119da22` · Test `r23_heartbeat_watermarks_follow_the_sent_state_frames` (fail-before: leere Watermarks ⇒ FAIL) — Heartbeat trägt je in dieser Session gesendetem Topic `StateWatermark{topic, generation, highest_seq_sent}` nach `stream.proto:228-237`; seq erst nach erfolgreichem Send verbucht. Gateway-Erwartung geprüft: gleiche Semantik wie der eigene Client-Heartbeat (`gateway/src/ws.rs:198`), Engine-Heartbeat-Inhalte werden nur als Liveness gelesen (`gateway/src/engine.rs:238`). Session-/Accept-Fehler über nicht blockierende `console::line` statt `eprintln!`. Single-Client bleibt (einziger Client: Gateway). Bestehende Tests: Reconnect/Seq, langsamer/getrennter Client, Burst-Coalescing; blockierte stderr nur über den generischen `console`-Drop-Test belegt, kein Reporter-spezifischer Test.
- **Ist:** `reporter/lib.rs` loggt Fehler per `eprintln!` im Reporter-/Session-Kontext, `Heartbeat.state_watermarks` wird als leeres Array erzeugt, Session-Loop nimmt einen Reader/Client je Verbindung an. Welche Watermarks und parallele Leser erforderlich sind, hängt vom tatsächlichen Gateway-Wire-Vertrag ab.
- **Soll:** Reporter-Lifecycle darf Engine nicht durch blockiertes stderr beeinträchtigen; optional safe bounded Diagnose. Tatsächliche Gateway-Semantik von Heartbeat, Cursor und LatestValue-State verifizieren, dann minimalen Vertrag mit deterministischer Sequence/Generation implementieren. Kein ungeprüfter Multi-Client-Broadcast im Hotpath.
- **Tests:** blockierte stderr, Verbindung stirbt, erneutes Accept, langsamer Client, Heartbeat-Watermark nach N STATE Updates, Gateway reconnect, Queue-Überlast.
- **Abnahme:** nachweislich vertragskonformes Status-Streaming ohne Engine-Abhängigkeit. **Commit/Evidence:** —.


### R24 — Market-only ohne Econ: `seen`-Population über lange Captures nicht explizit begrenzt

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **LOW / LONG-RUN MEMORY PROFILE · Cross-Ref R12/M1/PERSIST.** · **PARTIAL** `119da22` · Test `r24_market_only_retention_is_configured_bounded_and_fail_closed` (fail-before per Mutation) — Market-only `seen`, Event-Edges und Strict-`live_receipts` an `P0_MARKET_RETENTION` gebunden (Default 262.144 = Econ-Default, zulässig 1..=2.000.000; ersetzt das 100.000-Literal); Überlauf ⇒ `MARKET_RETENTION_OVERFLOW`/`MARKET_EVENT_RETENTION_OVERFLOW` INCOMPLETE, keine Verdrängung, Duplikat am Cap kein Overflow. RAM-Vertrag: höchstens Cap eindeutige Einträge je Struktur. **Offen:** 1M-Signaturen-Memoryprofil, Test des Event-Retention-Zweigs, Report-Parität Market-only vs. Econ.
- **Ist:** `market/core.rs::ObserverCore` hält `seen: HashSet<String>`; Reviewer meldet im Market-only-Pfad ohne Econ keine vergleichbare Kapazitätsgrenze. Für normale 4h-Econ-Runs ist der bounded-Kapazitätsvertrag R12 relevant, für unabhängige Tagesläufe kann der Market-only-Status ohne Cap anwachsen. Reviewer-Schätzung „~100 MB/Tag“ ist nicht gemessen.
- **Soll:** Memory-Profil und Relevance-/Dedupe-Invariante bei 4h/24h Market-only vergleichen; bounded Strategie nur wenn tatsächlicher Growth belegt und Verlustfreiheit der Dedupe-/Truth-Zählung sichergestellt ist. Keine stille LRU-Verdrängung eindeutiger TX.
- **Tests:** 1M eindeutige Signaturen im Fake, Duplikate, Memory-Budget/Overflow, Market-only vs Econ, Report-Parität.
- **Abnahme:** ausdrücklich geklärter RAM-Vertrag je Modus, keine Überlaufverschleierung. **Commit/Evidence:** —.


### R25 — Analytics-Supervisor: ungeordneter Abort verliert letzten Journalabschluss

**Abgleich:** Abgehakt in Quellregister; keine unabhängige Re-Abnahme.

- [x] A5 · Fix `a8bf405` · Test `analytics_abort_waits_for_the_owners_until_the_market_journal_is_sealed` — `supervise()` wartet nach Abbruch (Analytics-Ende/Deadline) begrenzt durch `shutdown_grace_seconds` (Default 180 s, `P0_SHUTDOWN_SECS`) auf Shadow- und Market-Thread und joint sie; läuft einer danach noch: `C0_E1_OWNERS_NOT_QUIESCENT`, beide Owner als failed, kein Hängen. Hinweis: die Market-Shadow-Barriere selbst wartet bis `shadow_drain + quiescence_grace` (330 s) und kann die 180-s-Grace überschreiten ⇒ dann fail-closed gemeldet.
- [x] **LOW / FAILURE-EVIDENCE CONTRACT · BEHAVIOR_TO_VERIFY · Cross-Ref H1/D1/R2/R22/PERSIST.**
- **Ist:** Reviewer sieht `startup.rs::supervise` mit hartem Prozessende bei Analytics-Panic/Deadline und potentiell noch laufenden Market-/Shadow-Threads. Ein unvollständiger Journal-Tail wird durch H1 fail-closed erkannt; die Evidence ist dann ggf. unvollständig, aber kein falscher PASS. Ob dieselbe Gefahr auf dem aktuellen Launchpfad noch besteht, beim Implementieren erneut lesen.
- **Soll:** begrenztes koordiniertes Stop-/Quiescence-/Incomplete-Abschlussprotokoll **nur sofern** der Abortpfad dafür beweisbar Zeit und Owner-Kontrolle besitzt; bei harter Deadline niemals eine falsche COMPLETE-Versiegelung oder Endloswarten einführen. Bereits persistierte Frames nicht löschen.
- **Tests:** Analytics-Panic, Deadline, Full Writer-Queue, Shadow-RPC in flight, Market-Drain Timeout, Re-Read über JournalVerifier, kein falscher READY/PASS.
- **Abnahme:** definierter Fail-closed-Status mit maximal möglicher konsistenter Evidence. **Commit/Evidence:** —.


### R26 — Folgereview: tatsächliche kanonische Market-Wahrheit noch nicht vollständig gegengelesen

**Abgleich:** Nicht abgehakt in Quellregister.

- [ ] **REVIEW_REQUIRED / NICHT AUTOMATISCHES BUG- ODER 4h-BLOCKER-URTEIL · Cross-Ref R13–R15/C0-E1/FE-00/FE-03.**
- **Ungelesene Quellen im Review `e9872cb`:** `market_truth.rs`, `market_verification.rs`, `market/strict_driver.rs`, `market_gap.rs`, `analytics/report.rs`, `analytics/census.rs` (Pfade je tatsächlichem Repo bestimmen) und die vollständige Gateway-Seite. Ein Teil davon wurde für R13 nur punktuell nachgelesen, nicht im Umfang eines Vertragsaudits.
- **Auftrag:** als Erstes `market_truth.rs` mit den gepinnten Classic/Receivership/Forced-Deleverage/Bankruptcy-Discriminatoren und tatsächlichen Top-Level-/CPI-/ALT-/LoadedAddress-Pfaden analysieren: Welche Chain-Events zählen kanonisch, welche sind nur provisional, wie wird Finality/InnerInstruction/Receiver-Identität nachgewiesen? Anschließend StrictDriver-/Verifier-/Gap-/Report-/Census- und Gateway-Vertragsmatrix mit RED-Fixtures. Zusätzliche echte Bugs erst nach Quellbeweis unter neuer ID anlegen; nicht aus ungelesenem Umfang einen Defekt erfinden.
- **Abnahme:** nachprüfbarer Source-/Acceptance-Report mit offenen Coverage-Lücken; keine neue Selbstfreigabe von G-B/G-C/G-D. **Commit/Evidence:** —.

**Review-8-Priorität (keine Roadmap-Neudefinition):** (1) R12 historische Zahlen offline lesen, (2) R14/R15 analytische Parität und gültige FE-00-Messgrößen klären, (3) R13 als 600-s-Kalibrierungsumfang planen und erst nach getrennter Freigabe messen, (4) R18/R19 vor echter Econ4h-Frontend-Anzeige, (5) R16/R17/R20–R25 nach Gate-/Profilinglage, (6) R26 Quellenreview, Beginn `market_truth.rs`. Der bisherige §16 bleibt verbindlich, inklusive FREE_ONLY, G-B/G-C/G-D-Halten, C1/R1–R11 und F6-Fremddateien. Jede erledigte ID braucht neuen, getesteten Commit mit Exit-Codes/Evidence; sonst `- [ ]` beibehalten.


## C. Neue Z1–Z11 und Q1–Q13 — konkrete Codefundstellen, Mängel, Pflichtnachweise

**Achtung: Die IDs N1–N8 existieren bereits für andere Registertickets.** Die hier im Gespräch neu bezeichneten N1–N8 heißen hier Z1–Z8. Mehrfachbezüge (vor allem Z1/Q4) sind **nicht** doppelt als Bugs zu zählen.

| ID | Priorität | Nachweisstatus | Verwandte Registertickets |
|---|---|---|---|
| Z1 | HOCH | Code bestätigt | Q4; R14 |
| Z2 | KRITISCH für Latenzvertrag | Code-/Vertragsmangel bestätigt; E2E-Messung fehlt | B6 |
| Z3 | HOCH | Direkt im Code bestätigt | E9 |
| Z4 | MITTEL | Anforderung und Lücke aus Review; Messung fehlt | E9 |
| Z5 | HOCH | Fachlicher Review-Befund; Binärmigration noch nicht getestet | R14; ECON_SNAPSHOT/ECON_FINAL |
| Z6 | MITTEL | Logpfad direkt gesehen; Wiederholungswirkung laut Review | B16 |
| Z7 | KRITISCH vor LIVE | Builder-Review-Befund, latente Safety-Lücke | B15; C11 |
| Z8 | MITTEL | Paritätsprüfung offen; kein bewiesener EMODE-Rechenfehler | B12 |
| Z9 | MITTEL | Test-/Profilinglücke | B7 |
| Z10 | PRÜFEN | Hypothese, kein bestätigter Defekt | B4; X1 |
| Z11 | PROZESS | Gate-Anforderung | Register übergreifend |
| Q1 | HOCH | Code bestätigt | B1; B15; C3 |
| Q2 | KRITISCH vor B1/C3 | Latente Codefehler bestätigt; aktiver LIVE-Pfad derzeit gesperrt | B1; C3; B15 |
| Q3 | HOCH | Code bestätigt | F1; Econ-Reports |
| Q4 | HOCH | Duplikat zu Z1, kein zweiter Produktionsfehler | Z1 |
| Q5 | HOCH | Code bestätigt | C10 |
| Q6 | MITTEL | Strukturmangel bestätigt; aktuell fehlender Kind nicht bewiesen | Journal/PERSIST |
| Q7 | NIEDRIG-MITTEL | Strukturmangel; kein falscher Index bewiesen | C5; Provider |
| Q8 | MITTEL | Strukturmangel bestätigt; kein unmittelbarer Runtime-Fehler bewiesen | Owner/Setup |
| Q9 | NIEDRIG | Build-Fehlerbehauptung widerlegt; redundantes cfg bestätigt | crate_health |
| Q10 | MITTEL | Code bestätigt; tatsächliche Config-Divergenz nicht gezeigt | L4; Setup |
| Q11 | NIEDRIG-MITTEL | Runtime-Parse bestätigt; konkrete SDK-API nicht kompiliert | E2 |
| Q12 | MITTEL | Code bestätigt | Econ-Tests |
| Q13 | MITTEL | Dokumentationslücke direkt bestätigt | B6; C10; R13 |

### Z1 — HOCH

**Nachweisstatus:** Code bestätigt. **Bezug:** Q4; R14.

**Codeort:** `runtime/src/analytics/econ_canonical.rs, loss(), Zeilen ~296–327`.

**Technischer Befund:** `_ => "UNKNOWN"` für bekannte `LossReason`-Varianten; darunter können RpcBudget, RpcDeadline, Tip, Submission, CircuitBreaker, IntegrationRefresh und UnsupportedIntegration unzutreffend in UNKNOWN landen.

**Was fehlt / überprüfbare Abnahme:** Exhaustive Zuordnung; Golden-Tests für jeden Verlustgrund, kein pauschaler Wildcard-Zweig.

### Z2 — KRITISCH für Latenzvertrag

**Nachweisstatus:** Code-/Vertragsmangel bestätigt; E2E-Messung fehlt. **Bezug:** B6.

**Codeort:** `runtime/src/execution/pipeline.rs; solana/src/provider.rs; solana/src/submit/rpc.rs; runtime/src/orchestration/state_owner/setup.rs`.

**Technischer Befund:** Kandidaten-Deadline wird nicht nachweisbar in alle blockierenden Jupiter-, Fee-, Permit-, Simulation-, HTTP- und Retry-Aufrufe durchgereicht. Die 5-s-Konfiguration begrenzt einzelne Provider-Aufrufe mit bis zu 30 s Timeout nicht automatisch.

**Was fehlt / überprüfbare Abnahme:** Eine monotone Deadline in allen Adaptern; typisierte Fee-Fehler; Tests gegen lokal kontrollierte hängende/verspätete HTTP-Responses, Deadline+100 ms nach definierten Bedingungen.

### Z3 — HOCH

**Nachweisstatus:** Direkt im Code bestätigt. **Bezug:** E9.

**Codeort:** `runtime/src/orchestration/state_owner/detect.rs:154–155`.

**Technischer Befund:** `.observation_accounts(&acct).unwrap_or_default()` ersetzt einen Fehler durch eine leere Liste. Missing Dependencies werden dadurch semantisch ununterscheidbar von fehlerhaftem Observation-State.

**Was fehlt / überprüfbare Abnahme:** Bei Fehler keine Projektion, explizite Evidenz und Zähler; negative Bank-Fixtures.

### Z4 — MITTEL

**Nachweisstatus:** Anforderung und Lücke aus Review; Messung fehlt. **Bezug:** E9.

**Codeort:** `runtime/src/orchestration/state_owner/*; Projection-Statistik`.

**Technischer Befund:** Requested, reused, discarded-at-result, discarded-at-consume und observation-unknown nicht vollständig und eindeutig bis Shutdown ausgewiesen.

**Was fehlt / überprüfbare Abnahme:** Exakte Zähler und finaler Snapshot; keine Doppelzählung oder Per-Ereignis-Logflut.

### Z5 — HOCH

**Nachweisstatus:** Fachlicher Review-Befund; Binärmigration noch nicht getestet. **Bezug:** R14; ECON_SNAPSHOT/ECON_FINAL.

**Codeort:** `runtime/src/analytics/econ_canonical.rs (race_margins, technical); analytics/econ.rs; analytics/payload.rs`.

**Technischer Befund:** Allgemeine Race-Population nicht gleich technische signierte/simulierte/gematchte Population. `ECON_SNAPSHOT` und `ECON_FINAL` haben historische Schema-2/3-Layouts; eine verschachtelte Erweiterung bricht sonst Bincode-Parität.

**Was fehlt / überprüfbare Abnahme:** `_all` und `_technical` getrennt, Samplezahlen/UNKNOWN; Schema-2/3-Altstrukturen und Golden-Frames; keine historische Wert-Erfindung.

### Z6 — MITTEL

**Nachweisstatus:** Logpfad direkt gesehen; Wiederholungswirkung laut Review. **Bezug:** B16.

**Codeort:** `runtime/src/orchestration/state_owner/events.rs:~712`.

**Technischer Befund:** `dependency_gap: bank=... code=... detail=...` wird bei wiederholten Updates erneut erzeugt; Risiko für unnötiges Logwachstum.

**Was fehlt / überprüfbare Abnahme:** Je Bank/Fehlerklasse nur Eintritt, Änderung und Recovery loggen; begrenzter Zustand; Wiederholungstests.

### Z7 — KRITISCH vor LIVE

**Nachweisstatus:** Builder-Review-Befund, latente Safety-Lücke. **Bezug:** B15; C11.

**Codeort:** `Builder-Funding-/Sizing-Pfad; liquidation/src/safety.rs:128–133; BUGS.md B15`.

**Technischer Befund:** ZeroPrefund/FlashFunded übergeben laut Review Inventar-Hold 0, obwohl Restinventar möglich ist. Die vorhandene `checked_add`-Aggregation in Safety ist dagegen korrekt.

**Was fehlt / überprüfbare Abnahme:** Vorläufig vollen konservativen Hold; B15 PARTIAL bis tatsächliche Tokenflüsse, Mint, USD-Werte, Restinventar und Wallet-Deltas belegt sind.

### Z8 — MITTEL

**Nachweisstatus:** Paritätsprüfung offen; kein bewiesener EMODE-Rechenfehler. **Bezug:** B12.

**Codeort:** `Health-/Sizing-EMODE-Code; BUGS.md B12`.

**Technischer Befund:** Gemeinsamer Filter wäre Refactoring, dessen Gleichheit mit gepinntem Marginfi-Referenzverhalten zu prüfen ist.

**Was fehlt / überprüfbare Abnahme:** Pinned-Fixtures; keine neue Bugfix-Zählung ohne reproduzierten Fehler.

### Z9 — MITTEL

**Nachweisstatus:** Test-/Profilinglücke. **Bezug:** B7.

**Codeort:** `Health/Sizing Oracle-/Observation-Optimierung; BUGS.md B7`.

**Technischer Befund:** Optimierung umgesetzt, aber numerische Äquivalenz und Allokations-/Perzentilmessungen nicht vollständig nachgewiesen.

**Was fehlt / überprüfbare Abnahme:** Vor/Nach-Parität und p50/p95/p99 plus Speicher-/Allokationsprofil.

### Z10 — PRÜFEN

**Nachweisstatus:** Hypothese, kein bestätigter Defekt. **Bezug:** B4; X1.

**Codeort:** `required_dependency_keys (Rent-Sysvar); X1 NetworkAudit BOOT-12`.

**Technischer Befund:** Neues Rent-Sysvar in Required-Keys könnte mit genehmigtem Audit-Record ohne diesen Key `BOOT_IDENTITY_DRIFT` erzeugen.

**Was fehlt / überprüfbare Abnahme:** Gezielte Audit-Record-/Keyset-Fixtures; tatsächliches Sollverhalten klären.

### Z11 — PROZESS

**Nachweisstatus:** Gate-Anforderung. **Bezug:** Register übergreifend.

**Codeort:** `BUGS.md; STATUS.md; MEASUREMENT_LEDGER.md; Git-HEAD`.

**Technischer Befund:** Tested-SHA und freizugebender HEAD dürfen nicht auseinanderlaufen.

**Was fehlt / überprüfbare Abnahme:** Code-/Test-/Commit-SHA, Diff und Exit-Codes unverwechselbar dokumentieren.

### Q1 — HOCH

**Nachweisstatus:** Code bestätigt. **Bezug:** B1; B15; C3.

**Codeort:** `liquidation/src/safety.rs:163–212 (preview/check)`.

**Technischer Befund:** Zahlreiche positionale nackte `u64` (Exposure/Inventory/Tip/Slippage) erlauben vertauschte Einheiten und Argumente ohne Compilerwarnung; `allow(too_many_arguments)` unterdrückt Signal.

**Was fehlt / überprüfbare Abnahme:** AttemptRequest, typisierte Einheiten `UsdMicro`, `Lamports`, `Bps`, gültige Bounds.

### Q2 — KRITISCH vor B1/C3

**Nachweisstatus:** Latente Codefehler bestätigt; aktiver LIVE-Pfad derzeit gesperrt. **Bezug:** B1; C3; B15.

**Codeort:** `liquidation/src/safety.rs:236–244, 263–268`.

**Technischer Befund:** `saturating_sub` verschleiert doppelte/zu hohe Freigabe; `(inventory_outstanding as i128 + inventory_delta).max(0) as u64` verschleiert negative Salden bzw. konvertiert ungesichert; `realized_pnl += p` ohne checked_add; None-Arm nach früherem Return unerreichbar.

**Was fehlt / überprüfbare Abnahme:** Atomare geprüfte Ledgertransaktionen, checked_sub/checked_add/try_from, HALT bei Invariant-Verletzung; Regression doppelte Freigabe/Overflow.

### Q3 — HOCH

**Nachweisstatus:** Code bestätigt. **Bezug:** F1; Econ-Reports.

**Codeort:** `analytics/econ.rs:674ff; execution/pipeline.rs:~881`.

**Technischer Befund:** `format!("{l:?}")` wird Report-Bucket, `format!("{:?}", other)` persistiertes simulation_result. Debug-Layout ist kein stabiles Datenschema.

**Was fehlt / überprüfbare Abnahme:** Eigene explizite code()-Strings, Versionierung, historische Lesbarkeit, Python-Parität.

### Q4 — HOCH

**Nachweisstatus:** Duplikat zu Z1, kein zweiter Produktionsfehler. **Bezug:** Z1.

**Codeort:** `analytics/econ_canonical.rs:296–327`.

**Technischer Befund:** Wildcard-Enum-Match akzeptiert neue `LossReason` ohne bewusstes Update.

**Was fehlt / überprüfbare Abnahme:** Exhaustive Matches und ggf. Clippy `wildcard_enum_match_arm`.

### Q5 — HOCH

**Nachweisstatus:** Code bestätigt. **Bezug:** C10.

**Codeort:** `execution/pipeline.rs:207–212 sowie classify_plan_error; C10`.

**Technischer Befund:** `plans()` liefert `Result<Vec<ReceivershipPlan>, String>` und `INTEGRATION_STALE:`-Textmarker dienen zur fachlichen Fehlerklassifikation.

**Was fehlt / überprüfbare Abnahme:** `PlanError`-Enum mit expliziter Übertragung bis Pipeline/Journale.

### Q6 — MITTEL

**Nachweisstatus:** Strukturmangel bestätigt; aktuell fehlender Kind nicht bewiesen. **Bezug:** Journal/PERSIST.

**Codeort:** `analytics/journal.rs:kind-Konstanten und `ALL`; analytics/payload.rs:`typed_kinds!``.

**Technischer Befund:** Neue Kind-Einträge erfordern Änderungen in mehreren manuellen Registrierungen.

**Was fehlt / überprüfbare Abnahme:** Eine generierte Registry oder strikter Vollständigkeitstest.

### Q7 — NIEDRIG-MITTEL

**Nachweisstatus:** Strukturmangel; kein falscher Index bewiesen. **Bezug:** C5; Provider.

**Codeort:** `solana/src/provider.rs:65,176 und Workload::index()`.

**Technischer Befund:** Handgezählte `[Instant; 7]`/`[now; 7]` im unmittelbar abgerufenen `main`; acht Workload-Varianten teilen sieben Pacing-Slots (`Reconcile` mit `Winner`). Fables `[8]` weicht vom abgerufenen Stand ab.

**Was fehlt / überprüfbare Abnahme:** Explizite Slotzahl, Compile-Assert, keine zusätzliche Reconcile-Rate.

### Q8 — MITTEL

**Nachweisstatus:** Strukturmangel bestätigt; kein unmittelbarer Runtime-Fehler bewiesen. **Bezug:** Owner/Setup.

**Codeort:** `runtime/src/orchestration/state_owner/setup.rs (beim Abruf ~1.150 Zeilen)`.

**Technischer Befund:** Viele Einzel-Nullinitialisierungen werden manuell in `OwnerMetrics` übernommen; gekoppelte Ressourcen-/Owner-Initialisierung.

**Was fehlt / überprüfbare Abnahme:** Default-/Teilstrukturen bei verhaltensgleichen Regressionen.

### Q9 — NIEDRIG

**Nachweisstatus:** Build-Fehlerbehauptung widerlegt; redundantes cfg bestätigt. **Bezug:** crate_health.

**Codeort:** `runtime/src/orchestration/mod.rs und state_owner/setup.rs`.

**Technischer Befund:** `#[cfg(feature = "crate_health")] mod state_owner;` schützt das gesamte Modul; zahlreiche innere gleiche cfg-Attribute sind überflüssig.

**Was fehlt / überprüfbare Abnahme:** Optionale Bereinigung; no-default-features Matrix getrennt prüfen.

### Q10 — MITTEL

**Nachweisstatus:** Code bestätigt; tatsächliche Config-Divergenz nicht gezeigt. **Bezug:** L4; Setup.

**Codeort:** `runtime/src/orchestration/state_owner/setup.rs:28,52,351,424 und 381,434`.

**Technischer Befund:** `P0_RPC_URL` mehrfach gelesen, `P0_MAX_SLIPPAGE_BPS` mehrfach geparst; Token-ID-Literale duplizieren Protokollkonstanten.

**Was fehlt / überprüfbare Abnahme:** Einmal parse/validate, ausschließlich kanonische Werte weiterreichen.

### Q11 — NIEDRIG-MITTEL

**Nachweisstatus:** Runtime-Parse bestätigt; konkrete SDK-API nicht kompiliert. **Bezug:** E2.

**Codeort:** `runtime/src/execution/pipeline.rs:141–146`.

**Technischer Befund:** `OnceLock<Option<Pubkey>>` mit statischem `COMPUTE_BUDGET_PROGRAM_ID.parse().ok()`, Parse-Fehler führt zu `false`.

**Was fehlt / überprüfbare Abnahme:** Typsichere konstante ID gemäß gepinntem Solana-SDK 3.0.0 (Compilerprüfung nötig).

### Q12 — MITTEL

**Nachweisstatus:** Code bestätigt. **Bezug:** Econ-Tests.

**Codeort:** `runtime/src/analytics/econ_tests.rs:866,972,1077,1192,1328,2083`.

**Technischer Befund:** Sechs wiederholte `Pipeline { ... }`-Initialisierungen und weitere Test-Fixture-Duplikate.

**Was fehlt / überprüfbare Abnahme:** Gemeinsamer Fixture-Builder und MapSource bei gleicher Coverage.

### Q13 — MITTEL

**Nachweisstatus:** Dokumentationslücke direkt bestätigt. **Bezug:** B6; C10; R13.

**Codeort:** `project_0/docs/BUGS.md B6,C10,R13`.

**Technischer Befund:** B6/C10 mit mehrfachen Checkboxen bzw. offenen Evidence-Vermerken; C10 `SHA im nächsten Ticket-Commit`; R13 abgehakt nur A4-Teilfix.

**Was fehlt / überprüfbare Abnahme:** Abnahmescope, eindeutige Commit/Evidence und Reststatus korrekt trennen.

## D. Abschlussbewertung ohne Doppelzählung

1. **Kanonische Anzahl:** 114 Registereinträge; 81 dokumentiert abgehakt, 33 nicht abgehakt. Z-/Q-Punkte sind Querverweise oder zusätzliche Qualitätsbefunde und dürfen nicht pauschal addiert werden.
2. **Abgehakt, aber fachlich zu weit verstanden:** B6 (nur Teilbudget, keine vollständige End-to-End-Deadline) und R13 (abgehakt nur A4-Teilfix, Strict-Scan-Durchsatz ausstehend).
3. **Bereits offen/teilweise mit konkreten zusätzlichen Nachweisen:** B1, B3, B4, B7, B15, C3, C11, C12, E2, E9, R12, R19, R24 usw. bleiben bis definiertem Gate offen.
4. **Zusatzmängel auf bereits abgeschlossenen Bereichen:** B16/Z6 (Logspam), C10/Q5 (Stringvertrag), B12/Z8 (Semantikprüfung), F1/Q3 (andere Debug-Persistenz), X1/Z10 (unbestätigte Hypothese). Originalfix dadurch nicht automatisch widerlegt.
5. **Aktive Safety-Gates:** Keine LIVE-/Submission-Freigabe, keine kostenpflichtigen RPC-Läufe, keine stillen Statusänderungen; jede tatsächliche neue Abnahme benötigt reproduzierbare Code-/Test-/Commit-Evidence.

**Nicht behauptet:** dass sämtliche 81 dokumentierten Fixes erneut auf dem aktuellen HEAD kompiliert, mit Mutationstests geprüft und vollständig als produktionsreif nachgewiesen wurden.

## E. Nachprüfung vom 20.10.2026 — aktualisierte Z1–Z19 und Q1–Q14 (ergänzt)

**Quellabgleich:** Ergänzender Review zu GitHub `main` nach Commit `dda08af`, direkte Einzeldatei-Lesung sowie Fables vollständig gelesene Dateien. **Keine Codeänderung/Tests/LIVE.** Der folgende Abschnitt **korrigiert ausdrücklich** veraltete Bewertungen in Abschnitt C (Z7, Z8, Z10). Die 114 Registereinträge und alle wörtlichen Original-Ticketdetails darüber bleiben **unverändert**. Diese Nachprüfung ist gegenüber widersprechenden älteren Abschnitten maßgeblich; nicht als neue 19 unabhängige Bugs addieren.

| ID | Korrigierter Status | Codebefund / Beschreibung | Noch nachzuweisen oder zu verbessern |
|---|---|---|---|
| **Z1** | BESTÄTIGT | econ_canonical.rs::loss(): 10 bislang als UNKNOWN behandelte Varianten: Ingress, State, CircuitBreaker, IntegrationRefresh, UnsupportedIntegration, Sizing, Submission, Tip, RpcBudget, RpcDeadline. Zusätzlich vorbelegter, nicht befüllter CAPITAL-Bucket und überladener Build-Grund. | Exhaustive Loss-Buckets; jedes Variant und Golden testen; BUILD-Subursachen sauber nach fachlichem Vertrag aufteilen. |
| **Z2** | BESTÄTIGTER VERTRAGSMANGEL | pipeline.rs: network_deadline vor Fee-/Simulation-Calls geprüft, nicht bis zu Jupiter/plans und in RpcSubmitter/BudgetedSender durchgereicht; submit/rpc.rs erstellt Simulation-RpcClient mit 30-s-Timeout. | Ein einziges monotones End-to-End-Budget einschließlich Permit, HTTP-Body, Retries und Jupiter. Adapter-/Timeout-Tests; vorgeschlagene DeadlineCell ist Designidee, nicht bereits verifizierter Fix. |
| **Z3** | BESTÄTIGT | state_owner/detect.rs:154–155 observation_accounts(&acct).unwrap_or_default(). | Fail-closed und geprüfter Fehler-/Zählerpfad. |
| **Z4** | DESIGNMANGEL | detect.rs und remote_apply.rs geben projection_dependencies_changed für at=consume/result per owner_log aus. | An Stelle des Per-Event-Logs eindeutig gezählte und finalisierte ProjectionStats. |
| **Z5** | BESTÄTIGTER AUSWERTUNGSREST | econ_canonical.rs: race_margins und summarize berücksichtigen technische Grundgesamtheit nicht separat. | _all/_technical, Samples; binäre Alt-Schemata 2/3 bei Layoutwechsel explizit migrieren. |
| **Z6** | BESTÄTIGTER LOGPFAD | events.rs: dependency_gap wird aus scan.gaps pro relevantem Bankupdate geloggt. | Bounded State-Deduplikation pro Bank/Fehlerklasse und Recovery-Test. |
| **Z7** | URSPRÜNGLICHE BEHAUPTUNG WIDERLEGT / REST OFFEN | builder.rs:1100 FlashFunded => continue; ZeroPrefund quote_exact_in bekommt route_input_amount = execution_amount_for_asset(...).underlying_out (bzw. withdraw_amount) und verlangt route.amount_in exakt gleich; pauschal verbliebenes Collateral und daher zwangsweise voller Seized-Hold sind NICHT belegt. Liability-Überschuss minimum_executable_out minus repay_amount und Integrations-Rundungsstaub müssen separat bilanziert werden. | Originalen vorgeschlagenen Voll-Hold NICHT blind implementieren. Inventardefinition (Gewinn-Token) fachlich bestimmen, nach echten Tokenflüssen/Wallet-Delta konservativ bilanzieren. B15 bleibt PARTIAL. |
| **Z8** | FILTERVERDACHT WIDERLEGT | sizing.rs::maintenance_emode filtert active && !empty(Liabilities); health.rs::maintenance_emode_config filtert active und überspringt empty(Liabilities) anschließend. Beide verwenden maintenance_emode_config_of. | Nur optionale Strukturvereinheitlichung. Unterschiedliche Bank-Owner-Prüfung in Health/Sizing als B2-Reviewrest dokumentieren, nicht als bewiesene EMODE-Abweichung. |
| **Z9** | TESTLÜCKE | health.rs::components_of liefert eine gemeinsame AccountInfo-Sammlung für zweimaligen pinned get_health_components-Aufruf. | Äquivalenz mit Integrationsbanks und messbare Performance. |
| **Z10** | DRIFT-HYPOTHESE WIDERLEGT / COVERAGE-REST | boot_audit.rs::bind_state iteriert for proof in &self.accounts, nicht über required. Fehlender Rent-Sysvar-Key im Audit kann daher allein keinen BOOT_IDENTITY_DRIFT auslösen. Nicht auditierte required Keys werden jedoch nicht ausgewiesen; Bank-Erkennung bindet BANK_PAYLOAD_LEN_0_1_11. | Zusätzliche Coverage required_not_audited ausweisen; 0.1.12-Banklänge für P1B untersuchen. Vorhandene X1-Driftprüfung nicht abschwächen. |
| **Z11** | PROZESS | Bisherige Gateverweise sind keine neue vollständige Verifikation des aktuellen Commitstands. | Tested-SHA, Exit-Codes, Evidence für tatsächliches main. |
| **Z12** | BESTÄTIGTER CODEPFAD / SEMANTIKABNAHME OFFEN | builder.rs::plans(): self.bank(...)? / self.receivership(...)? / bank_full(...)? / execution_amount_for_asset(...)? können Fehler eines einzelnen Paares auf den gesamten plans()-Return propagieren; Routefehler werden separat gesammelt. | Fehlerisolation paarweise prüfen; gültige Pläne anderer Paare erhalten, jedoch keine sicherheitsrelevanten globalen Fehler verschlucken. Test mit gültigem und ungültigem Paar. |
| **Z13** | BESTÄTIGTER CODEPFAD | state_owner/detect.rs:483 ff: is_bad_debt && equity_assets <= 0 überschreibt rec.loss/rec.reason nach evaluate; vorige Gründe (z. B. RpcBudget/State) können verschwinden. | Priorität und Provenienz des ursprünglichen Grundes bewahren; Negativtests. |
| **Z14** | BESTÄTIGTES RISIKO / RACE-REPRO FEHLT | events.rs:340–361 initiale bootstrap probes via self.producer.try_push(IngressEvent::Account) in denselben bounded Ingresskanal wie WSS; ws_rpc.rs::deliver meldet beim Channel::Full sink overflow und stoppt Session. | Probes getrennt/gebunden schedulen; kontrollierter Konkurrenztest bei Kanalnähe zur Kapazitätsgrenze. |
| **Z15** | LOW / DESIGN | planner.rs::steps_of erzeugt laut Quellreview IntegrationWithdraw, PriceUpdate, RepayFromInventory nicht; Validierungsregeln daher in diesem Pfad nicht erreichbar. | Über andere Step-Erzeuger prüfen; ungenutzte Regeln dokumentieren/entfernen oder echten Pfad implementieren. |
| **Z16** | LOW / DOKUMENTATION | builder.rs, health.rs kommentieren UNTESTED / VERIFY; record.rs beschreibt JSONL, obwohl native Journale existieren. | Quellkommentare auf fachlich belegten Istzustand bringen. |
| **Z17** | BESTÄTIGTE TESTLÜCKE | sizing.rs:1358ff normal_and_closeout_bounds_are_distinct prüft nur Bool-Literale false/true, ruft sizes nicht auf. | Echte normal/closeout Fixture, Mutation gegen Grenzen; Test darf bei invertierter Bedingung nicht grün bleiben. |
| **Z18** | IDENTITÄTSVALIDIERUNG FEHLT IM GENERISCHEN DECODER | liquidation/integration.rs::pod_after_disc(state,key): raw(state,key) verwirft Owner und prüft anschließend nur Mindestlänge und POD-Cast von data[8..need], nicht Discriminator oder erwarteten Owner. Andere Decoder können intern eigene Checks haben; jeder externe Integrationstyp gesondert betrachten. | Pro Call-Site korrekten Integrationsprogramm-Owner und erwarteten Discriminator aus gepinnter Quelle nachweisen, dann falsche Identität mit eindeutigen Negativtests ablehnen. |
| **Z19** | BESTÄTIGTER EVENT-PFAD | solana/ingress/ws_rpc.rs: frame timeout bei noch nicht erreichtem idle_timeout führt vor dem Health-Event-Block zu continue. Daher steigt last_frame_age_ns ohne Frames nicht regelmäßig im Eventstream. | Health-Snapshot auch während Stille nach periodischem Tick; WSS-Fake ohne Frames, steigendes Alter ohne falschen Fatal. |
| **Q14** | LOW / DUPLIKATION | integration.rs::kinds_for_account und pricing_kinds_for_account laut Review ähnlich. | Erst semantische Unterschiede prüfen, dann gemeinsame Iteration bei Parität. |

### Geänderte Bewertung bereits abgehakter Originaltickets

| Originalticket | Neue Bewertung | Begründung |
|---|---|---|
| B6 | Als erledigt markiert, aber Z2 bleibt Vertragsrest | Kandidatenzeitbudget nicht durch sämtliche externen Aufrufe |
| B12 | Vorheriger EMODE-Fix nicht widerlegt | Z8 Filterparität bestätigt, nur Struktur / B2-Owner-Abgleich |
| B15 | Weiterhin PARTIAL | Kein genereller ZeroPrefund-Collateral-Hold; echte Liability-Gewinn-/Staubreste separat klären |
| B16 | Vorheriger Diagnose-Fix gültig, Z6 neu offen | Wiederholte Logs auf Bankupdates |
| X1 | Alter Drift-Fix nicht widerlegt | Z10 Rent-Drift-Hypothese widerlegt; required_not_audited und Banklänge ergänzende Coverage |
| R19 | Weiterhin PARTIAL | Z19 bestätigt fehlende zeitgetriggerte Health-Meldungen bei stiller WSS |
| E9 | Weiterhin PARTIAL | Z3 und Z4 konkrete verbleibende Fehler/Diagnostiklücken |
| C2 | Weiterhin offen | Health in Detection, Pipeline und Sizing mehrfach; Messung / Cacheparität ausstehend |

**Vollständigkeit und Ungewissheit:** Neue Original-BUGS.md-Tickets werden durch dieses Dokument nicht abgehakt. Alle neu berichteten Compiler-/Laufzeit-/Performance-Effekte ohne eigenständige Test-Evidence behalten ihren begrenzten Nachweisstatus. Ein Codepfad kann als bestätigt gelten, ohne dass seine Auslösung bei realer Mainnetlast bewiesen ist.

## F. Umsetzungsstand (Claude, 20.10.2026) — Abbildung auf BUGS.md §18 (V-Serie)

Die in diesem Dokument Z1–Z8 genannten Befunde sind dieselben wie die Nutzerprüfung „N1–N8“, in `BUGS.md` §18 als **V1–V8** geführt (IDs N1–N8 dort belegt; Q1–Q14 dieses Dokuments bleiben eigene IDs). Nachweis je Eintrag in BUGS.md: Regressionstest, der per Mutation gegen das alte Verhalten fehlschlägt; Gate fmt/Clippy (all/default)/Tests (all/default)/Release/Python grün. Nur offline, keine Mainnet-Läufe.

| bugs_b | BUGS.md | Stand | Commit |
|---|---|---|---|
| Z1 (= Q4) | V1 | `[x]` exhaustive Loss-Buckets, Attributionsmodell, Golden geprüft (nur vorbelegte Nullen/neue Felder) | `423c024` |
| Z2 | V2 (Ergänzung B6) | `[x]` eine Kandidaten-Deadline bis in `BudgetedSender`, Jupiter, Route-Fit, Fee, Simulation; 12 Tests (10 Pipeline-/Adapter-Akzeptanztests mit echtem Adapter gegen Loopback-HTTP, 2 Transporttests) | `78d79c3` |
| Z3 | V3 | `[x]` Observation-Fehler ⇒ keine Projektion, Record + Zähler; Group/FeeState im Fingerprint | `423c024` |
| Z4 | V4 | `[x]` bilanzierende ProjectionStats, periodisch + Shutdown | `423c024` |
| Z5 | V5 | `[x]` `_all` unverändert + `_technical`; ECON Schema 4 mit expliziten V2/V3-Layouts, Golden aus Altjournalen | `423c024` |
| Z6 | V6 | `[x]` dependency_gap nur neu/geändert/aufgehoben | `423c024` |
| Z7 | V7 | **PARTIAL / KONFLIKT:** auf ausdrückliche Nutzeranweisung (Stufe 1) halten ZeroPrefund/FlashFunded vorläufig den vollen `seized_equity_value`. Abschnitt E dieses Dokuments bewertet den Voll-Hold als nicht belegt (der Swap verbraucht die volle Ausführungsmenge; Rest = Liability-Überschuss und Rundungsstaub). Wirkung nur im LIVE-Pfad (gesperrt), Shadow unverändert. **Nutzerentscheidung offen**, Stufe 2 (Token-Flüsse) vor FE-06. | `423c024` |
| Z8 | V8 | `[x]` Struktur: gemeinsamer EMODE-Filter (Semantik unverändert, wie Abschnitt E) | `423c024` |
| — | V9 | offen: technische Plan-Ablehnungen als ECONOMIC_SKIP attribuiert (verwandt mit Q5 `PlanError`) | — |

Weiter offen aus diesem Dokument (Reihenfolge laut Nutzerauftrag): Q2 (Safety-Ledger), Z18 (Integrationsidentität), Z12 (Fehlerisolation), Z14 (Ingress-Race), dann übrige bestätigte Z/Q.

