# DarkNode P1 – Bitget Public-API-Verifikation (Terminal-Evidence)

**Stand:** 27.10.2026 · **Status:** TEILWEISE VERIFIZIERT / P1 NICHT ABGENOMMEN  
**Repository:** `sawyerma/darkbot` · **Scope:** Bitget BTCUSDT, USDT-Futures, ausschließlich öffentliche Lese-APIs  
**Referenzen:** [PHASENPLAN_P1_V1.md](../PHASENPLAN_P1_V1.md), [GATE_1.md](../GATE_1.md)  
**Quelle der Evidenz:** Tatsächliche Terminal-Ausgaben vom Mac des Nutzers, im Chat am 26./27.10.2026 vorgelegt. Die Roh-Logdateien und ein vollständiger Mitschnitt der SBE-Frames sind **noch nicht versioniert**. Das Dokument ist eine protokollierte Beobachtung, kein Ersatz für reproduzierbare automatisierte Tests.

> **Keine Implementierungs-, ARM-, SIGN- oder LIVE-Freigabe.** Tests waren lesend, ohne API-Key/Handelsbefehle. `PASS (beobachtet)` bedeutet nur: Die beschriebene Eigenschaft wurde im angegebenen Messfenster tatsächlich nachgewiesen. Langzeitstabilität, Vollständigkeit aller Historie und Produktionskorrektheit sind damit nicht bewiesen.

## 1. Statusübersicht nach P1

| P1-Bereich | Beleg | Status / Einschränkung |
|---|---|---|
| P1.3 – Instrument-Katalog | V3-Instrument-REST, BTCUSDT-Parameter, 12 Position-Tiers | **PASS (Abruf und Schema);** Historisierung/Änderungserkennung nicht getestet |
| P1.3 – Funding | Aktueller Funding-Satz; 2 × 100 historische Einträge | **PASS (Abruf und zwei Seiten);** gesamte Reichweite nicht getestet |
| P1.4 – Public WebSocket | SBE `publicTrade`, `books1`, `books50`; JSON-`ticker`; ACK und Text-`pong` | **PASS (Kurztest);** Reconnect, Latenz, Alarm, Dauerbetrieb offen |
| P1.4 – SBE-V4-/V5-Decoder | V4: 235/235 Frames, 29/29 Trades REST-paritätisch; **Demo-V5:** 25/25 Frames, 1 Snapshot + 24 Updates angewandt | **PASS für beobachtete Kurztests**; V5-Buchparität/Recovery, Produktiv-V5, Rust-Code offen |
| P1.7 – öffentliche Trade-Historie | V2 `fills-history`, 1.000er-Seiten, Cursor, 60/89 Tage, Grenzfehler | **PASS (getestete Abfragen);** tatsächliche gesamte Coverage / Retry offen |
| P1.7 – historische Candles | V3-Kerzen, Mark/Index und 120 Tage alte 1h-Kerzen | **PASS (Abruf);** OHLCV-Parität noch nicht geprüft |
| R2b – Download-Center | Kein Export heruntergeladen oder gegen Live verglichen | **NOT_RUN** |
| P1.5/1.6/1.8/1.9/1.10 – Store, Aggregation, Gateway, Health, Dauerlauf | Nicht implementiert/ausgeführt | **NOT_RUN** |
| Gesamter P1-Phasenplan / Gate 1 | Nicht vollständig qualifiziert | **OPEN / NOT_RUN** |

## 2. Nachgewiesene REST-Endpunkte

Alle erfolgreichen Antworten meldeten `code=00000`. Zeiten sind **Einzelmessungen vom Mac inklusive Netzwerk**, keine garantierten Latenzen. Parameter wurden so wie angegeben getestet; sie sind keine allgemeine Aussage über alle Produkte/Accounts.

