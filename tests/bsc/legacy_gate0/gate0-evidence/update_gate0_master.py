"""Consolidate the final G0.2 semantic proof into the existing master, no new report."""
import pathlib,json
root=pathlib.Path(__file__).resolve().parent
p=root/'2026-09-28-g02-archive-witness-quicknode-153754'
gate=json.loads((p/'gate_result.json').read_text())
assert gate['status']=='G0.2_FLAP_PASS' and all(gate['requirements'].values())
assert gate['full_independent_first_buy_difference']=='0'
path=pathlib.Path('/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot/docs/GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md')
text=path.read_text()
text=text.replace('**AKTUELL — PARTIAL**','**PASS für den eingefrorenen historischen Modus**')
text=text.replace('**BLOCKIERT durch G0.2**','**G0.2-Voraussetzung erfüllt; STOP vor Implementierung / Review**')
text=text.replace('### 5.3 Historischer First-BUY unabhängig exakt reproduziert — PASS als Rechenresultat','### 5.3 Historischer First-BUY unabhängig exakt reproduziert — semantischer PASS')
text=text.replace('Aber:\n\n**Update 2026-09-28:**','**Update 2026-09-28:**')
old='Das ist ein unabhängiger **State-Diff-Replay mit exakter BUY-Arithmetik**. Es ist kein vollständig unabhängiger EVM-Replay aller externen Callees. G0.2 bleibt wegen der in Abschnitt 10 benannten Semantik-/Pending-Nachweise **PARTIAL**.'
new='''**Zusätzlich abgeschlossen:** Launch, First BUY und separater Direct BUY wurden vollständig als lokale EVM-Message-Ausführungen aus historischem Code, Prestate und Calldata replayt. Keine aufgezeichneten Callee-Returnwerte oder beobachteten Token-Outputs dienen dabei als Ausführungseingaben. Alle **237 Callframes / 100 Receipt-Logs** sowie beide BUY-Empfänger-Balancezuwächse stimmen exakt. Der Launch stimmt zusätzlich in **106 geänderten Storage-Werten** mit dem separaten State-Diff überein. Details und Grenzen stehen in Abschnitt 10.2.

Damit ist `full_independent_first_buy_difference = 0` jetzt auch als vollständiger semantischer Nachweis dieser historischen Message-Ausführung belegt. Die 65 Zwischen-TXs bleiben ein überprüfter Provider-State-Diff-Replay, keine unabhängige Ausführung sämtlicher BSC-Blocktransaktionen. Gas-Settlement und BSC-Konsens sind ausdrücklich nicht Teil dieses G0.2-PASS.'''
assert old in text;text=text.replace(old,new)
text=text.replace('## 10. Geprüfte Zwischen-Transaktionen und verbleibende G0.2-UNKNOWNs','## 10. G0.2-Nachweise, Abschluss und Scope-Grenzen')
text=text.replace('# Exit 2: G0.2_FLAP_PARTIAL; kein künstlicher PASS','# damaliger Exit 2 nach ausschließlich dem Zwischen-TX-Nachweis;\n# abschließender Exit 0 nach dem Semantik-Nachweis unten')
a=text.index('### 10.2 Verbleibende UNKNOWNs');b=text.index('## 11. Exakte Exit-Kriterien',a)
text=text[:a]+'''### 10.2 Die drei verbleibenden Requirements — geschlossen

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

''' + text[b:]
text=text.replace('Bis dahin:\n\n```text\nG0.2 = PARTIAL\nG0.3 = BLOCKED\n```','''Abschließender Nachweis gemäß Abschnitt 10.2:

```text
G0.2 = PASS (eingefrorener historischer Modus, dokumentierte Guards)
G0.3 = NICHT GESTARTET; STOP zur Review
```

Die Kriterien sind Voraussetzungen für den späteren Harness, kein Nachweis bereits verfügbarer Live-Coverage oder Latenz.''')
path.write_text(text)
print('Bestehende Master-Datei auf G0.2-Scope-PASS aktualisiert; keine zusätzliche Statusdatei.')
