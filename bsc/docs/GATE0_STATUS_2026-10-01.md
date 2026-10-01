# Gate 0 BSC — Status / Zusammenfassungsprotokoll

Stand: 2026-10-01  
Status: **BUY-HOTPATH QUALIFIZIERT / INGRESS NOCH NICHT QUALIFIZIERT**

Dieses Dokument ist die kurze operative Zusammenfassung zu
`GATE0_TESTPLAN_SHORT.md` und
`GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md`.

## 1. Was ist tatsächlich bewiesen?

### Direkter On-Chain-BUY funktioniert

DarkBot hat auf BSC Testnet kontrollierte Flap-Launches erkannt, den exakten
direkten Portal-BUY lokal gebaut/signiert/gesendet und den vorgesehenen Token
tatsächlich erhalten.

Mindestens zwei P2P-basierte kontrollierte Läufe belegten:
- Launch receipt status = 1;
- BUY receipt status = 1;
- positive zuordenbare Token-Akquisition;
- native wirtschaftliche Belastung;
- Launch und BUY im selben Block;
- unser BUY direkt nach dem Launch in der kanonischen Transaktionsreihenfolge;
- 0 erfolgreiche externe BUYs zwischen Launch und DarkBot.

Beispiel Raw-P2P-Lauf:
- Launch: `0x54d4876b3e7e2e3d1dfab84aaaee116efcc00e63664f49130569f64c2adbbb6c`
- BUY: `0xd3f07a6c9435463b0a725dcf6d0c3cee9309a76c5361d0c32074927927de8dc8`
- Block: `134255601`
- Launch index: `0`
- BUY index: `1`
- erfolgreiche externe BUYs dazwischen: `0`

Damit ist die wirtschaftliche Aussage für diesen kontrollierten Test stark:
**DarkBot erhielt die erste kanonisch erfolgreiche externe BUY-Position direkt
hinter dem Launch.**

## 2. Welche BUY-Zeit ist wirklich gemessen?

Es müssen drei unterschiedliche Größen getrennt werden.

### A. Lokale Reaktionszeit — gemessen

Vom lokal bereits vollständigen, verwertbaren Launch-Signal bis zur Übergabe
des fertigen BUY an den getesteten Transport:

- kontrollierter P2P-Lauf: `T_signal -> T_send = 433,125 ns = 0.433125 ms`
- Raw-P2P/Hash-Fetch-Lauf: `T_signal -> T_send = 535,250 ns = 0.535250 ms`

Damit ist der lokale Rust-BUY-Hotpath **unter 1 ms**.

Dieser Wert enthält je nach Lauf:
- Launch decode;
- Token-/CREATE2-Ableitung;
- BUY build;
- Signatur;
- finale Serialisierung/Queue;
- Übergabe an den getesteten Sendetransport.

### B. Wann wurde wirklich on-chain gekauft? — Reihenfolge bewiesen, Millisekunden unbekannt

`T_buy` ist der Zeitpunkt, an dem die BUY-Transaktion tatsächlich im
Validator/Blockbuilder ausgeführt wird und die Token-Akquisition stattfindet.

Standard-BSC-Blockdaten/Receipts enthalten **keinen vertrauenswürdigen
Sub-Millisekunden-Zeitstempel pro Transaktion innerhalb eines Blocks**.

Deshalb ist aktuell:

`T_buy - T0 = UNKNOWN_FROM_STANDARD_BSC_BLOCK_DATA`

Das ist keine fehlgeschlagene BUY-Ausführung. Die Ausführung ist kanonisch
bewiesen. Nicht verfügbar ist lediglich der genaue physische Zeitabstand in
Mikro-/Millisekunden zwischen dem ersten extern ausführbaren Punkt `T0` und
der EVM-Ausführung unseres BUY.

Die harte derzeit verfügbare On-Chain-Evidence ist:
- Launch index N;
- DarkBot BUY index N+1;
- gleicher Block;
- 0 erfolgreiche externe BUYs dazwischen;
- tatsächliche positive Token-Akquisition.

### C. Receipt-seen / ACK — KEINE BUY-Zeit

Beispiele wie:
- `T_signal -> ACK ~= 80-96 ms`;
- `T_signal -> receipts_seen ~= 1.32 s` oder in einem anderen Lauf deutlich länger;

sind **nicht** die tatsächliche BUY-Ausführungszeit.

ACK misst nur Transport-/RPC-Bestätigung.
Receipt-seen misst zusätzlich Blockerstellung, Netzwerkpropagation und
Polling/Beobachtung. Der BUY kann deutlich früher ausgeführt worden sein.

## 3. Was ist aktuell zu langsam?

Nicht der Rust-BUY-Pfad.

Der aufgeschlüsselte Raw-P2P-Lauf zeigte:
- Launch handoff -> erster P2P-Hash = `133.377959 ms`;
- P2P Hash -> vollständige TX = `101.263875 ms`;
- Launch handoff -> vollständige verwertbare P2P-TX = `234.641834 ms`;
- danach vollständige TX -> BUY send = `0.535250 ms`.

Damit liegt der dominante Engpass aktuell vor `T_signal`:
**frühestmögliche vollständige Launch-TX-Akquisition / Netzwerkposition.**

## 4. Aktueller P2P-Diagnosestand

Der Diagnoselauf vom 2026-10-01 bewies echte Chapel-Sessions:
- bis zu 3 gleichzeitig beobachtete P2P-Peers;
- Gegenstellen: BSC Geth `v1.7.6-69b3758c-20260708`;
- ausgehandeltes Protokoll: `eth/68`;
- Chain: `bsc-testnet`.

Damit sind RLPx/BSC-Handshake und eth/68-Kompatibilität grundsätzlich
funktional.

