# BSC — Einstieg (Version 7, Repo-Fassung)

**Stand:** 2026-10-22 · **Quellstand:** tatsächlicher Git-HEAD zum Zeitpunkt der Umstellung, siehe Git-Commit · **Geltung:** ausschließlich `bsc/`

Dies ist der **Dokumentations-Einstieg** in das BSC-Projekt. Verbindlich sind die Originaldateien in den Unterordnern dieses Verzeichnisses in der Rangfolge aus §2. Dieses Dokument ersetzt keinen Vertrag; bei Widerspruch gilt das Original. Inhaltlich entspricht es Version 7 des Repository-Einstiegs `00_BSC_VERBINDLICH.md`; nur die Pfade zeigen hier auf das Repository.

**Aktuelle Plan-Revision: v7 (22.10.2026).** Revisionsliste und welche Messung zu welcher Revision gehört: `1_PLAN/REVISIONEN.md`.

**Ordner (Nummer = Lesereihenfolge):**

| Ordner | Was drin ist | Was man damit tut |
|---|---|---|
| `1_PLAN/` | PHASENPLAN.md (= verbindliche Roadmap), Prüfliste (= Gate-0-Testplan), Bauplan (= Masterplan), `STATUS.md`, `ACCEPTANCE_MAP.json`, `REVISIONEN.md` | lesen, danach arbeiten |
| `2_REGELN/` | die technischen Verträge je Baustein | nur den Vertrag des Bausteins lesen, an dem man arbeitet |
| `3_BEWEISE/` | Ledger, Evidence-Register, Freezes, Mess-Reports | nachschlagen; nie ändern |
| `4_G07_FREIGABE/` | Unterlagen für den nächsten Schritt G0.7 (Self-Launch) | bei G0.7-Arbeit |
| `5_OFFENE_FEHLER/` | `bsc_bug_v1.md` | offene Korrekturaufträge |
| `6_VORSCHLAEGE/` | Anträge, nicht verbindlich | nichts daraus umsetzen ohne Freigabe |
| `9_ALT/` | abgelöst | nie als Auftrag lesen |

**Die drei Pläne sind ein Plan:**

| Datei in `1_PLAN/` | Was sie ist | kurz |
|---|---|---|
| `PHASENPLAN.md` | Reihenfolge der Phasen, Stop-Regel, Entscheidungsregel | **Phasenplan** (Rang 1) |
| `GATE0_TESTPLAN_SHORT.md` | Prüfungen G0.1–G0.8 vor der Produktion | **Prüfliste** (Rang 2) |
| `BSC_PRODUCTION_BUILD_MASTER_PLAN.md` | wie die Produktionssoftware gebaut und abgenommen wird | **Bauplan** (Rang 3) |


**Lesereihenfolge für jede KI / jeden Implementierer:** dieses README → `1_PLAN/PHASENPLAN.md` → `1_PLAN/GATE0_TESTPLAN_SHORT.md` → `1_PLAN/STATUS.md` → aus `2_REGELN/` nur den Vertrag des Bausteins, an dem gearbeitet wird. `3_BEWEISE/` und `5_OFFENE_FEHLER/` bei Bedarf. `9_ALT/` nie als Auftrag lesen.

---

**Neue fünf-Quellen-Architektur seit v6; Dateiname v7:** Fünf einzeln `OFF / SHADOW / LIVE` schaltbare Ingressquellen: eigene geth-Fast-Lane, PublicNode, dRPC, BlockReq, 48Club Auction Feed. Alle führen in **einen** Rust-BUY-Core; `FIRST VERIFIED ACTIONABLE WINS`. Die T0-Definition und der G0.7/G0.8-Vorrang bleiben unverändert. `T_head_local` und Blockintervall sind niemals `T0`. Einzelheiten: `1_PLAN/PHASENPLAN.md` §5a. WSS-Messungen vom 22.10. wurden **unter v5** erhoben und sind kein N+1-/T0-PASS.

## 1. Projektziel in einem Satz

Frühestmöglicher **direkter On-Chain-BUY** nach einem unterstützten BSC-Launch (Flap, später Four.meme), mit gemessenem Nachweis von Token-Erwerb, Reihenfolge und Kosten — nicht mit Frontend-, Receipt- oder Builder-ACK-Zeiten.

## 2. Verbindliche Rangfolge (unverändert)