| API | Tatsächlich getesteter Aufruf / Zweck | Beobachtung |
|---|---|---|
| `GET /api/v3/market/time` | Serverzeit | HTTP erfolgreich; `data.serverTime`, `data.serverTimeMicros`; 392 ms |
| `GET /api/v3/market/instruments` | `category=USDT-FUTURES&symbol=BTCUSDT` | 1 Instrument; 350 ms |
| `GET /api/v3/market/tickers` | BTCUSDT USDT-FUTURES | 1 Ticker; 342 ms |
| `GET /api/v3/market/orderbook` | BTCUSDT, `limit=50` | `a`, `b`, `ts`; 349 ms |
| `GET /api/v3/market/fills` | BTCUSDT, `limit=100` | 100 öffentliche aktuelle Trades; `execId, price, size, side, ts, execLinkId, isRPI`; 356 ms |
| `GET /api/v3/market/candles` | `interval=1m&type=market&limit=25` | 25 Zeilen, 7 Werte/Zeile; 351 ms |
| `GET /api/v3/market/candles` | `interval=1m&type=mark&limit=10` | 10 Zeilen, 7 Werte/Zeile; 355 ms |
| `GET /api/v3/market/candles` | `interval=1m&type=index&limit=10` | 10 Zeilen, 7 Werte/Zeile; 366 ms |
| `GET /api/v3/market/history-candles` | Erst 24 × 1h in einem alten Fenster, später explizit ca. 120 Tage vorher | 24 Zeilen/7 Werte; Test des 120-Tage-Fensters erfolgreich |
| `GET /api/v3/market/current-fund-rate` | BTCUSDT USDT-FUTURES | 1 Funding-Objekt; 363 ms im ersten Durchlauf |
| `GET /api/v3/market/history-fund-rate` | BTCUSDT, `cursor=1/2&limit=100` | Zwei verschiedene Seiten mit je 100 Einträgen |
| `GET /api/v2/mix/market/query-position-lever` | `productType=usdt-futures&symbol=BTCUSDT` | 12 Position-Tiers |
| `GET /api/v2/mix/market/fills-history` | `productType=usdt-futures&symbol=BTCUSDT` + Cursor/Zeit | Öffentliche Einzeltrade-Historie und Pagination nachgewiesen |

**Wichtige Trennung:** V3 `/market/fills` liefert hier aktuelle öffentliche Trades; für den bestätigten historischen Rückwärtsabruf wurde **V2 `/mix/market/fills-history`** verwendet. Private Konto-Fills/`/trade/fills` sind **keine** Quelle für öffentliche Markt-Historie.

## 3. P1.7 – Historische REST-Trades: Pagination und Grenzen

**Endpunkt:** `GET https://api.bitget.com/api/v2/mix/market/fills-history`  
**Testparameter:** `symbol=BTCUSDT`, `productType=usdt-futures`; `limit` = 5, 10, 100 oder 1000; optional `idLessThan`, `startTime`, `endTime`.

### 3.1 Seiten- und Zeitfenster-Tests

| ID | Prüfung | Reales Ergebnis | Bewertung |
|---|---|---|---|
| HIST-01 | aktuelle 100 Trades | 100 Zeilen, 384 ms | PASS |
| HIST-02 | Folge-Seite über `idLessThan=letzte tradeId` | 100 ältere Zeilen, 342 ms; 0 überlappende IDs | PASS |
| HIST-03 | altes Tagesfenster ca. 60 Tage | 5 öffentliche Trades, 375 ms | PASS |
| HIST-04 | aktuelle Seite mit `limit=1000` | 1.000 Zeilen, 442 ms; IDs streng fallend | PASS |
| HIST-05 | zweite Seite mit `limit=1000` | 1.000 Zeilen, 395 ms; alle IDs älter, 0 Überlappung | PASS |
| HIST-06 | Tagesfenster ca. 60 Tage alt, 1. Seite | 1.000 Zeilen, 415 ms; innerhalb des Fensters | PASS |
| HIST-07 | dasselbe Fenster, 2. Seite | 1.000 Zeilen, 390 ms; 0 Überlappung | PASS |
| HIST-08 | Fenster ca. 89 Tage alt | 10 Zeilen, 378 ms | PASS |
| HIST-09 | `startTime/endTime` genau 7 Tage auseinander | HTTP 200, `00000`, 5 Zeilen | PASS |
| HIST-10 | `startTime/endTime` 8 Tage auseinander | HTTP 400, Code `00001` | erwartete Ablehnung bestätigt |
| HIST-11 | `startTime` ca. 92 Tage alt | HTTP 400, Code `40934` | erwartete Ablehnung bestätigt |

**Original-Fehlerantworten (aus Terminal):**

