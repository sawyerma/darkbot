# Project_0 — 32-Iterationen-Strukturreform (verbindliches Fortschrittsregister)

**Stand:** 2026-10-15. **Baseline:** `main` auf `958d0b3d862d964866fd2646cd53b44105263673` vor Anlage dieser Datei.
**Reformstatus:** **Phase A 7/7 abgeschlossen; Iteration 8a (`958d0b3`) und 8b.1 (`e8e904cc5e350e2edfc382230c4189d34a4aa003`) auf `main` gesichert. Iteration 8b.2 ist NÄCHSTER SCHRITT, 8b.3 noch offen.** Streng gezählt sind **7 von 32 Hauptiterationen vollständig abgeschlossen**; Iteration 8 bleibt PARTIAL, bis 8b.2 und 8b.3 qualifiziert sind.
**Aktueller Wiederaufnahme-Handoff:** [../operations/restructure_handoff.md](../operations/restructure_handoff.md).

## Ziel und Abgrenzung

Die vorhandene vollständige Rust-/Python-Architektur des Solana-/Marginfi-Liquidationssystems wird in eine wartbare, professionelle Verzeichnisstruktur überführt, ohne die fachlichen Abläufe, Messungen, Sicherheitsgates oder bestehende CLI-Bedienbarkeit zu verlieren. **Das ist ein Strukturplan**, nicht der Produkt-/Wirtschaftlichkeits-Phasenplan und keine LIVE-Freigabe. Die kanonischen Produktvorgaben liegen weiterhin in [PHASENPLAN.md](../PHASENPLAN.md), [PHASENPLAN.json](../PHASENPLAN.json), [MEASUREMENT_LEDGER.md](../MEASUREMENT_LEDGER.md) und [MEASUREMENT_LEDGER.json](../MEASUREMENT_LEDGER.json).

Die Reihenfolge ist absichtlich kleinteilig: jeweils Inventur, genau ein Migrationsteil, gezielte Regression, unabhängige Prüfung, **nur vom Benutzer** durchgeführter Commit/Push. Große Codeänderungen können zusätzliche Unteriterationen benötigen. 32 bedeutet **nicht**, dass die Produktreife nach 32 Schritten automatisch erreicht ist.

## Fortschrittsregister

| Nr. | Phase | Arbeitseinheit | Stand / Commit |
|---:|:---:|---|---|
| 01 | A: Dokumentation | Messprotokolle unter `docs/measurements/protocols/` | **DONE** `ae6f96d` |
| 02 | A | Historische Messberichte unter `docs/measurements/historical_reports/` | **DONE** `a985816` |
| 03 | A | Architekturdokumentation unter `docs/architecture/` | **DONE** `de48ba3` |
| 04 | A | Betriebsdokumentation unter `docs/operations/`; altes `docs/OPERATIONAL_CONFIG.md` als Weiterleitung | **DONE** `ce4eb01` |
| 05 | A | Qualifikationsdokumentation unter `docs/qualification/` | **DONE** `a69af18` |
| 06 | A | Historische Handoffs/Audits unter `docs/development_history/` | **DONE** `1b87387` |
| 07 | A | Aktueller Status `docs/STATUS.md`, Dokumentationsnavigation, historischer Audit-Handoff | **DONE** `8b5c984` |
| **08** | **B: Daten/Config** | **Run-Ablage `data/runs/<run_id>/`** inkl. Schutzprüfung vor jeder Migration | **PARTIAL:** **8a DONE** `958d0b3`; **8b.1 DONE** `e8e904c` (validierter Opt-in-Run-Store, Legacy-Defaults, 15 neue Tests); **8b.2 NEXT** (versionierte Layout-/Verifier-Migration), **8b.3 OPEN** (Defaults umstellen) |
| 09 | B | `data/cache/` und temporäre Arbeitsdaten | OPEN |
| 10 | B | `config/profiles/` für geprüfte, typisierte Laufzeitprofile | OPEN |
| 11 | B | `config/examples/` für bereinigte Beispiele | OPEN |
| 12 | B | Daten-/Export-Policy und finale Ignore-/Ablage-Grenzen | OPEN |
| 13 | C: Python/Tests | `tests/python/` | OPEN |
| 14 | C | `tests/integration/` | OPEN |
| 15 | C | `tests/regression/` und `tests/fixtures/` | OPEN |
| 16 | C | Python-CLI-Komponenten unter `operations/cli/` | OPEN |
| 17 | C | Capture-Programme unter `operations/capture/` | OPEN |
| 18 | C | Verifikationsprogramme unter `operations/verification/` | OPEN |
| 19 | C | Reporting unter `operations/reporting/` | OPEN |
| 20 | C | Qualification/Preflight unter `operations/qualification/` | OPEN |
| 21 | D: Shell/CLI | `scripts/capture.sh`, Kompatibilität zu bisherigen Aufrufen | OPEN |
| 22 | D | `scripts/smoke.sh` | OPEN |
| 23 | D | `scripts/verify.sh` | OPEN |
| 24 | D | `scripts/report.sh` | OPEN |
| 25 | D | `scripts/preflight.sh` | OPEN |
| 26 | E: Rust | `native/crates/darkbot-common/` → `native/crates/common/` | OPEN |
| 27 | E | `native/crates/darkbot-solana/` → `native/crates/solana/` | OPEN |
| 28 | E | `native/crates/darkbot-project0/` → `native/crates/liquidation/` | OPEN |
| 29 | E | `native/crates/darkbot-runner/` → `native/crates/runtime/`; große Module kontrolliert teilen | OPEN |
| 30 | F: Abschluss | Nach Verbrauchernachweis ungenutzte Dateien/Ordner bereinigen | OPEN |
| 31 | F | Alle README-/Navigations-/Kompatibilitätsverweise finalisieren | OPEN |
| 32 | F | Gesamte Offline-/CLI-/Build-/Replay-Endqualifikation | OPEN |

