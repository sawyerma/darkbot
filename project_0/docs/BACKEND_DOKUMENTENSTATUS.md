# Project_0: aktive Backend-Dokumente, fertiggestellte Pläne und Restabnahmen

Standprüfung **24.10.2026**, Git `d32a369c0ebc0b2e52020895517f33a142d214b8`. Dieser Wegweiser erteilt keine Autorisierung für Mainnet, LIVE, Transaktionen oder kostenpflichtige Provider. **Immer Code, neue Evidence und offene Gate-Kriterien gegenprüfen.**

## Technischer Grundsatz: intern binär, kein JSON

Interne Zustände/Queues/Config-/Steuerungsübergaben sind typisierte Rust-Daten; dauerhafte interne Nachweise native binäre `.p0j`-Frames. Neue interne JSON-Werte, JSON-Journalschreiber, JSON-Cacheformate und schemalose `serde_json::Value`-Pipelines sind untersagt. Ausnahmen nur zwingende externe Protokollgrenzen (z. B. Solana JSON-RPC/WSS), explizite Reports/Exports oder Rückwärtsleser historischer, unveränderlicher Daten; am Rand in Typen wandeln, Ausnahme dokumentieren. Bestehende FE01- und Census-Import-Schema-1-Schreiber sind **offen**, nicht grandfathered.

## Quelle je noch offenem Bereich

| Bereich | Gültige Beschreibung (relativ zu docs/) | Status und Rest |
|---|---|---|
| **FE-04** Finanzjournal/Admission/Settlement | [PHASENPLAN.md](PHASENPLAN.md) FE-04/P8; [BUGS.md](BUGS.md); [MEASUREMENT_LEDGER.md](MEASUREMENT_LEDGER.md) | PARTIAL_LOCAL; lokal vollständig inkl. Versuchsarchiv/kumulierter FE-07-Abrechnung/read-only Treasury (`286f9cc8`, `366271b5`); echte Finality-, Wallet-Delta-/Exit-, Admission- (FE-06) und Gesamtbeweise offen |
| **P2–P5** State, Health, Planner, Economics | [PHASENPLAN.md](PHASENPLAN.md) P2–P5; [CURRENT_MAINNET.md](manifests/CURRENT_MAINNET.md); [UPCOMING_0_1_12.md](manifests/UPCOMING_0_1_12.md) | Große Codeanteile implementiert; neuer rc2-Codepin ≠ belegte Mainnet-Deployment-/Bank-/Oracle-Parität |
| **PERSIST/JFI/SK** | [PHASENPLAN.md](PHASENPLAN.md); [run_storage.md](operations/run_storage.md); [run_layout_versioning.md](architecture/run_layout_versioning.md); [BUGS.md](BUGS.md) | Binary Journal vorhanden, aber FE01-Evidence und `p0-journal import` schreiben noch Schema-1-JSON-Frames; Gesamtqualifikation offen |
| **P6-MT/M4** | [PHASENPLAN.md](PHASENPLAN.md) jüngste P6-MT-Bestimmungen; [parallel_execution_plan.md](architecture/parallel_execution_plan.md) nur als historischer Startentwurf | MT0, Owner, Worker/Wakeup teils implementiert; echte End-to-End-Perzentile/S1-RACE/MT-Qualifikation offen |
| **Binary Geyser/JIT/Inventory/p0ctl** | [PHASENPLAN.md](PHASENPLAN.md) S1/P4A-JIT/P8/Operations; [run_control.md](operations/run_control.md); [operational_config.md](operations/operational_config.md) | Off/Observe- und Codepfade existieren, jedoch Provider-/Race-/Funding-/Inventory-/Run-Horizon-Abnahme offen |
| **Race/Jito-gRPC/H0/T/P7/P4B/P9/K** | [PHASENPLAN.md](PHASENPLAN.md) S1-RACE/S3.R/H0/T/P7/P4B/P9/K; [MEASUREMENT_LEDGER.md](MEASUREMENT_LEDGER.md) | Wesentliche Produkt-/Wettbewerbs-/Routen-/Performance-/Forschungsgates offen, vorhandene Teilimplementation nicht neu beginnen |
| **C0-E1 4h/P6-LT 72h/FE-01…07** | [PHASENPLAN.md](PHASENPLAN.md); [C0E1_4H_READINESS.md](qualification/C0E1_4H_READINESS.md); [Messprotokolle](measurements/protocols/); [MEASUREMENT_LEDGER.md](MEASUREMENT_LEDGER.md) | Frühere 4h/5h-Datensätze unvollständig, 72h nicht gestartet; FE00 lokal NO_ECONOMIC_GO, FE01 P1A Parität blockiert, FE04 teilweise, FE05–07 nicht vollständig abgenommen |

## Explizit fertig – kein neuer Arbeitsauftrag

- [32 Schritte Strukturreform](fertig/restructure_32_step_plan.md): abgeschlossen, nur Archiv.
- [Reform-Handoff](fertig/restructure_handoff.md): überholter Arbeitsbaum-/Git-Snapshot.
- [Runtime-Modularisierungsplan](fertig/runtime_modularization_plan.md) und [Abschluss-Evidence](fertig/runtime_modularization_20261016.md): A0–A7 abgeschlossen, keine neue Strukturreform.
- P1B rc2 **lokale** Integration (Commit `c9decd7f`) abgeschlossen; fehlende P1A-Mainnetparität ist davon getrennt.
- [BUGS.md](BUGS.md) 91/114 ursprüngliche Tickets lokal erledigt, 23 offen; A23-05 PARTIAL. Abgehakt ≠ Live-Gate-PASS.

## Veraltete Unterlagen nicht blind ausführen

- [STATUS.md](STATUS.md): Kopf referenziert noch `c9decd7f`; **kein aktueller SHA-Status**.
- [PHASENPLAN.json](PHASENPLAN.json) und [MEASUREMENT_LEDGER.json](MEASUREMENT_LEDGER.json): ältere Snapshots gegenüber den kanonischen Markdown-Dateien; JSON-Indizes nicht als Arbeitsvertrag verwenden.
- [bugs_b.md](bugs_b.md): ältere 81/33-Matrix; ergänzende Z-/Q-Information nicht verwerfen.
- [parallel_execution_plan.md](architecture/parallel_execution_plan.md): alter Eintrag „IMPLEMENTATION NOT STARTED“ ist überholt; jüngsten P6-MT-Code prüfen.
- [UPCOMING_0_1_12.md](manifests/UPCOMING_0_1_12.md): erste alte 0.1.11-Tabelle ist nicht der letzte lokale RC2-Stand.
- [C0E1_4H_READINESS.md](qualification/C0E1_4H_READINESS.md): Gate-Bedingungen bleiben, alte Fortschrittszeilen sind Momentaufnahmen.

**Nicht archiviert:** verbindlicher [PHASENPLAN.md](PHASENPLAN.md), BUGS, Ledger, Betriebs-/Sicherheits-/Architekturverträge, Manifeste, Messprotokolle und [evidence/](evidence/); auch abgeschlossene Teilabschnitte ändern nicht den Gesamtstatus. Der Code selbst bleibt unverändert.
