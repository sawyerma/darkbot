# Project_0 — bug_v2.md: Gegenprüfung des Bug-/Latenz-Reviews (TXT-Päckchen 1–9 vs. MD-Protokoll 9.0)

**Erstellt:** 2026-10-24 · **Geprüfter Code:** `origin/main` `5368c82d` (Native-Code identisch mit `30140710`, dem Pin von MD-Paket 9; die zwei Folge-Commits sind reine Docs) · **Scope:** ausschließlich `project_0/`, kein `bsc/` · **Art:** statische Code-Lesung, **keine Codeänderung, kein Test ausgeführt, kein Benchmark** · **Ablage:** `project_0/docs/bug_v2.md` neben `BUGS.md` (kanonisches Register unverändert).

**Eingaben:** (1) „TXT“ = fremdes AI-Review, Päckchen 1, 2 und 9 im Wortlaut plus konsolidierte Rangliste; (2) „MD“ = `Project_0_Bug_Latenz_Review_Protokoll.md` Version 9.0 (Pakete 1–9). Aufgabe: prüfen, ob TXT, MD oder beide irren, und daraus einen Fixplan ableiten. Fixes erst nach Nutzerfreigabe.

**Hinweis Worktree:** Während der Prüfung enthielt der Project_0-Worktree uncommittete fremde Änderungen (parallele Session, 13 Dateien, u. a. `pipeline.rs`, `builder.rs`, `admission.rs`, `detect.rs`, `setup.rs`, `funding.rs`, `inventory.rs`, `exit.rs`). Sie sind **nicht** Gegenstand dieses Reviews; alle Zeilenangaben beziehen sich auf HEAD `5368c82d` (im Arbeitsbaum ist `pipeline.rs` ab Z. 401 um +1, `builder.rs` ab Z. 401 um +37 verschoben).

---

## 0. Gesamturteil in fünf Sätzen

1. **Das MD-Protokoll hat in allen Kernurteilen recht**: Simulation läuft genau einmal nach der Planwahl (nicht je Plan), Health wird seit MT2 einmal je Detektion gerechnet, nur der Gewinner wird signiert, Threshold-Proben sind keine vollen Health-Läufe, Route-Fit variiert `maxAccounts` und nicht die Eingabemenge, und „≥ 1,5 s bis Send“ gilt nicht für die heutige Kette.
2. **Das TXT hat die meisten Codefakten richtig, aber sechs Kernthesen falsch** (2.1, 1.3, 1.4-Signaturen, 1.6-Richtung, 2.4, 2.5, 8.2-1,5 s, 4.1-Drain-Vorschlag) und drei Zahlen ohne Beleg (1,5 s, 36 s, 50–200 ms).
3. **Das MD irrt an fünf Stellen** (§2): der fsync-p99 ist im Ledger gemessen, die C2-Health-Zahlen stammen aus einem Mainnet-Shadow-Run, P5b-04 ist kein HOCH-Safety-Fall, P5b-01 ist ein echter Latenzposten, und die Kopplung `T_detect → deadline_passed` fehlt in beiden Dokumenten.
4. **Echte Defekte am HEAD:** drei Liveness-Fehler (unvollständige finalized Receipts werden nie erneut gelesen; Root-/Fork-HALT ohne Recovery-Trigger; Threshold-Modus konsumiert keine Zeit-Wakeups), ein Messfehler mit Safety-Kopplung (`T_detect`), zwei belegte Latenzposten (Workload-Pacing, ProgramData-Rehash), zwei Konfig-/Hygienepunkte und ein Mikro-Sammelticket.
5. **Nicht bauen:** Simulation-Rangfolge (2.1), Health-Deduplizierung (1.3), Threshold-Warmstart gegen 36 s (2.4), parallele Jupiter-Quotes ×1,05 (2.5), Drain während Fetch A (4.1), Null-Wert-Guard im Settlement (7.5), Ein-Frame-Handoff als „mindestens so sicher“ (9.1), 3-Zeilen-Skip identischer Oracle-Bytes (4.2).

---

## 1. Urteilsmatrix (alle Punkte, HEAD `5368c82d`)

Legende: **TXT** = Urteil zur TXT-Behauptung, **MD** = Urteil zum MD-Verdikt. Defekt = tatsächlich am Code belegter Fehler/Posten. Severity: S = Safety, L = Liveness, LAT = Latenz, M = Messung/Vertrag, K = Konfig, µ = Mikro.

