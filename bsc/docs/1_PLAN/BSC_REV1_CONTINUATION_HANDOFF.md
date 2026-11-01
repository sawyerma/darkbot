# BSC REV.1.0 — Fortsetzungs-Übergabe (technische Momentaufnahme 01.11.2026, ~20:20 CET)

Zweck: Eine neue Claude-Instanz ohne den alten Chatverlauf setzt BSC REV.1.0 exakt an dieser Stelle fort.
Diese Datei ist **keine** Projektautorität. Verbindlich bleiben:
- `bsc/docs/1_PLAN/PHASENPLAN.md`
- `bsc/docs/1_PLAN/STATUS.md`, mit dem Abschnitt „REV.1.0-Fertigstellung“ und der Audit-E-Pflichtstatus-Tabelle
- `bsc/docs/1_PLAN/BSC_MESSPLAN_PASS_FAIL_OPEN_20261027.md` (Messplan)
- `bsc/docs/1_PLAN/BSC_PRODUCTION_BUILD_MASTER_PLAN.md`
- `bsc/docs/5_OFFENE_FEHLER/BSC_Bug_V3.md` (BSC3-Register) und `bsc/docs/5_OFFENE_FEHLER/BSC_BUG_V3_UMSETZUNGSAUFTRAG.md`
- `bsc/docs/1_PLAN/ACCEPTANCE_MAP.json`

Bei Widerspruch gelten diese Dateien. Alle Angaben hier sind am genannten Zeitpunkt geprüft. Was nicht geprüft ist, steht ausdrücklich als OFFEN oder UNVERIFIZIERT da.

---

## 0. Kurzfassung (wichtigste Fakten)

1. **R1, R2, R2b und E-A sind auf `origin/main` und haben grüne CI** (Nachweise in §3):
   - R1 `0159c1a7`
   - E-A `8a9584d9` … `f6156fa0`
   - R2 `1d29cec7`
   - R2b `9d5ed3a0`

   Die Annahme im Übergabeauftrag, R2 sei „mitten in der Implementierung“ und ein R2-Mutationslauf aktiv, ist **überholt**. R2 wurde nach fünf Review-Runden, 59 roten Mutanten und GATE_OK als `1d29cec7` gepusht.
2. **In Arbeit ist R3a** (Recovery/Betreiber, erster Teil). Der Stand ist **uncommitted** im Worktree `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/wtR2`:
   - Basis `1d29cec7`, 15 Dateien, +1391/−51.
   - `cargo check --workspace --all-features --tests --bins` ist grün.
   - **Noch keine Tests geschrieben, kein fmt, kein Re-Pin, kein Review, kein Gate.**
   - Zusätzliche Patch-Sicherung: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/wtR2_R3a_uncommitted_before_move.patch`, sha256 `fab0ae28ddbfb9aeb25cc008eb8a4b8c72b0c5626dd8477150ff461b571f3766`. Er ist identisch mit `git -C …/wtR2 diff` nach dem Verschieben.
3. **R2b (`9d5ed3a0`) hat noch kein unabhängiges Review.** Gate EB1 GATE_OK, CI grün. Das Review ist der erste offene Prüfschritt.
4. Keine Agenten und keine eigenen Hintergrundjobs laufen mehr. Die beiden Agenten (R2b, R8) endeten am Session-Limit, R8 hat nichts geändert. Kein mutierter Quellcode liegt herum, siehe §2.
5. **Dauerhafter Ort aller BSC-REV.1.0-Worktrees, Build-Targets, Werkzeuge und Notizen:** `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/`. Er liegt nicht mehr unter `/private/tmp`.

---

## 1. Orte (Repository, Worktrees, Werkzeuge)

| Was | Pfad | Stand |
|---|---|---|
| Hauptcheckout (BSC, gemeinsam genutzt) | `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot` | Branch `main` auf `1d29cec7`, **4 hinter `origin/main`**. Nur lokale untracked Evidence (siehe §3.1). **Nicht pullen, nicht resetten.** |
| R3a-Arbeitsbaum | `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/wtR2` | detached `1d29cec7`, 15 Dateien uncommitted (R3a), siehe §6 |
| R2b-Agent-Baum | `…/DarkBot-bsc-rev1/wtEA` | detached `9d5ed3a0`, sauber |
| R8-Agent-Baum | `…/DarkBot-bsc-rev1/wtR8` | detached `1d29cec7`, sauber (der Agent hat nichts geschrieben) |
| Build-Targets | `…/DarkBot-bsc-rev1/{wtR2,wtEA,wtR8}-target` | Je Worktree zeigt der Symlink `bsc/engine/target` auf das zugehörige Target. Der Symlink ist git-ignoriert. Nach dem Verschieben baut Cargo die Workspace-Crates einmal neu (Pfade geändert). |
| Werkzeuge (Pfade bereits angepasst) | `…/DarkBot-bsc-rev1/tools/` | siehe §11 |
| Notizen, Audits, Review-/Mutations-/Gate-Logs | `…/DarkBot-bsc-rev1/scratch/` | Kopie des alten Scratchpads: alle Dateien unter 2 MB, dazu `rev1/`, `eb/`, `ea/` ohne `ea/base-target` |
| Project_0-Worktrees (**tabu für BSC**) | `../DarkBot-project0`, `../DarkBot-p0-ctrl`, `../DarkBot-p0-gate`, `../DarkBot-p0-mut` | nicht anfassen |

Der alte Scratchpad-Pfad `/private/tmp/claude-501/-Users-sawyer-ma-Desktop-Firma-2-DarkMa-6-DarkBot-DarkBot/cb129474-4d1b-4d79-b89c-5091d2f40cb5/scratchpad` existiert noch, ist aber nicht mehr nötig. Die Worktrees wurden mit `git worktree move` verschoben, die Targets per `mv` (Rename auf demselben Volume). Es gab kein Neu-Auschecken und keinen Verlust.

**Plattenplatz: Auf `/System/Volumes/Data` sind nur ~18 GB frei.** Keine großen Kopien anlegen, Targets nicht duplizieren.

---

## 2. Laufende Arbeiten beim Einfrieren (geprüft 20:14–20:20)

- **Agenten:** keine aktiv.
  - Agent „R2b“ (vorher E-A): Endzustand Commit `9d5ed3a0` gepusht, CI-Warter beendet (`scratch/eb/ci1.txt`: bsc, gateway und Project 0 status contract = success). Danach am API-Session-Limit abgebrochen, sein Abschlussbericht kam nicht mehr.
  - Agent „R8“: nach dem Setup am Session-Limit abgebrochen. `wtR8` ist sauber, nur das Target ist vorgewärmt (`scratch/r8_warm.log`).
- **Eigene Hintergrundjobs:** keine. Der CI-Warter für R2 ist beendet (`scratch/ci_R2.out`).
- **Mutationen:** Kein Mutationslauf aktiv.
  - Die R2-Läufe (`scratch/mut_r2*.out`) sind abgeschlossen. `tools/mut_r2.py` stellt jede Datei mit frischem mtime wieder her.
  - Der R2-Code ist so committet, wie er nach den Läufen reviewt wurde (`1d29cec7`).
  - `wtEA` ist sauber, also enthält er keinen Mutanten.
  - Der Diff in `wtR2` ist ausschließlich beabsichtigter R3a-Code (Inhalt in §6). Es gab noch keinen Mutationslauf auf R3a.
- **Fremde Prozesse (nicht beendet, Regel: keine Prozesse töten):**
  - PID `64432`: G0.4-Kampagne `~/.darkbot/g04/campaign_20261027/bsc_runtime run …`. Sie läuft bis **03.11. 08:05Z**, eigenes Binary und eigene Config-Kopie, **read-only**.
  - PID `44297`: verwaister `…/DarkBot/bsc/engine/target/debug/bsc_runtime run --rundir /var/folders/…/T/bsc-runtime-sigterm-44061 …`, ppid 1, gestartet ~19:51. Vermutlich ein abgebrochener SIGTERM-Test, nicht von dieser Instanz. Er schreibt nur in sein `/var/folders`-Temp-Verzeichnis. Der Nutzer kann ihn beenden. Er gefährdet keinen Worktree.

---

## 3. Git- und CI-Stand (tatsächliche Ausgaben)

### 3.1 Hauptcheckout
```
$ git status -sb
## main...origin/main [hinterher 4]
?? bsc/config/g07_attempt1_acceptance_v4.dbrec
?? bsc/config/g07_attempt1_acceptance_v4.e1-review.dbrec
?? bsc/docs/9_ALT/Backup_v1.zip
?? bsc/engine/crates/execution/src/runtime_live.rs.orig
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/
?? bsc/native/evidence/g07/readiness_20261020T120918Z/
?? bsc/native/evidence/g07/readiness_20261020T121103Z/
```
`git status --porcelain=v1` zeigt dieselben acht `??`-Einträge, nichts gestaged und nichts geändert.
**Diese acht Pfade nie committen und nie anfassen** (lokale Evidence und Nutzerdateien; `runtime_live.rs.orig` gehört dem Nutzer).

### 3.2 Worktrees (`git worktree list --porcelain`, gekürzt auf BSC)
```
worktree /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot              HEAD 1d29cec7… branch refs/heads/main
worktree /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/wtEA HEAD 9d5ed3a0… detached
worktree /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/wtR2 HEAD 1d29cec7… detached
worktree /Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/wtR8 HEAD 1d29cec7… detached
(+ vier Project_0-Worktrees, tabu)
```

### 3.3 `origin/main` und neue Commits
`git fetch origin` → `origin/main = fffa69440dc8a85d2a7ac05021df4cfe05ebfcd0`, vor dem Commit dieser Übergabedatei.

Commits seit `1d29cec7`:
- `9d5ed3a0` feat(bsc): REV.1.0 R2b — LIVE-Tab zeigt Kampagnen-Verdikt und Versuche (BSC)
- `35f58ff0`, `69f0103b`, `fffa6944`: Project_0 DEC-18 package 2 (**Project_0, fremd, nicht anfassen**)

BSC-Pakete auf `main` (chronologisch): `0159c1a7` R1 → `8a9584d9`, `ca08fcb8`, `d0883feb`, `f6156fa0` (E-A) → `1d29cec7` R2 → `9d5ed3a0` R2b.

### 3.4 Worktrees gegenüber `origin/main`
- `wtR2`: HEAD `1d29cec7`, also 4 Commits hinter `origin/main` (R2b plus drei Project_0-Commits).
  - Uncommitted, nicht gestaged (`git status --porcelain=v1`):
```
 M bsc/config/ingress.public.env
 M bsc/engine/crates/execution/src/bin/bsc_runtime.rs
 M bsc/engine/crates/execution/src/live_attempts.rs
 M bsc/engine/crates/execution/src/live_end.rs
 M bsc/engine/crates/execution/src/live_ledger.rs
 M bsc/engine/crates/execution/src/paper_exec.rs
 M bsc/engine/crates/execution/src/runtime.rs
 M bsc/engine/crates/execution/src/runtime_exec.rs
 M bsc/engine/crates/execution/src/runtime_live.rs
 M bsc/engine/crates/execution/src/sources.rs
 M bsc/engine/crates/execution/tests/c3_paper_latency.rs
 M bsc/engine/crates/execution/tests/live_e2e.rs
 M bsc/engine/crates/execution/tests/strategy_paper_latency.rs
 M bsc/engine/crates/qualification/src/status/tests.rs
 M bsc/engine/crates/record/src/schema/bsc_runtime.rs
