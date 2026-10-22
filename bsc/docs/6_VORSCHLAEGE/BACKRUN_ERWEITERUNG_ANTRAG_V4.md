# Backrun-Erweiterung — Antrag V4 (nicht verbindlich)

**Stand:** 2026-10-22 · **Bezug:** `sawyerma/darkbot` `main` @ `f2b5840` (auf `origin/main` `72d9903` keine Änderungen unter `bsc/`) · **Status:** ANTRAG. Kein Auftrag, bis der Nutzer eine Stufe ausdrücklich freigibt.
**Ersetzt:** Antrag V3. V2 und `FABLE_PHASENPLAN_BACKRUN_V1.md` bleiben zurückgezogen.

**Stand der Gegenprüfung zu V3:** Die Gegenprüfung empfiehlt, BR0 als eng begrenzte Read-only-Ausnahme mit fünf Auflagen freizugeben. Die Auflagen 1–4 sind in BR0 Version 4 umgesetzt und getestet (§4). Auflage 5 (Tests im Repository-Arbeitsbaum, erster begrenzter Mainnet-Bereich) muss bei euch ausgeführt werden und braucht deine Freigabe. **Eine Freigabe ist mit diesem Dokument nicht erteilt.**

**Kennzeichnung in diesem Dokument:** **[belegt]** = gegen eine genannte Quelle oder das Repository geprüft · **[selbst geprüft]** = von mir nachgerechnet oder getestet · **[unbelegt]** = Annahme oder offene Frage.

## 0. Verhältnis zum bestehenden Plan

Dieser Antrag **ergänzt** Gate 0 um eine Untersuchung. Er ändert nichts an:

- Rangfolge Roadmap > Testplan > Masterplan, Stop-Regel, Entscheidungsregel §21
- Produktanforderung `T_buy − T0 < 1 ms` (bleibt verbindlich, Status NOT_PROVEN)
- G0.7 Self-Launch, G0.8, V4-Vertrag, Guard `UNARMED`, Deployment-Codehash-Prüfung, Verlustreserve
- C1–J einschließlich der nativen geth-Fast-Lane
- Originaldokumenten in `bsc/docs/`

Stufennamen **BR0–BR5** (nicht „F…", weil die Roadmap bereits eine Phase F „Dual Capture" hat).

## 1. Fragestellung

Kann DarkBot einen **fremden** Launch so früh erkennen und so einreichen, dass der eigene Kauf Position N+1 erreicht — und was kostet das? G0.7 beantwortet das nicht: Dort kauft DarkBot hinter dem eigenen Launch, und N+1 ist durch die Bundle-Konstruktion vorgegeben.

## 2. Was belegt ist und was nicht