**Phasenbilanz:** A **7/7 DONE**; B **0/5 vollständig** (8a und 8b.1 DONE, 8b.2/8b.3 offen); C **0/8**; D **0/5**; E **0/4**; F **0/3**.

## Iteration 8 – Unteriterationen und nachgewiesener Stand (2026-10-15)

| Unteriter. | Inhalt | Abnahme / Git |
|---|---|---|
| **8a** | Laufzeitdaten- und Secret-Ignore-Regeln, ohne historische Evidence auszuschließen | **DONE** `958d0b3` |
| **8b.1** | [Run-Store-Vertrag](../operations/run_storage.md) als Opt-in über `P0_RUN_STORE=runs`, zentrale Pfadvalidierung, `data/runs/` ignoriert, Cross-Root-Kollisionsschutz via lokalem `flock`, alte Standardpfade unverändert | **DONE** `e8e904cc5e350e2edfc382230c4189d34a4aa003`; eigenständiger Nutzer-Abnahmelauf `PROJECT0_8B1_PUSH_PASS` |
| **8b.2** | Zukunftslayout und Reader/Writer-Migration unter **expliziter versionierter** Spec-/Checkpoint-/Report-Identitätsstrategie, Legacy-Reader-Kompatibilität | **OPEN / NEXT**; gepinnte `measurement_contract.py`, `audit_market_quick.py` und `market_report.py` in 8b.1 unverändert |
| **8b.3** | Qualifizierter Wechsel des Standard-Stores auf `data/runs/` bei unveränderten historischen Läufen und CLI-Kompatibilität | **OPEN**; nicht vor 8b.2 |

**8b.1-Tests (auf dem Mac durch Nutzer nach Astra-Review bestätigt):** Python **83/83 PASS** (davon 15 neue Run-Store-Tests), Rust `crate_health` **375 PASS / 1 ignored**, Rust `crate_health,txv1` **377 PASS / 1 ignored**, beide Release-Builds PASS, Clippy-Aufruf PASS, Git-/Syntax-/Pinned-Identity-Checks PASS. Astra meldete den Differential-Clippy-Vergleich **38→38** und den parallelen Kollisions-Stresstest 5×12 Prozesse; diese beiden Zusatzangaben stammen aus Astras Bericht. Testlog `/tmp/project0-8b1-final.rmmPEa` **nur lokal**, nicht im Git-Repository.

**Aktuelle fachliche Grenzen:** `P0_RUN_STORE` bleibt standardmäßig `legacy`. Explizite Pfade zu `data/runs/<id>` funktionieren mit den vorhandenen Lesern; bloßes `--run <id>` sucht diesen Store **noch nicht**. Das aktuelle Run-Dateiformat wurde **nicht** umstrukturiert. Das Lock garantiert Cross-Root-Serialisierung nur auf dem lokalen unterstützten Dateisystem. Keine Mainnet-/LIVE-/Wirtschaftlichkeitsfreigabe.

## Geplanter Zielbaum (nicht mit dem Ist-Zustand verwechseln)