| Rang | Datei | Ordner |
|---|---|---|
| 1 | `1_PLAN/PHASENPLAN.md` — Roadmap, Stop-Regel, Phasenfolge, Invarianten, Entscheidungsregel | `1_PLAN/` |
| 2 | `1_PLAN/GATE0_TESTPLAN_SHORT.md` — Gate-0-Qualifikation G0.1–G0.8, Zeitdefinitionen, Spend-Freeze | `1_PLAN/` |
| 3 | `1_PLAN/BSC_PRODUCTION_BUILD_MASTER_PLAN.md` — Produktions-Masterplan, Abnahmematrix | `1_PLAN/` |
| — | Technische Verträge (Birth-Entry-Architektur, Runtime-Contract, Economics-Gate, Phase C/D/E/F, C1-Plan, C1.8, C1-Runbook) | `2_REGELN/`; Wire V1/V2 in `../geth/` |
| — | `1_PLAN/ACCEPTANCE_MAP.json` | `1_PLAN/` |
| — | Nachweisregister (Ledger, `EVIDENCE.md`) | `3_BEWEISE/` |
| — | Ist-Stand (`STATUS.md`) | `1_PLAN/` |
| — | G0.7/G0.8-Abnahmedokumente (Self-Launch) | `4_G07_FREIGABE/`, Ergebnisse in `3_BEWEISE/` |
| — | Bugfix-Aufträge (`bsc_bug_v1.md`) | `5_OFFENE_FEHLER/` |
| Planentwurf | `6_VORSCHLAEGE/MODULPLAN_ZUSCHALTBARE_PFADE_V1.md` — Grundsätze vom Nutzer entschieden (2026-10-22), Einordnung erst nach Freigabe des Änderungstextes (dort §11) | `6_VORSCHLAEGE/` |
| Antrag | `6_VORSCHLAEGE/BACKRUN_ERWEITERUNG_ANTRAG_V4.md` — **nicht verbindlich**, Detailbeschreibung der Backrun-Module, siehe §12 | `6_VORSCHLAEGE/` |

Alles in `9_ALT/` ist abgelöst und hat keine Autorität.

## 3. Ziel, Zeitdefinitionen und die „2 ms"-Frage

Zeitpunkte (Roadmap §5, Testplan §2):

```
T_pre = frühester handlungsfähiger Hinweis vor Handelbarkeit
T0    = erster Ausführungspunkt, an dem der Coin kaufbar ist
T_buy = unsere erfolgreiche BUY-Ausführung
```

**Verbindliches Produktziel (unverändert):** `T_buy − T0 < 1 ms`, lokaler Hot-Path (Ingress → Transport-Handoff) `< 1 ms`, bevorzugt `< 0,5 ms`.
**Status:** `T0_TO_T_BUY_LT_1MS = NOT_PROVEN`. Ohne Original-Producer-Execution-Witness (`source_ready=false`) bleibt T0→T_buy **UNKNOWN**. Fehlende Messbarkeit hebt die Anforderung nicht auf.

**„2 ms":** steht in keinem verbindlichen Dokument. Soll die Grenze auf 2 ms geändert werden, muss das ausdrücklich durch den Nutzer in Roadmap §5/§20 und Masterplan §2/§10 beschlossen werden. Bis dahin gilt 1 ms. Sub-ms-Zahlen aus Blockzeitstempeln oder Receipts abzuleiten ist verboten.

Nicht als Ziel-Nachweis gültig (Roadmap §20, Testplan §10): lokale Signatur, Submission, Builder-ACK, geth-Annahme, Receipt, Blockposition allein.

## 4. Stop-Regel und Reihenfolge (Roadmap, verbindlich, unverändert)

```
G0.7  kontrollierter minimaler Mainnet-BUY (ein Versuch, Self-Launch)
  -> G0.8  Nachweis: echter Token-Erwerb + Reihenfolge + Kosten
  -> PASS / ADAPT / STOP
  -> erst dann C1-Freeze / C2 / C3 / Optimierung
```

- **Kein** C1.10-Freeze, C2, C3, Decoder-Arbeit, Optimierung, Refactoring oder synthetisches Benchmarking vor G0.7/G0.8.
- Dokumentierte Ausnahmen (bleiben Ausnahmen, keine Phasenabnahme): C1.5–C1.8-Integration (2026-10-16), Strukturreform R1–R13 (2026-10-15/16), Offline-Bugfix E5–E8 (2026-10-21). **Jede weitere Ausnahme — auch die Backrun-Untersuchung BR0 aus §12 — braucht eine eigene, ausdrückliche Nutzerfreigabe.**
- Drei getrennte Zustände, keiner impliziert den nächsten: (1) technisches `PASS_READ_ONLY`, (2) explizite ökonomische Nutzerfreigabe des exakten V4-Approved-Hash + Guard-Scharfschaltung, (3) echter On-Chain-Erwerb/Reihenfolge/Timing-Nachweis.
- Entscheidungsregel (Roadmap §21): STOP für beide Venues → BSC-Roadmap endet. Ein Venue PASS → Produktion nur für dieses Venue.

