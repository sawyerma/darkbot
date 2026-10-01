# Arbeitsauftrag — Project 0 — Claude Code

Gültig ab: 2026-09-30
Grundlage: `project_0/docs/PHASENPLAN.md` (AUTHORITATIVE v4.4). Review-Nachweis: `project_0/docs/FABLE_REVIEW_v4.4.md`.

## Auftrag

Implementiere den **FIRST EURO PATH** in genau dieser Reihenfolge. Keine Architekturdiskussion, keine Optimierung aus Eigeninitiative, kein Umbau mehrerer Phasen gleichzeitig.

```text
P0 (nur Reuse/Lizenz/Oracle-Modell) + P1A + C0 + S0
→ S1-LT → S2 → S3-CORE → P2 → P3 → P4A → P5 → T → P6-LT → S4 → P8-LT
```

Race blockiert diesen Pfad nicht. Race-Tickets (`S1-RACE`, `S3.R`, Direct TPU, Deshred) werden **nicht** angefasst, bis `S1-RACE PASS` im Ledger steht.

## Arbeitsschleife pro Ticket

1. Ticket aus §8 der Roadmap in Reihenfolge nehmen; nur freigegebene Tickets.
2. Vorhandenen Repo-Code prüfen (`darkbot/`, `gate0/`, Captures, Tests) und wiederverwenden, wo er Produktionsqualität hat.
3. Implementieren — eine Codebasis, Rust für LIVE-State/Execution, Python nur für P0/P1/C0/H0/Evidence.
4. Tests ausführen; bei Netzwerk-/Chain-Tests gegen den echten Endpunkt (Helius Free, Staging).
5. Messwerte und Gate-Entscheidung in `project_0/docs/MEASUREMENT_LEDGER.md` eintragen.
6. Commit mit Ticket-ID im Titel.
7. Nächstes Ticket.

## Harte Regeln (Kurzform, Details in der Roadmap)

- keine kostenpflichtigen Provider, Abos oder API-Keys mit Kosten; Helius Free ist der Ingress, Autoscaling im Dashboard aus;
- Long-Tail/Premium läuft auf `WsRpcControlSource + Snapshot/Reconcile`;
- `marginfi`-Crate (Apache-2.0) ist die Health-Engine; kein Nachbau;
- kein Code aus `eva01` kopieren (keine Lizenz) — lesen und lernen ja;
- `EndLiquidation` ist die letzte Instruction der Transaktion, ohne Ausnahme;
- Swap nur über allowlisted Programme; NO_SWAP als eigener Pfad;
- unbekannter Program-Stand/Layout/Oracle/Integration → fail closed;
- P2 beginnt nicht vor `C0-GO`;
- Tagging/T-Pfad geht nicht LIVE, bevor 0.1.12 on-chain deployed und über P1B→P1A promoted ist;
- Prepared-Candidate-/TX-Prebuild aus P2/P5 wird umgesetzt, mit der "nicht cachen"-Liste.

## Sofort freigegebene Tickets

```text
P0-01  eva01 + mary Read-through → Reuse-Matrix, Oracle-Modell (Pull-Update in TX ja/nein)
P0-02  Lizenzen selbst öffnen: marginfi-v2, superagg, Pyth-Forks; API-Key-/Free-Tier-Prüfung Jupiter/Titan/DFlow
P0-04  Staging-Zugang (Feature network-staging) identifizieren
P1A-01 Current-Mainnet-Manifest (ProgramData on-chain verifizieren, erwartet 0.1.11-rc1)
P1A-02 Refresh-/Swap-Allowlist aus Source
C0-01..03 Census: alle MarginfiAccounts, Health-Klassifikation, Long-Tail-Report → C0-GO/STOP ins Ledger
S0-01..05 Workspace project_0/native/crates/*, timing, telemetry, journal/replay, lifecycle/channels
```

Danach in Reihenfolge: `S1-01`, `S1-03`, `S1-04`, `S1-07` (= S1-LT), dann S2, dann S3-CORE.

## Blocker-Regel

Nicht bei Kleinigkeiten rückfragen. Bei einem echten Blocker in `MEASUREMENT_LEDGER.md` festhalten: was blockiert, welche Primärquelle oder Messung betroffen ist, welche minimalen Optionen bestehen. Keine stillen Annahmen, keine Umgehung durch Paid-Provider.

## Umgebung (muss auf dem Mac vorhanden sein)

Rust stable ≥ 1.85, `protoc`, Python 3.12 mit `solders`/`solana-py`, `HELIUS_API_KEY` (Free) in der Shell, Git-Lesezugriff auf `0dotxyz/marginfi-v2` und `0dotxyz/eva01`, Repo-Checkout mit `project_0/docs/` und leerem `MEASUREMENT_LEDGER.md`.
