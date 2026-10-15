# Project_0 — aktiver Handoff für neue Chats

**Aufnahme:** 2026-10-15. **Repository:** `sawyerma/darkbot`, Branch `main`.
**Strukturstatus:** Phase A abgeschlossen (Iterationen 1–7); **8a DONE** (`958d0b3`); **8b.1 DONE und vom Nutzer getestet/gepusht** (`e8e904cc5e350e2edfc382230c4189d34a4aa003`); **8b.2 NEXT**, **8b.3 OPEN**. Damit **7/32 Hauptiterationen vollständig DONE**, Iteration 8 bleibt PARTIAL.
**Vollständige Fortschrittstabelle:** [32-Schritte-Roadmap](../architecture/restructure_32_step_plan.md).
**Fachlicher Bot-Status:** [STATUS.md](../STATUS.md). Maßgeblich bleiben das [Measurement-Ledger](../MEASUREMENT_LEDGER.md) und der [Phasenplan](../PHASENPLAN.md).

> Bei Wiederaufnahme zuerst den aktuellen tatsächlichen `main`-HEAD und lokalen Git-Status prüfen. In diesem Handoff genannte SHAs sind historische Nachweise, keine Garantie über künftigen HEAD. Diese Datei dokumentiert die **Repository-Strukturreform** und gewährt keine Handelsfreigabe.

## 1. Unser Gesamtziel

Wir bauen einen **vollwertigen professionellen Solana-/Marginfi-Liquidationsbot**, nicht eine Demo. Wiederverwendung der vorhandenen Rust-Architektur statt zweiter Engine: kanonischer Solana-/Bank-/Oracle-/Health-State, exakte Protokoll-/Sizing-/Funding-/Route-/Transaction-/Simulation-Kette, kontinuierliche Markterfassung und Chain-Verifikation, wirtschaftliche Selektion (Gebühren, Tips, Slippage, tatsächlicher Exit und Fehlversuche), Gewinner-/Konkurrenzanalyse, crashsichere Reservierungen und Abrechnung, später schnelle Race-/Jito-Integration, P6-MT für Apple M4 und Frontend.

**Geschäftliches Ziel:** der erste belegte **positive kumulative realisierte Netto-PnL**, erst nach vollständigen Sicherheits- und wirtschaftlichen Nachweisen. Kein Ertrag garantiert. `FREE_ONLY` bis separate Budgetgenehmigung; kein automatischer Paid-Fallback.

**Getrennte Arbeitsstränge:**
- **Strukturreform, 32 Schritte:** vorhandene Dateien ohne fachliche Änderung in den Zielbaum verschieben und alle Referenzen/Tests erhalten.
- **Fachliche Roadmap FE-00 bis FE-07 und vollständiges Endprodukt:** durch `PHASENPLAN.md/.json` und `MEASUREMENT_LEDGER.md/.json` bestimmt. Struktur-Fertigstellung ist kein wirtschaftliches GO.

## 2. Sicher abgeschlossene Strukturarbeiten

| Iteration | Inhalt | Commit |
|---:|---|---|
| 1 | Messprotokolle unter `docs/measurements/protocols/` | `ae6f96d` |
| 2 | Historische Messberichte | `a985816` |
| 3 | Architekturdokumentation | `de48ba3` |
| 4 | Betriebsdokumentation, alte `OPERATIONAL_CONFIG.md` als Weiterleitung erhalten | `ce4eb01` |
| 5 | Qualifikationsdokumentation unter `docs/qualification/` | `a69af18` |
| 6 | Fünf historische Audit-/Workorder-Dokumente unter `docs/development_history/` | `1b87387` |
| 7 | Aktuelle fachliche `docs/STATUS.md`, korrigierte Navigations-READMEs | `8b5c984` |
| **8a** | **Projekt-`.gitignore`: lokale Laufzeitdaten/Secrets schützen** | **DONE** `958d0b3` |
| **8b.1** | **Validierter Opt-in Run-Store** mit Lock und Kollisionsschutz, bestehenden Legacy-Defaults und synthetischen Tests; [Run-Store-Dokumentation](run_storage.md) | **DONE** `e8e904c`, 11 Dateien, +409/−13 Zeilen |
| **8b.2** | **Zukunftslayout und versionierte Identity-/Reader-/Writer-Migration** | **OPEN / NEXT**; gepinnte Dateien noch unverändert |
| **8b.3** | **Standard-Store qualifiziert von Legacy auf runs umstellen** | **OPEN** |