## 5. Harte Sicherheits- und Datenregeln (gelten immer)

1. **Keine Echtgeld-/Mainnet-Sends, kein Funding, keine LIVE-Freigabe** ohne ausdrückliche, separate Nutzerfreigabe. `FROZEN_APPROVED_V4_SHA256 = "UNARMED"`, `approved=false`, `execution_ready=false` bleiben bis dahin unverändert; Guard-Prüfungen werden nie gelockert.
2. **Kein signierter Mainnet-Transaktions-Bytestring verlässt die Maschine ohne Freigabe** — auch nicht in einem „DRY" mit abgelaufenem `maxBlockNumber`: Ein Builder-Ablaufdatum macht die signierte Transaktion nicht unausführbar; sie bleibt gültig, bis ihre Nonce verbraucht ist.
3. G0.7 ist ein streng bewachter Einzelversuch (Feature `g07-mainnet`), kein Trading-Runner; kein autonomer Retry. G0.8 und Producer-Timing sind read-only und senden nie.
4. Deployment-Identität (Portal-/Implementation-Codehash) bleibt Pflichtprüfung. ABI-Prüfung und `minOutput` ersetzen sie nicht.
5. Signierte Bytes, Kaufparameter, Gas-/Budget-Grenzen (max. Verlustreserve je Versuch **481163500000000 wei = 0,0004811635 BNB**), Nonce, Slippage, Wire-/SPSC-Layouts, Memory-Ordering, Version-Pins (Rust 1.93.1, bsc-geth `cdb7548b`, alloy 1.7.3, rustls 0.23.45, k256 0.13.4) bleiben unverändert, außer eine dokumentierte qualifizierte Migration verlangt es.
6. **JSON ist intern verboten**; Ausnahme nur an externen Provider-/RPC-/Builder-Grenzen. Interne Formate sind typisierte, versionierte Binärrecords (DBREC001). Historische hash-gebundene JSON-Dateien bleiben unveränderlich.
7. **Historische Evidence nie inhaltlich bearbeiten**, keine Hashes neu erfinden. Bei Umzug: Quellcommit, Originalpfad, Inhaltshash, neuer Pfad dokumentieren (`EVIDENCE.md`).
8. Ein Eigentümer je Funktion: keine zweite BUY-, Quote-, Signier-, Decoder- oder State-Engine. Kein WSS/JSON/RPC-Lookup im Entscheidungs-Hot-Path.
9. Topics, Selektoren, Adressen und Builder-EOAs werden aus kanonischen On-Chain-Receipts verifiziert, bevor sie in Code eingefroren werden — auch Werte aus Builder- oder Launchpad-Dokumentation.
10. Keine fremden Sniper-Repos, SDKs oder Skripte mit Zugriff auf Keys ausführen.
11. Git-Hygiene: kein `git clean`, kein Hard-Reset, kein `git add -A`; `git mv` vor Löschen; **Commit/Push nur mit Nutzerfreigabe**. `project_0/`, `frontend/`, `solana/` werden aus BSC-Arbeit heraus nie angefasst.
12. Tests sind Nachweise: Nicht ausgeführte Tests sind **nicht PASS**; max. 10 Testdurchläufe je Auftrag; keine Stress-/Benchmark-/Mainnet-Läufe ohne Auftrag.

## 6. Ist-Stand 2026-10-21 (aus `STATUS.md`)

