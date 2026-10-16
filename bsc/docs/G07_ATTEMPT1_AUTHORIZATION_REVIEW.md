# G0.7 Versuch 1 — Prüf- und Freigabevorlage

Stand: 2026-10-16, nach der G0.7-Härtung. **Nichts gesendet. Guard `FROZEN_APPROVED_V2_SHA256 = "UNARMED"`. Aktiver Vertrag `bsc/config/g01_mainnet_acceptance_proposal_v2.json` unverändert (`026b534f…0b19ff`, unapproved).** Diese Vorlage ist keine Freigabe; sie bereitet deine Entscheidung vor.

## 1. Vorbereitet

| Artefakt | Pfad | SHA-256 |
|---|---|---|
| DRY-Ergebnis (gebunden) | `bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/dry_result.json` | `0efe78a7faf1d817fae3cbfd8893f858a4386a4c2770755465508c668eeded6f` |
| Preflight-Baseline (gebunden) | `…/preflight.json` | `3eb5563d9e0bfea9ce8cc4e3750e3152d81a1b7ea7eff91de78091087bf77e69` |
| Weitere DRY-Dateien + `SHA256SUMS` | `…/g06a_final_dry_20261015T222241Z/` | byte-identisch zu `bsc/data/…` (lokal) |
| Builder-Policy (gebunden, unverändert) | `bsc/config/g06a_builder_floor_addendum_2026-10-11.json` | `ca76a93c432a9cee308b07e59170bac390e801caaa8e0697b0b31d405b9bcc87` |
| Producer-Witness-Quellvertrag, `source_ready=false` (gebunden) | `bsc/config/g07_producer_witness_source_attempt1.json` | `78660c7da9fbb8a2140daf1bca4187881d71f55a914c9fcddfc0e47f287116c4` |
| Builder-Refresh (informativ) | `bsc/native/evidence/g06a/builder_requirements_refresh_20261016T013943Z.json` | `ef4b72ffd26c07850fda15a8193a744abbd5e33015623a4c8d0818af740287b6` |
| **Finaler v2-Entwurf (inaktiv, nicht committet)** | `bsc/docs/G07_ATTEMPT1_ACCEPTANCE_V2_DRAFT.json` | `902d4cb93c33fb91364da07fb08108b0f43f51ce8ef9e7388e9573571ce70cc2` |
| G0.7-Härtung (Code) | `bsc/engine/crates/execution/src/bin/g07_mainnet_self_launch_buy/`, `src/qualification_capture.rs`, Tests | Checkpoint `dda7c9b` + nicht committete Änderungen |
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
- **Neu seit der Härtung:**
  - `bundle_bytes.send_head_rule`: Send-Head, Ablauf (`maxBlockNumber`/`maxTimestamp`) und alle Stopp-Bedingungen vor dem Senden.
  - `retention_observability.g07_local_attempt_state`: Einmal-Sperre, privates Journal, Freigabe der Sperre ohne Send, Evidence-Regeln.
  - 48Club-Text, Pre-Arm-Check 6 und DRY-Hinweis an den Code angepasst.
  - Budget, Bindungen und Versuchszahl unverändert.
- `approved_contract_sha256` bleibt `null`: Eine Datei kann nicht ihren eigenen Hash enthalten.

## 2. Ergebnisse

- **G0.6a-Final-DRY:** PASS (DRY1–DRY4, Null-Egress) bei Head 127865760, Nonces 0/0.
- **Readiness 2026-10-16:** `PASS_READ_ONLY`, `FUNDING=PASS` gegen die exakten DRY-Parameter, keine Drift.
- **Builder-Refresh (nur lesend):** Jeder auffrischbare explizite Floor liegt bei 50 000 000 wei. Blockzeit 450 ms (BEP-619). `maxBlockNumber` ist bei beiden Buildern ein Integer. Kein Builder-Endpunkt wurde kontaktiert.
- **Guard-Replik mit dem neuen Entwurf `902d4cb9…`:**
  - Mit Guard = Entwurfs-Hash bestehen alle Prüfungen.
  - Unter `UNARMED` wird abgelehnt, ebenso mit falscher DRY-Bindung, Versuch 2, 1 wei Vorverbrauch oder 1 wei zu hohem Gas.
- **Signierte Bytes unverändert:**
  - `transactions.rs` ist byte-identisch zum Stand vor der Härtung (`565d811`).
  - Im Signier-Block ist nur eine Variable umbenannt (`send_head` → `preflight_head`) und ein Lesen des Preflight-Hashes hinzugekommen.
  - Schlüssel, Nonces, Gas, Werte und Calldata sind gleich geblieben.
  - G0.7 erzwingt weiter zur Laufzeit, dass die neu signierten Hashes gleich den DRY-Hashes sind (`0xdb468c…` Launch, `0xb086f6…` BUY).
