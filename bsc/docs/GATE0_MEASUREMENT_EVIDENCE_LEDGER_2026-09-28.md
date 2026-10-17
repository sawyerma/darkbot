# DarkBot Gate 0 — Masterprotokoll BSC Birth-to-BUY

> **EXECUTION ORDER LOCK (2026-10-10): G0.7/G0.8 NOW. C1/C2 ARE PAUSED.**  
> **Do not resume C1.10 freeze, C2, C3 or any synthetic optimization before the controlled value-bearing Mainnet BUY has actually executed and G0.8 has verified token acquisition, ordering and costs.**

> **ABSOLUTE TOP PRIORITY / STOP RULE:** Der nächste und einzige relevante Gate-0-Nachweis ist ein **wirklich erfolgreich ausgeführter on-chain BUY mit tatsächlicher Token-Akquisition** und **gemessenem/belegtem `T0 → T_buy < 1 ms`**.  
> **Bis dieser Nachweis PASS ist, wird nichts anderes getestet, optimiert oder gebaut.** Erlaubt sind ausschließlich Code-, Mess-, Transport-/Inclusion- und Evidence-Arbeiten, die direkt notwendig sind, um diesen realen BUY auszuführen, `T0 → T_buy` zu messen/belegen oder unter 1 ms zu bringen.  
> `T_signal → T_send`, reine Build-/Sign-Latenz, Transport-Handoff, Builder-ACK, Receipt oder Ordering allein zählen **nicht** als Erfolg. Ohne belegten realen BUY <1 ms bleibt Gate 0 **BLOCKED/FAIL**.

> **PRIMÄRES PROJEKTZIEL: `T0 → T_buy` = vom frühestmöglichen technisch gültigen BUY bis zur erfolgreichen on-chain BUY-Ausführung / Token-Akquisition. `T_send` ist nur ein Zwischenmesspunkt und niemals der Erfolgsnachweis für „gekauft“.**

Status: **AUTHORITATIVE / HUMAN-READABLE MASTER RECORD**  
Stand: **2026-10-16** (G0.7-Scope-Abgleich G0.1–G0.11 als Ergänzung am Ende; historische Abschnitte unverändert)
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

Die **primäre Projekt-KPI** ist:

```text
T0 → T_buy
```

also: ab dem frühesten Zeitpunkt, zu dem ein unabhängiger externer BUY technisch gültig ist, bis zur **erfolgreichen on-chain Ausführung und tatsächlichen Token-Akquisition**.

`T_signal → T_send` ist nur eine **diagnostische Teil-KPI** des kontrollierbaren Bot-/Transportpfads. Das lokale Ziel bleibt:

```text
T_signal → T_send < 1,0 ms
bevorzugt < 0,5 ms
```

Ein `T_send`-PASS beweist ausdrücklich **nicht**, dass der Coin bereits gekauft wurde. Für „wirklich gekauft“ zählen zusätzlich Transport/Ankunft, Ordering/Validator-Selektion und erfolgreiche on-chain Ausführung bis `T_buy`.

Der wirtschaftlich entscheidende Nachweis ist:

```text
T0 → T_buy gemessen
+ BUY erfolgreich ausgeführt
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

- BSC-Blockzeiten sind **kein Bestandteil der lokalen `T_signal → T_send`-Latenz**. Für die tatsächliche Wall-Clock-Zeit bis `T_buy` ist die Inclusion/Block-Ausführung dagegen selbstverständlich relevant.
- Ein Receipt liefert keine vertrauenswürdige Sub-ms-EVM-Ausführungszeit für eine einzelne TX.
- Solana/PAPER-Latenzen dürfen niemals als BSC-BUY-Latenz bezeichnet werden.
- Nicht gemessene Werte bleiben **N/A** und werden niemals zu 0 umgedeutet.

---

## 3. Phasenplan und aktueller Stand

| Gate | Zweck | Status |
|---|---|---|
| G0.1 | Verträge, Versionen, Selektoren und Acceptance-Basis einfrieren | weitgehend vorhanden / Evidence eingefroren |
| **G0.2** | Precomputability, exakte Flap-Economics und historischer First-BUY-Witness | **PASS für den eingefrorenen historischen Modus** |
| **G0.3** | Minimaler korrekter Direct-Portal Launch→BUY-Harness | **PASS für den eingefrorenen Offline-Scope — unmittelbarer Launch→Direct-BUY gültig; siehe Abschluss 2026-09-29** |
| G0.4 | Pending/actionable Coverage und frühestes brauchbares Signal | **PARTIAL — QuickNode-Quellenprobe; noch kein Coverage-PASS** |
| **G0.5** | vollständige vorbereitete Self-Launch-Hotpath-Latenz bis echtem `T_send` | **PASS — 48Club p99 0.584 ms, BlockRazor p99 0.527 ms, jeweils 0/100 >1 ms** |
| **G0.6** | Ingress × Inclusion Dry-Qualifikation + Concurrency | **G0.6a-Final-DRY PASS für den G0.7-Minimal-Kandidaten (2026-10-15, DRY-1..4; zuvor 2026-10-11 PASS im 1-gwei-Profil); Builder-Semantik (Pre-Arm 6/10) UNQUALIFIED bis zur Probe; Ingress×Inclusion/Concurrency für externe Launches weiter PARTIAL** — siehe Abgleich 2026-10-16 |
| **G0.7** | kontrollierter Minimalwert-Mainnet-BUY | **NEXT / ONLY PRODUCT WORK — nicht ausgeführt, nicht autorisiert; DRY-PASS, FUNDING=PASS und Härtung vorhanden; offen: Builder-Probe, finale v2-Freigabe, Scharfschalten** — siehe Abgleich 2026-10-16 |
| **G0.8** | tatsächliche Acquisition + Ordering | **IMMEDIATELY AFTER G0.7 — Token-Balance-Delta, Receipt, Blockposition/Ordering, Kosten und externe BUYs auswerten** |
| G0.9+ | Builder-/Feasibility-Folgeschritte | blockiert |
| **C1** | Production bsc-geth Fast Lane / Wire V2 / Rust HOT0 / local-geth handoff | **PAUSED BEFORE FREEZE — C1.1–C1.8 PASS, C1.9/C1.9b qualifiziert (Stand vor der Umstrukturierung); C1.10A-Checkpoint 2026-10-10: Regression PASS mit Pinned-Baseline-Race-Ausnahme, aber `C1_10A_COMPLETE = NO` (6 Schritte offen); C1.10B ohne Abnahmeabschnitt; nach R1–R13 und C1-Reintegration (2026-10-16) C1.9/C1.9b-Bindung und C1.10-Freeze für den neuen Quellstand offen; DO NOT continue before G0.7/G0.8** — siehe Abgleich 2026-10-16 |

**Es wird kein Gate übersprungen.**

---


## C1.9b Testnet hook/path checkpoint — 2026-10-10

This checkpoint records the locally integrated C1.9b qualification result. It is a **controlled Testnet hook/path result only** and does not change the Gate-0 acquisition requirement.

### Root cause closed

The fresh Chapel node was still initial-syncing. In pinned bsc-geth v1.7.8, the ETH protocol handler checks `backend.AcceptTxs()` before decoding/forwarding a `Transactions` packet to the backend handler. With `acceptTxs=false`, both the public Chapel launch and an authenticated controlled ETH/68 replay were dropped before the C1 ingress hook, explaining the earlier zero-row captures.

The qualification-only pre-sync override is now hardened and fail-closed. It requires all of:

```text
DARKBOT_FASTLANE_V2_SCOPE=c19b-testnet
DARKBOT_C19B_ACCEPT_TXS_UNSYNCED=YES
networkID=97
```

Unset arm leaves normal behavior unchanged. Invalid arm, wrong scope, or wrong network ID fails closed. The earlier scope-string-only bypass is absent from the hardened generated/vendor source.

### Reproducibility and regression evidence

Pinned base:

```text
bnb-chain/bsc v1.7.8
cdb7548b5baacfdae92f9f63437d6456411665f3
```

Verified locally:

```text
fresh-pin overlay generation                  PASS
C1.9b qualification-gate targeted tests       PASS
affected geth tests                           PASS
geth build                                    PASS
root diff check                               PASS
vendor diff check                             PASS
verified candidate -> vendor byte match       PASS for handler.go / ingress.go / ingress_test.go
Project 0 touched by vendor integration        NO
```

The controlled hardened runtime replay then produced:

```text
chain                                         97 / 0x61
ETH protocol                                  68
RLPx handshake                                PASS
controlled launch observations                1
capture rows                                  1
invalid_records                               0
lane_faults                                   0
launches                                      1
observed                                      1
missed                                        0
analyzer                                      PASS
C1_9B_HARDENED_RUNTIME_PATH                   PASS
```

The handoff timestamp was captured immediately before the controlled local ETH/68 RLPx `Transactions` write and the observation was captured from the real C1 Fast-Lane arena by the Rust C1.9b consumer.

### Exact interpretation

Accepted statement:

```text
C1.9b CONTROLLED TESTNET ETH/68 HOOK/PATH = PASS
```

Not established by this result:

```text
MAINNET_INGRESS_COVERAGE = NOT_PROVEN
T0_TO_T_BUY_LT_1MS      = NOT_PROVEN
PROVIDER_ORDERING        = UNQUALIFIED unless separately qualified
LIVE                     = BLOCKED
```

The earlier real public Chapel launch remains useful evidence that the chain launch occurred, but its zero Fast-Lane capture occurred while the fresh node's normal `AcceptTxs` initial-sync gate was closed. The controlled qualification override exists only to exercise the real ETH/68 -> geth backend hook -> Fast-Lane -> Rust path without waiting for historical sync.

### Remaining C1 work

C1.9 production-devp2p measurement remains COMPLETE / integrity PASS / eligible with the previously recorded native latency distribution. The final C1 source/build/config freeze binding is **not yet closed** because the complete C1.5-C1.9 regression must be rerun against the final hardened integrated source before C1.10.

Next:

```text
1. run complete final C1.5-C1.9 / REV2 regression on hardened integrated source
2. bind final source/build/config/evidence identities
3. run C1.10 fail-closed freeze
4. only after C1.10 PASS/FROZEN proceed to C2
```



## C1.10A pre-freeze regression checkpoint — 2026-10-10

C1.10 has started. This checkpoint records the state **before** the final C1.10 freeze and deliberately does not mark C1 frozen.

Completed on the hardened integrated source:

```text
Python qualification tooling compile/tests   PASS
Rust fmt                                     PASS
Rust clippy -D warnings                      PASS
Rust debug workspace tests                   PASS
Rust release workspace tests                 PASS
C14 Go/Rust interop                          PASS
targeted geth tests                          PASS
DarkBot fastlane -race                       PASS
overlay/vendor ingress byte identity         PASS
```

The initial C14 interop stop was traced to a single extra blank line in the prepared overlay `ingress.go`; there was no semantic code difference. The overlay was synchronized to the already Fresh-Pin-generated, built and runtime-tested vendor source and C14 then passed.

### Pinned upstream race baseline

The broad Go race suites are not race-clean at the accepted bsc-geth pin `cdb7548b5baacfdae92f9f63437d6456411665f3`.

For `./eth/protocols/eth`, current and clean-pin runs both failed with the same `core/state/database.go:280 / CachingDB.NoTries` race family and the same affected protocol tests; DarkBot `./darkbot/fastlane -race` passed.

For `./eth`, the differential run produced:

```text
current WARNING: DATA RACE count            55
clean pinned WARNING: DATA RACE count       51
current package RC                           1
clean pinned package RC                      1
common core signatures                       core/blockchain.go, core/state/, core/rawdb/, triedb/
classification                               PINNED_BASELINE_PRESENT
```

There is broad overlap in failed upstream tests, including the block-broadcast, blob-sync, vote/transaction propagation and timeout cases. Counts are **not identical** (55 current vs 51 clean pin), so this checkpoint does **not** claim byte-for-byte or warning-for-warning race equivalence and does not claim the entire pinned upstream tree is race-free. It records only that the pinned upstream race family is independently present and that the DarkBot Fast-Lane package itself passed its race test.

Therefore the correct C1.10A interpretation is:

```text
DARKBOT_FASTLANE_RACE              = PASS
ETHPROTO_RACE                      = PINNED_BASELINE_EXCEPTION
ETH_PACKAGE_RACE                   = PINNED_BASELINE_PRESENT
FULL_PINNED_GETH_RACE_CLEAN        = NOT CLAIMED
C1_10A_COMPLETE                    = NO
```

Still required before C1.10A can close:

```text
1. full non-race go test ./...
2. final geth build from the exact hardened integrated source
3. final source/overlay/vendor consistency checks
4. bind final binary/source/config/evidence identities
5. rerun/attach final C1.9 measurement to the frozen binary as required
6. execute fail-closed C1.10 freeze
```

Required non-claims remain unchanged:

```text
T0_TO_T_BUY_LT_1MS = NOT_PROVEN
MAINNET_INGRESS_COVERAGE = NOT_PROVEN
PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED unless separately qualified
LIVE = BLOCKED
```

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

### 5.3 Historischer First-BUY unabhängig exakt reproduziert — semantischer PASS

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

**Update 2026-09-28:** Die 65 Zwischen-Transaktionen sind jetzt anhand vollständiger committed State-Diffs geprüft. Der historische First-BUY-Zustand wurde aus dem Parent-Block `124415356`, dem Launch-Diff und den 65 geordneten TX-Diffs fortgeschrieben und mit dem unabhängigen First-BUY-Prestate verglichen. Eine gesondert belegte Balance-Anpassung am Blockübergang ist ausdrücklich ausgewiesen (Abschnitt 10).

Alle **22 Accounts / 67 gelesenen Storage-Slots** stimmen überein; **111 Zwischenzustands-Prüfungen** bestehen. Mit dem fortgeschriebenen Zustand liefert die BUY-Rechnung erneut `expected = actual = 6243568333969093909994622`, `difference = 0`.

**Zusätzlich abgeschlossen:** Launch, First BUY und separater Direct BUY wurden vollständig als lokale EVM-Message-Ausführungen aus historischem Code, Prestate und Calldata replayt. Keine aufgezeichneten Callee-Returnwerte oder beobachteten Token-Outputs dienen dabei als Ausführungseingaben. Alle **237 Callframes / 100 Receipt-Logs** sowie beide BUY-Empfänger-Balancezuwächse stimmen exakt. Der Launch stimmt zusätzlich in **106 geänderten Storage-Werten** mit dem separaten State-Diff überein. Details und Grenzen stehen in Abschnitt 10.2.

Damit ist `full_independent_first_buy_difference = 0` jetzt auch als vollständiger semantischer Nachweis dieser historischen Message-Ausführung belegt. Die 65 Zwischen-TXs bleiben ein überprüfter Provider-State-Diff-Replay, keine unabhängige Ausführung sämtlicher BSC-Blocktransaktionen. Gas-Settlement und BSC-Konsens sind ausdrücklich nicht Teil dieses G0.2-PASS.

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

## 9. Aktueller authoritative Timing-Stand

Für den vorbereiteten kontrollierten Flap-Self-Launch ist `T_signal → T_send` inzwischen gemessen und G0.5-bestanden:

```text
48Club:
  p50 = 0.2415 ms
  p95 = 0.3371 ms
  p99 = 0.5843 ms
  max = 0.6026 ms
  >1 ms = 0/100

BlockRazor:
  p50 = 0.2285 ms
  p95 = 0.3763 ms
  p99 = 0.5266 ms
  max = 0.5570 ms
  >1 ms = 0/100

BSC T_signal → erfolgreicher BUY   N/A
exaktes T_buy - T0 in ms           N/A
```

Diese `T_signal → T_send`-Messungen sind **Hotpath-/Transport-Evidence**, keine Kauf- oder Inclusion-Evidence. `T0 → T_buy` bleibt der entscheidende offene Punkt und darf aus gewöhnlichen Blocktimestamps nicht als Sub-ms-Zahl abgeleitet werden.

Der beobachtete Blockabstand historischer Launch-/BUY-Blöcke ist **keine BUY-Latenz** und darf nicht als solche verwendet werden.

---

## 10. G0.2-Nachweise, Abschluss und Scope-Grenzen

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
# damaliger Exit 2 nach ausschließlich dem Zwischen-TX-Nachweis;
# abschließender Exit 0 nach dem Semantik-Nachweis unten
```

Es wurde keine neue Latenz gemessen. Die Zahlen in Abschnitt 8 bleiben unverändert; `T_signal → T_send` und `T_buy` bleiben N/A.

### 10.2 Die drei verbleibenden Requirements — geschlossen

**Ergebnis: `G0.2_FLAP_PASS`, Gesamtverifier Exit 0.** Scope: die eingefrorenen historischen Runtime-Codehashes, `newTokenV6`, `TOKEN_TAXED_V3`, DOGE als Quote-/Dividend-Token, belegter Extension-Modus 0. Der First BUY ist der beobachtete Router-V2-Pfad; der separate direkte Portal-BUY verifiziert den Native→DOGE-V3-Pfad. Dies qualifiziert keine anderen Flap-Modi oder ungeprüften Wallet-/Parameter-/Poolpfade.

| Requirement | Evidenzbasierter Wert |
|---|---|
| `pending_initial_state` | **true** im beschriebenen Modus, mit vollständig vorgehaltenem State und Gültigkeitsprüfungen |
| `full_first_buy_reproduction` | **true** — komplette Message-Semantik, Callees, Logs, Empfänger-Balance |
| `hotpath_dependencies_fully_classified` | **true** — Herkunft, Invalidierung und SKIP-Regeln für den belegten Read-Set |
| übrige Requirements einschließlich CREATE2, Selector, Router, Conversion und Fee-Rundung | **true** |
| alle 65 Zwischen-TXs geprüft | **true** |
| verbleibende UNKNOWNs innerhalb dieses eingefrorenen G0.2-Scopes | **keine** |

**Ausführungsnachweis:** Offline auf Mac / Darwin arm64, Node v24.1.0 und ethers 6.15.0. Neu wird `@ethereumjs/evm` 10.1.3 verwendet, mit `common`, `statemanager`, `util`, `block` jeweils 10.1.3. Profil: Chain-ID 56 und Cancun-Opcode-Semantik, tatsächliche historische Blockheader. Das ist eine vollständige Message-Ausführung, kein BSC-Consensus-/Block-Validierungsnachweis. Keine neuen RPC-Abfragen waren für diesen abschließenden Lauf nötig.

| Ausführung | Callframes exakt | Receipt-Logs exakt | tatsächlicher Token-Zugang aus lokalem `balanceOf` vor/nach |
|---|---:|---:|---:|
| Launch | 116 / 116 | 48 / 48 | Initialisierung separat geprüft |
| First BUY | 71 / 71 | 31 / 31 | 6243568333969093909994622 |
| Direct Portal BUY | 50 / 50 | 21 / 21 | 28438305464314215025968609 |

Alle drei Ausführungen kommen ohne fehlende Accounts oder Storage-Werte aus. Fehlende Werte werden nicht stillschweigend als 0 eingesetzt. Null-Storage ist nur für nachweislich neu erzeugte Contracts zulässig. Der First BUY benutzt den aus Parent-State + Launch + 65 TX-Diffs fortgeschriebenen Prestate. Beobachtete Calls und Receipt-Logs werden erst mit den berechneten Ergebnissen verglichen. Native-Transfers, Token-Transfers, Converter/V2/V3-Aufrufe, Callback-Prüfungen, Gebühren, Tax-Processor, Dividend-/Extension-Effekte laufen über den historischen Runtime-Code.

**Pending-Initialisierung:** Aus PRE-Launch-Code/-Storage und den Launch-Calldata entstehen Tokenadresse, Clone-Code, Oracle-Preis, Curve-/Tax-/Extension-State und Creator-BUY unabhängig. Gegen den separaten Launch-Diff bestehen **106 Storage-, 5 Runtime-Code- und 9 Nonce-Vergleiche**. **38 gemeinsame Storage-Werte** stimmen mit dem post-launch Read-Set der Zwischen-TX-Fortschreibung überein. Gasdebit des Launch-Senders und das Protokoll-Fee-Konto werden als außerhalb von `runCall` liegendes TX-Settlement ausgewiesen.

Das Initialisierungsergebnis lautet:

```text
quote price       = 96028517023029920
creator reserve   = 78962581156
creator supply    = 21453245135239692933310825
```

**Oracle vollständig gebunden:** Die Extension bildet ihren EIP-712-Digest aus Runtime-Type-/Domain-Konstanten, Chain-ID 56, Portaladresse sowie `quoteToken`, `salt`, `tokenVersion`, Moduswort 0, Preis und Deadline. Der Verifier rekonstruiert diese Hash-Preimages unabhängig aus Calldata und Bytecode-Literalen. `ecrecover` liefert `0x53f3a6115f6a2cc0ca8d388d7c169ecc9f5058ac`; die tatsächlich gelesene Signer-Rolle wird aus Portal-Storage validiert. Es werden keine ABI-Feldnamen für unbekannte Hash-Konstanten erfunden.

Die konkrete Gültigkeitsbedingung ist `block.timestamp <= 1790549433`. Gegenproben:

- Timestamp genau an der Deadline: akzeptiert, gleiche Creator-Reserve/-Supply.
- Timestamp Deadline + 1: **Revert** (`0xa29a0f00`).
- Tatsächlich benötigte Signer-Rolle im vorgehaltenen State entzogen: **Revert** (`0x4019b093`).
- Benötigter Signer-Storage-Slot aus dem Snapshot entfernt: **UNWITNESSED_STORAGE**, keine fingierte erfolgreiche Ausführung.

Der zukünftige Inclusion-Timestamp ist beim Pending-Signal nicht bekannt. Der spätere Harness muss mit ausreichender Gültigkeitsreserve arbeiten und bei nicht abgesicherter Zeit-/State-/Code-Bedingung **SKIP** ausgeben. Verzögerte Aufnahme kann weiterhin zum Revert führen.

**Hotpath-Klassifikation:** `hotpath_dependency_matrix.json` enthält **17 Feldgruppen**, **277 Storage-Abhängigkeiten** und insgesamt **497 Inventarzeilen** für alle drei beobachteten Pfade. Jede Zeile/Feldgruppe trägt Herkunft, Kategorie, Invalidierung und Neuberechnungsregel. Die vier Kategorien sind `startup precomputed`, `pre-resident`, `launch-derived`, `signal-time residual`.

Besonders verbindlich:

- Runtime/ABI/Templates/Verbindungen vorab auflösen; Registry-/Proxy-/Codewechsel invalidiert das Profil.
- Curve-, Pool-, Tick-/Bitmap-, Oracle-Rollen-, Extension-, Exemption-, Wallet-/Nonce-/Fee-State vorhalten; konsistente Version aus Blockhash, State-Cursor und Profilgeneration verwenden.
- Launch-Daten, CREATE2, Oracle-Digest und initialen Token-/Curve-State lokal ableiten.
- Quote, `minOutputAmount`, Nonce-Reservierung und spätere signaturgebundene Felder lokal finalisieren.
- Cache-Miss, unbekannter Pfad, fehlendes Tick-Fenster, unaufgelöste Reihenfolge, stale Snapshot oder zu knappe Oracle-Frist: **SKIP**. Kein synchrones RPC/HTTP/DB/filesystem/DNS im Entry-Hotpath.
- `minOutputAmount` kommt aus lokaler Quote und expliziter Slippage-Policy; es ist eine Revert-Schranke, keine Zusage über Inclusion oder erworbene Token. Die historische Router-Nullgrenze wird nicht als Trading-Policy übernommen.

Die Klassifikation zeigt, welche Daten vorher bereitliegen müssen. **Ein tatsächlich funktionierender Live-Cache und seine Coverage wurden damit nicht gemessen.** Insbesondere neue Wallets, andere Mengen, andere Tickpfade oder Extension-/Tax-Modi benötigen ihre eigenen G0.3-Korrektheitsfälle. Der genaue zukünftige Ausführungszustand kann sich durch fremde/private TXs ändern. Diese Grenzen werden nicht als gelöste Live-Eigenschaften ausgegeben.

Reproduzierbare Befehle im Audit-Workspace:

```bash
cd "/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2"
node gate0-evidence/replay_flap_evm.cjs launch first_buy direct_buy
node gate0-evidence/verify_flap_semantic_replay.cjs
node gate0-evidence/classify_flap_hotpath.cjs
python3 gate0-evidence/verify_flap_g02_economics.py
# Gesamtverifier: Exit 0, G0.2_FLAP_PASS
```

Der Gesamtverifier führt die State-Diff-, Semantik- und Klassifikationsprüfungen erneut aus. Die Requirements werden aus den bestandenen Prüfungen abgeleitet. Manipulierte/unvollständige Raw-Evidence oder eine fehlgeschlagene Ausführung führen zum Fehler; es werden keine bestehenden Resultat-JSONs als ungeprüfter PASS übernommen.

Technische Ergebnisse im bereits bestehenden Archive-Witness-Ordner aus Abschnitt 10.1:

```text
gate_result.json / witness_summary.json
semantic_replay_proof.json
launch_evm_replay.json / first_buy_evm_replay.json / direct_buy_evm_replay.json
pending_initial_state_evidence.json
hotpath_dependency_matrix.json
first_buy_calculation.json
verifier.stdout.txt / verifier.stderr.txt
final_artifact_sha256.json
```

Abhängigkeiten liegen isoliert unter `gate0-evidence/evm-audit-tools/`, mit exakten Releases und Tarball-Integrity in `package-lock.json`. Quelle: https://github.com/ethereumjs/ethereumjs-monorepo. Lizenz aus den installierten Paketen: **MPL-2.0** für evm/statemanager/util/block, **MIT** für common. Ein Git-Commit wird vom installierten Paket nicht ausgewiesen und deshalb nicht erfunden; der reproduzierbare Identitätsnachweis ist Release + Lockfile/Integrity. Es wurde kein EthereumJS-Code in den Produkt-BUY-Pfad kopiert.

**Gate-Entscheidung: G0.2 = PASS für diesen Scope. STOP vor G0.3 und Übergabe zur Review.** Kein LIVE-BUY, kein Signieren, kein Sender-Harness. Es wurde keine neue Latenz gemessen: `T_signal → T_send` und `T_buy` bleiben N/A. Die vorhandenen historischen Ausführungen beweisen keine 1-ms-Acquisition.

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

Abschließender Nachweis gemäß Abschnitt 10.2:

```text
G0.2 = PASS (eingefrorener historischer Modus, dokumentierte Guards)
G0.3 = NICHT GESTARTET; STOP zur Review
```

Die Kriterien sind Voraussetzungen für den späteren Harness, kein Nachweis bereits verfügbarer Live-Coverage oder Latenz.

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

<!-- G03_PROGRESS_START -->
## 12. G0.3 Zwischenprotokoll — Direct Portal Launch→BUY

Status: **IN PROGRESS / KEIN G0.3-PASS**

Scope:

