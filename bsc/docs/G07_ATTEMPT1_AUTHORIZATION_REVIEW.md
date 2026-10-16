# G0.7 Versuch 1 — Prüf- und Freigabevorlage

Stand: 2026-10-16. **Nichts gesendet. Guard `FROZEN_APPROVED_V2_SHA256 = "UNARMED"`. Aktiver Vertrag `bsc/config/g01_mainnet_acceptance_proposal_v2.json` unverändert (`026b534f…0b19ff`, unapproved).** Diese Vorlage ist keine Freigabe; sie bereitet deine Entscheidung vor.

## 1. Vorbereitet (nicht committet)

| Artefakt | Pfad | SHA-256 |
|---|---|---|
| DRY-Ergebnis (gebunden) | `bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/dry_result.json` | `0efe78a7faf1d817fae3cbfd8893f858a4386a4c2770755465508c668eeded6f` |
| Preflight-Baseline (gebunden) | `…/preflight.json` | `3eb5563d9e0bfea9ce8cc4e3750e3152d81a1b7ea7eff91de78091087bf77e69` |
| Weitere DRY-Dateien + `SHA256SUMS` | `…/g06a_final_dry_20261015T222241Z/` | byte-identisch zu `bsc/data/…` (lokal) |
| Builder-Policy (gebunden, unverändert) | `bsc/config/g06a_builder_floor_addendum_2026-10-11.json` | `ca76a93c432a9cee308b07e59170bac390e801caaa8e0697b0b31d405b9bcc87` |
| Producer-Witness-Quellvertrag, `source_ready=false` (gebunden) | `bsc/config/g07_producer_witness_source_attempt1.json` | `78660c7da9fbb8a2140daf1bca4187881d71f55a914c9fcddfc0e47f287116c4` |
| Builder-Refresh (informativ) | `bsc/native/evidence/g06a/builder_requirements_refresh_20261016T013943Z.json` | `ef4b72ffd26c07850fda15a8193a744abbd5e33015623a4c8d0818af740287b6` |
| **Finaler v2-Entwurf (inaktiv)** | `bsc/docs/G07_ATTEMPT1_ACCEPTANCE_V2_DRAFT.json` | `6051888d9b7779514a9567955c03375a1e7b750c7db4a515c955687c3db45b6d` |
| Byte-Schutz | `bsc/config/.gitattributes`, `bsc/native/evidence/g06a/.gitattributes` | `-text` |

`attempt_id`: `g07-mainnet-attempt-1-dry-20261015T222241Z`.

**v2-Entwurf gegenüber dem aktiven Vertrag:**
- `approved`/`execution_ready` = true.
- Vier `evidence_bindings`.
- `attempts.maximum` = 1.
- Budget eng: Gas 0,05 gwei, BUY 0,000001 BNB, Launch-Wert 0, Tip 0, pro Versuch und absolut **481 163 500 000 000 wei = 0,0004811635 BNB**. Davon Launcher 0,0003301635 BNB, Buyer 0,000151 BNB. Erwartete Ist-Kosten etwa 0,00028 BNB.
- Pflicht-Entschärfung (Disarm) nach dem Versuch.
- Ehrliche Statusliste der 11 Pre-Arm-Checks.
- Präzisierte Builder- und Timing-Texte.
- `approved_contract_sha256` bleibt `null`: Eine Datei kann nicht ihren eigenen Hash enthalten.

## 2. Ergebnisse

- **G0.6a-Final-DRY:** PASS (DRY1–DRY4, Null-Egress) bei Head 127865760, Nonces 0/0.
- **Readiness 2026-10-16:** `PASS_READ_ONLY`, `FUNDING=PASS` gegen die exakten DRY-Parameter, keine Drift.
- **DRY-Nachweise:** keine signierten Bytes, keine Secrets, deshalb versionierbar.
- **Builder-Refresh, nur lesend, unabhängig geprüft:** Jeder auffrischbare explizite Floor liegt bei 50 000 000 wei. Das gilt für BlockRazor (je Transaktion und als Durchschnitt), die 48Club-Web3-Docs und den BNB-Chain-Standardpreis. `eth_gasPrice` = 50 000 000, BaseFee 0. Blockzeit 450 ms (BEP-619). `maxBlockNumber` ist bei beiden Buildern ein Integer, wie G0.7 ihn sendet. Kein Builder-Endpunkt wurde kontaktiert.
- **Guard-Replik:** Mit Guard = Entwurfs-Hash bestehen alle Prüfungen. Unter `UNARMED`, mit falscher Bindung, Versuch 2, 1 wei Vorverbrauch oder 1 wei zu hohem Gas wird abgelehnt.
- **Adversarielle Prüfung:** drei unabhängige Prüfer für Kompatibilität gegen den echten Rust-Code, Sicherheit/Scope und Inhalt gegen die Pläne. Ergebnis: kein Blocker in Mechanik oder Sicherheit, die Befunde unten.

