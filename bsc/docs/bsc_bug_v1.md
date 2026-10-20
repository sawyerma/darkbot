# BSC Bugfix V1 — gepruefte Korrekturauftraege fuer Astra / Claude

**Stand:** 2026-10-20 · **Etappe:** 1/8, G0.7-Vertragskette · **gepruefter Git-Stand:** `cac786e66725f4ca23b4bb01a65209770099c2c1` (`main`)

**Zweck:** Verbindlicher, quellcodegestuetzter Korrekturauftrag. Dieses Dokument ist **kein neuer Phasenplan**, kein Test-PASS und keine Live-Freigabe. Jeder Befund wurde anhand des genannten Git-Stands und der autoritativen BSC-Vertraege gegen die Fable-Etappe 1 geprueft. Bei einem neuen HEAD muss der Implementierer die relevanten Stellen neu vergleichen, bevor er aendert.

## 0. Verbindliche Hierarchie und harte Arbeitsregeln

1. `bsc/docs/PHASENPLAN.md` ist der Einstieg. Rangfolge: `BSC_DUAL_VENUE_ROADMAP.md` > `GATE0_TESTPLAN_SHORT.md` > `BSC_PRODUCTION_BUILD_MASTER_PLAN.md`; dazu die phasenspezifischen technischen Vertraege, `ACCEPTANCE_MAP.json` und das chronologische `GATE0_MEASUREMENT_EVIDENCE_LEDGER_2026-09-28.md`. Besonders bindend: Roadmap 2026-10-20 „G0.7 CODE READY“ / STOP-Regel, G0.7-Requalifikationsplan §2/§5, Masterplan §§7–8/13.
2. **Nur `main`.** Keine Branches, keine neuen Phasen, kein ungefragter Refactor. Vor jedem Fix aktuellen HEAD und Diff gegen diesen Pruefstand lesen; laufende fremde Aenderungen nicht ueberschreiben. Pro Etappe nur minimal betroffene Dateien und enge Negativ-/Regressionstests.
3. **G0.7 bleibt `UNARMED`, v4 `approved=false` und `execution_ready=false`.** Keine privaten Schluessel auslesen/veroeffentlichen, keine Mainnet-BNB uebertragen, kein Builder-Send und kein echter BUY. `--write-approved`, Arming-Commit oder LIVE werden durch dieses Dokument ausdruecklich **nicht** autorisiert. Das Budget bleibt 481163500000000 wei, maximal ein wirtschaftlicher Versuch, kein Retry.
4. **Vertraege und signierte Bytes bewahren:** `transactions.rs`, die eingefrorenen Launch-/BUY-Hashes und der wirtschaftliche Release-/Transportpfad werden durch Etappe 1 nicht umgebaut. Nach dem Vertrags-Textfix entstehen **neue** v4-Proposal-/Approved-Hashes. Die alten Hashes `5da0068b…` und `4308f2cb…` duerfen danach nicht zur Freigabe dienen; den neuen Approved-Hash nur zur spaeteren ausdruecklichen Entscheidung anzeigen.
5. Vorhandenes R0/DRY **nicht ohne Invalidation wiederholen** (Roadmap 2026-10-20). `PASS_READ_ONLY` ist ausschliesslich technische Readiness, nicht BUY-/Acquisition-PASS. Builder-Inclusion, 48Club-Preis und noMerge/positionFirst-Wirkung bleiben `UNQUALIFIED`; originaler Producer-Witness und `T0 -> T_buy <1 ms` bleiben `UNKNOWN/NOT_PROVEN`. G0.7/G0.8: `NOT_EXECUTED`.
6. Nachweise nur mit exakt dokumentierten Scopes als PASS berichten. Kompilier- und Testbefunde in den bisherigen Statusdateien sind historische Meldungen; fuer diesen neuen Fix noch keine eigenen Testlaeufe behaupten.

### Etappe-1-Urteil (auf HEAD cac786e6)

