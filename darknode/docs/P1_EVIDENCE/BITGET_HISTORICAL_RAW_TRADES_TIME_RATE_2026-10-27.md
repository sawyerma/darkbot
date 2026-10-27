# DarkNode P1 – Historische Roh-Trades, Zeitauflösung und Ladegeschwindigkeit

**Datum:** 27.10.2026 · **BTCUSDT / Bitget USDT-Futures** · **Status:** echte API-/Mac-Messwerte; noch keine vollständige Backfill-Abnahme  
**Quelldatei:** `DARKNODE_ZEITAUFLOESUNG_DATENRATE.txt`, SHA-256 `d3fef8f385ecdeb53765cc13153413056ebb01674146d6b651938edce90d39e6`.  
**Testskript:** `darknode_datenrate_benchmark.py`, SHA-256 `32d6fc7d6c1afb571932856066ad59abecab7653d5c972229f2973a13950f531`. Die Bytes beider Uploads wurden am 27.10.2026 verifiziert. Die großen Rohcaptures liegen weiterhin auf dem Mac.  
**Vertrag:** [P1-Phasenplan](../PHASENPLAN_P1_V1.md) · [Endpoint-Registry](../ENDPOINT_REGISTRY.md).

## Verifizierte historische Datenquellen

| Quelle | Zeitstempel / kleinster im Test beobachteter Abstand | Verwendung in DarkNode |
|---|---|---|
| **Classic-V2-Public Einzeltrades** `GET /api/v2/mix/market/fills-history` | **Unix-ms; 1 ms** | **Primäre historische Rohdatenquelle für Backtesting.** Vollständige, qualifizierte Einzeltrades importieren und 1s, 5s, 10s, 30s, 1m sowie konfigurierte MTF-Candles **selbst** erzeugen |
| **Live-SBE `publicTrade`** | µs-Zeitstempelfeld, in diesem Live-Test Event-Timestamps auf 1 ms quantisiert | Echte Live-Roh-Trades für dieselbe eigene Candle-Pipeline; kein Umdeuten der Feldbreite als bewiesene Sub-ms-Trading-Präzision |
| V3 REST-Candles | 1m-Candles: **60.000 ms** zwischen Startstempeln | Nur Gegenprobe/ergänzende Referenz, **kein Ersatz** für qualifizierte Rohtrades bei 1s-/Tick-Backtests |
| Historische Orderbuch-Rohdaten | **nicht** in der getesteten Trade-REST-API enthalten | Nur aus gesondert belegtem Depth-Archiv oder eigenem L0/L1-WS-Recorder; **kein** synthetisches L2 aus Trades |

## API-Fähigkeiten und reale Historienleistung

- Verifizierte **Classic-V2** `fills-history`: bis 1.000 Trades/Request, `idLessThan` für ältere Seiten, bis 7 Tage pro abgefragtem Zeitfenster; maximal 90 Tage zurück gemäß real getesteter 89-/92-Tage-Grenze. **10 Requests/s/IP** ist dokumentiert und nicht als Dauerdurchsatz gemessen. 429/Retry, vollständige 90-Tage-Abdeckung und Endpunkt-Verfügbarkeit sind offen.
- **20.000 aktuelle Trades:** 20 Seiten; **8,632 s**, **2.317 Trades/s**, **280,6 kB/s REST-JSON**, 121,1 Byte/Trade. Enthaltener Marktzeitraum UTC 2026-10-26 23:50:27.780 – 2026-10-27 00:41:12.810 (**50 min 45,030 s**). Trade-TS: min. positiver Abstand **1 ms**, Median positiver Abstände 46 ms; **13.989 Timestamp-Kollisionen**, **0 doppelte Trade-IDs**.
- **5.000 Trades ca. 60 Tage alt:** 5 Seiten; **2,033 s**, **2.459 Trades/s**, **298,4 kB/s REST-JSON**, 121,3 Byte/Trade. UTC 2026-08-29 00:22:17.176 – 00:41:19.568 (**19 min 2,392 s**). Trade-TS min. positiver Abstand **1 ms**, Median positiver Abstände 67 ms; **3.216 Timestamp-Kollisionen**, **0 doppelte IDs**.
- **800 historische 1m-Candles:** 8 Seiten; **3,031 s**, **264 Candles/s**, **21,7 kB/s**, 82,1 Byte/Candle. UTC 2026-10-12 00:40 – 13:59 (**799 min Differenz der Startstempel; 800 min inklusive Candle-Dauer**).
- Die 20.000 bzw. 5.000 Trades deckten lokal **50 min 45 s** bzw. **19 min 2 s Marktzeit** ab. Daraus abgeleitete Ladebeschleunigung ca. **353×** bzw. **562×** – **kein repräsentatives Versprechen für 24h/90 Tage**, deren Trade-Dichte stark schwankt.