\`\`\`text
Flap Portal: 0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0
Launch: newTokenV6 / TOKEN_TAXED_V3
Token: 0xabe1fe16cf6bc48c9524713054efe6e954b17777
Direct BUY selector: 0xef7ec2e7
swapExactInput((address,address,uint256,uint256,bytes))
\`\`\`

### 12.1 Rust-Harness: historische Direct-BUY-Konstruktion

**STEP 1 — PASS**

\`\`\`text
selector       = 0xef7ec2e7
calldata_bytes = 228
\`\`\`

Die ABI-kodierte Direct-Portal-BUY-Calldata wurde lokal in Rust gebaut und bytegenau gegen den historischen Direct-BUY verglichen.

**STEP 2 — PASS**

\`\`\`text
portal            = 0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0
value_wei         = 150000000000000000
historical_output = 28438305464314215025968609
\`\`\`

**STEP 3 — PASS**

\`\`\`text
launch_token         = 0xabe1fe16cf6bc48c9524713054efe6e954b17777
recipient            = 0xb6864c017e9d9d89012d4f90ee382bf1461e4c90
expected_acquisition = 28438305464314215025968609
min_output           = 6000000000000000000000000
semantic_difference  = 0
\`\`\`

Damit sind historischer Token, Recipient, Acquisition und das verwendete historische \`minOutput\` an die bereits unabhängig replayte G0.2-Acquisition gebunden.

**STEP 4 — PASS**

Der vollständige historische Legacy-TX-Envelope wurde einschließlich EIP-155-Signaturbindung rekonstruiert:

\`\`\`text
chainId      = 56
type         = 0
nonce        = 0x32e
gas          = 0x16e360
gasPrice     = 0x127a3980
value        = 0x214e8348c4f0000
v            = 0x93
y_parity     = 0

recovered_sender =
0xb6864c017e9d9d89012d4f90ee382bf1461e4c90

reconstructed_tx_hash =
0x8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b
\`\`\`

Recovered Sender und TX-Hash stimmen exakt mit der historischen Transaktion überein.

### 12.2 Archive-State für den Sequential-Test

Ein historischer \`eth_call\` über den öffentlichen Binance-RPC scheiterte mit:

\`\`\`text
missing trie node
\`\`\`

Das ist eine Archive-/Pruning-Grenze und kein BUY-Ergebnis.

Für den G0.3-Sequenztest wurde deshalb der bereits qualifizierte Archive-Zugang verwendet.

Erster Direct-BUY-Abhängigkeits-Snapshot:

\`\`\`text
gates/bsc/gate0/evidence/g03_direct_parent_state.json
12 Accounts
64 Storage-Slots
SHA256 80e391d70cb33f2d658ab7ccd5686fde0f79cdab6aeb840c030213af07b655c6
\`\`\`

Vollständiger Parent-State für alle Accounts aus dem historischen Direct-BUY-Prestate:

\`\`\`text
gates/bsc/gate0/evidence/g03_direct_parent_state_full.json
26 Accounts
64 Storage-Slots
SHA256 3a090b8398c581ee0b3a06c1a1a7cfe288dcdd231a106b6a60ab22511ff532b7
\`\`\`

Merge mit dem vorhandenen Launch-Parent-State:

\`\`\`text
launch_accounts   = 28
direct_accounts   = 26
overlap_accounts  = 17
added_accounts    = 9
added_slots       = 22
merged_accounts   = 37
conflicts         = 0
\`\`\`

### 12.3 Launch-Replay auf kombiniertem Parent-State

\`\`\`text
launch_exception = null
launch_frames    = 116
launch_match     = true
\`\`\`

Der historische Launch bleibt auf dem erweiterten Parent-State vollständig reproduzierbar.

### 12.4 Echter Sequential-Test: Parent → Launch → unmittelbarer Direct-BUY

Ausgeführt wurde:

\`\`\`text
Parent-State Block 124415356
→ historischer Launch Block 124415357
→ lokal berechneter Post-Launch-State
→ Direct Portal BUY
\`\`\`

Ergebnis auf dem tatsächlichen unmittelbar berechneten Post-Launch-State:

\`\`\`text
direct_exception             = revert
direct_frames                = 15
direct_missing_accounts      = []
direct_missing_storage_count = 0
direct_return_value          = 0x
\`\`\`

Damit ist ein Direct-BUY **unmittelbar nach dem Launch aktuell nicht als gültig bewiesen**.

Der Revert wurde bis in den Native→DOGE-Converter-/Pool-Pfad lokalisiert:

\`\`\`text
Portal 0xe2ce...
→ Portal implementation 0x27a0...
→ trade implementation 0x1105...
→ converter 0xc09f...
→ 0x9818...
→ 0x13f4...
→ pool 0xce6160...
→ 0xbc9e1a...
→ selector 0x214a6fe2
→ revert
\`\`\`

### 12.5 Timestamp-/Gate-Untersuchung

Launch-Timestamp:

\`\`\`text
1790549254
\`\`\`

Beim unmittelbaren Zustand:

\`\`\`text
0x214a6fe2(...6ab99d06) → revert
\`\`\`

Beim später historisch erfolgreichen Direct-BUY:

\`\`\`text
0x214a6fe2(...6ab99d07) → success
\`\`\`

Ein reines Verändern des Ausführungs-Timestamps beseitigt den Revert jedoch nicht:

| Delta zum Launch-Timestamp | Ergebnis |
|---:|---|
| +0 s | revert |
| +1 s | revert |
| +2 s | revert |
| +3 s | revert |

Daher ist **Zeit allein nicht die Freischaltbedingung**.

### 12.6 Read-Set und kausale State-Tests

Historischer Direct-BUY:

\`\`\`text
64 unique Storage-Reads
30 im Post-Launch-State bereits identisch
9 später verändert
25 im ursprünglichen Launch-Replay nicht vorhanden
\`\`\`

Bis zum unmittelbaren Revert:

\`\`\`text
20 unique Reads
8 Werte unterscheiden sich vom später erfolgreichen Direct-BUY
\`\`\`

Drei echte Prestate-Unterschiede wurden einzeln als Gegenfaktentest gepatcht:

| Patch | Ergebnis |
|---|---|
| Portal/Curve-State | weiterhin Revert, 15 Frames |
| \`0xce6160...\` Slot 0 | weiterhin Revert, 15 Frames |
| \`0xbc9e1a...\` Slot 2 | Ausführung läuft von 15 auf 32 Frames weiter |

Damit ist \`0xbc9e1a... slot 2\` als **kausaler Gate-State** isoliert.

Wichtig: Der spätere Wert dieses Slots ist **nicht** als unmittelbar nach Launch vorhanden bewiesen. Der Patch ist ausschließlich ein Gegenfaktentest zur Ursachenanalyse.

Der danach fehlende Read:

\`\`\`text
address = 0x3a632ecc01a45b8002d76c15e172e71c79412003
slot    = 0x5
\`\`\`

wurde unabhängig am Parent-Block über Archive-RPC abgefragt:

\`\`\`text
value = 0x0
\`\`\`

### 12.7 Fortgesetzter Gegenfaktentest mit beweisbar zulässigem Null-State

Für nachweislich im Launch neu erzeugte Contracts wurden nur solche historischen Direct-BUY-Read-Slots auf Null gesetzt, die im Launch nicht beschrieben worden waren. Für neu erzeugten EVM-Storage ist dieser Ausgangswert exakt Null.

\`\`\`text
zero_filled_created_slots = 6
\`\`\`

Zusammen mit dem isolierten **späteren** Wert von \`0xbc9e1a... slot 2\` ergab der Gegenfaktentest:

\`\`\`text
exception             = null
frames                = 50
missing_accounts      = []
missing_storage_count = 0
return_value          = 0x000000000000000000000000000000000000000000195e890851ef24d3319fd4
return_value_decimal  = 30669575743216741084405716
\`\`\`

Damit ist gezeigt:

1. Der zuvor isolierte \`0xbc9e1a... slot 2\` ist tatsächlich entscheidend dafür, dass der Converter-Pfad den frühen Revert überwindet.
2. Sobald dieser Gate-State auf den späteren Wert gesetzt wird und ausschließlich beweisbar-null initialisierte Slots ergänzt werden, läuft der Direct-BUY über alle **50 Frames** ohne Exception.
3. Das ist **kein Beweis**, dass der spätere Gate-State unmittelbar nach Launch real vorhanden war.
4. Der reale unmittelbare Post-Launch-State revertiert weiterhin nach 15 Frames.

### 12.8 Aktueller G0.3-Stand

Bewiesen:

\`\`\`text
historische Direct-BUY-Calldata                   PASS
historischer Value/Recipient/Acquisition          PASS
historischer Legacy-TX-Envelope + Signaturbindung PASS
historischer Direct-BUY-TX-Hash                   PASS
Launch-Replay auf erweitertem Parent-State        PASS
Revert-Pfad unmittelbar nach Launch               lokalisiert
kausaler Gate-State                               isoliert
\`\`\`

Noch offen:

\`\`\`text
welche reale Zustandsänderung setzt 0xbc9e... slot 2?
welche Transaktion / Aktion verursacht sie?
wann relativ zum Launch wird dieser Zustand gültig?
ist ein Direct-BUY schon vor dem ersten beobachteten externen BUY möglich?
welcher exakte früheste gültige Direct-BUY-State ist T0?
G0.3 Gesamt-PASS = NEIN
\`\`\`

Keine neuen Latenzbehauptungen:

\`\`\`text
BSC T_signal → T_send          N/A
BSC T_signal → erfolgreicher BUY
                               N/A
T_buy - T0                     N/A
\`\`\`

**Nächster zwingender Schritt:** die reale Mutation von \`0xbc9e1a... slot 2\` zwischen Launch und späterem erfolgreichen Direct-BUY aus den bereits archivierten geordneten Transaktionen/State-Diffs identifizieren. Erst danach darf der früheste gültige Direct-BUY-Zeitpunkt bewertet und G0.3 weiter abgeschlossen werden.
<!-- G03_PROGRESS_END -->


<!-- G03_ACTIVATION_RESOLUTION_START -->
### G0.3 Update 2026-09-29 — Slot-2-Ursache und unmittelbarer BUY

**Die Interpretation des frühen Reverts in 12.4–12.8 ist hiermit korrigiert.** Der bisherige Replayer kontrollierte fehlenden Storage, jedoch nicht jeden Code-Zugriff (insbesondere `EXTCODESIZE`). Im Parent-Snapshot fehlte der Contract `0x556b9306565093c855aea9ae92a594704c2cd59e`. Sein fehlender Code wurde als leerer Code behandelt; die Solidity-Contract-Existenzprüfung führte zum Revert. Das war kein belegtes On-Chain-Verbot des frühen BUY.

Eine strikte `getCode`-Prüfung meldet auf unverändertem ursprünglichen Sequential-State nun `UNWITNESSED_CODE 0x556b...d59e`. Alle drei bestehenden historischen G0.2-Replays behalten mit der strengeren Prüfung ihren exakten Calltree-Match.

`0xbc9e... slot 2` enthält unten `uint128 lmLiquidity = 315711145240336558` und darüber `uint32 lastRewardTimestamp`. Der Selector `0x214a6fe2` entspricht `accumulateReward(uint32)`. Bei einem bereits aktuellen Timestamp wird die Reward-Aktualisierung übersprungen; dadurch hatte der eingesetzte spätere Slot-Wert den fehlenden Contract-Pfad verdeckt. Die externe MasterChef-Abfrage lautet `getLatestPeriodInfo(pool)` / `0xa15ea89f`. Der tatsächliche Runtime-Code bleibt die Autorität. Offizieller Source-Kontext: https://raw.githubusercontent.com/pancakeswap/pancake-v3-contracts/main/projects/v3-lm-pool/contracts/PancakeV3LmPool.sol (keine Behauptung einer bytegleichen Source-Version).

Gezielter Read-only Archive-Nachweis der realen Änderungen:

| TX | Block:Index | `lastRewardTimestamp` vorher → nachher | Aktion |
|---|---|---|---|
| `0x73716f5fffab9493483d8eae7c1b170bc4d33feaa8346bb07d5c6a0d91d2595b` | 124415358:16 | 1790549253 → 1790549254 | externer Direct-Portal-BUY, V3-Pool ruft `accumulateReward` |
| `0x6f723464dbb64ba0d5e6f468b3a411161e7861baa6214dc7d38938b5a46514b6` | 124415359:17 | 1790549254 → 1790549255 | weiterer externer Direct-Portal-BUY, gleicher Reward-Pfad |

Vollständige Slot-Werte: `0x0000000000000000000000006ab99d0500000000000000000461a19d27ae98ae` → `0x0000000000000000000000006ab99d0600000000000000000461a19d27ae98ae` → `0x0000000000000000000000006ab99d0700000000000000000461a19d27ae98ae`. Die Änderungen sind Folge externer BUY-Ausführung; sie müssen den eigenen BUY nicht zuvor freischalten.

**Vervollständigung ausschließlich am Parent-Blockhash** `0x854055a293db2a4c69d4827e00654bc3be75d0484b1f4258a8056380672d1104`: Code/Balance/Nonce des fehlenden Contracts plus fünf seiner Storage-Slots und Slot 0 des LM-Pools. Keine späteren State-Werte übernommen. Die sechs nachweislich frisch erzeugten Contract-Slots behalten ihre zulässige Nullinitialisierung.

Ein vollständiger Test `Parent 124415356 → Launch 124415357:0 → unmittelbar Direct BUY` ergibt:

```text
launch: 116 frames, exact historical match
buy:    51 frames, exception = null
missing accounts/storage: 0
return = 30669575743216741084405716
logs   = 21
```

Der zusätzliche 51. Frame ist die nun wirklich ausgeführte MasterChef-Abfrage. Der BUY selbst aktualisiert den Reward-Timestamp von 1790549253 auf 1790549254. Damit ist die bislang behauptete Wartebedingung widerlegt. Empfänger-Balance-/Kostenprüfung, Rust-Quote und Minimum-Output-Gegenproben folgen vor dem G0.3-Gesamtentscheid.

Evidence im echten Repo: `gates/bsc/gate0/evidence/g03_activation_raw/` (Requests, Raw-Antworten, Hashes), `g03_parent_closure.json`, `g03_sequential_execution.json`. Befehle: `python3 gates/bsc/gate0/tools/collect_g03_activation.py`, `node gates/bsc/gate0/tools/probe_sequence.cjs`. Ein Sequential-Sample, zwei historische Mutations-Traces, gezielte Closure-Reads. Kein Signing/Send, keine Live-Acquisition, keine neue Latenzzahl. Noch kein G0.3-Gesamt-PASS.
<!-- G03_ACTIVATION_RESOLUTION_END -->


### G0.3 Abschluss 2026-09-29 — PASS für den eingefrorenen Offline-Scope

Gesamtbefehl im echten Repo:

```bash
cargo test --manifest-path gates/bsc/gate0/Cargo.toml --locked --offline
node gates/bsc/gate0/tools/verify_g03.cjs
```

Rust: **3 Tests bestanden**, 0 fehlgeschlagen. Gesamtverifier: **Exit 0 / G0.3_PASS_PINNED_OFFLINE_SCOPE**. Das vorhandene `src/main.rs` bleibt erhalten und verifiziert weiterhin den historischen Legacy-Envelope, Chain-ID 56, Sender-Recovery und TX-Hash. `src/bin/g03.rs` ergänzt den minimalen Korrektheitslauf: dekodiertes V6-Launch-Tupel, CREATE2, Creator-Kurve, Native→DOGE-V3-Single-Segment-Quote, Fee-/Tax-Rundung und exakte Direct-Portal-Calldata. Keine neue Produktionsarchitektur.

Die verwendete V3-Quote ist ausdrücklich auf die belegte Poolposition/Liquidität und 0.15 BNB begrenzt. Der Verifier bindet diese Werte an den Parent-State und prüft, dass der EVM-Swap keinen Tick überschreitet. Unqualifizierte Werte werden abgewiesen; dies ist keine allgemeine V3-Quote-Engine.

```text
Token:                 0xabe1fe16cf6bc48c9524713054efe6e954b17777
Recipient:             0xb6864c017e9d9d89012d4f90ee382bf1461e4c90
Native Input/Debit:     150000000000000000 wei (Gas-Settlement separat)
DOGE Quote Input:       120880165276
Protocol Fee (DOGE):    1208801652
Tax (DOGE):             1208801653
Creator Reserve:       78962581156
Creator Acquisition:   21453245135239692933310825
External Acquisition:  30669575743216741084405716
Rust − EVM Difference: 0
Recipient Balance:     0 → 30669575743216741084405716
```

Der Tokenzugang wird durch lokal ausgeführten historischen `balanceOf`-Code vor/nach dem BUY nachgewiesen, zusätzlich zu Returnwert/Transferpfad. Der Native-Balanceabgang entspricht 0.15 BNB. Das historische Sender-Nonce ist bereits im Parent-State 814 (`0x32e`); dessen Balance deckt Value plus `gasLimit * gasPrice`. Gas-Settlement wird nicht als Teil der Message-Balanceprüfung behauptet.

Minimum-Output-Tests aus demselben echten Post-Launch-State:

| Test | Minimum | Ergebnis |
|---|---:|---|
| explizite Offline-Testpolicy 100 bps | 30362879985784573673561658 | erfolgreich, exakt erwarteter Balancezuwachs |
| exakt erwarteter Output | 30669575743216741084405716 | erfolgreich |
| erwarteter Output + 1 Basiseinheit | 30669575743216741084405717 | Revert; Empfänger-Balance unverändert |
| fehlender MasterChef-Code | — | `UNWITNESSED_CODE`, kein fingierter Contract-Revert |
| BUY vor dem Launch | — | Revert, vollständig belegter erforderlicher Read-Set |

Für die Vor-Launch-Negativprobe wurde ein zusätzlicher Portal-Slot am Parent-Blockhash abgefragt; auch hier kein geratenes Null-Default.

Die 100-bps-Regel ist **nur eine Testpolicy**, keine Freigabe einer Produktions-Slippage. Der historische signierte Envelope enthält weiterhin sein historisches Minimum 6e24 und ist für diesen Sequential-State gültig. Die separat in Rust gebaute Policy-Calldata wird semantisch geprüft, aber nicht als mit dem historischen Wallet neu signiert ausgegeben. Es wird kein Private Key benötigt.

**Aktivierung:** Der ausgewählte Direct-Portal-BUY kann im geprüften Zustand unmittelbar nach dem erfolgreichen Launch ausgeführt werden. Der BUY aktualisiert `lastRewardTimestamp` selbst. Kein vorgeschalteter fremder BUY, kein Frontend und keine zusätzliche Reward-Freischaltung nötig. Der Creator-BUY bleibt launch-intern und ist kein überholbarer externer Konkurrent. Dies beweist ausführbare Reihenfolge in der Simulation, keine erreichte reale Inclusion-Position und keinen numerischen `T_buy−T0`-Wert.

Eingefrorene Belege: `gates/bsc/gate0/evidence/g03_result.json`, `g03_rust_inputs.json`, `g03_rust_output.json`, `g03_sequential_execution.json`, `g03_parent_closure.json`, `g03_activation_raw/`, `g03_verifier.stdout.txt`, `g03_verifier.stderr.txt`, `g03_sha256.json`. Fünf explizite Grenz-/Negativfälle plus Rust-Arithmetiktests; der historische Signatur-/Hash-Test wird vom Gesamtverifier mit ausgeführt. Alle 3 G0.2-Referenzausführungen behalten mit strikter Code-Verfügbarkeitsprüfung ihre Übereinstimmung.

**Folgegates:** G0.4/G0.5 dürfen jetzt weiter vorbereitet werden. Die lokale Bestandsprüfung findet weder geth/BSC-Binary noch einen BSC/geth-Docker-Container; die laufenden Container gehören zu OpenProject. Im Repository-`.env` wurden keine BSC/geth/48Club/BlockRazor-Konfigurationsvariablen gefunden (keine Werte ausgegeben). Ein externer Node-/Feed-Zugang wurde angefragt. Die vorhandene QuickNode-Archive-Verbindung ist kein Nachweis eines eigenen P2P-Knotens oder einer qualifizierten Inclusion-Verbindung. `T_signal → T_send` bleibt N/A. Kein value-bearing Send und kein Gate-0-Gesamt-PASS.


### G0.4 Quellenprobe und Fortsetzungspunkt — 2026-09-29

Stand Git vor diesem Update: `ffd77c0`; G0.3-Abschluss: `7a6e532`. Der Nutzer bestätigt: kein eigener bsc-geth und kein eingerichteter 48Club-/BlockRazor-Zugang. Vorhanden ist nur der externe QuickNode-Archive-Zugang. Credentials werden nicht dokumentiert.

**Durchgeführte Quellenprobe:** ein WebSocket-Lauf auf dem Mac, Node v24.1.0, parallele read-only Subscriptions `newPendingTransactions(true)`, Portal-Logs und `newHeads`. Vorab begrenzt auf 60 Sekunden, 4.000 Nachrichten oder 8 MiB. Ende nach **18,752543375 Sekunden durch die 4.000-Nachrichten-Grenze**. Keine nachträgliche Auswahl günstiger Ergebnisse. Der JS-Observer prüft Quellenfähigkeit; er ist nicht der Rust-BUY-Hotpath.

| Messwert | Ergebnis |
|---|---:|
| vollständige Pending-TX-Objekte | 3.855 |
| Hash-only Pending-Nachrichten | 0 |
| fehlende geprüfte Basisfelder | 0 |
| Pending-TXs direkt an Portal | 3 |
| davon `newTokenV6` | 1 |
| Portal-Lognachrichten | 102 |
| Head-Nachrichten | 40 |
| RPC-/Verbindungsfehler | 0 |
| qualifizierte Launches unseres eingefrorenen Scopes | **0** |

Der V6-Launch `0xb8d5c29c6c647d393b1e883eca2e093ffac380c4345ca7dd9c281d124e58c898` hat TokenVersion 6, Tax 100/100 bps, aber Quote-Token `0x21caef8a43163eea865baee23b9c2e327696a3bf`, QuoteAmt 0 und Extension-ID 0. Er gehört **nicht** zum bewiesenen DOGE-/Extension-Modus. Selector-Gleichheit wird nicht als unterstützter Scope ausgegeben.

Zehn empfangene Portal-Logs gehören zu dieser TX. Zwischen lokalem Pending-Empfang und erster zugehöriger Logzustellung liegen **129.102.833 ns**. Das ist ausschließlich eine Zustellungsdifferenz zweier Subscriptions auf derselben lokalen Uhr, **keine BUY-Latenz, kein T0 und kein autoritativer Pre-Lead**. Aus normalen Block-/Logdaten wird kein Ausführungszeitpunkt im Millisekundenbereich erfunden.

Evidence: `gates/bsc/gate0/evidence/g04_quicknode_probe/capture.json`, `sha256.json`, `analysis.json`. Offline-Prüfung: `node gates/bsc/gate0/tools/verify_g04_probe.cjs` → Exit 0. Gate-Status bleibt **G0.4_PARTIAL_SOURCE_CAPABILITY_ONLY**; kanonischer Launch-Denominator/Coverage-Rate bleiben null. Ein positiver Verifier-Exit bedeutet hier lediglich konsistente Quellen-Evidence.

#### Konkrete Infrastruktur-Anforderungen und Setup-Reihenfolge

Lokal geprüft: Mac arm64, **16 CPU-Kerne, 128 GiB RAM, nur 116 GiB freier Speicher**. Kein geth-Binary und kein geth-Docker-Container. Deshalb keinen Mainnet-Snapshot auf diesem Laufwerk starten.

1. **Node-Host bereitstellen:** Linux-Host mit mindestens 16 Kernen / 64 GB RAM gemäß offizieller Full-Node-Baseline; für diesen Versuchsaufbau ist **4 TB dediziertes NVMe** mit Reserve für einen pruned Full-Node ein sinnvoller Planungsvorschlag, keine gemessene Performance-Garantie. Rust und geth auf demselben Host, damit IPC/P2P-Handoff messbar ist. Keine kostenpflichtige Ressource wurde bestellt.
2. **Version und Snapshot exakt einfrieren:** produktionsgeeignetes BSC-Release samt Commit/Checksumme und passendem Mainnet-config/genesis wählen. Nicht blind `releases/latest` installieren: der am 2026-09-29 geprüfte Link zeigte `v1.8.0-alpha`, ausdrücklich eine Preview. Das offizielle Snapshot-Verzeichnis listete Mainnet 20260805 mit etwa **1,7 TB pruned**, etwa **6,6 TB full**, jeweils BSC >=1.7.2. Auswahl und tatsächliche aktuelle Größe vor Download erneut prüfen.
3. **Snapshot laden/verifizieren:** passendes offizielles `fetch-snapshot.sh` prüfen und pinnen; pruned Modus mit Prüfsummen verwenden, in ein neues dediziertes Datadir extrahieren. Keine bestehende Chain-Datenbank überschreiben. Das dokumentierte Beispiel lautet `bash fetch-snapshot.sh -d -e -c -p --auto-delete -D /data/snapshot -E /data/bsc mainnet-geth-pbss-20260805`; es ist hier **nicht ausgeführt**. Der Auto-delete-Modus entfernt entpackte Download-Archive. Kein Archive-Node nötig: historische Evidence bleibt bei QuickNode.
4. **Node starten und qualifizieren:** versionseigenes Mainnet-config verwenden, IPC lokal halten, HTTP/WS nur Loopback; kein öffentliches Admin-/Debug-RPC. Synchronisation, Chain-ID 56, Head-Fortschritt und Peers prüfen. Erst nach Sync vollständige Pending-Bodies plus kanonische Launch-Erfassung prüfen. Der genaue Startbefehl wird gegen `geth --help` der ausgewählten Binary validiert; es gibt noch keinen installierten Host oder scheinbar getesteten Startbefehl.
5. **G0.4 messen:** eigener geth, QuickNode und zugängliche zusätzliche Feeds auf derselben lokalen Uhr vergleichen. Vollständige Launch-Daten, unsupported Modi, Source-Gaps, Reconnects und kanonisch nacherfasste/verpasste Launches in den Nenner aufnehmen. Die aktuelle Mini-Probe ersetzt diese Messung nicht.
6. **48Club separat qualifizieren:** Auction Feed benötigt laut Dokumentation 48 SP, State-Variante 480 SP und Timestamp-Signatur des berechtigten Accounts. Logs/State-Diffs werden gefiltert; die Topic-Liste enthält Four.meme Create, aber keinen ausdrücklich gelisteten Flap-Create-Topic. Flap-Verwendbarkeit deshalb offen prüfen, nicht voraussetzen. `backrunTarget`-Semantik und Feldvollständigkeit separat testen. Kein Zugang gekauft oder eingerichtet.
7. **BlockRazor separat qualifizieren:** Account/Auth und Berechtigung für den konkreten BSC-Stream erforderlich. Public-Mempool-Stream, Private-Orderflow und Builder-Submission sind unterschiedliche Produkte/Pfade. Region, vollständige Launch-Felder und jeweilige Bundle-Semantik prüfen; keine Gleichsetzung und kein angenommener Geschwindigkeitsvorteil. Kein Zugang gekauft oder eingerichtet.
8. **G0.5/G0.6 erst mit tatsächlich verfügbarem Transport:** Rust Decode/Quote/Patch/Nonce/Sign/Serialize plus echte Transportgrenze messen; Queue separat. Beim geth-Pfad reicht Rust→IPC nicht als P2P-T_send-Nachweis. Sender/Nonce/Cache bleiben vorgehalten. Der bestehende G0.3-Fixture-Harness ist noch kein vollständiger Live-Hotpath. Live-Wallet, Testbetrag und Produktions-Slippage sind nicht freigegeben; G0.7 stoppt vor jedem value-bearing Send.

Offizielle Quellen, geprüft 2026-09-29:

- [BNB Node-Baseline](https://docs.bnbchain.org/bnb-smart-chain/developers/node_operators/node_best_practices/)
- [Snapshot-Größen und Verwendung](https://github.com/bnb-chain/bsc-snapshots/blob/main/README.md)
- [BSC Preview v1.8.0-alpha](https://github.com/bnb-chain/bsc/releases/tag/v1.8.0-alpha)
- [48Club Auction Feed](https://docs.48.club/puissant-builder/auction-transaction-feed)
- [48Club Send Bundle](https://docs.48.club/puissant-builder/send-bundle)
- [BlockRazor Public Mempool](https://docs.blockrazor.io/streams/public-mempool)
- [BlockRazor Private Mempool](https://docs.blockrazor.io/streams/private-mempool)
- [QuickNode eth_subscribe](https://www.quicknode.com/docs/bnb-smart-chain/eth_subscribe)

**Fortsetzung:** G0.3 nicht neu anfangen. Zuerst Node-Host/Storage und Kandidatenzugänge konkret bereitstellen; dann G0.4 mit kanonischem Denominator und Rust-G0.5 bis zur qualifizierten Transportgrenze durchführen. Keine Gewinnerannahme. Aktuell bleiben `T_signal→T_send`, `T_buy−T0` und Gate-0-Gesamt-PASS unbelegt.


### G0.4 QuickNode Coverage v2 — rate-limit run 2026-09-29

Aufbau: lokale QuickNode-WebSocket-Probe mit vollständigen Pending-TX-Objekten, Portal-Logs und Head-Subscription. Für Portal-Log-TXs wurden zusätzlich `eth_getTransactionByHash`-Abfragen ausgelöst. Geplante Laufzeit 600 s; tatsächlicher Abbruch durch Provider nach **197,727906875 s**.

Ergebnis:

| Messwert | Ergebnis |
|---|---:|
| vollständige Pending-TXs | 42.886 |
| eindeutige Pending-Hashes | 32.917 |
| Hash-only Pending | 0 |
| Pending mit fehlenden Basisfeldern | 0 |
| Portal-Pending-TXs | 46 |
| Portal-Logs | 2.136 |
| Heads | 429 |
| kanonische Portal-TX-Fetches | 480 |
| kanonische `newTokenV6`-Launches im ausgewerteten Census-Fenster | 4 |
| davon zuvor im Pending gesehen | **4/4** |
| beobachtete V6-Pending-Coverage dieser Stichprobe | **100 %** |
| Launches im eingefrorenen DOGE-/Extension-Scope | **0** |

Status bleibt **G0.4_PARTIAL_NO_FROZEN_SCOPE_DENOMINATOR**. Die 4/4-Beobachtung ist eine positive QuickNode-Stichprobe für allgemeine V6-Sichtbarkeit, aber **kein Coverage-PASS für den eingefrorenen G0.3-Scope**, weil kein Launch dieses Scopes im Nenner lag.

Providerfehler: sieben `-32007`-Antworten mit **15 requests/s limit reached** und anschließend eine `-32003 request limit reached`-Antwort; danach schloss der Server die Verbindung. Damit ist die aktuelle Probe zusätzlich durch die eigene Log→`eth_getTransactionByHash`-Request-Burst-Strategie belastet. Diese Probe wird nicht als 600-s-Coverage-Lauf gewertet.

Evidence: `gates/bsc/gate0/evidence/g04_quicknode_coverage_v2/capture.json`, `sha256.json`, `analysis.json`; Collector/Analyzer: `gates/bsc/gate0/tools/probe_g04_coverage_v2.cjs`, `gates/bsc/gate0/tools/analyze_g04_coverage_v2.cjs`. Raw-Evidence und zugehörige Tools sind ab Evidence-Commit `cdd5950867c633906ce4aa5354f3f9a2cf85a2aa` versioniert.

**Nächster Messschritt:** keine Upgrade-Annahme. Collector so umbauen, dass der kanonische Nenner ohne per-Log-TX-Fetch-Bursts entsteht, Requests deutlich unter 15/s bleiben und Source-Gaps/Reconnects explizit erfasst werden. Danach längeren bounded Run durchführen. `T_send`, `T_buy`, `T0` und vollständige BUY-Latenz bleiben N/A; kein value-bearing Send.


### G0.4 QuickNode v3 connection-limit diagnosis — 2026-09-29

Die geplante v3-Coverage-Probe wurde **nicht als Messlauf gewertet**. Beim Start kam zwar eine WebSocket-Verbindung zustande (`OPEN`), aber bereits die erste `eth_subscribe`-Anfrage erhielt:

```text
code: -32003
message: request limit reached
```

Der Server schloss anschließend die Verbindung sauber mit WebSocket-Close-Code `1001` (`upstream went away`). Ein isolierter 30-s-`newHeads`-Verbindungstest reproduzierte denselben Zustand. Damit ist belegt, dass zu diesem Zeitpunkt **der QuickNode-Endpunkt selbst wegen Request-/Planlimit keine Subscription annahm**; die v3-Reconnect-Zahlen sind deshalb kein Ingress-/Coverage-Messwert.

Folge:
- v3 liefert **keine Coverage-Evidence** und keinen neuen Nenner;
- die vorherige v2-Stichprobe bleibt die letzte verwertbare QuickNode-Evidence;
- kein `T0`, `T_send`, `T_buy` oder BUY-Latenzwert wird daraus abgeleitet;
- weitere automatische Reconnects gegen denselben limitierten Endpoint sind bis Quota-Reset/Planänderung/anderem Endpoint zu vermeiden.

Security-Hinweis: Zugangsdaten werden nicht im Protokoll dokumentiert.


### Gate-0 Fortsetzungsentscheidung — 2026-09-29

Ziel bleibt unverändert: **zuerst falsifizieren oder bestätigen, ob der kontrollierbare lokale BUY-Pfad unter 1 ms bleiben kann; erst danach kostenpflichtige Infrastruktur oder Hardware beschaffen.**

Der Phasenplan wird dabei nicht geändert:

- G0.3 ist semantisch PASS und erfüllt damit die Voraussetzung für G0.5.
- G0.4 läuft weiterhin parallel/opportunistisch und bleibt PARTIAL; ein fehlender eigener Node oder ein vorübergehend limitierter QuickNode-Endpunkt darf die lokale G0.5-Vorqualifikation nicht unnötig stoppen.
- G0.5 wird jetzt in zwei klar getrennten Evidenzstufen fortgesetzt:
  1. **LOCAL PREQUALIFICATION:** kompletter lokal kontrollierbarer Pfad `T_pre → Decode → Residual/Quote → TX finalize → Sign → Serialize → lokale Adapter-/Socket-Übergabe`. Dieser Lauf darf das <1-ms-Ziel früh falsifizieren oder stützen.
  2. **TRANSPORT QUALIFICATION:** später derselbe Rust-Pfad bis zur tatsächlich ausgewählten realen Transportgrenze. Erst diese Stufe kann einen finalen G0.5-`T_send`-PASS liefern.
- Ein lokaler Mock-/Socket-Handoff wird **nicht** als echter BSC-P2P-/Builder-`T_send` ausgegeben.
- Node-Hardware, zusätzlicher Speicher, QuickNode-Upgrade sowie 48Club-/BlockRazor-Verträge werden **nicht vorab beschafft**, nur um Gate 0 fortsetzen zu können. Beschaffung wird erst nach positiver lokaler Latenz-Vorqualifikation und konkretem Bedarf aus G0.4–G0.6 entschieden.
- G0.6/G0.7/G0.8 bleiben in ihrer ursprünglichen Reihenfolge. Vor G0.7 kein value-bearing Send ohne ausdrückliche Nutzerfreigabe.

**Entscheidungslogik:** Wenn bereits die lokale G0.5-Vorqualifikation das Ziel `T_pre → lokale Übergabe < 1 ms` auf target-like Hardware nicht robust halten kann, wird zuerst lokal optimiert bzw. die Edge-Annahme neu bewertet. Wenn sie das Ziel hält, wird anschließend genau die Infrastruktur beschafft/eingerichtet, die für den realen G0.4/G0.5/G0.6-Nachweis benötigt wird.

Keine Änderung an den Non-Claims: `T_signal→T_send`, `T_buy−T0` und Gate-0-Gesamt-PASS bleiben bis zu den vorgesehenen realen Messungen unbelegt.


### G0.5 Local Prequalification — 2026-09-29

Commit der Messimplementierung und Evidence: `3d032f6` (`Add G0.5 local hotpath prequalification benchmark`).

Scope: eingefrorener G0.3-Flap-Fixture, Rust Release-Build auf macOS/aarch64. Gemessen wurde der lokal kontrollierbare Pfad:

```text
T_pre
→ Launch Decode + CREATE2
→ lokale Quote + minOutput
→ TX finalize + sighash
→ warmer secp256k1-Signer
→ signed legacy TX serialization
→ UnixDatagram send-return als lokale Adaptergrenze
```

Correctness-Bindung vor der Messung:
- G0.3-Fixture-Match = true
- Token = `0xabe1fE16cf6Bc48c9524713054efe6e954b17777`
- Expected output = `30669575743216741084405716`
- Expected minOutput = `30362879985784573673561658`

Messumfang: **5.000 Warmup + 50.000 Messsamples**.

| Stufe | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|
| Decode + CREATE2 | 0,542 µs | 0,625 µs | 0,667 µs | 7,291 µs |
| Quote + minOutput | 0,417 µs | 0,458 µs | 0,459 µs | 7,000 µs |
| TX finalize + sighash | 0,750 µs | 0,833 µs | 0,875 µs | 10,334 µs |
| secp256k1 Sign | 29,583 µs | 31,208 µs | 33,625 µs | 54,292 µs |
| Signed-TX serialize | 0,209 µs | 0,291 µs | 0,333 µs | 13,792 µs |
| UnixDatagram lokale Übergabe | 0,458 µs | 1,000 µs | 1,209 µs | 7,833 µs |
| **T_pre → T_ready local** | **31,541 µs** | **33,375 µs** | **35,916 µs** | **67,167 µs** |
| **T_pre → lokale Übergabe** | **32,000 µs** | **33,875 µs** | **36,750 µs** | **69,417 µs** |

Damit ist die **lokale 1-ms-Vorqualifikation klar PASS**: selbst der gemessene Maximalwert des 50k-Laufs liegt mit 69,417 µs weit unter 1.000 µs. Der dominante lokale Anteil ist der secp256k1-Signer.

Evidence:
- `gates/bsc/gate0/src/bin/g05_local.rs`
- `gates/bsc/gate0/evidence/g05_local_prequalification/result.json`
- SHA256 `8de2bba322cdbb85ae4dca788b6897dde3976180cf30190fd9a05d3dbdd42e9b`

**Non-Claims / Gate-Grenze:** Der UnixDatagram-`send()`-Return ist nur eine lokale Adaptergrenze. Er ist **kein** realer bsc-geth-P2P-`T_send`, **kein** Builder-`T_send`, **kein** `T_buy` und schließt G0.5 final noch nicht. Die Messung bestätigt jedoch, dass die lokale Rust-Verarbeitung die 1-ms-Hypothese nicht falsifiziert; kostenpflichtige Infrastruktur muss deshalb nicht vor dieser lokalen Entscheidung beschafft werden.

**Fortsetzung:** G0.4 bleibt parallel PARTIAL. Als nächstes G0.5 lokal robustifizieren (unabhängige Wiederholung/Verteilungsstabilität und Adaptergrenze), danach reale Transportqualifikation nur mit tatsächlich verfügbarem Kandidatenpfad. G0.6/G0.7 bleiben unverändert nach Plan; kein value-bearing Send ohne ausdrückliche Freigabe.


### G0.5 Local Stability — 500.000 Durchläufe

Evidence-Commit: `b903892` (`Add G0.5 local hotpath stability evidence`).

Aufbau: derselbe eingefrorene G0.3-Fixture und derselbe Rust-Release-Hotpath wie bei der ersten G0.5-Local-Prequalification. **10 unabhängige Läufe × 50.000 Messsamples = 500.000 gemessene Durchläufe**, jeweils zusätzlich 5.000 Warmups.

`T_pre → local handoff` über alle 10 Läufe:

| Kennwert | Ergebnis |
|---|---:|
| bester Run p99 | **35,333 µs** |
| schlechtester Run p99 | **37,125 µs** |
| bester beobachteter Run-Max | **58,583 µs** |
| schlechtester beobachteter Run-Max | **550,875 µs** |
| Runs mit p99 < 1 ms | **10/10** |
| beobachtete Maxima < 1 ms | **10/10** |

Einzelne höhere Scheduling-/OS-Ausreißer sind sichtbar (z. B. 550,875 µs im ersten Lauf und 287,084 µs im neunten Lauf), bleiben in dieser Stichprobe aber ebenfalls unter 1 ms. Der p99-Bereich ist über alle Läufe eng und stabil.

Evidence:
- `gates/bsc/gate0/evidence/g05_local_stability/run_01.json` bis `run_10.json`
- `gates/bsc/gate0/evidence/g05_local_stability/summary.json`
- SHA256 Summary: `1d1ea23a246d989a1cb6116379eea3a7be5eb8cbc834fa4bee7316f8f92c8388`

**Einordnung:** Die lokale <1-ms-Hypothese ist nach insgesamt 500.000 zusätzlichen Messdurchläufen robust gestützt. Dies bleibt weiterhin **LOCAL PREQUALIFICATION**, nicht finaler G0.5-Transport-PASS. Der gemessene UnixDatagram-Handoff ist kein realer bsc-geth-P2P- oder Builder-`T_send`.

**Nächster G0.5-Schritt:** die tatsächlich für einen lokalen geth-Pfad relevante Adapterarbeit separat vorqualifizieren: signed raw transaction → Hex/JSON-RPC `eth_sendRawTransaction` framing → vorverbundener Unix-Stream/IPC-Handoff. Danach erst reale geth-/P2P-Transportqualifikation auf verfügbarer Infrastruktur.


### G0.5 Local IPC Prequalification — 2026-09-29

Evidence-Commit: `8515823` (`Add G0.5 local IPC prequalification`).

Scope: exakter historischer G0.3 Direct-BUY-Envelope, lokaler vorverbundener UnixStream, kein echter geth-Prozess. Gemessen wurden 10.000 Warmups + **100.000 Samples** für:

```text
signed raw TX
→ Hex-Encoding
→ JSON-RPC eth_sendRawTransaction framing
→ UnixStream write_all
```

Ergebnis:

| Stufe | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|
| Raw-TX Hex-Encoding | 1,208 µs | 2,125 µs | 2,667 µs | 18,625 µs |
| JSON-RPC-Framing | 0,041 µs | 0,042 µs | 0,042 µs | 0,208 µs |
| UnixStream write_all | 0,583 µs | 1,042 µs | 1,333 µs | 15,750 µs |
| **signed raw TX → local IPC handoff** | **1,875 µs** | **2,917 µs** | **3,584 µs** | **19,208 µs** |

Raw-TX: 343 Bytes; JSON-RPC-Frame: 761 Bytes. SHA256 der Evidence: `fd162da5bf677aa9857c27971d2d52287e512c50cf056933681601f8cfa61ad9`.

**Einordnung:** Auch die realistischere lokale JSON-RPC/IPC-Adapterarbeit liegt mit deutlichem Abstand unter 1 ms. Dies bleibt Vorqualifikation: kein echter geth-Prozess, keine P2P-Propagation, kein finaler `T_send`, kein `T_buy`, kein value-bearing Broadcast.

**Nächster Schritt:** vollständigen lokalen Pfad in **einer** Messung erfassen: Launch Decode/CREATE2 → Quote/minOutput → TX finalize/sighash → warm sign → signed serialization → Hex/JSON-RPC → vorverbundener UnixStream-Handoff. Keine Addition separater Benchmarks.


### G0.5 Local E2E Prequalification — 2026-09-29

Evidence-Commit: `4239a0f` (`Add G0.5 local E2E hotpath benchmark`).

Scope: eingefrorener G0.3-Flap-Fixture; **eine zusammenhängende Messung** des kompletten lokal kontrollierbaren Pfads:

```text
Launch Decode + CREATE2
→ Quote + minOutput
→ TX finalize + sighash
→ warmer secp256k1-Signer
→ signed TX serialization
→ Hex + JSON-RPC eth_sendRawTransaction framing
→ vorverbundener UnixStream write_all
```

5.000 Warmups + **50.000 Messsamples**. Correctness-Bindung an den G0.3-Fixture blieb aktiv: Token, erwarteter Output und minOutput stimmen.

| Messpunkt | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|
| `T_pre → T_ready local` | 31,500 µs | 35,708 µs | 36,541 µs | 517,042 µs |
| **`T_pre → JSON-RPC IPC handoff`** | **32,875 µs** | **37,625 µs** | **38,875 µs** | **537,042 µs** |

Teilwerte p99: Decode/CREATE2 0,709 µs; Quote/minOutput 0,459 µs; TX finalize+sighash 0,875 µs; secp256k1 Sign 34,250 µs; signed serialization 0,334 µs; Hex+JSON-RPC 1,167 µs; UnixStream-Handoff 1,834 µs.

Evidence:
- `gates/bsc/gate0/src/bin/g05_e2e_local.rs`
- `gates/bsc/gate0/evidence/g05_local_e2e/result.json`
- SHA256: `91e051b6bdb19e31b28be7f9bff40c425f666b690a693d5684293d738df68cc3`

**Einordnung:** Die zentrale lokale <1-ms-Hypothese bleibt in einem einzigen End-to-End-Messpfad mit großem Abstand bestehen; selbst das beobachtete Maximum lag bei 537,042 µs. Damit ist weitere lokale Mikromessung für die Vorentscheidung nicht mehr der Engpass.

**Non-Claim:** UnixStream-`write_all` bleibt eine lokale Adaptergrenze und ist weder echter bsc-geth-P2P-`T_send` noch Builder-`T_send`, `T_buy` oder finaler G0.5-PASS.

**Fortsetzung nach Plan:** keine weitere Beschaffung nur für Vorqualifikation. Nächster Schritt ist reale, nicht-value-bearing Transportqualifikation mit einem tatsächlich verfügbaren BSC-Transportkandidaten; erst danach kann der finale `T_send`-Teil von G0.5 geschlossen werden.


### G0.5 Real Remote Transport Prequalification — 2026-09-29

Transportkandidat: offizieller öffentlicher BSC-Mainnet-RPC `bsc-dataseed.bnbchain.org:443`, vorverbundene TLS/TCP-HTTP/1.1-Verbindung. Chain-ID 56 wurde vor der Messung verifiziert.

Safety: gesendet wurde ausschließlich ein absichtlich **nicht parsebarer 343-Byte-Payload** (erste Bytefolge kein gültiger RLP-Transaction-Envelope). Der Endpoint antwortete deterministisch mit `unmarshal transaction failed`. Es wurde **keine gültige oder value-bearing Transaktion** ausgesendet.

Messung: 10 Warmups + **100 Samples**.

| Messpunkt | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|
| TLS/TCP `write_all + flush` Handoff | **176,750 µs** | **363,291 µs** | **472,084 µs** | **525,958 µs** |
| Request → JSON-RPC Response | 110,780 ms | 131,117 ms | 157,384 ms | 183,345 ms |

SHA256 der lokalen Evidence: `98c359c47a869588f84b55f08786f8d33de3a0101a4e1343eeb60d2ad358d935`.

**Einordnung:** Die tatsächliche warme TLS/TCP-Socket-Übergabe an einen realen externen BSC-RPC-Endpunkt liegt in dieser Stichprobe selbst bei p99 und Maximum unter 1 ms. Die Antwort-RTT liegt dagegen im Bereich 110–183 ms und ist **nicht** Teil der lokalen BUY-Latenz bzw. von `T_send`.

**Non-Claims:** Diese Messung umfasst noch **nicht** den lokalen BUY-Build im selben Zeitintervall; separate lokale und Remote-Benchmarks werden nicht addiert. Die Rückkehr von `write_all + flush` bedeutet Übergabe an den getesteten TLS/TCP-Transportpfad, nicht bestätigte Remote-Verarbeitung, Mempool-Aufnahme, P2P-Propagation, Inclusion oder `T_buy`. Der öffentliche RPC ist außerdem noch kein qualifizierter Produktions-Inclusion-Pfad.

**Nächster Schritt:** vollständiger einteiliger Remote-E2E-Benchmark `T_pre → Decode/Quote/Build/Sign/Serialize → Hex/JSON-RPC → vorverbundener realer TLS/TCP write+flush` mit absichtlich nicht broadcastbarer Payload-Variante. Erst dieser Lauf kann die <1-ms-Hypothese für einen realen Remote-Transport in einer einzigen Messung stützen oder falsifizieren.


### G0.5 Remote E2E Prequalification — 2026-09-29

Evidence-Commit: `f75d02d` (`Add G0.5 remote E2E prequalification`).

Scope: eingefrorener G0.3-Flap-Fixture; einteilige Messung von `T_pre` bis zur realen warmen TLS/TCP-`write_all + flush`-Übergabe an `bsc-dataseed.bnbchain.org:443`. Chain-ID 56 wurde verifiziert. Der lokal korrekt gebaute/signierte BUY wurde **vor jedem Netzwerk-Write absichtlich in ungültiges RLP verfälscht**, daher keine gültige/value-bearing Transaktion ausgesendet.

10 Warmups + **100 Messsamples**.

| Messpunkt | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|
| `T_pre → T_ready local` | 199,333 µs | 297,583 µs | 396,958 µs | 405,583 µs |
| Remote TLS/TCP write+flush | 176,500 µs | 312,000 µs | 425,167 µs | 1.458,166 µs |
| **`T_pre → remote TLS/TCP handoff`** | **393,750 µs** | **569,167 µs** | **681,250 µs** | **1.852,791 µs** |
| `T_pre → JSON-RPC response` | 115,334 ms | 153,175 ms | 171,312 ms | 182,712 ms |

SHA256: `9e071cca013af7cd976efded5f608c87eb7f0843809428b18d6387eefed3318e`.

**Einordnung:** p50/p95/p99 des vollständigen gemessenen Remote-E2E-Pfads liegen unter 1 ms, der beobachtete Maximalwert jedoch nicht. Deshalb ist dies **kein finaler G0.5-PASS**. Die Tail-Latenz muss über weitere unabhängige Remote-Läufe quantifiziert werden. Zudem war der lokale Teil in diesem Remote-Aufbau deutlich langsamer als im isolierten Local-E2E-Lauf; das ist als Testaufbau-/Scheduling-Effekt zu behandeln, nicht als Änderung der Flap-Arithmetik.

**Non-Claims:** Socket-`write_all + flush` misst den warmen Client→TLS/TCP-Handoff, nicht bestätigte Remote-Mempool-Aufnahme, P2P-Propagation, Inclusion oder `T_buy`. Die JSON-RPC-Antwortzeit ist nicht `T_send`.

**Nächster Schritt:** Remote-E2E-Stabilitätslauf mit expliziter Zählung der Samples über 1 ms, damit die Tail-Häufigkeit statt nur ein einzelnes Maximum bewertet werden kann.

### G0.5 Remote E2E Stability — 10 × 100 Samples — 2026-09-29

Aufbau: derselbe eingefrorene G0.3-Flap-Fixture und derselbe Remote-E2E-Pfad wie bei der vorherigen G0.5-Remote-E2E-Prequalification. Gemessen wurde weiterhin ausschließlich:

```text
T_pre
→ Decode / CREATE2
→ Quote / minOutput
→ TX finalize / sighash
→ secp256k1 Sign
→ signed TX serialization
→ Hex / JSON-RPC
→ warme Remote-TLS/TCP write_all + flush Übergabe
```

Der lokal korrekt gebaute/signierte BUY wurde vor dem Netzwerk-Write weiterhin absichtlich in einen ungültigen Wire-Payload überführt. Es wurde **keine gültige und keine value-bearing BUY-Transaktion** gesendet.

Messumfang: **10 unabhängige Läufe × 100 Samples = 1.000 gemessene Remote-E2E-Durchläufe**.

Für `T_pre → remote TLS/TCP handoff`:

| Run | p50 | p95 | p99 | max | > 1 ms |
|---|---:|---:|---:|---:|---:|
| 01 | 460,541 µs | 544,250 µs | 675,333 µs | 684,333 µs | 0/100 |
| 02 | 399,666 µs | 551,208 µs | 858,541 µs | 950,583 µs | 0/100 |
| 03 | 398,250 µs | 547,666 µs | 592,083 µs | 720,167 µs | 0/100 |
| 04 | 388,584 µs | 524,125 µs | 820,250 µs | 1.128,833 µs | 1/100 |
| 05 | 451,167 µs | 541,208 µs | 791,083 µs | 1.405,667 µs | 1/100 |
| 06 | 426,750 µs | 610,834 µs | 816,875 µs | 1.466,167 µs | 1/100 |
| 07 | 415,750 µs | 753,042 µs | 887,125 µs | 1.018,084 µs | 1/100 |
| 08 | 399,500 µs | 570,667 µs | 1.042,916 µs | 1.411,000 µs | 3/100 |
| 09 | 435,417 µs | 584,250 µs | 948,166 µs | 994,334 µs | 0/100 |
| 10 | 407,750 µs | 538,291 µs | 587,875 µs | 598,333 µs | 0/100 |

Gesamt:

```text
Runs                         = 10
Samples                      = 1.000
Samples > 1 ms               = 7
Anteil > 1 ms                = 0,007 = 0,7 %
bester Run-p99               = 587,875 µs
schlechtester Run-p99        = 1.042,916 µs
schlechtester beobachteter Max = 1.466,167 µs
```

Lokal ausgegebener SHA256 der erzeugten Summary-Datei:

```text
2ecdc06f9018fc69b44365ece3b21df905387cba4e35bf8f268e9c51deaccd3b
gates/bsc/gate0/evidence/g05_remote_e2e_stability/summary.json
```

**Einordnung:** Die vorherige Aussage wird präzisiert. Der warme Remote-E2E-Handoff liegt im typischen Bereich klar unter 1 ms, ist aber **nicht tail-stabil vollständig unter 1 ms**: 7 von 1.000 Samples lagen über 1 ms; in einem Run lag bereits p99 mit 1.042,916 µs über 1 ms. Damit ist ein harter Anspruch „jeder Remote-Handoff < 1 ms“ durch diese Stichprobe widerlegt. Die Messung stützt weiterhin, dass der eigene lokale Build-/Sign-/Serialize-/Transport-Handoff im Regelfall sub-ms bleiben kann.

**Non-Claims / unveränderte Gate-Grenze:** Dieser Lauf misst nur den warmen Client→TLS/TCP-Handoff. Er beweist **keine** bestätigte Remote-Node-Annahme, **keine** Mempool-Aufnahme, **keine** P2P-Propagation, **keine** Builder-/Validator-Selektion, **keine** Inclusion, **keine** erfolgreiche on-chain BUY-Ausführung und **kein** `T_buy`. Deshalb bleiben:

```text
T0 → T_buy                         N/A
Node-/Mempool-Acceptance           N/A
Ordering/Inclusion                 N/A
erfolgreiche reale Token-Akquisition N/A
```

Die 1.000-Sample-Stabilitätsmessung ändert den primären Gate-0-Maßstab nicht: `T_send`/Socket-Handoff ist nur ein Zwischenwert. Der nächste wirtschaftlich relevante Fortschritt muss reale Acceptance/Ordering/Inclusion und anschließend die tatsächliche Acquisition bis `T_buy` erfassen.

**Evidence-Status:** Die oben genannten Run-Dateien, `summary.json` und der zugehörige Stability-Runner sind ab Evidence-Commit `cdd5950867c633906ce4aa5354f3f9a2cf85a2aa` versioniert. Der versionierte SHA256 von `summary.json` bleibt `2ecdc06f9018fc69b44365ece3b21df905387cba4e35bf8f268e9c51deaccd3b`.



---

## 2026-09-30 — Testnet-Funktionsnachweis vor G0.6/G0.7, ohne Gate-Vorziehung

**Phase:** unterstützende Vorqualifikation innerhalb Gate 0; **kein** Ersatz für G0.6/G0.7/G0.8 und **keine** Änderung der festgelegten Phasenreihenfolge.

**Zweck:** Nachweis, dass der bereits rekonstruierte direkte Flap-Portal-BUY auf BSC Testnet als echte signierte Transaktion erfolgreich ausgeführt werden kann und die beabsichtigten Token tatsächlich im vorgesehenen Wallet ankommen. Dieser Test diente ausschließlich als Funktions-/Acquisition-Nachweis. Der hierfür kurzfristig angelegte separate Test-Harness `gates/bsc/gate0/src/bin/g07_testnet_buy.rs` wurde anschließend wieder entfernt, damit keine parallele BSC-BUY-Architektur neben dem bestehenden Gate-0-Hotpath verbleibt.

### Eingefrorener Testnet-Scope

```text
chain_id                = 97
wallet                  = 0x9c8a0Db41914beEE52Dd49790c4F087E252743DE
portal                  = 0x5bEacaF7ABCbB3aB280e80D007FD31fcE26510e9
token                   = 0xe39b945a917a3a7E17E006896FEc1f5e4B3D7777
buy value               = 0.0001 tBNB
selector                = 0xef7ec2e7
```

### Erfolgreicher realer Testnet-BUY 1

```text
tx hash                  = 0x910a26b707b170567954641f5039419294e28c2994849a7897525b7d8fdbcf8b
receipt_status           = 1
block_number             = 133949536
transaction_index        = 19
gas_used                 = 188047
token_balance_before     = 0
token_balance_after      = 17307906101479678993718
token_acquired           = 17307906101479678993718
native_balance_after     = 99881195300000000
REAL_TESTNET_BUY_PASS     = true
```

**Acquisition-Evidence:** Receipt-Erfolg allein wurde nicht als BUY-Erfolg gewertet. Der Token-Balance-Zuwachs war positiv und entsprach exakt dem unmittelbar zuvor simulierten Output.

### Erfolgreicher realer Testnet-BUY 2

```text
tx hash                  = 0x8a27e1149141b1438d94245741a640fecddafba931e52de34316d5664a8ffc8f
receipt_status           = 1
block_number             = 133950690
transaction_index        = 18
gas_used                 = 170947
token_balance_before     = 17307906101479678993718
token_balance_after      = 34615271003919266692310
token_acquired           = 17307364902439587698592
native_balance_after     = 99764100600000000
REAL_TESTNET_BUY_PASS     = true
```

Der zweite Lauf wurde lokal zusätzlich mit Diagnose-Timestamps instrumentiert:

```text
local_build_sign_us          = 411.375
send_to_rpc_ack_ms           = 625.197
send_to_receipt_seen_ms      = 2425.614
send_to_acquisition_seen_ms  = 3060.663
```

### Gemessene aktuelle Client→RPC-Baseline während des Testnet-Laufs

20 `curl`-Proben gegen den BSC-Testnet-RPC:

```text
DNS   median =   3.674 ms   min =   2.818 ms   max =  93.629 ms
TCP   median =  80.427 ms   min =  74.645 ms   max = 257.548 ms
TLS   median = 439.558 ms   min = 432.176 ms   max = 606.289 ms
TTFB  median = 619.458 ms   min = 603.697 ms   max = 781.294 ms
TOTAL median = 621.356 ms   min = 603.837 ms   max = 781.496 ms
```

**Einordnung:** Diese kumulativen `curl`-Zeiten enthalten DNS/TCP/TLS und sind **nicht** die definierte DarkBot-Hotpath-Latenz. Der bestehende Gate-0-Vertrag verlangt vorgewärmte Verbindungen und pre-resident State; deshalb werden diese Werte nicht als `T_pre -> T_send` verwendet und nicht nachträglich von Inclusion-/Acquisition-Zeiten abgezogen.

### PASS / ADAPT / STOP

```text
Testnet direct BUY functionality     PASS
Testnet acquisition proof           PASS
Gate G0.5 final real transport      UNCHANGED / still open
Gate G0.6                            BLOCKED
Gate G0.7 Mainnet                    BLOCKED
Gate G0.8 acquisition + ordering     BLOCKED
T0 -> T_buy                          N/A
```

### Architekturentscheidung / Bereinigung

Der separate Testnet-Harness war nur ein temporärer Funktionsprüfer und gehört **nicht** zur normativen BSC-Runtime. Er wurde in Commit `2caddeb7face2428a2fc102c8e8d523bf2baab87` wieder entfernt. Für weitere Latenz- und Inclusion-Arbeit ist ausschließlich der bereits festgelegte Gate-0-/Produktionspfad maßgeblich:

```text
pre-resident state
-> earliest actionable ingress
-> verified local decode/quote/build/sign
-> warmed inclusion adapter
-> actual acquisition/order evidence
```

**Explizite Non-Claims:** Kein Mainnet-BUY; keine Birth-Latenz; kein numerischer `T0 -> T_buy`; keine Aussage über Builder-/Validator-Selektion; keine Aussage, dass der kalte `curl`-RPC-Pfad die Produktionsarchitektur repräsentiert.


---

## 2026-09-30 — G0.5 48Club Builder Stability

Transportkandidat: 48Club Puissant Builder über `eth_sendBundle` und vorgewärmte TLS/TCP-HTTP/1.1-Verbindung.

Safety: Der vollständige G0.3-BUY wurde lokal gebaut und signiert, vor dem Netzwerk-Write jedoch absichtlich in ungültiges RLP verfälscht. Es wurde kein gültiger und kein value-bearing BUY gesendet.

Messumfang: **10 unabhängige Läufe × 100 Samples = 1.000 Samples**.

`T_pre -> remote TLS/TCP handoff`:

- bester Run-p99: **658,125 µs**
- schlechtester Run-p99: **1.950,542 µs**
- schlechtester beobachteter Maximalwert: **2.808,041 µs**
- Samples > 1 ms: **22/1.000 = 2,2 %**
- Runs mit p99 < 1 ms: **5/10**
- Runs mit Max < 1 ms: **3/10**
- Builder-Antworten: **1.000/1.000 `-32000: transaction type not supported`**

Die konsistente `-32000`-Antwort bestätigt, dass der Live-Endpunkt die `eth_sendBundle`-Methode verarbeitet und den absichtlich ungültigen Transaktionstyp inhaltlich ablehnt; es liegt kein `-32601 Method not found` vor.

**Bewertung:** 48Club bleibt als Inclusion-Kandidat technisch verwendbar, erfüllt auf der aktuell gemessenen Client-/Netzwerkroute aber das harte Tail-Ziel `T_pre -> T_send < 1 ms` nicht robust. Status für diesen Kandidaten: **ADAPT**, kein finaler G0.5-PASS.

Evidence:
- lokal erzeugte Runs unter `gates/bsc/gate0/evidence/g05_48club_builder_stability/`
- versionierte Terminal-Summary: `gates/bsc/gate0/evidence/g05_48club_builder_stability/summary_terminal_capture.json`
- lokal ausgegebener SHA256 der vollständigen `summary.json`: `cc7a522bee287a765ba38148efc07a2ade5ddf6422ee5138e3fc97588e6e6944`
- Terminal-Summary-Commit: `60fb52341ddde3c6a630ab1f2a1dda8f8801fb82`

**Nächster G0.5-Schritt:** keinen 48Club-PASS erzwingen. Einen zweiten realen Inclusion-Kandidaten messen. BlockRazor dokumentiert auf BSC sowohl Bundle-Submission über `eth_sendBundle` als auch einen öffentlichen BSC-RPC unter `https://bsc.blockrazor.xyz`. Der nächste Messlauf soll denselben vollständigen G0.5-Hotpath und dieselbe Safety-Korruption verwenden, damit die Transportpfade direkt vergleichbar bleiben.

