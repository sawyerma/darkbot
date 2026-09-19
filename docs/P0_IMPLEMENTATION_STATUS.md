# P0-Abschluss und nächstes Gate – 2026-09-20

Arbeit ausschließlich im Originalrepository `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`, Basis `48c21d9` (`R8_2b`). P0-Änderungen noch nicht committed/gepusht. SPEC und Strategie-Config unverändert. Geprüfte Dateien mit SHA-256: [Source-Manifest](P0_SOURCE_MANIFEST.json).

## Abgeschlossen

1. Originalaufnahme `edge_probe_01` konsistent gesichert; Manifest und Backup unter `data/evidence/`.
2. P0-Root-Cause am echten Mitschnitt nachgewiesen: bekannte Verwaltungsereignisse wurden fälschlich als unverified State-Tail behandelt. Kein unbekannter Pump-Event im konkreten Creator-Sell-Tx.
3. Decoder-Korrektur, Fee-/Complete-Copy-Recovery, ehrlicher recoverable UNRESOLVED-Diagnosepfad und kausale Fee-Replay-Ereignisse implementiert.
4. **141 Tests PASS** (elf neue). Echte Regression scheitert mit ursprünglichem Normalizer und besteht mit Fix.
5. Ein gebündelter unabhängiger SPEC/Test-Review plus fokussierter Timing-Review: keine blockierenden Findings. Zwei optionale zusätzliche Regressionen in der [Diagnose](P0_EDGE_PROBE_01_DIAGNOSIS.md) dokumentiert.
6. Neuer 60-Sekunden-Doctor: **D01–D06 PASS / ECONOMIC_PAPER_READY**, 14.201 persistierte Observations, 24 CREATEs, 22 SOL-actionable CREATEs, 509 Trades, keine Writer-Fehler. [Doctor-Bericht](P0_DOCTOR_RESULT.json).

## Kurzer PAPER-Sanity – technische Roundtrips bestätigt, Gesamtgate noch offen

Neue eigene DB `data/p0_exit_sanity_20260920.sqlite`, Run `p0_exit_sanity_paper_20260919T223903Z_854405`, Aufnahme 180 Sekunden. Unveränderte 0-ms-Entry-/Exit-Modelllatenz; keine Messung realer Ausführung/Landing, kein Latenzstress und kein Edge-Nachweis.

- 43.614 Frames, 96 CREATEs, drei Entry-Intents und drei BUY-Fills.
- Zwei vollständige Roundtrips: CREATOR_SELL und TARGET, jeweils genau ein SELL-Fill.
- Creator-Sell-Position: `−3.391.236` Lamports; TARGET-Position: `+61.721.029` Lamports. Summe geschlossener Trades `+58.329.793` Lamports, ausdrücklich kein Gesamtrendite-/Erwartungswertnachweis bei offener Exposure.
- Bei beiden geschlossenen Positionen entspricht PnL exakt der Ledger-Cash-Summe; Tokens und Locked Rent jeweils null. Alle fünf Attempts haben gemessene Kosten; beim SELL jeweils `1.513.840` Lamports Rent zurückgebucht.
- Dritte Position am Aufnahmeende OPEN / CENSORED_RUN_END, PnL NULL, Locked Rent `1.513.840` Lamports. Kein künstlicher Exit nach Aufnahmeende.
- Keine UNKNOWN-/UNRESOLVED-Ausführung, keine internen Fehler, Queue-Overflows, Writer-Failures, Quarantäne oder Feed-Reconnects. Cash-Reconciliation stimmt. DB-Integrity `ok`, Foreign-Key-Check leer.
- **Zwei Netzwerkgebührenabrufe mit URLError**; neun CREATEs wegen UNQUALIFIED_COSTS abgelehnt. End-Readiness wirtschaftlich bereit nach Recovery, aber Report konservativ **UNQUALIFIED_COSTS** über die Aufnahme.
- Sechs mintbezogene Decode-Gap-Ereignisse bei anderen Mints; keine Position mit Canonical-Gap-Flag. Daher keine pauschale Behauptung einer vollständig gapfreien Aufnahme.
- Helius fehlte wegen nicht gesetztem HELIUS_API_KEY. Nur `existing_rpc` aktiv. Kein Helius/Public-Race gemessen; Required-Feed-Semantik weiterhin ausdrücklich offen.

[Sanity-Ergebnis](P0_SANITY_RESULT.json), [Report](../exports/p0_exit_sanity/p0_exit_sanity_paper_20260919T223903Z_854405/report.json).

Zweifacher Offline-Replay der vollständigen neuen Aufnahme ist deterministisch. PAPER und Replay stimmen in wirtschaftlichen Positionsdaten und Cash überein; keine Behauptung identischer lokaler Verarbeitungszeitstempel. [Replay-Nachweis](P0_SANITY_REPLAY_RESULT.json).

## Noch offen / Reihenfolge

P0-Fix ist durch den realen Gegenfall, Tests, unabhängige Reviews und zwei Live-Roundtrips abgesichert. Das umfassende Sanity-Gate aus der Master-TODO ist wegen der obigen Einschränkungen **nicht als vollständig PASS** markiert.

Nächster zusammenhängender Block: P1.1 Live-Observability und P1.4 dokumentierte Required-Feed-Semantik; danach erneuter sauberer Sanity-/Feed-Race-Nachweis mit verfügbaren Endpunkten. Keine Freigabe für den ≥1.000-CREATE-Piloten aus diesem Lauf.

Weiter offen: aktueller sauberer Edge-Datensatz; Entry-/Exit-Stress 5/25/100/250/500 ms; alter A/B nur bei wiedergefundener Originalaufnahme; ≥1.000-CREATE-Pilot; Kandidaten-Freeze, Holdout, Fresh Forward, Stress/Bootstrap; P2-Härtung (Portfolio, Retention, RAM, Cleanup, PDA, Token-2022-Finalreview, CI); Full Audit und Implementation Freeze. Keine POSITIVE_UNDER_MODEL-Freigabe.

Die vollständige [Master-TODO](MASTER_TODO_2026-09-20.md) bleibt erhalten; bereits implementierte Token-2022-/Doctor-Blöcke wurden nicht neu gebaut.