Die 32 Schritte mit den offenen Phasen B–F stehen vollständig in [restructure_32_step_plan.md](../architecture/restructure_32_step_plan.md).

**Letzte erfolgreich ausgeführte Offline-Regressionsbasis (8b.1, vom Nutzer nach Astra-Review getestet):** **83 Python-Tests PASS**, davon 15 neue Run-Store-Tests; **375 Rust-Tests** unter `crate_health` PASS (1 ignored); **377 Rust-Tests** unter `crate_health,txv1` PASS (1 ignored); beide Release-Builds PASS; Syntax-, Gitignore- und Pinned-Identity-Checks PASS; Clippy-Ausführung PASS. Astra meldet Differential-Clippy 38→38 und 5×12 erfolgreiche konkurrierende Allocations. Terminal-Nachweis `PROJECT0_8B1_PUSH_PASS`; Logs lokal unter `/tmp/project0-8b1-final.rmmPEa` (nicht im Git). Diese Ergebnisse belegen weder eine FE-01-Gesamtfreigabe noch aktuelle Mainnet-Providerkapazität. Qualifizierte vorherige Operational-Config-Baseline: `0782d8b` (368/370 Rust PASS, 65 Python PASS, beide Release-Builds PASS, Differential-Clippy 38→38). FE-01-WIP-Code `4c271cb` ist offline getestet, hat aber keine vollständige eigene Qualifikations-Evidence.

## 3. Der derzeitige fachliche Zustand — nicht als Strukturfortschritt behandeln

| Produktgate | Aktuelles Urteil |
|---|---|
| Gesamt / realisierter Gewinn | `FIRST_REALIZED_PROFIT_NOT_PROVEN` |
| LIVE / werttragende Submission | `DISABLED` / `DISABLED`; Rust verweigert normalen Live-Modus, econ4h benutzt `ObserveOnly` |
| FE-00 | `LOCAL_PASS` bei `NO_ECONOMIC_GO`; geprüfte historische Stichprobe **keine** aktuell ausführbare positive Opportunity qualifiziert; nicht „es existieren keine Chancen“ |
| FE-01 | `OPEN`, `p0-fe-qualify` WIP; keine aktuelle ProgramData-/State-Paritätsfreigabe |
| FE-02 | `OPEN`; voller aktueller Executable-Economics-/Exit-/Simulation-Nachweis fehlt |
| FE-03 | `OPEN`; gezieltes Production Shadow noch nicht qualifiziert |
| FE-04 | `PARTIAL_LOCAL`; Journal, Signaturbindung, finale Fee-/Wallet-Deltas teilweise lokal qualifiziert; Terminal-Approval, Produktions-Intent/Current, Exit und kumulative Verluste offen |
| FE-05 | `NOT_RUN`; Providerqualifikation und separates read-only Mainnet-Gate offen |
| FE-06 | `NOT_AUTHORIZED`; kein Value-Transfer |
| FE-07 | `NOT_RUN`; keine realisierten Gewinne |
| Provider / Master-Census | FREE_ONLY-Providerkapazität `UNKNOWN`; C0-E1-Master `NOT_QUALIFIED`, 4h/5h unvollständig |
| 5h Rohdaten-Replay / 72h Shadow | Replay `NOT_RUN / USER_OWNED`; 72h Shadow nicht gestartet |