| ID (MD) | Thema | TXT | MD | Defekt | Sev. | Ticket §3 |
|---|---|---|---|---|---|---|
| P1-01 | Blockhash-Check nach Planung | teilweise (Reihenfolge ja, „strikt dominant“ nein) | richtig | ja, klein | LAT (Degradation) | V2-09 |
| P1-02 | Hash/Base58 doppelt | bestätigt (3× `message.hash()` je gequotetem Plan) | richtig | ja | µ | V2-14 |
| P1-03 | Health 3–4× | **widerlegt** (1× `health()` + 1× Maintenance nur für getaggte Accounts) | richtig | nein | — | — |
| P1-04 | Inventarpläne doppelt gebaut | teilweise (Build 2×, **Signatur 1×**) | richtig | ja | µ/LAT | V2-14 |
| P1-05 | String-Formatierung | bestätigt | richtig | ja | µ | V2-14 |
| P1-06 | `T_detect` vor Economics | teilweise (**Richtung falsch**) | richtig, aber Kopplung an `deadline_passed` übersehen | **ja** | **M + S-Kopplung** | **V2-04** |
| P1-07 | Freshness nach Health | bestätigt (Reihenfolge) | richtig | nein (Health bereits bezahlt) | — | — |
| P2-00 | Größenordnung | Zahlen richtig, Addition falsch | richtig, „Mainnet falsch“ überkorrigiert | — | — | §2 |
| P2-01 | Simulation je Plan | **widerlegt** | richtig | nein | — | nicht bauen |
| P2-02 | `Cow::into_owned` | bestätigt | richtig | ja | µ | V2-14 |
| P2-03 | `components_of`-Kopien | bestätigt | richtig | API-bedingt | µ | — |
| P2-04 | Threshold-Suche 36 s | **widerlegt** (Probe = `calc_value`, kein Health) | richtig | nein | — | nicht bauen |
| P2-05 | Route-Fit parallel ×1,05 | **widerlegt** (Variable ist `maxAccounts`) | richtig | nein | — | nicht bauen |
| P3-01 | WSS-Frame-Kopie | bestätigt | richtig | ja, klein | µ | V2-14 |
| P3-02 | `T_rx` nach Parse des Vorframes | bestätigt | richtig | ja | M | V2-14 (Messvertrag) |
| P3-03 | `eprintln!` im WSS-Thread | teilweise (nur Connect-/ACK-Phase, nicht je Frame) | richtig | ja, selten | L (selten) | V2-14 |
| P3-04 | Inline-Pong blockiert Leser ≤ 5 s | bestätigt | richtig | ja, selten | LAT | V2-14 |
| P3-05 / P5a-03 | Fork-Orphans, HALT ohne Recovery | **bestätigt** | richtig | **ja** | **L** | **V2-02** |
| P3-06 / P4-01 / P5a-01 | Snapshot-Fetch blockiert Owner | teilweise (**Fixvorschlag falsch**) | richtig | ja, bounded | L/LAT | V2-12 |
| P3-07 | `premium_decay` 2× im Census | bestätigt | richtig | ja, trivial | µ | V2-14 |
| P3-08 / P4-04 | Sweep-/Burst-Parameter nicht in `Limits` | bestätigt | richtig | ja | K | V2-08 |
| P4-02 | identische Oracle-Bytes | bestätigt (nur Zähler) | richtig (0,024 %) | nein, Design | — | nicht bauen |
| P4-03 / P5b-02 / P5c-01 | Threshold `all_accounts`-Vollscan, `candidates()` | bestätigt | richtig | ja (opt-in) | LAT | V2-13 |
| P4-05 | 180 s Stop-and-Join | bestätigt (Bound, nicht Normalfall) | richtig | nein | — | — |
| P5a-02 | `missing_remote`, kein Tombstone | bestätigt | richtig | ja, klein | M/LAT | V2-10 |
| P5a-04 | `entry`/`HashSet`/`binary_search` | bestätigt | richtig | trivial | µ | V2-14 |
| P5b-01 | ProgramData je Fingerprint voll gehasht | bestätigt | Fakten richtig, Gewicht zu niedrig | **ja** | **LAT** | **V2-06** |
| P5b-03 | Oracle-Enqueue Dedupe+Sort | bestätigt | richtig (Index unsortiert) | nein | µ | — |
| P5b-04 | Fingerprint ohne owner/lamports/wv | technisch bestätigt | **überzogen** (MT5a-Readset deckt Result-Pfad; Restfall health-irrelevant) | ja, vernachlässigbar | µ | V2-06 |
| P5b-05 | `admit` überschreibt `work` bedingungslos | bestätigt | richtig | ja, klein | M | V2-11 |
| P5c-03 | Threshold-Modus ohne Zeit-Wakeup-Consumer | **bestätigt** | richtig | **ja** (opt-in) | **L** | **V2-03** |
| P5c-05 | `set_account_banks` Rebuild | bestätigt | richtig | trivial | µ | V2-14 |
| P6-01 / P8-04 | getrennte HTTP-Pools, 90 s Idle, 10 s Blocking-Thread | bestätigt (Struktur) | richtig | ja (ungemessen) | LAT | V2-07 |
| P6-02 | `skip_preflight`, `ObserveOnly` | bestätigt | richtig | nein | — | — |
| P6-03 | Rejected-Allowlist | bestätigt | richtig | nein | — | — |
| P6-04 | Jito `error` → Rejected; Hold bleibt | bestätigt | richtig | nur Deadline-Nit (Pfad inaktiv) | L-Nit | V2-14 |
| P7-01 / P8-01 | unvollständige finalized Receipts nie erneut gelesen | **bestätigt** | richtig | **ja** | **L** (fail-closed) | **V2-01** |
| P7-02 | Nonlanding-Proof vertraut Provider-Archiv | bestätigt (Reihenfolge korrekt) | richtig | Policy, kein Codefehler | S-Policy | V2-15 |
| P7-03 | zwei Simulations-Callsites | bestätigt | richtig | heute nein (`fe04: None`) | LAT bei LIVE | V2-15 |
| P7-04 / P9-01a | zwei Sync-Grenzen vor Send | bestätigt | richtig; fsync-Wert existiert (§2) | nein (bewusst) | — | nicht bauen |
| P7-05 / P9-02 | Null-Wert-Guard fehlt | teilweise (nur Gewinnseite kann 0 werden) | richtig | nein | — | nicht bauen |
| P8-02 | Pacing ≥ 500 ms/Hop, ≥ 1,5 s bis Send | **1,5 s widerlegt**; 500 ms/Hop bestätigt | richtig | **ja** | **LAT** | **V2-05** |
| P8-03 | 50-ms-Wartequantisierung | bestätigt (mit `notify_all`) | richtig | nein | µ | — |
| P8-06 | `eprintln!`, `shared()` | teilweise (nur Fehlerpfade/Konstruktion) | richtig | nein | — | — |
| P9-01b | gleiche `Current` in Reserve und Handoff | bestätigt | richtig | heute nein (nicht verdrahtet) | S bei LIVE | V2-15 |
| P9-04 | Budget im Shadow aktiv? | bestätigt (p0ctl übergibt Args; `run_econ4h` aktiviert unbedingt) | richtig | nein | M-Konsistenz | — |

---

## 2. Wo das MD irrt oder ungenau ist

| Stelle | MD-Aussage | Befund |
|---|---|---|
| P9-01a | „~5 ms p99 je Sync ist unbestätigte Eingabeschätzung“ | **Falsch.** `MEASUREMENT_LEDGER.md:5318`: FE-04 append+fsync **p50 4,23 ms / p99 5,31 ms** (synthetische Writer-Probe, M4 Max/APFS, Release). Der TXT-Wert ist belegt; nur die Übertragung auf das Zielmedium im Echtlauf fehlt. |
| P2-00 | „Benennung ‚Detect-Health Mainnet‘ aus dem Paket ist falsch (Offline, no Mainnet)“ | **Überkorrigiert.** `d7_c2_measured.md` ist selbst offline, zitiert aber den Ledger-Run `r127_20261006T130642Z` (Ledger 4355–4392): Mainnet-Shadow vom 06.10., serieller Owner, **vor MT2**. Richtig bleibt: alte Zahl, nicht heutiger Pfad. Dieselbe Baseline zeigt **Pipeline Ø 1,57 ms / max 2,654 s je Kandidat gegen Health Ø 3,39 µs (Faktor ≈ 460)** — das ist der eigentliche Beleg, dass Owner-Remote-Arbeit und nicht Health der Hebel ist. |
| P5b-04 | „neue bestätigte Vertragslücke, HOCH / Safety-RED/GREEN vor Cache-Optimierung“ | **Überzogen.** MT5a bindet jeden akzeptierten Write über `last_changed_epoch` (`state/mod.rs:406/451`) und das Readset (`identity.rs` `changed_key → RecheckCurrent`, `remote_apply.rs:341-343`) am **Result**-Pfad. Nur der Cache-Consume-Pfad (`detect.rs:100-113`) nutzt allein den Fingerprint; der entkommende Fall „gleicher Slot, gleiche Daten, anderer owner/lamports“ ist nach Chain-Regeln (Owner-Wechsel nur bei genullten Daten) und Health-Formel (keine Lamports) sehr wahrscheinlich irrelevant, aber **nicht getestet** — `ProjectedState` reicht owner/lamports an Health/Sizing. Einstufung: Mikro mit offenem Test, fällt mit V2-06 zusammen. |
| P5b-01 | ProgramData-Rehash hinter E9-Safety einsortiert | **Zu niedrig gewichtet.** `candidates.rs:33-49` hasht `r.data` jeder Dependency vollständig, `projection_dependency_set` (`:75-96`) enthält `program_data` (Programm-Binary, Größenordnung MB) fest; Aufrufe `detect.rs:108`, `:217`, `remote_apply.rs:341` ⇒ 3 SHA-256 über das Binary je Projektionszyklus auf dem Owner. Größter belegter **lokaler** CPU-Posten; Fix ist äquivalent (V2-06). Absolutwert ungemessen. |
| P1-06 | Richtung korrekt, „Messfehler“ | **Unvollständig.** `pipeline.rs:1040-1043`: `deadline_passed(t_detect, now)` nimmt **`T_detect`** als Anker (`liquidation/src/execution.rs:16-27`). Die Marke zu verschieben verlängert das Execution-Deadline-Fenster. Deshalb kein reiner Messfix (V2-04). |
| P3-03 | „mögliche 5-s/ungegrenzte Stalls“ | Nur in Connect-/Subscribe-/ACK-Phase (`ws_rpc.rs:487,584,657,820,831`), keine Ausgabe je Frame. Realer, aber seltener Stall. |
| P3-06 | „nach begrenzter Retryzahl terminal“ | Richtig; Ergänzung: `RecoveryDriver::new(3)` (`setup.rs:351`) + genau ein C1-Zusatzzyklus (`recovery.rs:226-233`). |

