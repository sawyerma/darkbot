# DarkBot Gate 0 — Masterprotokoll BSC Birth-to-BUY

Status: **AUTHORITATIVE / HUMAN-READABLE MASTER RECORD**  
Stand: **2026-09-28**  
Primärer Scope: **BSC / Flap / Portal v5.24.0 / newTokenV6 / TOKEN_TAXED_V3**

> Diese Datei ist die eine zentrale menschlich lesbare Quelle für den aktuellen Gate-0-Stand.
> Rohdaten, JSON-Traces und Prüfscripte dürfen in Evidence-Ordnern liegen, aber Messwerte, Entscheidungen,
> offene Punkte und Phasenstatus werden hier zusammengeführt. Ein späterer Leser soll den Projektstand
> verstehen können, ohne 20–30 Einzeldokumente durchsuchen zu müssen.

---

## 1. Ziel des Projekts

DarkBot soll einen neuen Coin **direkt on-chain** so früh wie technisch möglich kaufen.

Kein BUY über:
- Website oder Browser-UI;
- Frontend-API als Readiness-Gate;
- manuelles Klicken;
- Listing-Seite.

Der relevante Pfad ist:

```text
frühestes verwertbares Chain/Pending-Signal
→ notwendige BUY-Daten lokal ableiten
→ BUY-Transaktion bauen/patchen
→ signieren
→ an echten Netzwerktransport übergeben
→ erfolgreiche on-chain Ausführung
→ Token tatsächlich erhalten
```

Die zentrale lokale Performance-KPI ab G0.5 ist:

```text
T_signal → T_send
```

mit Ziel:

```text
< 1,0 ms
bevorzugt < 0,5 ms
```

Dabei bedeutet `T_send` ausdrücklich **echter Transport-Handoff** und nicht nur eine interne Queue.

Der wirtschaftlich entscheidende spätere Nachweis ist:

```text
BUY erfolgreich ausgeführt
+ erwarteter Token tatsächlich erhalten
+ Position/Ordering gegenüber konkurrierenden BUYs bekannt
```

---

## 2. Harte Timing-Definitionen

```text
T_signal = lokale monotone Zeit beim Empfang des tatsächlich verwendeten actionable Signals

T_pre    = T_signal nur dann, wenn unabhängig bewiesen ist,
           dass dieses Signal vor externer Kaufbarkeit vorlag

T0       = erster Ausführungspunkt, an dem ein unabhängiger externer BUY
           technisch gültig ausgeführt werden kann

T_ready  = exakte BUY-TX / Bundle vollständig gebaut und signiert

T_queue  = signierter Payload in lokale Adapter-/IPC-Queue geschrieben

T_send   = signierter Payload an die tatsächlich getestete Transportgrenze übergeben

T_buy    = BUY erfolgreich ausgeführt und Acquisition-Evidence beweist,
           dass der gewünschte Token beim gewünschten Empfänger angekommen ist
```

Wichtig:

- BSC-Blockzeiten sind **keine BUY-Latenz**.
- Ein Receipt liefert keine vertrauenswürdige Sub-ms-EVM-Ausführungszeit für eine einzelne TX.
- Solana/PAPER-Latenzen dürfen niemals als BSC-BUY-Latenz bezeichnet werden.
- Nicht gemessene Werte bleiben **N/A** und werden niemals zu 0 umgedeutet.

---

## 3. Phasenplan und aktueller Stand