| ID | Einstufung | Befundstatus | Umsetzung |
| --- | --- | --- | --- |
| E1-1 | Hoch | BESTAETIGT | Pflichtfix, vollstaendige PASS-Verdikt-/Record-Bindung |
| E1-2 | Hoch (Freigabeintegritaet) | BESTAETIGT | Pflichtfix, v4-Prosa/Referenzen und neue Hashes |
| E1-3 | Mittel | BESTAETIGT | Pflichtfix, DRY gegen eingefrorene Vertragsidentitaet |
| E1-4 | Mittel | BESTAETIGT (bereits indirekte Bindung) | Pflichtfix, explizites attempt_id im Guard |
| E1-5 | Mittel | BESTAETIGT | Pflichtfix, korrektes Readiness-Latch-Urteil |
| E1-6 | Niedrig | BESTAETIGT | Pflichtfix, einzeln gepruefte DRY-Flags |
| E1-7 | Niedrig | BESTAETIGT | Pflichtfix, UTF-8-Bytegrenze |
| E1-8 | Info | BESTAETIGTER DIAGNOSE-HINWEIS, KEIN SELBSTAENDIGER SICHERHEITSBUG | Kommentar/korrektes Provenance-Label |
| E1-9 | Prozess | TEILWEISE BESTAETIGT | fehlende Artefakte kennzeichnen; sicher nachreichen |
| E1-OPTION | Ausstehende Entscheidung | Keine aktuelle Vertragsverletzung belegt | Builder-Frische im eigentlichen G0.7-Lauf nur nach gesonderter Entscheidung |

## E1-1 — v4-Generator ignoriert das abschliessende Requalifikations-Verdikt

**Quellen:** `bsc/engine/crates/execution/src/acceptance_contract.rs` (`attempt1_v4_evidence`, ca. Z. 557–624), `bsc/engine/crates/qualification/src/bin/g06a_requal_check.rs` (`evaluate`, `status_record`, ca. Z. 186–565), `bsc/engine/crates/record/src/schema/g06a.rs::requalification`. Plan: `G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md` §2 „Anything else = STOP“.

**Ist:** `attempt1_v4_evidence` akzeptiert beim DRY allein `dry_rec.bool(d::DRY_ALL)` und liest `requalification_status.dbrec` nicht. Der separate Requal-Pruefer kann wegen R0, Fee-/Salt-/Record-Drift oder einer anderen der sechs Einzelpruefungen `STOP` melden, waehrend `DRY_ALL` wahr bleibt. Der Generator kann dennoch einen *noch nicht autorisierten* v4-Vorschlag erzeugen. Der Guard bleibt unabhaengig davon `UNARMED`; eine Live-Ausfuehrung wurde hier nicht nachgewiesen.

**Soll / minimaler Fix:**
1. In `acceptance_contract.rs` den festen `REQUAL_STATUS = "requalification_status.dbrec"` einfuehren. `attempt1_v4_evidence` MUSS das vordefinierte Statusrecord (`g06a::requalification::SCHEMA`) im exakt angeforderten `requal_dir` lesen und `VERDICT == "PASS"` verlangen. Fehlend, falsches Schema, `STOP` oder Lesefehler => kein Vertrag.
2. Nicht Fables blosse `evaluated.contains(sha)`-Pruefung kopieren: der Status-Record hat **Name UND SHA256**. Ueber eine eindeutige Map `record.name -> record.sha256` *genau* die sieben vom Verifier gelisteten Elemente pruefen, ohne Duplikate, Ersatz-Namen, fehlende oder unerwartete Eintraege:
   `preflight.dbrec`, `native_quote.dbrec`, `economics_crosscheck.dbrec`, `local_simulation.dbrec`, `dry_result.dbrec`, `portal_upgrade.dbrec` und die **exakte** Builder-Policy-Datei. Die sechs Requal-Dateinamen sind relativ zum angeforderten `requal_dir`; die Builder-Policy ist der vom Aufrufer uebergebene Pfad. Jeder Vergleich benutzt erneut gelesene Datei-Bytes und deren SHA-256; dabei die nicht schon vom Vertrag gebundene `native_quote.dbrec` **nicht vergessen**.