| Punkt | Stand |
|---|---|
| Flap Portal Mainnet | **v5.26.0** seit Block 128914179. Read-only-Readiness: `CURRENT_DEPLOYMENT_DRIFT=STOP`. |
| Requalifikation 2026-10-20 | R0 + DRY-0…DRY-4 **PASS gegen v5.25.0** — durch v5.26.0 nicht mehr gültig. Neue Requalifikation gegen v5.26.0 braucht eine eigene Entscheidung (Salt-Wahl A/B, DRY-1 Signatur durch den Nutzer). |
| V4-Vertrag | Vorschlag SHA-256 `5da0068b…50ec3d` (approved=false, ein Versuch); Approved-Variante `4308f2cb…29ad0` nur berechnet. `config/g07_attempt1_acceptance_v4.dbrec` noch nicht erzeugt. |
| Guard / G0.7 / G0.8 | **UNARMED** / **NOT_EXECUTED** / **NOT_EXECUTED**. LIVE = BLOCKED. |
| Readiness-Code | **Bereits umgesetzt:** `g07_pre_live_readiness.rs` (`technical_ready`, `finish_report`) liefert `PASS_READ_ONLY` mit Exit 0, wenn alle Pflichtprüfungen `PASS` sind und nur `BUILDER_SEMANTICS=UNQUALIFIED` und `PRODUCER_WITNESS=UNKNOWN` offen sind; Unit-Test dafür vorhanden. Im Code gelesen, Test hier nicht ausgeführt (keine Rust-Toolchain verfügbar). **`STATUS.md` Zeile 13 ist veraltet** und behauptet das Gegenteil — beim nächsten STATUS-Update korrigieren. Blockierend sind die Deployment-Drift auf v5.26.0, die an v5.25.0 gebundene DRY-Evidence und die nicht bestandenen aktuellen Deployment- und Builder-Prüfungen. |
| Builder | Aufnahme, 48Club-Preis, Wirkung `noMerge`/`positionFirst` **UNQUALIFIED**; Builder-Beobachtung abgelaufen (Refresh ≤ 24 h vor Scharfschalten). |
| Producer-Witness | fehlt (`source_ready=false`) → T0→T_buy **UNKNOWN**. |
| C1 | C1.5–C1.8 integriert, regressionsgeprüft (479 PASS / 0 FAIL / 5 ignored), **nicht phasenqualifiziert**; C1.9/C1.9b/C1.10 offen. HOT0 nicht im produktiven BUY-Pfad. |
| Bugfix V1 | E1–E8 geprüft; Pakete E7-6…E2-4 offline FIXED/TESTED. |
| Nächster Meilenstein | **G0.7 CODE READY — AWAITING EXPLICIT ECONOMIC APPROVAL** (kein PASS). |

## 7. Phasenstatus (Kurzform; verbindlich ist Roadmap §3)

| Phase | Status |
|---|---|
| G0.1 | IN PROGRESS |
| G0.2 / G0.3 | PASS PER SCOPE |
| G0.4 | PARTIAL |
| G0.5 | PASS für presigned Self-Launch (Transport-Evidence, nicht T_buy) |
| G0.6 | G0.6a FINAL DRY PASS (v5.25, durch v5.26 invalidiert) / Matrix PARTIAL |
| **G0.7** | **NEXT** |
| G0.8 | unmittelbar nach G0.7 |
| G0.9–G0.11 | BLOCKED |
| C0 | IN PROGRESS / Foundation FROZEN |
| C1 | PAUSED vor Freeze |
| C2, C3, D–K, KB | BLOCKED |

## 8. Wo die Phasen im Code leben (`bsc/engine/`)

| Phase | Code / Werkzeug |
|---|---|
| G0.6a DRY / Requal | `crates/qualification/src/bin/g06a_*`, `crates/execution/src/bin/g06a_preflight_record.rs`, `tools/g06a_final_dry.sh`, `tools/g06a_*.cjs` |
| G0.7 | `crates/execution/src/bin/g07_mainnet_self_launch_buy/` (Feature `g07-mainnet`), `crates/execution/src/acceptance_guard.rs`, `config/g07_attempt1_acceptance_v4.dbrec` (zu erzeugen), `tools/g07_*.sh` |
| G0.8 | `crates/qualification/src/bin/g08_mainnet_verify.rs` (Flap-Self-Launch-spezifisch: eigener Launcher/Buyer, Flap-Portal, `TokenBought`), `g08_producer_timing.rs` |
| C0 | `crates/{trading,tx_encoding,execution,journal}` |
| C1 | `crates/geth_ipc`, `bsc/geth/`, `native/evidence/c1/`, `tools/c1*`, `c13*`, `c14*`, `c19*` |
| Binärformat | `crates/record` (DBREC001), `crates/legacy_import` |

Cargo immer aus `bsc/engine`: `cargo check --locked --workspace --all-targets --features darkbot-bsc-execution/g07-mainnet`.

## 9. Dokumentenregister

Status: **V** = verbindlich · **N** = Nachweis (unveränderlich) · **H** = historisch · **A** = abgelöst · **AN** = Antrag, nicht verbindlich

