# Project_0 — implementierte Zielstruktur, 2026-10-16

Status: **historischer Abschlussbericht der Runtime-Refaktorierung**; der Code wurde mit `8bd462e` gepusht. Aussagen über „uncommitted“, lokale Stashes und damalige HEADs sind zeitgebundene Originalnotizen. **Aktuelle Weiterentwicklung:** binäre Persistenz `4a27a1c` (PARTIAL) mit [Implementierungsnachweis](../development_history/persist_binary_completion_20261016.md) und [aktuellem Status](../STATUS.md). Produkt- und Mainnet-Gates bleiben unverändert.
Der ausdrückliche Implementierungsauftrag vom 2026-10-16 ersetzt die frühere Beschränkung auf A0.

## Tatsächliche Struktur

```text
project_0/
├── README.md, .env.example, .gitignore
├── native/                    # ein Workspace, Cargo-Paketnamen unverändert
│   ├── Cargo.toml, Cargo.lock
│   └── crates/
│       ├── common/            # bestehende Infrastruktur
│       ├── solana/            # ingress, einziger kanonischer State, tx, submit
│       ├── liquidation/       # unveränderte Geschäfts-/Protokolllogik
│       └── runtime/src/
│           ├── main.rs        # 7 Zeilen: Einstieg + Fehlerstatus
│           ├── lib.rs         # Domains und öffentliche API-Reexports
│           ├── orchestration/
│           │   ├── startup.rs, state_owner.rs
│           │   ├── remote_workers.rs, winner_tracker.rs, fe_qualification.rs
│           │   └── state_owner/
│           │       ├── setup.rs, ingress.rs, lifecycle.rs
│           │       ├── remote_apply.rs, dispatch.rs, sweep.rs
│           │       ├── events.rs, detect.rs, telemetry.rs, shutdown.rs
│           │       └── accounts.rs, candidates.rs, metrics.rs, tests.rs
│           ├── execution/
│           │   ├── builder.rs, pipeline.rs, pending.rs, route.rs
│           │   ├── lookup_tables.rs, observe_only.rs, one_shot.rs
│           │   └── one_shot/{receipts.rs,transport.rs}
│           ├── market/
│           │   ├── observer.rs, configuration.rs, time.rs, decoding.rs
│           │   ├── enrichment.rs, sink.rs, core.rs, summary.rs
│           │   ├── fetch.rs, capture.rs, deployment.rs, winner.rs
│           │   └── observer_tests.rs, market_{gap,reconcile,truth}_tests.rs
│           ├── analytics/
│           │   ├── econ.rs, econ_canonical.rs, econ_tests.rs
│           │   ├── market_report.rs, measurement.rs, record.rs
│           │   └── shadow_ledger.rs, fixture.rs
│           ├── market_{gap,reconcile,truth,verification}.rs  # Identitätspins
│           ├── run_layout.rs  # gemeinsamer persistierter Vertrag
│           └── bin/           # bisherige Programme und Release-Namen
├── operations/{cli,capture,verification,reporting,qualification}/
├── scripts/{capture,smoke,verify,report,preflight,test}.sh
├── config/{profiles,examples}/, config/wallet.public.env
├── tests/{python,integration,regression,fixtures}/
├── docs/{architecture,operations,qualification,measurements,development_history,evidence}/
├── data/                      # vorhandene lokale Laufzeitdaten bleiben am Originalpfad
├── exports/
└── delete/                    # bestehende, inaktive Wiederherstellungsquarantäne
```

Der vollständige Dateibaum einschließlich unveränderter Quellen und lokaler Datenwurzel-Namen steht in
[actual_project_tree.txt](../evidence/RUNTIME_MODULARIZATION_20261016/actual_project_tree.txt).
Die [Modul-/Consumer-Inventur](../evidence/RUNTIME_MODULARIZATION_20261016/module_inventory.json)
ordnet 157 aktive Rust-/Python-/Shell-/Cargo-Dateien zu und enthält Funktionen, Imports, Features, Includes,
Konfigurationsschlüssel und Cargo-Abhängigkeiten. Quarantäne, generierte Dateien und historische Evidence sind keine aktiven Module.