Fachlich priorisiert der Ledger FE-01/Evidence, Provider-FREE_ONLY-Read-only-Qualifikation mit separater Freigabe, FE-02 vollständig positive candidate-specific Economics und FE-04 Runtime-Safety, anschließend FE-03/05, erst nach ausdrücklicher Autorisierung FE-06/07.

## 4. Verbindliche Terminal- und Liquidationssemantik

Das Produkt soll über Terminalparameter vollständig steuerbar sein; das spätere Frontend nutzt **denselben typisierten Backend-Vertrag**, keine zweite fachliche Klassifizierung. Zielparameter: `--duration` (10m, 1h, 5h, 24h), `--chain-verify off|index|strict` mit optionalem unabhängigem Crosscheck, `--capture-source logs|transactions`, `--report-types`, `--report-markers`, `--report-related-events`, `--report-view`, `--report-provenance`, `--format table|json|csv`, Ressourcen-/Provider-Limits sowie Resume. **Dies ist ein Zielvertrag; vollständige Implementierung ist nicht unterstellt.**

**Vier fachliche Liquidationsarten; sechs Instruktionsmarker:**
1. CLASSIC: `lending_account_liquidate`.
2. RECEIVERSHIP: `start_liquidation` / `end_liquidation`.
3. FORCED_DELEVERAGE: `start_deleverage` / `end_deleverage`.
4. BANKRUPTCY_SETTLEMENT: `lending_pool_handle_bankruptcy`.

Sechs Marker sind **keine sechs selbstständigen Gewinne**. Reportfilter für Arten, Marker und verwandte Nicht-Liquidationsereignisse verändern **nur die Anzeige**, niemals Raw-Capture, Decoder, Verifikationsscope, echte Economics oder Trading-MinProfit. Start-/Endpaare nicht doppelt zählen. Failed/UNKNOWN bleiben UNKNOWN. Shadow-only ist niemals bestätigte Chain-Liquidation oder realisierter Gewinner. `off` liefert keinen historischen Chain-Beweis, `index` ist nicht `strict`-kanonisch, und 429/fehlende Slots bleiben INCOMPLETE. Alle Longrun-/Proof-Gates bleiben bestehen.

## 5. Iteration 8a: Was tatsächlich gesichert wurde

`project_0/.gitignore` wurde auf `main` unter **`958d0b3`** committed. Sie ignoriert gezielt:
`/data/econ4h/`, `/data/market/`, `/data/shadow/`, `/data/c0/`, `/evidence/C0_E1_local_integration_20261008/` und die lokale Datei `/env`. Die globale Root-`.gitignore` schützt bereits `.env`, `env/`, private Schlüssel, `*.log`, SQLite und Rust-`target/`.

**Offene Punkte aus Fables Metadaten-Inventur** (nicht eigenmächtig beseitigen):
- Lokal ca. **75 GB** in `project_0/data/econ4h/` (22 Runs), weitere Läufe in `data/market/`. Keine Daten verschoben oder gelöscht.
- `project_0/env` hatte laut Inventur Dateimodus **644** und enthielt vermutlich vertrauliche Konfiguration; Inhalt niemals ausgeben. **Empfehlung:** lokal Rechte mit `chmod 600 project_0/env` einschränken. Diese Maßnahme wurde nicht als ausgeführt belegt.
- `docs/evidence/C0E1_TRANSPORT_FINISH_20261010/` ist nur lokal vorhanden (15 untracked JSON, 23 durch globale `*.log`-Regel ignorierte Logs). **Keine** eigenmächtige Aufnahme in Git, kein pauschales Unignore; Evidence-Entscheidung separat.
- Bestehende `docs/evidence/`-Artefakte und ihre Checksummen bleiben unverändert; `data/.gitkeep`, `exports/.gitkeep` und öffentliche `config/wallet.public.env` weiterhin getrackt.
- Root-`.gitignore` enthält historisch eine Empfehlung `git add .`; für Project_0 gilt das **ausdrückliche Verbot** pauschalen Stagings trotzdem.

