# Kostenlose WebSocket-Quellen messen — Zahlen und Anleitung

**Stand:** 2026-10-22 · read-only, kein Key, kein Senden

## 1. Welche Zahlen es heute gibt

### Eigene Messungen (Ledger, `bsc/docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md`)

| Messung | Ergebnis | Einschränkung |
|---|---|---|
| QuickNode-WSS, volle Pending-TX, 2026-09-29 | **4 von 4** Flap-V6-Launches vor dem Block als Pending gesehen; 42.886 Pending-TX in 197,7 s | winzige Stichprobe. Abbruch durch den Provider nach Ratenlimit (15 Anfragen/s), Ursache waren zusätzliche Einzelabfragen |
| dRPC (frühere Versuche, Archivabfragen) | DNS-, HTTP-500- und HTTP-429-Fehler | betraf Archiv-/Debug-Abfragen, nicht den Pending-Stream |
| Testnet, eigener Launch über P2P | Launch → erster Hash **133,4 ms** → volle TX **234,6 ms**; volle TX → BUY gesendet **0,535 ms** | Testnet, eigener Launch |

### Veröffentlichte Vergleichszahlen von Anbietern

Diese Anbieter messen **ihren bezahlten Dienst gegen einen „normalen Node"**. Ein kostenloser öffentlicher Endpunkt gehört zur Klasse „normaler Node".

| Quelle | Ergebnis | Einschränkung |
|---|---|---|
| BlockRazor Public Mempool (Doku, aktuell) | Der bezahlte Stream war in 99,5–99,8 % der Fälle schneller als ein normaler Node. Vorsprung im Mittel: Frankfurt 29,0 ms, Dublin 43,7 ms, Virginia 55,1 ms, Tokio 136,4 ms. P90: 48,1 / 89,7 / 107,4 / 218,7 ms | Eigenmessung des Anbieters. Preis 300 $/Monat je Stream |
| bloXroute mit CertiK, BSC | Gateway sah 98 % der TX zuerst, im Mittel 619 ms schneller als ein normaler BSC-P2P-Node | **August 2022**, veraltet; Anbietermessung |

### Sniper-Projekte

**Kein Sniper-Projekt veröffentlicht nachprüfbare Latenzzahlen.** Das gilt für kommerzielle wie für Open-Source-Projekte. Das Open-Source-Projekt `swiftnodes/mempool-watcher` (kostenlose WSS-Pending-Subscription) nennt selbst „50–300 ms tx-to-print". Das ist eine eigene Angabe, keine Messung. Dasselbe Projekt weist darauf hin, dass private MEV-Pools auf BSC über öffentliche RPCs nicht sichtbar sind.

**Was über Konkurrenten wirklich messbar ist:** BR0 zeigt, wer auf N+1 kauft und ob im selben Block. Das ist das Ergebnis der Konkurrenten, nicht ihre interne Latenz.

### Folgerung

- Eine Zahl für „kostenlose WSS-Quelle X sieht Four.meme- und Flap-Launches so früh" **existiert nicht**. Sie muss gemessen werden, an deinem Standort.
- Nach den Anbieterzahlen liegt eine kostenlose Quelle typischerweise **Dutzende Millisekunden** hinter bezahlten Streams.
- Entscheidend ist aber eine andere Frage: Sieht die kostenlose Quelle den Launch **überhaupt vor dem Block**? Genau das misst das Werkzeug.

## 2. Messen auf dem Mac

**Voraussetzung:** Node 22 oder neuer (`node --version`). Das Werkzeug braucht keine Pakete.

```bash
# 30 Minuten, zwei kostenlose Quellen
SOURCES="publicnode=wss://bsc-rpc.publicnode.com,drpc=wss://bsc.drpc.org" \
DURATION_S=1800 node wss_race.mjs

# mit eigenem Node als dritte Quelle (falls vorhanden)
SOURCES="publicnode=wss://bsc-rpc.publicnode.com,drpc=wss://bsc.drpc.org,mygeth=ws://127.0.0.1:8546" \
DURATION_S=1800 node wss_race.mjs
```

Mit Strg+C vorzeitig beenden; die Auswertung wird trotzdem geschrieben.

**Ausgabe:**
- `wss_race_launches.csv`: eine Zeile je Launch, Status und Vorsprung je Quelle
- `wss_race_summary.txt`: Kennzahlen je Quelle

**Empfehlung:**
- mehrere Läufe zu verschiedenen Tageszeiten
- **parallel BR0** über denselben Zeitraum, um zu sehen, ob Konkurrenten die Launches im selben Block gekauft haben, die wir nicht vorher gesehen haben

**Tests des Werkzeugs:** `npm i ws@8.21.3 && node --test test_wss_race.mjs` ergibt 6/6. `ws` braucht nur der Test-Server.

## 3. Was die Zusammenfassung zeigt

| Kennzahl | Frage, die sie beantwortet |
|---|---|
| **Von KEINER Quelle vorher gesehen** | Wie viele Launches sind mit kostenlosen Quellen gar nicht vorab erkennbar (vermutlich privat an Builder)? |
| **vorher gesehen x/y** je Quelle | Abdeckung dieser Quelle, nur Zeiten, in denen sie verbunden war |
| **Vorsprung p50/p90** | Wie viele ms vor der Block-Ankunft bei uns die Quelle den Launch hatte |
| **Rückstand zur schnellsten Quelle** | Was diese Quelle gegenüber der besten verliert |
| **Modus FULL/HASH** | FULL heißt sofort handlungsfähig. HASH heißt, die volle TX muss noch geholt werden, und das kostet Zeit und Ratenlimit |
| **Fehler/Ratenlimit, verbunden %** | Taugt die Quelle für Dauerbetrieb? |

**Grenzen:**
- „Vorsprung" bezieht sich auf die **Ankunft des Blocks bei uns**, nicht auf die Blockproduktion beim Validator.
- Die Werte gelten für deinen Standort und Anschluss.

## 4. Einordnung

- Die Roadmap verbietet öffentliches WSS als **alleiniges** primäres Produktionssignal (§20). Als **zusätzlicher paralleler Eingang** ist es erlaubt.
- Im Modulplan ist es SIG-4: Kostenklasse K0, zunächst im SCHATTEN.
- Ob SIG-4 LIVE gehen soll, entscheiden die Zahlen aus §3, zusammen mit BR0.