---

## 3. Bestätigte Defekte und Fixplan (Format BUGS.md §0.1; Reihenfolge = Priorität)

### V2-01 — Unvollständiges `Ok(FinalizedObservation)` wird im automatischen Settlement/Exit nie erneut gelesen (P7-01/P8-01)
- **Stand 25.10.: erledigt** `e44243e1` (unvollständige Observations erneut gelesen, vollständige nie abgeschwächt; Gate PASS `d82df4a8`).
- **Priorität/Phase:** **P1 LIVENESS · vor FE-06 · G-E/G-F · Cross-Ref C3, C12.** Fail-closed (Hold bleibt), aber ohne Auflösungspfad; auch nach Neustart.
- **Pfad (HEAD):** `one_shot/receipts.rs:92-116` (`parse_finalized`: Token-Metadaten fehlen/inkonsistent ⇒ `Ok{wallet_token_deltas: None, unknown_reasons:[..]}`), `one_shot/settlement.rs:371-404` (`apply_settlement_evidence` persistiert jedes `Ok`), `:331-336` (`settlement_job` liefert **alle** `observations.keys()` als „observed“), `:229-253` (`fetch_settlement_evidence` filtert `!observed.contains`), `:274-291` (`reconcile_observed` lehnt `unknown_reasons` ab); Exit analog `one_shot/exit.rs:386-392`, `:649-663`; produktive Verdrahtung `state_owner/fe04.rs:54` (10 s), `:142-162` → `remote_workers.rs:275-280`. `receipts.rs::observe_finalized` (`:217-258`) liest alles neu, hat aber **keinen** produktiven Aufrufer.
- **Reproduktion (RED):** `handed_off(1)`, Observation mit `unknown_reasons=["ARCHIVE_429"]` persistieren ⇒ `settlement_job().1` enthält die Signatur (soll: leer). End-to-End: Evidence #1 unvollständig ⇒ Unknown; Evidence #2 vollständig für dieselbe Signatur wird **nie angefordert** (soll: Settled). Exit-Variante identisch.
- **Fix:** `settlement_job`/`exit_job` liefern nur **vollständige** Observations (`unknown_reasons.is_empty() && wallet_token_deltas.is_some()`) als beobachtet; `apply_*` ersetzt vollständige Evidence nie durch schwächere, persistiert nur bei Änderung (Journal bleibt bounded); `Err` lässt Zustand unangetastet; Nonlanding-Proof unverändert (nutzt Journal-Observations).
- **Tests:** RED/GREEN oben (3 Fälle), zusätzlich Crash/Restart zwischen Observe und Reconcile, Bundle mit zwei Signaturen (eine unvollständig), identisches erneutes `Ok` ohne Journalwachstum. Bestehend grün halten: `incomplete_evidence_never_settles_and_keeps_the_hold` (`settlement.rs:604`), `c3_settle_step_books_landed_and_not_landed_and_keeps_unknown` (`:871`), `receipts.rs:338/448`, `exit.rs:1309`.
- **Restgefahr:** dauerhaft defekte Provider-Metadaten bleiben UNKNOWN (korrekt); zweite Evidence-Quelle ist Policy (V2-15), nicht Teil dieses Fixes.

### V2-02 — Root-/Fork-induzierter `StateGap`-HALT ohne Recovery-Trigger (P3-05/P5a-03)
- **Stand 25.10.: erledigt** `e5d53fb7` (Root-/Fork-StateGap startet Recovery auf der lebenden Session).
- **Priorität/Phase:** **P1 LIVENESS (State-Verfügbarkeit) · G-C/G-D · Cross-Ref C1.**
- **Pfad:** `state/fork.rs:59-74` (Walk über bekannte Parent-Kanten; fehlende Kante beendet den Walk ⇒ konservative Orphans, auch falsch-positiv bei einer verpassten `slotNotification`), `state/mod.rs:241-254` (`invalidate_slots`: `retain(!orphaned.contains)` O(N·M); `halt(StateGap)` nur wenn Accounts entfernt), `state/recovery.rs:212-220` (`trigger = matches!(ev, Reconnected)`; sonst `return None`). Vollständiger Grep (`StateGap|Halt|gap_open|Reconnected|resubscribe|recover|is_raised|ingress_session.request`): `ws_rpc.rs` **raised** nur (457/540/928), `lifecycle.rs:81-83` re-haltet FeedGap, Session-Ersatz nur bei Dependency-Wechsel (`events.rs:768-830`) und Source-Control (`sources.rs:220-271`). Ergebnis: Stream lebt, kein Timeout feuert, `is_tradable()` bleibt false (`events.rs:93`) bis zu einem fremdverursachten Reconnect oder Fensterende.
- **Reproduktion (RED, `recovery.rs`):** Ready; Account bei Slot 12 (Parent 11); `Slot(13, parent 11)`, dann `Root(13)` ⇒ heute `on_event` → `None`, Zustand bleibt `Halted(StateGap)`; erwartet: Recovery → `Ready`.
- **Fix (minimal):** in `on_event_cancellable` zusätzlich triggern, wenn `was_tradable && matches!(run_state, Halted(StateGap))` nach einem Root-Event (A/B-Snapshot + Reconcile auf lebender Session; `gap_open` ist false ⇒ Ready erreichbar). C1-Verbotsliste beachten: kein Löschen des Halt-Flags, keine rekursive Recovery aus `drain`, Attempt-Budget bleibt (3 + 1). Diagnostik ergänzen: `walk_complete`, `orphan_candidates`, `removed_records`, Zeit Halt→Ready.
- **Tests:** RED oben; echter Fork mit Sibling-Slot (Orphan korrekt); Orphan ohne betroffene Accounts (kein Halt); Attempts-Zählung; Reconnect während Fork-Recovery; `state_epoch_is_monotone_across_writes_snapshot_reconcile_and_fork` (`mod.rs:621`), `detects_orphaned_branch`, `gap_then_reconnected_can_recover`, `gap_to_ready_full_loop` grün.
- **Restgefahr:** wiederholte Lücken verbrauchen Attempts ⇒ `STATE_RECOVERY_EXHAUSTED` terminal (bewusst). Kein `getBlocks`-Ersatz ohne Provider-/Finality-Qualifikation.