```text
project_0/
├── README.md
├── .env.example
├── .gitignore
├── config/
│   ├── profiles/
│   ├── examples/
│   └── wallet.public.env
├── native/
│   ├── Cargo.toml
│   ├── Cargo.lock
│   └── crates/
│       ├── common/              (vormals darkbot-common)
│       ├── solana/              (vormals darkbot-solana)
│       ├── liquidation/         (vormals darkbot-project0)
│       └── runtime/             (vormals darkbot-runner)
├── operations/
│   ├── cli/
│   ├── capture/
│   ├── verification/
│   ├── reporting/
│   └── qualification/
├── scripts/
│   ├── capture.sh
│   ├── smoke.sh
│   ├── verify.sh
│   ├── report.sh
│   └── preflight.sh
├── tests/
│   ├── python/
│   ├── integration/
│   ├── regression/
│   └── fixtures/
├── docs/
│   ├── README.md
│   ├── STATUS.md
│   ├── PHASENPLAN.md / PHASENPLAN.json
│   ├── MEASUREMENT_LEDGER.md / MEASUREMENT_LEDGER.json
│   ├── architecture/
│   ├── operations/
│   ├── qualification/
│   ├── measurements/{protocols,historical_reports}/
│   ├── development_history/
│   └── evidence/                (historisch, unverändert)
├── data/
│   ├── runs/<run_id>/{manifest.json,config.json,capture,snapshots,verification,reports,logs}/
│   └── cache/
└── exports/
```

Der Baum ist eine **Zielabsicht**. Neue Pfade dürfen bestehende Rust-/Python-/Shell-Consumers nicht stillschweigend brechen. Jede Cargo-Crate behält korrekte `Cargo.toml` und `src/`; Umbenennung des Ordners bedeutet nicht automatisch Änderung des Cargo-Paketnamens. Große Rohdaten werden nicht ungefragt migriert.

## Verbindliche Ablauf- und Sicherheitsregeln

1. **Nur `project_0/**` verändern**; niemals `bsc/**`, andere Projekte, historische Evidence oder user-eigene Rohdaten berühren. Bei der reinen Strukturreform bleiben `PHASENPLAN.md/.json` und `MEASUREMENT_LEDGER.md/.json` unverändert.
2. Vorher lokales `HEAD`, `origin/main`, Index, Dirty Tree, Referenzen, Pfad- und Konfigverbraucher erfassen; kein `git clean`, `git reset --hard` oder ungefragtes Verschieben großer Daten.
3. Fable bearbeitet genau den freigegebenen Teilschritt, staged ausschließlich die erlaubten Pfade und **stoppt**. **Kein Commit, kein Push, keine neuen Branches durch Agenten**; Commit und Push führt der Nutzer nach unabhängiger Abnahme aus.
4. `git add -A`, `git add .` oder broad staging **verboten**, insbesondere wegen vieler lokaler Rohdaten und BSC-Materialien.
5. Dokumentationsänderungen: Links, Referenzen, Originalblobs, Ledger-/Evidence-Integrität prüfen; Rust/Python/CLI-Änderungen: passende **volle** Offline-Regressionsmatrizen, Builds, Format, Clippy-Differenz und Ergebnisparität.
6. `FREE_ONLY`, `LIVE=DISABLED`, `submission=DISABLED` bleiben erhalten. Keine neuen Mainnet-Reads/Captures, historischen 5h-Replays, Providerkäufe oder value-bearing Transaktionen ohne jeweils eigenständige Freigabe.
7. Nach jedem bestätigten Push dieses Register und [den aktiven Handoff](../operations/restructure_handoff.md) mit Commit, Testnachweisen und nächstem Schritt aktualisieren. Ein Refactoring setzt kein fachliches Wirtschaftlichkeitsgate auf PASS.

## Referenzen und Startpunkt für neue Chats

- **Zuerst:** [Aktiver Struktur-Handoff](../operations/restructure_handoff.md).
- **Danach:** [STATUS.md](../STATUS.md) — fachlicher Ist-Stand des Bots.
- [PHASENPLAN.md](../PHASENPLAN.md), [PHASENPLAN.json](../PHASENPLAN.json) — verbindliches vollständiges Produktziel, Terminalsteuerung und FE-Priorisierung.
- [MEASUREMENT_LEDGER.md](../MEASUREMENT_LEDGER.md), [MEASUREMENT_LEDGER.json](../MEASUREMENT_LEDGER.json) — tatsächliche Tests, Messungen und Freigaben.
- [evidence/](../evidence/) — historische unveränderliche Nachweise.

**Nicht verwechseln:** 32 Strukturiterationen ≠ FE-00–FE-07 ≠ wirtschaftlicher GO ≠ LIVE- oder Mainnet-Autorisierung.
