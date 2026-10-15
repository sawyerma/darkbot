# Project_0 — 32-Iterationen-Strukturreform (verbindliches Fortschrittsregister)

**Stand:** 2026-10-15. **Baseline:** `main` auf `958d0b3d862d964866fd2646cd53b44105263673` vor Anlage dieser Datei.
**Reformstatus:** Phase A 7/7; 8a `958d0b3`, 8b.1 `e8e904c`, Wallet-Lock-Fix `6ec4483`/`218710b`; **Iterationen 8b.2–15 umgesetzt und mit `46be33c` auf `main` gepusht** (Claudes Offline-Tests; eine unabhängige Abnahme ist damit nicht behauptet). **Iterationen 16–29 im Arbeitsbaum umgesetzt und gezielt geprüft (nicht committed); 30–32 offen.** Die vollständige Gesamtqualifikation erfolgt in Iteration 32.
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
| **08** | **B: Daten/Config** | **Run-Ablage `data/runs/<run_id>/`** inkl. Schutzprüfung vor jeder Migration | **8a DONE** `958d0b3`; **8b.1 DONE** `e8e904c`; **8b.2 UMGESETZT** (Layout V2 in Rust/Python/Shell, versionierter Re-Pin Option A) und **8b.3 UMGESETZT** (`data/runs` V2 Standard für econ4h), beide `46be33c` |
| 09 | B | `data/cache/` und temporäre Arbeitsdaten | **UMGESETZT** `46be33c` |
| 10 | B | `config/profiles/` für geprüfte, typisierte Laufzeitprofile | **UMGESETZT** `46be33c` |
| 11 | B | `config/examples/` für bereinigte Beispiele | **UMGESETZT** `46be33c` |
| 12 | B | Daten-/Export-Policy und finale Ignore-/Ablage-Grenzen | **UMGESETZT** `46be33c` |
| 13 | C: Python/Tests | `tests/python/` | **UMGESETZT** `46be33c` |
| 14 | C | `tests/integration/` | **UMGESETZT** `46be33c` |
| 15 | C | `tests/regression/` und `tests/fixtures/` | **UMGESETZT** `46be33c` |
| 16 | C | Python-CLI-Komponenten unter `operations/cli/` | **UMGESETZT (Arbeitsbaum)** |
| 17 | C | Capture-Programme unter `operations/capture/` | **UMGESETZT (Arbeitsbaum)** |
| 18 | C | Verifikationsprogramme unter `operations/verification/` | **UMGESETZT (Arbeitsbaum)** |
| 19 | C | Reporting unter `operations/reporting/` | **UMGESETZT (Arbeitsbaum)** |
| 20 | C | Qualification/Preflight unter `operations/qualification/` | **UMGESETZT (Arbeitsbaum)** |
| 21 | D: Shell/CLI | `scripts/capture.sh`, Kompatibilität zu bisherigen Aufrufen | **UMGESETZT (Arbeitsbaum)** |
| 22 | D | `scripts/smoke.sh` | **UMGESETZT (Arbeitsbaum)** |
| 23 | D | `scripts/verify.sh` | **UMGESETZT (Arbeitsbaum)** |
| 24 | D | `scripts/report.sh` | **UMGESETZT (Arbeitsbaum)** |
| 25 | D | `scripts/preflight.sh` | **UMGESETZT (Arbeitsbaum)** |
| 26 | E: Rust | `native/crates/darkbot-common/` → `native/crates/common/` | **UMGESETZT (Arbeitsbaum)** |
| 27 | E | `native/crates/darkbot-solana/` → `native/crates/solana/` | **UMGESETZT (Arbeitsbaum)** |
| 28 | E | `native/crates/darkbot-project0/` → `native/crates/liquidation/` | **UMGESETZT (Arbeitsbaum)** |
| 29 | E | `native/crates/darkbot-runner/` → `native/crates/runtime/`; große Module kontrolliert teilen | **UMGESETZT (Arbeitsbaum)** |
| 30 | F: Abschluss | Nach Verbrauchernachweis ungenutzte Dateien/Ordner bereinigen | OPEN |
| 31 | F | Alle README-/Navigations-/Kompatibilitätsverweise finalisieren | OPEN |
| 32 | F | Gesamte Offline-/CLI-/Build-/Replay-Endqualifikation | OPEN |

**Phasenbilanz:** A **7/7 DONE**; B **5/5** (`46be33c`); C **8/8** (13–15 `46be33c`, 16–20 Arbeitsbaum); D **5/5** (Arbeitsbaum); E **4/4** (Arbeitsbaum); F **0/3**.

## Iteration 8 – Unteriterationen und nachgewiesener Stand (2026-10-15)

| Unteriter. | Inhalt | Abnahme / Git |
|---|---|---|
| **8a** | Laufzeitdaten- und Secret-Ignore-Regeln, ohne historische Evidence auszuschließen | **DONE** `958d0b3` |
| **8b.1** | [Run-Store-Vertrag](../operations/run_storage.md) als Opt-in über `P0_RUN_STORE=runs`, zentrale Pfadvalidierung, `data/runs/` ignoriert, Cross-Root-Kollisionsschutz via lokalem `flock`, alte Standardpfade unverändert | **DONE** `e8e904cc5e350e2edfc382230c4189d34a4aa003`; eigenständiger Nutzer-Abnahmelauf `PROJECT0_8B1_PUSH_PASS` |
| **8b.2** | Layout V2 (`manifest.json`, `config.json`, `capture/`, `snapshots/`, `verification/`, `reports/`, `logs/`); Rust `run_layout.rs`, `run_store.py` als gepinnter Layout-Vertrag; Spec/3, INDEX/3 (`index_v3`), Dataset NormalizedReport/3; `RESUME_REQUIRES_ARCHIVED_IMPLEMENTATION`; V1 byte-identisch zu `908b14c` ([Entscheidung](run_layout_versioning.md)) | **UMGESETZT** `46be33c` |
| **8b.3** | `data/runs/` + Layout V2 Standard für econ4h; `P0_RUN_STORE=legacy` und explizite Roots behalten V1; C0-MO bleibt `data/market` V1 (kein Pre-Capture-Manifest) | **UMGESETZT** `46be33c` |