| Aussage | Stand | Quelle / Prüfung |
|---|---|---|
| 48Club liefert per WebSocket `newPendingAuctions` Target-Hashes mit simulierten, gefilterten Logs | [belegt] | docs.48.club, Auction Transaction Feed |
| Ein Target kann eine **Einzel-TX oder ein Bundle** sein | [belegt] | dieselbe Seite. Folge: Ein Backrun hinter einem Target-Bundle steht nicht zwingend direkt hinter der Launch-TX → BR1 |
| `eth_sendBundle` mit `backrunTarget`: Der Server stellt das Target vor das eigene Bundle | [belegt] | docs.48.club, Send Bundle |
| Sortierung: Gasgebühren eigener, nicht öffentlicher TXs × 0,9 + direkte BNB-Zahlung an die Builder-Control-EOA | [belegt] | docs.48.club, Send Bundle |
| `48spSign` bei `eth_sendBundle` ist optional; für Bundles sind keine Soul Points Pflicht | [belegt] | docs.48.club, Send Bundle (Parametertabelle) |
| Feed `newPendingAuctions` verlangt 48 Soul Points; `…WithState` 710 SP; Wert = 48-Tage-Mittel; 48SP-Token nötig | [belegt] | docs.48.club, Auction Feed und 48 Soul Point |
| Backrun-Bundles unterliegen einer 1-gwei-Durchschnittsregel mit Sonderbehandlung des Targets | [belegt] | docs.48.club, Auction Feed |
| Topic-Liste des Feeds enthält „fourmeme Create" `0x396d5e90…ad20`, Flap nicht | [belegt] + [selbst geprüft] | Doku; keccak256 von `TokenCreate(address,address,uint256,string,string,uint256,uint256,uint256)` selbst berechnet = dieser Wert = Log 301 im kanonischen Launch-Receipt (`prestudy/…/four_create_receipt.json`) |
| Four Classic TokenManager2 = `0x5c95…762b`; Kauf-Event `TokenPurchase` = `0x7db5…2942`, Layout token, account, price, amount, cost, fee, offers, funds | [belegt] + [selbst geprüft] | `GATE0_G01_EVIDENCE_FREEZE`, kanonischer Receipt, QuickNode-Guide; keccak256 selbst berechnet; realer Beispiel-Log exakt dekodiert (Test) |
| 48Club + BlockRazor bauen **über 87 %** der Blöcke | [belegt] | arXiv 2602.15395 **v2 vom 30.04.2026**, Zeitraum April 2025 – Februar 2026. Die frühere Angabe „> 96 %" stammt aus v1 und ist überholt. Gegenstand der Studie ist Arbitrage, nicht Launch-Sniping |
| Ein korrekt eingereichtes Backrun-Bundle erreicht N+1 | [unbelegt] | — |
| `positionFirst` / `noMerge` sichern N+1 | [unbelegt] | Die Doku beschreibt nur Sortier- und Merge-Regeln |
| Professionelle Sniper nutzen diesen Feed | [unbelegt] | Aus On-Chain-Daten nicht beweisbar, siehe §3 |
| Der Feed liefert Four-Classic-Launches vollständig und früher als der eigene geth-Ingress | [unbelegt] | BR1 |
| Jeder Four-Classic-Kauf emittiert `TokenPurchase`, auch im AntiSniperFeeMode | [unbelegt] | BR0-Abnahme Schritt 3 |
| Four.meme hat mehrere Varianten (TokenManager V1, TokenManager2, OpenFour als eigene Familie; Quote-Assets BNB/ERC-20; AntiSniperFeeMode; `preSale` = Kauf im Launch) | [belegt] | `GATE0_G01_EVIDENCE_FREEZE` §5–§6. Folge: Jede Variante ist ein eigener Qualifikationsscope |
| Das Quote-Asset kann BNB oder BEP20 sein; `cost` und `fee` im Event stehen in Quote-Einheiten | [belegt] | four-meme-community/four-meme-ai @ `c81f0eeb`, `references/execute-trade.md`; Feld `quote` in `_tokenInfos` laut archivierter TokenManager2-ABI (`prestudy/…/sources/TokenManager2.lite.json`, sha256 `1a71bdab…b6ef`) |
| AntiSniperFeeMode on-chain: `_tokenInfoEx1s(token).feeSetting > 0`; TaxToken: `((template >> 10) & 0x3F) == 5`; X Mode: `template & 0x10000 > 0` | [belegt] | four-meme-ai `references/errors.md`; Getter-Layouts aus der archivierten ABI; Selektoren `0xe684626b` / `0x9f266331` selbst berechnet |
| Im AntiSniperFeeMode enthält das Event-Feld `fee` die Zusatzgebühr nicht | [unbelegt] | Von der Gegenprüfung genannt; in den mir zugänglichen Quellen nicht gefunden, Vertragsquellcode nicht archiviert. BR0 behandelt `fee` in diesem Modus deshalb als UNKNOWN — das ist in beiden Fällen korrekt |

## 3. Drei getrennte Evidenzklassen

Jede Auswertung führt diese drei Klassen getrennt, nie vermischt:

1. **Beobachteter Erwerb:** Welche Adresse hat nach welchem Launch auf welcher Position gekauft (TokenPurchase-Event, Receipt, Blockposition). Das ist aus kanonischen Daten direkt beweisbar.
2. **Sichtbare Beträge:** Gas sowie `cost`/`fee` aus dem Event, jeweils in der Einheit ihres Quote-Assets. `fee` ist im AntiSniper- oder TaxToken-Modus möglicherweise unvollständig. Benachbarte Transfers an Builder-Adressen werden notiert, sind aber **keinem Kauf zugeordnet** und deshalb weder Tip noch Untergrenze eines Tips. Zahlungen über interne Calls oder andere Konten sind ohne Traces unsichtbar.
3. **Vermutete Infrastruktur:** welcher Feed, welches Abo, welcher Builder-Zugang, welcher Betreiber. Aus Chain-Daten **nicht** beweisbar. Darf nur als Vermutung berichtet werden.

