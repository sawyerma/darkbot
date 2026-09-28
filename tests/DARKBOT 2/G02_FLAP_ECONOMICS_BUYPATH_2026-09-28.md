# G0.2 Flap / BSC — Economics und BUY-Pfade — 2026-09-28

**Entscheidung: G0.2_FLAP_PARTIAL. G0.3 bleibt gesperrt.**

Produktionsrepo: `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`, unverändert auf `main`, HEAD `2befab8`. Kein Pull nötig, weil der erwartete Stand bereits vorlag. Ausschließlich lokale Evidence, Offline-Analyse und lesende Netzwerkabfragen. Keine Keys, Signaturen, Transaktionssendungen, Mainnet-BUYs, Branches oder Rust-Harnesses.

Die Anforderungen sind streng bewertet: `FAIL` bedeutet in der folgenden Tabelle, dass der **vollständige geforderte Nachweis** nicht vorliegt. Es bedeutet nicht, dass Flap unmöglich oder die Chain-Transaktion fehlgeschlagen ist. Nachgewiesene Teilresultate bleiben separat erhalten.

## Tests und Ergebnis

| Test | Ergebnis | Befund |
|---|---|---|
1 | FAIL / Teilnachweise bestanden | Router-Envelope verlustfrei dekodiert; sämtliche Receipt-Logs gespeichert, Token-Flows und finaler Empfänger belegt. Command-Dispatcher und konkreter interner Portal-Aufruf ohne Trace/gebundenen Source noch offen. |
2 | FAIL / Teilnachweise bestanden | Direkter `swapExactInput` vollständig dekodiert, Selector unabhängig bestätigt, Output und sichtbare Gebühren belegt. Vollständige interne Konvertierungs-/Rundungssemantik fehlt. |
3 | PASS | Vergleich unten unterscheidet tatsächliche Pfade und markiert jede offene Semantik. Keine unbelegte Rangfolge. |
4 | FAIL / Teilnachweise bestanden | Post-launch State numerisch exakt aus historischer Evidence rekonstruiert; Ableitung aller Parameter ausschließlich aus Pending-Calldata + vorher gefrorenem State fehlt. |
5 | FAIL / Teilnachweise bestanden | V2-Konvertierung numerisch konsistent; unabhängiger Pre-State fehlt. Direkte V3-Konvertierung nicht lokal vollständig simuliert. |
6 | FAIL / Teilnachweise bestanden | Alle sichtbaren DOGE-Abzüge abgeglichen; Rundung und Exemption nicht gegen historische Implementierung bewiesen. |
7 | FAIL / Teilnachweise bestanden | Sieben Curve-Outputs mit beobachteten Parametern/Abzügen exakt reproduziert. Vollständige unabhängige First-BUY-Reproduktion aus zulässigen Eingabedaten fehlt. |
8 | PASS: Klassifikation UNKNOWN | Wirtschaftlich begründetes `minOutputAmount` bleibt unbewiesen. |
9 | PASS | 16 Felder/Zustände klassifiziert, inklusive Signature-Bindung und Invalidierung. |
10 | PASS: PARTIAL | Keine Freigabe von G0.3; genau ein nächster technischer Test am Ende. |

CREATE2 bleibt PASS. Als Autorität dient jetzt **ethers 6.15.0**, installiert mit festem Package-Lock, kein handgeschriebener Keccak. V6- und direkte BUY-Calldata werden ABI-dekodiert und bytegleich zurückkodiert. `swapExactInput((address,address,uint256,uint256,bytes))` ergibt `0xef7ec2e7`; die unabhängige BSC-`web3_sha3`-Abfrage ergibt denselben Selector. Ethers bestätigt auch die Tokenadresse `0xabe1fe16cf6bc48c9524713054efe6e954b17777`.

## 1. Erster externer Router-BUY