| Datei (`bsc/docs/` sofern nicht anders) | Ordner | Status |
|---|---|---|
| PHASENPLAN.md | `9_ALT/` | A — ersetzt durch dieses README |
| PHASENPLAN.md | `1_PLAN/` | **V, Rang 1** |
| GATE0_TESTPLAN_SHORT.md | `1_PLAN/` | **V, Rang 2** |
| BSC_PRODUCTION_BUILD_MASTER_PLAN.md | `1_PLAN/` | **V, Rang 3** |
| ARCHITECTURE.md, BSC_BIRTH_ENTRY_ARCHITECTURE.md, BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md, BSC_ECONOMICS_GATE_CONTRACT.md, BSC_PHASE_C/D/E/F-Verträge, C1-Plan, C1.8, C1-Runbook | `2_REGELN/` | V |
| `../geth/README.md`, `../geth/FASTLANE_WIRE_V1.md`, `../geth/FASTLANE_WIRE_V2.md`, `../engine/crates/qualification/README.md`, `../tools/STATUS_REPORTER.md`, `../../WALLET_SECURITY.md` | unverändert an ihrem Ort | V |
| `../README.md` (R1–R13) | unverändert an seinem Ort | H, §5 gültig |
| STATUS.md | `1_PLAN/` | V (Ist-Stand) |
| G07_REQUALIFICATION_RESULT_20261020.md | `3_BEWEISE/` | N (v5.25; überholt) |
| G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md | `4_G07_FREIGABE/` | H (Vorlage für Requal v5.26) |
| G07_ATTEMPT1_AUTHORIZATION_REVIEW.md | `4_G07_FREIGABE/` | H (Entscheidungsliste gültig) |
| G07_BUILDER_PROBE_APPROVAL.md | `3_BEWEISE/` | H/N |
| G07_G08_BINARY_MIGRATION_PLAN.md | `4_G07_FREIGABE/` | H (M1–M8 erledigt) |
| EVIDENCE.md, Ledger, GATE0_G01/G02-Dateien, G02_FLAP_CORRECTION, C1-Rev2-Report | `3_BEWEISE/` | N |
| G05_LOCAL_GETH_PREFLIGHT.md, BSC_C1_STATUS_AND_FABLE_HANDOFF_20261008.md | `9_ALT/` | H |
| `../../prestudy/bsc/README.md` | unverändert an seinem Ort | H |
| bsc_bug_v1.md | `5_OFFENE_FEHLER/` | V |
| BSC_IMPLEMENTATION_HANDOFF_CLAUDE.md, GATE0_FABLE_EXECUTION_CHECKLIST.md, G07_ATTEMPT1_ACCEPTANCE_V2_DRAFT.json, alter Index (jetzt `DOCS_INDEX_ALT_2026-10-16.md`) | `9_ALT/` | **A** |
| — (neu) `6_VORSCHLAEGE/MODULPLAN_ZUSCHALTBARE_PFADE_V1.md` | `6_VORSCHLAEGE/` | **Planentwurf** (Grundsätze [Nutzer]) |
| — (neu) `6_VORSCHLAEGE/BACKRUN_ERWEITERUNG_ANTRAG_V4.md` | `6_VORSCHLAEGE/` | **AN** |
| — (neu) `../tools/br0/wss_race.mjs`, `test_wss_race.mjs`, `6_VORSCHLAEGE/WSS_MESSUNG_ANLEITUNG.md` | `../tools/br0/` | Werkzeug für OBS-6 (Tests 6/6) |
| — (neu) `../tools/br0/br0_observe_launches.mjs` (Version 4), `test_br0.mjs`, `fixtures/` | `../tools/br0/` | AN (Werkzeug für BR0, Entwurf; Tests 15/15, eigenständig lauffähig) |
| frühere Entwürfe (Backrun-Plan V1, Antrag V2/V3, Skripte f0 und br0 v2/v3, Einstieg Version 2 bis 5) | nicht im Repo | **zurückgezogen**, nicht verwenden |

## 10. Repository-Hygiene (Vorschlag, nichts ausgeführt)

**Die Originaldokumente in `bsc/docs/` bleiben im Repository.** Sie sind Pfad-, Audit- und Vertragsgrenzen; die Unterordner enthalten die Originale. Neue Ergänzungen werden als eigene, versionierte Dateien abgelegt, nicht in bestehende Verträge eingemischt.

