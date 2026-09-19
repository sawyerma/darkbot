# R8_2 – Doctor-Gate-Nachtrag (2026-09-20)

Ausgangscommit: `1ac8e8be7f61a9f74b93eadd551d1a050cc2d7d0` (`R8_2`).
Korrektur direkt im Originalrepository, noch nicht committed/gepusht. SPEC unverändert.

## Korrektur und Regression

`run_doctor()` verlangt für die finale wirtschaftliche Freigabe nun ausdrücklich D01–D06 jeweils `PASS`. D01-/Feed-Ausfall bleibt `NOT_READY`; andere nicht bestandene Checks erlauben höchstens `CAPTURE_READY`. Fehlende/INCONCLUSIVE-Checks zählen nicht als PASS.

Die finale Readiness steht konsistent auf oberster Report-Ebene, in `summary.readiness` und in `runs.readiness`. Die vorherige technische Runtime-Bewertung bleibt getrennt als `summary.runtime_readiness` sichtbar.

Der integrierte Regressionstest erzeugt eine temporäre Aufnahme mit sonst bestandenen Checks und injiziert getrennt einen D04-Fixture-Fehler und unterschiedliche Replay-Ergebnisse (D06). Beide Fälle reproduzierten vor dem Fix die falsche wirtschaftliche Freigabe. Nach dem Fix blockieren beide; die Kontrolle mit allen sechs PASS bleibt wirtschaftlich bereit. Report und persistierter DB-Status werden ebenfalls geprüft.

**Gesamtsuite: 130 Tests PASS** (129 bestehende + ein neuer Test mit drei Szenarien). Quellcode aus dem Originalrepository; Testausgaben in temporärem Arbeitsordner, keine Repository-Kopie. [Lokales Testprotokoll](R8_2_DOCTOR_GATE_TEST_RESULT.txt). Das ist lokaler Ausführungsnachweis, kein GitHub-Actions-/CI-Lauf. Für diese kleine Korrektur wurden keine weiteren Reviewer gestartet.

## Echter Doctor

Ein weiterer capture-only Mainnet-Doctor lief 60 Sekunden mit den vorhandenen Endpunkten. **D01/D02/D03/D04/D05/D06 PASS**, finale und persistierte Readiness **ECONOMIC_PAPER_READY**, keine Fee-Fehler.

- 9.867 Observations, 28 CREATEs, 24 SOL-actionable CREATEs, 468 Trades.
- 10 beobachtete wirtschaftlich unterstützte Profil-Samples bei Laufende.
- Replay reproduzierbar, keine wirtschaftlichen Positionen, keine Transaktionen.

[Kompletter Doctor-Bericht](R8_2_DOCTOR_GATE_RESULT.json).
Run: `r8_2_gate_doctor_doctor_20260919T215946Z_044a34`.
DB: `data/r8_2_gate_doctor.sqlite` (lokal, gitignored).

## Nächster Schritt: alter Edge-A/B-Nachweis

Freigabe für den nächsten Nachweisschritt ist erreicht; **kein ≥1.000-CREATE-Pilot gestartet**. Ein positiver Erwartungswert ist damit nicht nachgewiesen.

Die alte positive Datenaufnahme wurde im DarkBot-Projektbaum einschließlich `alt` und den vorhandenen ZIP-Archiven nicht gefunden. Vorhanden sind die Research-Notizen zu positiven Einzeltrades bei 0-ms-Diagnose-Latenz, aber keine zuordenbaren Rohaufnahmen/Run-IDs dafür. `paper_fast_creator_hold_2x_01.db` ist eine in den alten Notizen genannte, hier fehlende Aufnahme; die dort dokumentierte Session war unterbrochen und belegt selbst keinen positiven Edge.

Für den echten Vergleich fehlen der genaue Pfad der positiven Aufnahme, die relevante Run-/Session-ID und die dazugehörige alte Engine-/Config-Version. Diese Informationen wurden beim Nutzer angefragt. Keine synthetische Ersatz-Baseline und keine Delta-Attribution erfunden.