## Ownership und Integration

`run_shadow()` besitzt genau einen `ShadowOwner`. Dessen `st` bleibt ein `solana::state::StateCore`;
der temporäre Bootstrap-Snapshot ist keine zweite State-Implementierung. Die Owner-Schleife ruft Lifecycle,
Worker-Ergebnisse, den bisherigen Drain-Gate, Reconcile-Dispatch, Sweep, Events, Detection und Telemetrie in der
ursprünglichen Reihenfolge auf. `OwnerStep::Continue` und die Detection-Rückgabe erhalten bisherige frühe
`continue`-Pfade. Recovery-Fetch bleibt statisch gebunden; `IngressSessions` startet und ersetzt dieselben WSS-Sessions.
Keine zusätzlichen Channels, Locks, Runtime-Threads oder RPC-Aufrufe. Zähler sind in `OwnerMetrics` gruppiert,
werden aber an denselben Ereignisstellen geschrieben. Zeitmessung und der bisherige externe MT0-Writer bleiben erhalten.

`execution` besitzt die vorhandene Pipeline, Builder-/Route-Integration, Pending- und One-Shot-Sicherheit sowie
`ObserveOnly`. Letzteres war zuvor in `econ.rs` eingebettet; seine Submission-Sperre ist unverändert.
`market` besitzt unabhängige Beobachtung und Chain-Nachweis; `analytics/market_report.rs` rendert fertige
Berichte ohne Trading- oder Capture-Entscheidungen. `liquidation` und `solana/submit` bleiben fachliche Autoritäten.

Die bisherigen Rust-Pfade (`darkbot_runner::market_observer`, `econ`, `pipeline`, `one_shot` usw.) bleiben durch
Reexports auf **dieselbe** Implementierung verfügbar. Das sind keine Wrapper-Runtimes oder duplizierten Engines.
Cargo-Paketnamen, vier Crates, Feature-Gates und Release-Binary-Pfade bleiben erhalten. Python-Betriebspaket,
sechs Shell-Einstiege und versionierter Backend-Konfigurationsvertrag bleiben getrennt und integriert.

## Vollständige Runtime-Migration

Pfade relativ zu `native/crates/runtime/src/`; nicht aufgeführte Dateien bleiben an ihrem bisherigen Platz.

