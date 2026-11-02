# BSC REV.1.0 — Fortsetzungs-Übergabe (technische Momentaufnahme 02.11.2026, 08:20 CET; ersetzt den Stand vom 01.11. 20:20)

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

1. **Auf `origin/main` (`49bcd98f`) liegen alle bisher fertigen BSC-Pakete:**

   | Paket | Commit(s) |
   |---|---|
   | R1 | `0159c1a7` |
   | E-A | `8a9584d9` … `f6156fa0` |
   | R2 | `1d29cec7` |
   | R2b | `9d5ed3a0` |
   | Übergabe-Datei (alte Fassung) | `4fbf4ddb` |
   | BSC-Golden-Schema-ID nach dem Project_0-Push `887a31bf` | `738a2cc7` |
   | **R3a** | **`49bcd98f`** |

   - CI grün bestätigt: R1, E-A, R2, R2b.
   - **CI für `738a2cc7`, `49bcd98f` und `4fbf4ddb`: nicht bestätigt (CI_PENDING).** Die Warter wurden beim Stopp beendet; prüfen mit `python3 -I <tools>/ci_wait2.py <sha>` als Hintergrundjob.
2. **Unfertig, uncommitted:** der **Fix der R2b-Review-Befunde**.
   - Er liegt im einzigen verbleibenden BSC-Worktree `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/wtEA`: HEAD `49bcd98f`, 18 Dateien geändert.
   - Der Agent wurde beim Stopp mitten im Übertrag auf `49bcd98f` angehalten: nach `stash apply` und Regenerieren, vor dem Gate.
   - Sichere Ausgangsfassung desselben Fixes vor dem Übertrag: **Stash `stash@{0}` „R2bfix-L1L4-20261101“** (Basis `047eafe7`).
   - Beides liegt zusätzlich als Patch in `DarkBot-bsc-rev1/backups/` (siehe §1).
3. **Aufgeräumt am 02.11.:**
   - Die überflüssigen Worktrees `wtR2` und `wtR8` sind mit `git worktree remove` ohne force entfernt, ihre Build-Caches gelöscht (etwa 21 GB frei).
   - Die drei überholten R3a-Stashes sind gelöscht; ihr Inhalt liegt auf `main` und als Patch in `backups/`.
   - Der Hauptcheckout ist per Fast-forward auf `49bcd98f` gebracht.
   - Kein Agent und kein eigener Hintergrundjob läuft.
4. **Nächster Schritt:** den R2b-Fix in `wtEA` fertigstellen, siehe den letzten Abschnitt. Danach R3b, dann R4 bis R14 und die offenen Audit-E-Befunde.

---

## 1. Orte (Repository, Worktrees, Werkzeuge)

| Was | Pfad | Stand |
|---|---|---|
| Hauptcheckout (BSC, mit Project_0 geteilt) | `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot` | `main` = `origin/main` = `49bcd98f`; nur die acht lokalen untracked Evidence-Pfade (§3.1); **nicht darin arbeiten, nichts stagen** |
| **Einziger BSC-Worktree** | `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1/wtEA` | detached `49bcd98f`, R2b-Review-Fix uncommitted (18 Dateien, siehe §7.D) |
| Build-Target dazu | `…/DarkBot-bsc-rev1/wtEA-target` | Symlink `wtEA/bsc/engine/target` zeigt darauf; `frontend/node_modules` und `bsc/tools/*/node_modules` in `wtEA` vorhanden (ignoriert) |
| Werkzeuge (Pfade angepasst) | `…/DarkBot-bsc-rev1/tools/` | `commit_wt.sh`, `gate_wt.sh`, `fmt_mine.py`, `repin.py`, `ci_wait2.py`, `mut_r2.py`, `mut_r3.py`, `fmt_base_files*.txt` |
| Notizen, Logs, Agentenberichte | `…/DarkBot-bsc-rev1/scratch/` | u. a. `rev1/` (Plan, Audits), `r3a/` (Commit-Text, Doku-Generator), `r2bfix/` (Fix-Agent: msg, files, Gate, Mutationen), Gate-/Mutations-Logs |
| Sicherungen | `…/DarkBot-bsc-rev1/backups/` | `wtEA_R2bfix_on_49bcd98f_uncommitted.patch` (Diff von `wtEA`), `stash_0_R2bfix-L1L4-20261101.patch`, die drei gelöschten R3a-Stashes als Patch, frühere R3a-Patches |
| Project_0-Worktrees (**tabu**) | `../DarkBot-p0-*`, `../DarkBot-project0` | gehören der Project_0-Sitzung |