### V2-03 — Threshold-Modus (opt-in) konsumiert keine Zeit-Wakeups (P5c-03)
- **Stand 25.10.: erledigt** `d82df4a8` (Threshold-Modus konsumiert Zeit-Wakeups; Premium-Wakeup aus der exakten Auswertung).
- **Priorität/Phase:** **P1 LIVENESS/Coverage des opt-in-Modus · vor jeder Aktivierung von `P0_USE_THRESHOLD_TRIGGER` · P6-MT.13/P2-04/P2-05.** Default-Sweep-Modus nicht betroffen.
- **Pfad:** `sweep.rs:7-13` (`sweep_batch_open() = !use_threshold_trigger && …`), `:31` (gesamter Sweep-Body dahinter), `:83` (**einziger** `time_schedule.pop_due`-Consumer), `:212` (PremiumBoundary nur dort geplant), `:331/500` (`candidate_should_forward` nur dort); Produzent `detect.rs:607-611` (`TagProfitable` in beiden Modi); `events.rs:21-23` Idle-Wartezeit nur bei `sweep_batch_open()`. Im Threshold-Modus sammeln sich Einträge an, die nie gepoppt werden; `candidates()` (alle jenseits Schwelle) maskiert das für bereits gefährdete Accounts, nicht für reine Zeitübergänge. **Kein Owner-Test läuft mit `use_threshold_trigger: true`** (`owner_e2e_tests.rs:137`).
- **Reproduktion (RED):** Klone von `p2_05_tag_wakeup_forwards_a_held_candidate_again` (`owner_e2e_tests.rs:2764`) und `p2_04_premium_boundary_wakes_the_sweep_without_an_event` (`:2718`) mit `use_threshold_trigger: true, threshold_ready: true` ⇒ feuern heute nie.
- **Fix:** modusunabhängiges `time_wakeups()` aus `sweep()` herauslösen; im Threshold-Modus fällige Accounts in die Lanes (TagProfitable → Force-Rearm/L1, PremiumBoundary → L2) über `enqueue_oracle_accounts`; PremiumBoundary aus der exakten Threshold-Auswertung planen; `time_schedule.earliest()` in den `next_event`-Timeout einbeziehen. **Kein** Austausch `candidates()` → `crossed_candidates()` ohne Per-Bank-Vorpreis-Map und ohne dieses Ticket (Schwellen wandern, Erstbeobachtung/Recovery-Rebuild).
- **Restgefahr:** Modus bleibt opt-in und ohne Mainnet-Qualifikation; Messung der L0-Trigger-Quote erst nach Fix.

### V2-04 — `T_detect` vor Fee-Quote/Economics markiert und zugleich Anker des Execution-Deadline-Gates (P1-06)
- **Priorität/Phase:** **P1 Messintegrität vor MT0-/P6-MT.15-Abnahme; Safety-Kopplung · S0-02.**
- **Pfad:** `pipeline.rs:761-764` (`tl.mark(Stage::Detect)` nach Allowlist/Capability, vor `tx::build` 772, Fee-Quote 795–861, `plan_economic_gate` 871), `Stage::Ready` `:1020`; **`:1040-1043` `deadline_passed(t_detect, now)`** mit `liquidation/src/execution.rs:16-27`; Konsumenten `record.rs:273`, `shadow_ledger.rs:131-132`, `econ.rs:2176-2177`; Stage-Namen gepinnt `common/src/timing.rs:193-199`. PHASENPLAN §6 (`:858`): `T_detect` = gültig **und** wirtschaftlich. Richtung: frühe Marke ⇒ `state→detect` zu **kurz**, `detect→ready` zu **lang** (TXT hatte es umgekehrt).
- **Fix (zwei Schritte, nicht einer):** (a) Deadline-Anker von `T_detect` entkoppeln — eigener Zeitstempel `t_eligible` (= heutige Markierungsstelle) oder `t_rx` als Anker; Entscheidung dokumentieren, weil sie die Strenge von `P0_DEADLINE_NS` bestimmt. **Empfehlung (Nachtrag 24.10.):** `t_rx` als Anker — extern definiert, stabil, wandert nicht mit der Marke; das ist die strengere Variante (verschärft gegenüber heute `T_detect`), Vertragstext `execution.rs:16-27` („ab T_detect“) und PHASENPLAN anpassen. Hinweis: das L0-Gate in P6-MT.15 ist `T_rx_first → T_admit ≤ 50 ms`, nicht `T_rx → T_sign`. (b) `Stage::Detect` erst beim ersten Plan markieren, der `actionable || technical_probe` passiert (nach Economics); nicht-wirtschaftliche Kandidaten erhalten kein `T_detect` (Intervalle `None`). (c) optional diagnostische Stage `T_eligible` im typisierten Timeline-Vertrag (Schema-Bump, alte Frames lesbar).
- **Tests (RED):** Fee-Adapter mit Sleep X ⇒ `T_detect→T_ready < X` (heute ≥ X); `deadline_passed`-Parität (gleiche Entscheidung vor/nach Entkopplung); Known-nonactionable ⇒ `t_detect_ns == None`; Unknown/Safety-denied/fehlender Blockhash; Ledger-Intervallrichtung; `shadow_ledger`/`econ`-Reports tragen `None`.
- **Restgefahr:** P6-MT.15-Shadow-Werte bis dahin nicht vertragsgemäß gemessen; Python-/Reporter-Konsumenten prüfen.

### V2-05 — Fee-Quote, Simulation und Send teilen den `Workload::Simulation`-Slot ⇒ ≥ 500 ms Abstand je Hop (P8-02)
- **Priorität/Phase:** **P1 LATENZ · P6-MT.7/MT1 · Cross-Ref P6MT-R1.** Gilt nur bei aktiviertem Budget — das sind alle `p0ctl`-Starts (Args ⇒ `startup.rs:51-79`) und jeder `run_econ4h` (`:192-195` unbedingt).
- **Pfad:** `provider.rs:402-411` (`next_workload[idx] = now + 1/rate`), `:343-345` (`ready = max(token, next_workload, cooldown)`); Defaults `runtime_config.rs:27-28,43` (`rpc_rps=5`, `rpc_burst=1`, `simulation_rps=2`); `submit/rpc.rs:147` (Fee-Quote **und** Simulate auf `DeadlineRpc(Simulation)`), `:171-176` (Send-Client ebenfalls `Simulation`). `standard.conf` setzt nichts.
- **Korrekte Zahlen:** heutiger Shadow = Fee-Quote je eindeutigem Message-Hash + **eine** Simulation, kein Send ⇒ Simulation ≥ 500 ms nach der letzten Fee-Quote (≥ 1,0 s bei zwei verschiedenen Messages). Hypothetische LIVE-Kette Fee→Sim→Send ≥ 1,0 s. **„≥ 1,5 s bis Send“ widerlegt.**
- **Fix:** eigener `Workload::FeeQuote` mit Parameter `fee_rps` in `parameters!` (kein Literal); Send-Client (`submit/rpc.rs:171-176`, heute ebenfalls `Simulation`) für die LIVE-Kette in einen eigenen Send-Workload (ein Send je Versuch; 500 ms hinter der Simulation haben keinen Quota-Nutzen; globales Token-Budget bleibt; heute ohne Wirkung, kein Send); globales `rpc_burst=1`/`rpc_rps=5` **unverändert** (FREE_ONLY), keine Erhöhung ohne Provider-Evidence; C5-Reconcile-Vorrang und 429/Cooldown-Semantik erhalten.
- **Tests:** Muster `c5_paced_reconcile_never_blocks_live_reads` (`provider.rs:~1290`): zwei `acquire(Simulation)` bei rps=2 ⇒ ≥ 500 ms (RED-Beleg); nach Fix Fee→Sim ≥ 200 ms und < 500 ms mit Defaults — der globale Bucket zieht je Admission ein Token ab (`provider.rs:336/401`, `rpc_burst=1`, Refill 1/`rpc_rps` = 200 ms), < 100 ms nur in einer Testkonfiguration mit `rpc_burst ≥ 2`; Einsparung = 500 ms Workload-Abstand minus 200 ms globaler Mindestabstand; keine Burst-Erhöhung im Free-Profil; 429/Cooldown-Parität; Deadline 5 s; kein Doppel-Send.
- **Restgefahr:** Provider-Sekundenlimit; nur innerhalb `rpc_rps` bleiben.