**Non-Claims:** Kein Builder-Inclusion-Nachweis, kein Ordering-Nachweis, kein `T_buy`, kein Mainnet-BUY.


---

## 2026-09-30 — G0.5 BlockRazor Bundle Stability

Transportkandidat: BlockRazor BSC RPC über `eth_sendBundle` und vorgewärmte TLS/TCP-HTTP/1.1-Verbindung.

Safety: Der vollständige G0.3-BUY wurde lokal gebaut und signiert, vor dem Netzwerk-Write jedoch absichtlich in ungültiges RLP verfälscht. Es wurde kein gültiger und kein value-bearing BUY gesendet.

Messumfang: **10 unabhängige Läufe × 100 Samples = 1.000 Samples**.

`T_pre -> remote TLS/TCP handoff`:

- bester Run-p99: **738,208 µs**
- schlechtester Run-p99: **1.534,583 µs**
- schlechtester beobachteter Maximalwert: **2.752,167 µs**
- Samples > 1 ms: **19/1.000 = 1,9 %**
- Runs mit p99 < 1 ms: **8/10**
- Runs mit Max < 1 ms: **2/10**
- Builder-Antworten: **1.000/1.000 `-32000: transaction type not supported`**

**Vergleich zu 48Club:** BlockRazor ist auf der aktuell gemessenen Client-/Netzwerkroute etwas stabiler als 48Club (1,9 % statt 2,2 % der Samples > 1 ms; schlechterster Run-p99 1.534,583 µs statt 1.950,542 µs). Dennoch erfüllt auch BlockRazor das harte Tail-Ziel `T_pre -> T_send < 1 ms` nicht robust.