`wtR2` und `wtR8` gibt es nicht mehr. Ein Worktree unter `/private/tmp` existiert nicht mehr. Freier Platz auf dem Datenvolume: etwa 67 GB.

---

## 2. Stand beim Stopp (02.11. 08:14–08:20)

- **Agenten:**
  - Gestoppt wurde der R2b-Fix-Agent, mitten im Übertrag nach `wtEA`; letzte Meldung: „Regenerated manifest passes F0-VAR (3496 native fields, including R3a's quarantine record)“.
  - Die Reviewer sind abgeschlossen: R3a-Review FAIL, R3a-Re-Check PASS (LOW behoben), R2b-Review FAIL, R2b-Re-Check PASS mit L-1 bis L-4.
- **Hintergrundjobs:** alle CI-Warter gestoppt. Kein Gate und kein Mutationslauf aktiv; keiner hinterließ Mutanten (R3a: `mut_r3.py --check`, alle 45 Muster wieder im Original).
- **Fremde Prozesse (nicht anfassen):**
  - PID 64432: G0.4-Kampagne, bis 03.11. 08:05Z, read-only.
  - PID 44297: verwaister `bsc_runtime`-Testprozess aus dem Target des Hauptcheckouts. Er schreibt nur in `/var/folders/…`; der Nutzer kann ihn beenden.
  - Ein `cargo test darkbot-runner`: Project_0.
- **Absprache mit der Project_0-Sitzung (01.11.):**
  - Jede Sitzung pusht nur ihren eigenen Fast-forward, nach fetch und rebase, und prüft die Dateiliste.
  - Gemeinsame generierte Dateien werden **regeneriert**, nie von Hand gemergt: `frontend/contracts/bsc-fields.json`, `bsc-native-fields.json`, `protocol/schema_id.txt`, generiertes Protokoll, Golden-Dateien.
  - P0 kündigt eine Änderung von `protocol/schema_id.txt` vorher an; BSC erzeugt dann `protocol/golden/bsc/engine.stream.hex` neu mit `UPDATE_BSC_GOLDENS=1 cargo test --locked --all-features -p darkbot-bsc-qualification -- status::tests::read_only_projection_and_golden_frames`.

---

## 3. Git- und CI-Stand (tatsächliche Ausgaben, 02.11. 08:16)

### 3.1 Hauptcheckout
```
$ git status -sb
## main...origin/main
?? bsc/config/g07_attempt1_acceptance_v4.dbrec
?? bsc/config/g07_attempt1_acceptance_v4.e1-review.dbrec
?? bsc/docs/9_ALT/Backup_v1.zip
?? bsc/engine/crates/execution/src/runtime_live.rs.orig
?? bsc/native/evidence/g06a/g06a_requal_dry_20261020T072853Z/
?? bsc/native/evidence/g06a/v526_requal_preview_20261023T190257Z/
?? bsc/native/evidence/g07/readiness_20261020T120918Z/
?? bsc/native/evidence/g07/readiness_20261020T121103Z/
```
**Diese acht Pfade nie committen und nie anfassen** (lokale Evidence und Nutzerdateien; `runtime_live.rs.orig` gehört dem Nutzer).

### 3.2 Worktrees (`git worktree list --porcelain`, BSC)
```
worktree …/DarkBot                       HEAD 49bcd98f… branch refs/heads/main
worktree …/DarkBot-bsc-rev1/wtEA         HEAD 49bcd98f… detached
(+ Project_0-Worktrees, tabu)
```
`git stash list`: `stash@{0}: On (no branch): R2bfix-L1L4-20261101` (BSC, behalten). Andere Stashes gibt es nicht.

### 3.3 `origin/main` (= `49bcd98f`) seit der alten Übergabe
```
49bcd98f fix(bsc): REV.1.0 R3a — a BUY attempt no byte of which left gives its slots back, …
738a2cc7 fix(bsc): the BSC golden engine stream carries the protocol schema id 09470f3d of 887a31bf
887a31bf feat(p0): DEC-18 package 4 …            (Project_0)
62bc70f0 fix(p0): DEC-18 package 2 re-check …    (Project_0)
047eafe7 docs(p0): DEC-18 session handoff …      (Project_0)
3f491f6c docs(p0): DEC-18 session handoff …      (Project_0)
4fbf4ddb docs(bsc): REV.1.0 continuation handoff (alte Fassung dieser Datei)
```

### 3.4 `wtEA` gegenüber `origin/main`
Er steht auf HEAD `49bcd98f`, mit 18 geänderten Dateien (2962+/638−), nichts gestaged, nichts untracked:
```
 M bsc/engine/crates/execution/src/live_attempts.rs
 M bsc/engine/crates/qualification/src/status/campaign.rs
 M bsc/engine/crates/qualification/src/status/live.rs
 M bsc/engine/crates/qualification/src/status/publisher.rs
 M bsc/engine/crates/qualification/src/status/tests.rs
 M docs/protocol/PROTOCOL.md
 M frontend/contracts/bsc-fields.json
 M frontend/src/core/generated/darkbot/bsc/v1/status_pb.ts
 M frontend/src/features/bsc/model/live.test.ts
 M frontend/src/features/bsc/model/live.ts
 M frontend/src/features/bsc/model/placement.generated.ts
 M frontend/src/features/bsc/model/types.ts
 M frontend/src/features/bsc/pages/LiveCampaign.tsx
 M frontend/src/features/bsc/pages/LivePanel.tsx
 M frontend/tests/bsc.spec.ts
 M protocol/golden/bsc/engine.stream.hex
 M protocol/schema_id.txt
 M protocol/schemas/darkbot/bsc/v1/status.proto
```

### 3.5 CI (GitHub Actions)
| Commit | Ergebnis | Beleg |
|---|---|---|
| `0159c1a7` R1 | bsc success, Project 0 status contract success | `scratch/ci_R1.out` |
| `f6156fa0` E-A | bsc (macOS, ubuntu, frontend) + Project 0 success | STATUS.md-Gate-Tabelle |
| `1d29cec7` R2 | bsc success, Project 0 status contract success | `scratch/ci_R2.out` |
| `9d5ed3a0` R2b | bsc, gateway, Project 0 status contract success | `scratch/eb/ci1.txt` |
| `4fbf4ddb`, `738a2cc7`, `49bcd98f` | **UNVERIFIZIERT** (Warter gestoppt) | prüfen: `python3 -I …/tools/ci_wait2.py <sha>` als Hintergrundjob |

Hinweis: Der bsc-Workflow auf `887a31bf` (Project_0) war wahrscheinlich rot, weil das BSC-Golden zur neuen `schema_id` fehlte; `738a2cc7` behebt das.

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
| R2b | LIVE-Tab-Projektion Kampagne/Versuche | **IN_PROGRESS** (`9d5ed3a0` auf main mit CI grün; Review FAIL → Fix uncommitted in `wtEA`, Re-Check PASS mit L-1–L-4, siehe §7.D) | `9d5ed3a0`; Fix in `wtEA` |
| R3 | Receipt-Recovery (024/041), EXIT_STOPPED resume/abandon, Nonce-Burn gesperrt, Ledger-Reparatur und Temp-Cleanup (045), Slot-Rückgabe (068), G0.7-Lot-Adoption; laut R1-Commit auch exit-now/write-off für gestrandete Lots; Audit A: Betreiberbefehle auch über Gateway und `LiveActions.tsx` | **R3a DONE (CI_PENDING)**, R3b NOT_STARTED (§6) | R3a `49bcd98f` |
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

## 6. R3 — Stand

### 6.1 R3a — fertig auf `main` (`49bcd98f`; CI UNVERIFIZIERT)
Commit-Text: `git log -1 --format=%B 49bcd98f`, auch in `scratch/r3a/msgR3a.txt`. Inhalt:
- **Ledger v4:** BUY-Fakten tragen `attempt`; Betreiberfakten RESUMED, WRITTEN_OFF, EXIT_REQUESTED und ADOPTED (`request_id`, `decided_by`, bei ADOPTED die Evidence); `source` RECEIPT/NONCE für R3b vorbereitet.
- **BSC3-068:** Ein Versuch, dessen jeder BUY-Fakt WITHDRAWN ist, gibt seine Slots zurück, sofern der eigene BUY-Fakt dabei ist, nichts verwaist war und keine Quarantäne besteht.
  - Umsetzung: `Store::return_attempt` (Rename unter dem Spend-Lock nach `buy-attempt-returns/`), `LiveRuntime::return_withdrawn` im Reconcile.
  - Die Vorwärtsuhr liest die zurückgegebenen Slots weiter.
- **Betreiberbefehle:**
  - `bsc_runtime live lot resume|write-off|exit-now` (einmal je Request-ID; Exit-now gesperrt durch `BSC_EXEC_LIVE_OPERATOR_EXIT=OFF`)
  - `live adopt-g07 --g08 --capture` (G0.8 v7 plus Capture; Launch- und BUY-Block final, Scope ok, kein offenes Lot des Tokens)
  - `live ledger check|quarantine`
- **BSC3-045:** Temp-Cleanup per Lock; Quarantäne mit Record zuerst, Nummer nie wiederverwendet; BUYs alter Grants gesperrt (`LIVE_LEDGER_QUARANTINED`).
- **R1-Rest:** Lots, die beim Start offen sind, gelten als gemeldet. Ein danach geschlossenes oder abgeschriebenes Lot endet für die Strategie.
- **Review:** FAIL (F1–F5) → behoben → Re-Check PASS; dessen RC1–RC4 behoben.
- **Nachweise:** 45 Mutanten RED (`tools/mut_r3.py`, Logs `scratch/mut_r3a*.out`). Gate GR3a3 GATE_OK: 1650/0/27, bsc_tools 7/7, Clippy, F0-VAR PASS mit 3496 nativen Feldern (`scratch/gate_GR3a3.out`).
- **Hot-Path:** Append plus Lock/Inode 3,1 µs p50 gegenüber einem Append von 10,8 ms; das liegt nach dem Senden.

### 6.2 R3b — NOT_STARTED (Soll und Plan)
- **Receipt-Recovery (BSC3-024/041)** für unaufgelöste Transaktionen, die blockieren: Nonce ≠ next, Fenster unbekannt oder verwaist, eingeschlossen in einem Block, dem der Follower nicht folgte.
  - `eth_getTransactionReceipt(hash)` → Blocknummer und -hash.
  - Hash durch die zweite Quelle bestätigen (BSC3-056, `canonical_confirm.rs` / `chain_follower/confirm.rs`, Header-Abruf).
  - Block und Receipts über den Follower-Abruf (`eth_getBlockByNumber(n,true)` + `eth_getBlockReceipts`, `chain_follower/follow.rs`) und `launches::flap_events`.
  - Fakten INCLUDED/REVERTED mit `source=RECEIPT`.
  - Ohne Receipt und mit bestätigter Konto-Nonce > Tx-Nonce (C2-Nonce-Fakten, `runtime_state.rs`) → REPLACED mit `source=NONCE`.
  - Begrenzt per Config (neue Schlüssel z. B. `BSC_CANONICAL_RECOVERY_READS_MAX`/`_RETRY_MS`), als Lese-Thread wie der Kurvenleser in R1 (`runtime.rs` `ask_lot_reads`).
  - Offen ist, wie das sauber an `on_canonical` andockt, ohne den laufenden Blockzustand (`self.block`, `seen`) zu stören: nur zwischen den Blöcken verarbeiten.
- **Nonce-Burn gesperrt:** `bsc_runtime live nonce-burn`, Default `BSC_LIVE_NONCE_BURN=OFF`, nur bei Gate CLEAR plus Grant; 0-Wert-Self-Transfer → REPLACED; nur Loopback-Tests, **nie real senden**.
- **Gateway/Frontend-Aktionen** für die R3-Befehle (Audit A: CLI + Gateway + `LiveActions.tsx`, CAS, gesperrt), zusammen mit R10.
- **Statusprojektion:** eine Warnung `LIVE_LEDGER_QUARANTINED` im LIVE-Tab. Bisher nur als Verweigerungsgrund sichtbar.


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

### 7.D R2b-Review-Fix — IN_PROGRESS (uncommitted in `wtEA`)

1. **Erstes Review von `9d5ed3a0`: FAIL.** HIGH-1, MEDIUM-2 bis 5 und LOW-6 bis 9 (Details: Agentenbericht in `scratch/r2bfix/msg.txt`):
   - HIGH-1: 40 Versuchsfelder als angeschlossen markiert, aber nicht projiziert
   - MEDIUM-2: unbegrenzte Reads und Warnungen
   - MEDIUM-3: Dateiname nicht gegen TX_HASH geprüft
   - MEDIUM-4: Versuche anderer Auswertungen ohne Warnung
   - MEDIUM-5: Fixtures mit gleichen Werten
   - LOW-6: Symlink-Verzeichnis
   - LOW-7: INVALID-Verdikt
   - LOW-8: Zähler als Raten
   - LOW-9: Fenstertext als Bool
2. **Fix-Agent auf Basis `047eafe7`:** alle 9 behoben. Gate R2bfix GATE_OK (1634/0/27, F0-VAR PASS), 21 Mutanten RED.
3. **Re-Check: PASS** mit LOW:
   - L-1: Rest des LIVE-Tabs nicht im Budget; Kampagnenteil zuletzt mit Restbudget projizieren und Test mit gefülltem Tab.
   - L-2: Link-Tests prüfen Links nicht wirklich.
   - L-3: Auswahlregel bei mehr als MAX+1 Records dokumentieren.
   - L-4: Frontend-Tests und Mutanten auf dem Endstand wiederholen.
   - Dazu Nits: eine gemeinsame `attempt_file_name`, Warnung bei gemischten Auswertungen ohne Verdikt, T0-Hinweis.
4. **Der Agent hat L-1 bis L-4 bearbeitet** (Umfang UNVERIFIZIERT), den Stand als **`stash@{0}` „R2bfix-L1L4-20261101“** gesichert (Basis `047eafe7`), `wtEA` auf `49bcd98f` gesetzt, den Stash angewendet und begonnen zu regenerieren. **Dann wurde er gestoppt.**

Ob in `wtEA` alle gemeinsamen Dateien korrekt neu erzeugt sind, ist UNVERIFIZIERT: `schema_id.txt`, `PROTOCOL.md`, `status_pb.ts`, Golden, `bsc-fields.json`, `placement.generated.ts`.

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

**Letzter abgeschlossener Schritt:**
- R3a ist als `49bcd98f` auf `main` gepusht (CI UNVERIFIZIERT).
- Danach Stopp und Aufräumen: `wtR2` und `wtR8` entfernt, R3a-Stashes gelöscht (als Patch gesichert), Hauptcheckout per Fast-forward auf `49bcd98f`.

**Unmittelbar nächster Schritt:** den R2b-Review-Fix in `wtEA` verifizieren und abschließen.

```
R=/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot
D=/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot-bsc-rev1
W=$D/wtEA
git -C $R fetch -q origin; git -C $R worktree list; git -C $R stash list
git -C $W status --porcelain=v1          # 18 Dateien (§3.4)
git -C $W diff | shasum -a 256           # vergleichen mit backups/wtEA_R2bfix_on_49bcd98f_uncommitted.patch
```

1. **CI-Stand prüfen:** `python3 -I $D/tools/ci_wait2.py 738a2cc7 49bcd98f` als Hintergrundjob (volle SHAs mit `git rev-parse`). Ist bsc rot, das zuerst beheben.
2. **Rebase in `wtEA` prüfen und abschließen.**
   - Gemeinsame Dateien nur regenerieren, nie von Hand mergen:
     - Proto-Generat und `PROTOCOL.md`, `schema_id.txt` mit den Repo-Generatoren (siehe `git show --stat 9d5ed3a0` und `scratch/r2bfix/`).
     - Golden mit `UPDATE_BSC_GOLDENS=1 cargo test … read_only_projection_and_golden_frames`.
     - `npm run gen:bsc-placement`, `python3 -I $D/tools/repin.py`, natives Inventar (§11), `node frontend/scripts/check-bsc-fields.mjs`.
   - Bei Zweifel: aus `stash@{0}` (Basis `047eafe7`) neu übertragen; `stash apply`, nicht `pop`.
3. **L-1 bis L-4 abschließen** (§7.D) und auf dem Endstand laufen lassen: Frontend vitest, type-check, `npm run test:bsc`, die Rust-Tests in `status::tests`, Mutationen (`scratch/r2bfix/mutR2bfix.py`), fmt nur der eigenen Hunks (Baseline 355).
4. **Unabhängiger Re-Check** (1 Agent), danach `zsh $D/tools/gate_wt.sh R2bfix2 $W $D/wtEA-target` → GATE_OK.
5. **Commit und Push:**
   - `bash $D/tools/commit_wt.sh $W <origin/main-sha> <msg> <liste> [bsc/docs/1_PLAN/STATUS.md=<datei>]`
   - Commit-Text: `scratch/r2bfix/msg.txt`; Dateiliste: `scratch/r2bfix/files.txt`, beide aktualisiert.
   - STATUS: R2-Zeile.
   - Dann `git -C $W push origin "<sha>:refs/heads/main"` (Refspec in zsh quoten) und `ci_wait2.py <sha>`.
6. **R3b** (§6.2), dann R4 bis R14 (§5) und die offenen Audit-E-Befunde (§8). Jedes Paket mit fail-before, Tests, Mutationen, unabhängigem Review, Gate, Push und CI.
   - **Nur ein BSC-Worktree:** neue Arbeit in `wtEA` (nach dem R2b-Commit sauber auf `origin/main` setzen) oder einem neuen, aber dann nur diesem einen; überflüssige sofort mit `git worktree remove` entfernen.
7. **Läufe nach 03.11. 08:05Z** (Kampagnenende; nur der Nutzer stoppt die Kampagne): Audit B/R14. Am Ende R14 mit Gesamtabnahme und der Anforderungs-/Abnahmematrix.

**Nicht tun:**
- den R2b-Fix oder `stash@{0}` verwerfen
- im Hauptcheckout arbeiten oder pullen, außer per Fast-forward nach Absprache
- neue Worktrees anhäufen
- eine Echtgeld-Aktion
- paketweites `cargo fmt`
- die acht lokalen Evidence-Pfade committen
- Project_0 berühren