| Gate | Zweck | Status |
|---|---|---|
| G0.1 | Verträge, Versionen, Selektoren und Acceptance-Basis einfrieren | weitgehend vorhanden / Evidence eingefroren |
| **G0.2** | Precomputability, exakte Flap-Economics und historischer First-BUY-Witness | **AKTUELL — PARTIAL** |
| G0.3 | Minimaler korrekter Direct-Portal Launch→BUY-Harness | **BLOCKIERT durch G0.2** |
| G0.4 | Pending/actionable Coverage und frühestes brauchbares Signal | parallel möglich, kein Gate-PASS |
| G0.5 | vollständige lokale BUY-Hotpath-Latenz bis echtem `T_send` | blockiert |
| G0.6 | Ingress × Inclusion Dry-Qualifikation + Concurrency | blockiert |
| G0.7 | kontrollierter Minimalwert-Mainnet-BUY | blockiert |
| G0.8 | tatsächliche Acquisition + Ordering | blockiert |
| G0.9+ | Builder-/Feasibility-Folgeschritte | blockiert |

**Es wird kein Gate übersprungen.**

---

## 4. Primärer historische Flap-Fixture

### Launch

```text
TX:
0x5b8cc9893c403a66d4f43bd73ed5c69227807661a58d1d6b5611def3f1d8352c

Block: 124415357
Index: 0
Portal: 0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0
Mode: newTokenV6
TokenVersion: TOKEN_TAXED_V3
Quote token: DOGE
```

Resultierender Token:

```text
0xabe1fe16cf6bc48c9524713054efe6e954b17777
```

### Frühester beobachteter externer BUY

```text
TX:
0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800

Block: 124415358
Index: 15
historischer Pfad: on-chain Router → Portal
tatsächlich erhalten:
6243568333969093909994622 Token-Basiseinheiten
```

### Direkter Portal-Referenz-BUY

```text
TX:
0x8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b

Block: 124415360
Index: 17
Selector: 0xef7ec2e7
swapExactInput((address,address,uint256,uint256,bytes))
Input: 0.15 BNB
tatsächlich erhalten:
28438305464314215025968609 Token-Basiseinheiten
```

Dieser direkte Portal-BUY war historisch der fünfte externe BUY. Das beweist **nicht**, dass DarkBot einen externen Router benutzen muss.

Produktziel bleibt:

```text
DarkBot baut den BUY selbst lokal
→ direkter on-chain Contract-Aufruf
```

---

## 5. Was bereits bewiesen ist

### 5.1 Tokenadresse vor Ausführung lokal berechenbar — PASS

Für den eingefrorenen Flap-V6-Scope reproduziert die korrekte EIP-1014/CREATE2-Berechnung exakt:

```text
0xabe1fe16cf6bc48c9524713054efe6e954b17777
```

Verifiziert durch:
- 7 EIP-1014-Testvektoren;
- unabhängigen RPC-Hash-Crosscheck;
- Übereinstimmung mit dem tatsächlich erzeugten Token.

Folge:

```text
Tokenadresse = SIGNAL_TIME_LOCAL_CALC
```

Kein Website-/Listing-Warten notwendig.

---

### 5.2 Archive-Zugriff — PASS

Öffentliche/pruned BSC-RPCs waren für den historischen Witness unzureichend:

```text
debug_traceTransaction → missing trie node
```

dRPC-Versuche endeten mit DNS-/HTTP-500-/HTTP-429-Fehlern.

Ein QuickNode BSC-Mainnet-Archive-Zugang wurde anschließend erfolgreich qualifiziert.

Ergebnisse:

| TX | callTracer | prestateTracer |
|---|---:|---:|
| First BUY | 71 Call-Frames | 22 Accounts / 67 Storage-Slots |
| Launch | 116 Call-Frames | 28 Accounts / 146 Storage-Slots |
| Direct BUY | 50 Call-Frames | 26 Accounts / 64 Storage-Slots |

Raw-Evidence lokal:

```text
/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/
gate0-evidence/2026-09-28-g02-archive-witness-quicknode-153754
```

Alle sechs Raw-Antworten wurden SHA256-geprüft.

---

### 5.3 Historischer First-BUY unabhängig exakt reproduziert — PASS als Rechenresultat

Aus dem **transaktionsgenauen historischen Prestate** wurde berechnet:

```text
expected:
6243568333969093909994622

actual:
6243568333969093909994622

difference:
0
```

