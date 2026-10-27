# DarkNode – Endpoint-Registry V1.1

**Stand:** 27.10.2026 · **Status:** Gate-1-Dokumentation, keine Implementierungsfreigabe  
**Referenzen:** [P1-Phasenplan](PHASENPLAN_P1_V1.md) · [Bitget-Terminal-Evidenz](P1_EVIDENCE/BITGET_PUBLIC_API_VERIFIKATION_2026-10-27.md)

## Verbindliche Architektur
- **SQLite als Runtime-Control-Speicher:** unveränderliche versionierte Protobuf-BLOBs, aktive Versionspointer, Instrument-Mapping, Audit, Coverage-/Gap-Ledger, Manifest-Watermarks und Alarm-Outbox. Aktivierung atomar in einer SQLite-Transaktion. Textproto nur als Seed/Fixture, keine aktiven verteilten `.binpb`-Wahrheiten. Datei-Segmente und SQLite haben **keine gemeinsame atomare Transaktion**: geordnetes Commit-Protokoll, fsync, Wasserzeichen und Reconciliation sind Pflicht.
- **Marktdaten:** L0 Rohframes, L1 typisierte Events, L2 Candle-Cache als versionierte Binärdateien. Kein produktives Redis, ClickHouse oder Parquet.
- **DN-D18 No JSON intern:** fremde Bitget-JSON-Nachrichten nur am Adapterrand typisieren; L0 darf Originalframes opak sichern. Danach Rust/Protobuf/binäre Formate.
- **Sicherheit:** öffentliche APIs ohne Key; Host-Allowlist und TLS unabhängig von Konfiguration erzwingen; keine frei definierbaren Fremdhosts oder Host-Redirects. Private Order-/Wallet-/Trade-Keys und Order-Endpunkte gehören nicht zu P1.
- **UI-Ort:** offen (O11), nicht automatisch Wallet; bestehende fünf DarkNode-Tabs bleiben unverändert.

## Öffentliche Basisadressen

| Transport | Adresse | Nachweis |
|---|---|---|
| REST | `https://api.bitget.com` | Real getestet |
| Public SBE | `wss://ws.bitget.com/v3/ws/public/sbe` | Real getestet, Schema 1 / V4 |
| Public JSON WS | `wss://ws.bitget.com/v3/ws/public` | Real getestet |
| **Demo Public SBE** | **`wss://wspap.bitget.com/v3/ws/public/sbe`** | **OBSERVED:** 25 echte V5-Depth50-Frames im öffentlichen 10-s-Terminaltest (1 Snapshot, 24 Deltas); Demo-SBE-Pfad praktisch ermittelt, nicht aus offiziellem Quickstart garantiert |
| VIP/Demo Private | Host und Berechtigung separat verifizieren | Offen |

**Parameterformen:** V3 REST: `category=USDT-FUTURES`; Classic V2 REST: `productType=usdt-futures`; Public WS: `instType=usdt-futures`. Symbol im Test `BTCUSDT`.

## Öffentliche REST-Endpunkte – reale Terminalbelege

| Pfad (GET) | Verifizierte Funktion | Stand |
|---|---|---|
| `/api/v3/market/time` | `serverTime`, `serverTimeMicros` | OBSERVED |
| `/api/v3/market/instruments` | BTCUSDT-Snapshot: `priceMultiplier=0.1`, `quantityMultiplier=0.0001`, Gebühren- und Kontraktfelder | OBSERVED |
| `/api/v3/market/tickers` | 1 BTCUSDT-Ticker | OBSERVED |
| `/api/v3/market/orderbook` | `limit=50`, `a/b/ts` | OBSERVED |
| `/api/v3/market/fills` | 100 aktuelle öffentliche Trades (`execId`); **kein bestätigter historischer Cursor** | OBSERVED recent |
| **`/api/v2/mix/market/fills-history`** | **Öffentliche Einzeltrades**, `limit=1000`, `idLessThan`, Zeitfenster ≤7 Tage, Start ≤90 Tage zurück; GET ohne API-Key | **PUBLIC_V2_HISTORY_VERIFIED** |
| `/api/v3/market/candles` | Market-, Mark-, Index-Candles (je 7 Felder) | OBSERVED |
| `/api/v3/market/history-candles` | 24 Stundenkerzen aus ca. 120 Tage altem Fenster | OBSERVED |
| `/api/v3/market/current-fund-rate` | aktueller Funding-Snapshot | OBSERVED |
| `/api/v3/market/history-fund-rate` | `cursor=1/2`, `limit=100`; 200 Einträge, 0 Timestamp-Overlap | OBSERVED |
| `/api/v2/mix/market/query-position-lever` | 12 BTCUSDT-Position-Tiers | OBSERVED |