3. Verifier-Provenienz, `REQUAL_DIR` und `FROZEN_DIR` sinnvoll gegen den tatsaechlichen angeforderten Lauf und das gepinnte Altverzeichnis binden (bei Pfaden keine manipulierbaren Aliase zulassen). Zusaetzlich die **sechs** Verifier-Checks `R0_UPGRADE`, `DRY0_PREFLIGHT`, `DRY1_BYTES_AND_SIGNERS`, `DRY2_SIMULATION`, `DRY3_GAS_ECONOMICS_BUDGET` und `DRY4_EXPIRY_NONCES_ZERO_EGRESS` eindeutig und mit `STATUS=PASS` verlangen. `VERDICT=PASS` allein ist keine inhaltlich ausreichende Bindung.
4. Das Statusrecord hat Integritaetsschutz gegen versehentliche Korruption (DBREC-Codec), aber **keine externe Signatur**. Deshalb daraus keine kryptographische Echtheitsbescheinigung erfinden. Sichere Herkunft, exakt gebundene Artefakte und spaetere Freigabe bleiben eigene Kontrollen; keine Statusdatei synthetisch erzeugen und als echte Mainnet-Requal ausgeben.
5. Die bestehenden Test-Fixtures `execution/tests/acceptance_guard.rs::requal_root` und `execution/tests/g07_readiness.rs::v4_root` minimal um ein klar als **offline-test** gekennzeichnetes Status-FIXTURE erweitern; es muss alle sieben korrekten Recordbindungen und sechs PASS-Checks enthalten. Das ist kein realer Requal-Nachweis.

**Zwingende Tests:** ohne Status; Verdikt STOP bei `DRY_ALL=true`; einzelne Check-STOP trotz Verdikt PASS; fehlender Name; doppelter Name; gleiche SHA unter falschem Namen; manipulierte einzelne Bytes; falscher Policy-Pfad; falscher Requal-Ordner; richtiges unveraendertes Offline-PASS-Fixture. Jeder Negativfall verweigert Vertrag, `--approved-sha` und `--check` gleichermaassen. Keine Netzwerkverbindung und kein Send.

**Abnahme:** Nachweis, dass ein nicht durch `g06a_requal_check` als PASS gebundener kompletter Lauf keine freigabefaehigen v4-Bytes produzieren kann.

## E1-2 — v3-Alttexte in v4-Freigabevertrag

**Quelle:** `execution/src/acceptance_contract.rs::common` ca. Z. 290–347 sowie `attempt1_v4` ab Z. 641. `common()` befuellt Felder mit v3-Text und `attempt1_v4` ueberschreibt nur Teilbereiche.

**Bestaetigte Falschaussagen in v4:**
- `economics_validation.gas_floor.blockrazor_current_documented_reference` verweist auf `authorization.evidence_paths.builder_requirements_refresh` (v3), obwohl die v4-Bindung `authorization.evidence_bindings.builder_policy_record_sha256` massgeblich ist und das alte v3-Feld nicht gesetzt wird.
- `economics_validation.gas_floor.bound_policy_superseded_fields` behauptet, das alte Addendum vom 2026-10-11 bleibe per DRY gebunden. Die neu bestimmte native Policy ist die aktive Builder-Floor-Evidenz; die alte Aussage ist nicht die v4-Bindungssemantik.
- `budget_wei.note` nennt den historischen Head 127865760, `eth_estimateGas=5502725` sowie Simulation 5251225 + 334654 als aktuelle Begruendung. Die v5.25.0-Requal basiert auf aktualisierten Feldern; die Grenzwerte blieben lediglich unveraendert.

**Fix:** Unmittelbar nach `common(&mut d, approved)?` in `attempt1_v4` diese drei Texte aus **gebundener nativer** Policy/Simulation/Funding und den unveraenderten Vertragsobergrenzen erzeugen. Wo ein konkreter Messwert nicht bereits im `V4Evidence` validiert vorliegt, stattdessen auf den zugehoerigen gebundenen DBREC-Datensatz verweisen und nicht frei Zahlen uebernehmen. `budget_wei.note` muss klar zwischen Gaslimit/Worst-Case-Reserve und historisch gemessenem Gas unterscheiden. Auch weitere v3-Formulierungen/kaputte Feldreferenzen des ausgegebenen v4-Textes einmal gezielt inventarisieren; keine wirtschaftlichen, numerischen oder signaturrelevanten Felder aendern.