- TX: `0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800`
- Block/Index: `124415358 / 15`; Receipt status `1`.
- From und finaler Tokenempfänger: `0xe6b55031e42587e540c629807a059ff7d1a849b6`.
- To/Router: `0x43dd5eebeebead726fbc84e2511567d9827ae1bb`.
- Native value: `30000000000000000` Wei = 0,03 BNB.
- Selector: `0x24856bc3`. Bytegleiches ABI-Envelope: `execute(bytes,bytes[])`.
- Commands: `0x0b090816`, vier dynamische Inputs. Der Contracttyp ist **nicht source-verifiziert**; ein passendes ABI-Envelope beweist keinen bestimmten Universal-Router-Fork.
- Endgültiger Erwerb: `6243568333969093909994622` Token-Base-Units.
- Portal-Event-Buyer ist der Router; finaler Empfänger ist die EOA oben. Beide dürfen nicht verwechselt werden.

Vollständige Original-Calldata, `from/to/value`, Command-Bytes und sämtliche Input-Wörter: `gate0-evidence/2026-09-28-g02-economics/abi_decode.json`. Vollständiges Receipt inklusive aller unbenannten Logs: `router_receipt.json`; dekodierte Logs: `decoded_receipts.json`. Keine unbekannten Logs wurden entfernt.

Die vier Command-Payloads enthalten faktisch:

| Command | Dekodierbare Daten | Grenze |
|---|---|---|
| `0x0b` | Null-Adresse, `0x1e4a7882aa4390fce8e9c0f88327305aa6ed9a72`, Wert `50` | Fee-Event und Budgetdifferenz passen zu 50 bps; genaue Dispatcher-Regel nicht source-gebunden. |
| `0x09` | `29850000000000000` | Entspricht exakt dem WBNB-Deposit; Commandname nicht aus fremdem Router übernehmen. |
| `0x08` | Werte `2`, `2^255`, `0`; Pfad WBNB / `9975` / DOGE; Pool `0xac109c8025f272414fd9e2faa805a583708a017f` | Sentinel-/Recipient-Regeln und Bedeutung des Multiplikators noch nicht gegen Dispatcher-Code bewiesen. |
| `0x16` | DOGE, neuer Token, `2^255`, `0`, `1` | Passt zu anschließender Flap-Ausführung und Weiterleitung; interner Selector und Slippage-Regel noch offen. |

**Belegte Asset-Flows (kein behaupteter vollständiger Call-Trace):**

```text
EOA -- 0,03 BNB msg.value --> Router
Router: Fee-Event 0,00015 BNB
Router: Deposit 0,02985 BNB -> WBNB
Router -- 29850000000000000 WBNB-Units --> Pool 0xac109c...
Pool   -- 24051320534 DOGE-Units --> Router
Router -- 24051320534 DOGE-Units --> Portal
Portal -- 6243568333969093909994622 neue Token-Units --> Router
Router -- dieselbe Tokenmenge --> EOA
```

DOGE-Approvals sind im Receipt sichtbar: Router → Portal zuerst 0, dann `24051320534`, nach Nutzung wieder 0. Dieser historische native-input Pfad zeigt keine vorab erforderliche DOGE-Approval der EOA. Das ist keine generelle Aussage über jeden Router-Modus.

Der Router-Runtime enthält den Literal-Selector `ef7ec2e7`. Zusammen mit Receipt/Calldata ist das ein Hinweis, **kein Beweis**, dass gerade Command `0x16` diesen Selector verwendet hat. `Router → Portal.swapExactInput` bleibt daher offen; auch ein anderer kompatibler Portal-Einstieg ist nicht ausgeschlossen.

## 2. Direkter Portal-BUY

TX: `0x8aff738f6fff1302e057609661acf39eb2ebbf8ac13f1bd69dbc0835bb570f6b`; Block/Index `124415360 / 17`; fünfter externer BUY; status `1`.

| Feld | Wert |
|---|---|
| from / Recipient | `0xb6864c017e9d9d89012d4f90ee382bf1461e4c90` |
| to | `0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0` |
| selector | `0xef7ec2e7` |
| inputToken | `0x0000000000000000000000000000000000000000` |
| outputToken | `0xabe1fE16cf6Bc48c9524713054efe6e954b17777` |
| inputAmount | `150000000000000000` |
| minOutputAmount | `6000000000000000000000000` |
| permitData | `0x` |
| msg.value | `150000000000000000` Wei = 0,15 BNB |
| tatsächlicher Transfer an Käufer | `28438305464314215025968609` Token-Base-Units |