| Altpfad | Tatsächliche Zielpfade |
|---|---|
| `builder.rs` | `execution/builder.rs` |
| `econ.rs` | `analytics/econ.rs`, `execution/observe_only.rs` |
| `econ_canonical.rs` | `analytics/econ_canonical.rs` |
| `econ_tests.rs` | `analytics/econ_tests.rs` |
| `fe_qualification.rs` | `orchestration/fe_qualification.rs` |
| `fixture.rs` | `analytics/fixture.rs` |
| `longtail/accounts.rs` | `orchestration/state_owner/accounts.rs` |
| `longtail/candidates.rs` | `orchestration/state_owner/candidates.rs` |
| `lookup_tables.rs` | `execution/lookup_tables.rs` |
| `main_tests.rs` | `orchestration/state_owner/tests.rs` |
| `market_gap_tests.rs` | `market/market_gap_tests.rs` |
| `market_observer.rs` | `market/capture.rs`, `market/configuration.rs`, `market/core.rs`, `market/decoding.rs`, `market/enrichment.rs`, `market/fetch.rs`, `market/observer.rs`, `market/sink.rs`, `market/summary.rs`, `market/time.rs`, `analytics/market_report.rs` |
| `market_observer_tests.rs` | `market/observer_tests.rs` |
| `market_paths.rs` | `market/deployment.rs` |
| `market_reconcile_tests.rs` | `market/market_reconcile_tests.rs` |
| `market_truth_tests.rs` | `market/market_truth_tests.rs` |
| `measurement.rs` | `analytics/measurement.rs` |
| `one_shot.rs` | `execution/one_shot.rs` |
| `one_shot/receipts.rs` | `execution/one_shot/receipts.rs` |
| `one_shot/transport.rs` | `execution/one_shot/transport.rs` |
| `pending.rs` | `execution/pending.rs` |
| `pipeline.rs` | `execution/pipeline.rs` |
| `record.rs` | `analytics/record.rs` |
| `remote_workers.rs` | `orchestration/remote_workers.rs` |
| `route.rs` | `execution/route.rs` |
| `shadow_ledger.rs` | `analytics/shadow_ledger.rs` |
| `winner.rs` | `market/winner.rs` |
| `winner_tracker.rs` | `orchestration/winner_tracker.rs` |
| `main.rs` | `main.rs`, `orchestration/startup.rs`, `orchestration/state_owner.rs`, `orchestration/state_owner/detect.rs`, `orchestration/state_owner/dispatch.rs`, `orchestration/state_owner/events.rs`, `orchestration/state_owner/ingress.rs`, `orchestration/state_owner/lifecycle.rs`, `orchestration/state_owner/metrics.rs`, `orchestration/state_owner/remote_apply.rs`, `orchestration/state_owner/setup.rs`, `orchestration/state_owner/shutdown.rs`, `orchestration/state_owner/sweep.rs`, `orchestration/state_owner/telemetry.rs` |
| `market_gap.rs` | `market_gap.rs` |
| `market_reconcile.rs` | `market_reconcile.rs` |
| `market_truth.rs` | `market_truth.rs` |
| `market_verification.rs` | `market_verification.rs` |
| `run_layout.rs` | `run_layout.rs` |
| `lib.rs` | `lib.rs` |

Maschinenlesbare Liste: [migration.json](../evidence/RUNTIME_MODULARIZATION_20261016/migration.json).

## Builds, Regressionen und Parität

| Prüfung | Vorher | Nachher |
|---|---:|---:|
| Rust Workspace, `crate_health` | 382 PASS, 1 ignored | 382 PASS, 1 ignored |
| Rust Workspace, `crate_health,txv1` | 384 PASS, 1 ignored | 384 PASS, 1 ignored |
| Workspace ohne Features, offline/locked | PASS | PASS |
| Release-Build, beide Feature-Kombinationen | PASS | PASS |
| Python, vollständige Discovery | 103 PASS, 0 SKIP | 103 PASS, 0 SKIP |
| Verifier-Selbsttest | 29 PASS | 29 PASS |
| Clippy, health / txv1, alle Targets | 72 / 73 Diagnosemeldungen | 72 / 73, keine zusätzliche Diagnose |
| Effektive Config, vier Profile; CLI-Hilfe und ungültige Option | Referenzbinary | identische Ausgaben und Exit-Codes |
| Normaler LIVE-Start | Exit 2 vor Schlüssel/RPC | identischer Exit 2 vor Schlüssel/RPC |
| Shell-Syntax, sechs Einstiege | PASS | PASS |
| Shell-Hilfe aus fremdem cwd, fünf nicht ausführende Einstiege | PASS | PASS |
| Python-Syntax / eingebettete Rust-Dateipfade | PASS | PASS |
| Chain-/Liquidation-/Config-/Cargo.lock-Quellen | Referenz | unverändert |
| Historische Evidence und vier Strict-Quellpins | Referenz | byteidentisch |

Clippy-Zahlen zählen die vom Cargo-JSON gemeldeten Target-Diagnosen einschließlich Duplikaten zwischen lib/lib-test;
sie sind kein Vergleich zu früher anders gezählten „38 Warnungen“. Keine bestehenden Warnungen wurden unterdrückt.
Alle ursprünglichen Rust-Testnamen und Ergebnisse sind nach Normalisierung der Modulpfade erhalten.