**Die Commit-Nachricht von `958d0b3` ist irreführend** (`preserve restructure roadmap and handoff`), weil jener Commit tatsächlich **nur `project_0/.gitignore`** enthält. Die Roadmap und dieser Handoff wurden deshalb erst **nachträglich** in separaten Dokumentations-Commits angelegt. Historie nicht nachträglich umschreiben.

## 6. NÄCHSTE AUFGABE: Iteration 8b.2 — versionierte Zukunftslayout-/Verifier-Migration (OFFEN)

**Ist:** Iteration 8b.1 wurde mit Commit [`e8e904c`](https://github.com/sawyerma/darkbot/commit/e8e904cc5e350e2edfc382230c4189d34a4aa003) auf `main` gesichert: `scripts/run_store.py`, `scripts/test_run_store.py`, drei angepasste Launcher und Dokumentation. `P0_RUN_STORE=runs` ist **Opt-in**, `legacy` weiterhin Default. Neue IDs werden lokal Root-übergreifend per `flock` geschützt, `data/runs/` ist ignoriert. Aktuelles Dateiformat bleibt erhalten; `--run <id>` findet die neuen Runs noch nicht, explizite Pfade funktionieren.

**Ziel 8b.2:** Einen **versionierten und migrationsfähigen** Weg für `project_0/data/runs/<run_id>/{manifest.json,config.json,capture/,snapshots/,verification/,reports/,logs/}` ausarbeiten und schrittweise realisieren. **Keine bestehenden 75 GB Run-Rohdaten bewegen. Keine unversionierte Überschreibung von Verifier-/Report-Identitäten.**

**Vorgeschriebene Reihenfolge:**
1. Zuerst den tatsächlich neuesten `main` und das lokale Staging prüfen. [Run-Store-Inventur](run_storage.md), PHASENPLAN, Measurement-Ledger, Manifest-/Resume-/Verification-/Report-Identitätsketten im Code lesen.
2. **Vor jeder Änderung** eine prüfbare Gegenüberstellung der gepinnten `measurement_contract.py`, `audit_market_quick.py`, `market_report.py` und ihrer Einbindung in `verification_spec_id`, Checkpoints, Resume und Report-Datasets erstellen. Neu gepinnte Versionen erfordern eine **explizit versionierte** Schematransition; niemals den Identitätsalgorithmus historischer Artefakte rückwirkend ändern.
3. Legacy-V1-Reader/-Resume und neue V2-Layouts müssen nebeneinander funktionieren, einschließlich exakter Run-ID-Suche und expliziter Pfade; `capture_run_id` muss stabil bleiben, Verification- und Report-IDs getrennt. Neue Artefakte sind atomar und kollisionssicher, bestehende Raw-Evidence unveränderlich.
4. **Keine große Einmalmigration:** zuerst präzise Abhängigkeiten und konservative Teilpakete für 8b.2 festlegen. Nur rückwärtskompatible, getestete neue Writer-/Reader-Pfade opt-in aktivieren; eine Änderung gepinnter Dateien mit Re-Pinning erst nach eigenständiger geprüfter Entscheidung über Spec-Version, Beweis und Resume-Fallback.
5. Testmatrix: synthetischer End-to-End-Run in Legacy und Zukunftslayout, Offline-Resume unter alter Spec-ID, Hash-/Evidence-Parität, 83+ Python-Tests, 375/377 Rust-Tests, beide Release-Builds, Differential-Clippy und CLI-Äquivalenz, sofern entsprechende Codepfade betroffen sind. Fehler müssen fail-closed bleiben.
6. **Kein eigenständiger Commit/Push durch Astra/Fable**; nur die zum Teilauftrag gehörenden Pfade gezielt stagen und mit Bericht stoppen. Keine Mainnet-/LIVE-Aktion, kein Repo-weiter Cleanup und keine Iteration 8b.3 oder 09.

**Danach:** 8b.3 Standard-Store und Regression, dann 09 Cache, 10 Profile, 11 Beispiele, 12 Daten-/Exportpolitik; 13–20 Tests/Python; 21–25 Shell-Entry-Points; 26–29 Rust; 30–32 Abschluss. Details und exakte Statusspalten stehen in der [32-Schritte-Roadmap](../architecture/restructure_32_step_plan.md).

## 7. Rollen, Sicherheitsgrenzen und Abnahme

- **Fable/Claude Code:** implementiert pro Auftrag einen eng begrenzten Teilschritt und staged nur die genehmigten `project_0/`-Pfade; kein Commit, kein Push, keine Branches und keine Vorwegnahme des Folgeschritts.
- **ChatGPT:** unabhängig nach GitHub-/Quellcode-/Evidence-Stand prüfen, begründete Risiken, Terminal-Abnahmetests und eindeutige Freigabe oder Korrekturauftrag liefern.
- **Nutzer:** allein führt lokale Tests, Commit und `git push origin main` nach Freigabe durch. **Keine** eigenständigen RPC-/Mainnet-/LIVE-/Submission-Aufrufe oder bezahlte Ressourcen durch Agenten.

**Immer:** `bsc/**` und außerhalb `project_0/**` unangetastet; PHASENPLAN/LEDGER mitsamt JSON-Indizes und historische `docs/evidence/**` bei reinen Refactors unverändert; `FREE_ONLY`, LIVE und Submission gesperrt; kein `git add -A` oder `git add .`; kein `git clean`/Hard-Reset und keine Änderungen an lokalen historischen Rohdaten.

**Vor der nächsten Fable-Arbeit lokal prüfen:**

```bash
git fetch origin main
git rev-parse HEAD
git rev-parse origin/main
git status --short
git diff --cached --name-status
git diff --cached --check
```

**Bestätigte Offline-Testkommandos:**

```bash
python3 -m unittest discover -s project_0/scripts -p 'test_*.py'
cargo test --manifest-path project_0/native/Cargo.toml --locked --offline --workspace --features crate_health
cargo test --manifest-path project_0/native/Cargo.toml --locked --offline --workspace --features crate_health,txv1
```

Bei Rust-/Programmumzügen zusätzlich passende Release-Builds, gezielte CLI-/Fixture-/Replay-Äquivalenz, Format und Differential-Clippy. Keine Test-PASS erfinden; tatsächliche `NOT_RUN` nennen. Danach vollständiger Staged-Diff und unabhängiges Review. **Commit und Push erst durch den Nutzer.**

## 8. Neuen Chat exakt hier fortsetzen

> Öffne das GitHub-Repository `sawyerma/darkbot`, Branch `main`. Lies **zuerst** `project_0/docs/operations/restructure_handoff.md`, danach `project_0/docs/architecture/restructure_32_step_plan.md`, dann `project_0/docs/STATUS.md`, die neuesten Ledger-Einträge und relevante Rust-/Python-Dateien. Das **Strukturprojekt** ist bei **Iteration 8b.2**: 1–7 DONE, 8a DONE, 8b.1 DONE (`e8e904c`) und auf GitHub, 8b.2/8b.3 offen. Die **fachliche** FE-00–FE-07-Roadmap läuft unabhängig. Verifiziere Remote-HEAD und lokales Staging, beginne keine unbekannte Folgetätigkeit, keine LIVE-/Mainnet-Aktion. Nächste Astra-/Fable-Aufgabe: versionierte, Legacy-kompatible Run-Layout-Migration mit ausdrücklicher Prüfung der Hash-Pins und der historischen Resume-Identitäten gemäß Abschnitt 6. Arbeite in genau einem abgenommenen Teilschritt und stoppe vor Commit/Push.