Das `ExactInputParams`-Tuple enthält keinen separaten Recipient. In dieser direkten Fixture geht der Output nachweislich an `tx.from`. Bei einem internen Contract-Aufruf ist der Portal-Aufrufer relevant; EOA- und Router-Empfänger sind daher getrennt zu prüfen.

Die nativen BNB werden in WBNB gewrappt. Belegte WBNB-Transfers:

```text
Portal -> 0x9818eb714a8c0955c5915c368b2766d950275d24
       -> 0x13f4ea83d0bd40e75c8222255bc855a974568dd4
       -> 0xce6160bb594fc055c943f59de92cee30b8c6b32c
```

Der letzte Pool zahlt `120878323272` DOGE-Units direkt an das Portal. Sein Event entspricht bytegenau dem offiziellen PancakeSwap-V3-Swap-Interface; Pool-Getter liefern DOGE als token0, WBNB als token1, Factory `0x0bfbcf9fa4f9c56b0f40a671ad40e0805a091865`. Aktuelle `fee()` = `2500` ppm und `tickSpacing()` = `50`. Aktuelle Getter ersetzen keinen historischen State-Snapshot.

V3-Swap-Event:

```json
{
  "sender": "0x13f4EA83D0bd40E75C8222255bc855a974568Dd4",
  "recipient": "0xe2cE6ab80874Fa9Fa2aAE65D277Dd6B8e65C9De0",
  "amount0": "-120878323272",
  "amount1": "150000000000000000",
  "sqrtPriceX96": "88148509295527537166464639469405",
  "liquidity": "3883217927027149621",
  "tick": "140295",
  "protocolFeesToken0": "0",
  "protocolFeesToken1": "120000000000000"
}
```

Der ausgewiesene `protocolFeesToken1`-Betrag ist **kein Beleg für die gesamten Swap-Kosten**. LP-Fee, Preiswirkung, Ticks/Liquidität und zusätzliche Converter-Regeln sind getrennt zu rekonstruieren. Der volle V3-Output wurde hier nicht unabhängig simuliert.

## 3. Vergleich

| Merkmal | Earliest Router BUY | Direct Portal BUY |
|---|---|---|
| Destination | `0x43dd5eebeebead726fbc84e2511567d9827ae1bb` | Portal `0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0` |
| Selector | `0x24856bc3` | `0xef7ec2e7` |
| Native BNB input | 0,03 | 0,15 |
| DOGE conversion | WBNB/DOGE-Pool `0xac109c...`, V2-Event | Pfad über `0x9818...`, `0x13f4...`, Pool `0xce616...`, V3-Event |
| Portal call | Portal-Ereignisse und Transfers belegt; interner Selector UNKNOWN | Top-Level `swapExactInput` bewiesen |
| Additional calls | Wrap, Swap, Approvals, Tokenweiterleitung anhand Logs; vollständiger Calltree UNKNOWN | Wrap/Converter/Swap nach Tokenflows; vollständiger Calltree UNKNOWN |
| Required pre-state | V2-Reserves, Dispatcher-Konfiguration, Portal/Curve/Tax/Extension | V3-Preis, Liquidität, Ticks, Converter-Konfiguration, Portal/Curve/Tax/Extension |
| Required approvals | Router erteilt und löscht DOGE→Portal-Approval während Fixture | Native-input; kein Nutzer-DOGE-Permit in Calldata; interne Approval-/Callback-Regeln offen |
| Required calldata fields | Commands + Inputs inkl. Pfad/Pool/Token/Betrag/Sentinels | inputToken, outputToken, inputAmount, minOutputAmount, permitData |
| Recipient | Portal zahlt Router; Router zahlt EOA | Portal zahlt direkte EOA |
| Slippage field | Null-Wörter vorhanden; exakte Command-Semantik offen | minOutputAmount = 6000000000000000000000000 |
| Fees | zusätzliche native Router-Fee + Pool + Portal + Tax | Pool + Portal + Tax; keine zusätzliche native Router-Fee im bekannten Envelope |
| Token tax | DOGE-Abzug; voller Token-Transfer zum Router und zur EOA | DOGE-Abzug; voller Token-Transfer zur EOA |
| Output calculation | V2 + Curve teilweise konditional reproduziert | Curve konditional reproduziert; V3-Quote offen |
| Precomputable from pending launch? | Adresse ja; vollständige Economics UNKNOWN | Adresse ja; vollständige Economics UNKNOWN |
| Extra hot-path dependency? | Synchroner RPC nicht erlaubt; Vollständigkeit lokal gehaltener State-Daten noch offen | Gleiches Prinzip; zusätzlich V3-Ticks/Converter-Modus zu beweisen |