```
  - Keine untracked Dateien (node_modules und der target-Symlink sind ignoriert).
- `wtEA`: HEAD `9d5ed3a0`, sauber, 3 hinter `origin/main` (nur Project_0).
- `wtR8`: HEAD `1d29cec7`, sauber.

### 3.5 CI (GitHub Actions) — geprüfte Ergebnisse
| Commit | Ergebnis | Beleg |
|---|---|---|
| `0159c1a7` R1 | bsc success, Project 0 status contract success | `scratch/ci_R1.out` (runs 41287776723 / 41287776667) |
| `f6156fa0` E-A | bsc (macOS, ubuntu, frontend) + Project 0 success | STATUS.md-Gate-Tabelle; Agentenbericht |
| `1d29cec7` R2 | bsc success (41300469409), Project 0 status contract success (41300469399) | `scratch/ci_R2.out` |
| `9d5ed3a0` R2b | bsc success (41302394662), gateway success (41302394674), Project 0 status contract success (41302394646) | `scratch/eb/ci1.txt` |
| Übergabe-Commit | CI nach dem Push abfragen, SHA siehe Schlussmeldung | `tools/ci_wait2.py <sha>` |

Hinweis: `gh` ist in der Shell nicht installiert. Direkte unauthentifizierte API-Aufrufe aus einer Vordergrund-Shell bekamen am 01.11. abends HTTP 404, obwohl `ci_wait2.py` zuvor als Hintergrundjob funktionierte. Die Ursache ist UNVERIFIZIERT, vermutlich Netz-/Sandbox-Kontext. `ci_wait2.py` deshalb als Hintergrundjob laufen lassen.

---

## 4. Verbindliche Regeln (dauerhaft, aus Nutzeraufträgen)

**Auftrag 01.11. (bindend):** BSC REV.1.0 vollständig fertigstellen:
- R1–R14
- Audit E1–E11: jeder Befund belegt geschlossen, mit Pflichtstatus
- alle offenen und teilweisen BSC3-Reste
- PHASENPLAN, Messplan und STATUS abgleichen
- die ganze Kette Quellen → Ingress → kanonisch → Strategie → Economics → Admission → Signatur → Builder → Receipt → Position → Exit → PnL → Status/Frontend

Abschluss ist eine vollständige Anforderungs-/Abnahmematrix (Anforderung → Datei, Test, SHA, Status).

**Arbeitsweise:**
- Keine Zeitschätzungen. **Keine Rückfragen**; nur fertige Ergebnisse und echte Blocker melden.
- Betreiberentscheidungen blockieren das Programmieren nicht. Dafür technische Varianten, Konfiguration und sichere gesperrte Defaults bauen.
- Pro Paket:
  - fail-before (Rot→Grün)
  - Unit-, Integrations-, System- und, wo relevant, Browser-E2E-Tests
  - Restart, Crash, Race, Reorg, Nonce, Budget, Recovery
  - Mutationen
  - Latenz und Ressourcen vorher/nachher auf Hot-Paths
  - unabhängiges Review
  - volles Gate auf dem Endstand
  - Commit und Push direkt auf `main`
  - CI auf genau diesem SHA
- Tests nie abschwächen. Externe oder wirtschaftliche Nachweise bleiben **EXTERNAL_BLOCKED**, nie PASS.
- Push grüner Inkremente mindestens alle 30 Minuten. Doku nur als Einzeiler. Maximal **2 Agenten** gleichzeitig. Agenten und Reviewer erteilen keine Freigaben.

**Audit-E-Regeln:**
- NO MOCK im Produktivpfad (Fixtures nur in Tests)
- NO LEGACY (außer für bindende historische Evidence oder bestehende persistente Daten)
- NO HARDCODING (Grenzen nur über typisierte Config/Limits mit abgeleiteten Defaults; Protokollkonstanten bleiben)
- keine parallelen Architekturen, kein Bloat, keine Scheinfunktionen
- Gepinnte Dateien brauchen Re-Pin und Requalifikation. Historische Evidence und eingefrorene Verträge bleiben.

**Sicherheit und Prozess:**
- **Kein SIGN/ARM/SEND/BUY/SELL, keine Echtgeld-Aktion** ohne ausdrückliche Freigabe. **Die Freigabe für den Echtgeldtest ist NICHT erteilt.**
- G0.7-Guard bleibt **UNARMED**.
- Reihenfolge: REV.1.0 fertig und gehärtet → Gesamtabnahme → externe Qualifikation und ausdrückliche Freigaben → Echtgeld-BUY/SELL-Test.
- FREE_ONLY. **Kein internes JSON** (intern nur binäres DBREC, JSON nur auf dem externen JSON-RPC-Draht und für historische Leser).
- BSC und Project_0 strikt getrennt; `project_0/` und `darknode/` sind tabu. Keine neue Architektur. REV.2.0 (BSC-WATCH) ist später und separat; nicht beginnen.
- Kein Force-Push, kein `reset --hard`, kein `git clean`, kein `stash pop`, kein Rebase eines fremden Stands. Fremde Änderungen nie überschreiben. Keine neuen Entwicklungsbranches; Ziel ist immer `main`.
- `bsc/secrets.env` und `.env` nie ausgeben. Den B-Salt nie in Git, Logs oder UI.
- Nie committen: die acht Pfade aus §3.1.
- Nie `darkbotctl deploy` ausführen. Nur Dateien formatieren, deren Basis sauber ist (nie paketweites `cargo fmt`, wegen der eingefrorenen G0.7-Hashes).
- G0.4-Kampagne read-only. Nie `bsc-dataseed.bnbchain.org`, PublicNode oder dRPC kontaktieren.
- Write-ahead und Idempotenz von BSC3-024/045 nicht verschlechtern. Credential-Materialisierung nicht umgehen.
- Keine Prozesse töten (TaskStop nur für eigene Tasks). Je Worktree/Target nur ein Gate gleichzeitig, und den Baum nicht ändern, während sein Gate läuft.
- **Commit-Mechanik:** immer über `tools/commit_wt.sh` (temporärer Index auf `origin/main`; verweigert, wenn `origin/main` einen gelisteten Pfad seit `<since>` geändert hat), dann `git push origin <sha>:refs/heads/main`, dann `tools/ci_wait2.py <sha>`.

---

## 5. Statusmatrix R1–R14

Statuswerte: NOT_STARTED, IN_PROGRESS, LOCAL_TESTED, REVIEW_PENDING, GATE_PENDING, PUSH_PENDING, CI_PENDING, DONE_VERIFIED, EXTERNAL_BLOCKED.

Quellen:
- Paketplan `scratch/rev1/PLAN.md`
- Audits `scratch/rev1/auditA..D_summary.md`, `scratch/rev1/auditE_cleanup.md`, `scratch/rev1/auditE_register.md`
- STATUS.md (REV.1.0-Abschnitt)

| Paket | Soll (Plan) | Status | Commit/Ort |
|---|---|---|---|
| R1 | LIVE-Betriebsblocker BSC3-080..084 | **DONE_VERIFIED** (Review FAIL→PASS, Gate GR1 OK, CI grün) | `0159c1a7` |
| R2 | Versuchs-Record, Auswerter, Platzierungs-Halt, Kampagnen-Verdikt, G0.8-15-s-Fenster | **DONE_VERIFIED** (5 Review-Runden, letzte PASS; 59 Mutanten RED; Gate GR2e OK; CI grün). Echte Kampagnenläufe: EXTERNAL_BLOCKED (nach 03.11., Manifest durch den Betreiber) | `1d29cec7` |
| R2b | LIVE-Tab-Projektion Kampagne/Versuche | **REVIEW_PENDING** (Code, Tests, 9 Mutanten RED, Gate EB1 OK, Push und CI grün; **unabhängiges Review fehlt**) | `9d5ed3a0` |
| R3 | Receipt-Recovery (024/041), EXIT_STOPPED resume/abandon, Nonce-Burn gesperrt, Ledger-Reparatur und Temp-Cleanup (045), Slot-Rückgabe (068), G0.7-Lot-Adoption; laut R1-Commit auch exit-now/write-off für gestrandete Lots; Audit A: Betreiberbefehle auch über Gateway und `LiveActions.tsx` | **IN_PROGRESS** (R3a kompiliert, uncommitted; R3b nicht begonnen) | `wtR2` (§6) |
| R4 | Grant-Varianten gesperrt: 069 Outlier-Ack, 059 max. Dauer, 029 Builder-Payment, 042 C3-Re-Arm, kampagnenweite Spend-Grenze, §20 public-only-Verweigerung, Builder-Frische/Fee-Check, C2-LIVE-Doku | NOT_STARTED | – |
| R5 | Exit-Routen gesperrt: 030 Public-Mempool-Adapter, Re-Price-Variante, DEX-Exit migrierter Lots | NOT_STARTED | – |
| R6 | Quellen: GETH/Hot0Poller-Adapter, Health-/Capability-Gating, Queues je Quelle, CONFLICT-Zustand, Census je Quelle (O05/§3a), 48Club-Adapter synthetisch | NOT_STARTED | – |
| R7 | Kettenintegrität 056: Header-RLP-Hash, Tx-/Receipt-Roots, Divergenz-Quarantäne (gesperrt) | NOT_STARTED | – |
| R8 | Qualifikation: 049 Evidence-Map und max. Alter, 066 Profilbindung, 064 OBSERVABLE-Kriterien, G0.4-10-min-Variante, 035 LaunchedToDEX (Netz), Q5A-DBREC, 018 dauerhafte Writes, Release-Manifest-Identität | NOT_STARTED (der Agent hat nur `wtR8` angelegt; sein Auftragstext: siehe §12.5) | `wtR8` sauber |
| R9 | 001 Präsenzsignatur-Anker (aus per Default) | NOT_STARTED | – |
| R10 | Ops/Terminal/Frontend: Provisionierung (Key, Token), BSC_LIVE_*-Config, darkbotctl-Wrapper, Status BSC, `qualify --replace`, LIVE-Tab-Vollständigkeit, Wallets LIVE-Buyer, execution.events-Topic (durch E-A entfernt), veraltete Kommentare, bsc.spec.ts und CI (durch E-A erledigt), Pixel/a11y, Dirty-Flag | NOT_STARTED (Teile durch E-A erledigt, siehe §8) | – |
| R11 | Harnesses: Mock-WSS-Runtime (A02–A16), run()-Matrix B01–B16, G0.7-Runner-Verhalten (C08–C11, C16), echter Prozess-LIVE-Loopback-E2E mit Stand-in-Builder, Kill/Crash, Disk-full-Image, C10/R5, Mutationsskript, §9.5/§3b-D-Register, C17/C20, Läufe nach 03.11. | NOT_STARTED | – |
| R12 | Four Classic/OpenFour Economics und Ausführung | NOT_STARTED | – |
| R13 | FOREIGN_AUTO-Profile | NOT_STARTED | – |
| R14 | fmt/clippy in der CI (Repo-weit `cargo fmt --check` mit Exit 1, 355 Hunks in 38 Dateien auf `9d5ed3a0`), Läufe nach 03.11., STATUS-/Runbook-/Messplan-Rebaseline, Gesamtabnahme | NOT_STARTED (Läufe nach 03.11.: EXTERNAL_BLOCKED durch die Kampagne) | – |
| E1–E11 | Audit-E-Befunde, siehe §8 | IN_PROGRESS (E-A fertig) | E-A `8a9584d9`…`f6156fa0` |

Abhängigkeiten:
- R3b (Receipt-Recovery) braucht den Follower-Aufruf `Call::Receipt` und die Bestätigung durch die zweite Quelle (BSC3-056, `canonical_confirm.rs`).
- R5 DEX-Exit hängt am Zustand gestrandeter Lots (R1/R3).
- R7 vor R6 CONFLICT ist sinnvoll, aber nicht zwingend.
- R10 Gateway- und Frontend-Aktionen für die R3-Betreiberbefehle kommen erst nach R3.
- R14 kommt zuletzt.

---

## 6. R3 — exakter Arbeitsstand (uncommitted in `wtR2`)

Design-Notiz: `scratch/rev1/r3/DESIGN.md`. R3 ist zweigeteilt:
- **R3a**: Ledger v4, 068, Betreiberfakten, Adopt, 045 Check/Quarantäne/Temp-Cleanup. Hier wird gearbeitet.
- **R3b**: Receipt-Recovery 024/041 und Nonce-Burn gesperrt. Noch nicht begonnen.

### 6.1 Bereits geschrieben (kompiliert, **ungetestet**)

**Record-Schema** `bsc/engine/crates/record/src/schema/bsc_runtime.rs`:
- `live_ledger::SCHEMA` jetzt **schema_version 4** mit den optionalen Feldern `ATTEMPT` 28 (B256), `REQUEST_ID` 29 (Text), `DECIDED_BY` 30 (Text), `EVIDENCE_SHA256` 31 (B256), `SOURCE` 32 (Text).
- v3 bleibt lesbar als `live_ledger::V3`; v2 und v1 bleiben ebenfalls. In `ALL` wurde `V3` ergänzt.
- KIND-Doku um RESPONDED, PLACED, RESUMED, WRITTEN_OFF, EXIT_REQUESTED und ADOPTED ergänzt.
- Neues Schema `live_ledger_quarantine` (record_type **74**, `darkbot.bsc.live-ledger-quarantine.v1`) mit den Feldern SEQ, SHA256, PROVENANCE_TOOL, PROVENANCE_OBSERVED_UNIX_MS, REQUEST_ID, DECIDED_BY, WHY und GENERATION; in `ALL` eingetragen.

**Ledger** `execution/src/live_ledger.rs`:
- `FactKind` hat neu `Resumed`, `WrittenOff`, `ExitRequested` und `Adopted`. `Adopted` ist eine erste Fakt-Art (`first()`), dazu kommt `operators()`.
- Neue Typen: `Operator {request_id, decided_by, evidence, at_ms}` (`at_ms` = Provenance-Zeit des Eintrags, beim Decode gesetzt) und `FactSource {Receipt, Nonce}`.
- `Fact` hat neu `attempt: Option<[u8;32]>`, `operator: Option<Operator>` und `source: Option<FactSource>`.
- `Fact::about()` setzt `operator` und `source` zurück.
- `Fact::admitted()` setzt `attempt` = tx_hash für einen nicht-Lease-BUY, sonst None.
- Encode/Decode der v4-Felder. Strenge Regeln: Operator-Felder genau in Operator-Fakten, Evidence genau in ADOPTED, Source nur auf INCLUDED/REVERTED/REPLACED. Decode-Reihenfolge SCHEMA → V3 → V2 → V1.
- `Tx.decided: Decided {resumes, excused, written_off: Option<u64>, exit_requested}`.
- Fold:
  - RESUMED entschuldigt die bis dahin revertierten Exit-Schritte des Tokens.
  - WRITTEN_OFF macht eine Position `Open` zu `PositionState::WrittenOff` mit `closed_ms` = Faktzeit und PnL = Erlös − Input − Gas. Ein noch ausstehender SELL entscheidet zuerst; ein später eingeschlossener SELL schließt die Position.
  - ADOPTED wird als INCLUDED gefaltet, `inclusion` ist der Fakt selbst. ORPHANED eines ADOPTED wird zu UNKNOWN.
  - `PositionState::WrittenOff` (Name `WRITTEN_OFF`).
- `Book::withdrawn_attempts()`: Versuche, deren jeder BUY-Fakt (Transaktion und Lease) WITHDRAWN-first und WITHDRAWN ist. Schlüssel ist `attempt` oder der tx_hash.
- `Ledger::fact_count()`.
- Konstanten `LIVE_LOT_NOT_OPEN`, `LIVE_LOT_EXIT_OUT`, `LIVE_OPERATOR_EXIT_OFF`, `LIVE_REQUEST_REUSED`, `LIVE_ADOPTION_REFUSED`.
- `LotDecision {Resume, WriteOff, ExitNow}` mit `parse("resume"|"write-off"|"exit-now")`.
- `Ledger::decide(lot, what, request_id, decided_by, operator_exit, now_ms)`:
  - Betreiber-Lock `live-ledger/operator.lock`.
  - Idempotent je Request-ID; eine andere Entscheidung unter derselben ID gibt `LIVE_REQUEST_REUSED`.
  - Write-off und Exit-now nur ohne ausstehenden Exit-Schritt und nur bei Zustand Open.
  - Exit-now nur mit `BSC_EXEC_LIVE_OPERATOR_EXIT=ON`.
- `G07Buy` und `g07_buy(result, capture, buyer, finality_blocks)`:
  - G0.8-Ergebnis v7 plus dessen Chain-Capture-Journal (SHA muss passen).
  - Bedingungen: BUY_STATUS==1, CANONICAL, Acquisition-Evidence vollständig, positive zurechenbare Acquisition, BUYER == LIVE-Buyer, CONFIRMATIONS ≥ Finalität, Token-Netto > 0.
  - Aus dem Capture kommen Nonce, Gas und Preis des BUY, der Sender des Launches und die Block-ms. Wert und Gas werden mit dem Ergebnis abgeglichen.
- `Ledger::adopt(buy, chain_id, request_id, decided_by, now_ms)`: ADOPTED-Fakt mit Grant-Generation 0 und Komponente ExecC3; verweigert, wenn der Hash schon im Ledger steht.
- **BSC3-045:**
  - `append()` hält die `pending-*.tmp` exklusiv gelockt, bis sie veröffentlicht ist, und prüft per Inode, dass die Datei noch die eigene ist; sonst legt es eine neue an. `write_entry` nimmt jetzt `&mut File`.
  - `Ledger::clear_pending()` entfernt nur tmp-Dateien, deren Lock es bekommt.
  - `Ledger::check()` meldet (Anzahl, unlesbare (seq, warum)).
  - `Ledger::quarantine(seq, request_id, decided_by, generation, now_ms)`: nur für eine nicht dekodierbare Datei; erst den Record create-new und dauerhaft schreiben, dann den Eintrag umbenennen nach `live-ledger-quarantine/<seq>.entry`, beide Verzeichnisse fsyncen; idempotent je Request.
  - `Ledger::quarantined()` liefert die höchste Owner-Generation; ein unlesbarer Record zählt als `u64::MAX`.

**Owner** `execution/src/sources.rs`:
- `safe_id` ist jetzt `pub(crate)`.
- `slot_returns_dir` = `buy-attempt-returns/`.
- `latest_slot_ms` liest auch die zurückgegebenen Kampagnen-Slots (Vorwärtsuhr BSC3-004).
- `Store::return_attempt(attempt_hex)`:
  - unter dem Spend-Lock
  - verschiebt jeden Tages-Slot mit attempt == A oder `A-<Tag>` (Lease-Tages-ID) und den Kampagnen-Slot von A per Rename nach `buy-attempt-returns/{A}-daily-{utc}-{slot}.dbrec` bzw. `{A}-total-{slot}.dbrec`
  - sync_dir aller berührten Verzeichnisse, danach `flush_device`
  - idempotent; nicht dekodierbare Slots bleiben
  - **Spend-Einträge bleiben bewusst stehen**: Write-ahead und „nie zweimal zugelassen“ (BSC3-024) bleiben unverändert. Die Alternativen einer Nonce zählen ohnehin nur einmal.
- Neue Konstante `LIVE_LEDGER_QUARANTINED`.

**LIVE-Runtime** `execution/src/runtime_live.rs`:
- Neue Felder `folded_len`, `returned`, `slot_return_failed`, `slot_return_error`, `resumes_seen`, `exit_requests_done`, `operator_exits_locked`, `decided` und `quarantine`.
- `refold()` setzt `folded_len` und `quarantine`.
- `position_changes()` wurde aus `record()` herausgelöst (gleiches Verhalten).
- `fold_decisions()` läuft im Tick nach dem Store-Open:
  - Wuchs das Ledger, folgt ein Refold und die Positionsänderungen landen in `decided`.
  - RESUMED → `reopen_exit`, dazu ein Token in `decided.1`; hatte das Lot schon Exit-Schritte, folgt `want_exit`.
  - EXIT_REQUESTED einmal je Prozess → `want_exit` bei ON, sonst wird `OPERATOR_EXIT_LOCKED` gezählt.
- `take_decided()`.
- `return_withdrawn(store)` im Tick nach `fold_decisions`: für jedes `withdrawn_attempts()`, das nicht von einem Lease oder `lease_attempt` gehalten wird → `store.return_attempt`. Fehler werden gezählt (`SLOT_RETURN_FAILED` in REFUSALS) und später wiederholt.
- `exit_stopped()` ignoriert die entschuldigten Reverts.
- `buy_refusal()`: direkt nach LIVE_PLACEMENT_VIOLATED → `LIVE_LEDGER_QUARANTINED`, solange die Grant-Generation ≤ der Quarantäne-Generation ist.
- `folded()` ruft beim Start `clear_pending()`.
- Der `match` der Fakt-Arten in der Funds-Rechnung ist um die neuen Arten ergänzt (zählen nicht).

**LIVE-End** `execution/src/live_end.rs`:
- Der BUY-Fakt trägt `attempt` = Lease-Attempt oder tx_hash.
- Der Lease-Fakt übernimmt ihn per `..fact.clone()`.

**Runtime-Loop** `execution/src/runtime.rs`:
- Neuer Config-Schlüssel `BSC_EXEC_LIVE_OPERATOR_EXIT` (ON/OFF, Pflicht mit `BSC_EXEC_PAPER_IDENTITY`, sonst verweigert) → `ExecConfig.live_operator_exit`.
- Nach dem LIVE-Tick führt `take_decided()` → `Opened`/`Ended` zu `s.hold`/`s.unhold` (journalisiert); `resumed` → `LotDeciders::resume(token)` (stranded/retry/refused gelöscht), danach `refresh_lot_deciders`.

**Config** `bsc/config/ingress.public.env`: `BSC_EXEC_LIVE_OPERATOR_EXIT=OFF` mit Kommentar.

**`ExecConfig`-Literale:** `live_operator_exit: false` in `paper_exec.rs`, `runtime_exec.rs`, `tests/c3_paper_latency.rs`, `tests/strategy_paper_latency.rs` und `tests/live_e2e.rs`.

**Fact-Literale:** `attempt: None, operator: None, source: None` in `runtime_live.rs` (10), `live_ledger.rs` (2), `live_end.rs` (1, Response-Fakt), `live_attempts.rs` (1) und `qualification/src/status/tests.rs` (1).

**CLI** `execution/src/bin/bsc_runtime.rs`:
- Hilfsfunktion `hex32()`; `sha256_flag` nutzt sie.
- Neue Befehle:
  - `live lot resume|write-off|exit-now --lot <0x…> --state --config --request-id --decided-by`
  - `live adopt-g07 --g08 --capture --state --config --request-id --decided-by` (chain_id = `wss::BSC_MAINNET_CHAIN_ID`, Finalität = `BSC_CANONICAL_REORG_DEPTH`)
  - `live ledger check --state` (Exit 0 nur, wenn alles lesbar ist)
  - `live ledger quarantine --seq --state --request-id --decided-by` (Generation aus dem Owner)
- Usage und Doku ergänzt.

### 6.2 Offen in R3a (in dieser Reihenfolge)

1. **Tests schreiben** (fail-before = Feature fehlt auf `1d29cec7`, dazu Mutationen):
   - `live_ledger.rs` tests:
     - v4-Roundtrip und v3-lesbar, Operator/Evidence/Source-Regeln
     - `withdrawn_attempts`: Lease-BUY und Lease-Fakt WITHDRAWN → ja; RELEASED/UNKNOWN → nein; v3 ohne attempt → tx_hash
     - `decide`: alle Vorbedingungen, Wiederholung → None, wiederverwendete ID → Fehler, Write-off bei ausstehendem SELL → `LIVE_LOT_EXIT_OUT`, Exit-now OFF → Fehler
     - Write-off-Fold: Zustand, PnL, `closed_ms`, SELL draußen / SELL später
     - RESUMED entschuldigt die Reverts
     - `g07_buy`/`adopt` mit synthetischem G0.8-v7-Record und einem `JournalWriter`-Capture (chain_block, chain_transaction): Erfolg und jede Verweigerung
     - `clear_pending`: gelockte Datei bleibt, ungelockte geht; `append` danach ok
     - `check`/`quarantine`/`quarantined`: unlesbar → `facts()` Err → Quarantäne → `facts()` Ok; lesbare seq verweigert; idempotent; andere Request-ID verweigert
   - `sources.rs` tests (`return_attempt`):
     - Lease-Tages-ID und normale ID, Usage sinkt
     - Uhr-zurück wird weiter verweigert (`latest_slot_ms` mit Returns)
     - zweiter Lauf → 0; halb verschoben → wird vervollständigt; nicht-hex verweigert; Spend-Lock gehalten → Fehler
   - `runtime_live.rs`:
     - ein WITHDRAWN-BUY gibt nach dem Tick seine Slots zurück (vorhandene Testhelfer prüfen: `a_buy_late_after_its_lease_was_taken_withdraws_both` in `live_end.rs` ~Z.2173, `live_ready_*` in `sources.rs`)
     - Betreiberfakten erreichen die laufende Runtime: resume → Exit wieder gewollt; write-off → `Ended`; exit-now OFF → `OPERATOR_EXIT_LOCKED`; ON → `exits_wanted`
     - Quarantäne verweigert BUYs alter Grants, ein neuer Grant ist frei
   - `runtime.rs`: Config `BSC_EXEC_LIVE_OPERATOR_EXIT` ON/OFF/anderes/fehlt.
2. **Rebase auf das aktuelle `origin/main`** (enthält R2b `9d5ed3a0`):
   - Überlappung nur `bsc/engine/crates/qualification/src/status/tests.rs`: R3a fügt 3 Felder in ein Fact-Literal ein, R2b fügt Tests hinzu. Nach dem Rebase prüfen, ob R2b neue Fact-Literale enthält, die ebenfalls die 3 Felder brauchen.
   - `frontend/contracts/bsc-fields.json` und `bsc-native-fields.json` hat R2b geändert; R3a hat sie noch nicht angefasst, sie werden in Schritt 4 neu erzeugt.
   - Methode wie bei R2: `git -C wtR2 stash push -m R3a` (benannt), `git -C wtR2 checkout --detach origin/main`, `git -C wtR2 stash apply` (nicht `pop`), Konflikte von Hand lösen, danach `stash drop` erst nach Sichtprüfung.
   - Vorher den Patch `wtR2_R3a_uncommitted_before_move.patch` als Sicherung behalten.
3. **fmt** nur der eigenen Hunks: `python3 -I tools/fmt_mine.py <wtR2>/bsc/engine origin/main <Datei relativ zu bsc/engine>…`.
   - Baseline: 355 Hunks (`tools/fmt_base_files.txt`); sie darf nicht wachsen.
   - Achtung: Das Skript hat einmal fälschlich einen Umordnungs-Hunk in `lib.rs` angewandt; das Ergebnis immer mit `git diff` prüfen.
4. **Re-Pin und Inventar:**
   - Gepinnte Dateien `sources.rs`, `live_ledger.rs`, `runtime_live.rs` (`bsc-fields.json`): `cd <wtR2> && python3 -I <DarkBot-bsc-rev1>/tools/repin.py`.
   - Natives Inventar neu: Ledger v4-Felder und Quarantäne-Schema. Ablauf:
     1. `cargo build --release -p darkbot-bsc-qualification --features status-reporter --bin bsc-status-reporter` (Bin und Feature in `qualification/Cargo.toml` geprüft).
     2. `DARKBOT_BSC_ROOT=<wtR2> DARKBOT_RUNDIR=<tmp-rundir> <target>/release/bsc-status-reporter --field-inventory > frontend/contracts/bsc-native-fields.json`
     3. `node frontend/scripts/check-bsc-fields.mjs --write-placement`, danach die Platzierung der neuen Felder in `bsc-fields.json` entscheiden (EXCLUDED mit Grund oder IST_CONNECTED), dann `node frontend/scripts/check-bsc-fields.mjs` → PASS.
   - Bei R2 lief das mit `DARKBOT_RUNDIR=$S/invrunR2`.
5. Status-/Frontend-Sichtbarkeit (R10-Anteil):
   - WRITTEN_OFF erscheint bereits als Positionszustand-Text im LIVE-Tab (`qualification/src/status/live.rs` projiziert `p.state.name()`).
   - Offen ist eine Warnung `LIVE_LEDGER_QUARANTINED` in der Statusprojektion; sie war bewusst zurückgestellt, um mit R2b nicht zu kollidieren.
6. Docs-Einzeiler:
   - STATUS.md: R3-Zeile, BSC3-068 (bisher „FIX_VERIFIED `a6943fe0`, Rückgabe … nicht umgesetzt“), BSC3-045, BSC3-024/041 (weiter offen bis R3b)
   - Messplan-Einzeiler
   - Bei R2 entstanden die Doku-Overrides über `scratch/rev1/r2/make_docs_R2.py`; analog vorgehen, Basis `origin/main`.
7. Mutationen: `tools/mut_r2.py` als Vorlage (Format `(ID, Datei, alt, neu, crate, --lib, Testname)`, stellt mit mtime wieder her). Eine neue Liste für R3a anlegen, z. B. `tools/mut_r3.py`.
8. **Unabhängiges Review** (1 Agent; Prompt mit Diff und Fragen zu Race, Crash, Idempotenz, 024/045-Regression), Befunde beheben, Re-Check.
9. **Gate:** `zsh tools/gate_wt.sh R3a <wtR2> <wtR2-target>` → GATE_OK, danach Commit über `tools/commit_wt.sh`, Push, CI.

### 6.3 R3b (nicht begonnen) — Soll

- **Receipt-Recovery (024/041):**
  - Für eine unaufgelöste Tx, deren Fenster vorbei ist oder deren Nonce ≠ next blockiert: Hintergrund-`eth_getTransactionReceipt` am kanonischen Endpoint, Blockhash per zweiter Quelle bestätigt (`canonical_confirm.rs`).
  - Gefunden → INCLUDED/REVERTED mit `source=RECEIPT` (Gas, Tokens aus dem Portal-Event).
  - Nicht gefunden und Konto-Nonce eines bestätigten Blocks > Tx-Nonce → REPLACED mit `source=NONCE` (by_tx unbekannt).
  - Begrenzt per Config (`BSC_CANONICAL_RECOVERY_READS_MAX` / `_RETRY_MS`, neu). Ergebnis als Fakt, danach Refold.
  - Follower-Call `Call::Receipt` im `chain_follower`.
- **Nonce-Burn gesperrt:** `bsc_runtime live nonce-burn`.
  - `BSC_LIVE_NONCE_BURN=OFF` als Default; nur bei Gate CLEAR plus Grant.
  - 0-Wert-Self-Transfer mit höherem Gas → REPLACED.
  - Nur Loopback-Tests, **nie real senden**.
- **Gateway und Frontend für die R3-Betreiberbefehle** (Audit A „CLI+gateway+LiveActions.tsx“, CAS, gesperrt): kann als R3c oder in R10 laufen.

---

## 7. R2 — vollständige Übergabe (DONE_VERIFIED, `1d29cec7`; Projektion R2b `9d5ed3a0`)

Der Commit-Text von `1d29cec7` (`git log -1 --format=%B 1d29cec7`, Kopie `scratch/rev1/r2/msgR2.txt`) ist die vollständige technische Beschreibung inklusive aller Review-Befunde. Das Wesentliche:

### 7.A Implementierungsstand (alles fertig, sofern nicht anders gesagt)

- **Versuchs-Record `l`** (`runtime_journal.rs`):
  - Layout-Byte `LIVE_RELEASE_LAYOUT = 0xA2`; das alte Layout wird mit `LIVE_RELEASE_LAYOUT_BEFORE_R2` verweigert.
  - Inhalt: Art, Trigger-Quelle, Tx, Attempt (Lease-Attempt bzw. Hash), Launch-Hash, Token, Block, `clock_id`, 8 Stufenzeiten inkl. T_select.
  - Je Adapter: Adapter, Builder (Index in `LiveAdapter::ALL` oder `NO_BUILDER`), Outcome, Boundary, Qualifikation, Set-Maske, 4 Zeiten (pre-permit, write-check, boundary, queue). `HEAD = 198`, Adapterlänge 38.
  - Antwort-Record `a` mit Zeit; `LiveRuntime.answers` puffert bei jedem Poll (Disarm verliert nichts).
  - Decoder: `live_release_of`, `live_answer_of`.
- **Ledger v3** (`live_ledger.rs`):
  - Fakt `PLACED` (FIRST/LATE/OTHER_BLOCK/NO_TOKENS, Launch- und eigener Index, BUYs dazwischen) und Feld `launcher`.
  - `Book.misplaced` = BTreeSet der Grant-Generationen mit einem PLACED ≠ FIRST, aus jedem PLACED-Fakt, auch später verwaisten.
  - **Halt:** `buy_refusal` prüft zuerst `LIVE_PLACEMENT_VIOLATED`; Exits laufen weiter; frei erst durch einen neuen Grant.
  - `Ledger::stamped()`.
- **Auswerter** `execution/src/live_attempts.rs` (~4100 Zeilen): `bsc_runtime live attempts --rundir --state --config [--manifest] [--witness-dir]`.
  - Je geschriebenem LIVE-BUY ein Record `bsc-runtime/live-attempts/<tx>.dbrec` (`darkbot.bsc.live-attempt.v1`, record_type 71). Inhalt:
    - Zustand (DROPPED nach Fensterende = unaufgelöst/UNKNOWN), Inklusion, Tokens (nur aus dem eigenen Portal-BUY), Kosten, Platzierung
    - lokale Zeiten ingress→send, select→send, send→ACK (nur auf qualifizierter Sendezeit)
    - Adapter mit Antwort
    - Marktfenster ab Launch-Block (`MARKET_WINDOW_MS = 15 000`, mit G0.8 geteilt): Trades, BUY-/SELL-Volumen, erster fremder BUY, unser Preis dagegen. Unvollständig bei fehlendem Block, verweigertem Portal-Log, nicht beendetem Fenster oder innerhalb des Reorg-Horizonts; ein Reorg-Ersatz ersetzt die Trades.
    - Komponenten-Verdikte (acquisition, canonical, timing, placement, capture, local, economics): ein FAIL macht den Versuch FAIL, PASS nur, wenn alle PASS sind.
    - Zähler mit Nenner.
  - Timing ist UNKNOWN ohne einen Producer-Timing-Witness (`g08_producer_timing`), der an das eigene G0.8-Ergebnis `<id>.g08.dbrec` gebunden ist (sha, Attempt, BUY-Tx, Launch-Tx, BUY-Blockhash, VERDICT PASS).
  - Strikte Grenze: 999 999 ns PASS, 1 000 000 ns FAIL.
- **Kampagnen-Verdikt** `bsc-runtime/live-campaign-verdict.dbrec` (record_type 73):
  - `CAMPAIGN_NOT_READY` ohne reviewtes, vollständiges Manifest dieses Builds und dieser Config, dessen Hash `bsc_runtime live campaign-precommit` vor den Versuchen festgelegt hat (`bsc-runtime/live-campaign-precommit.dbrec`, record_type 72).
  - Sonst OPEN, FAIL oder PASS; Release-Verdikt `RELEASE_EVIDENCE_PASS` oder `_NOT_READY`, nur für die Zelle des Release-Scopes. Self-Launch-Evidence gibt nie Drittlaunches frei.
  - Null Versuche ergeben nie PASS.
- **Manifest** `bsc/config/live_campaign.env`:
  - Im Repo `BSC_CAMPAIGN_STATE=PROPOSED`, alle Werte leer; Betreiberentscheidung.
  - **32 Pflichtschlüssel** in `MANIFEST_KEYS` (`live_attempts.rs` ~Z.588); die Datei hat 33 `BSC_CAMPAIGN_*`-Zeilen, also zusätzlich `BSC_CAMPAIGN_STATE`. Darunter HOST, ADAPTERS, SOURCES, SELF_LAUNCHERS, WORKLOAD, FAILURE_SCENARIOS, `ELIGIBILITY=CANONICAL_FLAP_C1_SCOPE`, PLACEMENT=FIRST_AFTER_LAUNCH, `MIN_COVERED_PPM`, `UNCOVERED=OPEN|BOUNDED`, Limits, Zellen und Minima, REVIEWER und RELEASE_SCOPE.
  - Strenge Validierung: jeder Schlüssel genau einmal, unbekannte verweigert, Zahlenbereiche.
- **Coverage:**
  - Kettenzeit je Lauf vom ersten bis zum letzten journalisierten Block.
  - Gründe: RUN_EMPTY, JOURNAL_UNREADABLE, RUN_SEGMENTS_DIFFER_FROM_MANIFEST, CANONICAL_GAP, RUN_FOLLOWED_NO_BLOCK, RUN_FOLLOWER_STOPPED, RUN_FOLLOWER_LATE, UNCOVERED_HEIGHTS.
  - Toleranz = `BSC_CANONICAL_STALL_MS` + `BSC_RUNTIME_STOP_TIMEOUT_MS`.
  - Frühere Versuche: EARLIER_ATTEMPTS_UNRESOLVED (OPEN) und EARLIER_ATTEMPTS_IN_THIS_RUN_DIRECTORY (NOT_READY).
  - Jeder Versuch braucht einen berechtigten Launch und genau die eingefrorenen Builder. Eigene Konten aus `BSC_EXEC_REFUSED_ACCOUNTS` zählen als Self-Launcher.
- **Catch-up-Records** `b`/`c`/`d`/`e` (Block, Launch, Trade im T-Layout, verweigertes Log im W-Layout): die Runtime journalisiert vor dem Fenster eines Laufs. Diese Tags liegen außerhalb jedes G0.4-Census-, Replay- und Capture-Quality-Lesers.
- **G0.8-Ergebnis v7** (`qualification/src/bin/g08_mainnet_verify.rs`, Schema `record/src/schema/g08.rs::result` v7): `market.*`-Felder und der Preisvergleich über den gemeinsamen Decoder; `MARKET_WINDOW_SECS = live_attempts::MARKET_WINDOW_MS/1000`.
- **Config:** `bsc/config/ingress.public.env` mit `BSC_CANONICAL_LOT_READ_REFUSED_MAX=3`.
- **Hot-Path:** Der Platzierungs-Halt ist ein Set-Lookup, 0,4–0,5 ns (vorher Scan 5,9–633 ns). Alles andere läuft nach dem Send oder offline.
- **Wirtschaft:** Kosten und Grenzen werden im Kampagnenverdikt geprüft (Betrag, Kosten, Fehlversuche in Folge, Skip-Rate). Realisierte PnL steht im Ledger. Eine echte wirtschaftliche Qualifikation ist EXTERNAL_BLOCKED.

### 7.B Tests, Mutationen, Format, F0-VAR (geprüfte Belege)

- Gate GR2e auf dem R2-Endbaum (`scratch/gate_GR2e.out`, 11:58–12:06):
  - Workspace **1631 passed / 0 failed / 27 ignored**
  - bsc_tools 7/7 mit festen Zählern, Clippy 0
  - **F0-VAR PASS** (3456 native Felder: IST_CONNECTED 2505, **SOLL_MISSING 112**, FUTURE_PHASE_BLOCKED 36, EXCLUDED 803; 199 Proto-Felder)
  - Ergebnis GATE_OK.
  - Die „1.453 Tests“ aus dem Auftrag sind ein älterer Zwischenstand.
- **SOLL_MISSING 112:** Das waren die neuen nativen R2-Felder (live-attempt, Kampagnen-Verdikt), die bis zur Projektion als SOLL platziert waren. **R2b hat sie angeschlossen:** Gate EB1 auf `9d5ed3a0` (`scratch/eb/gate_EB1.out`) zeigt 1633/0/27, F0-VAR PASS mit IST_CONNECTED 2617, **SOLL_MISSING 0**, 274 Proto-Felder, GATE_OK.
- **Mutationen R2:**
  - `scratch/mut_r2.out`: 18 RED, 1 GREEN (N7, danach behoben)
  - `mut_r2b.out`: 35 RED
  - `mut_r2c.out`: 45 RED, 2 PATTERN 0
  - `mut_r2d.out`: 54 RED; PATTERN 0 bei N22 (obsolet), N25 und N39 (Muster durch fmt verschoben, als N25b/N39b neu gefasst)
  - Über alle Läufe 57 verschiedene IDs RED in diesen Dateien. Der Commit nennt 59 RED (inklusive N25b/N39b). **Kein Überlebender im Endstand.**
  - Zuordnung Mutant → Test: siebte Spalte in `tools/mut_r2.py`.
- **R2b-Mutationen:** 9 RED (`scratch/eb/mut_*.log`, Skript `scratch/eb/mutEB.py`).
- **Format:** `cargo fmt --all -- --check` auf `9d5ed3a0` gibt Exit 1 mit **355 Hunks** in 38 Dateien. Das entspricht exakt der Baseline vor R2 (`tools/fmt_base_files.txt`, Stand `f6156fa0`); R2 und R2b haben **keine** neuen Format-Abweichungen eingebracht. Die „368“ war der Stand vor `fmt_mine`. Die repo-weite Bereinigung gehört zu R14 (CI-Schritt fmt/clippy fehlt); wegen der eingefrorenen G0.7-Hashes nicht paketweit formatieren.
- **Review R2:** Runden FAIL (H1–H3, M1–M5, L1–L11) → FAIL (N1–N3) → FAIL (Coverage = gefolgte Kettenzeit) → FAIL (Catch-up `d`/`e`) → **PASS**. Details im Commit-Text.
- **Review R2b: offen** (REVIEW_PENDING).

### 7.C Merge mit E-A (erledigt)

R2 wurde vor dem Commit auf `f6156fa0` rebased (stash, checkout, apply). Die überlappenden Dateien `execution/src/lib.rs`, `qualification/src/status/tests.rs` und `frontend/contracts/bsc-fields.json` ließen sich sauber mergen. Danach liefen GR2e (Gate) und CI grün auf `1d29cec7`.
- `lib.rs`: E-A entfernte `pub mod startup`, R2 fügte `pub mod live_attempts` hinzu. Beide Änderungen sind enthalten.
- `bsc-fields.json`: Re-Pins beider Seiten und die neuen R2-Felder.

Hier ist nichts mehr offen.

### 7.D Nächster Schritt zu R2

Kein Code-Schritt. Offen ist (1) das **unabhängige Review von R2b `9d5ed3a0`** und (2) die realen Läufe nach 03.11. (EXTERNAL_BLOCKED):
- Kampagnen-Manifest durch den Betreiber
- `live campaign-precommit`
- Läufe
- `live attempts`

Review-Fokus R2b:
- `qualification/src/status/campaign.rs` liest nur und bewertet nichts
- Verhalten bei beschädigten Records
- Begrenzung auf `MAX_BUY_ATTEMPTS_TOTAL`
- Frontend `pages/LiveCampaign.tsx` und `model/live.ts`
- Proto-Felder 30/31 und Golden-Dateien

Ein Befund wird als Folge-Commit behoben.

---

## 6a. R1 — Inhalt und Folgepunkte (DONE_VERIFIED `0159c1a7`)

- **BSC3-080:** Strategy C entscheidet die Exits jedes Grants. Eigene Strategy-C-BUYs nur mit STRATEGY_C im Grant, sonst `LIVE_NOT_IN_SCOPE` bzw. Zähler `STRATEGY_BUYS_NOT_GRANTED`. Ein LIVE-Arm ohne STRATEGY_C-Senderpfad wird verweigert.
- **BSC3-081:** Nur effektiv LIVE geschaltete Grant-Quellen lösen einen LIVE-BUY aus (`SOURCE_NOT_LIVE` gezählt).
- **BSC3-082:** Offene Lots nach Restart, Gap oder Reorg:
  - Die Kurve wird blockgebunden gelesen (getTokenV8Safe, nftGate) und das Lot erneut gehalten, solange nicht überholt (journalisiert `k`).
  - Bis dahin gilt EXIT_FIRST. Off-curve, unpriced oder unlesbar → gestrandet, kein neuer BUY (`runtime.rs` `LotDeciders`, `hold_read_lots`, `ask_lot_reads`, `refresh_lot_deciders`).
- **BSC3-083:** `live bound-inputs`; `bsc/tools/status_sync.py` kopiert genau die gebundenen Qualifikationsdateien in den Snapshot.
- **BSC3-084:** `source init` legt nur an, `source reset` beim Stopp; `live idle`; Deploy verweigert bei aktivem LIVE ohne `--stop-live`.
- **Vier LOW-Folgepunkte aus dem R1-Re-Check, alle in R2 `1d29cec7` umgesetzt und getestet:**
  1. Überholte Lesung wird sofort erneut gefragt, kein Backoff (`runtime.rs hold_read_lots`, Zweig `Ok(_) if r.overtaken`; Mutant N7 RED).
  2. Ein verweigerter Read strandet erst beim `BSC_CANONICAL_LOT_READ_REFUSED_MAX`-ten Mal in Folge (3; `refused_in_a_row`; Mutanten N8/N33 RED; Test `refused_answers_count_only_in_a_row`).
  3. `Engine::holds` verlangt `priced().is_ok()` (`trading/src/strategy/engine.rs`; Mutant N6 RED; Test `a_held_position_without_its_terms_is_not_held`).
  4. Creator-Sale, während ein Lot auf erneutes Halten wartet, bleibt erhalten und zählt beim Halten (Mutant N5 RED; Test `a_creator_sale_while_a_position_waits_to_be_held_again_still_counts`). Restrisiko ist nur noch, dass ein Verkauf die Lesung überholt; das wird erkannt und erneut gelesen.
- **Folgearbeiten aus R1:**
  - Betreiber-Auflösung gestrandeter Lots (exit-now, write-off): **R3a, in Arbeit**
  - DEX-Exit migrierter Lots: **R5**

---

## 8. Audit E — Befundmatrix (Pflichtstatus aus STATUS.md; Abschluss geprüft)

Paketschlüssel: E1 Texte, E2 Totcode/Platzhalter, E3 Konfiguration, E4 Projektion/Frontend, E5 Hotpath, E6 Wire, E7 Executor-Planung, E8 Readiness-Job, E9 RLP, E10 Refreeze, E11 Modulschnitt.

| ID | Inhalt (kurz) | Pflichtstatus/Paket | Stand |
|---|---|---|---|
| 1.1 | Overview `live_authorized` konstant | FIX/E4 | DONE `8a9584d9` |
| 1.2 | Placeholder-Topic `bsc.execution.events` | FIX/E2 | DONE `8a9584d9` (40 + Name reserviert) |
| 1.3 | Wallet-Status aus `wallet.public.env` | FIX+REPIN/E4 | DONE `8a9584d9`. **Rest:** `bsc/tools/status_sync.py` kopiert `wallet.public.env` noch, obwohl der Reporter sie nicht mehr liest → in E2/E3 entfernen (Test `test_status_sync.py` anpassen) |
| 1.4 | Projektions-Fallback `working-tree`/0 | FIX+REPIN/E4 | DONE `8a9584d9` |
| 1.5 | LOOPBACK-Stand-in | KEEP (BSC3-026) | geschlossen als KEEP |
| 1.6 | Grant bietet Quellen ohne Adapter | FIX/E4 | DONE `8a9584d9` |
| 1.7 | produktive Gas-Evidence unter `trading/fixtures` | FIX+REFREEZE/E10 | OFFEN |
| 2.1/2.2 | falsche „read-only/no tx“-Doku/Usage: `runtime.rs` 1–5, `lib.rs`, `bsc_runtime.rs` 1–2/56–58, `Cargo.toml` 3/139 | FIX/E1 | OFFEN. **Rest aus E-A:** auch `submission/send.rs`-Kopf (STATUS 2.18), `trading/Cargo.toml` und die Intent-Doku; die `bsc_runtime`-Usage enthält inzwischen die `live`-Befehle (R2/R3a) |
| 2.3–2.5 | proto/Gateway/bsc-control-Texte | FIX+REPIN/E1 | DONE `8a9584d9` |
| 2.6–2.8 | Frontend-Hinweise, bsc.spec.ts, bsc-fields decisions | FIX/E1/E2 | DONE `8a9584d9`…`f6156fa0` (Browser-Spec hermetisch, CI-Job bsc/frontend) |
| 2.9–2.11 | tote Tools, C1-JSON-Tools | FIX/E2 | DONE `8a9584d9` (`c14a_*` KEEP) |
| 2.12 | Q5A-TLS-Evidence JSON | FIX (DBREC-Writer)/E10 | OFFEN. Writer gehört zu R8; Requalifikation braucht BSC_ECON_SEND = Betreiber, also EXTERNAL_BLOCKED |
| 2.13 | `legacy_import` | KEEP | geschlossen |
| 2.14/5.9 | toter Code in eingefrorenen G0.7-Dateien | FIX+REFREEZE/E10 | OFFEN (Refreeze PROBE_SOURCES/PLAN_SHA256) |
| 2.15 | LIVE-Qualifikation V1–V3-Leser | FIX+REPIN | DONE (R1 `0159c1a7`) |
| 2.16 | Owner-Generation V1–V5-Decoder | KEEP | geschlossen |
| 2.17 | `startup.rs`-Doku | FIX/E2 | DONE `8a9584d9` (Modul entfernt) |
| 2.18 | Submission/TLS-Doku | FIX/E1 | TEILWEISE: `mod.rs` und `tls/config.rs` DONE `8a9584d9`; **`send.rs`-Kopf OFFEN** |
| 2.19 | `flap_gas_20261026.txt` | KEEP (mit 1.7 verschieben) | offen mit 1.7 |
| 2.20 | `__pycache__`; `runtime_live.rs.orig` | FIX/KEEP | geschlossen (`.orig` nie anfassen) |
| 3.1 | Kampagnen-Maximum 10 dreifach (`sources.rs` ~404, `gateway/src/control.rs` ~95, `frontend live.ts` ~13) → Config `BSC_BUY_ATTEMPTS_TOTAL_MAX` projiziert | FIX+REPIN/E3 | OFFEN |
| 3.2 | `PARITY_MIN_EXECUTED_PER_PATH=30` → Kriterien-Config | FIX+REPIN/E3 | OFFEN |
| 3.3 | Stale-Faktor 3 doppelt (`sources.rs` `STATUS_STALE_AFTER_INTERVALS`, `sources.ts` ~33) | FIX+REPIN/E3 | OFFEN |
| 3.4 | `KEEP_GENERATIONS=64` → Config | FIX+REPIN/E3 | OFFEN |
| 3.5/3.6 | Socket/Label, launchd-Zahlen | FIX+REPIN | DONE `8a9584d9` |
| 3.7/3.8 | Gateway-/Frontend-Timeouts | FIX | DONE `8a9584d9` (**Rest aus E-A:** Timeout-Literale der P0-Clients im Frontend gehören zu Project_0, **nicht BSC**) |
| 3.9–3.11, 3.13 | Q5A-Hosts, Deployment-Pins, Owner-Pfad, Protokollkonstanten | KEEP | geschlossen |
| 3.12 | abgelehnte Konten doppelt (Config vs. acceptance_contract) → Prüfung beim Start | FIX/E3 | OFFEN |
| 4.1/4.14/5.1 | `startup.rs` toter Parallelpfad, SecretSources | FIX/E2 | DONE `8a9584d9` |
| 4.2 | PaperEnd/LiveEnd-Planung und -Frames doppelt | FIX/E7 (Hot-Path, messen) | OFFEN |
| 4.3–4.5, 5.14 | JSON-RPC-Hex/Rate-Limit-Module, HTTP-Framer (follower `http.rs` vs. submission `response.rs`), rustls-Builder (wss vs. tls), https-Reparse als wss | FIX/E6 | OFFEN |
| 4.6/4.7 | G0.7/G0.6a-Eigenpfade | KEEP | geschlossen |
| 4.8 | Grant-Validierung dreifach; Scope-Text-Parität Rust/TS nur einseitig → Rust-Golden-Vektor | FIX+REPIN/E4 | OFFEN |
| 4.9 | ID-Listen 3–4-fach → Projektion | FIX+REPIN/E4 | OFFEN |
| 4.10 | drei LIVE-Indikatoren | FIX/E4 | DONE `8a9584d9` |
| 4.11 | `status.dbrec` doppelt projiziert (NativeRecord-Blätter + typisiert) | FIX/E4 | OFFEN |
| 4.12 | drei Frontend-Kommandoclients | FIX/E4 | DONE `8a9584d9` (`bscControl.ts`) |
| 4.13 | Readiness-Job py+sh+rust key=value → nativer Binär-Owner | FIX+REPIN/E8 | OFFEN |
| 4.15/5.5 | zwei secp256k1-Recovery-Pfade (Lane k256 vs. libsecp); k256-Eigensignatur-Recovery je Intent | FIX gemessen/E5 | OFFEN (Hinweis: libsecp256k1 als neue Abhängigkeit war am 30.10. „nicht genehmigt“, siehe Memory `bsc-completion-order-20261030`; vor der Konsolidierung klären, welche Bibliothek bereits genehmigt im Baum ist) |
| 4.16 | drei RLP-Decoder → `tx_encoding` | FIX/E9 (C1-Freeze prüfen) | OFFEN |
| 5.2 | Hot-Path: dreifacher Envelope-Decode und doppelter Flap-Decode | FIX gemessen/E5 | OFFEN |
| 5.3 | classify vor dedup | FIX gemessen/E5 | OFFEN |
| 5.4 | C2-pending()/Builder-Polling vor LIVE-BUY | FIX gemessen/E5 | OFFEN |
| 5.6 | `awaiting_granted` VecDeque mit linearer Suche (Bound 262144) → HashSet + Ring | FIX gemessen/E5 | OFFEN |
| 5.7 | `egress.rs` nur für Tests → Feature | FIX/E2 | OFFEN |
| 5.8 | `pub fn` nur mit Test-Aufrufern | FIX/E2 | OFFEN |
| 5.10/5.11 | Store-Reste, BACKEND_UNAVAILABLE dreifach | FIX/E2 | DONE `8a9584d9` |
| 5.12 | verwaiste Doku | FIX/E2 | OFFEN |
| 5.13 | Gott-Module | FIX+REPIN/E11 | OFFEN (schwerer Re-Pin; spät) |

Fehlende Prozess-, Browser- und Integrationsprüfungen (Kategorie aus dem Auftrag):
- Der reale Browser-/Gateway-/WSS-Prozess-E2E ist **offen** (R11). Der E-A-Browser-Test ist hermetisch (statischer Fixture-Server).
- Ein E2E gegen echte Hosts ist wegen der verbotenen Hosts BLOCKED.
- Die lokalen Tests des E-A-Agenten erreichten einmal das Gateway des Nutzers, nur lesend.

---

## 9. Produktiver Daten- und Steuerfluss (Autorität, Datei, Persistenz)

Der Pfadpräfix für Rust ist `bsc/engine/crates/`. „UNVERIFIZIERT“ heißt: nicht in dieser Sitzung im Code nachgesehen.

1. **Quellen → Ingress:**
   - WSS-Adapter `execution/src/wss/` (u. a. `BSC_MAINNET_CHAIN_ID`), gestartet in `execution/src/runtime.rs` `spawn_adapter`.
   - Ein Kanal (`sync_channel`; per-source Queues sind R6).
   - Quellen-Modi und Owner: `execution/src/sources.rs` (`Store`, `SourceId`, `Mode`, `Effective`, Gate `live_effective`/`live_clear`).
   - Persistent: `bsc-sources/generations/*` (Owner-Generationen, CAS über Request-ID und `--expect-generation`).
   - Restart: `source reset` beim Stopp, `source init` legt nur an (R1).
2. **Kanonische Validierung:**
   - Follower `execution/src/chain_follower/`.
   - Zweite Quelle `execution/src/canonical_confirm.rs` (BSC3-056).
   - C2-Zustand `execution/src/runtime_state.rs` (StateOwner, `clean_next_nonce`).
   - Ereignisse `CanonicalEvent` (Block, FlapTrade, Reorg, Gap …).
   - Fail-closed: Stall → STALE_HEAD; Gap/Reorg → Lots erneut lesen.
   - Header-Hash/Roots (056) sind **nicht** verifiziert → R7.
3. **Strategie:**
   - Strategy C: `execution/src/runtime_strategy.rs` + `trading/src/strategy/engine.rs` (Exit-Regeln, `holds`, `reheld`, `unhold`; Eingaben journalisiert, Replay `strategy-replay`).
   - C3 (Launch-BUY) über `execution/src/runtime_exec.rs` (`launch_to`).
3a. **Economics:** `trading/src/economics/` (`policy.rs`, `flap/native_curve20`; Slippage BUY/Exit getrennt per `BSC_EXEC_SLIPPAGE_BPS`/`BSC_EXEC_EXIT_SLIPPAGE_BPS`). Der Grant begrenzt Risiko und Gas (`GrantRisk`, `GrantLimits` in `sources.rs`).
4. **Owner-Admission (Autorität):**
   - `sources.rs` `Store::admit_live`, `lease_live` (BSC3-076), `return_leases`, neu `return_attempt` (R3a).
   - Persistent:
     - `buy-attempts/<UTC-Tag>/<slot>.dbrec` (record 64)
     - `buy-attempts-total/<slot>.dbrec` (65)
     - `live-spend/<id>.dbrec` (67; Write-ahead BSC3-024, Durability Handed und flush)
     - `live-lease-returns/`
     - neu `buy-attempt-returns/`
   - Lock `live-spend.lock`. Limits: `BSC_BUY_ATTEMPTS_PER_DAY`/`…TOTAL` (Kampagne max. 10, inkl. G0.7).
5. **Wallet/Signatur:**
   - LIVE-Buyer-Key-Datei (`BSC_LIVE_KEY`, 32 Byte außerhalb des Repos), Adresse `BSC_LIVE_BUYER`.
   - Schalter `BSC_WALLET_BUYER`/`LAUNCHER` und `BSC_ECON_SIGN|ARM|SEND|ATTEMPT|LIVE` (alle DISABLED, nur in ihrer Boot-Session).
   - Signatur in `execution/src/live_end.rs` / Lane (`paper_exec.rs`). Details UNVERIFIZIERT.
6. **Builder/Transport:**
   - `execution/src/submission/` (`send.rs` `dispatch_release` mit `TransportStartDeadline`, `club48.rs`, `blockrazor.rs`, `frame.rs`, `tls/`).
   - Q5A-TLS-Qualifikation; LIVE-Qualifikation `execution/src/live_qualification.rs` (Record `live_qualification`).
   - Fail-closed: Deadline vor der Admission und vor dem Write erneut geprüft (BSC3-068); nicht offengelegt → WITHDRAWN.
7. **Receipt/Ledger/Position/Exit/PnL:**
   - `execution/src/live_ledger.rs` (`live-ledger/<seq>.dbrec`, record 68, Append per Hardlink, nie überschrieben; Fold → Positionen, Gas, PnL).
   - `runtime_live.rs` `on_canonical` schreibt INCLUDED/REVERTED/REPLACED/ORPHANED/PLACED; Exits und Re-Exits über `exits_wanted`, `exit_stopped`, `BSC_EXEC_LIVE_EXIT_REVERTS_MAX`.
   - Restart: `recover()` macht aus Admissions ohne Fakt UNKNOWN; Fences `live/` werden settled.
8. **Status/Protobuf/Gateway/Frontend:**
   - Reporter `qualification/src/status/` (`live.rs`, `projection.rs`, `campaign.rs`), Binary `bsc-status-reporter` (Feature `status-reporter`).
   - `protocol/schemas/darkbot/bsc/status.proto` (BscLiveState inkl. campaign 30 und attempts 31).
   - Gateway `gateway/src/` (`control.rs`, `topics.rs`, `overview.rs`).
   - Frontend `frontend/src/features/bsc/` (LIVE-Tab, `LiveActions.tsx`, `LiveCampaign.tsx`); Feldvertrag `frontend/contracts/bsc-fields.json` + `bsc-native-fields.json`, Prüfung `frontend/scripts/check-bsc-fields.mjs` (F0-VAR).
   - Ops: `ops/bsc-control.sh`, `ops/darkbotctl`, `bsc/tools/status_sync.py`.

Evidence- und Record-Schemas: `record/src/schema/bsc_runtime.rs` (Liste `ALL`) und `record/src/schema/g08.rs`:
- chain_session 10, chain_block 11, chain_transaction, chain_receipt, result 14 v7, unresolved, producer_witness, timing_verdict
- record_types 64/65/67/68/71/72/73 (+74 lokal R3a)

Deployment-/ABI-Pins: Flap-Deployment-Konstanten (KEEP, Audit 3.10), G0.7-Freeze (acceptance_guard, `g07_builder_probe` PROBE_SOURCES, economics BOUND_FILES), Pins in `bsc-fields.json`.

Integrations-Tests (Auswahl, nachgewiesen grün im Gate): `execution/tests/live_e2e.rs`, `runtime_live.rs`-Tests (z. B. `a_live_release_is_journaled_and_evaluated_as_its_attempt`), `bsc_tools` (7 Tool-Tests), `frontend` `bsc.spec.ts`.

---

## 10. Entscheidungen, Freigaben, Risiken, offene Fragen

- **Nicht erteilt:** jede SIGN/ARM/SEND/BUY/SELL-Freigabe, der Echtgeldtest, das Armieren des G0.7-Guards, `BSC_ECON_SEND` für die Q5A-Requalifikation.
- **Fünf Betreiberentscheidungen (offen; Code bekommt Varianten und gesperrte Defaults):**
  - G0.4-Kriterien bzw. 10-Minuten-Cap
  - BSC3-001 Vertrauensanker
  - BSC3-030 Mempool-Exit/geth
  - BSC3-042 Re-Arm je Versuch
  - BSC3-069 Outlier-Schwellen
- **Kampagnen-Manifest** `bsc/config/live_campaign.env`: Werte setzt der Betreiber (PROPOSED). Zellen, Release-Scope, Self-Launcher und UNCOVERED=OPEN|BOUNDED sind Betreiberentscheidungen.
- **Neu R3a:** `BSC_EXEC_LIVE_OPERATOR_EXIT=OFF` (Default gesperrt; einschalten ist Betreibersache).
- **Wiederkehrende Fehlerursachen:**
  - F0-VAR-Pins nach späten Edits → am Ende erneut re-pinnen
  - Mutationsmuster driften nach rustfmt
  - `fmt_mine.py` mit relativen Pfaden
  - Gates, die durch neue Änderungen überholt sind
  - zsh-Wortaufteilung
  - Commit nie an `| grep` koppeln
  - Builds nie aus einem git-archive-Export in ein gemeinsames Target
- **Bekannte Restrisiken:**
  - Repo-weites fmt ist rot (R14).
  - 056 ohne Header-/Root-Prüfung (R7).
  - Ein Lease-Versuch, der vor einem Neustart ungenutzt zurückgegeben wurde, behält seinen Slot (BSC3-076, konservativ). Dasselbe gilt für einen WITHDRAWN-Lease-BUY aus `v3`-Fakten ohne `attempt`.

---

## 11. Build-, Test- und Gate-Umgebung

- Toolchain: `bsc/engine/rust-toolchain.toml` → **1.93.1** (rustfmt, clippy), rustc 1.93.1 (Homebrew), node v24.1.0. Workspace `bsc/engine/Cargo.toml`.
- Gateway: CI läuft mit stable 1.99; bei Gateway-Edits `cargo fmt` und `clippy` auf stable.
- Tests:
  - Workspace: `cargo test --locked --no-fail-fast --workspace --all-features -j 2`
  - Tools: `cargo test --locked --all-features -p darkbot-bsc-execution --test bsc_tools` und `-p darkbot-bsc-qualification --test bsc_tools`
  - Clippy: `cargo clippy --workspace --all-features --all-targets -- -D warnings`
  - F0-VAR: `node frontend/scripts/check-bsc-fields.mjs`
  - Frontend: `npm run test:bsc`
- Testdisziplin: 1–3 gezielte Tests je Änderung, höchstens 5 Testaufrufe je Schritt, `-j 2`; die volle Regression nur am Paketende.
- CI `.github/workflows/bsc.yml`:
  - Job `regression` (macOS und ubuntu): cargo regression all features, Release-Journal-E2E, Release-LIVE-Start-Limit-Tests, sieben Tool-Tests mit festen Zählern
  - Job `frontend`: `npm ci`, build, playwright, `npm run test:bsc`
  - Workflow `gateway`
  - Project-0-Workflow „Project 0 status contract“
- **Werkzeuge** in `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/tools/` (Pfade schon auf `…/DarkBot-bsc-rev1/scratch` umgeschrieben):

  | Werkzeug | Aufruf / Zweck |
  |---|---|
  | `commit_wt.sh` | `bash commit_wt.sh <worktree> <since-rev> <msg-file> <list-file> [repo/pfad=inhaltsdatei …]`. Committet genau die gelisteten Pfade auf `origin/main` (temporärer Index, Allowlist, Sperrliste der lokalen Evidence, verweigert bei Upstream-Änderung) und druckt `COMMIT=<sha>`. Danach `git -C <worktree> push origin <sha>:refs/heads/main`. |
  | `gate_wt.sh` | `zsh gate_wt.sh <tag> <worktree> <target>`: Workspace, bsc_tools mit den Zählern der 7 Dateien, Clippy, F0-VAR, Baum-Hash vorher/nachher → `GATE_OK`. Logs `scratch/{ws,tools,clippy,f0var}_<tag>.log`. |
  | `fmt_mine.py` | `python3 -I fmt_mine.py <worktree>/bsc/engine <base-rev> <dateien relativ zu bsc/engine>`. Wendet nur rustfmt-Hunks an, die eigene Zeilen berühren. |
  | `fmt_base_files.txt` | Baseline: 355 Hunks in 38 Dateien. |
  | `repin.py` | Im Worktree-Root ausführen: `python3 -I …/tools/repin.py`. Re-pinnt `source_sha` in `frontend/contracts/bsc-fields.json`, aber nur für Dateien, die gegenüber `origin/main` geändert sind; meldet sonst `UNCHANGED-BUT-STALE`. |
  | `ci_wait2.py` | `python3 -I ci_wait2.py <sha>…`. Wartet bis zu 3 h auf die CI genau dieser SHAs (öffentliche API; als Hintergrundjob laufen lassen) und druckt `<sha>|<workflow>|<conclusion>|<url>`, am Ende `DONE`. |
  | `mut_r2.py` | `python3 -I mut_r2.py <worktree> [IDs]`. Mutationsläufer (Liste `M`: ID, Datei, alt, neu, crate, Ziel, Test), stellt jede Datei mit frischem mtime wieder her, Target `<worktree>-target`. Vorlage für R3. |

- **Notizen** in `…/DarkBot-bsc-rev1/scratch/`:
  - `rev1/PLAN.md` (Paketplan)
  - `rev1/audit{A,B,C,D}_summary.md`, `rev1/auditE_cleanup.md`, `rev1/auditE_register.md`
  - `rev1/r3/DESIGN.md`
  - `rev1/r2/` (Diffs, Doku-Generator `make_docs_R2.py`, `msgR2.txt`)
  - `rev1/msgR1.txt`, `rev1/reviewR1.md`
  - `eb/` (R2b-Agent: Gate, CI, Mutationen, `msg.txt`)
  - `ea/` (E-A-Agent)
  - Gate-, CI- und Mutationslogs `gate_GR1*.out`, `gate_GR2*.out`, `ci_R1.out`, `ci_R2.out`, `mut_r2*.out`, `fmt_9d5ed3a0.txt`
- **Natives Feldinventar:**
  1. Reporter im Release-Modus bauen (`--features status-reporter`).
  2. `DARKBOT_BSC_ROOT=<worktree> DARKBOT_RUNDIR=<leeres tmp> …/bsc-status-reporter --field-inventory > frontend/contracts/bsc-native-fields.json`.
  3. `node frontend/scripts/check-bsc-fields.mjs --write-placement`, dann ohne Flag prüfen.
- **Umgebungsfallen:**
  - Ein Worktree braucht den Symlink `bsc/engine/target` und `node_modules` (per `cp -c -R` aus dem Hauptcheckout klonen, beide ignoriert), sonst scheitern die bsc_tools-Tests.
  - Nie in das Target des Hauptcheckouts bauen.
  - Nur ~18 GB frei.
  - `gh` ist nicht installiert.
  - Der Hauptcheckout ist der gemeinsame BSC-Checkout: nie pullen und nie den Index benutzen; immer `commit_wt.sh`.

---

## 12. Weitere Hinweise für die Nachfolge

1. Persönliche Memory-Dateien (falls auf dieser Maschine lesbar): `~/.claude/projects/-Users-sawyer-ma-Desktop-Firma-2-DarkMa-6-DarkBot-DarkBot/memory/`, insbesondere `bsc-order-20261101-rev1-complete.md`, `bsc-absolute-boundary.md`, `feedback-*.md`. Diese Übergabe ist auch ohne sie vollständig.
2. Der Übergabeauftrag nennt BSC3-080..084 als R1-IDs: stimmt (STATUS.md Zeilen BSC3-080..084).
3. Die E-A-Reste aus dem Agentenbericht sind in §8 zugeordnet:
   - `send.rs`-Doku → E1 (2.18 Rest)
   - `wallet.public.env`-Kopie → 1.3 Rest (E2/E3)
   - P0-Client-Timeouts → Project_0, nicht BSC
   - Prozess-/Browser-E2E → R11
4. Auftrag des R8-Agenten (zur Wiederverwendung, falls R8 erneut delegiert wird): R8-Punkte 1–6 wie in §5:
   - 049 Evidence-Map und `LIVE_QUALIFICATION_STALE` per Config
   - 066 Profilbindung
   - 064 OBSERVABLE und G0.4-10-min-Variante (PROPOSED, Default unverändert)
   - 018 Directory-fsync in `qualification/src/c1_common.rs` und `g06a_preflight_record.rs` (~Z.88) mit Re-Pin
   - Q5A-DBREC-Writer/-Leser (historisches JSON lesbar und gepinnt lassen)
   - `ops/release_manifest.py` um g07-Tools, `g08_mainnet_verify`, `dbrec_show`, `status_sync.py` und Configs ergänzen
   - 035 überspringen (Netz)

   Der Agent darf nicht selbst committen; er meldet READY_FOR_REVIEW, die Hauptinstanz reviewt.

---

## EXAKTER WIEDERAUFNAHMEPUNKT – ANWEISUNG AN DIE NÄCHSTE CLAUDE-INSTANZ

Letzter abgeschlossener Schritt:
- R3a-Code (Ledger v4, Slot-Rückgabe 068, Betreiberfakten resume/write-off/exit-now, adopt-g07, Ledger check/quarantine/Temp-Cleanup 045, CLI, Runtime-Verdrahtung, Config) ist geschrieben.
- `cargo check -q --workspace --all-features --tests --bins` war grün (vor dem Verschieben, Target `wtR2-target`).

Unmittelbar nächster Schritt:
- `bsc/engine/crates/execution/src/live_ledger.rs`, Modul `mod tests` (beginnt bei `#[cfg(test)] mod tests`, Helfer `released(...)` und `admitted(...)`): die R3a-Ledger-Tests aus §6.2 Punkt 1 schreiben.
- Danach die Tests für `sources.rs` (`return_attempt`), `runtime_live.rs` und `runtime.rs` (Config).

Reihenfolge:

1. **Orte prüfen**, ohne etwas zu ändern:
   ```
   R=/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot
   D=/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1
   git -C $R fetch -q origin; git -C $R worktree list
   git -C $D/wtR2 status --porcelain=v1        # die 15 R3a-Dateien aus §3.4
   git -C $D/wtR2 diff | shasum -a 256          # fab0ae28… = unveränderter R3a-Stand
   git -C $R log --oneline 0159c1a7^..origin/main -- bsc frontend gateway ops protocol
   ```
2. Diese Datei vollständig lesen; dann `bsc/docs/1_PLAN/PHASENPLAN.md`, `STATUS.md` (REV.1.0-Abschnitt und Audit-E-Tabelle), den Messplan, `bsc/docs/5_OFFENE_FEHLER/BSC_Bug_V3.md` (BSC3-024, -041, -045, -068) und `$D/scratch/rev1/{PLAN.md,auditC_summary.md,auditA_summary.md,auditE_register.md,r3/DESIGN.md}`.
3. Abschlüsse gegen Git prüfen: `git -C $R log -1 --format=%B 0159c1a7`, `… 8a9584d9`, `… 1d29cec7`, `… 9d5ed3a0`. CI über `python3 -I $D/tools/ci_wait2.py <sha>` als Hintergrundjob.
4. **R2b-Review** (unabhängiger Reviewer-Agent, höchstens 2 Agenten) auf `9d5ed3a0` starten; Fokus siehe §7.D. Befunde als Folge-Commit beheben.
5. **R3a fortsetzen in `$D/wtR2`:**
   1. `ln -sfn $D/wtR2-target $D/wtR2/bsc/engine/target` prüfen; ist `node_modules` vorhanden, ist es ignoriert.
   2. Tests schreiben (§6.2 Punkt 1) und gezielt laufen lassen:
      `cd $D/wtR2/bsc/engine && CARGO_TARGET_DIR=$D/wtR2-target cargo test --locked --all-features -p darkbot-bsc-execution --lib <testname> -j 2`
   3. Rebase auf das aktuelle `origin/main` (§6.2 Punkt 2), `stash apply`, nicht `pop`.
   4. fmt der eigenen Hunks, Re-Pin, natives Inventar, F0-VAR-Platzierung (§6.2 Punkte 3–4).
   5. Doku-Einzeiler (STATUS-Zeilen R3, BSC3-068/045; Messplan).
   6. Mutationen (`tools/mut_r2.py` als Vorlage) und unabhängiges Review mit Re-Check.
   7. `zsh $D/tools/gate_wt.sh R3a $D/wtR2 $D/wtR2-target` → GATE_OK.
   8. `bash $D/tools/commit_wt.sh $D/wtR2 <origin/main-sha> <msg> <liste> [doc-overrides]`, dann `git -C $D/wtR2 push origin <sha>:refs/heads/main`, dann `python3 -I $D/tools/ci_wait2.py <sha>`.
6. **R3b** (§6.3): Receipt-Recovery mit zweiter Quelle, Nonce-Burn gesperrt, danach Gateway/Frontend-Aktionen für die R3-Befehle. Gleicher Paketablauf.
7. Danach R4 bis R13 gemäß §5 und die offenen Audit-E-Befunde gemäß §8 (Reihenfolge des Paketplans; maximal zwei Agenten; jedes Paket mit Review, Mutationen, Gate, Push, CI).
8. Nach 03.11. 08:05Z (Kampagnenende, nur der Nutzer stoppt die Kampagne): die Läufe aus Audit B/R14. Am Ende R14 mit Gesamtabnahme und der vollständigen Anforderungs-/Abnahmematrix.

Nicht tun: R3a verwerfen, `wtR2` neu auschecken, den Hauptcheckout pullen oder resetten, eine Echtgeld-Aktion, paketweites `cargo fmt`, die acht lokalen Evidence-Pfade committen, Project_0 berühren.