## Echte Live-Datenrate – nur 60-Sekunden-Stichprobe

| Kanal | Frames | Rate | SBE-Payloadrate | Auf 24h extrapoliert (nicht gemessen) |
|---|---:|---:|---:|---:|
| Live Depth50 V4 | 2.477 | 41,11/s | 68,07 kB/s | 5,882 GB |
| Live BBO V4 | 3.041 | 50,42/s | 4,03 kB/s | 0,348 GB |
| Live Trades V4 | 89 Frames / 160 Trades | 1,47 Frames/s, 2,64 Trades/s | 0,16 kB/s | 0,014 GB |
| Demo Depth50 V5 | 62 Frames | 1,02/s | 0,38 kB/s | **nicht qualifiziert** |

**Kritisch:** Beim 60s-Demo-V5-Benchmark gab es **8 Parserfehler von 62 Frames** (`ValueError('depth group')`). Nur 54 Frames wurden erfolgreich bis zu den Levelgruppen interpretiert; 1 Snapshot + 53 Deltas im ausgegebenen Teilresultat. V5-Decoder und vollständige Depth50-Interpretation **FAIL/INCONCLUSIVE**, und diese Demo-Rate taugt **nicht** als prognostizierter produktiver V5-Speicherverbrauch. Ein älterer kurzer Offline-V5-Mitschnitt bestand 25/25; daraus folgt **keine** Abnahme dieses längeren Laufs.

Im SBE-Event-Timestamp-Feld sind Mikrosekunden kodierbar, aber im Test waren **0 sub-ms Event-Timestamps** zu sehen. Ein separates `sts`-Feld zeigte Sub-ms-Anteile; dieses Feld darf ohne bestätigtes XML-Schema und Semantik **nicht** mit Trade-Ausführungszeit gleichgesetzt werden. Die grobe Empfangszeit-minus-`sts`-Differenz 115–118 ms ist **ohne Uhrqualifizierung keine belastbare Exchange→Host-Latenz**.

## Verbindlicher Backtest-Aggregationsvertrag

**Rohtrades → versioniertes L1 → 1s OHLCV → 5s/10s/30s/1m/konfigurierbare MTF.** Für `[12:00:00.000, 12:00:01.000)` bilden alle qualifizierten Trades in dieser Sekunde **Open** (erster deterministisch geordneter Trade), **High**, **Low**, **Close** (letzter), **Volume** (Menge aufsummiert), TradeCount und, wenn zuverlässig verfügbar, Buy-/Sell-Volumen.

**Wichtig:** Zahlreiche Trades teilen denselben Millisekunden-Zeitstempel. Deshalb sind Trade-ID, Quellidentität, originale Exchange-Zeit und Ingress-/Original-Reihenfolge verlustfrei zu speichern; weder Timestamp allein noch globale numerische Trade-ID-Monotonie als Ordnungsvertrag voraussetzen. Sekunden ohne Trade und unbekannte Datenlücken strikt unterscheiden; kein erfundener Tick. Nicht qualifizierte Intervalle für Backtests markieren bzw. sperren. Gleicher Datensatz + gleiche Policy ergibt identische 1s/MTF-Hashes; Korrekturen führen zu neuer Dataset-Version.

**Offen:** Bitget Download-Center >90 Tage, echte L2/Depth-Archive, 24h/90d-Import mit Coverage/429/Resume und Writer-Persistenz, Rust-Aggregator-Golden-Tests und V5-Decoder-Fehler. **P1 = NOT_RUN; Gate 1 = OPEN.**