```json
{"code":"00001","msg":"startTime and endTime interval cannot be greater than 7 days","requestTime":1793056006405,"data":null}
{"code":"40934","msg":"startTime is not allowed to be earlier than 90 days ago","requestTime":1793056007725,"data":null}
```

**Schlussfolgerung:** Mit diesem Endpoint funktioniert öffentliche BTCUSDT-Trade-Pagination über `idLessThan`, einschließlich historischer Tagesfenster. `limit=1000` ist akzeptiert und liefert 1.000 Records, und die getesteten 7-/90-Tage-Grenzen werden vom Server durchgesetzt. `tradeId` wird als verlustfreier Ganzzahlbezeichner (mindestens `u64` im validierten Wertebereich) transportiert; im JavaScript-Frontend keine unsichere Konvertierung zu `number`.

**Nicht bewiesen:** Vollständigkeit über Millionen Datensätze, lückenlose 90-Tage-Abdeckung, Seitengrenzen bei identischen Zeitstempeln, Rate-Limit-/429-Verhalten, Erholung nach Timeout, Kursänderungen/Trade-ID-Eindeutigkeit über weitere Märkte oder die exakte Exklusivität an *allen* Cursor-Grenzen.

## 4. P1.4 – WebSocket- und SBE-Nachweise

**SBE-URL:** `wss://ws.bitget.com/v3/ws/public/sbe`; Subscription mit `instType=usdt-futures`, `symbol=BTCUSDT`.  
**JSON-WS:** `wss://ws.bitget.com/v3/ws/public`, Topic `ticker`.  
**Heartbeat:** Client sendete Text `ping`, Server antwortete `pong` während des Kurztests. Das belegt keine unbegrenzte Liveness.

### 4.1 Erster Transporttest

| Topic | SBE-Template-ID | Frames | Beobachtung |
|---|---:|---:|---|
| `publicTrade` | 1003 | 23 | ACK=1, `pong`=1; SBE Schema-ID=1, Version=4; Headerbeispiel 76 Byte |
| `books1` | 1002 | 807 | ACK=1, `pong`=1; Schema 1, Version 4; Headerbeispiel 80 Byte |
| `books50` | 1001 | 463 | ACK=1, `pong`=1; Schema 1, Version 4; Headerbeispiel 1.656 Byte |
| JSON `ticker` | – | 84 Datennachrichten | ACK=1, `pong`=1, kein Fehler |

### 4.2 Zweiter Test: tatsächliche V4-Dekodierung

| Topic | Empfangen | Dekodiert | Reale Felder/Befunde |
|---|---:|---:|---|
| `publicTrade` (1003) | 11 | 11 | **29 Einzeltrades**, davon 8 Multi-Trade-Frames; V4, Root=16 Byte, Gruppen-Entry=40 Byte |
| `books1` (1002) | 55 | 55 | `seq`, Bid/Ask und Mengen; Beispiel Bid `79850.0`, Ask `79850.1` |
| `books50` (1001) | 169 | 169 | **169 Snapshots**, jeweils 50 Ask- und 50 Bid-Levels; `pseq=None`, `action=0` im V4-Parser |

**Summe:** **235/235 Frames** dekodiert; in dieser Stichprobe `decode_failed=0` und keine gemeldete rückläufige `seq`.

**SBE ↔ REST-Abgleich:** Alle **29 SBE-Trades** waren unter derselben ID in der 1.000er-REST-Seite enthalten; `field_mismatches=0` für `tradeId`, Preis, Menge, Richtung und Millisekunden-Zeitstempel. Beispiel einer SBE-Trade-ID: `1498752928271056898`. Die am selben Timestamp gelieferten Trades können in einem Frame absteigende IDs besitzen: nicht von aufsteigender Feed-Reihenfolge ausgehen.

**Einschränkung:** Der bisherige Python-Spike beweist die beobachtete V4-Struktur, **nicht** den endgültigen aus Bitgets XML generierten Rust-Decoder, die exakte Vollständigkeit unter Paketverlust oder korrekte Orderbuch-Rekonstruktion bei Delta-Updates. Das SBE-V5-Schema und `books50` Snapshot+Delta (`pseq`/Action), Schema-Hash und kontrollierte Disconnect-/Resync-Tests bleiben **OPEN**. Die hier gesehenen V4-Vollsnapshots sind kein dauerhafter Vertrag für alle künftigen SBE-Versionen.