## 4. Stufen

Jede Stufe endet mit einem Bericht (geänderte Dateien, Befehle, PASS/FAIL, Testzähler, Risiken) und braucht für den Start eine **eigene Nutzerfreigabe**.

### BR0 — Read-only-Beobachtung on-chain (0 BNB, keine Kapitalbindung)

**Werkzeug:** `br0_observe_launches.mjs` **Version 4**, `test_br0.mjs`, `fixtures/` (unveränderte Kopien der kanonischen Fixtures mit `SHA256SUMS` und Originalpfad; das Paket ist damit ohne Repository testbar).

- **Scope fest:** Four Classic TokenManager2. TokenManager V1, OpenFour und Flap sind nicht enthalten.
- **Kauf = `TokenPurchase`-Event** von TokenManager2 für genau diesen Token in einer erfolgreichen TX. Ein bloßer Transfer ist kein Kauf. Käufer = `account`. Ein Kauf ohne passenden Transfer wird als Konsistenzflag gezählt.
- **Token = `TokenCreate.token`**, gegengeprüft gegen den Mint-Transfer.
- **„Fremd"** = `account` und `tx.from` sind weder Launch-Absender noch `creator`. Das ist eine Obergrenze, weil verbundene Wallets nicht erkennbar sind. Käufe über andere Kontrakte werden markiert.

**Umsetzung der Auflagen aus der Gegenprüfung:**

| Auflage | Umsetzung |
|---|---|
| 1. Jede nicht auswertbare Zeile → UNVOLLSTÄNDIG, Exit 3 | Exit 0 nur, wenn **jede** Zeile `OK` ist, keine RPC-Lücke besteht und die Snapshot-Nachprüfung ohne Abweichung und Fehler läuft (`isComplete`, eigene Funktion). Damit sind `DECODE_FAILED`, `TOKEN_MISMATCH`, `LAUNCH_FAILED` (fehlte in V3 ebenfalls) und jeder künftige Status abgedeckt. Ein kaputtes Kauf-Event bricht nicht mehr ab, sondern ergibt `DECODE_FAILED`. Die drei reproduzierten Fälle der Gegenprüfung sind Regressionstests |
| 2. Tips nicht zuordnen | Spalte heißt jetzt `adjacent_builder_transfer_wei`, mit `tip_attribution = UNPROVEN`. Sie ist weder Tip noch Untergrenze. Die Zusammenfassung gibt nur die Fallzahl aus, mit dem Hinweis „nicht als Tip oder Gebotsbasis verwenden" |
| 3. Quote-Asset und Gebühren | `quote_asset`, `quote_decimals` und der Modus werden am TokenManager2 gelesen, zuerst am Launch-Block, sonst `latest` (dann markiert, denn die Unveränderlichkeit ist nicht belegt), sonst `UNREAD`. Beträge heißen `*_quote_units`. `fee_completeness` ist `UNKNOWN_ANTISNIPER`, `UNKNOWN_TAXTOKEN`, `UNKNOWN_MODE_UNREAD` oder `EVENT_FEE_NO_KNOWN_EXTRA`. `tx_value_wei` ist ausdrücklich keine Kostenangabe. **Es gibt keine Kostenstatistik.** |
| 4. Kettenkontinuität dokumentieren | Im Skriptkopf steht genau, was geprüft wird: (a) Bindung je Launch, (b) `parentHash`-Kette je Launch durch die Folgeblöcke, (c) neu: Nachprüfung **jedes verwendeten** Blocks am Laufende (`SNAPSHOT_CHANGED` → Exit 3). Ausdrücklich **nicht** geprüft: eine durchgehende Kette über den gesamten Bereich und Blöcke ohne Launch |
| 5. Tests im Arbeitsbaum, erster Mainnet-Bereich | offen, bei euch auszuführen (siehe unten) |

