# DarkNode P1 – Gesamtprüfung öffentlicher Bitget-Daten (27.10.2026)

**Status:** `PUBLIC_API_SPIKE = PASS (getesteter Umfang)` · `R2 = PARTIAL` · `P1 = NOT_RUN` · `GATE 1 = OPEN`  
**Scope:** Bitget BTCUSDT USDT-Futures; ausschließlich öffentliche REST/WebSocket-Reads und lokale Python-Replays; keine Keys, keine Orders, keine Code-/Gateway-Abnahme.  
**Bezug:** [P1 Phasenplan V1.1](../PHASENPLAN_P1_V1.md), [Endpoint-Registry](../ENDPOINT_REGISTRY.md), [frühere Terminal-Evidenz](BITGET_PUBLIC_API_VERIFIKATION_2026-10-27.md).  
**Datenbasis:** Vom Nutzer am 27.10.2026 bereitgestellte vier Originaldateien; die SHA-256-Hashes wurden **an den erhaltenen Dateibytes** in der Prüf-Containerumgebung berechnet. Vorhergehende Ergebnisse wurden durch den Nutzer auf dem Mac ermittelt; **keine unabhängige Wiederholung der Live-Netzwerkzugriffe durch den Reviewer**.

## 1. Gesamtresultat – tatsächliche Bedeutung der Zähler

Das Testskript meldet **16 PASS, 23 BLOCKED, 1 NOT_RUN**, aber diese 40 Positionen sind **keine 40 unabhängig ausgeführten P1-Abnahmetests**.

- **16 PASS** sind erfolgreiche Wrapper-/REST-/WS-/Python-Checks, zum Teil Wiederholung oder Zusammenfassung bereits geprüfter Funktionen.
- **23 BLOCKED T01–T23** wurden im `finally`-Abschnitt **pauschal vergeben, ohne einen spezifischen Test auszuführen**. Fachlich sind diese Einträge **NICHT ABGENOMMEN / NOT_RUN auf Produktcodeebene**, nicht 23 festgestellte Defekte und nicht jeweils ein nachgewiesener technischer Blocker.
- **T24 = NOT_RUN**, weil der verbindliche reale 7-Tage-Dauerlauf nicht stattfand.
- Die bisherigen Python-Fault-Tests liefern **technische Teil-Evidenz** für T01/T03/T04, ersetzen aber keine Verifikation des tatsächlichen Rust-Adaters samt Store und Gateway.

## 2. PASS-Nachweise im Gesamtlauf

| Bereich | Beobachtung |
|---|---|
| REST V3 instrument/time/ticker/book/fills/candles/mark/index/funding | Alle angefragten Endpunkte antworten `00000`; Beobachtungszeiten ca. 338–399 ms (einzelne Mac-/Netzwerk-Messungen, **keine p99/SLA**) |
| SBE V4 Transport | 20 `publicTrade`-, 481 `books1`-, 558 `books50`-Frames mit Header-Version 4 beobachtet (1.059 Transportframes); JSON ticker 84 Datenframes |
| SBE V4 vollständig dekodiert | 21 `publicTrade`-, 65 `books1`-, 191 `books50`-Frames; `publicTrade` entspricht **47 Einzelticks** (11 Multi-Trade-Frames); 191 Depth50-Snapshots |
| SBE↔REST-Einzelticks | **47 von 47 gematchten IDs**, **0 abweichende Felder** für ID/Preis/Menge/Richtung/Zeit |
| Classic V2 öffentliche Historie | Erste und zweite Seite je **1.000 Records**; alle zweite-Seite-IDs älter und 0 Überschneidung |
| Classic V2 Zeit-/Grenzregeln | Genau 7 Tage HTTP200/`00000`; 8 Tage HTTP400/`00001`; 89 Tage HTTP200/`00000`; 92 Tage HTTP400/`40934` |
| Funding | 2 Seiten à 100 Records mit 0 überlappenden Funding-Zeitstempeln |
| Position-Tiers / historische Candles | 12 Tiers; 24 Stundenkerzen aus ca. 120 Tage altem Fenster |
| Demo V5 neu verbunden – Lauf 1 | `ack=1`, `pong=1`, **5 SBE-V5-Frames = 1 Snapshot + 4 Updates**, keine gemeldete Gap-/Error-Zählung |
| Demo V5 neu verbunden – Lauf 2 | `ack=1`, `pong=1`, **7 SBE-V5-Frames = 1 Snapshot + 6 Updates**, keine gemeldete Gap-/Error-Zählung |
| Original-V5-Capture Offline | SHA-256-Check PASS, **27 Records/25 SBE-Frames** (1 Snapshot + 24 Updates), Replay-Endbuch-Hash identisch |
| Python-Fault-Injection | Delta entfernt → GAP und Block; Delta vertauscht → GAP und Block; identisches Delta doppelt → ignoriert; **gespeicherter Original-Snapshot** erneut eingespielt → Zustand offline wiederhergestellt |

### Exakt gemessener V5-Replay-Endhash