Damit ist die Rechenstrecke erheblich stärker abgesichert als die frühere rein konditionale Rückrechnung.

Aber:

**Update 2026-09-28:** Die 65 Zwischen-Transaktionen sind jetzt anhand vollständiger committed State-Diffs geprüft. Der historische First-BUY-Zustand wurde aus dem Parent-Block `124415356`, dem Launch-Diff und den 65 geordneten TX-Diffs fortgeschrieben und mit dem unabhängigen First-BUY-Prestate verglichen. Eine gesondert belegte Balance-Anpassung am Blockübergang ist ausdrücklich ausgewiesen (Abschnitt 10).

Alle **22 Accounts / 67 gelesenen Storage-Slots** stimmen überein; **111 Zwischenzustands-Prüfungen** bestehen. Mit dem fortgeschriebenen Zustand liefert die BUY-Rechnung erneut `expected = actual = 6243568333969093909994622`, `difference = 0`.

Das ist ein unabhängiger **State-Diff-Replay mit exakter BUY-Arithmetik**. Es ist kein vollständig unabhängiger EVM-Replay aller externen Callees. G0.2 bleibt wegen der in Abschnitt 10 benannten Semantik-/Pending-Nachweise **PARTIAL**.

---

## 6. Historische Economics / bekannte Parameter

Für den eingefrorenen Fixture wurden unter anderem rekonstruiert:

```text
quoteAmt                      80574062404
buyTaxRate                    100 bps
sellTaxRate                   100 bps
reserve after launch          78962581156
circulating supply            21453245135239692933310825
r                             39956880715754427756773
h                             107036751000000000000000000
k                             44233735407663336217722200164523
DEX threshold                 800000000000000000000000000
```

Die frühere konditionale Kurvenrechnung reproduzierte Creator-Acquisition plus sechs externe BUYs exakt.

Für den First BUY:

```text
conditional expected = 6243568333969093909994622
actual               = 6243568333969093909994622
difference           = 0
```

Diese ältere konditionale Rechnung ist weiterhin nur Supporting Evidence. Maßgeblich ist jetzt der transaction-precise Archive-Witness.

---

## 7. Fee-/Tax-Beobachtungen

Protocol Fee und Tax erscheinen als getrennte DOGE-Abzüge.

Die einfache Annahme

```text
fee = floor(input / 100)
tax = floor(input / 100)
```

ist durch historische Daten widerlegt.

Beispiele:

| Quote Input | Protocol Fee | Tax |
|---:|---:|---:|
| 111646524369 | 1116465243 | 1116465244 |
| 120878323272 | 1208783232 | 1208783233 |
| 67309479679 | 673094796 | 673094797 |

Keine Rundungsregel darf aus diesen Werten nur „gefittet“ werden.

**Jetzt für den beobachteten Modus aus historischem Runtime-Bytecode belegt:**

```text
combinedFee = floor(q * (protocolBps + taxBps) / 10000)
protocolFee = floor(q * protocolBps / 10000)
tax         = combinedFee - protocolFee
```

Bei jeweils 100 bps: `tax = floor(q/50) - floor(q/100)`. Das ergibt genau dann `floor(q/100)+1`, wenn `q mod 100 >= 50`. Die Regel wurde anhand tatsächlich ausgeführter Bytecode-Operationen mit Programmzählern und Operanden geprüft, nicht an beobachtete Gebühren gefittet. First-BUY und Direct-BUY bestätigen die Rechnung.

Der Portal-Caller ist beim First BUY der Router; dessen Fee-Exemption-Mapping-Slot 177 ist 0. Die Decimals-, Buytax-, Pair- und Tax-Processor-Getter wurden zusätzlich aus historischem Code/Storage ohne übernommene Returnwerte nachgerechnet. Die Aussage gilt für diese Fixture und ersetzt keine vollständige Klassifikation anderer Caller, Extension-/Transfer-Effekte oder Modi.