**Tests [selbst geprüft]:** 15 von 15 bestanden, aus einem frischen Ordner nur mit den Paketdateien und ebenso mit `FIXTURE_DIR` auf den Repository-Originalen. Grundlage sind der echte kanonische Launch-Receipt und der echte `TokenPurchase`-Log aus dem QuickNode-Guide. Je Auflage gibt es mindestens einen Test. Mutationsprüfung: Jeder der fünf gemeldeten Fehler wurde absichtlich wieder eingebaut und von mindestens einem Test erkannt. Das waren die V3-Vollständigkeitsbedingung, Tip als zugeordnet, Anti-Sniper ignoriert, Snapshot-Prüfung aus und alles als BNB.

**Nicht geprüft:** ein Lauf gegen BSC Mainnet (kein RPC-Zugang in meiner Umgebung); der genaue Inhalt von `fee` im AntiSniperFeeMode.

**Auflage 5 — Ablauf bei euch (nach Freigabe):**
1. Paketdateien nach `bsc/tools/br0/` legen.
2. Im Arbeitsbaum ausführen: `node --test test_br0.mjs` (beigelegte Fixtures) und `FIXTURE_DIR=prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-27-g01/canonical_fixture_scan node --test test_br0.mjs` (Originale). Beide müssen 15/15 ergeben.
3. Ersten begrenzten Bereich lesend auswerten, z. B. `N_BLOCKS=2000`, und Exit-Code, CSV und Zusammenfassung mit SHA-256 ablegen. Bei Exit 3 gibt es keine Statistik als Ergebnis.
4. Abnahme laut Skriptkopf (Stichprobe, AntiSniper-Fälle, Builder-Adressen, Flags).

**Exit-Frage:** Gibt es regelmäßig fremde Käufe auf N+1? Wie viele verschiedene Käufer, in welchen Quote-Assets und Modi? Erst danach wird entschieden, ob BR1 und die Kapitalbindung für 48 SP sinnvoll sind. BR0 liefert Erkenntnisse über Konkurrenten, keine schnellere Engine.

### BR1 — Feed-Qualifikation (read-only; Kapital erst nach BR0)

- 48-SP-Zugang herstellen (Kapitalentscheidung des Nutzers; 48-Tage-Mittelung beachten).
- Den Feed mitschneiden, ohne zu senden, und **parallel denselben Zeitraum über den eigenen geth-Ingress** erfassen. Für jeden Four-Classic-Launch des BR0-Satzes messen:

| Messgröße | heute |
|---|---|
| Launch zuerst im Feed oder zuerst in geth gesehen | UNKNOWN |
| Anteil der Launches, die nur im Feed sichtbar sind | UNKNOWN |
| Vorlauf Feed-Nachricht → Blockaufnahme | UNKNOWN |
| Target als Einzel-TX oder als Bundle geliefert; Position des Launches im Target-Bundle | UNKNOWN |
| Vollständigkeit gegenüber BR0 | UNKNOWN |

- Protokoll: gzip, Reconnect, Duplikate, Filterverhalten.
- Flap-Topic bei 48Club anfragen und das Ergebnis dokumentieren.

**Exit:** alle Messgrößen mit Rohmitschnitt als Evidence. Der Feed gilt erst dann als nützlich, wenn er gegenüber dem eigenen geth-Ingress einen **gemessenen** Zusatznutzen zeigt.

### BR2 — Integration als zusätzlicher Gate-0-Pfad (offline, nichts verlässt die Maschine)