`8f0ac92007a70cf90bd6b8616d6be6f302e00e949a5b37751572d84162e137b3`

**Aussagegrenzen:** Der Endhash ist ein deterministischer Python-Modellhash, **kein externer Nachweis**, dass das vollständige 50-Level-Buch in jedem Zwischenschritt korrekt war. Die zwei Demo-Verbindungen sind erfolgreiche **separate neue WS-Abos**, kein erzwungener Prozess-/Netzwerkfehler mit automatischem Resubscribe und frischem Snapshot in der Recovery-State-Machine. Offline-Recovery verwendet den **früher gespeicherten** Snapshot. Es wurde kein Rust-V5-Produktionscode getestet.

## 3. Originaldateien und SHA-256

| Nutzerdatei | SHA-256 (über bereitgestellte Datei hier verifiziert) |
|---|---|
| `DARKNODE_P1_GESAMTTEST_RESULT.txt` | `5493e2474cb6318943d8938b72584a14460c1e0d20a877c7c3bf28b40b73c071` |
| `DARKNODE_P1_GESAMT_LOG.txt` | `607532c9a9a1f68a735bed80ad8b10d640ab3c5450fb3d174adbc9dd3e68d03a` |
| `BITGET_R2_OFFLINE_FAULT_RESULT.txt` | `ac2dbc88db0bad68f875b0d10b2ebe297749fc0b7e236e936cc7c4c00ee7a601` |
| `bitget_r2_offline_fault.py` | `cd23986007e26c659f97c822ae53fd2be32712478cd428de6c064864d1ff94ab` |

`bitget_r2_offline_fault.py` wurde außerdem separat mit `python -m py_compile` auf Syntaxfehler geprüft (**PASS**). Die Originaldateien wurden **nicht** im Git-Repository abgelegt, sondern nur ihre Hashes und das ausgewertete Protokoll dokumentiert.

Die Datei `BITGET_R2_DEMO_SBE_CANDIDATE.capture` war bei diesem Upload **nicht enthalten**. Deren Hash `e4a0d1be4d49a1e1e7d397bc78efc175f96d55f940c23e40f56b6b5dff2f760f` ist durch den Nutzer-Test gegen den lokalen Mitschnitt bestätigt; er wurde hier **nicht selbst gegen Capture-Bytes nachgerechnet**.

## 4. Was weiterhin fehlt – konkretes P1-Gate, nicht erneut REST-Kurztests

| Gate | Ausstehende Abnahme |
|---|---|
| **R2/P1.4** | Offizielles SBE-V5-XML + Hash/Fixtures, vollständige 50-Level-Parität, echten Feedabriss und automatische Recovery über Resubscribe/neuen Snapshot, Rust-Code, Langzeit-Lane |
| **R2b/P1.7** | Download-Center-Originalarchive; vollständiger Coverage-/Gap-/Repair-Lauf; 429/Retry, Pagination-Endfälle, Archiv-Live-Konflikte |
| **R2c/R2d/P1.2/P1.5** | Rust L0/L1/L2-Binärdatenwriter, SQLite Control-Store, Crash-/File-vs-DB-Reconciliation, Durability/Performance |
| **R3/R4/P1.6** | Chrony/Uhrenqualitäts-Gates, Live-/Replay-Zeitsemantik, Bar-/MTF-Golden-Tests, Late-Trades |
| **P1.8** | Bestehendes DarkBot Protobuf-Gateway, mehrere Charts, Paging-Cursor, Reconnect |
| **P1.9** | Persistente Alarm-Outbox, lokaler Watchdog, **externer Heartbeat**, disk-full- und writer-backlog-Negativtests |
| **R6/K10** | Echter Host-/Region- und Libraryvergleich, reale V5-B/Tag auf mindestens 24h nach 05.11.2026 |
| **P1.10/T24** | 7 zusammenhängende Tage autonomer Betrieb auf festem Commit, mit allen Fehlerfällen/Evidenz |

**Testlabel-Korrektur:** Das Terminalskript sollte künftig T01–T23 nicht blanket als `BLOCKED` ausgeben. Jeder T-Fall braucht eine eigene ausführbare Prüfbedingung; ohne Prüfcode `NOT_RUN` oder `NOT_IMPLEMENTED`, bei tatsächlicher technischer Abhängigkeit `BLOCKED`; fachliche Python-Vorprüfung separat als `SPIKE_PASS`.

## 5. Entscheidung

Die **öffentliche API-Erreichbarkeit und mehrere elementare Live-/Offline-Dekoderfunktionen sind erfolgreich verifiziert**. Es gibt im Gesamtlauf **keinen beobachteten Fehler in den tatsächlich ausgeführten 16 Testgruppen**. Diese Evidenz kann die Exchange-Capability-/R2-Discovery fundieren. Sie **beweist nicht** die Verfügbarkeit einer fertigen P1-Rust-Marktdatenkette, die vollständige Historienabdeckung oder die Produktionssicherheit. Der P1-Phasenplan V1.1 bleibt unverändert, ebenso `P1 = NOT_RUN` und `GATE 1 = OPEN`.