**Tests:** Dekodiertes v4-Proposal und Approved-Variant duerfen die drei widerlegten Aussagen und tote Feldreferenzen nicht mehr enthalten. Obergrenzen `50000000 wei` Gas, `6603270 + 3000000` Gaslimit, `1000000000000 wei` BUY, `481163500000000 wei` maximales Gesamtbudget, genau ein Versuch und alle unveraenderten signierten Transaktionshashes pruefen. Proposal- und Approved-SHA-256 **neu berechnen und zur Pruefung vorlegen**, die alten nur als historische Referenz behalten. Keine automatische Approved-Datei schreiben.

**Abnahme:** Verstaendliche, sachlich zutreffende v4-Prosa, eindeutige aktive Evidenzreferenzen, unveraenderte Limits, frischer SHA-256.

## E1-3 — Vertrags-Identitaet erst in Readiness/Guard statt bereits im Generator

**Quelle:** `execution/src/acceptance_contract.rs::common` (feste Wallets, attempt_id, Chain 56, Budget/Gas) und `attempt1_v4_evidence` (vorher nur DRY_ALL). Readiness/Guard verweigern viele Abweichungen spaeter, der Generator selbst nicht.

**Fix:** Feste, einmal definierte Konstanten fuer `ATTEMPT_ID`, `LAUNCHER` und `BUYER` in der Vertragsquelle verwenden und die DRY-Wallet-/Chain-/Portal-/Nonce-/Gas-/Wert-/Limit-/Hash-Identitaet vor dem v4-Encode mit dem eingefrorenen Kandidaten und Requal-PASS vergleichen. Witness-Source-Record nicht wegwerfen, sondern seine `ATTEMPT_ID` direkt mit dem v4-`attempt_id` vergleichen. Die verwendeten Felder in `dry`, `preflight`, `witness` und festem Budget konsistent validieren. Numerische Obergrenzen nur dort als `<=` behandeln, wo laut Vertrag tatsaechlich ein Limit (statt eingefrorener exakter Transaktionsidentitaet) vorgesehen ist. Keine stillschweigende Anpassung an abweichende Eingaben.

**Tests:** vertauschte Wallets, falsche Chain/Portal, abweichende Nonces oder signiertes Gas/Wert, ueberschrittenes Limit, anderer Witness-`attempt_id`, falscher TX-Hash => Generator STOP vor der SHA-Ausgabe; guter Requal-Lauf bleibt reproduzierbar.

**Abnahme:** Kein Vertrag mit offensichtlich unvereinbaren DRY-/Witness-/Vertragswerten wird erzeugt. Unveraenderte richtige Eingaben produzieren dieselben wirtschaftlichen Bytes ausser den von E1-2/E1-1 begruendeten Metadaten-/Textaenderungen.

## E1-4 — attempt_id fehlt im expliziten LiveGuardContext

**Quelle:** `execution/src/acceptance_guard.rs::LiveGuardContext`, `snapshot` und `verify_live_authorization` (ca. Z. 22–380). `snapshot` liest die Vertrags-ID; der Kontext besitzt keine ID. `main.rs` prueft den ID-bezogenen Witness bisher separat. Die **indirekte** Witness-Hash-Bindung mindert das Risiko, ist aber keine direkte Guard-Pruefung.

**Fix:** `attempt_id: String` in `LiveGuardContext` aufnehmen; im Guard die exakte Gleichheit zur `ContractSnapshot.attempt_id` vor Freigabe pruefen. Das G0.7-`main.rs`-Literal mit der **bereits validierten** Laufzeit-ID befuellen, in `execution/tests/acceptance_guard.rs::matching_context` und allen weiteren Konstruktionen ergaenzen. Bestehende Witness-/Hash- und Latch-Pruefungen unveraendert lassen; keine zweite Attempt-ID-Quelle erfinden.

**Tests:** exakt passende ID akzeptiert die isolierte Identitaetspruefung; abweichende, leere und manipulierte ID scheitern. Solange `FROZEN_APPROVED_V4_SHA256=UNARMED` ist, muss der umfassende Live-Guard weiterhin **vor jedem Vertragslesen** verweigern. Fuer Tests ggf. den reinen Vergleich auslagern; niemals den produktiven Guard scharfschalten.

