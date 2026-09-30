# DarkBot V3 – geprüfter Korrekturblock, 2026-09-21

Originalrepository: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`, Branch `main`, Ausgangscommit `707f4fa`.

## Auftrag und Grenzen

Die beiden beigefügten Audits und `darkbot_fixes_v3.zip` wurden gegen den Originalcode geprüft. Die erneut angehängten Berichte waren identisch. Kein pauschales Einspielen von Patch A/B, keine neuen Branches, kein Commit/Push. Keine Live-Feeds, kein Doctor, kein PAPER-Lauf.

Die frühere Anweisung, dass der Nutzer selbst testet, gilt weiter. Die Rückfrage zur Ausführung lokaler deterministischer Tests blieb bislang unbeantwortet. Deshalb sind vorbereitete Tests kein bestandener Testnachweis.

## Agenten

- Analyse: `audit_v3` / Sagan, ausschließlich lesend. Eingangsbefunde und ein gebündelter Abschlussreview.
- CODER: `coder_v3` / Nash, zusammenhängender Korrekturblock und Tests; anschließend gesammelte Review-Korrekturen.
- Orchestrator: Übertragung des CODER-Patches in das Originalrepository, statische Kontrollen, Git-Index-Bereinigung und Übergabe.

## Bestätigte Befunde und Änderungen

1. Source-COST_MODEL-Snapshots: Inhalts-/Hash-/Kind-Prüfung sowie Zielkonfliktprüfung vor Replay-Persistierung. Gebührenreferenzen und konsumierte Provenienz gehören zur neuen Dataset-Identität; alte Freezes nicht still weiterverwenden.
2. Fehlende Regressionen zu Replay-FK, Writer-Fehlern, Strategy-C-Default/TX_LANDS und ungültigen Optionswerten vorbereitet.
3. Kostenlose Creator-Stornos separat im Report sichtbar. Legacy-C bleibt historische Reproduktion, kann aber weder wirtschaftlich positiv qualifizieren noch als positiver Forschungskandidat ausgewählt werden.
4. TX_LANDS-Dokumentation korrigiert: normale Fill-Auswertung statt garantiertem Fill; Slippage, Coverage und Kostenprüfung bleiben wirksam.
5. PAPER-Slot-Override verwendet dieselbe Kapital-pro-Slot-Regel wie Replay/Research; USD-Profil weiterhin FX-basiert.
6. Einheitliche Gebührenauswahl in Live-Cache, StateOwner/Pending-Fills, Replay und Edge-Analyse. Nur ausdrücklich klassifizierte vorübergehende Erfassungsfehler dürfen frische passende Evidenz erhalten. Ursprüngliche Snapshot-ID, Zeit und TTL bleiben erhalten. Tatsächliche Invalidierung und unklare gemischte Teilabfragen bleiben fail-closed. Priority-/Network-Paar wird zusammen behandelt. Rohfehler bleiben gespeichert.
7. Genau sechs bereits getrackte SQLite-WAL/SHM-Dateien unter data/ mit `git rm --cached` aus dem Index genommen. Alle sechs lokalen Dateien waren danach weiterhin vorhanden.

## Nicht übernommen

Patch B (COVERAGE_AND_FEED_LIVENESS / Quote-Age-Ausnahme) bleibt draußen. Die früheren zwei Lookahead-Beispiele scheinen korrigiert, jedoch fehlt im Paket die erzwungene diagnostische Qualifikationssperre; globale Feed-Aktivität beweist zudem keine mintbezogene Vollständigkeit. Zusätzliche per-Frame-Arbeit an Pending-Attempts wurde nicht als Latenzverbesserung akzeptiert.

Keine stillschweigende Sizing-Normalisierung von USD-A gegen FIXED_SOL-C. Keine Verschiebung des bestehenden C-Schlüssels von strategy nach execution. Bestehende JSON-Profile und die SPEC werden nicht geändert.

## Befunde im gebündelten Abschlussreview

- Neuer Stale-Reserve-Test verwendete C mit 600000 ms statt der beabsichtigten 250-ms-Grenze: Test muss die Grenze ausdrücklich setzen.
- Neuer Fee-Selektor nutzt mehr Provenienzfelder als das bisherige Dataset-Manifest bindet: vollständiges konsumiertes FeeSnapshot-Manifest einschließlich TTL binden und Mutationstest ergänzen.

Diese zwei Punkte wurden an den CODER zurückgegeben. Ihr endgültiger Erledigungsstand wird nach Übertragung und Re-Review unten ergänzt.

## Prüfstand und nächste Schritte

Der erste CODER-Patch bestand AST-Syntaxprüfung und `git apply --check`; nach Übertragung war `git diff --check` sauber. Das ist ausdrücklich kein funktionaler Testlauf.

Vor Abschluss: beide Review-Korrekturen übertragen und einmal eng nachprüfen; dann nach Freigabe die neuen Regressionen und die vollständige vorhandene Suite lokal im Originalrepository ausführen. Fehlermeldungen an CODER zurückgeben. Erst nach erfolgreichen Tests wirtschaftliche Runs erwägen; positiver Erwartungswert ist weiterhin nicht nachgewiesen.

Vorgesehene lokale Testbefehle (noch nicht ausgeführt):

```bash
cd "/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot"
python3 -m unittest tests.test_fee_refresh_evidence tests.test_replay_evidence_fk tests.test_strategy_c tests.test_paper_duration
python3 -m unittest discover -s tests
```

Arbeitsdateien: `/tmp/darkbot-v3-confirmed.patch`, `/tmp/darkbot-v3-coder`. Diese sind Vorbereitungsartefakte, nicht ein zweites aktives Repository. Maßgeblich bleibt der Originalpfad.

## Abschließender Stand

Grundpatch und beide Review-Korrekturen wurden im Originalrepository auf main angewendet. Der Analyse-Agent hat beide korrigierten Findings nochmals statisch geprüft und sieht daraus keine verbleibenden Blocker. Der ergänzte Hash bindet nun sämtliche konsumierten FeeSnapshot-Felder einschließlich TTL. Der Stale-Test setzt ausdrücklich 250 ms.

Die endgültige AST-Prüfung aller 12 geänderten/neuen Python-Dateien und git diff --check waren erfolgreich. 20 neue Testmethoden sind vorbereitet, keine ausgeführt. Bestehende JSON-Profile und docs/SPEC.json unverändert. Kein Commit/Push. Sechs SQLite-Begleitdateien sind lediglich im Index zur Entfernung vorgemerkt und lokal weiterhin vorhanden.

Offen: funktionale Regressionstests und Gesamtsuite nach Testfreigabe; echte Mainnet-/PAPER-Validierung, Performance-Messung und positiver Erwartungswert sind nicht nachgewiesen. Die statische Prüfung ist kein Versprechen fehlerfreien Betriebs.
