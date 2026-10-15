# G0.2 Flap: verifizierte Korrekturen vom 28.09.2026

Dieser Bericht ersetzt den bisherigen CREATE2-Mismatch-Befund. Produktionscode und Git im DarkBot-Repository wurden nicht geändert. Alle neuen Netzwerkabfragen waren lesend. Kein BUY wurde signiert oder gesendet.

## 1. Adresse: Fixture-Nachweis bestanden

Der bisherige Test hat die 20-Byte-Portal-Adresse vor dem CREATE2-Hash auf 32 Byte aufgefüllt. Dadurch wurde eine **97-Byte-Eingabe** gehasht. Nach [EIP-1014](https://eips.ethereum.org/EIPS/eip-1014) müssen es genau **85 Byte** sein:

`0xff (1) || deployer (20) || salt (32) || keccak256(init_code) (32)`

Mit den unveränderten V6-Calldata und der im Token-Runtime eingebetteten Implementation ergibt die korrekte Berechnung:

| Feld | Ergebnis |
|---|---|
| Portal | `0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0` |
| Implementation | `0x024f18294970b5c76c0691b87f138a0317156422` |
| Salt | `0x0c5ae0f6ffd0a19acfb501ad8eb95542ce1ee50305987db5b6d388537a128e2c` |
| Init-Code-Länge | 55 Byte |
| Init-Code-Hash | `0x2f7f413fcc6c3812c665c15bd4a012e663f567d626112a81d401066fd5a771b4` |
| Berechnete Adresse | `0xabe1fe16cf6bc48c9524713054efe6e954b17777` |
| TokenCreated-Adresse | `0xabe1fe16cf6bc48c9524713054efe6e954b17777` |
| Vergleich | **EXAKTER MATCH** |

Der lokale Prüfer besteht alle sieben veröffentlichten EIP-1014-Beispiele. Eine unabhängige `web3_sha3`-Abfrage bestätigt den Hash der korrekten Fixture-Eingabe. Der alte Wert `0xaf5b…818a` lässt sich ausschließlich mit der fehlerhaften 97-Byte-Eingabe reproduzieren.

Die 45-Byte-Runtime ist ein EIP-1167-Clone mit der oben genannten Implementation. Die [offizielle Flap-Referenz](https://github.com/flap-sh/flap-skills/blob/main/launch-bnb-token-on-flap/references/salt-finding.md) passt zu diesem Ergebnis. Ein Archive-Trace ist zur Auflösung dieses Adressfehlers nicht mehr erforderlich. Die historische Runtime-Abfrage selbst blieb mit `missing trie node` unbeantwortet; die Runtime-Evidence stammt aus `latest`.

**Grenze des Nachweises:** Die Adresse dieser Fixture ist aus Calldata und bekannter Implementation lokal reproduzierbar. Noch nicht nachgewiesen sind rechtzeitige Pending-Sichtbarkeit und sichere Aktualisierung des Implementation-Caches für zukünftige Launches.

## 2. Quote: DOGE statt WBNB

Der Launch verwendet `0xba2ae424d960c26247dd6c32edc70b295c744c43`. Die aktuellen Contract-Abfragen liefern `name() = Dogecoin`, `symbol() = DOGE`, `decimals() = 8`.

- `quoteAmt = 80574062404`, entsprechend 805,74062404 DOGE bei diesen Dezimalstellen.
- Launch-Transaktionswert: 0 native BNB.
- Auch `dividendToken` enthält diese Adresse.
- Der spätere direkte Portal-BUY `0x8aff…70f6b` hat `inputToken = address(0)`, `inputAmount = msg.value = 150000000000000000` Wei und `minOutputAmount = 6000000000000000000000000`.
- Sein Receipt enthält DOGE-Zufluss zum Portal. Die genaue native-BNB→Quote-Konvertierung, Gebühren und Rundung müssen Bestandteil der lokalen Reproduktion sein. Ein reines BNB/WBNB-Curve-Modell bildet diese Fixture nicht ab.

## 3. Vollständige BUY-Reihenfolge statt ausgewählter Receipts

Vier aufeinanderfolgende Blöcke `124415357–124415360` wurden vollständig geprüft: **211 Receipts**. Receipt-Transaktionslisten stimmen mit den Block-Transaktionslisten überein; Block-Hashes, Reihenfolge und Parent-Hashes sind konsistent. Portal-TokenBought-Events wurden nach Token, Block, Transaktionsindex und Logindex ausgewertet.

| Art | Block / TX-Index | TX-Präfix |
|---|---|---|
| Creator-BUY im Launch | 124415357 / 0 | `0x5b8cc989…` |
| **Erster externer BUY** | **124415358 / 15** | **`0x189585be…`** |
| Zweiter externer BUY | 124415358 / 16 | `0x73716f5f…` |
| Dritter externer BUY | 124415359 / 1 | `0x0b94910a…` |
| Vierter externer BUY | 124415359 / 17 | `0x6f723464…` |
| Bisherige BUY-Fixture: fünfter externer BUY | 124415360 / 17 | `0x8aff738f…` |
| Sechster externer BUY | 124415360 / 23 | `0x39784336…` |

Erster externer BUY vollständig: `0x189585bec2fbfac6d2ff03455e1137b6c938bd17f495895e4e076b808d926800`.

Dessen Top-Level-Ziel ist `0x43dd5eebeebead726fbc84e2511567d9827ae1bb`, Selector `0x24856bc3`; es ist kein direkter Portal-Aufruf. Die internen Router-Aufrufe sind noch nicht vollständig dekodiert. Sein erfolgreiches Portal-Event meldet Output `6243568333969093909994622` und Quote-Einsatz `24051320534`. Event-Output ist Beobachtung, noch keine unabhängige lokale Quote-Reproduktion.

## 4. Ausgeführte Befehle und Ergebnisse

Arbeitsverzeichnis: `/Users/sawyer_ma/Documents/ChatGPT/DARKBOT 2`.

```sh
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-27-g01/flap_v6_corrected_create2_rpc --requests gate0-evidence/2026-09-27-g01/flap_v6_corrected_create2_requests.json
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-27-g01/flap_quote_identity_rpc --requests gate0-evidence/2026-09-27-g01/flap_quote_identity_requests.json
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-27-g01/flap_buy_order_receipts_rpc --requests gate0-evidence/2026-09-27-g01/flap_buy_order_receipts_requests.json
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-27-g01/flap_buy_order_blocks_rpc --requests gate0-evidence/2026-09-27-g01/flap_buy_order_blocks_requests.json
python3 gate0-evidence/read_rpc.py --url https://bsc-dataseed1.binance.org --out gate0-evidence/2026-09-27-g01/flap_first_external_buy_rpc --requests gate0-evidence/2026-09-27-g01/flap_first_external_buy_requests.json
python3 gate0-evidence/verify_flap_g02.py
```

Ergebnisse: Hash-Match; DOGE/8; vier vollständige Receipt-Listen; vier zusammenhängende Block-Header samt Transaktionslisten; echter erster BUY geladen; Offline-Prüfung **Exit 0**.

Vorherige Logabfragen scheiterten: Binance `eth_getLogs`-Batch mit RPC `-32005` (Rate Limit), PublicNode mit HTTP 403 auch außerhalb der Sandbox. Deshalb wurde auf vollständige Block-Receipts gewechselt. Alle Versuchsartefakte bleiben erhalten. RPC-Laufzeiten sind keine BUY-Latenz.

Der letzte Befehl ist ohne Netzwerk wiederholbar. Roh-RPC-Befehle benötigen für Wiederholung neue Ausgabeverzeichnisse, weil der Collector Überschreiben absichtlich verhindert. Maschinenlesbare Ergebnisse inklusive SHA-256 der eingelesenen Evidence: `gate0-evidence/2026-09-27-g01/flap_g02_verified_correction.json`.

## 5. Gate-Entscheidung und nächste Arbeit

- Adressreproduktion der konkreten V6-Fixture: **bestanden**. Den CREATE2-Mismatch-Blocker in der Roadmap entfernen.
- Quote-Scope auf **DOGE** korrigieren und native-BNB-Eingang vom Curve-Quote-Asset trennen.
- Ersten externen Router-BUY und späteren direkten Portal-BUY getrennt reproduzieren; intervenierende Käufe dürfen nicht übersprungen werden.
- Als Nächstes: Curve-/Fee-/Extension-Konfiguration aus vorab bekanntem Zustand plus Launch-Calldata ableiten; dann exakte Quote-/Output-Reproduktion inklusive Integer-Rundung und tatsächlicher Route.
- **G0.2 bleibt IN PROGRESS; G0.3 bleibt ohne vollständigen G0.2-Nachweis gesperrt.** Der verbleibende Grund ist Zustand/Quote/Route, kein Adress-Mismatch.
- Noch keine BSC-`T_send`-Messung und kein Nachweis für „innerhalb 1 ms gekauft“. Die lokale Sendezeit und tatsächliche On-Chain-Ausführung bleiben getrennte Nachweise.