- **Tests (2026-10-16, offline, kein Netz):**

  | Suite | Ergebnis |
  |---|---|
  | Execution-Lib | 21/21 |
  | G0.7-Binary | 18/18 |
  | `acceptance_guard` | 6/6 |
  | `g07_no_send` | 10/10 |
  | G0.8-Kompatibilität (`qualification/tests/g08_reads_g07_evidence.rs`, echtes G0.8-Binary) | 2/2 |

  `cargo fmt --check` ist sauber, `cargo check --workspace` ok, `cargo clippy` ohne neue Warnungen (eine vorbestehende in `qualification_capture.rs`).
- **Adversarielle Prüfung der Härtung:** vier Runden mit unabhängigen Prüfern und Gegenprüfung jedes MEDIUM+-Befunds. Die bestätigten Befunde sind behoben. Die letzte Runde ergab nur noch LOW/NIT; die günstigen davon sind ebenfalls behoben, der Rest steht unten als Restrisiko.

## 3. Offene Entscheidungen — vor einer Freigabe

1. **G0.7-Härtung — umgesetzt** (Checkpoint `dda7c9b` plus nicht committete Änderungen).
   - **Frischer Send-Head:**
     - Nach dem Builder-TLS-Prewarm wartet G0.7 auf den **nächsten Block**. Gepollt wird über eine vorgewärmte Keep-alive-RPC-Verbindung, höchstens 3 s, bis zu 3 Lesefehler in Folge werden toleriert.
     - `maxBlockNumber` = dieser Head + 2, für beide Builder.
     - STOP vor dem Senden, wenn
       - der Head älter als 450 ms ist (lokale Uhr);
       - er, unabhängig von der Uhr, mindestens 450 ms staler ist als der frischeste Read dieser Verbindung;
       - die lokale Uhr mehr als 150 ms hinter dem Block-Zeitstempel liegt;
       - der Head unter dem Preflight-Head liegt oder auf derselben Höhe einen anderen Hash hat.
   - **48Club `maxTimestamp`:**
     - Wert: die Sekunde nach der nominellen Sekunde von Head+2, also `floor((ms+900)/1000)+1`.
     - Ohne die +1 hätte eine übliche Prüfung „`maxTimestamp` muss nach der aktuellen Zeit bzw. dem aktuellen Block liegen“ das Bundle in rund 40 % der Phasen sofort abgelehnt.
     - Die Head+2-Grenze trägt `maxBlockNumber`. `maxTimestamp` allein ließe Blöcke bis etwa Head+6 zu.
   - **Einmal-Sperre und Journal:**
     - Ort: `bsc/.local/g07/<attempt_id>/` mit `ATTEMPT_LATCH` und `capture.journal`. Verzeichnisse 0700, Dateien 0600, kein Symlink auf dem Weg, Datei- und Verzeichniseinträge gesynct. Start nur aus dem Haupt-Checkout.
     - Ein zweiter Start desselben Versuchs wird abgewiesen.
     - Endet ein Lauf nachweislich vor jedem Bundle-Byte (Guard-Ablehnung oder `PRE_EGRESS_ABORT`), benennt G0.7 das Verzeichnis um (`.refused-…`/`.pre-egress-…`, das Journal bleibt erhalten) und entfernt nur seinen eigenen, noch leeren Evidence-Platzhalter. Ein Neustart braucht keine Handarbeit.
     - Direkt nach der Freigabe schreibt G0.7 `RELEASED`. Ab dann wird die Sperre nie mehr gelöst.
   - **Evidence, ausfallsicher und privat:**
     - `DARKBOT_G07_EVIDENCE_PATH` ist Pflicht und darf nicht existieren. Er muss in einem privaten Verzeichnis liegen: echt, kein Symlink, 0700, dir gehörend; z. B. `bsc/.local/g07/attempt1.evidence.json`.
     - Die Datei hat 0600 und wird **nie** an Ort und Stelle gekürzt. Jede Fassung wird als private Schwesterdatei vollständig geschrieben, gefsynct, atomar umbenannt, danach wird das Verzeichnis gesynct.
     - Kann G0.7 nicht ersetzen, bleibt die vollständige Kopie erhalten und wird benannt.
     - Vor dem Guard prüft ein Probelauf, dass das Staging überhaupt funktioniert.
     - Drei Fassungen: unmittelbar nach der Freigabe (noch während die Sender schreiben), nach den Sende-Threads, final nach dem Empfangsfenster.
     - Jede Fassung wird vorher gegen die 12 Felder geprüft, die G0.8 liest.
     - `canonical_t0_plus_15s_reconciled_in_g08` ist `false` (`PENDING_G08`). Den Abgleich macht erst G0.8.
     - Exit-Codes: 3 = Evidence nicht geschrieben (Kopie auf stdout), 2 = Journal unvollständig.
   - **Vor dem Guard geprüft:** BlockRazor-Token, Evidence-Pfad, RPC-URL (https, kein Builder-Host), Attempt-ID, Repo-Root und `.gitignore`. Zusätzlich ein Probe-Read des nächsten Blocks.
   - **Vor dem Senden zusätzlich geprüft:**
     - Builder-Verbindungen fertig verhandelt, lebendig (kein EOF, kein close_notify, keine unaufgeforderten Daten) und höchstens 5 s im Leerlauf.
     - Das Journal enthält die exakten Request-Bodies; G0.7 wartet darauf höchstens 50 ms.
     - Sende-Threads warten auf ein Go-Signal vor der Barriere. Scheitert das Starten eines Threads, endet der Lauf ohne ein gesendetes Byte.
   - **Env-Isolation:** `node`/`curl` laufen nur mit `PATH` (node zusätzlich mit RPC-URL und `DARKBOT_G06A_*`), `curl -q` mit 2 s/5 s Timeout. Nach der Freigabe gibt es keinen Panic-Pfad mehr.
   - **Restrisiken (bewusst nicht behoben):**
     - `curl`/`node` werden über `PATH` gefunden.
     - Die RPC-URL steht in `curl`s argv.
     - Die Schlüssel bleiben in der Prozessumgebung.
     - Die Sperre gilt pro Haupt-Checkout; ein separater Clone hätte eine eigene.
     - Eine langsame lokale Uhr *und* ein gleichmäßig verzögerter RPC zusammen bleiben unerkannt. Deshalb die Uhr vorher per NTP synchronisieren.
     - HeadPoller und Freigabepfad sind über reine Funktionen und Quelltext-Reihenfolge getestet, nicht Ende-zu-Ende gegen einen lokalen TLS-Server.
     - Der G0.8-Test erreicht 4 der 12 Felder im echten Binary (bis zum ersten RPC). Die übrigen 8 sichern der Abgleich mit dem G0.8-Quelltext und die Laufzeitprüfung vor jedem Schreiben.
     - Die Evidence wird inline gebaut; kein Test wendet die G0.8-Prüfung auf ein von G0.7 erzeugtes Objekt an. Das tut erst die Laufzeitprüfung.
     - Ein Kill in den wenigen Millisekunden zwischen Freigabe und erster Fassung hinterlässt einen leeren Platzhalter. Rekonstruktion dann aus Latch-Body und Journal.
     - Ein Send-Thread, dem nach erfolgreichem Start das Go-Signal nicht zugestellt werden kann, ließe den Prozess hängen. Laut Code ist das nicht erreichbar.
     - iCloud: Das Repo liegt nicht in der iCloud-Desktop-Kopie. Bitte trotzdem bestätigen, dass die Desktop-Synchronisierung aus ist.
