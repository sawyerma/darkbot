# R6/R7 – Implementierungsstand und ausstehende Prüfung

Arbeitsverzeichnis: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`.
Verbindliche Quelle: `docs/SPEC.json`, identisch mit der externen Roadmap
`DARKBOT_PHASE1_IMPLEMENTATION_SPEC_v1_3_2.json` (SHA256
`40fe4ab8130a732ca8a1023c2fb6e0165fc2832e91a6dac0e81d0ddd1e7036f5`).

**Auf ausdrücklichen Nutzerwunsch werden weitere Tests und Review-Runden verschoben.
Der aktuelle Gesamtstand hat noch keine abschließende Abnahme. Ein positiver
Netto-Erwartungswert ist nicht nachgewiesen.**

## Implementierte Korrekturen

- Replay liefert verspätete Callbacks nach ihrer tatsächlichen Zustellzeit; die modellierte Fillzeit bleibt separat. Konflikte wirken erst ab ihrer Beobachtung.
- PARTIAL→COMPLETE darf neuere Reserven nicht zurücksetzen. Unverifizierte Trade-Tails und widersprüchliche Quotes sperren wirtschaftliche Auswertung. Multi-Mint-Reorgs erfassen alle betroffenen Mints.
- Gebührenpublikation ist atomar und kausal; Pending-Fills behalten ihre Gebührenbasis. Abgewiesene Fills verwenden aktuelle Gebühren samt Provenienz. Replay erhält die ursprüngliche Kostenevidenz.
- Writer-Shutdown berücksichtigt noch laufende Schreibvorgänge; Quarantäne speichert vollständige Datensätze. Begrenzte Historienverluste hinterlassen Unsicherheit statt günstiger Fills. Diagnostisches Tail-Hashing wurde aus dem BUY-Hot-Path entfernt.
- Feature-Truncation verändert frühere Cutoffs nicht rückwirkend. Fehlgeschlagene Intents erhalten den Decision-Zeitblock.
- Research bindet Runtime- und Researchparameter im Freeze, trennt Holdout von neuen Forward-Aufnahmen und verwendet kampagnenweite UTC-Splits. Dataset-Manifeste berücksichtigen zusätzlich Run-/Canonical-/Qualitätsdaten. Stressmodelle einschließlich Seed haben eigene Provenienz.
- Datasetweite Research-Aggregation und Qualitäts-/Exposure-/Stress-Gates wurden ergänzt. Die CLI unterstützt wiederholtes `--source-run-id` für die Auswahl späterer Aufnahmen.
- Falsche Global-Adresse korrigiert. Aktuelle offizielle Event-/Accountlayouts gepinnt; reale Mainnet-Fixtures gespeichert. FeeConfig-Tiers, Creatorfee-Regeln und versionierte BUY-Rundung ergänzt.
- Unsigned BUY-/SELL-Nachrichtenvorlagen für Hintergrund-`getFeeForMessage`, RPC-Budget und strengere Konfigurationsvalidierung ergänzt. Kein LIVE-Broadcast.

## Unabhängige Rollen und Befunde

- **CODER:** `coder_r6`, verantwortlich für Produktivcode und Regressionstests.
- **SPEC REVIEWER:** `r4_spec` / Pauli, ausschließlich lesend. Bestätigte u. a. unvollständigen Freeze, Testkontamination, fehlenden echten Forward-Pfad, falsche Aggregation, Qualitätslücken, fehlende FeeConfig-Logik und unvollständige Dataset-Hashes. Korrekturen geschrieben; letzte Research-/Fee-Phase noch nicht erneut unabhängig abgenommen.
- **TEST REVIEWER:** `r4_tests` / Boyle, unabhängig. Zehn integrierte Gegenbeispiele zu Replay, Stale-Upgrades, Features, Bootstrap, Tail-/Quote-Konflikten, Reorgs und Gebühren. Alle zehn auf R6 nachgeprüft und geschlossen.
- **ARCHITECTURE/LATENCY REVIEWER:** `r4_architecture` / Helmholtz, ausschließlich lesend. Fee-Cachebindung, Shutdown, Quarantäne, Historienretention und Hot-Path-Hashing geprüft. Kern-Gegenbeispiele geschlossen; keine Abnahme der danach erweiterten Fee-Pipeline oder langfristiger Lastfestigkeit.

Reviewer haben sich nicht gegenseitig konsultiert. Findings wurden durch den Orchestrator an den CODER gegeben.

## Tatsächlich vorliegende Testergebnisse

- Unveränderter r5-Ausgangsstand: 106 deterministische Tests bestanden, dennoch integrierte Fehler gefunden.
- Zwischencommit **R6 `23590b0`**: vollständige Suite mit **106 Tests bestanden**, 8,706 Sekunden. Dieser Commit wurde auf ausdrücklichen Nutzerwunsch gepusht.
- Nachfolgende Kernphase: **9 neue Regressionstests bestanden**. Diese Aussage ist keine Gesamtfreigabe der späteren IDL-/Fee-/Researchänderungen.
- Kurzer echter Doctor-Lauf mit temporärer Public-RPC-Konfiguration: **5.515 Beobachtungen**, 1.126 Trade-Zeilen, keine Writer-/internen Fehler, Replay reproduzierbar. D01/D02/D04/D06 PASS, D03 INCONCLUSIVE, D05 FAIL, CAPTURE_READY. Der Lauf fand **vor** der abschließenden Fee-/IDL-Erweiterung statt; Helius wurde dabei nicht geprüft. Kein wirtschaftlicher PAPER-Trade.
- Der spätere vollständige Lauf mit 115 Tests hatte zwei inzwischen korrigierte veraltete Fixture-Erwartungen. Danach bestanden zehn gezielte Tests. Die abschließende vollständige Suite wurde auf Nutzeranweisung ausgesetzt; die letzten Änderungen wurden nur auf Syntax und Konfigurationskohärenz geprüft.

## Verbleibende Grenzen und nächste Prüfung

- Aktuelle Softwareänderungen sind noch nicht vollständig getestet. Keine Behauptung „alle Anforderungen bestanden“.
- Token-2022 bleibt gemäß aktueller Runtime-Konfiguration deaktiviert. Alle elf CREATEs in einer separaten kurzen Mainnet-Stichprobe verwendeten Token-2022. Das begrenzt die praktisch handelbare Stichprobe erheblich und muss vor einer Erwartungswertaussage berücksichtigt werden.
- Nicht unterstützte Gebührenmodi bleiben gesperrt. Layoutverifikation allein ist keine wirtschaftliche Freigabe aller Tokenvarianten.
- Begrenzte Historien können bewusst UNKNOWN/UNRESOLVED auslösen. Retention unter Mainnetlast sowie im RAM verbleibende historische Positions-/Auditdaten benötigen weitere Prüfung.
- Research spielt Aufnahmen weiterhin als einzelne Portfolio-Sessions ab. Die statistische Aggregation setzt den Kapitalzustand nicht zwischen Aufnahmen fort; kampagnenweite Portfolio-Kontinuität ist noch offen.
- Splitgrenzüberschreitende Exposition wird zensiert und sperrt Qualifikation; ein vollständiges separates Purging-Protokoll fehlt noch.
- Die vorhandenen Einstellungen `liveness_check_interval_ms` und `position_state_stale_ms` sind noch nicht als eigenständige Betriebssteuerung verdrahtet; WebSocket-Pings und Quote-Frische ersetzen keine Prüfung dieser Config-Lücke.
- Nächster Schritt: vollständige deterministische Suite, alle unabhängigen Gegenbeispiele einschließlich neuer Research-Gates, anschließend Doctor mit aktueller Fee-Pipeline. Erst danach wirtschaftlicher Pilot und unberührter Forward-Test.
- Der geforderte Pilot mit mindestens 1.000 beobachteten CREATEs und der statistisch ausreichend große Forward-Test sind nicht abgeschlossen. POSITIVE_UNDER_MODEL darf erst nach den unveränderten Roadmap-Gates gemeldet werden.

Im Repository existiert inzwischen zusätzlich Commit **R7 `58caca3`**; dieser wurde nicht durch den Orchestrator dieser Sitzung erstellt. Weitere Korrekturen liegen im Arbeitsverzeichnis. Es wurde kein zusätzlicher Push durch den Orchestrator vorgenommen.
