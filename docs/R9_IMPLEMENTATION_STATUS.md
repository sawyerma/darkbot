# R9 – Token-2022 / V2 PAPER: Implementierungs- und Prüfstand

Stand: 2026-09-19. Änderungen direkt im Originalrepository
`/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`, Ausgangspunkt `afaab27 R8_1`.
Nicht committed oder gepusht. Die normative SPEC wurde nicht geändert:
SHA256 `40fe4ab8130a732ca8a1023c2fb6e0165fc2832e91a6dac0e81d0ddd1e7036f5`.

## Abgeschlossener Codeblock

- `buy_v2(amount,max_sol_cost)` und `sell_v2(amount,min_sol_output)` mit den Account-Metas des gepinnten offiziellen Pump-IDLs. Exaktes Token-Sizing und Gebührensemantik bleiben erhalten. V2-Account-/ATA-/PDA-Beziehungen werden im Hintergrund abgeleitet. Nur unsigned Fee-Probes, kein Senden oder Signieren.
- Strikte SPL-/Token-2022-Programmliste. Token-2022 benötigt das in der CREATE-Transaktion belegte Pump-CreateV2-Profil: erlaubte Token-Initialisierungen, MetadataPointer/TokenMetadata, ImmutableOwner und bestätigte Accountgröße 170 Byte. Unbekannte Extensions, fehlende Evidenz und unbekannte Programme bleiben unqualifiziert. Mayhem/Cashback/Holder-Rewards/Fee-Sharing bleiben ausgeschlossen.
- Unveränderliche Network-Snapshots enthalten getrennte BUY-/SELL-Kosten je Token-Programm und Profil. Rent-Snapshots enthalten getrennte 165-/170-Byte-Profile. Keine automatische Übernahme einer SPL-Kostenbasis für Token-2022. Kein zusätzlicher Quote-ATA-/WSOL-Wrapping-Rent für den unterstützten nativen SOL-Pfad.
- Quotes, Exit-Reserve, NLV, Pending-Fills und fehlgeschlagene Attempts verwenden das ausgewählte Profil. Rückerstattet wird die tatsächlich bei Entry hinterlegte Rent, auch nach einem Rent-Snapshotwechsel.
- Migration `0006_token_account_profiles.sql` ergänzt Programm-/Profil-/Accountgrößen-/Instruktionsprovenienz. Quotes referenzieren den konkreten Message-Template-Hash. Das Decoder-Freeze umfasst auch `token_accounts.py`.
- Vier Hintergrund-Fee-Probes (zwei Programme × BUY/SELL); Refresh-Intervall versioniert von 10 auf 20 Sekunden erhöht, innerhalb des konfigurierten kostenlosen RPC-Budgets. Keine neuen RPC-/DB-/PDA-/Message-Hash-Aufrufe im BUY-Pfad.
- Readiness benötigt tatsächlich beobachtete unterstützte Profile und passende Kosten. Capture-only-Entscheidungen heißen `CAPTURE_ONLY_MODE`. Statusprüfung endet beim ersten geeigneten Sample; die vollständige Zählung erfolgt einmal nach Feed-Stopp vor der Tracker-Bereinigung.

## Tests und reale Kurzaufnahme

- Gesamtsuite nach dem Korrekturblock: **129 Tests PASS** (115 vorhandene + 14 neue).
- Neue Tests: echter aufgezeichneter CreateV2 → PAPER-BUY → Creator-SELL; Rent-Rückerstattung; SQL-Provenienz/FKs; fehlende oder fremde Profile; unbekannte Extensions/Programmkontexte; BUY-/SELL-Kostentrennung; fehlende SELL-Reserve; Profilverlust/Refresh während Pending-Fill; PAPER-/Replay-Parität; IDL-Metas im serialisierten Message; PDA/ATA gegen aufgezeichnete Mainnet-Adressen; RPC-Budget; vollständigere Feed-Kopie; begrenzte Statusarbeit, Shutdown-Evidenz und Token-2022-Completion während Pending-BUY/-SELL.
- Alte SPL-Fixtures bleiben Regressionstests. Der alte „Token-2022 ausgeschaltet“-Test setzt den Ausschalter jetzt ausdrücklich, statt vom früheren Default abzuhängen.
- Quellcode ausschließlich aus dem Originalrepository getestet. Gesamtsuiten liefen mit temporärem Ausgabe-CWD, damit CLI-Tests keine versionierten Reports verändern.
- Zwei reine 30-Sekunden-Doctor-Aufnahmen, keine wirtschaftlichen Positionen, keine Transaktionen, kein Langlauf.
- Erster Lauf: 5.877 Observations, 8 CREATEs, 34 Trades. Alle Kosten-Snapshots VALID; die neue Abschlusszählung verlor durch `finalize()` ihre Tracker und meldete D05 FAIL. Dieser Integrationsfehler ist korrigiert und deterministisch getestet. Rohbericht: [initialer Doctor](R9_DOCTOR_INITIAL_RESULT.json).
- Zweiter Lauf nach Korrektur: 9.904 Observations, 16 CREATEs, 9 SOL-actionable CREATEs, 95 Trades. D01/D02/D03/D04/D06 PASS, Replay reproduzierbar. **D05 FAIL / CAPTURE_READY**, weil der letzte Priority-Fee-Abruf mit `URLError` fehlschlug und Network entsprechend `PRIORITY_UNKNOWN` wurde. Kein erfolgreicher finaler D05-Gate behauptet. Rohbericht: [letzter Doctor](R9_DOCTOR_RESULT.json).
- `getFeeForMessage` prüft Gebühren für das Nachrichtenformat; es beweist weder erfolgreiche Programmausführung noch Landing-Latenz oder Ertrag. Die Nachrichtenform wird zusätzlich gegen das gepinnte IDL geprüft.