## 5. P1.3 – Instrumentdaten, Tiers und Funding

### 5.1 Instrument-Katalog – BTCUSDT, USDT-FUTURES

Die echte Antwort von `/api/v3/market/instruments` enthält unter anderem:

| Feld | Beobachteter Wert |
|---|---|
| `symbol`, `category` | `BTCUSDT`, `USDT-FUTURES` |
| `status`, `type` | `online`, `perpetual` |
| `priceMultiplier` / `quantityMultiplier` | `0.1` / `0.0001` |
| `pricePrecision` / `quantityPrecision` | `1` / `4` |
| `minOrderQty` / `maxOrderQty` | `0.0001` / `1200` |
| `minOrderAmount` | `5` |
| `maxMarketOrderQty` | `220` |
| `makerFeeRate` / `takerFeeRate` | `0.0002` / `0.0006` |
| `fundInterval` | `8` |
| `minLeverage` / `maxLeverage` | `1` / `150` |

**Keine Schlussfolgerung über eigene Accountgebühren oder tatsächlich freigegebenen Hebel!** Die Werte stammen aus einem **öffentlichen Instrument-Snapshot**. Für präzise Tick-/Lot-Umrechnung müssen die Multiplikatoren je Instrument-Version gesichert und geprüft werden; niemals aus `precision` allein auf alle Regeln schließen.

### 5.2 Position-Tiers

`GET /api/v2/mix/market/query-position-lever` ergab **12 Tiers**.

- Tier 1: `startUnit=0`, `endUnit=200000`, `leverage=150`, `keepMarginRate=0.0040`.
- Tier 12: `startUnit=900000000`, `endUnit=1200000000`, `leverage=1`, `keepMarginRate=0.6000`.

Die Zwischen-Tiers wurden im Terminal lediglich als Anzahl bestätigt, **nicht vollständig als Tabelle protokolliert**. Der Production-Katalog muss später die vollständige Antwort versioniert sichern. Kein Test einer echten Margin-/Order-Situation.

### 5.3 Funding aktuell und historisch

`GET /api/v3/market/current-fund-rate` gab ein BTCUSDT-Objekt mit `fundingRate=0.0001`, `fundingRateInterval=8` sowie `nextUpdate`, `lastFundingRate`, `nextFundingRate`, `minFundingRate`, `maxFundingRate`. Es ist eine **Momentaufnahme**, keine Gebühren-/Renditeprognose.

`GET /api/v3/market/history-fund-rate` mit `cursor=1` und `cursor=2`, jeweils `limit=100`, lieferte **100 + 100** Einträge in `data.resultList`, mit `fundingRateTimestamp` und `fundingRate`.

| Prüfung | Terminalbefund |
|---|---|
| Seite 1 | 100 Einträge, erstes Timestamp `1793030400000`, letztes `1790179200000` |
| Seite 2 | 100 Einträge, erstes `1790150400000`, letztes `1787299200000` |
| Overlap | **0** gleiche Timestamps zwischen beiden Seiten |
| Weitere Cursors, Maximalreichweite, Lückenfreiheit | **NOT_RUN** |

### 5.4 120 Tage alte historische Candles

`GET /api/v3/market/history-candles` mit `interval=1H`, `type=market`, einem etwa **120 Tage alten 24h-Zeitfenster** und `limit=100` gab HTTP-Erfolg, `code=00000` und **24 × 7 Werte**. Beispiel erster Datensatz:

```text
["1782687600000","59046.9","59831","58985.6","59553.4","2160.6278","128557628.78972"]
```

**Wichtig:** Ältere **Candles** sind nicht mit einem älteren **Einzeltrade-Archiv** gleichzusetzen. Der REST-Einzeltick-Endpunkt hat in den Tests 90 Tage als Grenze durchgesetzt. OHLCV-Abgleich gegen eigene Trades / genaue verfügbare Candle-Grenze noch offen.

## 6. Noch offene Gate-1-Nachweise