702 von 705 bisherigen Rust-Funktionskörpern der Runtime sind nach reinen Import-/Pfad-/Formatnormalisierungen
identisch. Die drei bewussten Änderungen sind `main`, der zerlegte `run_shadow` und der auf sämtliche neuen
Observer-Dateien erweiterte Sicherheits-Quellscan. Die sieben extrahierten Owner-Schritte sind separat tokenweise
gegen den gesicherten Ausgangsstand verglichen; alle identisch nach dokumentierten Kontextfeld-/Kontrollflussanpassungen.
Sämtliche bisherigen Anwendungs-Stringliterale der Main-/Owner-/Startup-Logik bleiben erhalten.
Das ist ein nachvollziehbarer Quell-Paritätsnachweis, keine Behauptung eines ausgeführten Mainnet-Replays.

Die vorhandenen Regressionen prüfen reale historische Transaktionsfixtures, Protokoll-/Economics-Ergebnisse,
V1/V2-Leser und Resume-Grenzen, Ressourcen-/One-Shot-Sperren und Fail-Closed-Verhalten. Historischer 5h-Gesamtreplay:
**NOT_RUN / USER_OWNED**. Mainnet, Paid-Provider und werttragende Submission: **NOT_RUN / nicht aktiviert**.

Zwei im Ausgangsstand vorhandene Launcherfehler sind mit den echten Programmen reproduziert und korrigiert:
relative `--measurement-config`-Pfade werden vor `cd` aufgelöst; Shadow-Banner zeigen nur Scheme/Hostname statt
URL-Benutzerinfo oder geheimen Pfadsegmenten. Dieselben erweiterten Regressionen schlugen vorher zweimal fehl
und bestehen nachher. Keine geänderten Handelsentscheidungen oder Konfigurationsdefaults.

Logs und Checksummen: [Qualification](../evidence/RUNTIME_MODULARIZATION_20261016/qualification.json),
[Integrität/CLI](../evidence/RUNTIME_MODULARIZATION_20261016/integrity_and_cli.json),
[Owner-Parität](../evidence/RUNTIME_MODULARIZATION_20261016/owner_step_parity.json),
[Funktions-Parität](../evidence/RUNTIME_MODULARIZATION_20261016/function_parity.json).
Die beigefügten `parity.py` und `function_parity.py` erhalten beim Wiederholen als erstes Argument den lokalen Baseline-Ordner mit `source-before.tar.gz`; ohne diese gesicherte Referenz ist der Vergleich NOT_RUN.
Die `final-*.json` neben diesen Dateien enthalten den exakten Prüfaufruf, Exit-Code, Laufzeit und Log-Hash;
Build-/Testlogs liegen dort ebenfalls. Vollständige lokale Arbeitslogs und geschützte Referenz:
`/private/tmp/project0-architecture-20261016-ii5qqh4s`.

## Performance und Identitäten

p50/p95/p99 Hot-Path-Latenz, Queue-Verhalten unter repräsentativer Last, M4-Durchsatz und aktuelle Providerleistung:
**vorher NOT_MEASURED, nachher NOT_MEASURED**. Build-/Testdauer ist kein Performancebeweis.
Keine Optimierung oder Geschwindigkeitssteigerung behauptet. Der frühere Race-/Geyser-/Jito-Qualifikationsbedarf bleibt bestehen.

Quellpins bleiben unverändert. Neu gebaute Binaries haben neue Hashes; damit ändern sich neue Strict-Verifikation-
und Replay-Reportidentitäten gemäß dem vorhandenen Vertrag. Alte Ergebnisse werden gelesen, aber nicht als
neue Verifikation ausgegeben oder mit fremder Implementierungsidentität fortgesetzt. Historische Manifest-/Evidence-
Bytes, Schemata, Capture-IDs und gespeicherte Spec-IDs werden nicht umgeschrieben.

## Verbleibende bewusste Abweichungen und offene Produktarbeit