## 3. Offene Entscheidungen — vor einer Freigabe

1. **G0.7-Härtung (Codeänderung, empfohlen).**
   - **Problem:** G0.7 setzt `maxBlockNumber = send_head + 2` aus einem Head, der Sekunden vor dem Send gelesen wird. Bei 450-ms-Blöcken wäre das Bundle sehr wahrscheinlich abgelaufen. Geld ginge dabei nicht verloren, aber der einzige Versuch wäre verbraucht.
   - **Vorschlag:**
     - Den Head unmittelbar vor der Freigabe neu lesen (nach dem TLS-Prewarm) und `maxBlockNumber` daraus setzen; beide Heads protokollieren.
     - Für 48Club zusätzlich ein enges `maxTimestamp`.
     - Eine Einmal-Sperre (feste Latch-Datei) plus festen Journal-Pfad unter `bsc/.local/` mit Rechten 0600.
     - `env_clear` für die Kindprozesse `node`/`curl`, damit sie keine Schlüssel erben.
     - Den BlockRazor-Token vor dem Guard prüfen.
     - Einen Pflicht-Evidence-Pfad.
     - Dieselben Bytes hashen, die geparst werden.
   - **Auswirkung:** Die signierten Bytes ändern sich nicht, der DRY bleibt gültig. Nötig sind Review und gezielte Tests.
2. **Builder-Semantik (Pre-Arm-Checks 6 und 10).** `noMerge`/`positionFirst` und die 48Club-Expiry-Akzeptanz (`maxTimestamp`) sind unqualifiziert. Entweder du erlaubst ausdrücklich eine Probe mit nicht ausführbaren Fixtures bei den Buildern, oder du akzeptierst die Checks als UNQUALIFIED; eine Ablehnung wäre dann ADAPT.
3. **Restrisiken akzeptieren:**
   - null Spielraum (Gas = Floor = Netzpreis = Obergrenze);
   - niedrigster Prioritätsrang;
   - nicht verifizierte Annahme durch die Endpunkte;
   - ein nicht eingeschlossenes Bundle verbraucht den einzigen Versuch;
   - mit `source_ready=false` bleibt T0→T_buy UNKNOWN, und das Timing-Gate kann in Versuch 1 nicht bestehen.
4. **Zahlen und Budget bestätigen.** Außerdem die Wallet-Bestände: je 0,005 BNB, etwa das 20-Fache des Bedarfs. Der Spend-Freeze verbietet Funding für Wiederholungen.
5. **Frische:** Den Builder-Refresh höchstens 24 h vor dem Scharfschalten wiederholen; bei Änderung STOP.

Jede Entscheidung, die den Entwurf ändert, ändert seinen SHA-256. Freigegeben wird nur der dann finale Hash.

## 4. Ablauf nach deinen Entscheidungen

1. Evidence, Witness, Refresh und `.gitattributes` committest du selbst.
2. Die Readiness läuft **vor** dem Scharfschalten. Danach verweigern DRY, Readiness und Probes absichtlich ihren Dienst.
3. **Scharfschalt-Commit**, nur mit deiner ausdrücklichen Freigabe:
   - die finalen Entwurfsbytes per `cp` byte-gleich nach `bsc/config/g01_mainnet_acceptance_proposal_v2.json`;
   - `FROZEN_APPROVED_V2_SHA256` auf den finalen Hash in Kleinbuchstaben;
   - `execution/tests/acceptance_guard.rs` anpassen;
   - G0.7 aus diesem Commit neu bauen (`--features g07-mainnet --locked`).
4. Die Umgebung aus dem Repo-Root:
   - `DARKBOT_G07_MAINNET_SEND=YES`, `ATTEMPT_NUMBER=1`, `SPENT_BUDGET_WEI=0`;
   - `ATTEMPT_ID` = Witness-`attempt_id`;
   - die gebundenen Pfade oben, das DRY-Salt aus `economics_crosscheck.json`;
   - die Schlüssel, die zu den DRY-Wallets gehören, und der BlockRazor-Token;
   - `DARKBOT_G07_EVIDENCE_PATH` und ein neuer Journal-Pfad unter `bsc/.local/`.
5. **Genau ein Lauf.** Danach sofort `g08_mainnet_verify`, denn historischer State wird von RPC-Knoten gekappt.
6. Den **Entschärf-Commit** (`UNARMED`) machst du unabhängig vom Ergebnis.

## 5. Sicherheitshinweise

- Das G0.7-Capture-Journal enthält sendefähige signierte Bytes. **Nie committen, bevor beide Nonces on-chain verbraucht sind.**
- `bsc/secrets.env` hat die Rechte 0644. Empfehlung: `chmod 600 bsc/secrets.env`.
- Das Salt ist seit 2026-10-03 im Repo. Ist das Repo öffentlich, könnte jemand die Token-Adresse vorab belegen. G0.7 bricht dann vor dem Send ab, Geld ist nicht gefährdet.