| ID | Offene Frage / erforderliche Evidenz |
|---|---|
| R2 / P1.4 | XML-Schema mit Hash fixieren, Rust-SBE-Decoder/Fixtures, V5-Snapshot+Delta (`pseq`) vollständig testen, Resync nach Sequenzbruch |
| R2 / P1.4 | Text-Ping-Intervall, Reconnect/Backoff, zweite WS-Verbindung, Rate-/Burst-Verhalten, reale Host-Latenzen und Feed-Vollständigkeit über längere Zeit |
| R2b / P1.7 | Bitget Download-Center: Originaleinzeltrades inkl. IDs? Zeitraum, Integrität, Lieferformat, Importdauer, Überlappung mit eigenem Live-Capture |
| R2b / P1.7 | REST-Abdeckung über ganze Fenster, Rand-/Gleichzeitstempel, Retry/429, Wiederaufnahme, echte Gap-Reparatur mit Coverage-Ledger |
| P1.3 | Änderungsereignisse, vollständige Tier-Historie, historische Instrumentversionen, Funding-Randfälle und Page-Ende |
| R2c/R2d / P1.5 | Binär-Store vs. Benchmark, Journal-Durability, Crash-Recovery, SQLite-Metadaten-Konsistenz; keine Speichervariante per API-Kurztest freigeben |
| R3/R4 / P1.6 | Zwei-Uhren-Regeln, Late Trades, Bar-Ende, deterministisches Replay, OHLCV-Golden-Vectors |
| P1.8 | Gateway-Protobuf, unabhängige Chart-Paging-Cursors, History/Live-Merge, Reconnect |
| P1.9/P1.10 | Health/Alarm, tatsächlicher 7-Tage-Dauerlauf und Last-/Latenzmessungen |

## 7. Lokal erzeugte Evidenzdateien (noch nicht ins Repo kopiert)

Folgende Dateinamen wurden durch die ausgeführten Terminalbefehle verwendet:

```text
~/Downloads/BITGET_P1_TERMINAL_EVIDENCE.md
~/Downloads/BITGET_P1_TERMINAL_LOG.txt
~/Downloads/BITGET_P1_HISTORY_REAL.txt
~/Downloads/BITGET_P1_HISTORY_LIMITS.txt
~/Downloads/BITGET_P1_BOUNDARY_ERRORS.txt
~/Downloads/BITGET_P1_SBE_DECODE_RESULT.txt
~/Downloads/BITGET_P1_REFERENCE_DATA.txt
```

Die Rohlogs wurden dem Repository **nicht** hinzugefügt und nicht mit einem SHA-256-Beweis gesichert. Bitte die unveränderten Originaldateien bei Gelegenheit als separate Artefakte mit Hash und reproduzierbarem Testskript sichern. Diese Übersicht enthält ausschließlich die im Chat sichtbaren Terminalbefunde. Bei künftigen Tests: UTC-Messbeginn/-ende, verwendetes Skript und Version, Endpoint/Parameter, Antwortdaten (ohne Secrets), PASS-/FAIL-Regel, Git-Commit und SHA-256 pro Evidenzartefakt erfassen.

## 8. Verbindlicher Status

**R2 Public Transport/REST-Fähigkeiten:** in den getesteten Punkten **OBSERVED/PASS**.  
**P1.3, P1.4, P1.7:** **teilweise verifiziert**, verbleibende Prüfungen **OPEN/NOT_RUN**.  
**Gesamt-P1:** **NOT_RUN**. **Gate 1:** **OPEN**. Keine Freigabe zum Handel oder zur Implementierung weiterer Phasen durch dieses Dokument.

## 9. R2 – Demo-SBE V5: echter BTCUSDT-Depth50-Test (27.10.2026)

**Quelle:** Nutzer-Terminalausgabe aus `bitget_r2_depth_probe.py` unter macOS; drei serielle öffentliche WS-Messfenster (15 s / 10 s / 6 s) mit `symbol=BTCUSDT`, `instType=usdt-futures`, `topic=books50`. Empfangene rohe WS-Frames wurden **lokal** als `DNR2CAP1`-Captures mit `time_ns, frame_type, length, payload` gespeichert. Die Captures selbst wurden dem Git-Repository **nicht** übergeben; Hashes sind **vom lokalen Terminal gemeldet, nicht unabhängig durch einen Git-/Container-Readback überprüft**. Keine Orders und kein API-Key.