- Vier `market_*.rs` bleiben physisch an der bisherigen Runtime-Wurzel, sind aber ausschließlich in `market/observer`
  eingebunden. Der Verifier hasht Dateiname und Inhalt; eine kosmetische Verschiebung würde historische Reader gefährden.
- `run_layout.rs` bleibt ein geteilter Runtime-Vertrag für mehrere Domains und Python-Reader; `lib.rs` hält die öffentliche API.
- `delete/` bleibt als bereits vorhandene Wiederherstellungsquarantäne für 20 obsolete Pfade und den Mock-Ban-Patch;
  kein aktiver Import, Build oder Einstieg benutzt diese Dateien. Keine zweite produktive Struktur.
- Ignorierte lokale `.env`, `env`, `evidence/`, vorhandene `data/econ4h`-/`data/market`-Runs, `native/target` und Python-Caches
  bleiben unangetastet. Laufzeitdaten/Secrets wurden nicht kosmetisch umgezogen. `data/runs` und `data/cache` werden
  bei tatsächlicher Nutzung erzeugt; es wurden keine leeren Zukunftsverzeichnisse angelegt.
- `native` bleibt `native`; `engine` war ausdrücklich optional. Keine neuen Crates oder Runtime-Engines.
- Der vorhandene typisierte Konfigurationsvertrag und seine Defaults bleiben byteidentisch. Die Modularisierung
  behauptet keine zusätzliche Abdeckung bisher direkt aus Env gelesener Legacy-Parameter.
- FE-00 bleibt LOCAL_PASS/NO_ECONOMIC_GO; FE-01/02/03 OPEN, FE-04 PARTIAL_LOCAL, FE-05 NOT_RUN,
  FE-06 NOT_AUTHORIZED, FE-07 NOT_RUN. Race/Binary-Geyser, Jito-Qualifikation, weitere M4-Arbeit,
  Tagger/T, P7/P4B, vollständige P8-RES/INV/Inventory/Rebalancing, K und Frontend werden durch die Zuständigkeiten
  ermöglicht, aber nicht als neue fertige Produktfeatures ausgegeben. Vier Funding-Klassen und sechs
  Instruktionsmarker sowie bestehendes JIT-/Health-/Economics-Verhalten wurden nicht reduziert.

## Git und Änderungsumfang

Ausgangs-HEAD `0d538b5`; Index-Diff, 405 Quellen/Evidence-Dateien und vier Referenzbinaries wurden zuvor gesichert.
Während der Arbeit hat ein anderer Prozess die ursprünglichen 42 gestagten Project_0-Dateien exakt als `c10df8b`
gespeichert; der Commit-Diff ist byteidentisch zum gesicherten Index-Diff. Weitere parallele BSC-Commits führten
zum beim Abschlussaudit gelesenen HEAD `795c1ef`. Keine davon wurde durch diese Refaktorierung erstellt.

Alle hier angewandten Patches betreffen ausschließlich `project_0/`. Kein `git add`, Commit, Push, Pull, Merge,
Reset, Clean oder Stash-Eingriff durch diesen Arbeitslauf. BSC wurde nicht bearbeitet; parallele BSC-Änderungen
werden nicht als eigene Änderungen ausgegeben. Ursprüngliche Stashes unverändert, alte Evidence unverändert.
Beim letzten Statusabgleich hat ein anderer Prozess auch die Runtime-Migration, Launcher-Korrektur und
Integrationstests sowie 15 bereits vorhandene Evidence-JSON-Dateien gestagt. Dokumentationsänderungen liegen
teils ungestagt und neu vor. Dieser Arbeitslauf hat keine Index-Operation ausgeführt; Status und HEAD sind
zeitgebundene Momentaufnahmen. Die eigene Änderungsliste relativ zur gesicherten Dateibasis steht in [changed_files.txt](../evidence/RUNTIME_MODULARIZATION_20261016/changed_files.txt)
und [git_status.txt](../evidence/RUNTIME_MODULARIZATION_20261016/git_status.txt).
