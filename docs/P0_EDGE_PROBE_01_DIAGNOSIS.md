# P0 – edge_probe_01: gesicherte Evidenz und Root Cause

Stand: 2026-09-20, Ausgangscommit `48c21d9f1219321fef5ccb3b77e24b38d469f4c3` (`R8_2b`).
Originalrepository: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`.
Die neue Master-TODO ist als `docs/MASTER_TODO_2026-09-20.md` archiviert. Ihre offenen Punkte sind Arbeitsaufträge; `docs/SPEC.json` bleibt die normative Basis. Kein Strategieparameter wurde für diese Diagnose verändert.

## Sicherung

Die originale `data/edge_probe_01.sqlite` und die bereits vorhandenen `exports/edge_probe_01/` wurden nicht verändert.
Eine konsistente Sicherung wurde mit der SQLite-Backup-API über eine read-only Quellverbindung erstellt:

- Backup: `data/evidence/edge_probe_01_20260919T222709Z.sqlite` (ca. 719 MiB).
- Backup-SHA256: `e4fb90452c3e897ff89eec16c704a2219fe2040f3ec558a52ca029c9bf617675`.
- `PRAGMA integrity_check`: `ok`.
- Run: `edge_probe_01_paper_20260919T220756Z_804f03`, Status `INTERRUPTED`.
- Sicherungsmanifest: `data/evidence/edge_probe_01_20260919T222709Z.json`.
- Gezielter Kontext-Export: `data/evidence/edge_probe_01_p2_context.json` mit Original-Config, vier Roh-Observations, Kosten-Snapshots, Positionen, Decisions, Attempts und Mint-Quality-Events. Dieser Auszug ist ausdrücklich kein vollständiger Run-Replay.

## Tatsächlicher Fehler

Position `..._p2`, Mint `9CbtNYyjtqD5rMHBKtczGf5EVUqiLCy8x8mUVJgupump`, Creator `E8Yma8FN33iJ4aMvCXfQTn5z1qkPqJ7Bp3XJEWNGpv2x`.

| Zeitpunkt / Observation | Evidenz |
|---|---|
| `1657166199672458`, `..._o111479` | CREATE-Transaktion inklusive initialem BUY, COMPLETE |
| Entry-Fill `1657166201665750` | Position geöffnet |
| `1657213278011000`, `..._o124112` | BUY-Transaktion mit bekanntem `ExtendAccountEvent`; fälschlich PARTIAL, erster Decode-Gap |
| `1657250079401541`, `..._o142062` | COMPLETE-SELL; vertrauenswürdiger State beendet ersten Gap |
| `1657260603098791`, `..._o147596` | Creator-SELL und bekanntes `CloseUserVolumeAccumulatorEvent`; erneut fälschlich PARTIAL |
| Exit-Decision `1657260603247500` | `EXIT_LATCHED_DECODE_GAP`; 148.709 ns nach Empfang |

Der Creator-Verkauf kam 94.401.433.041 ns nach Entry. Danach existiert in der Aufnahme keine weitere Observation dieser Mint. Das erklärt, warum die bisherige rein markteventbasierte Wiederholung nicht mehr lief.

**Es gibt in dieser Creator-Sell-Transaktion keinen unbekannten Pump-Event-Discriminator.** Der Trade bei Logindex 34 hat einen verifizierten `TradeEvent_ffe966c42f1a`-Tail und Reserven:

- virtuelle SOL-Reserve: `30_000_000_001` Lamports;
- virtuelle Token-Reserve: `1_073_000_000_000_000` Basiseinheiten;
- reale SOL-Reserve: `1` Lamport;
- reale Token-Reserve: `793_100_000_000_000` Basiseinheiten.

Danach folgt bei Logindex 47 `CloseUserVolumeAccumulatorEvent` (Discriminator `929fbdac925838f4`, 80 Byte). Dieses Ereignis war bereits in `KNOWN_NON_STATE_EVENTS`. Ebenso war das frühere `ExtendAccountEvent` bei Logindex 28 bekannt (`6161d7905d92167c`, 96 Byte).

`PumpDecoder.decode_logs()` liefert beide als `kind="ignored"`, speichert jedoch ihre Payloadlänge in `tail_bytes`. Die pauschale Tail-Prüfung in `Normalizer.normalize()` behandelte jedes Nicht-CREATE mit `tail_bytes > 0` ohne `tail_verified` als PARTIAL – auch die absichtlich ignorierten bekannten Verwaltungsereignisse. Der Reducer öffnete daraufhin konservativ einen Gap und übernahm den SELL-State nicht. Die gemeinsame Quality-Meldung „unknown Pump event“ war für diesen Fall irreführend.

Der Fix muss die falsche Partial-Einstufung bekannter Verwaltungsereignisse beheben. Echte unbekannte/undekodierbare State-Ereignisse dürfen dadurch nicht handelbar werden. Die Datenintegritätsprüfung des Exit-Pfads war hier nicht zu streng; sie erhielt einen falschen Vollständigkeitsstatus.

## Ergänzende Liveness-Anforderungen des Blocks

- Bereits gelatchten Exit bei frisch publizierten Gebühren wieder prüfen, ohne zufälligen Coin-Trade abzuwarten.
- Vollständigere Kopie eines bereits gesehenen State-Events muss die betroffene Position nach State-Recovery erneut prüfen.
- Bei weiterhin unbekanntem State keine erfundene Quote, keinen Fill und keine unbelegten Kosten buchen.
- Blockierte Quote zeitnah explizit als unaufgelöste Exposure markieren; den Exit-Wunsch weiter erhalten. Spätere sichere Recovery nur für diese No-Submission-Blockade zulassen, nicht für UNKNOWN-Ausführung, Venue-Konflikt oder erschöpfte eingereichte Attempts.
- Gebührenzeitpunkte, State-Recovery und Timer in PAPER und Replay kausal gleich verarbeiten.

## Implementierung und unabhängige Abnahme

- Normalizer: bekannte Verwaltungsereignisse (`ignored`) erzeugen keinen falschen unverified State-Tail mehr. Echte unbekannte Ereignisse bleiben PARTIAL.
- Vollständigere Feed-Kopien melden angewendete State-Updates als `touched`, damit gelatchte Exits unmittelbar erneut geprüft werden.
- Frisch publizierte Gebühren prüfen gelatchte Exits ohne zusätzliches Marktevent. Keine I/O-Arbeit im Exit-/BUY-Pfad.
- Creator-Sell bleibt als Audit-Trigger auch bei pending SELL sichtbar; eine frühere kausale Exit-Ursache wird nicht überschrieben. Keine doppelten SELLs.
- Echte Decode-Gaps verhindern auch TARGET-Berechnung aus dem vorherigen State.
- Nach dem bestehenden `connectivity.position_state_stale_ms` (2.000 ms) wird eine weiterhin blockierte, noch nicht eingereichte Quote explizit `UNRESOLVED / EXIT_QUOTE_UNAVAILABLE`. Der Exit bleibt gelatcht und ausschließlich diese Blockade ist bei späteren gültigen frischen Inputs recoverable. Keine erfundenen Fills/Kosten. UNKNOWN-Ausführung, Venue-/Quote-Konflikte und ausgeschöpfte Attempts bleiben terminal.
- Replay verarbeitet Gebührenpublikation als eigenes zeitliches Ereignis. Bei gleichem Zeitpunkt werden Marktbeobachtungen vor Fee-Wakeup und Timer-Callbacks angewendet.
- Keine SPEC-, Config-, Schema- oder RPC-Änderung.

### Tests

**141 Tests PASS**: 130 bestehende plus elf neue Regressionen. Der bestehende Timeout-Test T09 erwartet jetzt den expliziten recoverable UNRESOLVED-Diagnosestatus statt dauerhaft OPEN; Exposure und fehlender Fill bleiben unverändert. Vollständiges Protokoll: `docs/P0_TEST_RESULT.txt`.

`tests/fixtures/edge_probe_01_p2.json` enthält vier Original-Rohframes und 29 Original-Kostensnapshots. Der reale Gegenbeispieltest schlägt mit dem ursprünglichen Normalizer aus `48c21d9` fehl (PARTIAL statt COMPLETE) und besteht mit dem Fix. Zweifacher Replay liefert denselben Digest, keinen falschen Decode-Gap und SELL-Submit bei `1657260603098791`. Dies ist ein abgegrenzter Mint-Kontexttest, kein vollständiger Replay der 719-MiB-Aufnahme und kein Edge-Nachweis.

### Eingesetzte Subagenten

- `p0_exit_coder`: zusammenhängender Produktiv-/Testblock; elf neue Regressionen und 72 betroffene bestehende Tests bestanden.
- `p0_spec_test_review` (read-only): SPEC/Test-Prüfung des fertigen Diffs; 46 relevante Tests PASS, zusätzlicher unabhängiger Gegenfall für Venue-Invalidierung nach recoverable Blockade PASS. Keine blockierenden Findings.
- `p0_timing_review` (read-only): Gebührenpublikation, Replay-Reihenfolge, Pending-/UNKNOWN-Schutz, begrenzte Arbeit über aktive Positionen, kein zusätzlicher Hot-Path-I/O. Keine blockierenden Findings; statische Prüfung, kein eigener Live-Lauf.

Nicht blockierende Testempfehlungen bleiben offen: ein gemeinsamer Replay-Zeitpunkt für Fee-Recovery + Venue-Invalidierung + Timer; dauerhafte Regression der unabhängig geprüften Transition von recoverable zu terminaler Venue-/Conflict-Blockade. Kein zweiter Review-Zyklus nötig, da keine blockierenden Findings entstanden.

Doctor, kurzer Sanity-Lauf und die übrige Master-TODO werden getrennt bewertet. Weder Pilot-Freigabe noch positiver Erwartungswert werden aus diesen Tests abgeleitet.