| Quelle und URL | Beobachtung | Bewertung |
|---|---|---|
| **DEMO_JSON** `wss://wspap.bitget.com/v3/ws/public` | 3 Textframes: Subscribe-ACK=1, Pong=1, **1 Snapshot**. Best Bid 80150.7, Ask 80155.3, 50+50 Level, seq=27298460305, pseq=0 | **OBSERVED** JSON-Demo-Snapshot; keine JSON-Updates im Kurztest |
| **DEMO_SBE_CANDIDATE** `wss://wspap.bitget.com/v3/ws/public/sbe` | 27 Frames: ACK=1, Pong=1, **25 echte binäre V5-Frames**; **1 Snapshot + 24 Updates**; 25 `applied`, kein gemeldeter Parser-/Sequenzfehler | **OBSERVED V5**. Der vorher nur vermutete `/sbe`-Demo-Pfad war zur Messzeit tatsächlich erreichbar; keine Garantie gegen spätere Änderungen |
| **LIVE_SBE_BASELINE** `wss://ws.bitget.com/v3/ws/public/sbe` | 221 Frames: ACK=1, Pong=1, **219 × V4**; keine V5-Buchverarbeitung versucht | **OBSERVED V4** vor dem offiziellen Live-Wechsel am 05.11.2026 |

**V5-Sequenzkette (echte Ausgabestichprobe):**

```text
snapshot  seq=27298460305  pseq=0             bid=80150.7 ask=80155.3 levels=50/50
update 1 seq=27298476413  pseq=27298460305   bid=80150.7 ask=80155.3 levels=50/50
update 2 seq=27298476475  pseq=27298476413   bid=80150.7 ask=80155.6 levels=50/50
```

Die erste JSON- und SBE-Messung zeigen denselben Snapshot-Sequenzwert und denselben Top-of-Book. **Messungen liefen nacheinander**, nicht als zeitgleicher, unabhängiger 50-Level-Paritätstest. 24 Updates wurden auf eine lokale Dezimal-Preis→Menge-Tabelle angewandt; das Terminal meldet kein `gap`, kein `crossed_book` und keinen `decode_error`. Es wurden **nicht** alle resultierenden 50 Tiefenstufen nach jedem Delta gegen eine unabhängige Referenz validiert. Der Python-Test akzeptiert eine weiter gefasste erste Update-Sequenzbedingung und ersetzt keinen formal geprüften Rust-State-Machine-Test. Keinesfalls als Produktionsabnahme deklarieren.

**Lokal gemeldete Originalcapture-Hashes (SHA-256):**

| Capture in `~/Downloads` | SHA-256 (Terminalmeldung) |
|---|---|
| `BITGET_R2_DEMO_JSON.capture` | `3903dcff42a1f35fbcd8d41574426d94b1239a61fff7d21d1c8004ed28f1154f` |
| `BITGET_R2_DEMO_SBE_CANDIDATE.capture` | `e4a0d1be4d49a1e1e7d397bc78efc175f96d55f940c23e40f56b6b5dff2f760f` |
| `BITGET_R2_LIVE_SBE_BASELINE.capture` | `76f393f4ba26e2c2ddea9479801be19eda964a5f0c56b7acea6814134e8bd61e` |

**Externe Primärquellen:** [Bitget Update Preview – V5 und 05.11.2026](https://www.bitget.com/docs/uta/update-preview), [SBE XML-Schema](https://www.bitget.com/docs/uta/websocket/sbe/sbe-intro), [Demo-Quickstart](https://www.bitget.com/docs/uta/quick-start). Bitgets Quickstart dokumentiert den Demo-JSON-Host; der ergänzte Demo-SBE-Pfad ist **praktisch ermittelt**, nicht als offizieller Quickstart-Host nachgewiesen.

**R2-Folgeprüfungen / OPEN:** Originalcapture auf Checksumme/Format prüfen, echtes V5-Schema als Hash pinnen, kanonisches deterministisches Offline-Replay, absichtlich entfernten/duplizierten Frame als `GAP` erkennen, Resubscribe+neuen Snapshot validieren, 50-Level-Parität über gesamte Sequenz (idealerweise mit parallel synchronisiertem Referenzfeed), nach 05.11.2026 live V5 prüfen, schließlich Rust-Decoder und Burst-/Dauerlauf testen. Die bisherige `P1=NOT_RUN`- und `Gate 1=OPEN`-Einstufung bleibt unverändert.