## E1-5 — Readiness kann Latch aus einem anderen Checkout beurteilen

**Quelle:** `execution/src/bin/g07_pre_live_readiness.rs` ca. Z. 303–325: `symlink_metadata(bsc/.local/g07/<attempt_id>)` relativ zum Arbeitsverzeichnis, ohne Checkout-Pruefung. `execution/src/bin/g07_mainnet_self_launch_buy/inputs.rs::assert_repository_root` ca. Z. 109–132 verlangt fuer den **eigentlichen Runner** hingegen Haupt-Checkout (`.git` ist Verzeichnis), v4 am festen Root-Pfad und `bsc/.gitignore` mit `/.local/` ohne Negation.

**Fix:** Readiness muss das Latch fuer **denselben** Haupt-Checkout wie der Runner beurteilen, ohne Zustandsaenderung. Ist Root/Checkout nicht sicher festgestellt (einschliesslich `.git` und erforderlicher Ignore-Regeln), `ATTEMPT_LATCH=BLOCKED`, niemals PASS. Existiert dort die Attempt-State-Directory oder ist deren Inspektion fehlerhaft, ebenfalls BLOCKED/STOP. Nur nach sicherem Root-Check und nachgewiesen fehlendem State darf `ATTEMPT_LATCH=PASS` gesetzt werden. **Nicht nur `.git` als Verzeichnis pruefen** und danach einen beliebigen Ordner als sicheren Root deklarieren. Root-Pruefung als Ergebnis statt Panic, damit Offline-Fixtures ein vollstaendiges Readiness-Record mit BLOCKED erhalten; keine voreiligen `VALIDATION`-Fehler fuer lediglich anders angelegte Test-Fixtures.

**Tests:** Haupt-Checkout mit und ohne Latch, verlinktes Worktree (`.git` Datei), Temp-Ordner ohne Root, negierte `/.local/`-Ignore-Regel, nicht lesbares/symlinkiertes Attempt-State-Objekt. `PASS_READ_ONLY` darf im falschen Checkout nicht erreichbar sein; LIVE immer blockiert. Realen Latch niemals loeschen oder umbenennen.

## E1-6 — G0.7 prüft nur das aggregierte DRY_ALL

**Quelle:** `execution/src/bin/g07_mainnet_self_launch_buy/main.rs` ca. Z. 77–82. Einzige direkte Flagpruefung: `DRY_ALL`. Readiness verlangt auch `DRY_1`, `DRY_2`, `DRY_3`, `DRY_4` und `ZERO_EGRESS_STRUCTURAL`.

**Fix:** Vor Schluessel-/Latch-/Live-Freigabe **alle sechs** Flags explizit verlangen, ausserdem `DRY_ALL` und die bestehenden Status-/Hashpruefungen beibehalten. Fables kleine Schleife ueber die sechs Felder ist sachlich richtig. Nur einen veralteten Header-Kommentar `v3` -> `v4` aendern, wenn er im betroffenen Modul tatsaechlich noch besteht.

**Tests:** jeweils genau ein individuelles Flag falsch bei `DRY_ALL=true` => Fail-closed/kein Send/kein Latch. Richtiges Offline-Golden => gleiches Verhalten/gleiches signiertes Hashpaar. Guard bleibt UNARMED.

## E1-7 — Verdikt-Detail in Zeichen statt UTF-8-Bytes begrenzt

**Quelle:** `qualification/src/bin/g06a_requal_check.rs::status_record` ca. Z. 510–536: `c.detail.chars().take(4000)`. DBREC-Text hat eine **Byte**-Grenze 4096; 4000 mehrbyteige UTF-8-Zeichen koennen darueber liegen. Dann `encode().expect` -> Panic ohne Statusfile. Dies ist kein unberechtigtes PASS, aber unnoetig instabil.

**Fix:** Durch UTF-8-sicheren **Byte**-Prefix mit einer bewusst konservativen Grenze (etwa 4000 Bytes) ersetzen; am `char_boundary` abschneiden, keine ungueltigen UTF-8-Daten erzeugen. Keine Codec-, Status- oder Sicherheitssemantik aendern.