### V2-06 — Dependency-Fingerprint hasht ProgramData vollständig, 3× je Projektionszyklus; owner/lamports/write_version ungebunden (P5b-01 + P5b-04)
- **Priorität/Phase:** **P2 LATENZ (ungemessen, strukturell größter lokaler Posten) · P6-MT.2/E9.**
- **Pfad:** `candidates.rs:33-49` (`hash(&r.data)` je Dependency), `:75-96` (`program_data` fest im Set; N3-Test `:1264-1273` pinnt das), Aufrufe `detect.rs:108` (Cache-Consume), `:217` (Work-Erstellung), `remote_apply.rs:341` (Result); `state/mod.rs:52` `last_changed_epoch`, `:406/451` Bump bei jedem akzeptierten Write (auch Same-Slot owner/lamports), Re-Epoch bei Install, Cache-Clear bei Ready (`events.rs:293`).
- **Fix:** `hash(&r.data)` → `last_changed_epoch.to_le_bytes()` im Fingerprint-Part (Präsenz + Slot bleiben). Vertrag ausdrücklich auf **dieselbe `StateCore`-Instanz** begrenzen: Epochen sind instanzlokal, nach Snapshot-Install/Recovery/Ready sind alle alten Fingerprints ungültig (Cache-Clear bei Ready bleibt Pflicht); nie persistieren, nie über Instanzen vergleichen. Schließt P5b-04 mit, **sofern** der gezielte owner/lamports-Test (unten) besteht.
- **Tests:** `e9_dependency_fingerprint_tracks_every_dependency_revision` (`candidates.rs:1102-1170`) **muss umgebaut werden**: die `install`-Closure baut je Datenstand einen frischen `StateCore::new(1)`; gleiche Einfügereihenfolge ⇒ gleiche Epochen ⇒ ein rein epochenbasierter Fingerprint fiele im Gleich-Slot-Datenfall durch. Neuer Test mit In-place-Writes auf **einem** `StateCore`: gleicher Slot + Datenänderung ⇒ ≠; gleicher Slot, gleiche Daten, lamports 1→2 ⇒ ≠ (heute =); Snapshot-Install/Ready ⇒ alte Fingerprints ungültig; zusätzlich gezielter Test, ob `ProjectedState` (reicht owner/lamports an Health/Sizing) bei owner-/lamports-only-Revision dasselbe Ergebnis liefert (Owner-Wechsel ist on-chain nur bei genullten Daten möglich, Lamports gehen nicht in die Health-Rechnung ein — bis zum Test offen); N3 `program_data ∈ set`; `projection_stats` Stale/Ready-Parität; Vorher/Nachher-Messung SHA-Zeit über ProgramData-Größe (MT0).
- **Restgefahr:** Rewrite identischer Bytes bumpt Epoch ⇒ konservative Verwerfung (mehr Rework, kein Fehlentscheid).

### V2-07 — Getrennte HTTP-Clients/Pools ohne Keepalive, 90 s Idle; Blocking-Thread 10 s (P6-01/P8-04)
- **Priorität/Phase:** **P2 LATENZ (ungemessen) · MT1 Remote-Client-Hygiene · Cross-Ref P6MT-R1.**
- **Pfad:** `route.rs:169-171` (Jupiter blocking Client), `jito.rs:54-56`, `provider.rs:688-695` (`rpc_client` eigener Client), `:999-1010` (`DeadlineRpc` eigener Client + eigene current-thread Runtime), `:1424`; `submit/rpc.rs:145-148,165-181` (zwei Pools je Submitter). Kein `pool_idle_timeout`/`tcp_keepalive` im Workspace; `reqwest 0.12.28` Default 90 s; `send_until` `spawn_blocking` (`provider.rs:785-800`), Tokio-Default `thread_keep_alive` 10 s.
- **Fix:** ein gemeinsamer `reqwest::Client` je Hostklasse (`Arc`) mit `tcp_keepalive`/`pool_idle_timeout` aus `Limits` (keine Literale); `thread_keep_alive` konfigurierbar oder Admission inline, wenn `ready <= now`. **Kein** Ping-/Prewarm-Request ohne FREE_ONLY-Budgetentscheid.
- **Tests:** Loopback-Server zählt TCP-Accepts über zwei aufeinanderfolgende Kandidaten (heute ≥ 2 Clients); Idle-A/B 0/30/90/120 s; Deadline-/Budget-Parität.
- **Restgefahr:** LB/Server-Idle-Policies unbekannt; 50–200 ms nicht als erwiesen führen.

### V2-08 — Sweep-/Burst-Parameter außerhalb des typisierten `Limits`-Vertrags (P3-08/P4-04)
- **Priorität/Phase:** **P2 KONFIG/Manifest · P6-MT.13 · Regel „keine Literale, Tunables nur über Config“.**
- **Pfad:** `setup.rs:716` (`P0_SWEEP_BATCH_MAX_EVENTS`=64), `:722` (`P0_SWEEP_BATCH_MAX_AGE_NS`=5 000 000), `:843-844` (`P0_INGRESS_BURST_BEFORE_DETECT`=64) via `parsed_or`; `runtime_config.rs:26-70 parameters!` ohne diese Felder; `standard.conf` kann sie nicht ausdrücken. Flush `sweep.rs:36-39` (`events ≥ max || age ≥ max_age || wake_due`) — keine feste 50-ms-Wartezeit (TXT-Ungleichung falsch).
- **Fix:** drei Felder in `parameters!` (Einheit, Default, Min/Max), `setup.rs` liest `limits.*`, Manifest-Readback/CLI. **Test:** Profiltest `sweep_batch_max_events = 32` (heute ignoriert).