Die Beträge und Vorzustände unterscheiden sich. Aus dieser Tabelle ergibt sich **kein fairer Kosten-/Geschwindigkeitssieger**.

## 4. Initial-State direkt nach Launch

DOGE-Contract wurde erneut gelesen: `Dogecoin`, `DOGE`, `8`. Quote und Dividend-Token sind beide DOGE. Im normativen Repo steht in einem G0.1-Bullet weiterhin `dividendToken = WBNB`; dieser Bullet widerspricht der Fixture und muss separat korrigiert werden. Produktionsrepo wurde nicht geändert.

| Größe | Rekonstruierter Wert | Herkunft / Grenze |
|---|---:|---|
| quoteAmt | 80574062404 | V6-Calldata, DOGE-Base-Units |
| Reserve nach Creator-BUY | 78962581156 | Input minus zwei beobachtete DOGE-Abzüge |
| Circulating supply | 21453245135239692933310825 | Lokal gerechnet, gegen Launch-Transfer/Event geprüft |
| r | 39956880715754427756773 | Historisches TokenCurveSetV2; vor Signal verfügbare Herleitung noch unbewiesen |
| h | 107036751000000000000000000 | Gleiches |
| k | 44233735407663336217722200164523 | Gleiches |
| dex threshold | 800000000000000000000000000 | Launch-Event |
| buyTaxRate / sellTaxRate | 100 / 100 bps | Calldata; Rundungs-/Exemption-Regeln noch offen |
| nativeToQuoteSwapEnabled | spätere native BUYs erfolgreich; latest Lens = true | Historischer Konfigurations-Snapshot fehlt |
| aktuelle Quote-Konfiguration | `[1, 41, 41, 7, 0]` | Latest-only; keine historische Gleichsetzung |

Alle 26 V6-Felder wurden mit ethers verlustfrei dekodiert, einschließlich `extensionID`, `extensionData`, `beneficiary`, `dividendToken`, Verteilung und Dauer. Sie stehen vollständig in `initial_state_reconstruction.json`. Der Extension-Code bzw. seine historischen Registry-Bindungen sind noch nicht bewiesen. Deswegen wäre es falsch, alle nicht in der Curve-Gleichung auftauchenden Felder als irrelevant zu behandeln.

**Versionslücke:** Das gespeicherte offizielle Interface definiert NativeToQuoteSwapType nur für 0–6; die aktuelle Chain liefert **7**. Ebenso liefert `getTokenV7` im heutigen State an der dokumentierten Versionsposition `5`, während die V6-Launch-Calldata `6` enthält. Die Bedeutung dieser Abweichungen ist offen. Die aktuelle Lens-Ausgabe wird nicht als Initial-State benutzt.

Verwendete konditionale Curve-Gleichung:

```text
scale = 10^(18 - DOGE_decimals) = 10^10
S(q) = 10^27 + h - ceil(k * 10^18 / (r + q * scale))
netReserve = previousReserve + DOGE_input - protocolFee - tax
output = S(netReserve) - previousCirculatingSupply
```

Die Form ist im gespeicherten `libraries_Curve.sol` nachvollziehbar. Dieser stammt aus einem **nicht als Live-Deployment verifizierten Repository-Spiegel**; sein Commit und SHA-256 sind gespeichert. Die Übereinstimmung mit Receipts ist starke arithmetische Evidence, aber kein Ersatz für die Bindung an historischen Runtime-Code.