**Tests:** ASCII exakt/an der Grenze, 2/3/4-Byte-Zeichen, laengerer gemischter Fehlertext; DBREC-Encode/Decode ohne Panic, `VERDICT=STOP` bleibt STOP und der Fehlertext wird nur begrenzt, nicht faelschlich in PASS umgewandelt.

## E1-8 — TAX_V3_IMPL-Driftfeld bezieht seine Adresse aus einer Konstante

**Quelle:** `execution/src/bin/g07_mainnet_self_launch_buy/chain_reads.rs::run_current_preflight` ca. Z. 347–490 setzt `TAX_V3_IMPL` aus `TAX_V3_IMPL_EXPECTED` statt aus einer live extrahierten Portal-Referenz; `schema/g06a.rs::preflight::DRIFT_CHECKED` enthaelt das Feld. Live werden dagegen Portal-Implementation und ihr Codehash abgerufen und geprueft; laut Requalifikationsbericht befindet sich die relevante TAX_V3-Referenz in den Immutables des Portal-Bytecodes.

**Einordnung:** Kein durch diesen Befund belegter Erwerbs-/Safety-Bypass, weil eine geaenderte Portal-Implementation unter den aktuellen Hash-Gates STOP ausloest. Der Adressfeldvergleich selbst liefert **keine unabhaengige** Live-Aussage.

**Umsetzung:** Knappen Kommentar an der Befuellung und ggf. in der Drift-Dokumentation anbringen: konstantenbasiertes Identitaetsfeld, tatsaechlicher Schutz ueber abgefragten Codehash und gepinnte Referenz. Nicht ohne neue Evidenz ein zweites Contract-Discovery-System bauen und nicht den Portal-Codehash-Guard entfernen. Negativtest fuer Portal-Codehash-Drift erhalten.

## E1-9 — Requal-Artefakte auf GitHub nur teilweise vorhanden

**GitHub-`main`-Tree auf cac786e6 bestaetigt:** `bsc/docs/G07_REQUALIFICATION_RESULT_20261020.md` **ist versioniert**. Dagegen fehlen `bsc/config/g07_attempt1_acceptance_v4.dbrec` und ein `bsc/native/evidence/g06a/g06a_requal_dry_*`-Verzeichnis. Die Aussage „alle drei fehlen“ von Fable ist **falsch**.

**Prozessauftrag:** Vor einer wirtschaftlichen Freigabe muessen die gebundenen nativen DRY-/Requal-Artefakte und der exakte vorgeschlagene v4-Vertrag fuer unabhaengige Pruefung verifizierbar sein, samt `SHA256SUMS`, Source-/Build-/Scope-Bindung und reproduzierbarem `--check`. Vor jeder Veroeffentlichung **einzeln** auf private Schluessel, Secret-Strings, Session-/Auth-Tokens, private Journal-/Latch-Daten und noch ausfuehrbare rohe signierte TX-Bytes pruefen; solche Bytes duerfen nicht in Git. Falls sichere Veroeffentlichung nicht moeglich ist, manipulationssichere private Artefaktablage mit validierbaren Hashes und dokumentiertem Zugriff nutzen; Status nicht als verifiziert hochstufen. Historisches Ledger **append-only** ergaenzen, keine alten Messungen ueberschreiben. Weil die Artefakte im GitHub-Tree fehlen, hier keine lokale SHA-/Datei-Verifikation und **keine** Behauptung, sie seien erneut getestet.

## E1-OPTION — Builder-Policy-Frische auch im eigentlichen G0.7-Lauf

Fable schlaegt zusaetzlich eine strikte 24h-Pruefung direkt in `main.rs` vor. **Urteil:** Das ist eine moegliche Safety-Verschaerfung, aber das aktuelle Vertragswortlaut verlangt die Frische vor dem Scharfschalten und die Readiness kontrolliert sie bereits. Aus dem Befund allein folgt keine gesicherte aktuelle Vertragsverletzung. **Nicht stillschweigend implementieren**. Bei spaeterer ausdruecklicher Freigabe vor Latch/Write pruefen; importierte Policy ohne Fetch-Timestamps und Zukunftszeiten sicher ablehnen. Jedwede geaenderte Laufzeit-Gate-Semantik erneut reviewen und regressieren; keine automatische Fee-/Gas-Anhebung.