## Rollen und gebündelte Findings

Der Orchestrator übernahm nach dem früheren Subagenten-Limit die CODER-Rolle für den gesamten Block. Erst danach wurden unabhängige Read-only-Reviewer eingesetzt:

- `r9_arch_latency`: gezielte Architektur-/Latenzprüfung; kein Repository-Vollaudit. Zwei reproduzierte Findings: fehlende Profil-Aufwertung bei vollständigerer Feed-Kopie und vollständiger synchroner Readiness-Scan (ca. 55 ms bei 5.000 States im Reviewer-Versuch). Beide gesammelt korrigiert, mit Gegenbeispiel-Tests. Die Profil-Aufwertung setzt keine Reserven zurück und erzeugt keinen zweiten Entry/CREATE-Zähler. README-Aussage „Token-2022 deaktiviert“ korrigiert.
- `r9_spec_test`: unabhängige gebündelte SPEC-/Testprüfung; 126 Tests selbst ausgeführt, PASS, V2-Instruktionsverträge gegen offiziellen IDL geprüft. Ein P1: Der Sonderpfad für Venue-Invalidierung verlor Token-Programm/Profil, wodurch Pending-Token-2022-Attempts als UNKNOWN statt VENUE_INVALID behandelt wurden. Vom CODER anschließend für BUY und SELL reproduziert (beide FAIL vor Fix), durch Erhalt des vollständigen CurveState korrigiert und mit integriertem Regressionstest abgesichert.
- Abschlussprüfung des korrigierten Blocks durch CODER: 129/129 PASS. Ausdrücklich kein weiterer unabhängiger Re-Review gestartet, um die letzte Vorgabe zur Credit-Ersparnis einzuhalten. Die unabhängigen Reviewer haben den Stand vor den gebündelten Korrekturen geprüft; daraus wird kein Sign-off für den finalen Stand abgeleitet.
- Zusätzlich beim echten Doctor vom CODER gefunden: Abschlusszählung nach Tracker-Eviction. Vor `finalize()` gespeicherte Readiness-Evidenz behebt das Problem; eigener Regressionstest.

## Weiterhin offen

Kein vollständiges Projekt-Sign-off, kein SPEC-/Implementation-Freeze und kein nachgewiesener positiver Erwartungswert. Der letzte reale D05-Gate ist offen; weitere Netzläufe wurden aus Kostengründen nicht gestartet. Vor einem PAPER-Langlauf sind stabile aktuelle Kosten-/Feed-Gates erforderlich. Danach folgen der geplante größere Pilot/≥1.000 CREATEs und die getrennten Research-/Holdout-/Forward-Gates. Bekannte Themen aus der R8_1-Übergabe (u. a. Portfolio-Kontinuität, Purge-Protokoll, weitere RAM-/Liveness-Grenzen, Ressourcen-Cleanup) waren kein Bestandteil dieses Token-2022-Blocks.

## Protokollquellen

- Pump-IDL: https://raw.githubusercontent.com/pump-fun/pump-public-docs/main/idl/pump.json
  – gepinnter SHA256 `ffe966c42f1af41652ee753fe2f1e3f7cd4077d7e6f49faf3138959c8b56064b`.
- Offline-Instruktionsauszug: `tests/fixtures/pump_v2_instruction_contract.json`.
- Offizielles `@pump-fun/pump-sdk@2.0.0`, V2-Builder und PDA-/Quote-ATA-Semantik.
- Token-2022 ImmutableOwner: https://solana.com/docs/tokens/extensions/immutable-owner.
- Aufgezeichnete Mainnet-Fixtures und Quellenbelege in `tests/fixtures/provenance.json`.