## 5. Konvertierung und Gebühren

Router-Fall:

- Native Router-Fee: `150000000000000` Wei, aus Fee-Event und Budgetdifferenz bestätigt (50 bps in diesem Fall).
- WBNB in V2-Pool: `29850000000000000` Wei.
- Pfad enthält Multiplikator `9975`. Das dazu passende V2-Modell entspricht 25/10000; Modell-Fee `74625000000000` Wei. Die Dispatcher-Bedeutung ist noch nicht source-gebunden.
- Rekonstruierte Pre-Reserves: DOGE `523620527542026`, WBNB `648208958251729359290`.
- Formel `floor(in*9975*reserveOut / (reserveIn*10000 + in*9975))` ergibt exakt `24051320534` DOGE-Units.

**Nicht als unabhängigen Quote-Nachweis zählen:** Diese Pre-Reserves wurden aus dem Post-Sync und dem Swap desselben BUYs zurückgerechnet. Damit steckt der beobachtete Output in der Reserve-Rekonstruktion. Eine frühere unabhängige Reserve-Evidence fehlt. Der separate historische Logversuch scheiterte mit `limit exceeded`.

Sichtbare Portal-Fee und Tax sind zwei getrennte DOGE-Abzüge. Das vollständige Transfer-Ledger prüft jeweils Eingang minus alle beobachteten Ausgänge, statt nur das TokenBought-`fee`-Feld als Gesamtkosten zu verwenden.

| Kauf | DOGE Input | Protocol Fee (beobachtet) | Tax (beobachtet) | Tax minus floor(Input/100) | Curve-Output-Abweichung mit beobachteten Abzügen |
|---|---:|---:|---:|---:|---:|
| Creator | 80574062404 | 805740624 | 805740624 | 0 | 0 |
| External 1 | 24051320534 | 240513205 | 240513205 | 0 | 0 |
| External 2 | 16117839140 | 161178391 | 161178391 | 0 | 0 |
| External 3 | 111646524369 | 1116465243 | 1116465244 | 1 | 0 |
| External 4 | 10476515617 | 104765156 | 104765156 | 0 | 0 |
| External 5 | 120878323272 | 1208783232 | 1208783233 | 1 | 0 |
| External 6 | 67309479679 | 673094796 | 673094797 | 1 | 0 |

Naive Regel „Protocol Fee und Tax beide `floor(input/100)`“ ist **widerlegt**. Beim direkten Referenz-BUY produziert sie mit dem richtigen Vorzustand `233530901372890` Token-Base-Units zu viel. Die Ursache lässt sich auf eine DOGE-Base-Unit Tax-Differenz eingrenzen; die ursächliche Contract-Rundungsregel ist noch offen. Es wird keine passende Round-to-nearest-Regel aus wenigen Beispielen erfunden.

Die Tax wurde sichtbar in DOGE abgezogen. Die hier beobachteten Flap-Token-Transfers belegen keinen zusätzlichen 1-%-Tokenabzug auf den Router→EOA-Transfer. Ob hierfür eine Exemption oder die allgemeine Token-Transferregel verantwortlich ist, bleibt offen.

### Gas und native Zahlung

| Pfad | Gas used | Effective gas price (Wei) | Gas cost (Wei) | msg.value + Gas (Wei) |
|---|---:|---:|---:|---:|
| router | 507282 | 600000000 | 304369200000000 | 30304369200000000 |
| direct | 456336 | 310000000 | 141464160000000 | 150141464160000000 |

`msg.value + Gas` ist die Bruttobelastung ohne eventuelle interne native Rückflüsse. Mangels Value-Trace/Balancediff wird sie nicht als abschließend bewiesener Netto-BNB-Debit bezeichnet.

## 6. Exakter Output und minOutputAmount

Für den **ersten externen BUY**:

```text
ACTUAL_ACQUIRED_OUTPUT                = 6243568333969093909994622
CONDITIONAL_CURVE_EXPECTED_OUTPUT     = 6243568333969093909994622
CONDITIONAL_DIFFERENCE                = 0
FULL_PENDING_ONLY_EXPECTED_OUTPUT    = UNKNOWN
FULL_PENDING_ONLY_DIFFERENCE         = UNKNOWN
```

Dasselbe konditionale Curve-Modell reproduziert Creator und alle sechs externen Käufe exakt. Es setzt aber beobachtete Curve-Konstanten, DOGE-Zuflüsse und tatsächliche Gebühren voraus. Der geforderte Beweis aus `Launch-Calldata + vorher gefrorenem State/Code + BUY-Calldata` ist deshalb **noch nicht bestanden**.

**minOutputAmount = UNKNOWN.** Fehlende Eingaben: belegte historische Curve-/Extension-/Fee-Semantik sowie unabhängige Pool-/Converter-State-Daten und Invalidierung bei intervenierenden Transaktionen. Prinzipiell kann die Quote signalzeitlich lokal berechnet werden, wenn diese Daten vollständig und korrekt vorliegen. Das ist eine bedingte Architektur-Möglichkeit, noch keine PASS-Klassifikation.

`minOutputAmount = 0` ist als Calldata darstellbar; in der älteren vorhandenen direkten BUY-Fixture eines **anderen** Tokens war der Wert 0 erfolgreich. Das beweist keine wirtschaftlich brauchbare Ausführung für diesen Scope. Es setzt die Slippage-Untergrenze außer Kraft und ist keine Produktionsentscheidung oder Lösung der G0.2-Economics-Lücke. Beim ersten Router-BUY sind die Null-Wörter ohne vollständig verifizierte Command-Semantik nicht automatisch als Portal-Minimum zu interpretieren.

## 7. BUY-Feld-Matrix

Signature-bound bezieht sich auf unsere spätere BUY-Transaktion. State-Daten sind nicht selbst signiert, beeinflussen aber die signierten Beträge und deren Gültigkeit.

| Feld | Quelle | Frühester Zeitpunkt | Signature-bound | Klassifikation | Signalzeit-Restarbeit | Invalidierung |
|---|---|---|---|---|---|---|
| destination | Portal launch.to + captured runtime; router code present, implementation semantics unverified | startup | True | PRECOMPUTABLE | mode validation | deployment/proxy/code change |
| selector | official IPortal + ethers + web3_sha3: 0xef7ec2e7 | startup | True | PRECOMPUTABLE | none | ABI/mode change |
| token address | V6 salt + Portal + observed EIP1167 implementation, ethers CREATE2 | pending launch | True | SIGNAL_TIME_LOCAL_CALC | CREATE2 and patch | salt/implementation/deployer change |
| input token | direct fixture address(0); router downstream DOGE observed | startup policy + pending quote mode | True | PRECOMPUTABLE | validate chosen mode | quote mode or routing change |
| input amount | local budget; direct BUY decoded 150000000000000000 | startup policy | True | PRECOMPUTABLE | budget/amount patch | policy/funds/conversion change |
| recipient | direct: tx.from; router: final Transfer to tx.from; command semantics unverified | startup wallet policy | True | PRECOMPUTABLE | sender/recipient consistency | sender/router command/config change |
| minOutputAmount | slippage policy + verified state/route math, currently missing | UNKNOWN | True | UNKNOWN | local quote then patch only after proof | curve/pool/ticks/fees/tax/ordering/input/policy change |
| permitData | direct fixture empty 0x; ERC20 allowance separate from permit | startup if mode frozen | True | PRECOMPUTABLE | none for native input | ERC20 mode/allowance/permit nonce/deadline change |
| nonce | maintained own-account nonce lane; not inspected in this no-wallet audit | continuous pre-signal | True | PRECOMPUTABLE conditional | reserve/finalize lane | competing tx/replacement/reorg |
| gas/fee | local policy + gas/head state; historical gas is not a future gas-limit guarantee | continuous pre-signal | True | PRECOMPUTABLE conditional | finalize policy values | head/policy/route/state change |
| DOGE conversion state | router V2 reserves; direct V3 price/liquidity/ticks + actual converter mode | pre-signal cache possible, completeness unproven | False | UNKNOWN | pool simulation if fully cached | any pool swap/liquidity/fee/converter mode change |
| curve state | r/h/k observed in launch event; conditional post-launch reserve reproduced | post-execution evidence now; pre-signal derivation unproven | False | UNKNOWN | derive creator-buy state after source proof | curve/config/creator buy/intervening trades/reorg |
| tax state | V6 rates and allocation available in calldata; actual rounding/exemption code not bound | pending + pre-signal registry/code cache | False | UNKNOWN | exact integer deductions after proof | rate/exemption/config/extension/version change |
| router-specific fields | execute(bytes,bytes[]) envelope; command bytes 0b090816 and raw operands | calldata now; future build semantics unproven | True | UNKNOWN | patch amount/token/path/recipient after decoder proven | dispatcher/code/pool/path/slippage changes |
| extension state | V6 extensionID/data + beneficiary/dividend; implementation lookup unproven | pending plus cached registry | False | UNKNOWN | mode validation and hook-state effects | registry/hook/state/code changes |
| chainId/type/access list | BSC chainId56 verified; exact tx type is a future scope decision | startup | True | PRECOMPUTABLE conditional | serialize frozen type | chain/transaction-type/policy change |