Kandidaten zum Entfernen, nur nach Nutzerfreigabe und nur per `git mv` nach `prestudy/bsc/`, danach `cargo check` (§8) und `cargo test -p darkbot-bsc-execution --test acceptance_guard`:

- `tools/g06a_final_dry_legacy_v2.sh`, `tools/g07_pre_live_readiness_legacy_v2.sh` (nur „historical only"-Kommentare)
- `native/evidence/c1/c1-9-native-latency-20261008T221120*`, `…20261010T230358*` (keine Referenz)
- `native/evidence/c1/*run2-burst-invalid*` (als ungültig markiert, keine Referenz)
- byteidentische Duplikate von `native/evidence/c1/rev2-c14a-rerun-20261008/` auf Ordnerebene

Vor jedem Entfernen: Referenzsuche über das gesamte Repository (Code, Tests, Docs, Ledger, `EVIDENCE.md`) erneut ausführen und im Bericht zeigen.

**Nicht entfernen:** `config/g07_attempt1_acceptance_v3.dbrec`, `config/g01_mainnet_acceptance_proposal_v2.json`, `native/evidence/{q5a,g06a,4_G07_FREIGABE/readiness_*,c1/rev2-regression-20261008,c1/templates}`, `crates/record`, `crates/legacy_import`, `tools/test_*.py`, `prestudy/bsc/` (Archiv; Entfernung nur mit Tag und eigener Entscheidung — enthält u. a. die kanonischen Four-Classic-Fixtures unter `prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-27-g01/canonical_fixture_scan/`, die `test_br0.mjs` benötigt).

## 11. Arbeitsregeln für Agenten

1. Zuerst `git status -sb`, `git log -1`, dann 00 → Roadmap → Testplan → `STATUS.md`. Lokale Arbeit/Secrets/Stashes nie überschreiben.
2. Nur den beauftragten Scope ändern; keine neuen Phasen, Ordner, Engines oder Platzhalter.
3. Jede Behauptung über PASS/Latenz muss auf einen Nachweis oder einen ausgeführten Test zeigen. UNKNOWN/UNQUALIFIED bleiben so stehen.
4. Ein Antrag (§12) ist kein Auftrag. Nichts daraus umsetzen ohne ausdrückliche Nutzerfreigabe der betreffenden Stufe.
5. Ergebnisbericht je Auftrag: geänderte Dateien, Befehle, PASS/FAIL, Testzähler, offene Risiken, nächster Schritt. Kein Commit/Push ohne Freigabe.

## 12. Modulare Pfade und Backrun-Erweiterung

**Grundsätze [Nutzer, 2026-10-22]:** Kostenlose Module mit Erkenntnisgewinn werden vorgezogen. Jeder Pfad ist ein Modul mit den Zuständen AUS / SCHATTEN / LIVE. Kostenverursachende Module sind standardmäßig AUS, ihr Nutzen für die Profitabilität wird gemessen. Ausgearbeitet in `6_VORSCHLAEGE/MODULPLAN_ZUSCHALTBARE_PFADE_V1.md`. Verbindlich für Roadmap und Stop-Regel wird der Plan erst mit Freigabe seines Änderungstextes (§11 dort).

### Backrun-Erweiterung — Status: Antrag, nicht verbindlich

`6_VORSCHLAEGE/BACKRUN_ERWEITERUNG_ANTRAG_V4.md` beschreibt eine **zusätzliche** Fähigkeit: fremde Launches über den 48Club-Auction-Feed erkennen und mit einem Backrun-Bundle (`backrunTarget`) direkt dahinter kaufen. Der Antrag ersetzt nichts aus §2–§11:

- Roadmap, Testplan, Masterplan, `T0→T_buy < 1 ms`, G0.7/G0.8, Deployment-Identitätsprüfung und C1–J bleiben verbindlich.
- Der Feed wäre eine weitere Signalquelle, die Backrun-Submission eine weitere Ausführungsart desselben Intents — kein zweiter Bot, kein zweiter Trading-Core.
- **Einzig BR0** (Read-only-Beobachtung mit `../tools/br0/br0_observe_launches.mjs` Version 4, nur Four Classic TokenManager2, sendet und signiert nichts) kann als Ausnahme von der Stop-Regel freigegeben werden — nur durch ausdrückliche Nutzerfreigabe, wie die früheren Ausnahmen.
- Jede weitere Stufe (BR1–BR5) braucht eine eigene Nutzerfreigabe; Kapitalbindung (48 Soul Points) erst auf Grundlage der BR0-Ergebnisse.