**Bewertung:** BlockRazor bleibt der derzeit bessere gemessene Remote-Builder-Kandidat, Status aber ebenfalls **ADAPT**, kein finaler G0.5-PASS.

Evidence:
- lokal erzeugte Runs unter `gates/bsc/gate0/evidence/g05_blockrazor_rpc_bundle_stability/`
- versionierte Terminal-Summary: `gates/bsc/gate0/evidence/g05_blockrazor_rpc_bundle_stability/summary_terminal_capture.json`
- lokal ausgegebener SHA256 der vollständigen `summary.json`: `2b5f820a94bce0be0abfd4d590d8eb33716ecd637e922f7782a14ade7a6e1b07`

**Nächster G0.5-Schritt:** keine weiteren identischen Remote-Builder-Stability-Schleifen. Der Plan sieht als alternativen Inclusion-Pfad ausdrücklich lokalen `bsc-geth/P2P` vor. Dieser Pfad ist jetzt der nächste Kandidat für die finale Transportqualifikation. G0.6 bleibt bis zu dieser Qualifikation noch nicht offiziell eröffnet.

**Non-Claims:** Kein Builder-Inclusion-Nachweis, kein Ordering-Nachweis, kein `T_buy`, kein Mainnet-BUY.


---

## 2026-09-30 — G0.5 Local bsc-geth Preflight

Current development machine preflight:

~~~text
command -v geth       -> no result
command -v bsc        -> no result
command -v bsc-geth   -> no result
find $HOME ... geth.ipc -> no result
~~~

**Result:** no local geth-compatible BSC node or IPC socket is currently available on this machine. Therefore the local `bsc-geth/P2P` inclusion candidate cannot be measured here without first provisioning and synchronizing a node.

This is an availability result, not a latency failure:

~~~text
local bsc-geth/P2P candidate = UNAVAILABLE_ON_CURRENT_MACHINE
T_pre -> local-geth T_send   = N/A
~~~

Given the current Gate-0 objective is to reach an actual successful BUY/order measurement rather than spend time provisioning infrastructure before it is shown necessary, the available remote builder paths remain the practical G0.6 dry-qualification candidates:

- BlockRazor: preferred current remote candidate; 19/1000 samples >1 ms in G0.5 stability.
- 48Club: secondary current remote candidate; 22/1000 samples >1 ms in G0.5 stability.

This does not convert either remote path into a final G0.5 PASS and does not remove the local-geth/P2P candidate from the overall architecture. It records that the candidate is presently unavailable for measurement on the current machine.

**Next phase action:** begin G0.6 dry qualification for the available builder candidates and concurrency/nonce validity. No value-bearing transaction is permitted in G0.6.


---

## 2026-09-30 — G0.6 Concurrency / Nonce Dry Qualification

Harness: `gates/bsc/gate0/src/bin/g06_concurrency.rs`

Messumfang: **1.000 parallele Intent-Builds mit 8 Worker-Threads**, eingefrorene G0.3-Flap-BUY-Form, synthetisch getrennte Launch-Identitäten, **kein Netzwerk-Write und kein value-bearing BUY**.

Ergebnis:

~~~text
result                              PASS
unique nonces                       1000/1000
duplicate nonces                    0
accidental replacements             0
unique tokens                       1000/1000
cross-wiring                        0
overwritten BUY inputs              0
unique signed tx hashes             1000/1000
duplicate economic submissions      0
adapter projection mismatches       0
contiguous nonce lane               true
~~~

Lokale Concurrent-Build/Sign-Latenz:

~~~text
p50      80,500 µs
p95     102,584 µs
p99     273,792 µs
max    3.137,834 µs
>1 ms     5/1000 = 0,5 %
~~~

Der synthetische Allocation-/Build-/Projection-Test meldet **PASS**. Dies ist kein vollständiger G0.6-Concurrency-PASS: `duplicate_economic_submission_count` zählt lediglich doppelte erzeugte TX-Hashes; reale Zustellung, Retry-Idempotenz, Nonce-Lücken und Recipient-Isolation wurden hier nicht getestet. Der p99 bleibt deutlich unter 1 ms; einzelne Scheduler-/OS-Tails oberhalb 1 ms werden separat dokumentiert und sind kein `T_send`.

Candidate inclusion projection im Harness:
- BlockRazor `eth_sendBundle`
- 48Club `eth_sendBundle` mit `noMerge=true`

Evidence:
- `gates/bsc/gate0/evidence/g06_concurrency_dry_qualification/summary_terminal_capture.json`
- Evidence-Commit: `da31f00fca417a51f23a9f4612ec063fe4c6e83a`

**G0.6 Gesamtstatus:** **PARTIAL / synthetic allocation PASS**. Die maßgeblichen verbleibenden Anforderungen stehen in der Vertragsprüfung vom 2026-09-30 unten; fehlende DOGE-Beobachtung ist kein Field-Completeness-Blocker. Der lokale bsc-geth/P2P-Kandidat ist auf der aktuellen Maschine nicht verfügbar; QuickNode-Coverage bleibt G0.4 PARTIAL wegen fehlendem frozen-scope denominator bzw. Providerlimit. Transport-Readiness der beiden Remote-Builder ist durch G0.5 vorqualifiziert, aber nicht Inclusion-/Ordering-bewiesen.

**Non-Claims:** kein Mainnet-BUY, kein Builder-Inclusion-/Ordering-Nachweis, kein `T_buy`, kein Ingress-Coverage-PASS.


---

## 2026-09-30 — G0.6 Flap actionable pending field completeness

QuickNode delivered a real pending Flap `newTokenV6` transaction and the Rust verifier decoded the complete calldata successfully.

Observed capture:

~~~text
full pending txs                 482
hash-only pending txs              0
missing required fields            0
Flap tx hash
0xefe37cc398f69e29775e4282da099dac4174761a8e4cbbf472f2be7f04ee2fe0
pending sample SHA256
f887e6c2df0f252f9321159ceec11c9f7aa21b874298ba5baa23057bf29837de
~~~

Decoder result:

~~~text
ABI roundtrip                     PASS
tokenVersion                         6
buyTax                             100
sellTax                            100
quoteToken                         native / zero address
extensionId                        zero
exact frozen DOGE/extension scope  false
result                             PASS_DECODE_ONLY_SCOPE_MISMATCH
~~~

Interpretation: this is positive evidence that the selected QuickNode pending feed can expose a complete, directly decodable Flap `newTokenV6` transaction before canonical inclusion. This proves availability of the decoded V6 fields; the different mode values do not invalidate field completeness. It does not establish frequency or coverage of the pinned DOGE/extension scope.

Evidence:
- `gates/bsc/gate0/evidence/g06_flap_actionable_pending/summary_terminal_capture.json`
- evidence commit: `ade0d92d294028f9ad2e41b95de8e44d6c1721ee`

**G0.6 ingress status:** field completeness for the observed V6 fields is PASS. An additional exact-DOGE pending sample is not required for field completeness. Exact-scope frequency/coverage remains UNKNOWN under G0.4; it requires a canonical denominator, not an endless matching-sample wait.

Non-claims remain unchanged: no canonical coverage denominator, no inclusion/order proof, no `T0`, `T_send` or `T_buy`, no value-bearing transaction.


## 2026-09-30 — G0.6 Vertragsprüfung und vorgeschlagener Mainnet-Pilot

Diese Einordnung ersetzt frühere Aussagen, die einen weiteren Live-DOGE-Treffer für Field Completeness oder synthetische Hash-Eindeutigkeit für echte Submission-Idempotenz hielten. Keine neue Capture-Schleife, kein Microbenchmark, kein Node-Setup und kein Mainnet-Senden wurden durchgeführt.

Versionierte Matrix: `gates/bsc/gate0/evidence/g06_candidate_matrix_v1.json`. Sie bindet die verwendeten Evidence-Dateien per SHA256; die transportbezogenen Zahlen stammen aus vorhandenen Terminal-Summaries, nicht aus einem neuen Lauf.

| Kombination | V6-Felder | Konstruktion / Simulation | Nonce / Concurrency | Transport | G0.5 |
|---|---|---|---|---|---|
| QuickNode full pending × BlockRazor eth_sendBundle | PASS beobachtete vollständige Calldata | PASS historischer G0.3-Scope | 1000 Intents / 8 Worker: Allocation PASS; Lifecycle offen | Warm-TLS + Methode + Invalid-TX-Antwort bewiesen | ADAPT: 19/1000 >1 ms; schlechtester Run p99 1,534583 ms |
| QuickNode full pending × 48Club eth_sendBundle/noMerge | PASS beobachtete vollständige Calldata | PASS historischer G0.3-Scope | identischer synthetischer Test | Warm-TLS + Methode + Invalid-TX-Antwort bewiesen | ADAPT: 22/1000 >1 ms; schlechtester Run p99 1,950542 ms |

Beide bleiben Kandidaten, ohne G0.7-Sendefreigabe. Gültige Bundle-Annahme und tatsächliche Inclusion sind durch absichtlich ungültige Wire-TXs nicht bewiesen. Local bsc-geth/P2P: **UNAVAILABLE_ON_CURRENT_MACHINE / OPTIONAL CANDIDATE**, kein Gate-Fehler.

**G0.6 bleibt PARTIAL aus folgenden konkreten Gründen:**

1. `g06_concurrency.rs` erzeugt nur unterschiedliche synthetische Launch-IDs und vergibt atomar Nonces. `duplicate_economic_submission_count = duplicate_tx_hash_count` beweist keine Idempotenz bei erneutem Signal/Retry. Recipient-Isolation, Nonce-Gap-/Drop-Verhalten und geordnete Zustellung fehlen. Nächster Nachweis: gezielte Offline-Fehlerfälle im bestehenden Pfad, keine weitere Timing-Schleife.
2. Der Timer beginnt nach Entnahme eines Work-Items. Ingress-Wartezeit und Nonce-bedingtes Head-of-line-Blocking sind nicht erfasst. Build-p99 0,273792 ms und Maximum 3,137834 ms erfüllen deshalb nicht automatisch die separate Head-of-line-Anforderung des Testplans. Fünf von 1000 Samples liegen über 1 ms.
3. `g03_result.json` begrenzt PASS auf historischen Caller, 0,15 BNB und historischen Single-Segment-State. Der neue Sender, ein kleiner Live-Betrag und frischer State samt Invalidation sind noch nicht freigegeben. Der bestehende Rust-Pfad muss dafür gezielt validiert werden; historische Mindestoutputs dürfen nicht übernommen werden.

Field Completeness ist davon getrennt: DOGE ist ein Feldwert derselben vollständigen V6-Calldata. Der korrigierte Lauf mit 18.148 Full-Pending-Objekten, einem V6-Launch und keinem DOGE-Treffer beweist weder fehlende Felder noch einen Venue-FAIL. G0.4-Coverage bleibt mangels kanonischem Nenner UNKNOWN/PARTIAL.

### G0.1 Testvertrag v1 — Vorschlag, noch NICHT autorisiert

Maschinenlesbar: `gates/bsc/gate0/evidence/g01_mainnet_acceptance_proposal_v1.json`.

- Scope: eingefrorener Flap V6 / TAXED_V3 / DOGE / Extension, direkter nativer Portal-BUY. Runtime- und State-Bindung bleiben Pflicht.
- Maximal **4 Versuche**, zwei je Builder, abwechselnd auf verschiedenen geeigneten Launches; höchstens ein wirtschaftlicher Intent pro Launch. Keine automatischen Wiederholungen oder parallelen Käufe.
- Maximal **0,001 BNB BUY-Wert je Versuch**, zusätzlich höchstens **0,0005 BNB Gas + Builder-Kosten je Versuch**. Absolutes Gesamtlimit **0,006 BNB**, ohne Verrechnung etwaiger Verkaufserlöse. Der kleine Betrag benötigt einen eigenen Replay-Nachweis im vorhandenen Pfad.
- Vorgeschlagene Slippage-Grenze 100 bps mit unabhängig berechnetem, positivem minOutput. Kein historischer Zahlenwert als Live-Quote.
- 30-Minuten-Beobachtungsfenster nach separater Freigabe. Alle ausgewählten Versuche bleiben im Nenner, auch Skip, Deadline-Miss, Revert, Drop oder UNKNOWN. Alle kanonischen scope-konformen Launches bilden separat den Coverage-Nenner; kein Launch bzw. fehlende Daten bedeutet UNKNOWN.
- Pilot-PASS je Kandidat: 2/2 belegte Token-Akquisitionen, im Launch-Block nach Launch, **null erfolgreiche externe BUYs dazwischen**, null Reverts/Drops/late/UNKNOWN; vollständige kanonische Coverage und null verpasste geeignete Pending-Launches. Creator-/Launch-internen BUY separat zählen. Vier Versuche beweisen keine Produktionszuverlässigkeit.
- SUCCESSFUL_BUY verlangt korrekten Token und Empfänger, positive zuordenbare Balance-Änderung, tatsächlichen Debit und Kosten, unsere TX, kanonische Position, konkurrierende externe BUYs und Preisvergleich. Receipt-Erfolg allein genügt nicht.
- ADAPT bei unzureichender Stichprobe oder verfehlten Timing-/Ordering-/Coverage-Kriterien. STOP bei Sicherheits-/Budget-/State-Verletzung, Providerlücke im scharfgeschalteten Zustand, Nonce-Konflikt, erstem Revert oder ungeklärter gesendeter TX. Kein automatischer Ersatz-/Cancel-/Sell-Trade.
- Vor Sendefreigabe: echte G0.6-Lücken schließen, Sender/Empfänger, frischen State, kleinen Betrag sowie Bundle-Gültigkeit/Expiry und Deadline-Policy trocken validieren und binden. Anschließend Stephans ausdrückliche Freigabe für Vertragsversion und Gesamtbetrag einholen. Aktuell **approved=false**.

**Kein echter BUY in dieser Prüfung. Keine neue T_buy-Zahl.** Ohne vertrauenswürdigen Intra-Block-Zeitstempel liefern spätere G0.7/G0.8-Versuche Acquisition + Ordering; Blockabstände werden nicht als BUY-Latenz ausgegeben.


## 2026-09-30 — Offline-Signaturregression und Concurrency-Gegenbeispiele