Technische Belege: `fee_rounding_proof.json`, `independent_view_reads.json` und `first_buy_trade_arithmetic_audit.json` im Archive-Witness-Ordner aus Abschnitt 5.2.

---

## 8. Bisherige Performance-Messungen

### 8.1 Alte Solana/PAPER-Messung — nur Referenz

```text
287 Tests PASS
119 Python Syntaxchecks PASS
22.109 Frames
300 ENTRY intents
writer failures = 0
overflow = false

receive → decode p99       0.859333 ms
receive → decision p99     2.545375 ms
decision → enqueue p99     0.078375 ms
receive → enqueue p99      2.992500 ms
receive → enqueue max      3.559292 ms
queue callback lateness    0.756292 ms p99
```

Diese Zahlen sind **nicht BSC BUY-Latenz**.

---

### 8.2 BSC/Flap Offline-Warm-Microbenchmark — gemessen 2026-09-28

Gemessener Pfad:

```text
Launch-Calldata ABI decode
→ CREATE2-Tokenadresse
→ Extension-Preis decode
→ Conversion-/Quote-Arithmetik
→ r/h/k
→ Flap Curve BUY-Output
```

Testaufbau:

```text
3 Runden × 2.000 Messungen = 6.000 Samples
200 Warmups vor jeder Runde/Kategorie
historischer State bereits im RAM
single process
process.hrtime.bigint()
```

Ergebnis des vollständigen lokalen Decode/Create2/Quote-Pfads:

| Kennzahl | Ergebnis |
|---|---:|
| **p50** | **0.252 ms** |
| **p99** | **0.405 ms** |
| **max** | **2.229 ms** |

Korrektheitscheck:

```text
berechneter Output =
6243568333969093909994622

historischer Output =
6243568333969093909994622
```

Nicht enthalten:

- Netzwerkempfang;
- Mempool-Erkennung;
- State Refresh/Invalidation;
- Strategy Filter;
- BUY-TX Build/Patch;
- Nonce-Finalisierung;
- Signieren;
- finale TX-Serialisierung;
- Socket-/Transport-Handoff;
- RPC ACK;
- Inclusion;
- on-chain Execution.

**Interpretation:** Der reine lokale Signal-/Quote-Anteil ist für dieses Fixture warm deutlich unter 1 ms. Das ist positiv für das Ziel, aber noch kein vollständiger BUY-Latenznachweis.

Raw-Result:

```text
/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/
gate0-evidence/2026-09-28-g02-latency-offline/latency_result.json
```

---

## 9. Was ausdrücklich noch NICHT gemessen wurde

Aktuell authoritative:

```text
BSC T_signal → T_send          N/A
BSC T_signal → erfolgreicher BUY
                               N/A
exaktes T_buy - T0 in ms       N/A
```

Der beobachtete Blockabstand von ungefähr 450 ms zwischen Launch-Block und anschließendem BUY-Block ist **keine BUY-Latenz** und darf nicht als solche verwendet werden.

---

## 10. Geprüfte Zwischen-Transaktionen und verbleibende G0.2-UNKNOWNs

### 10.1 Vollständiger State-Diff-Nachweis — abgeschlossen

Geprüfter Abschnitt:

```text
Launch:    124415357:0
Zwischen:  124415357:1–50 und 124415358:0–14 (65 TXs)
First BUY: 124415358:15
```

Testaufbau: Mac / Darwin arm64, Python 3.14.3 und Node v24.1.0 / ethers 6.15.0. Read-only QuickNode-Archive-Abfragen und Offline-Verifier. Kein Live-BUY, keine Signatur, keine G0.3-Arbeit.