### V2-09 — Blockhash-Freshness erst nach Planung (P1-01)
- **Priorität/Phase:** **P2 LATENZ nur im Degradationsfall (Blockhash-Cache > 30 s alt) · P6-MT.4.**
- **Pfad:** `pipeline.rs:544` Freshness → `:550/558` Budget → `:565-567` `plans_with_health` → `:587` RpcDeadline → `:598-611` leere/Err-Pläne → `:613-620` `blockhash.fresh` (RwLock-Read, kein I/O, `blockhash.rs:53-60`).
- **Fix nur mit Vertragsentscheid:** früher Check vor dem Budget-Decrement ändert Loss-Priorität (heute gewinnen RpcBudget/RpcDeadline/Routing/PlanError), lässt `candidates_created = None` und verbraucht kein Budget. Alternative: diagnostischer Preflight-Zähler + bestehender Spätcheck. **Test:** `TestBuilder` zählt `plans_with_health`; leerer Cache ⇒ 0 Aufrufe, `remaining` unverändert, `loss == Build` (heute: 1 Aufruf, Budget verbraucht). Kein Blockhash-Stale-Pipelinetest vorhanden.

### V2-10 — Close-/Out-of-Filter-Updates entfernen ReverseIndex/Schedule nicht (P5a-02)
- **Priorität/Phase:** P3 Messung/Latenz · S2/P6-MT.2.
- **Pfad:** `events.rs:116-122` (`updated_marginfi_account` nur bei eingehender MarginfiAccount-Klassifikation), `:651-658` (Remove-Zweig läuft bei Close nie), `:283-291` Rebuild erst bei Ready; `state/mod.rs:514-524` `missing_remote` ⇒ ReconcileFailure ⇒ genau ein Retry je Close im A→B-Fenster (bounded).
- **Fix:** `updated_marginfi_account` auch setzen, wenn der Account bereits im ReverseIndex ist; bestehender Post-Apply-Pfad liefert `None` ⇒ Remove. **Test:** indexierter Account, Event mit leerer Data/System-Owner ⇒ `banks_for_account` leer (heute nicht leer). Kein Prune aus bloßem Fehlen in B (GPA liefert keine `null`-Zeilen).
- **Grenze des Fixes (Nachtrag):** greift nur, wenn ein Update zugestellt wird. Liability-Accounts kommen über gefiltertes `programSubscribe` (`accounts.rs:341-353`: DataSize + Group-Memcmp + `is_lending_only == 0`; `setup.rs:287-292`; `ws_rpc.rs:172-178`); ein Account, der den Filter verlässt (Close ⇒ DataSize 0, Owner-Wechsel), erzeugt keine passende Notification. Dependencies laufen über `accountSubscribe` (`ws_rpc.rs:186`), dort kommt ein Close an. Vollständige Bereinigung nur über den Recovery-Rebuild bei Ready (`events.rs:284`) oder einen gezielten Account-Read. Abnahme: Filter-Austritt, Close, Recovery-Rebuild je als Test; Restlücke bis dahin dokumentiert.

### V2-11 — `CandidateLanes::admit` überschreibt `work` bedingungslos (P5b-05)
- **Priorität/Phase:** P3 Messung/Freshness-Provenienz · MT3/FIX-21.
- **Pfad:** `candidates.rs:697`, `:719-727` (kein Vergleich mit `existing.work`); älterer Trigger über Bank-Projection-Fallback möglich (`events.rs:948-950`). Auswertung bleibt korrekt (Detect liest kanonischen State), aber `max_opportunity_age_ns` kann fälschlich „stale“ werten.
- **Fix:** `existing.work` behalten, wenn `(a.slot, a.t_rx_ns) < (existing.work.2, existing.work.1)`. **Test:** admit (250,12) dann (100,10) ⇒ (250,12) (heute (100,10)); `mt3_23_coalescing_keeps_t_rx_first_and_takes_the_newest_work` grün.

### V2-12 — Recovery-Snapshot-Fetch blockiert den Owner-Drain (P3-06/P4-01/P5a-01)
- **Priorität/Phase:** **P2 (bounded, kein belegter Overflow) · MT6 Snapshot-Modell · Cross-Ref D3, C1.**
- **Pfad:** `state_owner.rs:312` (`owner.events(&mut fetch, …)` auf dem Owner), `events.rs:182-195`, `recovery.rs:282→309→310→315→337` (A, Install, Drain, B, Drain; kein Drain **im** Fetch), `setup.rs:361-395` (Liability-GPA + Bank-GPA + 100er-Chunks), Queue 65 536 (`setup.rs:115/129`, `runtime_config.rs:56`), Overflow `ws_rpc.rs:926-936` (HALT + give_up + Gap-Versuch), `state/mod.rs:444` `accounts.clear()`, `:471-482` Replay nur `self.buffer` (füllt sich nur im Bootstrap, `:366-376`).
- **TXT-Vorschlag „Drain während Fetch A vor Install“ ist falsch:** Deltas würden per `apply()` in den alten State gehen und bei `accounts.clear()` verschwinden ⇒ B-Reconcile-Mismatch ⇒ Retry. Historische Messung: A 6 253 ms / B 6 273 ms bei 83 274 Accounts (Ledger 1647/1650) ⇒ Full-Schwelle bei leerer Queue ≈ 10 481 Events/s; Events/s **desselben** Laufs nicht vorhanden.
- **Fix nur als Staging-Modell:** Fetch auf Helper-Thread, Owner drainet unter `recovering`-Flag in einen Recovery-Staging-Puffer, danach Install + Replay mit **einer** Regel (Slot + write_version wie `apply()`); Control-Events (Root/Gap/Reconnected/Fatal) getrennt; Kapazität O(verschiedene Konten) mit HALT bei Überlauf. Vorher Messauftrag: A/B-Dauer, Events/s, Queue-Peak, `sink_overflow_lines`, Gap/Reconnected aus **demselben** qualifizierten Lauf.
- **Tests:** RED/GREEN-Matrix (MD §18 a–h): Event vor A-Context, neuer als A, gleicher Slot ± write_version, A/B verschiedene Kontexte, 100k Events mit 1k/65k Keys, Gap/Fatal/Root dazwischen, Cancellation/Reconnect im letzten Versuch, keine frühe READY-Promotion; `owner_end_to_end_recovers_only_with_a_valid_snapshot` (`owner_e2e_tests.rs:305`).