2. **Builder-Semantik (Pre-Arm-Checks 6 und 10).** Für die Einreichungs-Seite ist ein Probe mit nicht ausführbaren Bundles vorbereitet, aber nicht gesendet: [G07_BUILDER_PROBE_APPROVAL.md](G07_BUILDER_PROBE_APPROVAL.md), Plan-SHA-256 `15ee10f9…`. Unqualifiziert sind:
   - `noMerge`/`positionFirst`;
   - ob 48Club `maxTimestamp` zusammen mit `maxBlockNumber` akzeptiert und `maxBlockNumber` dann weiter erzwingt;
   - die Annahme eines Bundles mit 0,05 gwei und Tip 0.

   Entweder erlaubst du ausdrücklich eine Probe mit nicht ausführbaren Fixtures bei den Buildern, oder du akzeptierst die Checks als UNQUALIFIED; eine Ablehnung wäre dann ADAPT. Alternative zu `+1`: `maxTimestamp` ohne +1, enger, aber mit dem oben genannten Ablehnungsrisiko.
3. **Restrisiken akzeptieren:**
   - null Spielraum (Gas = Floor = Netzpreis = Obergrenze);
   - niedrigster Prioritätsrang;
   - nicht verifizierte Annahme durch die Endpunkte;
   - ein nicht eingeschlossenes Bundle verbraucht den einzigen Versuch;
   - mit `source_ready=false` bleibt T0→T_buy UNKNOWN, und das Timing-Gate kann in Versuch 1 nicht bestehen;
   - dazu die Restrisiken aus Punkt 1.