- **Ingress:** Der Feed-Adapter ist eine Signalquelle und übergibt typisierte Records an `trading`. Kein eigener Entscheidungspfad, keine zweite Engine.
- **Venue:** Four-Classic-Varianten **strikt getrennt** qualifizieren (TokenManager-Version, Quote-Asset, BUY-Funktion, Gebührenmodus). Jede nicht qualifizierte Variante wird fail-closed abgelehnt. Die Deployment-Identität (Codehash) wird vor jedem Versuch read-only geprüft; Drift bedeutet STOP für diese Variante.
- **Tip-Transaktion:** Vor dem ersten Code ist zu spezifizieren: Nonce-Reihenfolge (BUY `n`, Tip `n+1`), Gas-Reserve, Verhalten bei teilweiser Ausführung, Ausschluss einer späteren Einzelausführung.
- **Lebensdauer signierter Bytes:** Ein nicht aufgenommenes Bundle hinterlässt gültige signierte TXs beim Builder. Der Vertrag legt fest, wie die Nonce danach verbraucht wird (Cancel-TX); diese Kosten gehören zum Budget.
- **DRY:** bauen, signieren, dekodieren, lokal simulieren (Fork/State-Override), Byte-Parität mit einem unabhängigen Encoder. **Kein Versand signierter Mainnet-TXs.** Format-Probes beim Builder nur mit nachweislich nicht ausführbaren TXs (z. B. bereits verbrauchte Nonce) und nur mit eigener Freigabe.
- **Wiederverwendung:** Signer-, Nonce-, Budget-, Session-Cap-, Journal-, Einmal-Sperre- und Guard-Logik aus G0.7.

### BR3 — Eigener Nachweispfad (keine Erweiterung von G0.8)

`g08_mainnet_verify.rs` ist auf den Flap-Self-Launch zugeschnitten [selbst geprüft, Code gelesen]: Es liest den binären G0.7-Evidence-Record, hat das Flap-Portal `0xe2ce…9de0` fest codiert, prüft Launcher- und Buyer-Nonces und erkennt Käufe nur über `TokenBought`. Für fremde Launches braucht es ein **eigenes** Qualifikations-Binary:

- Eingaben: Target-Hash, eigene Buyer-Adresse, Bundle-Hash, Variante.
- Prüfungen: Launch auf N, eigener Kauf auf N+1, `TokenPurchase` mit eigenem `account`, Token-Erwerb, Tip-TX ausgeführt und benachbart, Gesamtkosten, Codehash der Venue im Block, Bundle-Status beim Builder.
- Klassifikation von Fehlversuchen: nicht aufgenommen / spätere Position / revertiert / Nonce-Cancel ausgeführt.

### BR4 — Kontrollierte wirtschaftliche Versuche (eigener Vertrag, eigene Freigabe)

Zwei getrennte Aussagen, nicht vermischen:

- **BR4a Machbarkeit:** kleine Anzahl. Aussage nur: „N+1 ist mit sichtbaren Kosten X erreichbar" oder „nicht erreichbar". Kein Produktions-PASS.
- **BR4b Abnahme:** Vor dem ersten Versuch schriftlich festlegen:
  - Grundgesamtheit (Variante, Quote-Asset, Builder des Blocks)
  - Auswahlregel (z. B. die ersten K qualifizierten Launches ab Startzeit, kein Aussuchen)
  - Zählung von Fehlversuchen
  - Kosten- und Sicherheitsgrenzen
  - statistische Zielaussage

**Stichprobe [selbst geprüft, Wilson 95 %]:**
- 0 Treffer in 5 Versuchen schließt eine Trefferquote bis ≈ 43,4 % nicht aus.
- 10 von 20 entspricht ≈ 29,9–70,1 %.
- Eine Trefferquote **über 50 %** zu belegen, erfordert bei beobachteten 70 % mindestens 21 und bei 60 % mindestens 91 auswertbare Versuche.

Anzahl und Beträge werden aus den BR0- und BR1-Daten abgeleitet.

**Wirtschaftlichkeit:** N+1 zu gewinnen heißt nicht, Gewinn zu machen. Eine Rentabilitätsaussage braucht mindestens:

```
Nettoergebnis = realisierter Verkaufserlös − Kaufaufwand − BUY- und SELL-Gas − Builder-Tips − Cancel-TXs − sonstige Gebühren
```

Der Verkaufspfad gehört nicht zu BR0–BR4. BR4 darf deshalb keine Rentabilität behaupten. Die G0.7-Verlustreserve (0,0004811635 BNB) gilt nur für den Self-Launch-Versuch und wird nicht übertragen; BR4 braucht ein eigenes, neu berechnetes Budget.

### BR5 — Einordnung

Die Entscheidung PASS/ADAPT/STOP fällt nach Roadmap §21. Bei PASS wird der Feed ein qualifizierter Ingress **neben** der C1-Fast-Lane. Die Fast-Lane bleibt für unabhängige Abdeckung bestehen, denn die beiden großen Builder bauen laut Studie über 87 % der Blöcke, nicht alle. Die Aufnahme in die Roadmap erfolgt durch Nutzerbeschluss.