## 1. Implementierungsreihenfolge und Regressionen

1. **E1-7** UTF-8-sichere Verdict-Datei; danach **E1-1** die eindeutige Status-/7-Record-/6-Check-Bindung. Bestehende Offline-Fixtures sauber aktualisieren. Keine synthetischen PASS-Evidenzen als real ausgeben.
2. **E1-3** Generator-Identitaet und Witness, dann **E1-2** v4-Prosa samt Hash-Neuberechnung. Vertragszahlen, TX-Bytes und Hashreferenzen weiter einfrieren.
3. **E1-4** explizite Guard-ID; **E1-5** Checkout-/Latch-Pruefung; **E1-6** einzelne DRY-Flags; **E1-8** nur Kommentar. Minimal notwendige Tests fuer jede betroffene Sicherheitsentscheidung.
4. **E1-9** Nachweisverfuegbarkeit dokumentieren; Artefakte nur nach Geheimnis-/Broadcast-Risiko-Pruefung sicher bereitstellen. Alte Status-Hashes fuer v4 als `SUPERSEDED` markieren, nicht als aktive Freigabe.
5. Abschlussbericht mit Datei/Zeile, minimalem Diff, allen Red-/Green-Faellen, Rueckwaertskompatibilitaet, Hash-Differenz und expliziten NON-CLAIMS. Protokollierte bekannte Risiken duerfen nicht als Test-PASS umetikettiert werden.

**Gezielte Kommandos (erst nach Umsetzung, von `bsc/engine`):**
~~~bash
cargo test --locked -p darkbot-bsc-execution --features g07-mainnet
cargo test --locked -p darkbot-bsc-qualification --bin g06a_requal_check
cargo fmt --all -- --check
cargo clippy --locked -p darkbot-bsc-execution --features g07-mainnet --all-targets -- -D warnings
~~~

Clippy-Warnungen nur gegen dokumentierten Baseline-Stand bewerten, nicht blind unrelated Code aendern. Dann offline Vertrag `--check` und `--approved-sha` reproduzieren, **ohne `--write-approved`**. Keinen echten R0-Scan, keine erneute DRY-Signatur und keine Mainnet-Submission als Teil dieser Etappe starten.

**Etappe-1-Abnahme:** E1-1..E1-7 nachweisbar behoben; E1-8 sachgerecht kommentiert; E1-9 wahrheitsgemaess als vorhanden/lokal/unverifizierbar erfasst; die kompilierten Guard-Konstanten bleiben `UNARMED`; das alte signierte Transaktionshashpaar bleibt unveraendert; neues v4-Proposal bleibt unapproved; keine reale Transaktion gesendet. Diese dokumentierte Korrektur ist **keine** Freigabe fuer G0.7 oder fuer Phase C1/C2.

## 2. Nachfolgende Etappen

Weitere durch den Nutzer gelieferte Fable-Etappen werden in **dieselbe Datei** aufgenommen, aber erst nach unabhaengigem Code-/Phasenplanabgleich. Bereits behandelte Behauptungen nicht doppelt als Bug anlegen. Auch nach erfolgreicher Umsetzung bleibt eine Etappe „FIXED / TESTED“ erst mit konkretem Commit und Tests/Evidence belegbar; sonst `OPEN`.

| Etappe | Geplanter Gegenstand | Stand |
| --- | --- | --- |
| 1 | G0.7 Guard/Vertrag/Readiness/Requal | **REVIEWED / FIXES OPEN** |
| 2 | G0.8 Verifikation/Witness/Evidence | AWAITING INPUT |
| 3 | G0.6a DRY-/Requal-Werkzeuge | AWAITING INPUT |
| 4 | Trading/Signer/RLP/Intent | AWAITING INPUT |
| 5 | Execution/Submission/TLS | AWAITING INPUT |
| 6 | geth_ipc Rust | AWAITING INPUT |
| 7 | Go-geth Overlay/Rust-Paritaet | AWAITING INPUT |
| 8 | Journal/Records/Legacy | AWAITING INPUT |