**Iterationen 9–15 (gepusht in `46be33c`):**
- **09** `data/cache/` (ignoriert): Gap-Proxy-Arbeitsdaten `data/cache/gap_proxy/`; Secret-Temp-Dateien bleiben bewusst im System-TMPDIR, run-gebundene Verifier-Caches bleiben im Run ([Datenpolitik](../operations/data_policy.md)).
- **10** `config/profiles/{smoke,standard,performance,custom}.json` (Schema `Project0/RuntimeProfile/1`) als einzige Quelle der Profilwerte, per `include_str!` in `runtime_config.rs` eingebettet; Präzedenz unverändert; Endpunkte/LIVE/Submission nicht ausdrückbar.
- **11** `config/examples/` (Override, Endpoint-Vererbung, nicht bestehende FREE_ONLY-Providerqualifikation), gegen echte Rust- und Python-Parser getestet.
- **12** [Daten-/Export-Policy](../operations/data_policy.md): Klassen, Schreiber, Ignore-Grenzen, Aufbewahrungs-/Löschgrenzen (nichts gelöscht); `c0_census.py --out` arbeitsverzeichnisunabhängig.
- **13–15** `tests/{python,integration,regression,fixtures}/` ([Testkarte](../../tests/README.md)); Kompatibilität `scripts/test_suite.py` für den alten Discover-Befehl; `verify.sh --self-test` unverändert 29 Tests.

**8b.1-Tests (auf dem Mac durch Nutzer nach Astra-Review bestätigt):** Python **83/83 PASS** (davon 15 neue Run-Store-Tests), Rust `crate_health` **375 PASS / 1 ignored**, Rust `crate_health,txv1` **377 PASS / 1 ignored**, beide Release-Builds PASS, Clippy-Aufruf PASS, Git-/Syntax-/Pinned-Identity-Checks PASS. Astra meldete den Differential-Clippy-Vergleich **38→38** und den parallelen Kollisions-Stresstest 5×12 Prozesse; diese beiden Zusatzangaben stammen aus Astras Bericht. Testlog `/tmp/project0-8b1-final.rmmPEa` **nur lokal**, nicht im Git-Repository.

**Aktuelle fachliche Grenzen:** Neue econ4h-Runs entstehen in `data/runs/<id>` (Layout V2); historische Runs bleiben unverändert am alten Ort und lesbar. Neue Verifikationen erhalten neue, versionierte Spec-/Checkpoint-/Dataset-IDs; alte bleiben über ihre Digests prüfbar, ein Resume alter Versuche verlangt die archivierte Implementierung. Echte V2-Captures wurden **nicht** ausgeführt (keine Mainnet-Freigabe); V2 ist über Rust-Unit-Tests der Schreiber und synthetische Launcher-/Reader-End-to-End-Tests qualifiziert. Das Lock garantiert Cross-Root-Serialisierung nur auf dem lokalen unterstützten Dateisystem. Keine Mainnet-/LIVE-/Wirtschaftlichkeitsfreigabe.

**Iterationen 16–29 (Arbeitsbaum, gezielt geprüft):**
- **16–20** Python-Paket `operations/` (`cli/`, `capture/`, `verification/`, `reporting/`, `qualification/`) mit Paket-Imports; alle Module laufen als Datei, als Modul (`python3 -m operations...`) und über die alten `scripts/<modul>.py`-Aufrufe (dünne Weiterleitungen über `scripts/_compat.py`).
- **21–25** Shell-Einstiegspunkte `scripts/capture.sh` (econ4h, econ4h-launcher, market, shadow; gemeinsamer Env- und Signer-Gate statt zweier identischer Kopien), `smoke.sh`, `verify.sh`, `report.sh`, `preflight.sh`; alte Namen leiten unverändert weiter.
- **26–29** Crate-Ordner `common/`, `solana/`, `liquidation/`, `runtime/` (Cargo-Paketnamen unverändert); Tests aus `market_observer.rs` und `main.rs` in eigene Dateien, Shadow-Hilfsfunktionen aus `main.rs` in `runtime/src/longtail/{accounts,candidates}.rs`. Der 3 260-Zeilen-Rumpf `run_shadow` bleibt unverändert (keine Ausführungsänderung ohne Gesamtqualifikation).

**Hinweise für 30–32:** Kompatibilitäts-Weiterleitungen in `scripts/` (alte Shell-Namen, `*.py`-Weiterleitungen, `test_suite.py`) per Verbrauchernachweis prüfen; `run_layout.rs` und `run_store.ARTIFACTS` müssen identisch bleiben (Tests pinnen beide); gepinnte Dateien (`run_store.py`, `measurement_contract.py`, `audit_market_quick.py`, `market_report.py`) erzeugen bei jeder Änderung neue Verifikationsidentitäten; Python-Leser erwarten Release-Binaries unter `native/target/release/`; der Binärbau ohne Feature `crate_health` schlug schon vor der Reform fehl (`bank_filters` u. a.), alle Launcher bauen mit `crate_health`; `cargo fmt --check` über den Workspace scheitert an Alt-Hunks (u. a. strict-gepinntes `market_truth.rs`).

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