### V2-13 — Threshold-Pfad: `all_accounts`-Vollscan je Bank und wiederholte `candidates()` (P4-03/P5b-02/P5c-01)
- **Priorität/Phase:** P2 LATENZ (opt-in) · nach V2-03.
- **Pfad:** `events.rs:889-893` (`all_accounts: Vec<Pubkey>` je Bank je Dependency-Update), `:907-912` (`candidates()`), `:914-917` (Schnittmenge über die große Seite), Fehlerzweige `:939-1083` (`fallback.extend(all_accounts)`); `remote_apply.rs:209-230`; `index.rs:30` (`HashMap<Pubkey, HashSet<Pubkey>>`), `:116-121` `banks_for_account`, `:197-220` `candidates`, `:228-258` `crossed_candidates` **ohne Aufrufer**; Richtung Asset-Low fällt / Liability-High steigt (`index.rs:8-10,145-149,221-227`; TXT-Vorzeichen „monoton steigend im Liability-Preis“ falsch).
- **Wiederholungsdrossel fehlt (Nachtrag 24.10.):** `candidate_cache`/`periodic_due`/`candidate_should_forward` existieren nur im Sweep-Pfad (`sweep.rs:232-352`); der Threshold-Enqueue (`events.rs:907-912`, `remote_apply.rs:209-230`) admittiert bei jedem Oracle-Tick alle bereits gefährdeten Accounts erneut nach L0 und verbraucht Kandidaten-Netzbudget (`P0_RPC_CANDIDATES_PER_DRAIN`). Fix: Recheck-/Soft-Cap-Policy modusunabhängig herauslösen (zusammen mit V2-03). Test: zwei Oracle-Ticks ohne Revisionsänderung ⇒ genau ein Forward.
- **Fix:** `ReverseIndex::bank_has_account` O(1); Fallback-Schnittmenge über `threshold_fallback` iterieren; `all_accounts` nur in Fehlerzweigen lazy materialisieren; `fallback_all` bei Stale/Fehler unverändert. **Tests:** Accessor-Unit-Test; Owner-Test Threshold-Modus `enqueued = candidates ∪ fallback` bei 0/228/alle Fallbacks, Success/Stale/Fehler.

### V2-14 — Mikro-Sammelticket (nur nach Profil oder im ohnehin berührten Modul; keine Latenzbehauptung)
| Punkt | Pfad | Einzeiliger Fix | Absicherung |
|---|---|---|---|
| P1-02 | `pipeline.rs:807/856`, `submit/rpc.rs:337`, `submit/mod.rs:336-340` | `MessageFeeQuote.message_hash: Hash`; Vergleich mit dem bereits berechneten Hash | `d2_identical_funding_plans_share_one_fee_quote` |
| P1-04 | `funding.rs:87/97` (identisches Prädikat ⇒ beide Klassen immer zusammen), `builder.rs:1483-1491`, `pipeline.rs:772` | Instruktionen einmal bauen und teilen; `BuiltTx` bei gleichen Instruktionen/Version wiederverwenden | Builder-Test zählt `receivership`-Aufrufe (heute 2) |
| P1-05 | `pipeline.rs:674…937`, `record.rs:278-282` | typisierte Rejection, Text im Writer | Schema-Bump |
| P2-02 | `sizing.rs:519-527/252-254`, `builder.rs:936-944` | `Cow<'a, Bank>` durchreichen | Sizing-Suite |
| P3-01 | `ws_rpc.rs:789-803` | `Text(t) => t`, `Binary(b) => String::from_utf8(b)` | `transport_ping_pong_never_counts_as_stream_activity` |
| P3-03 | `ws_rpc.rs:487,584,657,820,831`; `setup.rs:363,372` | injizierter Sink (`WsRpcConfig`) oder `Console` nach `darkbot-common` (kein `solana`→`runtime`-Zyklus) | Sink-Injektionstest |
| P3-04 | `ws_rpc.rs:793-799` | manuellen Pong entfernen, **nur** nach Loopback-Nachweis, dass tokio-tungstenite 0.24 den gepufferten Pong beim Poll flusht | Loopback Ping + blockierter Writer |
| P3-07 | `sweep.rs:152-163/181-195` | `premium_decay_per_second` einmal, typisiertes `Result` an beide Verbraucher | `p2_04_premium_boundary_wakes_the_sweep_without_an_event` |
| P5a-04 | `state/mod.rs:386-416/494/242` | `entry`, `HashSet`, `binary_search` | bestehende State-Tests |
| P5c-05 | `index.rs:77-95` | Early-Return bei unveränderter Bankmenge | `reverse_index_fanout_and_replacement` |
| P6-04 Nit | `jito.rs:297-310` | `deadline` durch `submit` reichen wie bei `simulate_bundle` (`:246-254`) | Pfad produktiv inaktiv |
| P3-02 | `ws_rpc.rs:749-750` | Messvertrag `T_rx` = Userland-Dequeue dokumentieren; Reader-Task nur mit Vertrag für `T_rx_first` | Burst-Loopback |

### V2-15 — Vor FE-06/LIVE zu entscheidende Verträge (keine heutigen Codefehler)
- **P9-01b:** `admission.rs:144-156` baut **eine** `Current` und reicht sie an `reserve_payload` (`:159`) und `submit_payload` (`:170`); `validate()` prüft nur übergebene Felder. Vor FE-06: `Current.slot/now_unix_ms` unmittelbar vor `submit_payload` aus autoritativen Quellen erneuern; RED-Test: Alterung während der Simulation ⇒ Handoff-Refusal (heute Submitted). Heute kein produktiver Aufrufer (`detect.rs:541 fe04: None`).
- **P7-03:** zweite Simulation in `admission.rs:113` nur relevant, wenn FE-04 verdrahtet wird; dann Pipeline-`SimResult` an `wire_sha256` gebunden wiederverwenden — Entscheid, kein heutiger Doppelaufruf.
- **P7-02:** Nonlanding-Proof (`settlement.rs:158-197`, Reihenfolge korrekt) vertraut Provider-History; Policy „zweiter Endpunkt / `getTransaction`“ vor LIVE entscheiden. Cross-Ref C12.

---

## 4. Widerlegte Thesen — nicht bauen

| These | Grund (HEAD) |
|---|---|
| 2.1 Simulation je Plan ⇒ Rangfolge spart (k−1)·RTT | `pipeline.rs:649-972` Schleife ohne Simulation; `:977` Auswahl; `:1132/1134` genau eine Simulation; `builder_n2_tests.rs:424/828/1054` erwarten `simulateTransaction` 1×. Rang 4 der TXT-Liste entfällt. |
| 1.3 Health 3–4× je Kandidat | `detect.rs:472` 1× → `evaluate_prepared(…, health)` `:551-559` → `sizes_with_health` (`sizing.rs:1010-1019`); `evaluate()` nur in Tests. Nur `emode_boosted` (Maintenance, getaggte Accounts) bleibt. |
| 1.4 zwei Signaturen | `tx::sign` nur `:1022` nach Auswahl. |
| 1.6 Richtung | frühe Marke ⇒ `detect→ready` zu lang, nicht zu kurz. |
| 2.4 36 s Owner-Bootstrap | Probe = `calc_value` über vorbereitete Komponenten (`health.rs:818-871`), eine offizielle Health je Account (`:625-648`); Modus default aus. |
| 2.5 parallele Quotes ×1,00/×1,05 | Route-Fit variiert `cap`=`maxAccounts` (`builder.rs:663-729`); Eingangsmenge nur im JIT-Shortfall-Solver. |
| 8.2 ≥ 1,5 s bis Send | Shadow: Fee→Sim ≥ 500 ms (≥ 1,0 s bei zwei Messages), kein Send (`ObserveOnly`, `setup.rs:514-517`). |
| 4.1 Drain während Fetch A | `accounts.clear()` (`state/mod.rs:444`) löscht angewendete Deltas. |
| 7.5 Guard `d≠0 && v==0` | produktive Kosten runden **auf** (`sizing.rs:683-717`, `ceil_u64`); Null nur auf der Gewinnseite (`floor_u64`, konservativ). Ein Guard würde Dust-Gewinne dauerhaft auf UNKNOWN setzen — **nicht** einbauen. |
| 9.1 Ein Frame „mindestens so sicher“ | `Reserved→HandoffUnknown` beim Replay ist bewusste Fail-Closed-Policy (`one_shot.rs:769-774`); Fusion = Schema-/Crash-Matrix-Änderung; UNKNOWN-Fenster nach Sync vor HTTP-Send bleibt. |
| 4.2 Skip identischer Oracle-Bytes (3 Zeilen) | 7/28 894 = 0,024 % (Ledger 4365); Skip wäre Coverage-Vertragsänderung (Slot/Revision/Clock). |
| 9.4 Budget im Shadow unklar | `p0ctl run start` übergibt Args (`p0ctl.rs:287-288`) ⇒ `startup.rs:79`; `run_econ4h` `:192-195` unbedingt. |