## 5. Kosten

| Posten | Art |
|---|---|
| BR0 | 0 BNB; nur ein BSC-RPC-Zugang |
| 48 Soul Points (BR1) | Kapitalbindung (KOGE halten/staken, BNB staken, govBNB delegieren oder 48er NFT), kein Abo; Kursrisiko, Opportunitätskosten; erst nach BR0 entscheiden |
| `eth_sendBundle` 48Club | keine Gebühr; Soul-Point-Signatur optional |
| `eth_sendBundle` BlockRazor | Grundfunktion ohne Authentifizierung; erweiterte Funktionen (0-gwei-TXs im Bundle, gRPC) laut Doku nur für bestimmte Tiers mit Authentifizierung; Mindest-Gaspreis laut Doku |
| Mempool-, Block- und Gateway-Streams (BlockRazor, bloXroute u. a.) | **kostenpflichtige Monatsabos** laut Anbieter, Preise vor einer Entscheidung beim Anbieter prüfen. Nicht Teil von BR0–BR4; nur erwägen, wenn ein gemessener Zusatznutzen gegenüber dem eigenen geth-Ingress vorliegt |
| BR4 | Gas (1-gwei-Regel) + Tip + Kaufbetrag + Cancel-TX je Fehlversuch; Beträge aus BR0/BR1, im Vertrag fixiert |

## 6. Korrekturen gegenüber V1 bis V3

| Früher | Jetzt |
|---|---|
| V1: Plan wird oberste Autorität; `<1 ms` gestrichen; Codehash durch ABI ersetzt; G0.7 aufgegeben; DRY mit abgelaufenem `maxBlockNumber`; „G0.8 fast fertig"; 0,001 BNB fest; 20 Versuche als PASS; Originale löschen | alles zurückgenommen (siehe V2 §5; in V3 unverändert) |
| V2: „> 96 % der Blöcke" | > 87 % (Studie v2, längerer Zeitraum) |
| V2: BR0-Skript erkennt Kauf an einem Token-Transfer | Kauf nur über das `TokenPurchase`-Event; Gegenfall getestet |
| V2: Token über „genau einen Mint-Transfer" | Token aus `TokenCreate`, gegen den Mint geprüft |
| V2: keine Blockhash-/Ketten-Bindung | volle Bindung inkl. `parentHash`-Kette; Bruch = eigener Status, Exit 3 |
| V2: Scope über Umgebungsvariablen frei wählbar | fest auf Four Classic TokenManager2, Konstanten mit Belegen im Code |
| V2: Feed-Targets als Einzel-TX angenommen | Bundle-Targets möglich; Prüfung in BR1 |
| V2: keine Trennung der Evidenzklassen | §3 |
| V2: Wirtschaftlichkeit nur als Kostenposten | Nettoformel; keine Rentabilitätsaussage ohne Verkaufspfad; Reserve nicht übertragbar |
| V2: Stream-Abos nicht erwähnt | als kostenpflichtig und außerhalb von BR0–BR4 geführt |
| V3: Exit 0 trotz `DECODE_FAILED`, `TOKEN_MISMATCH`, `LAUNCH_FAILED` | Exit 0 nur bei ausschließlich `OK`-Zeilen; Regressionstests |
| V3: `observed_direct_tip_wei_lower_bound` | `adjacent_builder_transfer_wei`, Zuordnung `UNPROVEN`, keine Untergrenze |
| V3: `cost` implizit als BNB-Wei | Quote-Asset gelesen, Beträge in Quote-Einheiten, keine Kostenstatistik |
| V3: `fee` ohne Modusprüfung | `fee_completeness` aus AntiSniper-/TaxToken-Modus |
| V3: „volle Bindung inkl. `parentHash`-Kette" (zu weit formuliert) | genaue Beschreibung des Geprüften, zusätzlich Snapshot-Nachprüfung jedes verwendeten Blocks |
| V3: Tests nur im vollständigen Repository lauffähig | Fixtures mit SHA-256 und Originalpfad beigelegt; Test prüft die Hashes vor der Nutzung |