Die gezielte Offline-Prüfung fand einen echten Envelope-Fehler: fünf bestehende G0.5/G0.6-Serializer kodierten ECDSA `r`/`s` als feste 32-Byte-Strings. Führende Nullbytes sind bei RLP-Integern unzulässig ([Ethereum RLP-Spezifikation](https://ethereum.org/developers/docs/data-structures-and-encoding/rlp/)). Korrektur: bestehendes `rlp_u256` auf den jeweiligen Signatur-Integer anwenden. Kein neuer BUY-Pfad, keine Änderung an Quote oder Solana.

Reproduzierbare Befehle, jeweils vom Repo-Root:

```sh
cargo test --offline --manifest-path gates/bsc/gate0/Cargo.toml --bin g06_concurrency -- --nocapture
cargo test --offline --manifest-path gates/bsc/gate0/Cargo.toml --bins -- --nocapture
```

Vor der Korrektur: 2 Tests bestanden, Signaturtest fehlgeschlagen bei Test-Key-Byte 34, Nonce 206, Feld `s` mit führendem Nullbyte. Danach: insgesamt 6 Tests bestanden, 0 fehlgeschlagen. Der G0.6-Test dekodiert 1.024 echte Offline-Signaturen unabhängig mit alloy-rlp, prüft Integer-Kanonizität, Low-s, Ziel/Calldata und rekonstruiert den korrekten Absender für zwei nicht finanzierte Testschlüssel; zehn Signaturwerte benötigen weniger als 32 Bytes. Die vier anderen korrigierten Serializer sind mitkompiliert, haben aber noch keinen eigenen Envelope-Integrationstest.

Zwei erfolgreiche **Diagnosetests belegen Grenzen**, nicht bestandene Sicherheitsanforderungen:

- Identische BUY-Calldata mit Nonces 7 und 8 ergibt unterschiedliche Hashes: Hash-Eindeutigkeit verhindert keinen doppelten wirtschaftlichen BUY.
- Kontrollierte Thread-Reihenfolge ohne Sleep ergibt Completion `[8, 7]` trotz atomarer eindeutiger Nonces. Allocation beweist weder geordneten Versand noch Verhalten bei einer Nonce-Lücke.

Raw-Testausgaben und maschinenlesbares Ergebnis liegen unter `gates/bsc/gate0/evidence/g06_offline_regression/`. Historische Transportmessungen bleiben historische Evidence der damaligen Binärdateien; der Fix ist kein neuer Latenznachweis. **G0.6 bleibt PARTIAL**: Submission-Idempotenz/Nonce-Lifecycle, Head-of-line und Live-Sender/-Betrag/-State noch offen. Kein Mainnet-BUY, kein RPC-Capture, kein T_buy-Messwert.


## 2026-09-30 — Begrenzte Offline-Intent-/Nonce-Spur

Der vorhandene G0.6-Harness verwendet jetzt `src/intent_lane.rs` für die Admission und Nonce-Vergabe. Die BUY-/Signierimplementierung bleibt erhalten. Eine Sitzung bindet Chain und Sender, merkt jeden Token als wirtschaftliche Kaufabsicht und behält diese Sperre auch nach Bestätigung. Doppelte Signale verbrauchen keine weitere Nonce. Ein Prepared-Hash ist unveränderlich; ein Send-Permit wird vor dem gedachten I/O verbraucht. Unbekannter Sendeausgang, Revert/Timeout oder ungültige Bestätigung führen über Halt zu keinem weiteren Permit und keiner automatischen Nonce-Wiederverwendung. Tickets fremder Sender/Chains werden abgelehnt.

**Verifikation:** `cargo test --offline --manifest-path gates/bsc/gate0/Cargo.toml --lib --bins -- --nocapture` — **14 Tests bestanden, 0 fehlgeschlagen**. Sieben Lane-Tests einschließlich 8 Worker × 1.000 wiederholte Signale → genau 1.000 Intents/Permits; ein zusätzlicher Integrationstest prüft die tatsächlichen bestehenden G0.6-Signaturen/Calldata bei umgekehrter Build-Reihenfolge. Die bisherigen Signatur- und G0.3-Tests bleiben grün. Dies ist kein neuer Microbenchmark.

Belege: `gates/bsc/gate0/evidence/g06_intent_lane/result.json` und `test_output.txt`. Das Harness-Ergebnis heißt nun ausdrücklich `PASS_SYNTHETIC_DRY_ONLY`; irreführende Submission-/Replacement-Zähler sind durch Dry-Permits und `actual_submission_lifecycle=NOT_TESTED` ersetzt.

**Grenzen / nächste notwendige Arbeit:** Die Spur ist nur in-memory und nur in den Dry-Harness integriert. Sie wartet konservativ auf die Bestätigung der Vorgänger-Nonce; im Test wird diese simuliert. Das ist keine Lösung für das Head-of-line-Latenzziel. ACKs dürfen später nicht als kanonische Bestätigung gelten. Live-Transport-Anbindung, Wiederanlauf/Reconciliation, Kopfblockierung sowie Live-Sender/-Betrag/-State bleiben unqualifiziert. **G0.6 bleibt PARTIAL**, beide Builder bleiben Kandidaten. Keine value-bearing TX und kein neuer BUY-Latenzwert. Der vorgeschlagene Mainnet-Vertrag bleibt nicht autorisiert.


## 2026-10-01 — Builder-Grenze, Head-of-line, Recovery und Empfängerbindung

**G0.6 bleibt PARTIAL. Der injizierte HoL-Test beweist Kopplung, keine reale Verletzung des <1-ms-Ziels.** Single-Sender bleibt; aus diesem Test folgt keine notwendige Architekturänderung.

### Bestehende Builder-Anbindung, lokal geprüft

`g05_remote_e2e::run_once` verwendet jetzt die bestehende IntentLane unmittelbar vor `write_all/flush`: erst `begin_send()` konsumieren, Ticket/Hash vergleichen, dann erster Write. Fehler nach Teil-Write oder Flush führen zu HALT; kein erneuter Write. Der bestehende Aufbau von Flap-BUY, Signatur, BlockRazor-/48Club-Request bleibt erhalten. Der Stream ist für Tests abstrahiert; im Test werden nur Speicher-Streams verwendet. Die absichtliche Wire-Korruption des bisherigen Diagnosepfads bleibt aktiv.

Vier Adaptertests decken beide Builder-Formate (`noMerge=true` für 48Club), HTTP 200 mit RPC-Fehler bzw. positiver ACK-Antwort, Teil-Write/Flush-Fehler, falschen Hash und falschen Sender ab. ACK/HTTP-Erfolg ruft niemals `confirm()` auf. Positive ACKs auf das bewusst ungültige Payload lösen erwartungsgemäß den abgefangenen Safety-Panic des bisherigen Probes aus; der Nachfolger bleibt gesperrt. **Ein kanonischer Reconciler ist noch nicht implementiert.** Der alte CLI-Diagnosemodus verwendet je ungültiger Probe eine frische Dry-Lane; das ist kein Live-/Restart-Modell und wurde hier nicht ausgeführt.

### Gemessene HoL-Szenarien

Nonce N wird `Sending`, acht nachfolgende Intents sind vollständig vorbereitet. Gemessen wird ab deren jeweiligem `T_actionable`, nicht ab TX-Build. Der Test hält N absichtlich für 25 ms ungeklärt. Die tatsächlich beobachtete Wartezeit inklusive Scheduler beträgt:

| Ende des Szenarios | Blockierte Nachfolger | T_actionable → erste blockierte Freigabe | T_actionable → HALT / simulierte Resolution |
|---|---:|---:|---:|
| HALT | 8 | 125–1167 ns | 30.029750–30.048667 ms |
| SIMULATED_RESOLUTION | 8 | 83–4417 ns | 30.029000–30.043708 ms |

Dies sind zwei gezielte lokale Fehlerszenarien, **keine Netzwerk-/BUY-Latenzen und keine Perzentilschätzung**. Ohne HALT oder separate Resolution gibt es in der Spur keinen zeitlichen Freigabemechanismus; die Blockierung ist strukturell unbeschränkt. Ein schneller `NotReady`-Rückgabewert verkürzt die Wartezeit des BUY nicht. HALT beendet nur den Versuch und macht aus dem blockierten Kauf keinen erfolgreichen Entry.

**Einordnung korrigiert:** Die 25 ms wurden künstlich injiziert. Reale HoL-Latenz ist NOT MEASURED, reale sub-1-ms-Verletzung NOT PROVEN. Der Test verlangt keinen Multi-Sender-Umbau. Single-Sender bleibt gemäß Projektentscheidung. Das ist ebenso wenig ein neuer Latenz-PASS; Build/Sign, Transport und tatsächliche Acquisition bleiben getrennte Nachweise.

### Restart: reproduzierter Blocker und minimaler Vertrag

Für **Reserved, Prepared, Sending und Confirmed** wird der Prozesszustand verworfen und die Lane mit bewusst veralteter Start-Nonce rekonstruiert. In allen vier Fällen kann derselbe Token erneut reserviert und ein Permit erzeugt werden. Die positiven Diagnosetests beweisen diese Lücke, keinen Recovery-PASS.

Minimaler Recovery-Vertrag, **Design, noch nicht implementiert**:

1. Vor Scharfschaltung exklusiven Chain-/Sender-Owner, persistente Session-Epoch, kanonischen Checkpoint und begrenzten Nonce-Bereich dauerhaft festhalten. Kein synchroner DB-/Filesystem-Write im späteren Hotpath. Ist die Detailpersistenz beim Crash unvollständig, wird konservativ der gesamte geleaste Bereich quarantänisiert.
2. Start grundsätzlich DISARMED/QUARANTINED. Persistenz muss Chain, Sender, Epoch, Nonce-Bereich, wirtschaftliche Intent-ID, Token, Empfänger, bekannte TX-Hashes sowie Bestätigungsblock binden. Keine Freigabe aus einer ungeprüft übernommenen `first_nonce`.
3. Reserved/Prepared nach Crash nicht blind als ungesendet behandeln; Sending niemals automatisch erneut senden; Confirmed nur nach kanonischer Revalidierung und wiederhergestellter Deduplication übernehmen.
4. Separater Reconciler prüft kanonische Nonce-/TX-/Receipt-/Block-Evidence und Reorgs. ACK, Pending-Nonce, fehlendes Receipt oder abgelaufenes Builder-Fenster allein beweisen nicht, dass eine signierte TX später nicht mehr ausgeführt werden kann.
5. Unklare Nonces nicht automatisch freigeben, ersetzen oder canceln. Ohne kanonischen Verbrauch oder ausdrücklich geprüften Recovery-Nachweis bleibt die Spur gesperrt. Vor Implementierungsfreigabe Crash-Punkte rund um Persistenz/Write, Reorg, veraltete Epoch und konkurrierenden Prozess testen.

### Empfängerbindung und Gesamtergebnis

256 finale signierte Envelopes über acht parallele Sender-Spuren geprüft: Ticket → Chain/Nonce/Token → Portal-Ziel und dekodierte BUY-Calldata → kryptographisch rekonstruierter Absender. Die direkte Portal-ABI enthält kein separates Recipient-Feld; die Zuordnung Empfänger = direkter Caller gilt unter der bereits eingefrorenen G0.3-Semantik. Dies ist kein neuer Beweis für beliebige zukünftige Implementierungen oder Live-State.

Befehl: `cargo test --offline --manifest-path gates/bsc/gate0/Cargo.toml --lib --bins -- --nocapture`.
**21 Tests bestanden, 0 fehlgeschlagen.** Raw-Ausgabe und strukturierte Ergebnisse: `gates/bsc/gate0/evidence/g06_submission_hol_recovery/`. Die Matrix verweist darauf. Kein Mainnet-BUY, kein Netzwerk-Capture, kein neuer G0.5-Microbenchmark. Offen bleiben die kanonische Reconciliation sowie die bereits benannte Live-Sender/-Betrag/-State-Qualifikation. Die nachstehende Startsperre ergänzt jetzt die fail-closed Recovery; kein Multi-Sender-Umbau wird verlangt.


## 2026-10-01 — Korrigierte HoL-Einordnung und persistente Single-Sender-Startsperre

Verbindliche Einordnung: **HoL coupling PROVEN; 25-ms scenario ARTIFICIALLY INJECTED; real HoL latency NOT MEASURED; real sub-1-ms violation NOT PROVEN; architecture change required NO aus diesem Test.** Single-Sender bleibt. Die ursprünglichen Rohwerte bleiben erhalten; ihre zu weit gehende FAIL-Interpretation wurde korrigiert. Matrix `reviewed_head` ist auf den tatsächlich geprüften Ausgangscommit aktualisiert; `new_measurement_performed=true` bezeichnet ausdrücklich die lokale injizierte Messung, keine Netzwerk-/BUY-Messung.

`restart_fence::FencedLane` umschließt jetzt die bestehende IntentLane. Vor der ersten Admission wird im stabilen Registry-Verzeichnis eine eindeutige Chain-/Sender-Fence mit `create_new` angelegt, geschrieben und per Datei-/Directory-Sync dauerhaft gemacht. Existiert die Fence bereits — auch leer oder beschädigt — liefert Arm ausschließlich `Quarantined`, keine Lane. Schreib-/Sync-Fehler geben keine Lane zurück und entfernen keine eventuell entstandene Fence. Während Reserve/Prepare/BeginSend/Confirm erfolgt kein Dateizugriff. Die Fence bleibt nach Drop und auch nach Confirm bestehen.

**Recovery-Scope:** fail-closed Wiederanlauf ist implementiert; automatische Wiederaufnahme ist es nicht. Alle unbekannten Session-Zustände bleiben quarantänisiert. Registry-Pfad über Starts stabil halten; Fence weder löschen noch durch einen neuen Pfad umgehen. Ein kanonischer Reconciler und kontrollierte Wiederfreigabe fehlen weiterhin. Die vorhandenen isolierten Invalid-Wire-Diagnoseprobes sind keine G0.7-Starts; ein künftiger werttragender Lauf muss zwingend über FencedLane armiert werden. Kein Multi-Lane-Code angelegt.

**26 Tests bestanden, 0 fehlgeschlagen**, mit demselben Offline-Cargo-Befehl. Neu geprüft: Wiederanlauf aus allen vier Zuständen, zweiter Owner, leere/kaputte Fence, Filesystem-Fehler, bestehende Dedup-/HALT-Regeln und die bestehende Builder-Grenze mit lokalem Memory-Stream unter persistenter Fence. Die Tests simulieren Zustandsverlust per Drop/Reopen; keine Behauptung einer Hardware-Power-Loss-Zertifizierung.

Evidence: `gates/bsc/gate0/evidence/g06_single_sender_restart/`. G0.6 bleibt PARTIAL; die Startsperre ist kein Live-Scope-PASS. Nächste fachliche Qualifikation bleibt der bestehende Direct-Portal-Pfad für den vorgeschlagenen 0,001-BNB-Betrag und tatsächlichen Sender/State sowie die kontrollierte kanonische Reconciliation. Kein Mainnet-BUY, kein neues Latenzversprechen; der Mainnet-Vertrag bleibt nicht autorisiert.


## 2026-10-01 — G0.7 controlled Testnet: standalone BSC P2P ingress vs public WSS control

Controlled Flap Testnet launch+BUY completed successfully with the new standalone BSC P2P sensor. This run is a **measurement of the current adapter only**, not a Gate-0 PASS for P2P ingress.

Scope:
- chain id 97;
- Flap Testnet Portal `0x5bEacaF7ABCbB3aB280e80D007FD31fcE26510e9`;
- launch tx `0xe17635e85e1ab47e8bdf980928190ba3dfae7084cd3275345b41bc8bd74b7346`;
- BUY tx `0x469ec76cab25a0c2eb4a729dd6a12e7967bf0ae1794068ef76503256647129b8`;
- token `0xFAD8ce8C891130D7838f8F987688E2D953f07777`;
- P2P readiness threshold 1 peer; 2 peers connected when the launch signal was emitted;
- public WSS retained only as control/evidence and submission transport.

Correctness / acquisition:
- launch receipt status = 1;
- BUY receipt status = 1;
- acquired token balance = 17669034223232267628301;
- launch block = 134253906, tx index = 3;
- BUY block = 134253906, tx index = 4;
- same block = true;
- successful external BUYs between launch and ours = 0.

Local reaction path from the P2P adapter's actionable signal:
- T_signal -> decode = 91,417 ns;
- decode -> build = 8,792 ns;
- build -> sign = 109,958 ns;
- sign -> ready = 7,042 ns;
- ready -> queue = 5,833 ns;
- queue -> send = 210,083 ns;
- **T_signal -> T_ready = 217,209 ns**;
- **T_signal -> T_send = 433,125 ns**;
- T_signal -> submission ACK = 96,268,542 ns.

Ingress/control timing:
- launch handoff -> current P2P signal = **230,536,750 ns**;
- WSS_minus_P2P_signal = **-130,313,708 ns**, i.e. the control WSS observed this same launch about 130.314 ms before the current P2P signal;
- derived launch handoff -> WSS control signal for this run is about **100.223 ms**.

Interpretation:
- The Rust reaction path remains comfortably below the local 1-ms target.
- The current P2P adapter is **not qualified** as a race ingress: its present `T_signal` is taken only after a complete transaction has entered the in-memory Reth transaction pool. Hash-only gossip can therefore incur a `GetPooledTransactions` round trip and pool/import work before the timestamp.
- This run had only two connected P2P peers at signal time, so it does not prove that direct BSC P2P is intrinsically slower than public WSS. It proves that **this pool-admission measurement boundary/topology is too late**.
- Next qualification must separate (a) first matching hash announcement, (b) first full `Transactions` message received directly from the ETH/RLPx network channel before pool validation/import, and (c) pool/fetch fallback. Only a complete full transaction is actionable for the real launch decoder.
- `T_buy-T0` remains unavailable from standard BSC block data; same-block adjacent ordering with zero successful competitors is hard ordering evidence only.

No Mainnet value was used. No claim of sub-ms on-chain execution is made.


---

## 2026-10-01 — Zentraler Gate-0-Status: realer BUY, echte gemessene Zeit und nächster Pfad

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



## 2026-10-01 — G0.7 Real Testnet BUY Prequalification — independent HTTPS launch + P2P ingress + prewarmed HTTPS BUY

Controlled BSC Testnet Flap birth->BUY completed successfully after decoupling the value-bearing path from PublicNode WSS.

Scope:
- chain id: 97;
- launch tx: `0x39b8f3853b595a23a2937870ce699d8b437ba806dcdd4d8644b381eda7fc3535`;
- BUY tx: `0xb334938a4c36324ea87e8484eb4c2076a1d2f290d1699b35c797390eeae31a9f`;
- token: `0xf9ffE5b7368D65FE184e5DA26dd284E0Da887777`;
- launch transport: independent prewarmed HTTPS to `bsc-testnet-dataseed.bnbchain.org:443`;
- race ingress: raw BSC ETH P2P channel before txpool;
- BUY transport: separate prewarmed HTTPS `write_all+flush`;
- WSS control: disabled for this BUY proof;
- signal source: `BSC_P2P_HASH_FETCH`.

Functional / economic result:
- launch receipt status = 1;
- BUY receipt status = 1;
- acquired token balance = 17669034223232267628301;
- native debit = 499300700000000 wei;
- launch block = 134315024, tx index = 5;
- BUY block = 134315025, tx index = 1;
- same block = false;
- successful external BUYs between launch and DarkBot = 0.

Therefore the BUY was real and economically successful. DarkBot was the first observed successful external BUY after the launch in canonical ordering for this controlled trial, but it did **not** achieve same-block adjacency.

Measured ingress:
- launch handoff -> first P2P hash = **182,209,833 ns = 182.209833 ms**;
- P2P hash -> complete transaction = **87,510,417 ns = 87.510417 ms**;
- launch handoff -> actionable full transaction = **269,720,250 ns = 269.720250 ms**.

Measured local BUY path:
- T_signal -> decode = 139,500 ns;
- decode -> build = 10,083 ns;
- build -> sign = 218,542 ns;
- sign -> ready = 10,375 ns;
- ready -> queue = 10,458 ns;
- queue -> send = 228,709 ns;
- **T_signal -> T_ready = 378,500 ns = 0.378500 ms**;
- **T_signal -> T_send = 617,667 ns = 0.617667 ms**.

Observation:
- T_signal -> receipts seen = 6,416,029,875 ns = 6.416029875 s.
- This is observation latency, **not** BUY execution latency.
- `T_buy - T0` remains `UNKNOWN_FROM_STANDARD_BSC_BLOCK_DATA`; ordinary BSC block/receipt data does not expose a trustworthy sub-ms execution timestamp for each transaction.

Interpretation:
1. The local BUY hotpath remains below the 1-ms target.
2. The dominant bottleneck is still ingress. Roughly 269.7 ms elapsed from launch handoff until the complete actionable launch transaction reached DarkBot.
3. Of that ingress delay, ~182.2 ms occurred before the first matching P2P hash and another ~87.5 ms in hash -> full transaction retrieval.
4. Because actionable ingress arrived this late, the BUY landed in the next block rather than directly behind the launch in the same block.
5. Zero successful competing BUYs happened in between, so this run still achieved first observed successful external BUY ordering, but it does not qualify the current P2P ingress as the technical maximum.
6. Further local Rust build/sign optimization has low leverage relative to the ~270-ms ingress deficit. The next Gate-0 work belongs in G0.6/G0.9-style ingress x inclusion qualification: earlier transaction feeds, ordered/backrun builder paths, and ultimately validator/sentry-near or own-builder infrastructure if required.

Status for this exact run:
- actual BUY/acquisition: PASS;
- local T_signal -> T_send <1 ms: PASS;
- same-block adjacency: FAIL for this run;
- zero successful competitors between: PASS;
- current public/ordinary P2P ingress as technical maximum: NOT QUALIFIED;
- exact `T_buy - T0`: NOT MEASURABLE from standard BSC block data.


## 2026-10-01 — G0.6 Ingress × Inclusion Prequalification — current bottleneck and research brief

### Problem statement

The current controlled Flap Testnet path proves that the Rust BUY hotpath is **not** the dominant bottleneck.

Latest successful run:
- actual BUY/acquisition: PASS;
- T_signal -> T_ready = 0.378500 ms;
- T_signal -> T_send = 0.617667 ms;
- launch handoff -> first P2P hash = 182.209833 ms;
- P2P hash -> full actionable launch transaction = 87.510417 ms;
- launch handoff -> actionable full launch transaction = 269.720250 ms;
- launch block 134315024 index 5;
- BUY block 134315025 index 1;
- successful external BUYs between launch and DarkBot = 0.

Conclusion: further shaving tens or hundreds of microseconds from Rust does not solve the product problem while ingress is roughly 270 ms late. Current ordinary BSC P2P/hash-fetch ingress is therefore **not qualified as the technical maximum**.

### Research question

Find the technically fastest practical BSC path for:

`earliest actionable launch evidence -> DarkBot BUY -> ordered inclusion -> actual first external BUY`

The target remains `T_buy - T0 < 1 ms`. If post-T0 internet reaction cannot physically satisfy this target, prioritize architectures that move work before T0:

`T_pre < T0 -> BUY already prepared -> ordered/backrun builder -> [launch][DarkBot BUY]`.

### Mandatory candidate classes

Research and compare, with current documentation/code/evidence:
1. direct custom BSC devp2p / network-only Reth relays with many strategically placed peers;
2. own full bsc-geth / Reth nodes with early pre-txpool hooks;
3. multi-region sensors and FIRST ACTIONABLE WINS;
4. BlockRazor Public Mempool gRPC / Private Mempool / Fast Submit / Builder;
5. 48Club Auction Feed + `backrunTarget` / Puissant ordered bundle;
6. bloXroute BDN `newTxs` / local Gateway / gRPC / `blxr_tx`;
7. current BSC PBS Builder -> Sentry -> Validator path;
8. own builder based on BNB Chain's open-source `bsc-builder`;
9. direct validator/sentry-near deployment where protocol/economic access allows it;
10. kernel/NIC specialization only after network topology/ingress is proven no longer dominant.

### Known external evidence to verify

- BSC PBS architecture explicitly supports searchers sending bundles to builders, which bid to the in-turn validator through Sentry.
- BSC ordinary tx propagation sends full transactions only to a subset of peers and hash-announces to others; therefore hash-fetch latency is structurally expected on ordinary peers.
- BSC EVN directly connects validator/sentry peers but disables transaction broadcast between EVN peers, so EVN membership is not automatically an early public-mempool feed.
- 48Club Auction Feed can prepend a `backrunTarget` ahead of the submitted bundle and currently filters a Four.meme Create topic.
- BlockRazor publishes a BSC Public Mempool gRPC product and Fast Submit path; provider benchmarks claim material lead over regular nodes and ~20-50 ms submission-path reductions depending on geography.
- bloXroute exposes BSC BDN transaction streams and Gateway APIs; its own docs recommend WSS/Gateway for latency-sensitive submission.
- Merkle/Blink publicly described a network-only Reth relay architecture with many replicas/brokers and geographic distribution for BSC mempool latency; this architecture is directly relevant to a no-subscription alternative.

### Required output of the next research/design step

Do not merely suggest providers. Produce:
- a ranked-by-physics architecture ladder from cheapest/free to absolute technical maximum;
- exact timing boundary for each candidate;
- which parts can run before T0;
- whether full signed launch calldata is available without synchronous RPC;
- whether exact ordered `[launch][BUY]` inclusion can be expressed;
- expected unavoidable network hops and geographic dependencies;
- current cost/access requirements;
- open-source code/repositories worth reusing;
- the smallest benchmark harness needed to compare all candidates under the same controlled launch;
- explicit STOP criteria for ordinary P2P if it cannot approach the required lead.

No provider is declared fastest from marketing claims alone. Gate 0 must measure complete ingress x inclusion paths under comparable conditions.


## 2026-10-02 — G0.1 v2 self-launch authorization contract and Dry-PASS guard

A conflict in the previous acceptance evidence was resolved explicitly:

- `g01_mainnet_acceptance_proposal_v1.json` historically contains `authorization.approved=true`, dated 2026-10-01, while also containing `execution_ready=false`.
- The human-readable ledger had described the state as not approved for execution.
- Interpretation is now frozen: v1 represented a historical budget-only approval proposal/state and is **not execution authority**.
- v1 is retained for audit history but marked `SUPERSEDED_NOT_EXECUTION_AUTHORITY` and points to v2.
- All future controlled Mainnet self-launch execution authority must come exclusively from `g01_mainnet_acceptance_proposal_v2.json`.

Current v2 state remains:

```text
approved = false
execution_ready = false
```

No value-bearing Mainnet bundle may be sent while either remains false.

### Runtime send guard requirements

The G0.6a harness must implement a fail-closed guard before any network write of valid Mainnet transaction or bundle bytes. It must verify at minimum:

1. exact v2 contract file path;
2. SHA-256 of exact v2 bytes equals the frozen/armed hash;
3. schema version 2;
4. `approved=true`;
5. `execution_ready=true`;
6. recovered launcher address equals the authorized launcher address;
7. recovered buyer address equals the authorized buyer address;
8. launcher and buyer are distinct;
9. approved contract hash field equals the frozen hash;
10. consumed budget + worst-case reserved attempt cost stays within the approved absolute budget;
11. attempt count stays within the approved maximum;
12. every required Dry-PASS predicate is true.

v1, environment variables and CLI flags are explicitly forbidden as standalone execution authority.

### Dry-PASS definition before authorization

All four criteria are mandatory:

**DRY-1 — bytes and signers**
- both builder payload constructors contain byte-identical raw launch and BUY transactions;
- order is `[raw_signed_launch_A, raw_signed_buy_B]`;
- independent decoding/signature recovery yields distinct wallet A and wallet B;
- ChainID, nonces, Portal, value, gas and calldata all match the frozen scope.

**DRY-2 — independent local simulation**
- local/offline sequential execution of launch A followed by BUY B succeeds;
- predicted token address matches;
- exact received token amount matches the local economics calculation;
- signed `minOutput > 0` and is <= independently simulated received amount;
- historical DOGE fixture values are not reused as authority for the native self-launch mode.

**DRY-3 — gas floor and budget**
- current acceptance/ranking floor for every armed builder is independently verified before signing;
- identical signed bytes satisfy the strictest verified builder requirement;
- independently simulated launch+BUY gas multiplied by the signed gas price, plus launch value, BUY value and permitted builder costs, fits the per-attempt and absolute approved budget.

**DRY-4 — expiry, nonces and zero egress**
- expiry/max-block fields stay inside the approved tight window;
- launcher and buyer pending nonces match the signed transactions;
- before authorization, zero bytes of the valid signed launch, valid signed BUY or valid bundle payload may leave the machine.

Provider API/method/expiry prequalification before authorization may therefore use only deliberately non-executable fixtures. Exact valid Mainnet transaction/bundle bytes are not sent to remote simulation endpoints before authorization.

The provisional 0.05 BNB total ceiling remains only a proposal. It must be recalculated from verified launch costs, simulated gas and the strictest current builder floor before approval and should be reduced if the evidence permits.


## 2026-10-02 — G0.6a two-wallet self-launch bundle dry harness implemented

Implemented components:

- `bsc/gates/gate0/src/acceptance_guard.rs`
  - v2-only fail-closed authorization reader;
  - exact v2 path check;
  - SHA-256 check;
  - `approved` + `execution_ready`;
  - frozen launcher/buyer address match;
  - distinct-wallet requirement;
  - attempt counter and total-budget check;
  - explicit `FROZEN_APPROVED_V2_SHA256 = "UNARMED"` source lock. No environment variable or CLI flag can arm Mainnet execution.

- `bsc/gates/gate0/src/bin/g06a_self_launch_bundle.rs`
  - ChainID 56 transaction construction;
  - distinct launcher-A / buyer-B keys;
  - deterministic Flap CREATE2 token prediction from a separately supplied, reverified TAX_V3 implementation;
  - native-quote `newTokenV6` launch calldata;
  - nonzero BUY `minOutput` from the local expected-output input;
  - final signed legacy-RLP reconstruction and independent signature recovery from the resulting RLP bytes;
  - byte-identical ordered `[raw_launch_A, raw_buy_B]` projection into both BlockRazor and 48Club builder envelopes;
  - tight `send_head..=send_head+2` expiry check;
  - worst-case gas/value reservation check;
  - no networking imports or network-write path; valid Mainnet transaction/bundle bytes remain in RAM and are not printed.

The binary is deliberately two-phase:

1. without a local simulation evidence file it performs DRY-1 construction/recovery and DRY-4 structural zero-egress checks, prints only hashes/metadata, then exits nonzero because DRY-2/3 are not yet independently proven;
2. `--verify-evidence <local_simulation.json>` additionally requires exact transaction-hash/token/output/gas/floor/nonce evidence from an independent local simulation source before DRY-2/3/4 can all pass.

This avoids claiming an "independent" simulation by simply re-running the same Rust formula.

Current live state remains:

```text
FROZEN_APPROVED_V2_SHA256 = UNARMED
v2 approved = false
v2 execution_ready = false
```

Therefore this implementation cannot perform a value-bearing Mainnet submission.


## 2026-10-02 — Verbindlicher Zielpfad nach Sniper-/Builder-Vergleich

Die aktuelle externe Vergleichsanalyse bestätigt die bisherige Gate-0-Richtung: Der lokale Rust-Hotpath ist mit den bereits gemessenen ~0,43–0,62 ms nicht der dominante Engpass. Entscheidend für die erste extern erreichbare Position sind frühe Sicht auf den Launch, der Netzweg zum Builder, die Builder-Abdeckung und das Ordering/Gebot.

### Verbindlicher Architekturpfad

**A. Öffentliche Launch-TX**

```text
mehrere Ingress-Quellen parallel
  - eigener Mainnet-Reth/BSC-P2P-Sensor
  - BlockRazor Public Mempool / weitere geprüfte Full-TX-Feeds
        ↓
FIRST FULL ACTIONABLE WINS
        ↓
Launch vollständig lokal dekodieren
Token / BUY-Felder lokal bestimmen
        ↓
bestehender Rust BUY-Core
        ↓
ordered bundle:
[raw signed Launch TX]
[raw signed DarkBot BUY]
        ↓
parallel:
48Club/Puissant
BlockRazor
        ↓
Builder
        ↓
Validator
```

Für fremde öffentliche Launches bleibt die Ingress-Latenz zwingend relevant: DarkBot kann die Launch-TX nur mitbundeln, wenn die vollständige signierte Launch-TX früh genug vorliegt. Die bisherigen ~182 ms bis zum P2P-Hash plus ~87 ms Hash->Full aus dem Testnet-Versuch sind für Produktion nicht akzeptabel und werden nicht durch den Bundle-Pfad magisch entfernt.

**B. Privater Launch-/Orderflow**

```text
Builder-/Auction-/Private-Mempool-Feed
        ↓
Target-Hash + ausreichende simulierte Fakten
        ↓
DarkBot BUY lokal vorbereiten
        ↓
backrunTarget / ordered inclusion
        ↓
Builder materialisiert:
[private target Launch]
[DarkBot BUY]
```

Für privaten Flow ist normales Public-P2P prinzipiell unvollständig. Deshalb werden 48Club Auction Feed / `backrunTarget` und ein geeigneter Private-Mempool-Feed als eigene Ingress-Klasse behandelt, nicht als Optimierung des öffentlichen Sensors.

### Builder-Abdeckung und Konkurrenz

Die externe Vergleichsanalyse findet keinen öffentlich dokumentierten Retail-Sniper, der auf BSC nachweislich einen strukturell besseren Pfad als direkten ordered Builder-Fan-out besitzt. Die relevante Konkurrenz sind vielmehr:

- Builder selbst und builder-nahe Searcher;
- Creator-/Dev-Bundles;
- Searcher mit privatem Orderflow;
- kapitalstärkere Searcher bei gleichem Signal und gleichem Builder.

Retail-/Telegram-Produkte ohne dokumentierten BSC-Bundle-Pfad werden nicht als technische Referenz für Gate 0 verwendet.

48Club und BlockRazor werden als primäre Inclusion-Kandidaten weitergeführt. Deren historisch sehr hohe gemeinsame Builder-Abdeckung wird als externe Evidenz behandelt, aber nicht als konstante aktuelle Garantie. Abdeckung und Block-Builder-Zuordnung werden in den eigenen Mainnet-Versuchen mitprotokolliert.

### 48Club-spezifische Pflichtpunkte

Vor Live-Freigabe müssen zusätzlich geprüft werden:

1. `positionFirst` und `noMerge` Semantik im aktuellen 48Club-Pfad;
2. aktuelle Gas-/Priority-/Tip-Regeln statt historischer Annahmen;
3. Auction Feed + `backrunTarget` für privaten Flow;
4. ob der ausgewählte Flap/Four.Meme-Scope im Feed vollständig genug ist, um den BUY ohne synchronen RPC-Roundtrip zu bauen.

Spät eintreffende Bundles dürfen nicht still ans Blockende rutschen, wenn das Ziel N+1 lautet.

### Mess- und Acceptance-Regel

Das primäre Produktziel bleibt unverändert:

```text
T_buy - T0 < 1 ms
```

Die operative Vor-T0-Zielarchitektur lautet:

```text
T_pre < T0
BUY vollständig vorbereitet/signiert < T0
ordered bundle beim Builder < T0
T0 = erste externe technische Kaufbarkeit
T_buy = erfolgreiche tatsächliche BUY-Ausführung
```

Chain-Ordering wird separat bewiesen:

```text
Launch = Position N
DarkBot BUY = Position N+1
successful external BUYs between = 0
positive intended-token acquisition
```

**N+1 beweist optimale Reihenfolge, aber nicht automatisch `T_buy-T0 < 1 ms`.**
Ohne vertrauenswürdigen hochauflösenden Builder-/Validator-Execution-Trace bleibt die numerische reale Sub-ms-Zeit `UNKNOWN`. Ein lokaler Full-Node-Replay darf später eine Execution-Dauer in µs als Schätzung liefern, wird aber nicht als echte Validator-Uhr ausgegeben.

### Reihenfolge ab jetzt

1. G0.6a Zwei-Wallet-Eigenlaunch Dry-PASS vollständig schließen.
2. V2-Budget und aktuelle Builder-Regeln endgültig einfrieren.
3. Nach expliziter Freigabe 1–4 kontrollierte Minimalwert-Mainnet-`[Launch][BUY]`-Versuche.
4. Danach fremden Mainnet-Ingress messen und optimieren:
   - viele Mainnet-Peers;
   - Full-TX first-seen;
   - Peer-Ranking;
   - eigener Sensor nahe Builder-Region;
   - zusätzliche Full-TX-/Private-Flow-Feeds nur anhand echter Messung.
5. Gleichzeitig reale Launch-/N+1-Statistik erfassen:
   - Public vs. privat sichtbar;
   - Dev-/Creator-Bundles;
   - N+1-Gewinner;
   - Builder;
   - Gas/Tip;
   - realisierte PnL/EV.
6. Erst danach zusätzliche kostenpflichtige Infrastruktur, Multi-Region oder eigener Builder, wenn die Messung den Engpass dort zeigt.

### Nicht priorisieren

Bis Ingress und Inclusion nachweislich nicht mehr dominieren:

- keine weitere Optimierung des Rust-Hotpaths um einzelne 10–100 µs;
- kein Kernel-Bypass/DPDK/AF_XDP nur aus Prinzip;
- kein eigener Builder ohne Messnachweis;
- kein Multi-Region-Rollout ohne regionale First-Seen-/Builder-Handoff-Daten.

Der bestehende Rust-Core wird beibehalten. Optimiert werden jetzt primär Sicht, Builder-Handoff, ordered inclusion und Zugang zu privatem Orderflow.


## 2026-10-02 — G0.6a Mainnet read-only preflight

Read-only BSC Mainnet preflight executed successfully from the G0.6a tooling.

Observed evidence:

```text
schema                  darkbot.g06a.preflight-readonly.v2
chain_id                56
latest_block_number     125347789
latest_block_hash       0xa91d8fe1250dd24c37f6af4e451b2a1c6b7d9825c0b29f9109dea32b3ba2eedc

portal                  0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0
portal_code_bytes       2882
portal_code_sha256      1fc365bfda5c9694d38901227927872e610f84d9de9b4e360fea40578fb4bcc1

tax_v3_impl             0x024f18294970b5c76c0691b87f138a0317156422
tax_v3_impl_code_bytes  19331
tax_v3_impl_code_sha256 34000dd228055664fda2debd21b5fe5288b750ca3ce8fb4e6933e5c529586ef5

gas_price_wei           50000000
```

Safety observations:

```text
network_actions         READ_ONLY_RPC_ONLY
signed_tx_submitted     false
bundle_submitted        false
```

Wallet binding was intentionally not part of this run:

```text
launcher_address        null
buyer_address           null
launcher_pending_nonce  null
buyer_pending_nonce     null
```

Interpretation:

- current BSC Mainnet chain binding is correct;
- current frozen Flap Portal address has runtime code;
- current frozen TAX_V3 implementation address has runtime code;
- G0.6a can proceed to wallet-bound Dry construction;
- this preflight does not prove launch semantics, BUY semantics, gas sufficiency, builder acceptance or N+1 inclusion;
- no value-bearing action occurred.

Next required G0.6a evidence is the wallet-bound Dry run:
distinct launcher A / buyer B, current pending nonces, salt, native launch parameters, positive minOutput, local sequential launch->BUY simulation, gas/budget proof, and builder-envelope identity.


## 2026-10-02 — G0.6a live native-BNB economics preflight v2

The expanded read-only Mainnet preflight was executed successfully after commit `d4041f1b49c5a704e12621ae34c8f6ba2633edaf`.

Frozen raw evidence:
`bsc/gates/gate0/evidence/g06a_preflight_readonly.json`

Observed Mainnet binding:

```text
schema                         darkbot.g06a.preflight-readonly.v2
chain_id                       56
latest_block_number            125351041
latest_block_hash              0xcd2188894c5f839538aab6c36f48f56a25b08bdc65f3e01b158e01b5cc3039e2

portal                         0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0
portal_code_bytes              2882
portal_code_sha256             1fc365bfda5c9694d38901227927872e610f84d9de9b4e360fea40578fb4bcc1

tax_v3_impl                    0x024f18294970b5c76c0691b87f138a0317156422
tax_v3_impl_code_bytes         19331
tax_v3_impl_code_sha256        34000dd228055664fda2debd21b5fe5288b750ca3ce8fb4e6933e5c529586ef5

gas_price_wei                  50000000
```

Live Portal economics read directly from Mainnet:

```text
native_quote_enabled           1
default_curve_id               20
alternative_curve_id           20
native_to_quote_swap_type      0
dex_id                         0

protocol_buy_fee_bps           100
protocol_sell_fee_bps          100
tax_on_bonding_curve_enabled   true
```

Safety:

```text
network_actions                READ_ONLY_RPC_ONLY
signed_tx_submitted            false
bundle_submitted               false
launcher_address               null
buyer_address                  null
```

Interpretation:
- native BNB launch configuration is enabled on the current Mainnet Portal;
- both reported native curve selectors resolve to curve ID 20 for this observation;
- current protocol buy/sell fees are 100 bps / 100 bps;
- bonding-curve taxation is enabled;
- Portal and TAX_V3 runtime byte fingerprints match the previously frozen deployment evidence;
- no wallet was bound and no value-bearing action occurred;
- these values are empirical Mainnet configuration evidence, not yet a proof of the independent native-BNB Curve-20 quote implementation.

### Frozen next step

Do not move directly to a value-bearing Mainnet trial.

Next, implement and independently verify the native-BNB quote/economics path for exactly:

```text
quote token              native BNB
curve                    20
native->quote swap type  0
dex id                   0
protocol buy fee         100 bps
bonding-curve tax        enabled
token version            TAXED_V3 / newTokenV6
```

Required result:
1. derive the exact Curve-20 arithmetic from verified runtime semantics/state, not by fitting observed outputs;
2. implement the production Rust quote/minOutput calculation;
3. cross-check it against the independent local sequential Launch-A -> BUY-B simulation;
4. freeze exact positive expectedOutput/minOutput and gas evidence;
5. only then bind launcher A / buyer B and close DRY-1..DRY-4.

This preserves the already frozen G0.6a -> G0.7 stage order.


## 2026-10-02 — G0.6a native-BNB economics crosscheck PASS

Successful read-only/current-state crosscheck was captured at BSC Mainnet block `125373566`.

Frozen evidence:
`bsc/gates/gate0/evidence/g06a_economics_evm_crosscheck_2026-10-02.json`

Current Portal binding from the same run:

```text
version                         v5.24.0
proxy                           0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0
implementation                  0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20
implementation_code_sha256      2ac893fca8dfa730d66cb4f0fc3e2c769f2ffdb4565e55903d9fa12e7cd7149f
curve_id                        20
protocol_buy_fee_bps            100
buy_tax_bps                     100
tax_on_bonding_curve_enabled    true
```

Independent result:

```text
Rust expectedOutput             176664965031639901123273
EVM simulated received amount   176664965031639901123273
difference                      0
buyer balance after             176664965031639901123273
status                          EVM_SEQUENCE_PASS
```

The EVM execution did not use the Rust expected output as execution input; its BUY minimum was deliberately `1`. Therefore the exact match is an independent semantics/economics crosscheck rather than a circular assertion.

Mainnet launch semantics also passed read-only node execution:

```text
eth_estimateGas                 PASS
launch gas estimate             5502725
120% launch gas reserve         6603270
eth_call                        PASS
predicted token match           true
local launch execution gas      5223342
local BUY execution gas         359291
```

Safety:

```text
signed transactions created     false
signed transactions submitted   false
bundle submitted                false
network action                  READ_ONLY_RPC_STATE_AND_HEADER_ONLY
```

### Gate interpretation

- Native-BNB Curve-20 arithmetic/economics crosscheck: **PASS**.
- Exact received amount agrees between independent Rust calculation and sequential local EVM execution: **PASS**.
- Full DRY-2 remains pending until the exact wallet-bound signed launch_A / buy_B bytes are built and verified against this same semantics path.
- DRY-3 is currently blocked by the existing v2 proposal gas cap: `max_launch_gas_limit=5000000` is below the measured Mainnet requirement. The proposal must not be armed unchanged.
- Mainnet execution remains **UNAUTHORIZED / UNARMED**.

Next stage remains the frozen plan:
1. bind distinct launcher_A and buyer_B;
2. capture both pending nonces and balances read-only;
3. build exact signed bytes locally and recover/verify all envelope fields;
4. update the proposed launch gas/budget ceiling using measured evidence;
5. close DRY-1, full DRY-2, DRY-3 and DRY-4 before any authorization.


## 2026-10-02 — G0.6a current-target builder handoff diagnostics

Two current native-BNB Flap V6 invalid-wire transport diagnostics were run against warm real builder TLS/TCP boundaries. These are diagnostic only because the benchmark still performs quote + BUY build/sign after T_signal, which violates the frozen G0.2/G0.5 preparation rule for the controlled self-launch where those fields are precomputable before T0.

48Club Puissant diagnostic:
- T_signal -> T_ready p50: 255.541 us
- T_signal -> T_send p50: 518.833 us
- T_signal -> T_send p95: 963.041 us
- T_signal -> T_send p99: 1642.791 us
- >1 ms: 5/100
- remote TLS write+flush p50: 224.541 us

BlockRazor standard bundle diagnostic:
- T_signal -> T_ready p50: 274.125 us
- T_signal -> T_send p50: 543.625 us
- T_signal -> T_send p95: 1084.375 us
- T_signal -> T_send p99: 2216.959 us
- >1 ms: 8/100
- remote TLS write+flush p50: 232.750 us

Interpretation:
- Both paths demonstrate sub-ms median current-target remote handoff.
- 48Club is slightly better in these runs, but no real inclusion winner is claimed.
- JSON-RPC response time is not T_send and not T_buy.
- These runs MUST NOT be promoted to final G0.5 self-launch latency evidence because quote/build/sign remained inside the measured interval even though the controlled self-launch knows the exact token/BUY before T0.
- The next valid self-launch benchmark MUST obey G0.2/G0.5 exactly: precompute/prebuild/pre-sign every signature-bound field that is final before T0; keep only proven dynamic residual work after T_signal; keep builder connections warm; measure T_ready, T_queue and actual builder transport T_send separately.
- 48Club qualification path MUST carry noMerge=true and positionFirst=true.
- BlockRazor and 48Club remain candidate inclusion adapters; actual controlled Mainnet acquisition/order result decides the winner.
- T0 -> T_buy remains NOT MEASURED. Do not derive it from block timestamps or these handoff diagnostics.


## 2026-10-02 — G0.5 prepared/presigned self-launch transport PASS

The controlled Flap self-launch path was remeasured under the frozen G0.2/G0.5 preparation rule: expensive work is complete before T0; after T_signal the benchmark only selects the prepared request and performs the warm TLS write+flush to the tested remote transport.

48Club:
- T_signal -> T_ready select-only p50: 0.542 us
- T_signal -> T_send p50: 241.500 us
- T_signal -> T_send p95: 337.125 us
- T_signal -> T_send p99: 584.291 us
- max: 602.625 us
- >1 ms: 0/100

BlockRazor standard endpoint:
- T_signal -> T_ready select-only p50: 0.500 us
- T_signal -> T_send p50: 228.458 us
- T_signal -> T_send p95: 376.250 us
- T_signal -> T_send p99: 526.584 us
- max: 557.000 us
- >1 ms: 0/100

Classification:
- G0.5 prepared/presigned self-launch local+transport target <1 ms: PASS for both tested standard endpoints.
- These measurements are transport/hotpath evidence only. They are not T_buy, inclusion acceptance, or ordering evidence.
- Real controlled Mainnet G0.7/G0.8 remains required for actual token acquisition and Launch=N / BUY=N+1 proof.
- No numeric sub-ms T0->T_buy claim is permitted without trusted builder/validator execution timing evidence.


## 2026-10-03 — Wirklicher presigned Testnet-BUY erfolgreich; Execution-Latenz noch offen

Der bestehende `g07_testnet_birth_buy.rs` baut/signiert den vollständigen BUY und serialisiert den HTTP-Request jetzt vor Launch. Nach dem erwarteten P2P-Signal folgen nur vorbereiteter Write/Flush; Decode und Logging erfolgen nach `T_send`. Beide HTTPS-Verbindungen werden vor der P2P-Bereitschaft aufgewärmt, damit Ready nicht während langsamer Verbindungsaufbauten veraltet. Release-Build: `cargo build --release --locked --offline --manifest-path bsc/gates/gate0/Cargo.toml --bin g07_testnet_birth_buy` erfolgreich; bestehende ungenutzte WSS-Helfer erzeugen Warnungen. WSS bleibt deaktiviert.

**Realer Testnet-Kauf, Chain 97, Exit 0:**

- Wallet: `0x9c8a0Db41914beEE52Dd49790c4F087E252743DE`.
- Launch: `0x1958d18adc295c8f1e882649d8d82a55647a117e79ba733cb400dd7f8aa27237`, Receipt Status 1, Block 134530812 / Index 0.
- BUY: `0x37139afd5bec7441d43750e8cd340fad158f6cb6fe80e74751309e82b956aec8`, Receipt Status 1, Block 134530813 / Index 0.
- Token: `0x80646D82174Ef7E890389B70e41c89Eff6ed7777`.
- Tatsächlich erhalten: **17669034223232267628301 Basiseinheiten**, sowohl Runner-balanceOf >0 als auch Transfer im BUY-Receipt an die Wallet stimmen exakt überein.
- BUY-Wert 0,0001 tBNB; gemessener Netto-Wallet-Debit über Launch + BUY: 499295900000000 wei. Kein Mainnet-BNB.
- BUY im Folgeblock; kein N/N+1-same-block-PASS. Bestehender Scanner findet null erfolgreiche direkte Portal-BUYs dazwischen; er deckt keine beliebigen internen Router-Calls ab.
- Konfigurierte Peer-Schwelle 1, beim Full-Signal 2 Peers. Keine robuste Ingress-Coverage-Behauptung.

Messwerte dieses erfolgreichen Laufs:

| Grenze | Wert |
|---|---:|
| BUY vorab bauen/signieren | 0,172167 ms |
| vollständiges P2P-Signal → Send | **0,103417 ms** |
| Queue → Send | 0,074792 ms |
| Launch-Handoff → P2P-Hash | 229,235916 ms |
| P2P-Hash → vollständige TX | 151,978500 ms |
| Launch-Handoff → vollständiges Signal | 381,214416 ms |
| Signal → Receipts beobachtet | 3844,980667 ms |
| T0 → T_buy | **UNKNOWN_FROM_STANDARD_BSC_BLOCK_DATA** |

Diese Grenzen sind verschieden. Internet-/Gossip-/Provider-/Beobachtungsanteile dürfen nicht pauschal von einer behaupteten Execution-Zeit abgezogen werden. Weder Folgeblockabstand noch Receipt-Abfragezeit sind `T0 → T_buy`. Echte Sub-ms-Ausführungszeit braucht vertrauenswürdige Builder-/Validator-/Execution-Zeitstempel.

Vorversuche bleiben erhalten: erster Lauf vor Launch wegen 8-Peer-Timeout abgebrochen; zweiter Launch erfolgreich (`0x72ecc14f16d03a35952a473b8e43f42803725d85b3f21e0d50e5760adb53ec5a`), BUY nicht gesendet wegen fehlendem P2P-Signal nach Peer-Abbruch während HTTPS-Warmup; dritter Versuch vor Launch durch `RateLimitExceeded` zurückgewiesen. Keine BUY- oder Latenz-PASS-Werte aus diesen Fehlerfällen.

Evidence-Ordner: `bsc/gates/gate0/evidence/g07_presigned_testnet_20261002T235624Z`, `...20261002T235824Z`, `...20261003T000013Z`, `...20261003T000326Z`. Beim erfolgreichen Lauf sind beide Raw-Receipts und beide Blöcke gespeichert. Historisches balanceOf am BUY-Block scheiterte beim öffentlichen RPC mit `missing trie node`; dieser Fehler bleibt gespeichert und wird nicht als erfolgreicher historischer State-Read ausgegeben.

**Nächste ausschließlich BUY-bezogene Arbeit:** Der neue Runner-Stand reagiert für den vollständig bekannten Self-Launch direkt auf dessen exakt gefilterten P2P-Hash (oder Full-Broadcast), statt vor Send auf GetPooledTransactions zu warten. Das soll die beobachtete Hash→Full-Wartezeit aus dem kritischen Pfad entfernen. Dieser zusätzliche Schritt ist gebaut, **noch nicht live gemessen**; alle obigen Werte stammen vom Full-Signal-Lauf. Danach weiterhin tatsächliche Acquisition/Ordering prüfen und vertrauenswürdige Execution-Zeitstempel beschaffen. Gate 0 bleibt bis zum echten `T0 → T_buy <1 ms`-Nachweis offen. Keys wurden nur über verdeckte lokale Eingabe an jeweils einen Prozess gegeben, weder protokolliert noch gespeichert.


## 2026-10-03 — Hash-Trigger live getestet: tatsächlicher BUY, erneut Folgeblock

Zielgrenze unverändert: `T0` ist der erste Ausführungspunkt, an dem ein unabhängiger externer BUY gültig ausführbar ist; `T_buy` ist erfolgreiche BUY-Ausführung mit Erwerb. Pending-Erkennung/Vorbereitung vor T0 gehört nicht in dieses Intervall. Dass ein Signal über den Pending/P2P-Pfad kommt, beweist allein noch nicht, dass es lokal vor T0 eintraf. Erste Chart-Kerze und `<1 ms` sind unterschiedliche Aussagen.

Der reale Hash-Trigger-Lauf `bsc/gates/gate0/evidence/g07_hash_trigger_testnet_20261003T002258Z` endete mit Exit 0. Launch `0x0d8cbea2ea060158246a51379fb5dd2cf1310218ab796d3302eb2aebfbaa2ee4` und BUY `0xfcab0afaca413182e879e44dd87b7f86ff547e696e5c5d320b81869d55538d0a` haben beide Receipt-Status 1. Token `0xFC56AC387716B144294a6E896C21E6d523657777`; Erwerb **17669034223232267628301 Basiseinheiten**, Receipt-Transfer an die Testwallet stimmt mit Runner-balanceOf überein. BUY 0,0001 tBNB; Netto-Wallet-Debit 500203100000000 wei.

| Messgrenze | Einzelner realer Lauf |
|---|---:|
| P2P-Hash → vorbereiteter HTTPS-Write/Flush abgeschlossen | **0,045125 ms** |
| Queue → Send | 0,044416 ms |
| BUY bauen/signieren vor Launch | 0,110916 ms |
| Launch-Handoff → P2P-Hash | 216,260291 ms |
| P2P-Hash → vollständige TX, jetzt nach Send | 148,544500 ms |
| Signal → Receipts beobachtet | 3916,428750 ms |
| Launch-Position | 134533644:0 |
| BUY-Position | 134533645:0 |
| `T_buy - T0` | **UNKNOWN** |

Die Hash→Full-Wartezeit liegt nachweislich nicht mehr vor Send. Dennoch kein Same-block-Ergebnis: schneller lokaler Versand allein hat im getesteten öffentlichen RPC-Pfad keine Platzierung unmittelbar nach Launch erzielt. Weder der Zeitpunkt der Validator-Ankunft noch ein positives Pending-Lead vor T0 ist gemessen; aus diesen Daten lässt sich die verbleibende Verzögerung nicht eindeutig auf Internet, Gossip, RPC oder Blockauswahl aufteilen. Keine Sub-ms-Acquisition-Behauptung. Der bestehende direkte Portal-Scanner fand null erfolgreiche konkurrierende direkte BUYs; interne Router-Calls sind nicht vollständig abgedeckt.

Vorversuch `...20261003T002100Z`: null Peers, 30-s-Timeout vor Launch, keine TX gesendet. Die Startfrist wurde für die Wiederholung auf 120 s erhöht; die erfolgreiche Wiederholung erreichte einen Peer. Zusätzlich wurde die Ausgabe `P2P_hash_to_full_ns` an den tatsächlichen Full-Empfang statt den früheren Hash-Trigger gebunden. Release-Build erfolgreich (sieben bestehende ungenutzte WSS-Helferwarnungen). Keine Mainnet-TX, kein persistierter Key.

**Verbleibender Nachweis an der Quelle:** Es gibt in diesem Testaufbau keinen instrumentierten Blockproduzenten-/Validator-Zugang. Standard-Receipts und die gespeicherten Blöcke (einschließlich `milliTimestamp`) enthalten keine individuellen historischen TX-Ausführungszeiten. Erforderlich sind auf derselben monotonen Uhr erfasste T0- und erfolgreiche BUY-Endpunkte aus der Produktion des später kanonischen Blocks, mit Blockhash, beiden TX-Hashes/Indizes und Reorg-Abgleich. Kandidaten-Simulation, nachträglicher Archive-Replay oder Import auf einem anderen Node messen jeweils ihre eigene Ausführung und dürfen nicht als ursprüngliche kanonische Ausführungszeit ausgegeben werden. Messaufwand und Auflösung müssen für den `<1 ms`-Nachweis ausgewiesen werden.

Geth beschreibt Live-Hooks für Transaktionsverarbeitung und Receipts: https://geth.ethereum.org/docs/developers/evm-tracing/live-tracing . Das ist eine technische Referenz für Instrumentierung, kein Nachweis, dass unser öffentlicher BSC-Testnet-RPC diese Telemetrie bereitstellt. Die Produktions-/Builder-Integration sowie ihre Kompatibilität mit dem konkreten BSC-Client müssen noch hergestellt werden. Parallel bleibt ein geordneter Inclusion-Pfad für Launch→BUY zu qualifizieren; ein öffentlicher SendRawTransaction-Aufruf sichert keine direkte Nachfolgeposition. Zielstatus bleibt OFFEN; kein automatisches Gate-PASS.

Nachrechnung der Blockzeit-/Sekundenaggregation für denselben Hash-Trigger-Lauf: Launch-Block `milliTimestamp=1790987097600` = 2026-10-03 00:24:57.600 UTC; BUY-Block `1790987098050` = 00:24:58.050 UTC. **Header-Zeitabstand 450 ms**. Auf volle Sekunden gebucktet: 00:24:57 versus 00:24:58, also unterschiedliche 1-s-Intervalle; beide liegen im selben UTC-Minutenintervall 00:24. Das sind aus Headern abgeleitete Ereignis-Buckets, keine unabhängigen Trade-Ticks oder Chart-Kerzen. Die 1-s-Aggregation zeigt eine Bucket-Differenz von 1 s, nicht eine gemessene BUY-Latenz von 1 s. Ebenso bleibt der Header-Abstand 450 ms vom unbekannten individuellen Ausführungsabstand `T_buy-T0` getrennt. Aggregation gewinnt keine zusätzliche zeitliche Auflösung.

### Direkte Trade-Event-Prüfung desselben Laufs (keine OHLC-Rekonstruktion behauptet)

`TokenCreated` und `TokenBought` gemäß eingefrorener offizieller IPortal-ABI dekodiert; Signatur-Hashes mit ethers geprüft. Event-Zeitstempel: Launch `1790987097`, BUY `1790987098`, jeweils ganze Sekunden. BUY-Event: `amount=17669034223232267628301`, `eth=100000000000000`, `fee=1000000000000`, `postPrice=5546515046`. Effektiver Einsatz pro erhaltener atomarer Einheit ist `5.659618898044479368682226228898161862940e-9`; bei 18 Dezimalstellen beider Assets derselbe Zahlenwert in tBNB pro Token, ohne Gas. `postPrice` ist der Zustandspreis nach dem Trade und nicht der durchschnittliche Ausführungspreis.

**Alle vier Receipts beider kanonisch zugeordneten Blöcke vollständig erfasst** (TX-Hash-Mengen jeweils gegen Block abgeglichen). Genau ein TokenBought-Event dieses Tokens vorhanden: unser BUY. Damit **erster aufgezeichneter Portal-BUY nach Launch**, null frühere Portal-BUY-Events, einschließlich Events aus internen Router-Aufrufen. Diese engere vollständige Event-Prüfung ersetzt für dieses Zeitfenster die vorherige Top-level-only-Einschränkung; keine Behauptung über beliebige andere Handelsplätze. Dennoch Kauf im Folgeblock und kein bewiesenes `<1 ms` nach T0. Coin-Alter bleibt nur als ein Block / 450-ms-Headerabstand einordenbar, nicht als individuelle Ausführungszeit.

Externe OHLC-Daten wurden nicht abgerufen: direkter Flap-HTTP-Aufruf 403; öffentliche Flap-Netzwerkauswahl im Browser zeigt BSC, Robinhood, Base, XLayer, Monad, Morph, keinen BSC-Testnet-Eintrag. Konkreter ms-Tick-Feed für Chain 97 bisher nicht identifiziert. `eth_getLogs` lieferte `-32005 limit exceeded`; anschließend wurden erfolgreich die zwei fehlenden Receipts abgerufen. Event-Felddefinitionen: https://docs.flap.sh/flap/developers/wallet-and-terminal-and-bot-developers/trade-tokens . Weder Event-Sekundenwerte noch OHLC-Aggregation ergeben neue Sub-ms-Ausführungszeitstempel.

## 2026-10-03 — Aufgabenplan erweitert: früherer BUY und 15 Sekunden Rohdaten

Auf ausdrücklichen Nutzerauftrag enthält `bsc/docs/GATE0_TESTPLAN_SHORT.md`, Abschnitt „Current task amendment — 2026-10-03“, nun die nächste Arbeitsreihenfolge: vor T0 vorbereiten, geordnete Pre-Submission/Inclusion qualifizieren, stabilen frühen Ingress und Zustellung prüfen, Produktions-Ausführungszeiten instrumentieren und bei **jedem** Versuch die ersten 15 Sekunden ab T0 als Rohdaten erfassen. Auch fehlgeschlagene/verpasste Käufe bleiben enthalten. Die Aufzeichnung startet vor Launch und wird anschließend kanonisch abgeglichen; unbekannter T0, Datenlücken und Reorgs werden explizit ausgewiesen. Preis/OHLCV/Kosten/Reihenfolge werden nachträglich aus Roh-Trades abgeleitet, ohne dadurch Sub-ms-Ausführungszeiten zu behaupten.

Dies ist ein **Dokumentations-/Aufgabenplan-Update**, keine implementierte Capture-Funktion, kein neuer Testlauf und keine Freigabe zusätzlicher Mainnet-Ausgaben. Die oben gemessenen Ergebnisse bleiben unverändert.

## 2026-10-05 — Unit-5 TLS frozen; Q5A qualification harness accepted

Current BSC handoff state after synchronizing the complete repository and completing the local Q5A harness acceptance run.

Repository baseline before this ledger update:

```text
tracked local main                 79162927c0df392aaa59ac155a9b600bf7f7180b
origin/main                        79162927c0df392aaa59ac155a9b600bf7f7180b
tracked repository state           SYNCHRONIZED
Q5A harness commit                 79162927c0df392aaa59ac155a9b600bf7f7180b
```

The two known local Project-0 paths `project_0/data/shadow/` and `project_0/native/Cargo.lock` remained untracked and were not part of this BSC commit/state.

### Frozen Unit-5 implementation state

```text
Units 1-4                          IMPLEMENTED_TESTED / FROZEN
Unit 5 Core                       IMPLEMENTED_TESTED / FROZEN
Unit 5 Runner v1 Rev3             IMPLEMENTED_TESTED / FROZEN
Unit 5 TLS/socket transport Rev2  IMPLEMENTED_TESTED / FROZEN
Q5A qualification harness         IMPLEMENTED_TESTED
real TLS transport qualification  NOT_RUN
real provider qualification       OPEN
Production QualifiedTransport     NONE
LIVE                               BLOCKED
T0 -> T_buy < 1 ms                NOT YET PROVEN
```

The frozen TLS implementation uses rustls `0.23.45` with the ring provider and `webpki-roots 1.0.9`. DNS resolution, TCP connect, TLS handshake, warm-up and reconnect remain outside the BUY dispatch. The dispatch prepares TLS records before the final monotonic deadline check; the first `write_tls` socket operation occurs after that check. The recorded local handoff boundary remains userspace TLS write/flush completion, meaning ciphertext accepted by the kernel send path, not NIC transmission, builder receipt, inclusion or execution.

### Q5A qualification harness

The new workspace crate `bsc/native/crates/darkbot-qualify` is committed and uses the frozen production TLS/transport types rather than a second transport implementation.

The harness is deliberately non-value-bearing:

- safe probe only;
- no funded BUY;
- no executable transaction;
- no LIVE authorization;
- no production `QualifiedTransport` constructor;
- provider semantics and inclusion remain separate from transport qualification.

The evaluator was hardened fail-closed before acceptance:

- exactly 100 measured timing values are required for a PASS;
- every `HandedOff` sample requires the mandatory connection/timing/wire evidence;
- `t_boundary_ns >= t_write_check_ns`;
- the stored duration must exactly equal `t_boundary_ns - t_write_check_ns`;
- `wire_bytes > 0`;
- peer/generation must match the applicable connection scope;
- the independent Python checker recomputes durations and aggregates from raw timestamps;
- build identity accepts only the exact expected tri-state values and checks the frozen transport implementation identity.

### Target-system acceptance

Final chained acceptance on the user's arm64 macOS target:

```text
cargo fmt --all -- --check                                      PASS
cargo build --workspace --locked --offline                     PASS
cargo build --workspace --release --locked --offline           PASS
cargo test --workspace --locked --offline                      PASS
cargo test --workspace --release --locked --offline            PASS
cargo clippy --workspace --all-targets --locked --offline
  -- -D warnings                                                PASS
workspace test inventory                                        303
final marker                                                    Q5A_HARNESS_ALL_GATES_PASS
```

The earlier Clippy-only failure in `darkbot-qualify/tests/offline.rs` was limited to three `type_complexity` warnings in test case array declarations. It was corrected with test-only type aliases; no production, measurement or qualification semantics changed. The complete chained gate then passed.

### Gate interpretation

This checkpoint proves that the Q5A qualification harness itself is implemented and regression-tested on the target machine. It does **not** qualify a real 48Club or BlockRazor transport yet.

Still open:

1. run Q5A from a clean committed release build against the real provider endpoints;
2. collect 100 measured samples plus reconnect evidence per provider;
3. require zero UNKNOWN/write/flush/deadline failures and a maximum qualified handoff below 1,000,000 ns for the declared workload;
4. independently verify the generated evidence;
5. only then decide whether a production `TransportQualification` / `QualifiedTransport` construction path may be introduced;
6. keep actual provider ordering/atomicity/same-block behavior, inclusion and `T0 -> T_buy` as separate later evidence.

No provider ACK, TLS handoff result, same-block observation or local latency measurement may be promoted to a `T0 -> T_buy < 1 ms` claim. Product LIVE remains blocked.

## 2026-10-05 — Q5A real TLS transport qualification PASS

This checkpoint supersedes the earlier `real TLS transport qualification = NOT_RUN` status. The Q5A harness was run against both frozen production builder endpoints with the accepted release binary and the resulting raw evidence is now committed.

### Evidence identity and custody

```text
qualification harness              u5_tls_qualify v1
release-binary git_head            0635a3978a471bf4916236fb32de6c9ae96d0b9c
frozen baseline                    92fa5204777e566acbaccf111721d9295ffb3a90
git_dirty                          false
baseline_ancestor                  true
frozen_sources_unchanged           true
transport implementation           darkbot-runner/tls-v1 rustls-0.23.45 ring http/1.1
target                             macos / aarch64
profile                            release
combined raw evidence              bsc/native/evidence/q5a/q5a-both-20261005T203919Z.json
combined SHA256                    e2c537765f539f7551d89613c337d2079820731b77a531b7306a14fe4c845849
first evidence commit              0424e439e49bd21057f6ba64dd722422e5c84a54
independent checker                PASS / consistent with the harness
```

The two single-provider supporting files are retained as well:

```text
48Club:
  bsc/native/evidence/q5a/q5a-club48-20261005T171555Z.json
  SHA256 af12d9cc036e33fbbfee071ac7b28b14011b6c0cff4ab9030d023f6a9e6752a5

BlockRazor:
  bsc/native/evidence/q5a/q5a-blockrazor-20261005T203649Z.json
  SHA256 e3a12c8e1d3f77b5e21bc66405bf7ad0e1b75d8e8e7fdd6a14598a229324fad3
```

No BlockRazor authorization value is written to the evidence.

### Combined real-provider result

The accepted combined run used the declared Q5A workload per provider: 10 warmups, 100 measured samples, 200-ms pacing and 3 reconnect cycles with a probe after each reconnect. The strict miss threshold is `>= 1,000,000 ns`.

| Provider | API scope | Handoffs | skipped | unknown | p50 | p95 | p99 | max | >=1 ms |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 48Club / `puissant-builder.48.club:443` | ALL_ANSWERED | 113 | 0 | 0 | 178,708 ns | 386,375 ns | 564,208 ns | **766,541 ns** | **0** |
| BlockRazor / `virginia.builder.blockrazor.io:443` | ALL_ANSWERED | 113 | 0 | 0 | 136,500 ns | 254,833 ns | 335,584 ns | **566,250 ns** | **0** |

```text
48Club transport verdict           PASS
BlockRazor transport verdict       PASS
TLS_TRANSPORT_QUALIFICATION        PASS
Q5A process exit                   0
independent raw-timestamp recheck  PASS
```

For this exact workload, implementation and target, every measured handoff stayed below the strict 1-ms limit and every required handoff/reconnect datum was present.

### Exact meaning of this PASS

Q5A qualifies the frozen production TLS transport path and its declared local boundary:

```text
t_write_check
→ first TLS socket write
→ remaining TLS write/flush
→ t_boundary
```

The boundary is userspace TLS write/flush completion with the request ciphertext accepted by the local kernel send path. It is **not** NIC transmission, remote receipt, builder scheduling, block inclusion or EVM execution.

`API ALL_ANSWERED` means the safe empty-bundle probes received complete classifiable responses. It is not a qualification of `positionFirst`, `noMerge`, ordering, atomicity, same-block behaviour, pre-submission, N/N+1 placement, extraction/rebroadcast policy or any other provider lifecycle property.

### Current Unit-5 state after Q5A

```text
Units 1-4                          IMPLEMENTED_TESTED / FROZEN
Unit 5 Core                       IMPLEMENTED_TESTED / FROZEN
Unit 5 Runner v1 Rev3             IMPLEMENTED_TESTED / FROZEN
Unit 5 TLS/socket transport Rev2  IMPLEMENTED_TESTED / FROZEN
Q5A qualification harness         IMPLEMENTED_TESTED
real TLS transport qualification  PASS
48Club TLS transport              PASS
BlockRazor TLS transport          PASS
real provider semantics           OPEN
Production QualifiedTransport     NONE
LIVE                               BLOCKED
T0 -> T_buy < 1 ms                NOT YET PROVEN
```

The next Unit-5 slice may now introduce the production transport-qualification generation / `QualifiedTransport` construction path **only for this accepted TLS transport scope**. It must remain fail-closed on implementation, endpoint/profile, adapter/capability generation and handoff-boundary mismatches. It must not promote any provider semantic property or authorize LIVE. Provider semantics, inclusion, canonical acquisition and the original-producer `T0 -> T_buy` proof remain separate required gates.

## 2026-10-06 — Q5B fail-closed production TLS qualification binding

Q5B promotes the accepted Q5A TLS evidence into the production runner without promoting any provider semantic property.

### Implementation and acceptance

```text
final tested commit                  bc51d96352f906050239455aad254b957112e8e6
accepted Q5A evidence                bsc/native/evidence/q5a/q5a-both-20261005T203919Z.json
accepted Q5A SHA256                  e2c537765f539f7551d89613c337d2079820731b77a531b7306a14fe4c845849
transport implementation             darkbot-runner/tls-v1 rustls-0.23.45 ring http/1.1
Q5A qualification generation        1
target                               macos / aarch64
Q5B final chained acceptance         Q5B_FINAL_ALL_GATES_PASS
```

The final target-system acceptance command completed all of:

```text
cargo fmt --all -- --check
cargo build --workspace --locked --offline
cargo build --workspace --release --locked --offline
cargo test --workspace --locked --offline
cargo test --workspace --release --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
```

### Production trust boundary

The production qualification path is fail-closed and sealed:

- `tls::qualification` is the only production module that can create the opaque accepted-evidence token.
- `transport::bind_accepted_tls` consumes that token and creates the opaque `TransportQualification` plus concrete `QualifiedTransport`.
- Callers cannot manufacture a production qualification from a string, generation number or arbitrary `dyn Transport`.
- The binding checks the frozen capability adapter/kind and `TlsUserspaceWriteFlushReturned` handoff boundary plus the accepted endpoint/profile: implementation id, host, port, exact Q5A policy, target OS/arch, nonzero connection generation, accepted peer, TLS version, cipher suite, HTTP/1.1 and ALPN.
- The qualification is bound to the current capability-table generation. The existing release dispatch also requires the runner's current `TransportQualifications` entry to equal the transport binding, so qualification-generation rotation retires older transports.

The five Q5B qualification tests cover the pinned evidence identity/generation, both accepted provider profiles, rejection of unqualified adapter kinds, implementation/endpoint/policy/target mismatches, and peer/generation/TLS/cipher/ALPN mismatches.

### Scope remains deliberately narrow

```text
Q5B transport binding                IMPLEMENTED_TESTED / FROZEN
real TLS transport qualification     PASS
production QualifiedTransport path   PRESENT, fail-closed
provider semantic qualification      OPEN
LIVE                                 BLOCKED
T0 -> T_buy < 1 ms                   NOT YET PROVEN
```

Q5B does **not** qualify `positionFirst`, `noMerge`, ordering, atomicity, same-block placement, pre-submission, N/N+1 placement, extraction/rebroadcast/downstream exposure, builder inclusion, canonical acquisition or the product-level `T0 -> T_buy` requirement. The frozen capability records remain unchanged, so those missing semantic qualifications still prevent release eligibility.

The next C0 task is Unit 6 typed startup config/secrets/authorization plus the remaining minimal recorder/replay foundation. Q5B does not authorize a funded Mainnet action.

## 2026-10-06 — C0 Unit 6 startup authorization + recorder/replay foundation

Final tested production-workspace commit:

```text
57e7dbbdf83c0c479c0e0a7c5365044914e181e3
UNIT6_FINAL_ALL_GATES_PASS
```

The target-system chained acceptance completed:

```text
cargo fmt --all -- --check
cargo build --workspace --locked --offline
cargo build --workspace --release --locked --offline
cargo test --workspace --locked --offline
cargo test --workspace --release --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
```

### Unit 6 startup scope accepted

The runner now owns a typed, fail-closed startup boundary:

- typed chain/profile, policy/codehash/capability/authorization generations, sender/first nonce, lane/capture capacity, clock identity, eligible adapters/endpoints and economic gas/bid/amount/slippage/budget policy;
- signer secret and BlockRazor authorization are loaded before trading; debug output redacts credential material and the temporary signer-file byte buffer is overwritten after parsing;
- startup funding, next nonce, chain, sender and canonical checkpoint are supplied separately as `StartupFacts` and must agree with the configured policy before authorization is minted;
- actual observed balance must cover the configured maximum total exposure;
- a zero canonical checkpoint, observed chain/sender/nonce mismatch, missing credential, wrong signer, wrong endpoint, stale generation or invalid budget fails closed;
- background policy/codehash/capability/authorization invalidation is represented by immutable generation snapshots; old startup authorization stops comparing current after a generation changes;
- the production `ArmedFence` constructor is feature-gated to the runner startup path;
- the admitting `IntentLane` is created only after a create-new startup fence is written and fsynced; an existing fence always returns restart quarantine and no automatic resume exists;
- the durable fence binds chain, sender, first nonce, capacities, clock, canonical block checkpoint and the four startup generations.

The source that obtains/reconciles the `StartupFacts` from the live chain is **not** invented in Unit 6. Canonical/pending/recovery ownership remains C2; the eventual runner assembly must provide independently reconciled facts to this already tested validation boundary.

### C0 recorder/replay foundation accepted

The new `darkbot-capture` crate provides the concrete minimum C0 foundation:

- versioned binary journal framing with explicit little-endian fields;
- per-frame CRC32 integrity;
- SHA-256 manifest over the exact journal bytes;
- sequence-preserving replay with corruption, truncation, payload-bound and sequence-gap rejection;
- bounded non-waiting producer API using `try_send`, with overflow/disconnect/oversize counters and visible sequence identities;
- background drain/writer ownership rather than filesystem writes in the decision path.

This is a **foundation**, not a C1/C3 latency qualification. The current recorder copies the payload and the standard bounded channel is not claimed lock-free or allocation-free. C1 must provide the production mmap/SPSC Fast Lane, and C3 must qualify the integrated hot path under recorder load.

New Unit-6/C0 tests: 7 startup tests + 8 capture/replay tests = 15. With the previously accepted 308-workspace inventory after Q5B, the current source inventory is 323 workspace tests.

### Status

```text
Units 1-4                         IMPLEMENTED_TESTED / FROZEN
Unit 5 Core/runner/TLS            IMPLEMENTED_TESTED / FROZEN
Q5A real TLS qualification        PASS
Q5B production TLS binding        IMPLEMENTED_TESTED / FROZEN
Unit 6 startup authorization      IMPLEMENTED_TESTED / FROZEN
C0 recorder/replay foundation     IMPLEMENTED_TESTED / FROZEN for foundation scope
provider semantic qualification  OPEN
C1 geth mmap/SPSC Fast Lane       IN PROGRESS — C1.5-C1.8 LOCAL PASS; C1.9 MEASUREMENT COMPLETE / FINAL BIND OPEN
C1.9b Testnet hook/path            NEXT
C1.10 freeze                       BLOCKED until C1.9 final bind + C1.9b PASS
LIVE                               BLOCKED
T0 -> T_buy < 1 ms                 NOT YET PROVEN
```

Unit 6 does not qualify provider ordering/inclusion semantics, does not implement the C2 canonical-state owner, does not establish the C1 producer hook or mmap/SPSC latency, and authorizes no funded Mainnet action.


## 2026-10-08 — C1 REV2 lokale sequentielle Qualifikation: Common + C1.4a + C1.5-C1.8 PASS; C1.9 Messung vollständig/eligible

Dieser Eintrag dokumentiert den lokal erreichten C1-Stand nach Integration des C1.5-C1.10-REV2-Kandidaten. Er ersetzt keine normative Exit-Regel. Die Reihenfolge bleibt:

```text
reopened C1.3/C1.4 regressions
-> C1.4a
-> C1.5
-> C1.6
-> C1.7
-> C1.8
-> C1.9
-> C1.9b
-> C1.10 freeze
-> erst danach C2
```

### Source-/Basisidentität

REV2-Kandidat ursprünglich gegen:

```text
faba91395198d1deb21cd1a318f12d1c2b653aab
```

Lokaler Repository-HEAD während der heutigen Integration/Qualifikation:

```text
12c4616f9b24bf036d99cdd549e56ff2d448d372
```

Pinned bsc-geth:

```text
cdb7548b5baacfdae92f9f63437d6456411665f3
```

Wichtig: Die integrierten C1-REV2-Änderungen liegen während dieses Checkpoints noch lokal/uncommitted auf diesem HEAD. Deshalb darf der endgültige C1.10-Freeze den obigen Git-HEAD nicht als alleinige Bindung des finalen C1-Quellstands behandeln. Der REV2-Freeze-Pfad muss den finalen clean commit plus vollständigen deterministischen C1-Source-/Patch-Hash binden.

Unabhängige lokale Project-0-Arbeit bleibt außerhalb dieses C1-Scope und wurde nicht verändert.

### REV2-Korrekturen vor der lokalen Gate-Sequenz

Der REV2-Kandidat schloss die drei unabhängigen Review-Blocker:

1. **C1.7 queued BUY target binding**
   - jeder eingereihte BUY bindet den bei Dispatch ausgewählten Peer/Target-Snapshot;
   - Reassignment darf einen bereits queued BUY nicht still auf einen anderen Peer umbiegen;
   - Evidence bleibt dem gebundenen Ziel zurechenbar;
   - deterministischer Reassignment-Test vorhanden.

2. **C1.9 echter Production-Transportpfad**
   - Qualification-Modus nutzt lokale authentifizierte devp2p/RLPx-Peers;
   - gemessener Outbound-Pfad enthält tatsächlich `Peer.SendRawTransaction -> p2p.Send -> protoRW -> rlpxTransport`;
   - `loopback_microbenchmark` bleibt separat und ist nicht C1.9-qualifizierend;
   - C1.9b bleibt der getrennte controlled-Testnet Ingress-/Hook-Pfad.

3. **C1.10 fail-closed final-source binding**
   - fehlender Git-HEAD, dirty/ungebundener Source-Zustand, fehlender/abweichender finaler C1-Source-/Patch-Hash und nachträgliche Änderungen blockieren Freeze/Verify;
   - ein Loopback-only-C1.9-Report kann C1.10 nicht erfüllen.

Zusätzliche während REV2 gefundene Korrekturen:
- C1.9b capture: Rust-Borrow vor Lane-drain-ACK beendet; E0502 beseitigt ohne Borrowed-slot-Lifetime abzuschwächen.
- C1.8 churn test: negativer Owner wird garantiert vom echten Owner unterschieden; der alte feste Wert kollidierte in einem Zyklus.
- Typed EIP-2718 outbound: der echte Receiver zeigte die Envelope-Grenze; typed TX-Bytes werden als RLP string innerhalb der ETH Transactions list gewrappt, Legacy-list envelopes bleiben unverändert.
- Nach lokaler Integration wurde ein reiner Formatting-Defekt im Overlay-Generator gefunden: `c1_apply_geth_overlay.py` erzeugte in `peer.go` ein einzeiliges `if err != nil { return err }`. Die Generatorquelle wurde lokal korrigiert; danach erzeugt ein frischer Overlay-Lauf gofmt-sauberen Go-Code. Keine Semantikänderung.

Fables Integrations-/Regressionstand vor der lokalen Phase-Qualifikation war:

```text
INTEGRATED / COMPILED / REGRESSION CHECKED / NOT PHASE-QUALIFIED
Rust Debug/Release: je 122 unit tests PASS, 5 ignored
C1.8 stress: 8 PASS
doctests: 3 PASS
cargo clippy -D warnings: PASS
cargo fmt --check: PASS
C1.4 prefilter regression: PASS
Go fastlane race: PASS
overlay hook/outbound/chain ownership checks: PASS
full geth build: PASS
Python qualification tools: 20 PASS
integrated devp2p smoke: integrity PASS / not phase-eligible
```

Diese Fable-Ergebnisse allein wurden nicht als Gate-PASS übernommen; danach lief die lokale sequentielle Abnahme.

### Reopened/common C1.3/C1.4-Requalification — PASS

Frischer pinned-geth-Overlay-Lauf nach dem lokalen Generator-Formatting-Fix:

```text
C1_4_HOOK_ORDER_VERIFIED              PASS
C1_7_OUTBOUND_HOOKS_VERIFIED          PASS
C1_8_CHAIN_OWNERSHIP_VERIFIED         PASS
C1_GETH_OVERLAY_APPLIED               PASS
marker                                 DARKBOT_C1_FASTLANE_C1_7
RUSTFMT                                PASS
CLIPPY -D warnings                     PASS
C1.4 prefilter scope                   PASS
GOFMT                                  PASS
GO common eth/protocols/eth + eth + core PASS
GETH_BUILD                             PASS
C1_3_GO_RUST_LIFECYCLE_INTEROP         PASS
C1_4_GO_RUST_INGRESS_INTEROP           PASS
COMMON_REQUALIFICATION                 PASS
```

Damit sind die durch REV2 wieder geöffneten gemeinsamen C1.3/C1.4-Schnittstellen lokal nachqualifiziert.

### C1.4a Rev2-Rerun — PASS / GREEN_STRONG_HEADROOM

Separater Rev2-Rerun; die vorher akzeptierte C1.4a-Evidence wurde nicht überschrieben.

Evidence-Verzeichnis:

```text
bsc/native/evidence/c1/rev2-c14a-rerun-20261008/
```

Ergebnis:

```text
qualification samples        1000
max T_ingress -> L2          20,000 ns = 20 us
classification               GREEN_STRONG_HEADROOM
C1_4A_GATE_PASS              PASS
C1_4A_REV2_RERUN_PASS        PASS
```

Bedeutung unverändert: C1.4a beweist die lokale geth/Wire-V2/Rust-Feasibility, nicht tatsächliche Acquisition, Mainnet-Coverage oder `T0 -> T_buy`.

### C1.5 Rust HOT0 / zero-copy — LOCAL PASS

Ausgeführt:

```text
darkbot-fastlane debug suite     PASS
darkbot-fastlane release suite   PASS
c15_hot0_alloc                   PASS
C1_5_ACCEPTANCE_TESTS_PASS       PASS
```

Der entscheidende Allocation-Test:

```text
the_hot0_receive_path_allocates_nothing ... ok
```

Damit ist der lokal getestete HOT0-Receive-Pfad allocation-free. Borrowed-slot/guard- und IntentSlot-/Poller-Tests sind Bestandteil der bestandenen Suite.

### C1.6 M4 Development Profile — LOCAL PASS

Host-Evidence:

```text
OS                    macOS
arch                  aarch64
model                 Mac16,6
CPU                   Apple M4 Max
physical CPUs         16
logical CPUs          16
Performance CPUs      12
Efficiency CPUs       4
cache line            128 bytes
page size             16384 bytes
memory                137438953472 bytes
topology CRC32C       0xfc5501a8
```

Abnahme:

```text
C16_HOST_TESTS_PASS                  PASS
Profile S run A                      PASS
Profile S run B                      PASS
bit-identical/reproducible report    PASS
Profile P explicit refusal exit=2    PASS
Go C16 QoS race tests                PASS
C1_6_ACCEPTANCE_TESTS_PASS           PASS
```

Der Report markiert Topologie nur als initialen Experiment-Hinweis. QoS ist advisory, kein Core-Pinning. Der M4-C1-Host qualifiziert ausdrücklich nicht den späteren Linux-C3-Host.

### C1.7 local-geth outbound — LOCAL PASS

```text
C17_GO_OUTBOUND_PASS            PASS
C17_RUST_OUTBOUND_PASS          PASS
C1_7_ACCEPTANCE_TESTS_PASS      PASS
```

Damit sind unter den lokalen Tests insbesondere Slow-/Failure-/Ordering-/Worker-Lifecycle-Verhalten und der REV2-Reassignment-Fall abgedeckt: ein bereits queued BUY wird nicht still auf ein später neu zugewiesenes Peer-Ziel umgebogen.

### C1.8 concurrency/state-safety stress — LOCAL PASS

```text
C18_GO_FOCUSED_PASS             PASS
C18_GO_FULL_RACE_PASS           PASS
C18_RUST_DEBUG_PASS             PASS
C18_RUST_RELEASE_PASS           PASS
C1_8_CHAIN_OWNERSHIP_VERIFIED   PASS
C18_CHAIN_OWNERSHIP_PASS        PASS
C18_OWNERSHIP_DOC_PRESENT       PASS
C1_8_ACCEPTANCE_TESTS_PASS      PASS
```

Scope umfasst die geforderten Ownership-/Churn-/Reuse-/Restart-/Generation-/Wrap-/Full-/Corruption-/Oversize-/Stall-/Burst-Fälle sowie Chain-Lane-single-producer/chainmu-Verifikation und slow/failing local peers. Kein beobachteter Invariant-Verstoß im akzeptierten Testumfang.

### C1.9 full native latency run — MEASUREMENT COMPLETE / ELIGIBLE / FINAL GATE BINDING OPEN

Run-Konfiguration wurde vor der Messung festgelegt:

```text
purpose                         qualification
outbound_mode                   production_devp2p_selected_peer_handoff
peers                           4
warmup_per_peer                 32
samples_per_peer                2500
outbound_workers                2
warmup raw retained             128
qualification samples           10000
load model                      parallel_peers_one_outstanding_per_lane_with_explicit_warmup
```

Primary report:

```text
bsc/native/evidence/c1/c1-9-native-latency-20261008T225213.json
```

Report verdict fields:

```text
integrity                       PASS
c1_9_qualification_eligible     true
```

Ingress timings:

```text
T_packet_decoded -> T_ingress
p50        0 us
p95        1 us
p99        1 us
p99.9      1 us
p99.99     4 us
max        5 us

T_ingress -> L2
p50        81 us
p95        102 us
p99        116 us
p99.9      265 us
p99.99     401 us
max        418 us
misses >=1 ms  0
queue_depth_max 2
```

Outbound timings:

```text
egress commit -> coordinator observed
p50        3 us
p95        9 us
p99        14 us
p99.9      22 us
p99.99     64 us
max        113 us
misses >=1 ms 0

egress commit -> first selected-peer handoff
p50        15 us
p95        22 us
p99        29 us
p99.9      43 us
p99.99     139 us
max        157 us
misses >=1 ms 0

egress commit -> each selected-peer handoff
samples    20000
p50        17 us
p95        27 us
p99        34 us
p99.9      63 us
p99.99     162 us
max        184 us
misses >=1 ms 0

egress commit -> all selected-peer handoffs
p50        19 us
p95        29 us
p99        36 us
p99.9      72 us
p99.99     182 us
max        184 us
misses >=1 ms 0
```

Prepared integrated local path:

```text
T_ingress -> egress commit
p50        81 us
p95        103 us
p99        117 us
p99.9      265 us
p99.99     402 us
max        419 us
misses >=1 ms 0

T_ingress -> first selected-peer handoff
p50        97 us
p95        120 us
p99        136 us
p99.9      299 us
p99.99     424 us
max        438 us
misses >=1 ms 0
```

Observed failure/resource counters:

```text
peer_queue_full                           0
outbound_queue_full_hol                   0
peer_oversize                             0
hot0_dropped                              0
hot0_allocations_in_qualification_window  0
HOT0 receive-path locks                   none by construction; not instrumented
```

Measurement identity present in the report:

```text
darkbot_commit                 12c4616f9b24bf036d99cdd549e56ff2d448d372
pinned_geth_commit             cdb7548b5baacfdae92f9f63437d6456411665f3
config_manifest_sha256         70975678dfcccf73860b0f63f35bb384f2c31d70ae6f7f7b16b56f1066f5317e
topology_fingerprint_crc32c    0xfc5501a8
clock_id                       1
```

**Gate interpretation:** This is a valid, integrity-PASS, qualification-eligible C1.9 measurement over the actual local production devp2p handoff fixture. It demonstrates substantial local C1 latency headroom; the measured maximum `T_ingress -> first selected-peer handoff` was 438 us across 10,000 qualification samples.

However, **C1.9 is not yet promoted to final PASS in this ledger**. The measured working tree contained the integrated REV2 candidate on top of HEAD `12c4616...`, and the currently printed identity does not by itself bind a clean final committed C1 source tree/build artifact. The normative C1.9 exit requires exact source/build/config binding. Final promotion therefore waits for the final candidate source/build binding required before C1.10.

The 15-second raw market/acquisition capture from the production master plan is a separate later acquisition/`T0 -> T_buy` requirement. It is not substituted by this C1.9 local-native measurement.

### Current C1 state after this checkpoint

```text
reopened C1.3/C1.4 common regressions     PASS
C1.4a REV2 rerun                          PASS / GREEN_STRONG_HEADROOM
C1.5 HOT0/zero-copy                       LOCAL PASS
C1.6 M4 development profile               LOCAL PASS
C1.7 local-geth outbound                  LOCAL PASS
C1.8 concurrency/state-safety stress      LOCAL PASS
C1.9 native latency measurement           COMPLETE / integrity PASS / eligible
C1.9 final source/build/config binding     OPEN
C1.9b controlled Testnet hook/path         NEXT
C1.10 clean committed freeze               BLOCKED until C1.9 final bind + C1.9b PASS
C2                                         NOT STARTED
LIVE                                       BLOCKED
```

Required non-claims remain:

```text
T0_TO_T_BUY_LT_1MS = NOT_PROVEN
MAINNET_INGRESS_COVERAGE = NOT_PROVEN
PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED
LIVE = BLOCKED
```

The product requirement remains an actual successful acquisition with conservatively bounded original-producer `T0 -> T_buy < 1,000,000 ns`. Local send/handoff latency is not acquisition.


---

## C1.9b checkpoint — real Chapel peer/Fast-Lane ready (2026-10-09)

Purpose of this checkpoint: record the exact point reached before attaching the C1.9b capture and emitting controlled Testnet launches. This is **not yet a C1.9b measurement PASS**.

### What is now working

```text
C1.9b capture example build                 PASS
C1.9b analyzer unit tests                   PASS
host boot ID obtained                       PASS
G07 absolute monotonic handoff timestamp    LOCAL BUILD PASS
C1.9b explicit Testnet scope                LOCAL TEST PASS
Mainnet scope regression                    PASS
pinned bsc-geth build                       PASS
Chapel / chain 97 startup                   PASS
Fast-Lane arena creation                    PASS
official v1.7.8 Testnet config verified     PASS
real Chapel static-peer connection          PASS
```

The local G07 launcher was minimally extended to emit an absolute monotonic launch-handoff timestamp immediately after the prepared launch HTTPS `write_all+flush`. The modified launcher compiled successfully with `cargo build --release --offline`. A `--locked` build was not possible because the pre-existing Gate0 `Cargo.lock` is incomplete/stale relative to the manifest; the diagnostic offline build regenerated dependencies in the lockfile, then the original lockfile was restored. No lockfile change is accepted as evidence from this checkpoint.

The C1.4 ingress scope was also extended locally with an explicit qualification-only scope selector:

```text
unset / mainnet                         -> frozen Mainnet chain 56 scope
DARKBOT_FASTLANE_V2_SCOPE=c19b-testnet -> Chapel chain 97 scope
any other value                        -> startup refusal
```

The existing Mainnet scope remains chain 56 / Portal `0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0`. The explicit C1.9b Testnet scope is chain 97 / Portal `0x5beacaf7abcbb3ab280e80d007fd31fce26510e9`, with launch selector `0x8cb5772c`.

The following local checks passed after applying the overlay to the pinned geth checkout:

```text
C19B_SCOPE_DIFF_CHECK_PASS
C19B_OVERLAY_PASS
C19B_NEW_SCOPE_TEST_PASS
C19B_MAINNET_REGRESSION_PASS
MAINNET_SCOPE_56_PRESENT
C19B_SCOPE_97_PRESENT
C19B_EXPLICIT_OPT_IN_PRESENT
C1_9B_TESTNET_SCOPE_PASS
```

### Chapel / pinned-geth qualification setup

Pinned client:

```text
bnb-chain/bsc v1.7.8
commit cdb7548b5baacfdae92f9f63437d6456411665f3
network Chapel / chain ID 97
```

A fresh isolated Chapel datadir was started with the C1.9b Testnet scope. Initial discovery had zero peers because the pinned client has Mainnet bootnodes as the generic default and no built-in Chapel DNS discovery tree. The official **v1.7.8 `testnet.zip`** release asset was therefore used for its Testnet static-peer configuration.

Verified release asset:

```text
testnet.zip SHA256
0d9de8b7f1bafc36711eed80e97bf33f906fadc2ceb5457361645941d7ad19f7

StaticNodes = 4
```

With that official Testnet config, the real node reached:

```text
CHAIN_ID=0x61
PEERS=1
generation=202610090001
arena=/tmp/darkbot-c19b-fastlane/fastlane-v2-g00000000202610090001.mmap
arena_bytes=20454656
process_state=SN
C1_9B_REAL_TESTNET_PEERING_PASS
```

This proves the qualification infrastructure is now connected through:

```text
real Chapel peer
-> pinned bsc-geth ETH ingress
-> local C1.4 Fast-Lane runtime
-> generation-bound mmap arena
```

It does **not** yet prove controlled-launch observation latency because no C1.9b controlled launch has been emitted and analyzed in this checkpoint.

### Failed setup attempts that are not product failures

Two setup failures were diagnosed and removed:

1. `--authrpc.addr` was rejected by the built CLI; AuthRPC is unnecessary for C1.9b and was removed from the invocation.
2. A background geth was suspended by zsh job control (`SIGTTOU`, process state `T`) before startup. It produced a zero-byte log and no arena. The corrected detached invocation uses stdin `/dev/null` and redirected stdout/stderr.

A later Chapel run with correct detachment reached chain 97 and created the Fast-Lane arena but remained at `PEERS=0`; loading the official v1.7.8 Testnet static nodes resolved that and produced the real peer PASS above.

### Important source/evidence binding status

The G07 timestamp addition and C1.9b Testnet-scope addition are currently **local working-tree changes**. They are not represented by this documentation commit and are not yet the clean final C1 source commit. Therefore:

```text
C1.9 final source/build/config binding   OPEN
C1.9b infrastructure                     READY THROUGH REAL TESTNET PEER
C1.9b c19b_capture attached              NOT YET
C1.9b controlled launches                NOT YET
C1.9b analyzer report                    NOT YET
C1.9b gate                               NOT YET PASS
C1.10 clean committed freeze             BLOCKED
C2                                      NOT STARTED
LIVE                                    BLOCKED
```

### Immediate next step

Do not add another architecture branch. The next C1.9b operation is:

```text
attach c19b_capture to generation 202610090001
-> verify same host/boot/monotonic clock and clean capture startup
-> emit predeclared controlled Chapel launches
-> retain raw launch handoff + peer/source timestamps
-> run c19b_hook_path.py
-> record misses/lateness and broadcast-vs-pooled attribution
-> only then decide C1.9b PASS/FAIL
```

Required non-claims remain unchanged:

```text
T0_TO_T_BUY_LT_1MS = NOT_PROVEN
MAINNET_INGRESS_COVERAGE = NOT_PROVEN
PROVIDER_ORDERING/PLACEMENT = UNQUALIFIED
LIVE = BLOCKED
```

---

## 2026-10-16 — G0.7-Scope-Abgleich G0.1–G0.11 (Ergänzung; keine Neubewertung historischer Messungen)

Zweck: die für den kontrollierten G0.7-Versuch 1 maßgeblichen Gate-0-Nachweise zusammenführen, fehlende Ledger-Einträge nachtragen und offene Punkte exakt benennen. Historische Abschnitte oben bleiben unverändert; frühere Scopes behalten ihre eigenen Urteile.

### Scope von G0.7-Versuch 1

Nativer BNB-Flap-Self-Launch: Portal `0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0` (v5.24.0, Implementation laut Preflight-Baseline), Launch `newTokenV6`, BUY `swapExactInput`, Quote = natives BNB, gebunden durch den finalen DRY `20261015T222241Z` und den inaktiven v2-Entwurf. Der formale `PRIMARY_GATE0_SCOPE` im Kopf dieses Ledgers nennt weiter den historischen Modus; ein PASS aus Versuch 1 gilt **nur** für diesen Self-Launch-Scope (GATE0_TESTPLAN_SHORT §G0.11: keine Übertragung auf andere Modi/Venues).

### Nachgetragene Nachweise (bisher nicht im Ledger)

| Datum | Nachweis | Ergebnis | Fundstelle |
|---|---|---|---|
| 2026-10-11 | G0.6 final DRY (1-gwei-/0,001-BNB-Profil), Lauf `g06a_final_dry_20261011T110407Z` | PASS | GATE0_TESTPLAN_SHORT.md „G0.6 final dry status — 2026-10-11: PASS“; Rohdaten lokal unter `bsc/data/g06a_final_dry/` |
| 2026-10-15 | G0.6a-Final-DRY des Minimal-Kandidaten (Gas 0,05 gwei, BUY 0,000001 BNB, Head 127865760, Nonces 0/0), DRY-1..4, Null-Egress | PASS | `bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/` (versioniert, `SHA256SUMS`); `dry_result.json` SHA-256 `0efe78a7faf1d817fae3cbfd8893f858a4386a4c2770755465508c668eeded6f`, `preflight.json` `3eb5563d9e0bfea9ce8cc4e3750e3152d81a1b7ea7eff91de78091087bf77e69` |
| 2026-10-16 | Pre-Live-Readiness gegen die exakten DRY-Parameter | `PASS_READ_ONLY`, `FUNDING=PASS`, keine Drift | STATUS.md §2a |
| 2026-10-16 | Builder-Anforderungen read-only aufgefrischt | alle auffrischbaren expliziten Floors 50 000 000 wei; Blockzeit 450 ms | `bsc/native/evidence/g06a/builder_requirements_refresh_20261016T013943Z.json` (`ef4b72ff…`) |
| 2026-10-16 | G0.7-Härtung (frischer Send-Head, 48Club `maxTimestamp`, Einmal-Sperre, private atomare Evidence, Env-Isolation, keine Panics nach der Freigabe) | implementiert, offline getestet; signierte Bytes unverändert (`transactions.rs` byte-gleich zu `565d811`) | Commits `dda7c9b`, `51e2a5e`, `9ab5fe6`; G07_ATTEMPT1_AUTHORIZATION_REVIEW.md |
| 2026-10-16 | Builder-Probe für Pre-Arm-Checks 6/10 (11 nicht ausführbare Fälle) | vorbereitet, **nicht gesendet**; Plan-SHA-256 `77957205fd844dea51d4eb97a4b12c345978952fafc91c95cefec75d28f9abf5` wartet auf Freigabe | G07_BUILDER_PROBE_APPROVAL.md |

Bindungsprüfung 2026-10-16 (direkt): alle sechs DRY-Dateien = `SHA256SUMS`; Builder-Policy `ca76a93c…`, Witness-Quellvertrag `78660c7d…`, aktiver v2 `026b534f…` (unapproved, `attempts.maximum` 4 — **keine** Autorisierung), Entwurf `902d4cb9…` (`attempts.maximum` 1, inaktiv). Seit dem DRY keine signaturrelevante Code-Änderung; ein neuer DRY ist nicht nötig. Chain-seitige Eingaben (Nonces, Portal-Zustand, Gaspreis) prüft G0.7 zur Laufzeit erneut gegen die Baseline und die DRY-Hashes.

### Status je Gate

| Gate | G0.7-Self-Launch-Scope | Andere Scopes | Belege |
|---|---|---|---|
| G0.1 | Portal/Version/Implementation eingefroren. **Offen:** numerischer Akzeptanzvertrag = v2-Entwurf `902d4cb9…` (1 Versuch, Budget 481 163 500 000 000 wei) noch nicht freigegeben | OpenFour-Fixture und Modus-Freeze, Four-Classic-Modus-Freeze offen | Abschnitte 2026-10-02 (v2 als einzige Autorität), STATUS §2a, Review §1 |
| G0.2 | Felder belegt: Native-Curve-20-Crosscheck PASS (Abschnitt 2026-10-02) plus finaler DRY (Token, Empfänger, BUY-Wert, `minOutput`, Nonces, Gas, Calldata; Invalidierung bei geändertem Nonce/State/Portal prüft G0.7 zur Laufzeit) | `G0.2_FLAP_PASS` gilt nur für den historischen DOGE-Modus; andere Modi offen | `G0.2_FLAP_PASS`-Abschnitt, Native-Crosscheck 2026-10-02, DRY 20261015 |
| G0.3 | Exakte Launch→BUY-Semantik der eingefrorenen Bytes: DRY-1 (Bytes/Signer), DRY-2 (unabhängige EVM-Sequenz, Gas 5 251 225 / 334 654), positiver `minOutput` | `G0.3_PASS_PINNED_OFFLINE_SCOPE` historischer Fall; neue Wallets/Beträge/Pfade brauchen eigene Fälle | `G0.3_PASS…`-Abschnitt, DRY 20261015 |
| G0.4 | Für einen Self-Launch nicht erforderlich (kein externes Signal zu erkennen; GATE0_TESTPLAN_SHORT: Self-Launch und externe Coverage getrennt) | **PARTIAL** — kein kanonischer Nenner, Mainnet-Ingress-Coverage NOT_PROVEN | `G0.4_PARTIAL_NO_FROZEN_SCOPE_DENOMINATOR` |
| G0.5 | **PASS** für vorbereitete/presignierte Self-Launch-Transportgrenze (48Club p99 0,584 ms, BlockRazor p99 0,527 ms, je 0/100 > 1 ms). Rest: Das gehärtete Binary wartet vor der Freigabe auf den nächsten Block — eine bewusste Wartezeit, nicht neu vermessen | externe Launches: Decode/Quote/Sign-Pfad diagnostisch p99 1,64/2,22 ms (ADAPT); C1-Pfad nicht an Execution angebunden | Abschnitt 2026-10-02 (G0.5 PASS), STATUS §1 |
| G0.6 / G0.6a | Final-DRY **PASS**; Bindungen intakt. **UNQUALIFIED:** Builder-Methode/Format/Ablauf/Preis bei Einreichung (Check 6) und `noMerge`/`positionFirst` (Check 10) bis zur Probe; Annahme gültiger Bundles, Ordering, Inclusion und ausreichender Gaspreis bleiben auch danach UNQUALIFIED | Ingress×Inclusion-Matrix und Concurrency nur synthetisch/historisch: PARTIAL | DRY 20261015, G07_BUILDER_PROBE_APPROVAL.md |
| G0.7 | **NICHT AUSGEFÜHRT / NICHT AUTORISIERT.** Guard `UNARMED` | — | STATUS §3 |
| G0.8 | Werkzeuge bauen, verweigern ohne Eingaben; G0.7-Evidence G0.8-lesbar (Kompatibilitätstest) | Testnet-Acquisition/Ordering bewiesen (2026-10-01) | `qualification/tests/g08_reads_g07_evidence.rs` |
| G0.9–G0.11 | nach G0.8. **Bekannt:** Producer-Witness `source_ready=false` ⇒ `T0 → T_buy` bleibt UNKNOWN, das Timing-Kriterium kann in Versuch 1 nicht bestehen; G0.9 ohne P2P-Vergleich voraussichtlich UNKNOWN | — | Witness-Quellvertrag, Review §3 |

### Offen bis zum Versuch (Reihenfolge)

1. Builder-Probe: Freigabe genau des Plan-SHA-256 `77957205…` durch den Nutzer, ein Lauf, Auswertung nach G07_BUILDER_PROBE_APPROVAL.md §4.
2. Pre-Arm-Checks 6/10 im v2-Entwurf nach dem Probe-Ergebnis setzen (neuer Entwurfs-Hash).
3. Nutzerentscheidungen: finale Entwurfsbytes, Restrisiken (null Gas-Spielraum, ein nicht eingeschlossenes Bundle verbraucht den einzigen Versuch, Timing UNKNOWN), Budget und Wallet-Bestände.
4. Builder-Refresh höchstens 24 h vor dem Scharfschalten erneuern (der Refresh vom 2026-10-16T01:39Z gilt bis 2026-10-17T01:39Z).
5. Readiness unmittelbar vor dem Scharfschalten; NTP-Uhr.
6. Scharfschalt-Commit nur mit ausdrücklicher Echtgeldfreigabe; genau ein Lauf; sofort G0.8; Entschärf-Commit.

### Nicht behauptet

```text
T0_TO_T_BUY_LT_1MS            = NOT_PROVEN (source_ready=false)
REAL_MAINNET_BUY              = NOT_EXECUTED
BUILDER_ACCEPTANCE/ORDERING   = UNQUALIFIED
MAINNET_INGRESS_COVERAGE      = NOT_PROVEN
LIVE                          = BLOCKED
```

Unverändert gelassen: `ACCEPTANCE_MAP.json` (versionierter Vertrag; seine Zähler `323` und `next_step` sind älter als STATUS.md, werden aber nicht nachträglich umgeschrieben).

---

## 2026-10-16 — Builder-Probe für G0.7-Pre-Arm-Checks 6/10: Ergebnis

Einmaliger, vom Nutzer freigegebener Lauf des Plans `77957205fd844dea51d4eb97a4b12c345978952fafc91c95cefec75d28f9abf5` (11 nicht ausführbare Bundles: Nonce 2^40, Wegwerf-Konten mit Guthaben 0; Freigabevorlage G07_BUILDER_PROBE_APPROVAL.md), 2026-10-16 18:34:11–18:34:49Z, Exit 0, Status `COMPLETE`. Evidence: `bsc/native/evidence/g06a/builder_probe_20261016T183411Z/probe.evidence.json`, SHA-256 `5636c463d6247b90d17d9bc977a6e50aae499434a17d241f16f05b87ba95cc7f`.

| Fall | Antwort (roh, gekürzt) | Klasse | Lesart |
|---|---|---|---|
| 1 `48club-g07-shape` | `-32000 nonce too high … tx: 1099511627776 state: 0` | `EXECUTION_REJECTED` | Methode und Format geparst; da 48Club die Ablaufwerte vor den Transaktionen prüft (Fälle 4/5), haben die G0.7-Werte HEAD+2/MAXTS die Einreichungsprüfung bestanden |
| 2 `48club-no-maxtimestamp` | `nonce too high` | `EXECUTION_REJECTED` | `maxTimestamp` ist bei Einreichung optional |
| 3 `48club-maxtimestamp-minus-one` | `nonce too high` (Phase 950 ms) | `EXECUTION_REJECTED` | wie erwartet ohne Aussage (Phase ≥ 100 ms) |
| 4 `48club-past-block` | `-38000 the maxBlockNumber should not be less than currentBlockNum` | `EXPIRY_REJECTED` | 48Club prüft `maxBlockNumber` bei Einreichung |
| 5 `48club-past-timestamp` | `-38000 the maxTimestamp(1792175670) should not be less than currentBlockTimestamp(1792175672)` | `EXPIRY_REJECTED` | 48Club prüft `maxTimestamp ≥` aktuelle Blockzeit |
| 6 `48club-underpriced-control` | `nonce too high` (1 wei) | `EXECUTION_REJECTED` | keine Preisschranke vor der Simulation; Preis UNQUALIFIED |
| 7 `48club-nomerge-type-control` | `-32602 … cannot unmarshal string into Go struct field SendBundleArgs.noMerge of type bool` | `FORMAT_REJECTED` | Feld erkannt (typisiertes bool); Wirkung UNQUALIFIED |
| 8 `48club-positionfirst-type-control` | `-32602 … SendBundleArgs.positionFirst of type bool` | `FORMAT_REJECTED` | Feld erkannt (typisiertes bool); Wirkung UNQUALIFIED |
| 9 `blockrazor-g07-shape` | `-32000 nonce too high … tx: 1099511627776 state: 0` | `EXECUTION_REJECTED` | Methode, Format und Token angenommen; HEAD+2 hat die Einreichungsprüfung bestanden (Fall 10) |
| 10 `blockrazor-past-block` | `-38000 the maxBlockNumber should be larger than currentBlockNum` | `EXPIRY_REJECTED` | BlockRazor prüft `maxBlockNumber > ` aktueller Block bei Einreichung |
| 11 `blockrazor-underpriced-control` | `-38000 gasprice must be at least 0.05 gwei` | `PRICE_REJECTED` | Preisschranke vor der Simulation genau 0,05 gwei; G0.7 mit 0,05 gwei besteht sie (Fall 9), ohne Spielraum |

```text
CHECK_6_METHOD_FORMAT_EXPIRY_AT_SUBMISSION   = PASS (48Club, BlockRazor)
BLOCKRAZOR_SUBMISSION_GAS_FLOOR             = 0.05 gwei exactly; G0.7 price passes (zero margin)
48CLUB_SUBMISSION_PRICE_CHECK               = NOT OBSERVED -> price acceptance UNQUALIFIED
CHECK_10_NOMERGE_POSITIONFIRST              = FIELDS RECOGNIZED (typed bool); effect UNQUALIFIED
BUILDING_TIME_ENFORCEMENT / INCLUSION        = UNQUALIFIED (not observable with non-executable bundles)
REAL_MAINNET_BUY                             = NOT_EXECUTED
```

Folgen: v2-Entwurf (inaktiv) auf `1b66718d476e938fc6c936cf3c0b4d49ebc59cc2765c043600130b05de8ccf42` fortgeschrieben (Checks 6/7/10); die verbleibenden UNQUALIFIED-Punkte brauchen die ausdrückliche Annahme des Nutzers vor einer Freigabe.

## 2026-10-17 — G0.7/G0.8-Binärmigration M8-Endabnahme und Read-only-Readiness (Ergänzung)

- Binärmigration nach Roadmap-Commit `e19fdde`: M1–M8 offline IMPLEMENTED/VERIFIED (Details: `docs/G07_G08_BINARY_MIGRATION_PLAN.md`, Abschnitt „M8 final acceptance“). Workspace-Check mit G0.7-Features PASS (0 Warnungen); 129 gezielte Tests PASS. Aktive Autorität: binärer v3-Vorschlag `3966b227…6a`, Guard `UNARMED`. Historische JSON-Nachweise unverändert (u. a. Probe-Evidence `5636c463…`, DRY `0efe78a7…`).
- **Read-only-Readiness 2026-10-17T10:12:15Z** (öffentlicher RPC, keine Secrets, keine Signatur, kein Builder): Offline-Modus BLOCKED (8 PASS / 6 BLOCKED); aktueller Modus **STOP wegen Deployment-Drift**: `portal_version` v5.24.0 → v5.25.0; `portal_implementation` `0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20` → `0xe13227370996f892a56d2ff4a30f6026887b48fb`; Implementation-Code-SHA-256 `2ac893fc…` → `42c91172…`. Unverändert: Proxy-Code, TAX_V3-Implementation, Native-Quote-Konfiguration, Fees, Tax-Flag. Evidence: `bsc/native/evidence/g07/readiness_20261017T101215Z/` (DBREC001, abgeleitete Ansichten, `SHA256SUMS`).
- Folge: Der gebundene G0.6a-DRY (Head 127865760, Portal v5.24.0) und seine signierten Bytes sind für den aktuellen Mainnet nicht mehr qualifiziert; G0.7-Versuch 1 darf darauf nicht scharfgeschaltet werden. G0.7 und G0.8 NOT_EXECUTED; LIVE BLOCKED.