---

## 5. Umsetzungsreihenfolge für die Freigabe (jedes Paket: RED→GREEN, 1–3 gezielte Tests, Gate einmal am Abschluss)

| Nr. | Ticket | Art | Gate/Phase | Abhängigkeit |
|---|---|---|---|---|
| 1 | **V2-01** Receipts/Exit erneut lesen | Code (2 Funktionen) + 3 RED-Tests | FE-04 / G-E | keine |
| 2 | **V2-02** Fork-HALT triggert Recovery | Code (1 Bedingung) + Diagnostik + 4 Tests | S2-Recovery / G-C | C1-Verbotsliste |
| 3 | **V2-04** `T_detect` + Deadline-Anker | Vertragsentscheid (Anker) + Code + Schema-Bump | S0-02 / MT0 | Nutzerentscheid Anker |
| 4 | **V2-05** `Workload::FeeQuote` + `fee_rps` | Code + Config-Feld + Pacing-Tests | P6-MT.7 / MT1 | FREE_ONLY unverändert |
| 5 | **V2-06** Epoch-Fingerprint | Code (1 Zeile) + RED-Test + Messung | P6-MT.2 / E9 | keine |
| 6 | **V2-03** Threshold-Zeit-Wakeups | Code (Refactor `time_wakeups`) + erste Threshold-E2E-Tests | P6-MT.13 | nur vor Aktivierung des Modus |
| 7 | **V2-08** Sweep-/Burst-Parameter in `Limits` | Config + Manifest + Test | P6-MT.13 | keine |
| 8 | **V2-07** gemeinsamer Client/Keepalive | Code + Loopback-Test + A/B | MT1 | Budget-Entscheid (kein Prewarm) |
| 9 | **V2-09** Blockhash-Preflight | nur mit Loss-Prioritäts-Entscheid | P6-MT.4 | Nutzerentscheid |
| 10 | **V2-10 / V2-11** Index-Close, `admit`-Work | 2 kleine Fixes + 2 Tests | MT3 / S2 | keine |
| 11 | **V2-12** Recovery-Staging | zuerst Messauftrag, dann Modell | MT6 | gemessene Events/s |
| 12 | **V2-13** Threshold-Schnittmenge | Code + Tests | nach V2-03 | V2-03 |
| 13 | **V2-14** Mikro | nur bei Profilbeleg oder berührtem Modul | — | MT0-Profil |
| — | **V2-15** FE-06-Verträge | Policy/Design | G-F | vor LIVE |

**Korrigierte Rangliste gegenüber TXT:** Rang 1 „8.2 Konfig + 2 Codezeilen, 1,5 s“ → V2-05 (500 ms/Hop, eigener Workload, Config-Feld); Rang 2 „B6-REST“ = P6MT-J1/R1 (MT8/MT10, Architektur, hier nicht neu bewertet); Rang 3 Pool-Idle → V2-07 (ungemessen); Rang 4 „drei Simulationen, zwei fsyncs“ → **entfällt** (eine Simulation; fsync bewusst); Rang 5 → V2-12 nur als Staging; Rang 6 → V2-02 ist ein Codefix, nicht nur Zähler; Rang 7 → V2-03 (kein `crossed_candidates`-Swap); Rang 8 → V2-01 ist Code, nicht nur Policy; Rang 9 → V2-06; Rang 10 → entfällt; Rang 11 → V2-15; Rang 12 → V2-09/V2-14.

---

## 6. Nicht geprüft / Grenzen

- Nicht gelesen: `reservation.rs`, `archive.rs`, `exit_admission.rs`, `ownership.rs` (MT5b), `tx/mod.rs`, `lookup_tables.rs`, `projection.rs`, `integration.rs`, `winner_tracker.rs`, `market/*`, `storage/*`, Python-Pins. `exit.rs`, `admission.rs`, `lifecycle.rs`, `startup.rs`, `census.rs` nur in den genannten Funktionen.
- Keine eigene Messung: alle µs/ms-Angaben sind Ledger-/BUGS-Zitate (mit Zeile) oder ausdrücklich „ungemessen“. Statische Beweise belegen Aufrufreihenfolge und Arbeit, nicht den absoluten Gewinn.
- Die uncommitteten parallelen Änderungen im Worktree (§ Hinweis oben) können einzelne Pfade (`pipeline.rs`, `builder.rs` JIT, `admission.rs`) bereits verschieben; vor jedem Fix neu pinnen.
- Keine Änderung an FREE_ONLY-, PERSIST-/JFI-, Provider-, LIVE- oder `ObserveOnly`-Freigaben durch dieses Dokument.

---

## 7. Nachtrag 24.10. — Antwort des TXT-Autors auf dieses Dokument geprüft

- **Übernommen:** Wiederholungsdrossel im Threshold-Pfad (V2-13), Send-Client aus dem `Simulation`-Slot (V2-05), Anker-Empfehlung `t_rx` für `deadline_passed` (V2-04). V2-12 entspricht 5a.1 plus Helper-Thread: kein Dissens.
- **Nicht übernommen:** „2.1/1.4 lagen am Pin“ — am 20.10. (`ba4a3c3e`) stand bereits Planwahl → eine Signatur → eine Simulation; Lesefehler, kein Code-Drift (1.3 dagegen plausibel: MT2a `1a05bd78` vom 24.10.). „Kein 72-h-Blocker“ für V2-12 ist zu stark: nicht belegt ≠ ausgeschlossen; Events/s und Queue-Peak aus **demselben** Lauf bleiben Pflicht vor dem 72-h-Gate.
- **Prozess:** zwei Project_0-Sessions im selben Worktree `../DarkBot-project0` waren die Ursache des HEAD-Vorfalls beim ersten Push; jede Session erhält einen eigenen Worktree mit eigenem Index.
- **Zweiter Nachtrag:** drei weitere Einwände geprüft und übernommen — V2-05 Testbedingung (globaler Bucket 200 ms), V2-06 Testumbau + Lebensdauer-Vertrag + offener owner/lamports-Test, V2-10 Filter-Austritt nicht durch Update-Handling abgedeckt.