Keiner der offenen Punkte wird durch einen synchronen RPC-/HTTP-Aufruf im späteren Hotpath „gelöst“. Ein vollständiger Cache ist erst dann ausreichend, wenn seine Aktualisierung, Invalidierung und die angenommene Ausführungsreihenfolge nachgewiesen sind.

## 8. Quellen, Abfragen und Wiederholung

Vetted Werkzeug: `ethers@6.15.0`; offizielles `IPortal.sol` aus `flap-sh/FlapVaultExample`, Commit `5949cc7eb99bcb5ac5f679cc710ae456e627f12a`; offizielles PancakeSwap `IPancakeV3PoolEvents.sol`, URL und SHA-256 gespeichert. Source-Spiegel `incjanta/Portal`, Commit `bed46ef000364beba64d8ea74fd33ba3e9f7b50d`: **Referenz, keine verifizierte Deployment-Autorität**. Der Baum enthält keine Implementierungsquellen für die benötigten Trade-/Launcher-Module. Explizite Pfadversuche sind als 404 dokumentiert.

BscScan-Sourcezugriff: HTTP 403. Sourcify-Abfragen für Router und Portal-Implementation: HTTP 404. Daher weder Routername noch Runtime/Source-Bindung als bewiesen markiert. Manifest-Dateien unter `sources/` enthalten URLs, Status und Hashes. Die fehlgeschlagenen Netzwerkversuche blieben reine Lesezugriffe.

Historische `eth_call` für TokenState, QuoteConfiguration und FeeRate am Launch-Block: `missing trie node`. `debug_traceTransaction` im Batch: RPC `-32004` (Batch-Methode nicht unterstützt). Danach einzeln für Launch, ersten Router-BUY und direkten BUY: jeweils `missing trie node`. Fehlender Archive-State wurde nicht als Protokoll-Blockade interpretiert; Offline-Rekonstruktion, Event-Abgleich, Quellenprüfung und separate Reserve-Abfrage wurden trotzdem durchgeführt.

Ausgeführte wesentliche Testbefehle, jeweils aus `/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2`:

```sh
npm install --prefix gate0-evidence/abi-tools --ignore-scripts --no-audit --no-fund ethers@6.15.0
python3 gate0-evidence/collect_g02_sources.py
python3 gate0-evidence/collect_g02_modules.py
python3 gate0-evidence/collect_g02_tree.py
python3 gate0-evidence/collect_g02_pool_source.py
node gate0-evidence/decode_flap_economics.cjs
python3 gate0-evidence/trace_flap_g02.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-28-g02-economics/trace_binance
python3 gate0-evidence/trace_flap_g02.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-28-g02-economics/trace_binance_retry
python3 gate0-evidence/trace_flap_g02.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-28-g02-economics/trace_binance_single --single
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-28-g02-economics/state_rpc --requests gate0-evidence/2026-09-28-g02-economics/state_requests.json
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-28-g02-economics/v2_presync_rpc --requests gate0-evidence/2026-09-28-g02-economics/v2_presync_requests.json --single
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-28-g02-economics/v2_presync_rpc_retry --requests gate0-evidence/2026-09-28-g02-economics/v2_presync_requests.json --single
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-28-g02-economics/pool_fee_rpc --requests gate0-evidence/2026-09-28-g02-economics/pool_fee_requests.json
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-28-g02-economics/pool_fee_rpc_retry --requests gate0-evidence/2026-09-28-g02-economics/pool_fee_requests.json
python3 gate0-evidence/verify_flap_g02_economics.py
```

Netzwerkbefehle wurden bei DNS-Sandboxfehlern mit Berechtigung wiederholt. Quellen-Downloader wurden nach solchen DNS-Fehlern mit identischem Befehl erneut ausgeführt. RPC-Collector benötigt absichtlich neue Ausgabeverzeichnisse für Wiederholungen. `read_rpc.py` erhielt nur für die lokale Evidence einen Einzelrequest-Modus; die Read-only-Allowlist blieb unverändert.

**Einzelner reproduzierbarer Offline-Prüfbefehl:**

```sh
python3 gate0-evidence/verify_flap_g02_economics.py
```

Aktuelles Ergebnis: **Exit 2 = G0.2_FLAP_PARTIAL**. Die implementierten ABI-/Hash-/Transfer-/Integer-Konsistenzchecks bestehen. Exit 1 bedeutet einen gescheiterten Prüfschritt. Exit 0 bleibt einem vollständig nachgewiesenen Gate vorbehalten; offene Anforderungen werden nicht durch tolerierte Differenzen oder leere Assertions grün gemacht. Das Skript ist ein Evidence-Prüfer, kein Produktiv-Adapter und kein Latenzbenchmark.

Ergebnisdateien: `abi_decode.json`, `decoded_receipts.json`, `decoded_timeline.json`, `initial_state_reconstruction.json`, `economics_reproduction.json`, `buy_field_matrix.json`, `gate_result.json`, `evidence_sha256.json`. Sämtliche Beträge werden als Integer berechnet; keine Float-Toleranz.

## 9. Entscheidung und genau ein nächster technischer Test

**G0.2_FLAP_PARTIAL.** Kein unvermeidbar erst später verfügbares Feld ist bislang bewiesen; daher wäre `BLOCKED` zu stark. Ebenso wäre `READY_FOR_G0.3` wegen der aufgeführten Evidence-Lücken falsch. Kein PRIMARY_GATE0_SCOPE wird freigegeben. Kein Wechsel zu Four oder einem anderen Flap-Modus ist aus diesen Teilresultaten begründet.

**Ein nächster Test: ein Archive-gestützter, transaktionsindexgenauer Execution-Witness-Replay vom Launch `124415357:0` bis einschließlich erstem BUY `124415358:15`.**

Dieser eine Replay muss historische Runtime-Code/Read-Prestate, Calltree und intervenierende Zustandsänderungen liefern und offline wiederholbar machen. Er soll insbesondere den tatsächlichen Portal-Selector des Router-Kommandos, Herkunft von r/h/k, Quote-/Converter-Modus, V2-Pre-Reserves, Extension-/Exemption-Effekte und die Integer-Gebührenrundung beweisen. Ein `eth_call` am falschen Block-Endzustand oder bloßes Abschreiben des beobachteten Outputs erfüllt diesen Test nicht.

Für den im Auftrag zusätzlich geforderten direkten Pfad ist die bereits eingefrorene spätere Fixture separat abzugleichen; die First-BUY-Replay-Priorität bleibt unverändert. G0.3 wird nach diesem Arbeitsblock nicht begonnen.

Keine BSC-Werte für T_signal, T_ready, T_queue, T_send oder T_buy wurden gemessen oder aus Block-Zeitstempeln erfunden.