Der Lauf erzeugte jedoch keine neue Latenzmessung, weil der unabhängige
Launch-Injektor versehentlich TCP/TLS-Port 443 statt des BNB-Testnet-RPC-Ports
8545 verwendete und vor dem Launch mit TCP timeoutete. Das wurde in
`a11c6c723b899d295dd0db767033769d394b99f7` korrigiert.

Die vier in reth-bsc eingebetteten Chapel-Adressen sind Bootnodes und werden
nicht mehr als garantiert hochwertige Transaction-Feed-/Static-Peers
interpretiert.

## 5. Zieldefinition: technisches Maximum

Das Produktziel bleibt:

`T_buy - T0 < 1 ms`

Der stärkste technische Weg ist jedoch nicht, erst nach `T0` über das
öffentliche Internet zu reagieren.

Die Zielrichtung ist:

`T_pre < T0`

und möglichst:

1. Launch/Target vor Ausführung sehen;
2. BUY bereits vor `T0` vollständig vorbereitet/signiert haben;
3. Launch + BUY als geordnete Builder-/Backrun-Sequenz vorhalten;
4. Validator/Builder führt unmittelbar aus:
   `[Launch][DarkBot BUY]`.

Damit verschiebt sich die externe Netzwerkverzögerung vor `T0`. Das ist
technisch stärker als ein nachträglicher Sub-ms-Internet-Race.

## 6. Verbindliche nächste Reihenfolge

### Schritt 1 — korrigierten unabhängigen Testnet-Lauf abschließen

- unabhängige Launch-Injektion über korrekten BNB-Testnet-RPC;
- P2P mindestens 1 stabiler Peer, weitere Peers als Beobachtung;
- PublicNode WSS nur als unabhängiger Control-Sensor;
- messen:
  - earliest P2P hash;
  - direct full broadcast vs hash fetch;
  - hash -> full;
  - WSS vs P2P;
  - full actionable -> BUY send;
  - tatsächliche Acquisition/Ordering.

### Schritt 2 — G0.6 Ingress × Inclusion Matrix, nicht weiter nur P2P

Parallel qualifizieren:
- direkter BSC P2P / eigener Node als kostenlose Baseline;
- 48Club Auction Feed, insbesondere für Four.Meme, sofern der Feed die
  erforderlichen Target-/Launch-Fakten liefert;
- 48Club/Puissant `backrunTarget` / ordered bundle;
- BlockRazor Public Mempool / Streaming-Feed;
- BlockRazor Builder/Bundle;
- bloXroute BSC transaction stream / BDN, falls der kostenlose/andere Pfad
  nicht genügt;
- weitere verifizierte aktuelle Builder/Relays nur mit Messung.

Kein Anbieter wird aufgrund von Marketing als Gewinner angenommen.

### Schritt 3 — Zielpfad = pre-T0 ordered inclusion

Wo verfügbar, bevorzugt testen:

`early target feed -> DarkBot BUY -> ordered/backrun builder -> validator`

statt:

`late pending signal -> normal tx broadcast -> Gas-/Gossip-Race`

Für Four.Meme ist ein Feed/Builder-Pfad mit Target-Prepend besonders relevant,
weil er semantisch direkt `[Launch][BUY]` herstellen kann.

### Schritt 4 — Kostenentscheidung erst nach Messung

Bevorzugt ohne Abo. Falls ein bezahlter Feed/Relay/Builder den kostenlosen
Pfad jedoch deutlich schlägt und für das Ziel notwendig ist:

`G0.9 BUILDER_DEPENDENCY = BENEFICIAL oder REQUIRED`

Danach G0.10:
- eigene Builder-Alternative;
- direkte Sentry-/Validator-Nähe;
- eigener dauerhaft laufender BSC-Knoten mit hochwertigen Peers;
- Multi-Region-Sensoren.

### Schritt 5 — absolutes technisches Maximum

Falls erforderlich:
- dedicated Linux host;
- dauerhaft vorgewärmte direkte P2P-Verbindungen;
- geografische Nähe zu Buildern/Validator-/Sentry-Infrastruktur;
- eigener Builder;
- mehrere Ingress-Sensoren / FIRST ACTIONABLE WINS;
- Shared-memory/SPSC;
- direkter Rust devp2p nur wenn er gegenüber gepinntem bsc-geth messbar
  zusätzlichen Lead bringt;
- Kernel/NIC-Tuning erst nach belegtem Netzwerk-/Kernelbottleneck.

## 7. Aktueller Gate-Status

- G0.1–G0.3: Flap-Scope weitgehend funktional/semantisch belegt.
- G0.4: Coverage noch nicht abgeschlossen.
- G0.5: lokaler Hotpath für die kontrollierten Läufe <1 ms — PASS im gemessenen Scope.
- G0.6: PARTIAL — Ingress × Inclusion Matrix noch nicht vollständig geraced.
- G0.7 Testnet: erfolgreiche kontrollierte Käufe vorhanden; korrigierter neuer
  P2P-vs-Control-Lauf noch offen.
- G0.7 Mainnet: noch nicht autorisiert.
- G0.8: Testnet-Acquisition/Ordering bewiesen; Mainnet noch offen.
- G0.9/G0.10: noch offen; werden relevant, sobald Builder/Relay-Messungen vorliegen.

## 8. Entscheidender Satz

**Wir haben bereits wirklich gekauft. Der lokal kontrollierbare BUY-Pfad ist
unter 1 ms. Was wir noch nicht numerisch kennen, ist die echte
`T_buy - T0`-Zeit innerhalb des Blocks. Die nächste Optimierung gehört deshalb
nicht in Build/Sign, sondern in früheren Ingress und geordnete
Builder-/Validator-Inclusion.**