**V2-Historie – konkrete Grenztests:** 1.000 + 1.000 Trades ohne ID-Überschneidung, ebenso 60 Tage altes Fenster; 89 Tage alte Trades abrufbar; exakt 7 Tage HTTP 200; 8 Tage HTTP 400 Bitget `00001`; 92 Tage zurück HTTP 400 Bitget `40934`. Dokumentierte **10 Requests/s/IP nicht real lastgetestet**. Vollständigkeit, Cursor-Endfälle, Retry/429, Ausfall-/Recovery-Betrieb weiterhin R2b.

**Instrument-Snapshot:** `minOrderQty=0.0001`, `minOrderAmount=5`, `priceMultiplier=0.1`, `quantityMultiplier=0.0001`, `fundInterval=8`, `makerFeeRate=0.0002`, `takerFeeRate=0.0006`, `minLeverage=1`, `maxLeverage=150`. Werte als öffentliche Momentaufnahme; keine Bestätigung kontospezifischer Gebühren oder real handelbarer Hebel.

## Public WebSocket – reale Kurztests

| Kanal | Template | Beobachtung |
|---|---:|---|
| SBE `publicTrade` | 1003 | Schema-ID 1/V4; 11/11 Frames dekodiert, 29 Trades; 29/29 Felder gegen REST identisch |
| SBE `books1` | 1002 | Schema-ID 1/V4; 55/55 Frames mit Bid/Ask, Mengen, `seq` |
| SBE `books50` | 1001 | Schema-ID 1/V4; 169/169 vollständige 50x2-Level-Snapshots |
| JSON `ticker` | — | 84 Live-Datenframes |

Alle getesteten Subscriptions erhielten ACK und auf Text-`ping` ein `pong`. **Keine** Langzeit- oder Reconnect-Abnahme.

**SBE V5:** Bitget kündigt für den 05.11.2026 Depth50-Snapshot gefolgt von Deltas mit `depthAction`, `seq/pseq` an (Demo schon umgestellt). V5 als neues **Live-Zielprofil**; vorliegende V4-Frames ausschließlich versioniert offline/CLI replaysicher lesbar. Unbekannte Live-Version nicht still dekodieren; Resnapshot nach Sequenzlücke zwingend. **V5-Demo real getestet (25/25 V5-Frames, 1 Snapshot + 24 Updates, 0 gemeldete Fehler)** über `wss://wspap.bitget.com/v3/ws/public/sbe`. **V5-Offline-Fault-Spike auf dem gespeicherten echten Capture: PASS** (SHA-256, deterministisches Replay, entferntes/vertauschtes Delta, exaktes Duplikat, Replay des gespeicherten Snapshots); siehe [Evidenz §10](P1_EVIDENCE/BITGET_PUBLIC_API_VERIFIKATION_2026-10-27.md). **Weiter NOT_RUN:** echter Live-Resubscribe/Neusnapshot, vollständige Top-50-Buchparität, Rust-V5-Decoder und V5-Live nach 05.11.2026. [R2-Messdaten/Hashes](P1_EVIDENCE/BITGET_PUBLIC_API_VERIFIKATION_2026-10-27.md#9-r2--demo-sbe-v5-echter-btcusdt-depth50-test-27102026).

## Betrieb / offen

- **P1.9:** SQLite-Alarm-Outbox für STALE/GAP/Writer-/Plattenfehler; unabhängiger Host-Prozesswatchdog; **externer, separat alarmierender Heartbeat-Empfänger** für Totalausfall von Host/VM/Netz. Adapter/Intervalle noch offen.
- **Zeit:** chrony überwachen; `CAPTURE_READY` und `TIME_QUALIFIED` getrennt; `CLOCK_UNSYNC` kennzeichnet unsichere Empfangszeit.
- **L0-Löschung (O13):** nur nach Frist, Hashprüfung von L0 und neu dekodiertem kanonischen L1, Persistenz-/Coverage-/Audit-Gates; L0-Originalbytes sind aus L1 nicht rekonstruierbar. 30 Tage nur Vorschlag.
- **Hostregion:** Tokyo als R6-Messkandidat. Nutzer verweist auf Tardis-Angabe „Bitget AWS ap-northeast-1“; **Drittanbieterangabe, keine verifizierte Bitget-Bestätigung**.
- **K10:** V5-Depth50-Speichervolumen erst mit repräsentativem ≥24h-Live-Mitschnitt nach 05.11.2026 messen. V4 nur Vergleich.
- **R2b:** Download-Center original prüfen (Futures-Trades/IDs, Tiefe, Zeitraum, Checksummen, Import/Overlap). Keine Aussage zur Bulk-Abdeckung ohne Export.

**Vertragsstatus:** Gate 1 OPEN, P1 NOT_RUN; keinerlei Trading-Freigabe.