Erfasst wurden **207 Raw-RPC-Antworten**: Chain-ID, kanonische Blockheader/Rechecks, Launch plus 65 `prestateTracer(diffMode=true)`-Antworten, historischer Parent-Code/-Balance/-Nonce/-Storage für die First-BUY-Abhängigkeiten sowie ein zusätzlicher Balance-Beleg am Blockübergang. Historische Zustandsabfragen sind per Blockhash gebunden. Alle Raw-Antworten besitzen SHA256-Nachweise; Zugangsdaten sind nicht enthalten.

Jede der 65 TXs besitzt im bestehenden technischen Beleg `intervening_transactions.json` Hash, Block/Index, betroffene Accounts, konkrete Storage-Slots bzw. Accountfelder, Vorher-/Nachherwerte, Relevanzentscheidung und Einfluss auf die Quote. Es gibt **keine unklassifizierte Zwischen-TX**. Die Relevanzentscheidung bezieht sich ausdrücklich auf den tatsächlich ausgeführten historischen First-BUY-Pfad.

| Prüfumfang | Ergebnis |
|---|---|
| 65 Zwischen-TXs | vollständig geprüft |
| Änderungen an gelesenen First-BUY-Storage-Slots / relevantem Runtime-Code | **0** |
| Portal-Storage-Änderungen | 2, beide für einen anderen Token |
| WBNB-Änderungen | 14 andere Storage-Slots und 8 Native-Balance-Änderungen; einzeln dokumentiert |
| Zieltoken-/Curve-/Preis-State | keine beeinflussende Zwischen-Mutation |
| WBNB/DOGE-Poolreserven für diese Quote | keine Zwischen-Mutation |
| Gelesene Extension-/Fee-/Exemption-/Proxy-/Router-Abhängigkeiten | keine beeinflussende Zwischen-Mutation |
| Abgleich nach Fortschreibung | **22 Accounts / 67 Storage-Slots stimmen überein** |
| Laufende Vorzustandsvergleiche | **111 bestanden** |
| Erneute First-BUY-Rechnung | **Differenz 0** |

Die beiden Portal-TXs `124415357:12` (`0xe82b9496286c436cc6a1b46889071b5b4c6bff93f8055a3530823fd680b39980`) und `124415357:17` (`0x0b85b5a3fc78f03a9d181e1fb8b1c7010b872b9a91943c2b51e8fe7102393933`) verändern Reserve/Supply des anderen Tokens `0x76668c2ebe22185b82b73cb4e77707845ba67777`. Dessen Mapping-Slot wurde gegen die bestehenden Receipt-Logs und `keccak256(token,152)+1` geprüft. Sie verändern nicht die Kurve unseres Ziel-Tokens.

**Explizite Blockgrenze:** Das Konto `0xfffffffffffffffffffffffffffffffffffffffe` hat nach dem letzten betroffenen Launch-Block-TX-Diff die Balance `0x5d9c5f336ddc3`; der per Blockhash abgefragte kanonische Block-Endzustand beträgt 0. Diese außerhalb der erfassten TX-Diffs liegende Balance-Anpassung wird separat eingespielt und dokumentiert. Ihre Protokollimplementierung wurde nicht unabhängig replayt. Sie ist kein Curve-/Pool-/Quote-Input; sie wurde nicht stillschweigend ignoriert oder als TX-Mutation ausgegeben.

Die Fortschreibung verwendet **keinen beobachteten Token-Output als Eingabe**. Das beobachtete First-BUY-Prestate liefert die unabhängige Vergleichsbasis und den Scope der gelesenen Werte; die verwendeten Zustandswerte werden aus Parent-State und geordneten Änderungen rekonstruiert.

```text
replayed_state_arithmetic_expected_output = 6243568333969093909994622
actual                                   = 6243568333969093909994622
replayed_state_arithmetic_difference      = 0
```

Raw- und technische Belege im bestehenden Ordner:

```text
/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2/gate0-evidence/
2026-09-28-g02-archive-witness-quicknode-153754/
  intervening_raw/                    # requests, unveränderte responses, SHA256
  intervening_transactions.json      # alle 65 Klassifikationen, Vor-/Nachwerte
  replayed_first_buy_prestate.json    # aus Parent + Diffs fortgeschrieben
  first_buy_calculation.json          # Rechnung mit diesem Zustand
  witness_summary.json
  verifier.stdout.txt
```

Ausgeführt:

```bash
python3 gate0-evidence/collect_flap_intervening.py
python3 gate0-evidence/verify_flap_intervening.py
# Exit 0: 65 TXs klassifiziert, Zustand stimmt überein

python3 gate0-evidence/verify_flap_g02_economics.py
# Exit 2: G0.2_FLAP_PARTIAL; kein künstlicher PASS
```

Es wurde keine neue Latenz gemessen. Die Zahlen in Abschnitt 8 bleiben unverändert; `T_signal → T_send` und `T_buy` bleiben N/A.

### 10.2 Verbleibende UNKNOWNs — weiterhin PARTIAL

Die frühere Formulierung „einziger Restblock = 65 TXs“ war gegenüber den bereits dokumentierten Verifier-Requirements unvollständig. Der jetzt abgeschlossene TX-Nachweis beseitigt diese anderen Anforderungen nicht:

1. **Pending-Initialisierung / Oracle / Extension:** Der benötigte Preis ist in den Launch-Calldata vorhanden und stimmt mit dem gespeicherten Preis überein. Vollständige unabhängige Herleitung des signierten Digests, Rollen-/Frischeprüfung und Launcher-/Extension-Effekte fehlen noch. Die erfolgreiche historische Ausführung allein belegt nicht alle Vorbedingungen für die lokale Vorausberechnung.
2. **Externe Callee-/Transfer-Effekte:** Die Arithmetik und die genannten View-Getter sind unabhängig berechnet. Der begrenzte Trade-Bytecode-Auditor übernimmt für externe Effekte weiterhin aufgezeichnete erfolgreiche Antworten. Er ist kein unabhängiger Ausführungsnachweis sämtlicher Nebenwirkungen. Der First-BUY-Audit verwendet jetzt den belegten Blocktimestamp und erreicht den Return; ein künstlicher Timestamp wird nicht eingesetzt.
3. **Vollständige Hotpath-Klassifikation:** Aktualisierung, Invalidierung und rechtzeitige Verfügbarkeit aller Quote-/Converter-/Registry-/Extension-/Exemption-Werte sind noch nicht geschlossen. Für den direkten BNB-Portal-Referenzpfad fehlt zusätzlich die unabhängig gerechnete V3-Konvertierung; sie darf nicht mit der bewiesenen Router-V2-Konvertierung gleichgesetzt werden.

Aktuelle Requirements:

| Requirement | Wert |
|---|---|
| router_command_semantics | true für die historische Fixture |
| independent_conversion_prestate | true für den First-BUY-V2-Pfad |
| fee_rounding_source | true für den belegten Modus |
| alle 65 Zwischen-TXs geprüft | true |
| pending_initial_state | false |
| full_first_buy_reproduction als vollständiger semantischer Ausführungsnachweis | false |
| hotpath_dependencies_fully_classified | false |

`full_independent_first_buy_difference` bleibt im strengen Gesamt-Witness `null`; die exakt bestandene Rechenprüfung ist separat als `replayed_state_arithmetic_difference = 0` ausgewiesen. Damit wird der historische Rechenerfolg nicht als vollständige G0.2-Freigabe ausgegeben.

**Gate-Entscheidung: G0.2 = PARTIAL, G0.3 = BLOCKED.** Nächster fachlicher Nachweis ist die konkrete Launcher-/Oracle-/Extension-Semantik und ihre pending-seitige Abhängigkeitsklassifikation, nicht eine erneute Prüfung derselben 65 TXs.

---

## 11. Exakte Exit-Kriterien für G0.2

G0.2 darf erst auf PASS gesetzt werden, wenn:

- CREATE2/token address = proven;
- Router→Portal tatsächlicher Call/Selector = proven;
- transaction-precise initial state = proven;
- conversion prestate = independent/proven;
- fee/tax semantics für den Scope ausreichend bound;
- relevante Extension-/Exemption-Effekte = classified;
- alle 65 Zwischen-TXs = reviewed;
- First-BUY nach Einbeziehung aller relevanten Mutationen weiterhin unabhängig exakt reproduziert;
- jeder für den BUY notwendige Wert klassifiziert ist als:
  - startup precomputed,
  - pre-resident,
  - aus Launch-Signal lokal ableitbar,
  - oder signal-time residual calculation;
- kein notwendiger synchroner RPC/HTTP/DB/filesystem/DNS-Read im späteren BUY-Hotpath verbleibt;
- G0.2-Verifier aus Evidence PASS ergibt und nicht durch manuell gesetzte Flags.

Bis dahin:

```text
G0.2 = PARTIAL
G0.3 = BLOCKED
```

---

## 12. Was unmittelbar danach passiert

Sobald G0.2 PASS ist:

### G0.3 — Correctness-Harness

Minimalen Direct-Portal-BUY-Harness bauen:

```text
Launch facts
→ exakte Tokenadresse
→ exakte Quote
→ exakte Direct-Portal-BUY-TX
→ korrekte Signature-Binding-Felder
→ erwartete Acquisition-Semantik
```

Noch keine Performance-Schönheitsarbeit.

### G0.4 — Actionable Signal

Beweisen, welche Pending-/Ingress-Quelle das früheste vollständige Signal liefert und mit welcher Coverage.

### G0.5 — vollständige lokale BUY-Latenz

Dann wird zum ersten Mal der gesamte lokale BSC-BUY-Hotpath gemessen:

```text
T_signal
→ Decode/Derive
→ Quote
→ TX Build/Patch
→ Nonce/Gas finalize
→ Sign
→ Final Serialize
→ echter Transport-Handoff T_send
```

Pflichtmesswerte:

```text
p50
p95
p99
max
Sample count
Stage timings
machine/runtime/config
raw evidence hashes
```

Ziel:

```text
T_signal → T_send
p99 < 1 ms
bevorzugt < 0.5 ms
```

### G0.6

Ingress × Inclusion trocken qualifizieren und Concurrency/Nonce-Races ausschließen.

### G0.7

Erst danach **kontrollierter echter Mainnet-BUY mit minimalem autorisierten Wert**.

### G0.8

Für diesen BUY beweisen:

- Receipt success;
- richtige Tokenadresse;
- richtige Recipient-Adresse;
- Tokenmenge > 0;
- Balance/Transfer Acquisition;
- tatsächliche Kosten;
- Block + TX Index;
- erfolgreiche konkurrierende BUYs zwischen Launch und uns.

---

## 13. Dokumentationsregel ab jetzt

Diese Datei ist der zentrale menschlich lesbare Gate-0-Stand.

Andere Dokumente und Evidence-Verzeichnisse haben nur noch zwei Rollen:

1. normative technische Detailquelle;
2. Rohdaten-/Maschinennachweis.

**Ein neuer Messwert oder eine Gate-Entscheidung gilt erst als dokumentiert, wenn sie hier eingetragen oder von hier eindeutig auf eine unveränderliche Evidence referenziert wird.**

Pro neuer Messung muss mindestens festgehalten werden:

```text
Was wurde getestet?
Welche Phase?
Welche Maschine/Runtime?
Wie viele Samples/Versuche?
Was war enthalten?
Was war ausgeschlossen?
Wo liegen die Rohdaten?
Was ist das Ergebnis?
Was beweist es?
Was beweist es ausdrücklich nicht?
Welche Gate-Entscheidung ändert sich dadurch?
```

Damit bleibt die Phasenlogik später vollständig nachvollziehbar.