4. **Zahlen und Budget bestätigen.** Außerdem die Wallet-Bestände: je 0,005 BNB, etwa das 20-Fache des Bedarfs. Der Spend-Freeze verbietet Funding für Wiederholungen.
5. **Frische:** Den Builder-Refresh höchstens 24 h vor dem Scharfschalten wiederholen; bei Änderung STOP.

Jede Entscheidung, die den Entwurf ändert, ändert seinen SHA-256. Freigegeben wird nur der dann finale Hash.

## 4. Ablauf nach deinen Entscheidungen

1. Den Code-Stand und die Evidence committest du selbst. Den v2-Entwurf erst nach der Freigabe.
2. Die Readiness läuft **vor** dem Scharfschalten. Danach verweigern DRY, Readiness und Probes absichtlich ihren Dienst.
3. **Scharfschalt-Commit**, nur mit deiner ausdrücklichen Freigabe:
   - die finalen Entwurfsbytes per `cp` byte-gleich nach `bsc/config/g01_mainnet_acceptance_proposal_v2.json`;
   - `FROZEN_APPROVED_V2_SHA256` auf den finalen Hash in Kleinbuchstaben;
   - `execution/tests/acceptance_guard.rs` anpassen;
   - G0.7 aus diesem Commit neu bauen (`--features g07-mainnet --locked`).
4. **Umgebung**, aus dem Repo-Root des Haupt-Checkouts:
   - `DARKBOT_G07_MAINNET_SEND=YES`, `ATTEMPT_NUMBER=1`, `SPENT_BUDGET_WEI=0`;
   - `ATTEMPT_ID` = Witness-`attempt_id`;
   - die gebundenen Pfade oben und das DRY-Salt aus `economics_crosscheck.json`;
   - die Schlüssel, die zu den DRY-Wallets gehören, und der BlockRazor-Token;
   - `DARKBOT_G07_EVIDENCE_PATH` in einem privaten Verzeichnis (z. B. `bsc/.local/g07/attempt1.evidence.json`; die Datei darf nicht existieren);
   - `DARKBOT_G07_CAPTURE_JOURNAL` **nicht** setzen: Er wird abgelehnt, das Journal liegt fest unter `bsc/.local/g07/<attempt_id>/`;
   - `BSC_MAINNET_RPC_HTTPS` nur als https, ohne Benutzer:Passwort, ohne IPv6-Literal;
   - `bsc/.local` mit Rechten 0700;
   - die lokale Uhr per NTP synchronisiert (z. B. `sntp time.apple.com`, Abweichung deutlich unter 100 ms).
5. **Genau ein Lauf.**
   - Endet er mit `PRE_EGRESS_ABORT` oder einer Guard-Ablehnung (Exit 101, stderr nennt die umbenannte Sperre), ging kein Byte hinaus. Ein erneuter Start ist dann deine Entscheidung.
   - Existiert `RELEASED`, ist der Versuch verbraucht: kein zweiter Lauf, sofort `g08_mainnet_verify` mit der Evidence, denn historischer State wird von RPC-Knoten gekappt.
   - Während des Empfangsfensters (bis etwa 45 s nach der Freigabe) nicht abbrechen.
6. Den **Entschärf-Commit** (`UNARMED`) machst du unabhängig vom Ergebnis.

## 5. Sicherheitshinweise

- Das G0.7-Journal enthält sendefähige signierte Bytes. Es liegt privat (0600) und git-ignoriert unter `bsc/.local/g07/`. **Nie committen, bevor beide Nonces on-chain verbraucht sind.**
- `bsc/secrets.env` hat die Rechte 0644. Empfehlung: `chmod 600 bsc/secrets.env`.
- Das Salt ist seit 2026-10-03 im Repo. Ist das Repo öffentlich, könnte jemand die Token-Adresse vorab belegen. G0.7 bricht dann vor dem Send ab, Geld ist nicht gefährdet.
- Der Produktions-Hot-Path (binär, ohne JSON/WSS/RPC/Prozessstarts/Dateizugriffe zwischen Ingress und Handoff) ist laut Phasenplan C2/C3 und **nicht** Teil dieser Arbeit. G0.7 bleibt ein isoliertes Qualifikations-Harness.
