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

## 2. Etappe 2 — G0.8 Verifier, Producer-Timing und Qualification-Crate

**Geprueft am:** 2026-10-20 · **Basis:** `main` Commit `09fb64b9b97704d65f99383580baf6a2d65ca305` (gegenueber `cac786e` nur diese Bugdatei zusaetzlich) · **Fable-Etappe:** 2/8 · **Codefix-Status:** OFFEN. Die genannten Tests wurden hier **nicht ausgefuehrt**.

**Bindende Regeln:** `BSC_DUAL_VENUE_ROADMAP.md` (2026-10-20), `GATE0_TESTPLAN_SHORT.md` §§2 und Capture-Anhang, `BSC_BIRTH_ENTRY_ARCHITECTURE.md` §1, `G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md` §5, G0.7-v4-Vertrag (`thresholds.ADAPT/STOP`, `retention_observability`). Keine BSC-LIVE-Ausfuehrung, keine neuen Wirtschaftstransaktionen, `UNARMED`, ein Intent, globale Nonce-/Replay-Sicherung; G0.8 hat **ausschliesslich** Nachweis-/Reconciliation-Autoritaet.

### Etappe-2-Urteil

| ID | Einstufung | Nach Code-/Planabgleich | Konsequenz |
| --- | --- | --- | --- |
| E2-1 | Hoch, Safety-Semantik | **TEILWEISE BESTAETIGT:** G0.8 kann einen Ablauf nicht positiv dokumentieren; `UNRESOLVED` ist bis zum Beweis aber richtig | Keine voreilige `ADAPT`-Freigabe; nur sichere Evidenz-Erweiterung |
| E2-2 | Mittel | **BESTAETIGT:** Receipt-Poller verwirft RPC-/Decode-Fehler | Pflichtfix: Fehlerursache und begrenzte Wiederholung |
| E2-3 | Mittel | **BESTAETIGT MIT BEDINGUNG:** Attribution-Err + unvollstaendige historische State-Reads endet als `ADAPT`; `STOP` nicht pauschal | Nach E2-4 differenzierte Entscheidungslogik |
| E2-4 | Blockierende Ungewissheit fuer Attribution | **ABI STRUKTUR BESTAETIGT, Tax-v5.25.0-Semantik UNVERIFIZIERT** | Read-only realen Receipt pruefen; sonst keine harte Zuordnungsannahme |
| E2-5 | Mittel | **BESTAETIGT:** einmalige RPC-Lookups, Capture-Fehler und ungekuerzter Evidence-Text | Pflichtfix: bounded RPC-Recovery und fehlerfeste Evidence |
| E2-6a | Messdefinition | **WIDERLEGT:** Pre-signiertes Self-Launch-Bundle invalidiert `T0 -> T_buy` nicht | Keine Aenderung der autoritativen Timing-Formel oder Hooks aus dieser Begruendung |
| E2-6b | Niedrig bis Mittel | **BESTAETIGT:** Timing-Verdikt nur optional persistent | Pflichtfix: Output-Evidence bei Verdict verpflichtend |
| E2-7 | Hoch, Build-/Phasenscope | **BESTAETIGT:** G0.8-Qualification kompiliert ungefragt Root-`protocol/schemas` | Pflichtfix: Status-Reporter-Feature von G0.8 entkoppeln |
| E2-8 | Niedrig | **BESTAETIGT, AUSSER SCOPE:** Reporter-Socket-/Readiness-Label | Zurueckstellen, keine GUI-/Gateway-Baustelle jetzt |
| E2-9 | Info | Teilweise bekannte, nicht neu qualifizierte Einschraenkungen | Offenlegen, keine pauschale Verschärfung/Finalitaetsbehauptung |

### E2-1 — Nichtaufnahme/Ablauf: `UNRESOLVED` nicht unbewiesen zu `ADAPT` aendern

**Codebeweis:** `bsc/engine/crates/qualification/src/bin/g08_mainnet_verify.rs` ca. Z. 1060–1108 liest elf G0.7-Felder und setzt bei fehlendem Launch- oder BUY-Receipt stets `unresolved::VERDICT=UNRESOLVED`, Exit 2. Die G0.7-Evidence enthaelt `evidence::LAUNCHER_NONCE`, `BUYER_NONCE` (Pflichtfelder) und `evidence::BUNDLE/bundle::MAX_BLOCK` (optionaler Bundle-Record), wird bisher fuer diese Entscheidung nicht verwendet. Laut v4-Vertrag ist nachweislich abgelaufen/nicht aufgenommen ein `ADAPT`-Experimentausgang, **unaufgeklaerte versendete TX** dagegen `STOP/UNRESOLVED` und niemals eine Startfreigabe.

**Fable-Patch 1 nicht unveraendert einsetzen.** `head > maxBlock+3 && nonce_latest == frozen && eth_getTransactionByHash == null` ist nur eine **Momentaufnahme eines Knotens**. Sie beweist weder anbieteruebergreifende Propagation noch, dass rohe gueltige TX-Bytes nicht spaeter ausserhalb des Bundle-Fensters verarbeitet werden. Die Signaturen koennen mit noch freier Nonce fortbestehen. Deshalb kann dieser Test **keine** globale Nichtausfuehrung oder Wiederverwendbarkeit des Attempts garantieren. Auch darf die Schemaklasse `darkbot.g08.mainnet-acquisition-unresolved.v1` nicht durch Setzen ihres `VERDICT` auf `ADAPT` semantisch umgewidmet werden.

**Phasentreuer Korrekturauftrag:**
1. Ohne beide Receipts fortgesetzt **`UNRESOLVED`/STOP-gegen-Retry** ausgeben. Zusatzinformationen zu Head, gebundenem `max_block`, exakten Launcher-/Buyer-Nonces und TX-Hash-Lookup duerfen als *explizit nicht abschliessende* `expiry_observation` in einem versionierten DBREC-Evidenzrecord protokolliert werden; niemals als Bestaetigung von Nichtausfuehrung aufgrund nur dieser Momentaufnahme.
2. Falls zusaetzlich ein **nachweisbar abgeschlossener** Non-Inclusion-/Expiry-Status gemaess v4-Vertrag technisch beweisbar ist, seine Voraussetzungen explizit mit kanonischem Block-/Receipt-Coverage, ausreichend abgestimmter Chain-Sicht, Konsistenz beider Nonces, Builder-/Transport-Scope, evt. Mempool-Beobachtung und Ablauf-/Reorg-Sicherheit spezifizieren. Fallback bei jeder fehlenden Beobachtung: `UNRESOLVED`. Keine automatische Wiederholung und keine eigenmaechtige Armung.
3. Ein **Nonce-Burn** (zwei neue 0-Wert-Transaktionen, Gas-/Egress-Kosten und neue Signaturen) ist eine zusaetzliche wirtschaftliche Handlung, veraendert Intent-/Nonce-Risiko und liegt **ausserhalb** des derzeit autorisierten einen G0.7-Experiments. Weder automatisch implementieren noch senden. Separate Design-/Budget-/User-Freigabe erforderlich, falls spaeter ueberhaupt verfolgt.
4. Bei neuen optionalen G0.7-Lesefeldern: `execution/tests/g07_no_send.rs` oder Source-Test der `G08_READS`-Liste und `qualification/tests/g08_binary_records.rs::g08_reads_exactly_the_twelve_g07_fields` synchronisieren; **Pflicht- und optionale Lesestellen separat** pinnen. Provisional Evidence kann `BUNDLE` bewusst noch nicht enthalten.

**Gezielte Tests:** kein Receipt / 1 Receipt / beide Receipts; abgelaufen aber unbekannter Providerstatus; gleicher/veraenderter Nonce; Fehler in `eth_getTransactionByHash`; Reorg; falscher Head; G0.7-Provisional ohne Bundle. In allen Unklarheiten `UNRESOLVED`, kein Retry, kein `ADAPT`-Scheinerfolg. Erst ein separat bewiesener geschlossener Ausgang kann `ADAPT` erhalten.

### E2-2 — Receipt-Poller verwirft Fehlerursachen

**Code:** `qualification/src/bin/g08_mainnet_verify.rs::wait_receipt`, ca. Z. 604–614: `Ok(None) | Err(_) => sleep`, maximal `240 * 500 ms` **pro Transaktion**. So sehen dauerhaft unlesbare Receipts wie unbekannte/nicht vorhandene Transaktionen aus.

**Fix:** Poller als typisiertes Resultat mit `Some(Receipt)`, `NotYetIncluded` und Fehlerdiagnostik (letzter Fehler **und Fehleranzahl**, optional Klasse RPC/HTTP/JSON/Decode) gestalten, bei transienten Fehlern begrenzt weiter pruefen, bei permanenten Schema-/Chain-Problemen erkennbar stoppen. Der Polling-Bound bleibt endlich; kein unkontrollierter Neustart. Ergebnisursache bytebegrenzt im existierenden fail-closed `unresolved`-Record oder einem gezielt erweiterten DBREC-Evidence-Feld persistent halten. `UNRESOLVED` darf nicht zu `ADAPT`/PASS hochgestuft werden.

**Besondere Provider-Kompatibilitaet:** `wire::receipt_of` ca. Z. 379–407 verlangt fuer **jedes** Receipt-Log `removed == false`; `removed` fehlt bei manchen `eth_getTransactionReceipt`-/`eth_getBlockReceipts`-Implementierungen. Vor einer Decoder-Aenderung den erwarteten JSON-RPC-Feldvertrag mit realen, vertrauenswuerdigen Antworten pruefen. `removed=true` **immer ablehnen**; fehlendes `removed` nur bei belegter semantischer Gleichwertigkeit durch andere kanonische Block-/Receipt-/Log-Pruefungen akzeptieren. Niemals die Reorg-/Identitaetspruefungen pauschal entfernen.

**Tests:** RPC-Timeout, JSON-Syntax, `null`, echtes Receipt, Log ohne/mit `removed`, `removed=true`, korruptes Receipt; Fehlerdiagnose nach dem Bound und unveraenderte keine-Send-Grenze.

### E2-3 — Bei fehlerhafter Kaufzuordnung und unvollstaendiger State-History zu milde Verdict-Logik

**Code:** `g08_mainnet_verify.rs` ca. Z. 1160–1256. `wire::code(token, block)` und `balance_of_at(...).ok()` verschlucken Archiv-/State-Fehler. `hard_stop` prueft `(acquisition_evidence_complete && !acquisition_positive)`. Liefern eventbasierte `attributable_acquisition(...)`-Pruefungen einen Fehler, aber eine historische State-Abfrage keinen Wert, entsteht aktuell `ADAPT` statt eines fehlerdifferenzierten, nachvollziehbaren Resultats.

**Fix nach E2-4:** Ergebnisarten auseinanderhalten: (a) **semantisch nachweislich falscher BUY** bei vollstaendigen validierten Receipts/Logs/Event-ABI -> `STOP`; (b) **fehlende Archivdaten / RPC-Decode- oder Provider-Ausfall** -> `UNRESOLVED/INCOMPLETE`, niemals PASS und niemals ein als wirtschaftliches Scheitern missdeutbares `ADAPT`; (c) fachlich valides Acquisition-Ergebnis, aber Nicht-Adjazenz/Outbid/abgeschlossener Fehlversuch -> vertragliches `ADAPT` nur bei nachgewiesenem Scope. Fehlerursachen und fehlende Felder versioniert im DBREC-Evidence erhalten. Keine Tests mit frei erfundenen Eventwerten zur Beweisfuehrung ueber reale Flap-v5.25-Semantik verwenden.

**Tests:** Event fehlt / falscher Buyer / falscher Token / falsches Amount-Feld / historisches `eth_getCode` missing trie / `balanceOf` revert / Code nachweislich abwesend / zero balance / positiver Net-Transfer. Nachweis, dass fehlende State-History niemals `PASS` erzeugt, aber auch keinen falschen qualitativen `STOP` aufgrund eines nur unerreichbaren Knotens.

### E2-4 — `TokenBought`-Bedeutung fuer v5.25-Tax-Tokens ist nicht verifiziert

**Code:** `g08_mainnet_verify.rs::portal_buys` ca. Z. 643–671, `attributable_acquisition` ca. Z. 672–726; `TokenBought(uint256,address,address,uint256,uint256,uint256,uint256)` (7 nicht-indizierte Woerter), Betrag `event.amount` an `Transfer`-Netto, `event.value` an `buy_tx.value`, Gebuehr `<= buy_tx.value`. Das Event-ABI ist strukturell mit der dokumentierten Flap-`IPortal`-Definition kompatibel: https://github.com/flap-sh/FlapVaultExample/blob/main/src/flap/IPortal.sol und https://docs.flap.sh/dev/trade-tokens . **Damit ist nicht die aktuelle v5.25.0-Zuordnungssemantik bei tax-aktiviertem Kauf bewiesen.** Die vorhandenen Rust-Tests erzeugen synthetische Event-Logs; kein entsprechendes originales Mainnet-Receipt liegt in dieser Pruefung vor.

**Vorgehen nur read-only, VOR einer harten Verdict-Aenderung:** Einen kanonischen erfolgreichen **realen, zeitlich zum Portal-v5.25.0-Upgrade passenden Mainnet-Receipt** eines `swapExactInput` fuer ein vergleichbares Bonding-Curve-Tax-Token heranziehen. Bindung von Portal/Implementation-Version, Token-Tax-Konfiguration, TX-`value`, event `eth/fee/amount`, Transfer-Netto an den richtigen Buyer und ggf. direktem/Router-Aufruf nachvollziehbar machen; echten Receipt-/TX-/Blockhash dokumentieren. Keine private Signatur, kein eigener BUY und kein synthetischer Log als Ersatz. Falls Werte ungleich sind, den Event-/Tax-Weg vor einem Echtgeld-G0.7-Test korrigieren und gezielte Golden-/Negativtests einfuehren. **Bis dahin: UNVERIFIED, nicht als FIXED oder bewaehrte v5.25-Tax-Attribution ausgeben.**

### E2-5 — Einmalige RPC-Lookups und fehleranfaellige Evidence-Ausgabe

**Codebeweise:** `g08_mainnet_verify.rs` ca. Z. 1090–1160: `wire::transaction(...).expect`, `wire::block(...).expect`, anschliessend ca. Z. 1258 `wire::block_number().expect`; einige pruefende Re-Reads `.ok()` werden ohne Ursache zu false. `capture_chain_window` ca. Z. 940–1001 wartet auf `Ok(None)`, bricht bei `Err` direkt ab und markiert spaeter `CHAIN_WINDOW_COMPLETE=false`. `result::CHAIN_JOURNAL_ERROR` ca. Z. 1368 bekommt unbegrenzt Fehlertext; die DBREC-Textgroesse ist endlich. Die **vor dem ersten RPC dauerhaft geschriebene Pending-Evidence** (Z. 1077–1088) bleibt dabei ein vorhandenes, wichtiges Safety-Fence; nicht abbauen.

**Fix:**
1. Kleine **bounded Retry-/Recovery-Funktion** fuer Read-only-RPCs, mit dokumentierter Maximalzeit / Versuchen, unterscheidbaren `Ok(None)`, transienten Fehlern und dauerhaften Identitaets-/Decode-/Chain-Konflikten. Bei ausbleibender Transaction oder Block nicht `panic!` ohne terminale Diagnose, sondern die vorsorglich angelegte Pending-Evidence atomar mit einem aussagekraeftigen, eindeutigen `UNRESOLVED`-Grund und Quelleninformationen aktualisieren, soweit noch sicher moeglich. **Kein automatischer Send-/Retry-Start**.
2. In Chain-Capture einzelne temporäre RPC-Fehler begrenzt wiederholen; Backfill-/Horizon-/Parent-Chain-/Receipt-Vollstaendigkeit muss weiterhin exakt geprueft werden. Teilweise geschriebene Chain-Journals niemals als vollstaendige Evidenz deklarieren. Wenn eine Unterbrechung auftritt: `INCOMPLETE/UNRESOLVED` mit konkreter Ursache, nicht stillschweigend erfolgreicher `ADAPT`.
3. Fehlermeldungen vor Einfuegen in `CHAIN_JOURNAL_ERROR` und weitere textbegrenzte DBREC-Felder UTF-8-sicher auf **Bytes**, nicht Zeichen, begrenzen; rohe Fremdantworten/secrets nicht unredigiert aufnehmen. `write_atomic`-fsync/rename-Pruefungen beibehalten. Reconciliation bei wieder anlaufendem RPC muss mit bestehender Evidenz und ohne neue wirtschaftliche Transaktion moeglich bleiben.
4. Fables pauschale 20-Versuche/250-ms-Schleife nicht ohne Deadline-/Worst-Case-Budget uebernehmen; die eigentliche 15-s-Capture ist eine zeitliche *Beobachtungsabdeckung*, nicht der maximal erlaubte historische Backfill. Den 15-s-Zeitraum genau erhalten und die Wartebudgets als eigene Kennzahlen dokumentieren.

**Tests:** einmaliger/anhaltender RPC-Ausfall in jeder kritischen Phase, fehlende historische Trie, `null`-Block, Canonical-Reorg, bis 4096+ UTF-8-Bytes Fehlermeldung, Capture-Unterbrechung. Niemals scheinbar abgeschlossenes Journal oder `PASS` erzeugen; Pending-State bleibt bei Absturz nachvollziehbar.

### E2-6 — Timing: Messdefinition erhalten, Verdict immer persistieren

**E2-6a WIDERLEGT (kein Fix):** Fable behauptet, dass wegen vor `T0` signiertem Self-Launch-BUY ein Producer-`T0 -> T_buy` nur intra-Bundle und deshalb **nicht** die Produktmetrik sein koenne. Der autoritative `BSC_BIRTH_ENTRY_ARCHITECTURE.md` §1 und `GATE0_TESTPLAN_SHORT.md` §2 definieren **`T0` als ersten Zeitpunkt tatsaechlicher Kaufbarkeit** und **`T_buy` als unsere erfolgreiche on-chain BUY-Ausfuehrung**. Explizit zugelassen ist ein vor `T0` signiertes/eingereichtes geordnetes Launch→BUY-Bundle. Bei **Original-Producer-Hooks genau an diesen semantischen Grenzen** kann deshalb auch die intra-Bundle-Ausfuehrungsluecke den korrekten `T_buy - T0`-Wert liefern. Das ist **keine** Aussage ueber `T_signal -> T_send`; diese getrennte Reaktionsmetrik muss weiterhin separat berichtet werden. Aus Block-Timestamps, Receipt-Index oder ACK darf nie der sub-ms-Wert extrapoliert werden. Fables vorgeschlagene Aenderung von `v::AUTHORITY` zu „not product latency“ widerspricht diesem verbindlichen Definitionsrahmen und **darf nicht implementiert werden**.
 
**E2-6b BESTAETIGT:** `qualification/src/bin/g08_producer_timing.rs` ca. Z. 309 schreibt die Verdict-Evidence nur falls `DARKBOT_G08_TIMING_EVIDENCE_PATH` gesetzt ist. Damit kann stdout `PASS` enthalten, obwohl keine dauerhaft gesicherte binäre, gehashte Timing-Verdict-Datei existiert. Vor jeder Verdict-Berechnung einen **verpflichtenden** sicheren Outputpfad verlangen und den atomaren/fsync-Schreibvorgang vor Anzeige/Exit durchfuehren. Ohne Outputpfad oder bei Schreibfehler **kein `PASS`**; keine neue Hook-Definition und keine Aenderung der `<1000000ns`-Grenze. Test: nicht gesetzter Pfad => kein PASS/stdout und Exit ungleich 0; nicht beschreibbarer Pfad => kein PASS; positiver Witness-Fixture -> DBREC mit Hashes beider Inputs und genauem konservativem Upper-Bound.

**Aktuelle reale Evidenz:** Originaler Producer-Witness fuer den geplanten Mainnet-Versuch fehlt, `T0 -> T_buy` bleibt `UNKNOWN`. Ein Offline-Test mit erfundenen NS-Werten ist keine Produktmessung.

### E2-7 — G0.8-Build haengt zwingend von Root-`protocol/` ab

**Code:** `qualification/build.rs::main` ruft immer `compile_status_protocol()` auf; dessen Protobuf-Quellen liegen unter `../../../../protocol/schemas`. `qualification/Cargo.toml` hat stets `prost`, `prost-build`, `protox` und `lib.rs` bindet `pub mod status;` unbedingt ein; `status/publisher.rs` verwendet `include_str!("../../../../../../protocol/schema_id.txt")`. Das betrifft damit auch **`g08_mainnet_verify`** und die DRY-/Requal-Werkzeuge, obwohl sie das GUI-/Gateway-Protokoll nicht brauchen. Eine Root-Protokoll-Aenderung kann Qualification-Builds unnoetig invalidieren. Die Behauptung, dass ein blosses `git archive <commit> bsc README.md` *unabhaengig* kompiliert, ist fuer den aktuellen Code **falsch**; das konkrete Archiv-Experiment wurde hier nicht ausgefuehrt.

**Fix (minimal, phasentreu):** Den Status-Reporter hinter ein **nicht-default** Cargo-Feature `status-reporter` verschieben: `prost`, `prost-build`, `protox` optional; `build.rs` mit bedingter Proto-Generierung; `lib.rs` mit `#[cfg(feature = "status-reporter")]` fuer `pub mod status`; expliziten `[[bin]]`-Eintrag mit `required-features = ["status-reporter"]` fuer `bsc-status-reporter`. Das existierende Reporter-Verhalten bei explizit aktiviertem Feature muss unveraendert bleiben. Vorhandene Verbraucher/Workflows/Workspace-defaults pruefen und **nur erforderliche** Build-Aufrufe anpassen. Keine Protobuf-Dateien duplizieren, nicht heimlich nach `bsc/` umziehen und kein Gateway-Refactoring. Der bereits gelockte Dependency-Satz sollte nur durch Feature-Flags geaendert werden; Lockfile-/Cargo-Feature-Resolver-Ergebnis ist durch Test zu belegen statt vorauszusetzen.

**Tests:** `cargo check --locked -p darkbot-bsc-qualification` aus einem reproduzierbar gepackten BSC-only-Tree ohne Root-`protocol` erfolgreich; `cargo check --locked -p darkbot-bsc-qualification --features status-reporter` aus dem vollstaendigen Checkout erfolgreich; vorhandene Status-Reporter-Tests unter aktiviertem Feature. G0.8-Tests und G0.7-No-Send isoliert. Keine zufaellige Abhaengigkeit von GUI-Dateien zur Ausfuehrungszeit.

### E2-8 — Reporter-Socket/Label: bestaetigt, aber nicht jetzt implementieren

**Code:** `qualification/src/bin/bsc-status-reporter.rs` verwendet standardmaessig `/tmp/darkbot/bsc.sock`; `status/publisher.rs::serve` erstellt Elternverzeichnisse und setzt den **Socket** auf 0600, prueft aber nicht hinreichend die Vertrauenswuerdigkeit des Elternpfads gegen andere lokale Nutzer. `status/projection.rs` ca. Z. 267 mappt alle Statuswerte ausser `PASS_READ_ONLY` pauschal auf `STOP` und verliert `BLOCKED`/offline-Semantik.

**Entscheidung:** als spaeteres GUI-/Statusprojekt merken, **kein** Patch in der aktuellen Gate-0-kritischen Reparaturrunde. Ein Reporter-Snapshot ist weder Sicherheitsautoritaet noch RUN-Entscheidung. Kein Arbeiten an GUI, Root-Gateway oder neuen Control-Features, solange G0.7/G0.8 nicht abgearbeitet ist.

### E2-9 — Informations- und Scope-Befunde

- **Finalitaet:** G0.8 prueft aktuelle kanonische Block-/Receipt-Konsistenz und Reorg innerhalb des Beobachtungsfensters, aber kein eigenes `finalized >= buy_block`-Gate. Das ist eine dokumentierbare Nachlauf-/Finality-Frage, kein automatisch geltender neuer PASS-Standard. Keine fiktive Instant-Finality.
- **Budget:** `g08_mainnet_verify.rs` berechnet `experiment_total_cost` aus Receipt-Gas und BUY-Wert, vergleicht den Wert aber nicht nochmals explizit mit dem v4-Maximum. Der bestehende signierte Gaspreis, Limits und Guard begrenzen den geplanten Versuch; fehlende G0.8-Zweitpruefung ist ein moeglicher **Defense-in-depth**-Punkt, noch kein Beleg fuer einen Budgetueberlauf. Falls beim ohnehin erforderlichen Kosten-Handoff ergaenzt: nur zusaetzlicher read-only Vergleich mit dem **exakt gebundenen** Budget, keine neuen wirtschaftlichen Parameter.
- **RPC-URL und Secrets:** `wire::call` startet `curl` mit der URL als argv. Risiko bei URLs mit eingebetteten API-Tokens (lokale Prozessliste, Fehlersichtbarkeit), aber kein Nachweis eines aktuell betroffenen Keys. Als spezifische Transport-Haertung vermerken, keine pauschale `curl`-zu-neuer-HTTP-Library-Migration in Gate 0 ohne Bedarf.
- **Q5A:** `qualification/build.rs` bindet fuer die alte Q5A-„frozen sources“-Messung `../execution/src` relativ zu Baseline `92fa520...`. Die bestehende G0.7-Weiterentwicklung macht `Q5A_BUILD_FROZEN_UNCHANGED=false`, weshalb ein neuer Q5A-Lauf als `INCOMPLETE` gelten kann. Keine Versionierungsaenderung ohne tatsaechlichen Q5A-Neuqualifikationsauftrag; alte Evidenz nicht nachtraeglich hochstufen.

### Etappe-2-Fixreihenfolge und Abnahme

1. **E2-4 zuerst fachlich klaeren** mit echtem read-only Mainnet-Receipt unter dem richtigen Portal/Tax-Scope, bevor Event-Mismatch zu STOP umdefiniert wird. Bis dahin kein fiktiver Verifikations-PASS.
2. **E2-2 + E2-5**: begrenzte, diagnostizierbare RPC-Wiederholung, Decoder-/Receipt-Kompatibilitaet, robuste, bytebegrenzte Evidence und Crash-/Reorg-Regressionen. Keine unkontrollierte >120-s-Blockade oder Verlust des vor RPC geschriebenen `UNRESOLVED`-Fences.
3. **E2-1** nur als Safety-feste Ablaufbeobachtung; keine automatische ADAPT-Umwandlung und kein Nonce-Burn. **E2-3** vertragliche STOP/ADAPT/UNRESOLVED-Trennung nach belastbarer Event-Semantik/Archivdaten.
4. **E2-6b** verbindliche Timing-Evidence; **E2-7** Reporter optional vom Qualification-Build trennen. **E2-6a** bleibt ausdruecklich ohne Fix. **E2-8** zurueckgestellt.
5. Tests, Docs und aktuelles Ledger mit echten Ergebnissen aktualisieren. Keine neue Gate-0-Evidenz aus synthetischen Fixtures ableiten. G0.7-/G0.8-Status bleiben `NOT_EXECUTED / NOT_AUTHORIZED`, bis die explizite spaetere wirtschaftliche Freigabe und der echte Versuch erfolgen.

**Gezielte Testkommandos (Implementierer):**

~~~bash
cd bsc/engine
cargo test --locked -p darkbot-bsc-qualification --bin g08_mainnet_verify
cargo test --locked -p darkbot-bsc-qualification --bin g08_producer_timing
cargo test --locked -p darkbot-bsc-qualification --test g08_binary_records
cargo test --locked -p darkbot-bsc-qualification --test g08_reads_g07_evidence
cargo check --locked -p darkbot-bsc-qualification
cargo check --locked -p darkbot-bsc-qualification --features status-reporter
cargo fmt --all -- --check
~~~

Ergaenzend **BSC-only-Archiv-/Buildprobe ohne Root-`protocol/`** und gezielte E2-1..E2-5 Negativ-/Recovery-Fixtures. Vor dem Testen Agent/README-Kontext und HEAD verifizieren; wenn Feature-Namen oder Testtargets zwischenzeitlich geaendert wurden, Kommandos entsprechend dem realen Workspace validieren, nicht blind erfinden. `cargo test`-GREEN der Offline-Fixtures beweist keine echte Mainnet-Acquisition oder ein originales Timing-Witness-PASS.

**Etappe-2-Abnahme:** auf `main` nur nachgepruefte minimal-invasive Fixes, klar unterschiedenes `UNRESOLVED`/STOP/ADAPT, kein Warten ohne endliche Obergrenze, keine falsche Timing-Definition, durable BIN-Evidence, G0.8 ohne GUI-Buildabhaengigkeit. Keine Freigabe, kein Mainnet-BUY, kein automatischer Nonce-Burn und keine Wiederaufnahme von C1/C2.


## 3. Etappe 3 — DRY-/Requal-Werkzeuge, Salt-Exposure und Prozessisolation

**Pruefstand:** 2026-10-20, GitHub `main` Commit `c2de791a0833ff35be48f326c533c348093945de`; Fable-Report Etappe 3 bezieht sich auf `cac786e`. Fuer diese Codepfade lag bis zu dieser Revision keine neuere Implementierung vor. **Hier wurden die Codepfade, die vorhandenen gepinnten Evidenzen und die verbindlichen Plaene gelesen, aber weder aktuelle Mainnet-RPC-Tests noch Rust-/Shell-Tests ausgefuehrt.** Alle Fixes sind weiter `OPEN`. Die bisherige signed-byte-Identitaet, die historischen R0-/DRY-Records, Guard `UNARMED` und der eine wirtschaftliche Attempt duerfen nicht unbemerkt geaendert werden.

**Phasen-/Vertragsbezug:** `BSC_DUAL_VENUE_ROADMAP.md` (Binding Update 2026-10-20), `G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md` §§1,2,5, `G07_REQUALIFICATION_RESULT_20261020.md`, `BSC_BIRTH_ENTRY_ARCHITECTURE.md` und `GATE0_TESTPLAN_SHORT.md`. Die historische Requal hat R0 + DRY0–DRY4 PASS mit dem eingefrorenen Salt/Token/Launch-/BUY-Hash dokumentiert; daraus folgt **weder** Geheimhaltung des Salts noch live BUY/ordering/timing PASS. Keine Wiederholung des langen R0/DRY allein zur Umsetzung kosmetischer Härtung.

### Etappe-3-Urteil

| ID | Schwere | Urteil | Umsetzungsstatus / Sicherheitsgrenze |
| --- | --- | --- | --- |
| E3-1 | Hoch, Angriffs-/Kandidatenrisiko | **BESTAETIGT:** Salt deterministisch, Repository-veroeffentlicht und RPC-seitig offengelegt; **tatsaechlicher fremder erfolgreichen Pre-Launch NICHT NACHGEWIESEN** | Anwenderentscheidung A/B erforderlich; kein stiller Re-Salt |
| E3-2 | Hoch fuer Geheimnisisolation | **BESTAETIGT:** Keys durch `set -a`/globales `export` zu breit vererbt | Pflichtfix ohne Kandidaten-/DRY-Aenderung |
| E3-3 | Niedrig bis Mittel | **BESTAETIGT:** DRY-3 prueft gegen veraltete v2-Maximum-Zahl | Pflichtfix fuer kuenftige erneute DRY-Laeufe; alte Records unveraendert |
| E3-4 | Niedrig, Assurance | **BESTAETIGT:** `ZERO_EGRESS_STRUCTURAL=true` ist eine manuell behauptete Eigenschaft | Enger Quell-/Offline-Negativtest, keine neue Network-Faehigkeit |
| E3-5 | Niedrig, Reproduzierbarkeit | **BESTAETIGT:** Readiness-Wrapper startet Cargo vom Repo-Root ohne den Workspace-Pin | Pflichtfix, gezielte Shell-/Toolchain-Probe |
| E3-6 | Niedrig, irrefuehrender Fehlzustand | **BESTAETIGT:** zwei Legacy-Probes erwarten V2-Guard/JSON-Toolchain | Explizit ausser Betrieb nehmen; nicht in Gate-0-Hotfix portieren |
| E3-7 | Info / moegliche Konsistenzhaertung | **BESTAETIGTE ARCHITEKTUR-EIGENSCHAFT:** RPC-State ohne Proof, `latest`-Reads nicht auf festen Block gepinnt; **konkreter falscher PASS nicht nachgewiesen** | Transparent dokumentieren, minimalen Snapshot-Fix nur bei nachgewiesenem G0.7-Sicherheitsbedarf |

### E3-1 — Eingefrorener Launch-Salt ist vorhersagbar und oeffentlich

**Harter Code-/Evidence-Nachweis:**

- `bsc/tools/g06a_economics_evm_crosscheck.cjs::chooseSalt` ca. Z. 145–157 berechnet `keccak256(UTF8("darkbot-g06a-economics-v1:" + i))` fuer `i=0,1,...`, prueft den Vanity-Suffix `7777` und sucht die erste beim gesichteten Block noch nicht belegte CREATE2-Adresse. Es wird **keine** CSPRNG-Entropie fuer diesen Salt benutzt.
- `bsc/tools/g06a_final_dry.sh` ca. Z. 143–153 uebernimmt `salt` aus `economics_crosscheck.dbrec`, verlangt Identitaet zum bereits eingefrorenen DRY und verwendet ihn spaeter fuer Signatur und Simulation.
- Die versionierte Datei `bsc/native/evidence/g06a/g06a_final_dry_20261015T222241Z/economics_crosscheck.json` enthaelt nachweislich ein nichtleeres `salt`-Feld; auch `bsc/native/evidence/g06a/g07_binary_import_20261016/economics_crosscheck.dbrec` ist versioniert. Die Token-Adresse steht zusaetzlich im Requal-Plan. Der Salt ist also nicht vertraulich. Den geheimen Wert nicht erneut in Fehlermeldungen oder diesen Bugtext kopieren.
- **Zusaetzlich verifiziert, bei Fable nicht ausreichend beschrieben:** `g06a_economics_evm_crosscheck.cjs` ca. Z. 267–303 sendet `launchCalldata(LAUNCHER, salt)` bereits bei `eth_estimateGas` und `eth_call` an den konfigurierten externen RPC; ca. Z. 450 wird `SALT=...` auf stdout geschrieben. Auch `bsc/engine/crates/execution/src/bin/g07_mainnet_self_launch_buy/main.rs` ca. Z. 192–203 verwendet nach der Token-Code-Pruefung ein `rpc_estimate_gas(..., &launch_data)`, welches den aktuellen Launch-Salt an einen RPC weitergibt. **Read-only RPC verhindert wirtschaftliches Senden, bedeutet aber nicht Geheimhaltung von Call-Daten.**
- Offizielle Flap-Referenz: `https://github.com/flap-sh/flap-skills/blob/main/launch-bnb-token-on-flap/references/salt-finding.md`. Der Token wird nach dem Protokoll ueber CREATE2 aus Portal (Deployer), Salt und Clone-Initcode mit TAX_V3-Implementation berechnet; der offizielle Salt-Finder nutzt einen frischen Zufalls-Seed. **Die Live-Autorisierungs-/Collision-Semantik der genauen Portal-v5.25.0-Implementation ist hier nicht durch einen fremden Test-Launch belegt.** Kein Mainnet-Launch darf zur Pruefung dieser Behauptung ausgefuehrt werden.

**Risiko:** Ein Dritter kann Kandidat und Token-Adresse offline im Voraus kennen. Falls die aktuelle `newTokenV6`-Deployment-Semantik einen anderen Caller nicht an der Nutzung der bekannten CREATE2-Adresse hindert, kann ein fremder Vorab-Launch die gewaehlte Adresse belegen; G0.7 STOP vor dem Latch oder ein Race/Revert beim Release ist moeglich. Der tatsaechliche fruehere Fremd-Launch bzw. eine bereits belegte Adresse sind **nicht belegt**. Der vorhandene G0.7-Test `rpc_code(token)` mit `predicted token already exists` ca. Z. 185–191 findet eine **bereits belegte** Token-Adresse, garantiert aber **nicht** die Abwesenheit einer spaeteren Race-/Builder-Propagation. Fables „A: vor Latch Token frei pruefen“ ist insofern **schon weitgehend implementiert**; erfordert allenfalls nachgewiesene Verkuerzung/Aktualitaetspruefung, keinen neuen Preflight-Mechanismus.

**Keine automatische Codeentscheidung:**

- **A — eingefrorenen Salt behalten:** Exaktes Launch-/BUY-Hashpaar, Vertrags-/Requal-Bindung und wirtschaftliche Obergrenzen bleiben unveraendert. Bei spaeterer wirtschaftlicher Freigabe muss das dokumentierte Salt-/Adress-Collision-/Front-run-Risiko explizit akzeptiert werden. Vor dem Latch und moeglichst nahe am autorisierten Release weiterhin Code-/Head-/Nonce-/Chain-Gates ausfuehren. **Auch ein frisch leerer Account ist keine Garantie** gegen Fremdausfuehrung nach dem Check. Kein neuer unterschwelliger Salt-Wechsel.
- **B — neuer kryptographisch zufaelliger Salt (mit Vanity-Suche):** nur nach neuer ausdruecklicher Entscheidung; neue Token-Adresse, neue Launch-Calldata, neues signiertes Launch-/BUY-Hashpaar, notwendige DRY-/EVM-/Economics-/Guard-/Contract-V4-(oder Folgevertrags-)Bindung und neue Approved-Variant-SHA-256. Die bisherigen PASS-Evidenzen bleiben historische Belege, autorisieren aber **nicht** den neuen Kandidaten. **Besonderer Pflichtpunkt:** blosse Ersetzung von `chooseSalt` beseitigt nicht das Leakage: alle externen `eth_estimateGas`-/`eth_call`-Pfade mit echten sensitive Launch-Daten und die G0.7-Laufzeitpruefung muessen fuer eine *Geheimhaltungs-Behauptung* ebenfalls entfernt/auf unverfaengliche Kontrollfaelle bzw. lokale EVM-Qualifikation umgestellt werden. Jeden Verifikationsverlust explizit bewerten; keine G0.7-Safety-Checks ersatzlos loeschen.
- Kein `Option B` im laufenden Bugfix stillschweigend umsetzen. Zu A/B **keinen** neuen R0-Scan oder Echtgeldversuch veranlassen. Zuerst eine read-only ABI-/Contract-Analyse, ob andere Caller denselben Salt/Deployment-Adressbereich kollidieren lassen koennen. Danach Benutzerentscheidung mit konkretem Risiko.

**Dokumentation:** Die Aussage `The DRY salt was not disclosed` in `G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md` §1 bezieht sich auf die historischen Read-only-Probes vom 2026-10-17 mit Random-Salt, ist aber **keine zulaessige Aussage ueber den spaeteren eingefrorenen DRY-Salt**. Kontextpraezise korrigieren/annotieren; historische Fakten nicht loeschen. Die alte Salt-Vertraulichkeit nicht mehr als Schutzmaessnahme ausweisen.

**Abnahme:** Eine klare A/B-Risikoentscheidung, kein unbeabsichtigter Aenderungspfad und keine Behauptung, die bestehende G0.7-Token-Code-Pruefung sichere ein Race vollstaendig ab. `UNARMED` bleibt erhalten.

### E3-2 — Private Schluessel aus `secrets.env` werden an unberechtigte Folgeprozesse weitergegeben

**Code:** `bsc/tools/g06a_final_dry.sh` ca. Z. 59–75: `set -a; source "$SECRETS"; set +a` und danach `export DARKBOT_G06A_LAUNCHER_PRIVATE_KEY`, `export DARKBOT_G06A_BUYER_PRIVATE_KEY`. Jeder spaetere Kindprozess erbt den gesamten exportierten Variablensatz, auch `node g06a_economics_evm_crosscheck.cjs`, `g06a_builder_policy_record` (offizielle Netzabrufe), `g06a_preflight_record`, `dbrec_show`, `shasum` usw. Schluessel brauchen nur `g06a_local_simulate.cjs` und `g06a_self_launch_bundle`. Bereits als `export` in `secrets.env` markierte oder aus der aeusseren Umgebung geerbte Variablen bleiben auch **ohne `set -a`** exportiert. Fables Patch 1 entfernt das grobe `set -a`, garantiert aber **allein** keine vollstaendige Prozessisolation.

**Pflichtfix vor dem naechsten DRY mit echten Schluesseln:**

1. Konfigurationswerte aus `secrets.env` **nicht per `set -a` massenhaft exportieren**, fuer Secret-Datei und Shell-Variablen Berechtigungen/Ignore-Regeln erhalten; kein `set -x`, kein Secret in Stdout/Stderr/Journal. Falls Datei selbst `export` enthaelt, diese Attribute nach dem Laden gezielt entfernen oder Quelle fail-closed ablehnen; zuvor geerbte Export-Flags ebenfalls beruecksichtigen.
2. **Jede Kindprozess-Grenze** mit klar erlaubter Umgebung ausfuehren, vorzugsweise `env -i` + minimaler Allowlist (`PATH`, ggf. `HOME`/Toolchain-/Node-Laufzeitvariablen soweit tatsaechlich notwendig, RPC-URL und jeweils erforderliche nichtgeheime DRY-Parameter). Insbesondere fuer alle RPC-/HTTPS-/Dokumentations-Schritte **keine** `BSC_*_PRIVATE_KEY`- oder `DARKBOT_G06A_*_PRIVATE_KEY`-Variablen; auch sonstige Tokens aus `secrets.env` nicht blind vererben. Die zwei erlaubten Signier-/lokalen EVM-Schritte erhalten beide Keys nur **fuer ihren direkten Prozess**.
3. Den letzten Schritt, `g06a_self_launch_bundle`, und die lokale `g06a_local_simulate.cjs` mit identischen Parametern aufrufen; Binary-/ABI-/RLP-Konstruktion und Signatur nicht aendern. Sollten die Tools transitive Unterprozesse starten, deren Umgebung gesondert pruefen; eine Shell-Funktion `with_keys` allein isoliert keine Kindkette.
4. Eine **gefaelschte Offline-Key-Fixture** bzw. instrumentierte Test-Commands nutzen, die ausschliesslich Prasenz/Abwesenheit von Variablennamen und einen SHA-256-Testdigest dokumentieren, **nicht** Secret-Werte oder `ps -E`-Ausgaben. Explizit negative Faelle: `secrets.env` mit `export`, bereits beim Scriptstart vererbter Key, unerwartete Environment-Variablen, abgebrochener Signierschritt.

**Abnahme:** Kein nicht berechtigter Kindprozess sieht Key-Variablen; beide berechtigten Schritte koennen sie weiterhin verwenden; kein Netzprozess erhaelt Keys; Signatur-/Transaktions-Bytes bleiben fuer denselben eingefrorenen Input unveraendert. **Nicht** den kompletten historischen R0/DRY neu starten, nur um Umgebungs-Scopes zu pruefen.

### E3-3 — DRY-3 verwendet weiterhin die historische v2-Budgetgrenze

**Code:** `bsc/engine/crates/qualification/src/bin/g06a_self_launch_bundle.rs` ca. Z. 571–578 und Z. 649–655 akzeptiert `reserved` und `simulated_cost` jeweils bis **10603270000000000 wei** (0.01060327 BNB). Der jetzt bindende v3/v4-Maximumwert ist **481163500000000 wei** (0.0004811635 BNB). Andere Komponenten (`g06a_requal_check`, live Guard, Readiness) pruefen das strengere Limit; **kein bestehender Budgetdurchbruch wurde durch die alte DRY-3-Zeile nachgewiesen**, aber der G06A-Teilnachweis wird in diesem Punkt zu optimistisch.

**Minimaler Fix:** Beide Grenzpruefungen gegen die exakt gebundene Kontrakt-Obergrenze **481163500000000 wei** aendern, bevorzugt ueber eine geprüfte gemeinsame numerische Konstante statt einer neu erfundenen Quelle. `budget.v2_proposal_per_attempt_ceiling_wei` im historisch kompatiblen Record **nicht** stillschweigend umbenennen oder umschreiben (Fables Vorschlag ist hier passend); unveraenderte DRY-Records/Hashes muessen historische Wahrheit behalten. Uebernehmen der **exakten** Signaturparameter unveraendert sicherstellen. Bei Abweichung STOP, niemals Cap automatisch erhoehen.

**Tests:** beide Vergleichsstellen bei `MAX`, `MAX+1`, Ueberlauf und dem aktuellen signierten Versuch; historische v2-Feldbytes unveraendert; kein verdeckter v2-Fallback als aktuelle Scope-Autoritaet.

### E3-4 — Zero-Egress-Flag ist eine statische Behauptung, keine Laufzeit-Observation

**Code:** `g06a_self_launch_bundle.rs` ca. Z. 682–688: `let dry4_zero_egress = true;` und `dry::ZERO_EGRESS_STRUCTURAL` wird aus dieser Konstanten geschrieben. Aktuelle Quellpruefung zeigt fuer **dieses** DRY-Binary keine intendierte Transport-Send-Funktion; dennoch kann der boolesche Literalwert allein keine lueckenlose Negativaussage ueber jede kuenftige Abhaengigkeit oder den ganzen DRY-Prozess beweisen.

**Fix:** Den Flag explizit **`STRUCTURAL_CODE_SCOPE_ONLY`** bzw. im bestehenden Typfeld mit klarer Record-Provenienz-/Qualification-Note versehen; den booleschen Inhalt des alten Felds nicht umwidmen. Engen Quelltest fuer direkten Netzwerk-/Child-Process-/Signed-RLP-Persistenzpfad dieses Binaries einfuehren, und wenn moeglich einen Offline-Durchlauf mit gesperrtem Netzzugriff/Fixture-Keys. **Fables reine Suche nach sieben verbotenen Strings ist ein guenstiger Regressionstest, aber keine formale Callgraph- oder transitive Netzwerkfreiheit.** Lokale DRY-2-EthereumJS-Simulation und Builder-Policy-Fetch nutzen weiterhin **Read-only RPC/HTTPS**, also kein globales „Netzwerk unmoeglich“ behaupten.

**Abnahme:** Qualifizierter Scope des Flags ist dokumentiert und bei neuen Netzwerk-/Process-/Persistenzaufrufen im betroffenen Binary wird der Test rot. Keine Schlussfolgerung `G0.7 NEVER SEND` allein aus diesem Bool.

### E3-5 — Readiness-Wrapper ignoriert Toolchain-Auswahl des Cargo-Workspace

**Codebeweis:** `bsc/tools/g07_pre_live_readiness.sh` macht `cd "$ROOT"` und dort `cargo run --locked --offline --manifest-path bsc/engine/Cargo.toml ...`. Im Git-Tree gibt es `bsc/engine/rust-toolchain.toml` mit `channel="1.93.1"`, aber **keinen** Root-`rust-toolchain.toml`. Rustup waehlt das Toolchain-Override grundsaetzlich nach dem laufenden Arbeitsverzeichnis; der Manifestpfad allein pinnt es nicht. Kann bei abweichender Default-Version Kompilierung/Ergebnis invalidieren.

**Fix:** Im Teilprozess `(cd "$ROOT/bsc/engine" && cargo build --locked --offline ... --bin g07_pre_live_readiness)` bauen, dann aus dem **Repo-Root** das gebaute `bsc/engine/target/debug/g07_pre_live_readiness` ausfuehren, damit die bestehenden relativen `bsc/config`-/Evidenzpfade unveraendert funktionieren. Alternativ ausdruecklich gepinnte Toolchain `cargo +1.93.1` ausfuehren, wenn Umgebung dies verlaesslich unterstuetzt. Exitcode, Args, offline-Zwang, stdout und kein Send unveraendert.

**Tests:** `bash -n`, `rustup show active-toolchain` innerhalb Workspace, dokumentierte `rustc --version`, Wrapper von fremdem Arbeitsverzeichnis, invalides Argument, Guard UNARMED, keine Netz-/Send-Aktivierung.

### E3-6 — Zwei alte Minimum-Probes sind inkompatibel und liefern eine falsche Guard-Diagnose

**Code:** `bsc/tools/g07_find_min_executable_buy.sh` (ca. Z. 32) und `bsc/tools/g07_min_spend_probe.sh` (ca. Z. 28) suchen nach `FROZEN_APPROVED_V2_SHA256 = "UNARMED"`. In `bsc/engine/crates/execution/src/acceptance_guard.rs` existiert fuer den aktiven Kandidaten **nur V4**. `g07_min_spend_probe.sh` nennt diese fehlende Zeichenfolge faelschlich `live guard is armed`. Beide Skripte rechnen mit veralteten JSON-Preflights/Outputs bzw. frueheren Argumentlisten, waehrend aktuelle Rust-Qualifikationsprogramme DBREC und andere Pfade erwarten.

**Fix:** **Nicht** beide alten Minimum-Searches fuer die eingefrorene G0.7-Phase zu einer zweiten qualifizierten Pipeline portieren: das waere Scope-Drift und koennte andere GAS/BUY-Kandidaten ohne passende Zustimmung hervorbringen. Stattdessen den beiden Einstiegspunkten einen sofortigen, klaren und sicheren Fehlerhinweis `SUPERSEDED_INCOMPATIBLE_WITH_V4_DBREC; NO_LIVE_AUTHORITY` geben, ohne vorher Keys oder RPC zu lesen; Verweis auf den bindenden frozen G0.7-Kandidaten und die aktive DRY-Requal-Toolchain. Optional nach Dokumentation/Archivierung in `prestudy` verschieben, aber keine neuen produktiven Entrypoints oder Branches erstellen. **Keinen** angeblich „armed“-Guard-Fehler behaupten.

**Tests:** beide Entrypoints ohne Secrets und ohne Netz -> sofort erklaerbarer nichtnull Exit, keine Credentials geladen, keine signierten TXs, kein JSON; `bash -n`. Benutzer soll die in `README` genannte historische Toolzaehlung nicht mit aktueller Run-Qualifikation gleichsetzen.

### E3-7 — State-/Block-Provenienz im read-only Qualifikationspfad

**Code:** `g06a_economics_evm_crosscheck.cjs` und `g06a_local_simulate.cjs` verwenden denselben konfigurierten Archiv-RPC und lesen historische Balances/Nonce/Code/Storage ueber RPC (`eth_getStorageAt`, Account-Loader ohne `eth_getProof`), waehrend die lokale Quote einen **unabhaengigen Rechenalgorithmus**, aber keine zweite unabhaengige RPC-State-Quelle darstellt. Einige Plausibilitaets-/Headerchecks binden Parentblock/Hash. `execution/src/bin/g07_mainnet_self_launch_buy/chain_reads.rs::run_current_preflight` beginnt mit einem Head, verwendet bei vielen spaeteren Read-only-Aufrufen jedoch `"latest"`. Ein load-balanced RPC kann dabei unterschiedliche Head-States liefern.

**Bewertung:** Kein konkreter Mix-State-Falsch-PASS oder RPC-Defekt in den vorhandenen Evidenzen nachgewiesen. Trotzdem `same-RPC-source` nicht als unabhaengige *State-Quelle* oder Merkle-verifizierte State-Trie-Aussage bezeichnen. Fuer die G0.7-Vorbereitungsqualitaet ist eine **gezielte Head-Konsistenzpruefung** im Preflight gerechtfertigt: kanonische Read-Only-State-Felder soweit moeglich auf denselben Blocktag/Hash beziehen oder den Head/Hash nach Abschluss validieren; **pending** Nonces separat frisch vor Release halten, nicht mit finalen Block-Nonces verwechseln. Jede damit verbundene Aenderung der bereits qualifizierten Baseline-/DRY-Semantik muss gegen den bestehenden Record-Vergleich reviewt und nur mit anlassbezogenen Tests belegt werden. Keine neue RPC-Providerpflicht, `eth_getProof`-Redesign oder pauschaler R0-Rescan als Reparatur der Beobachtbarkeit.

**Tests bei einem beauftragten engen Fix:** simuliert wechselnder `latest`-Head, Block-Hash-Reorg, inkonsistenter RPC-Backend-State, ungeaenderter eingefrorener Parent-Snapshot; keine Hoherstufung zu PASS bei unvereinbaren Heads. Ohne Fix ist der Safety-Scope als **INFO/OPEN** auszuweisen, nicht falsch als schon kryptographisch nachgewiesen.

### Etappe-3-Zielzustand und Agent-Regressionen

1. E3-2 (Key-Isolation) vor allen weiteren DRY-Schritten mit echten Schluesseln priorisieren. Schon vorhandene Historien-Artefakte nicht mit anderen Secret-Umgebungen ueberschreiben; spaetere Tests mit Fake-Keys oder ueber reine Funktions-Mocks. Kein `ps -E` mit echten Keys.
2. E3-5 Wrapper und E3-6 irrefuehrende Alt-Tools enger Scope, E3-3 DRY-3-Maximum, E3-4 direktes No-Egress-Assurance-Testing. Gezielt daran gebundene Tests; keine Voll-Requal ohne Invalidation.
3. E3-1 Salt-Risiko offenlegen, On-chain-Autorisierungs-/Collision-Regeln **read-only** pruefen, und A/B zur ausdruecklichen Userentscheidung vor spaeterer wirtschaftlicher Freigabe vorbereiten. A ist derzeit kein Beweis gegen Race, B invalidiert die bisherige Signed-Byte-Freeze. Keine Option automatisch waehlen.
4. E3-7 als Quelle/Head-Konsistenzlimit dokumentieren. Keine versteckte Neudefinition von Gate 0 oder Aenderung der Mainnet-/Release-Regeln.
5. Crosscheck-Binary-Paritaet und zero-egress-Quell-Grenzen in kurzer Test-/Evidence-Matrix erweisen. Abschlussmeldung angeben: geaenderte Datei/Zeile, neue Testnamen, die real ausgefuehrten PASS/FAIL-Ergebnisse, geladene Umgebungs-**Namen** ohne Inhalte, unveraenderte TX-Hashes und **UNARMED**.

**Gezielte Kommandos fuer die Agent-Implementierung (nur nach Fix, nicht hier ausgefuehrt):**

~~~bash
# Von repo root
bash -n bsc/tools/g06a_final_dry.sh
bash -n bsc/tools/g07_pre_live_readiness.sh
bash -n bsc/tools/g07_find_min_executable_buy.sh
bash -n bsc/tools/g07_min_spend_probe.sh
# Vorerst ausschliesslich Mock-/Offline-Pruefung der isolierten Secret-Scopes.
(cd bsc/engine && cargo test --locked -p darkbot-bsc-qualification --bin g06a_self_launch_bundle)
(cd bsc/engine && cargo test --locked -p darkbot-bsc-qualification --test g06a_dry_no_network)
(cd bsc/engine && cargo fmt --all -- --check)
~~~

Der neue Test `g06a_dry_no_network` existiert **noch nicht** und ist vom Implementierer nur fuer den konkret geaenderten Code anzulegen; der `cargo test --test`-Aufruf ist deshalb **kein** Beleg, dass er zum jetzigen Stand laeuft. Kein `g06a_final_dry.sh` ohne ausdrueckliche Anforderung starten, keinen Salt auf `main` austauschen, keine private Key-Datei committen und keinen G0.7-LIVE-Versuch ausloesen.

**Etappe-3-Abnahme:** Alle bestaetigten lokalen Korrekturauftraege geprueft implementiert, die Saltrisiko-Entscheidung sichtbar getrennt, geklaerte Sicherheitsgrenzen und reproduzierbare eng begrenzte Offline-Tests. G0.7 bleibt **CODE-READY-ZIEL / LIVE NOT AUTHORIZED**, G0.8 **NOT EXECUTED**, Phase C1/C2 weiter **PAUSED**.


## 4. Etappe 4a — Flap-Economics, Signer, RLP, Launch-Admission und G0.7-Key-Hygiene

**Review:** 2026-10-20 · **Fable-Basis:** `cac786e` · **unabhaengig gepruefter `main`-HEAD:** `b11edda156423ce31a18221e1d0ffdabe4a11413`. Gegenueber `cac786e` ist in den hier geprueften BSC-Quellpfaden keine spaetere Implementierung festgestellt worden. Die auf `main` erschienenen neueren Changesets betreffen u.a. Project_0-Dokumentation; nicht auf einen veralteten Gesamt-HEAD zuruecksetzen. **Keine Rust-/Netzwerk-/Mainnet-Tests in diesem Review ausgefuehrt.**

**Relevante Vertraege:** `BSC_DUAL_VENUE_ROADMAP.md` (Binding 2026-10-20: G0.7/G0.8 vor C1/C2, keine breite Optimierung), `BSC_PRODUCTION_BUILD_MASTER_PLAN.md` (ein Trading-/Signing-Owner in der spaeteren Runtime), `BSC_BIRTH_ENTRY_ARCHITECTURE.md` (realer erfolgreicher Kauf und sub-ms-Produzentenmessung), `GATE0_TESTPLAN_SHORT.md` sowie unveraenderter G0.7-v4-/Signed-Byte-/Unarmed-Guard-Vertrag. Ein produktiver Fremd-Launch ist **noch nicht qualifiziert**; diese Etappe macht keine neuen wirtschaftlichen Parameter frei.

### Etappe-4a-Urteil

| ID | Kategorie | Unabhaengiges Urteil | Auftrag / Scope |
| --- | --- | --- | --- |
| E4-1 | Produktqualifikation | **BESTAETIGT, KEIN IMPLEMENTIERUNGSBUG:** `classify_launch` und `admit_native_buy` sind bewusst auf den witnessed G0.6a-Crosscheck eingeschraenkt | Keine jetzige Profil-/Betragsaufweitung; **spatere C2-/Produktions-Qualifikation** |
| E4-2 | Evidenz/Salt-Exposure | **BESTAETIGT, BEREITS E3-1:** `WITNESSED_SALT` und `WITNESSED_TOKEN` stehen wortwoertlich in versioniertem Testquelltext | E3-1 verstaerken, kein zweites Salt-Ticket |
| E4-3 | Codeparitaet / qualifizierte Grenze | **BESTAETIGT, TESTLUECKE:** parallele RLP-/Signing-Implementierungen von Trading, G0.7 und DRY; kein expliziter bytegenauer G0.7-vs-Production-Quertest gefunden | Kleiner Offline-Regressionsbeweis, **kein Signer-Umbau** |
| E4-4 | Credential-Hygiene | **BESTAETIGT, DEFENSE-IN-DEPTH:** `String` + `Vec<u8>` aus G0.7-`key_from_env` nicht bewusst ge-zerot, Key-ENV bleibt vorhanden | Enger Sicherheitsfix nur mit Source-/Plan-Hash-Review, ohne Signed-Byte- oder Egress-Aenderung |
| E4-AUDIT | Keine neuen Fehler | In den betrachteten Kurven-/Fee-/Slippage-/RLP-/ABI-Teilen kein konkreter, quellcodegestuetzter Gegenbeweis zur bestehenden Testlogik gefunden | **Nicht** als „mathematisch formal bewiesen“ oder neu getestet ausgeben |

### E4-1 — Klassifizierungs-/Buy-Admission-Profil laesst gegenwaertig keine fremden Standard-Launches durch

**Fundstellen:** `bsc/engine/crates/trading/src/venues/flap/profile.rs` ca. Z. 1–41 und `classify_launch` Z. 220–303; `bsc/engine/crates/trading/src/economics/flap/native_curve20.rs` ca. Z. 174–275.

**Gegenpruefung:**
- Die qualifizierte native Launch-Klasse verlangt ausser allen festen relevanten Fee-/Tax-/DEX-/Quote-/Distribution-/Token-Version-Feldern auch exakt `name = "DarkBot G06A Economics"`, `symbol = "DBG06E"`, die Economics-Test-URI und den Test-`beneficiary = 0x...a11c`. Nur der `salt` ist in dieser Klasse variabel; dieser beeinflusst die abgeleitete Tokenadresse. Ein anderer Launch faellt u.a. an `UnsupportedLaunch::{BuyTax,Distribution,Beneficiary,Metadata,...}` durch.
- `admit_native_buy` verlangt **exakt `QUALIFIED_MAINNET_INPUT_WEI = 1000000000000000 wei` (= 0.001 BNB)**. Der eingefrorene G0.7-Versuch ist dagegen `1000000000000 wei` (= 0.000001 BNB), also **Faktor 1000 kleiner**. Daraus entsteht **kein** Widerspruch im aktuellen G0.7-Code: `g07_mainnet_self_launch_buy` benutzt seine eigene fuer Gate 0 eingefrorene Launch-/BUY-/Signaturstrecke, **nicht** `classify_launch`/`admit_native_buy`.
- Auch der vom G0.7-Harness erzeugte Launch hat andere Metadata (z.B. `DarkBot G06A Self Launch`) und den tatsaechlichen Launcher als Beneficiary. Die aktuelle Produktions-Admission wuerde ihn nicht als witnessed Economics-Profil klassifizieren. Dieses Verhalten ist **absichtlich fail-closed** und entspricht der aktuellen Gating-/Phasengrenze, kein Versehens-Reject im bereits freigegebenen Produktionsbot.

**Verbindliche Entscheidung:** **Kein Patch in Etappe 4a**. Nicht `Metadata`/`Beneficiary`-Pruefungen entfernen, keine Buy-Amount-Schranke erweitern und keinen fremden Real-Launch als heute „unterstuetzt“ deklarieren. Nach dem Gate-0-G0.7→G0.8-Verdikt und erst im autorisierten Produktions-/C2-Abschnitt sind mindestens erforderlich: (a) Beweis aus gepinntem aktuellem Portal-Implementation-Code, ob Metadata/Beneficiary tatsaechlich kaufirrelevant sind oder eigene Profilklassen benoetigen; (b) unabhängige EVM-Validierung zusaetzlicher Steuer-/Creator-Purchase-/Quote-/Distribution-Profile mit negativen Boundaries; (c) qualifizierte Input-Ranges und Risiko-/Slippage-/Gas-/Profit-Grenzen samt exact-state Evidence; (d) ausreichende Replay/PAPER- und End-to-End-Aufnahmen. Nicht aus der Existenz dieser Funktionen auf bereits LIVE-funktionierende Fremd-Launch-Admission schliessen.

**Abnahme dieser Etappe:** Testbares, dokumentiertes `UNQUALIFIED_LAUNCH`-/`UnqualifiedAmount`-Fail-closed-Verhalten bleibt erhalten; kein G0.7-Byte- oder Budget-Eingriff.

### E4-2 — Der eingefrorene Salt steht auch im Trading-Testkit

**Fundstelle:** `trading/src/venues/flap/profile.rs`, `#[cfg(test)] pub(crate) mod testkit`, ca. Z. 305–319: Konstanten `WITNESSED_SALT` und `WITNESSED_TOKEN`. Der Token entspricht der bereits im `G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md` beschriebenen DRY-Adresse; der Salt stimmt mit dem bekannten G0.6a-Crosscheck-Kandidaten ueberein. **`#[cfg(test)]` schliesst die Konstanten nur aus dem normalen Produktions-Build aus, versteckt sie aber nicht im oeffentlichen Git-Quelltext.** Die reproduzierbare feste Kandidatengenerierung und die versionierten Records standen bereits unter E3-1.

**Fix:** Kein neues Issue und kein kosmetisches Entfernen des Testkit-Salts mit der Behauptung, der Salt sei wieder geheim: er ist aus Seed, RPC-Call-Daten, historischen Artefakten und Git-Historie weiterhin rekonstruierbar. Als zusaetzlichen Beleg zu **E3-1** dokumentieren. A/B-Entscheidung bleibt offen, vor der spaeteren gesonderten wirtschaftlichen Freigabe. Keine Aenderung am v4-Kandidaten ohne neue DRY-/Hash-/Freigabekette.

### E4-3 — Fehlender expliziter G0.7-vs-Production Byte-Paritaetsnachweis

**Fundstellen:** `trading/src/signer/mod.rs::Signer::sign_legacy` (ca. Z. 108–167), `trading/src/transactions/legacy.rs::LegacyTx` (ab Z. 56), `tx_encoding/src/rlp.rs`, `execution/src/bin/g07_mainnet_self_launch_buy/transactions.rs::sign_legacy` (ca. Z. 66–119), `qualification/src/bin/g06a_self_launch_bundle.rs::sign_legacy` (ca. Z. 180 ff.), `g06a_local_simulate.cjs` (ethers-Pfad). `trading::signer` und `trading::transactions::legacy` sind **tatsaechlich oeffentlich exportiert** (`trading/src/lib.rs` und `transactions/mod.rs`); die von Fable verwendeten Modulpfade sind richtig. `execution/Cargo.toml` haengt `darkbot-bsc-trading` bereits ab, daher ist kein neuer Produktiv-Signer zu implementieren.

**Bewertung:** Die getrennten Signier-/RLP-Pfade stammen aus einem bewusst isolierten und eingefrorenen Gate-0-Harness. Das derzeitige Produktionsziel „ein Signer-Owner“ ist ein **Integrationsziel fuer die spaetere Runtime**, nicht die Erlaubnis, jetzt den eingefrorenen G0.7-Signierer auszutauschen. Es gibt bereits **Trading-Golden-Tests** (u.a. EIP-155 und unabhaengig berechnete Signaturen) und G0.7/DRY-Byte-Bindung, aber nicht denselben klaren Test beider Signierer **mit identischen Testinputs in einem Lauf**. Das ist eine Regressionsevidenzluecke, **kein nachgewiesener Signatur-/Encoding-Fehler**.

**Minimaler Fix:** Einen **reinen Offline-Test** im bestehenden G0.7-Binary-`mod tests` oder einem passend gelinkten Testmodul ergaenzen, der mit ausschliesslich **synthetischem** `[0x22;32]`-Key `G0.7::sign_legacy` und `trading::signer::Signer::sign_legacy` auf **identischen** `LegacyTx`-Feldern aufruft und **raw RLP bytes und Keccak-TX-Hash** exakt vergleicht. Mindestens:
1. BUY-`swapExactInput` kurzer/realistischer Payload, kurzer Integer-/Null-Wert-Fall;
2. kompletter `newTokenV6`-Launch mit langem Calldata-Payload inkl. RLP-Long-Form und groesserem Gaslimit, gegebenenfalls gesonderte Feste-Nonce-/Gas-/Buy-Value-Fixtures;
3. beide unterschiedlichen wirtschaftlichen Rollen/Signer, feste EIP-155-Chain `56`, Recovered-Sender/Low-s und reproduzierbare exakte Bytes, unter den wirklich identischen Feldern;
4. eine **separate** Assertion gegen bereits vorhandene, unabhaengig gepinnte DRY-/Golden-`launch_tx_hash` und `buy_tx_hash`-Referenzen, **nur wenn** ein geeigneter Test mit diesen Hashes ohne reale Wallet-Secrets machbar ist; niemals private Schluessel aus `secrets.env` ziehen, keine simulierten Secrets als Beweis des realen eingefrorenen Hashpaares ausgeben.

Fables Patch 1 (Test in `main.rs::tests`) ist vom Typ-/Importaufbau her plausibel: `Signer`, `LegacyTx`, `LegacyTx { chain_id, nonce, gas_price, gas_limit, to, value, input }`, `Signer::sign_legacy(&tx,&mut scratch,&mut raw) -> Result<B256,_>` sind tatsaechlich vorhanden. **In der Rohfassung sind Launch und BUY aber beide mit demselben `value=1e12` und `gas_limit=3000000` kodiert**. Das testet nur generische Gleichheit, **nicht** das korrekt eingefrorene Launcher-/BUY-Profil. Fuer einen nuetzlichen Regressionsbeweis getrennte realistische Fix-Fixtures mit Launch `value=0`, `gas_limit=6603270` und BUY `value=1e12`, `gas_limit=3000000` verwenden und die gleichen Daten strikt beiden Signierern zufuehren. Die synthetische Signier-ID bleibt synthetisch; kein LIVE-Versuch.

**Tests:** alle Varianten raw + hash gleich; Test faellt bei einem geaenderten Signatur-`v`, RLP-Integer-/List-Encoding oder unerwartetem Byte abweichend rot; source-only `#[cfg(test)]`-Aenderung in G0.7 darf **nicht** die freigegebenen Live-Signed-Bytes veraendern. Enge `cargo test`-Probe und bestehenden No-Send-Guard beibehalten. Die Quertest-Gleichheit allein liefert keine unabhaengige Kryptographie-/Mainnet-Erfolgsbescheinigung; Golden-Vektoren behalten eigene Aussagekraft.

### E4-4 — G0.7 behaelt kurzlebige Key-Rohkopien und geerbte Key-Environment

**Fundstelle:** `execution/src/bin/g07_mainnet_self_launch_buy/inputs.rs::key_from_env` ca. Z. 28–35:
`let raw = req_env(name);` -> `hex::decode(...)` liefert `Vec<u8>` -> `SigningKey::from_slice(&bytes)`. Danach verlassen `raw` und `bytes` ohne explizites Zeroize den Scope, auch auf manchen Fehlerpfaden. Die geerbten Env-Variablen `DARKBOT_G07_LAUNCHER_PRIVATE_KEY` und `DARKBOT_G07_BUYER_PRIVATE_KEY` bleiben im Prozess. Die `SigningKey`-Objekte muessen dagegen fuer den spaeteren signierenden Schritt **absichtlich** bis zur Nutzung verfuegbar sein.

**Konkrete Risikoeinordnung:** Restliche Key-Kopien/ENV erhoehen den potentiellen Expositionsradius bei Process-Memory-/Debug-/Crash-/unbedachter Spawn-Inspektion. Dies ist **keine** bereits nachgewiesene Schluesseluebertragung an einen externen RPC: `g07.../chain_reads.rs::child_command` (ca. Z. 36–43) verwendet fuer seine `curl`-Unterprozesse **`env_clear()`** und uebergibt keine privaten Keys. Die Sicherheitsluecke ist damit ein **Hygiene-/Defense-in-depth-Befund**, nicht ein bewiesener Mainnet-Key-Diebstahl.

**Minimale Härtung (Fables Patch 2 nur nach Anpassung):**
1. Den aus ENV gelesenen `String` und dekodierte Key-Bytes **direkt mit RAII-gesichertem Zeroize auf allen Rueckgabe- und Fehlerrouten** halten, bevorzugt `zeroize::Zeroizing<String>` bzw. `Zeroizing<Vec<u8>>`, um ein `unwrap_or_else(panic!)` vor manuellem `zeroize()` nicht als Sicherheitsloch zu behalten. Eine explizite `zeroize`-Abhaengigkeit nur hinter `g07-mainnet`, Lockfile-/Feature-Flags geprueft, keine Produktions-Hot-Path-Allocation hinzufuegen.
2. ENV-Schluessel **vor** jedem moeglichen Subprocess-/Thread-/TLS-Abschnitt entfernen oder anderweitig so scopen, dass sie nicht unnoetig vererbt werden. Da das Workspace `edition = "2024"` und `std::env::remove_var` in Rust 2024 **unsafe** ist, den Ein-Thread-Startup-/Concurrent-Library-Kontext und den Betriebssystem-/Prozessvertrag **explizit beweisen**, bevor Fables `unsafe { env::remove_var(name) }` kopiert wird. Bei unklarem Concurrent-Environment niemals ein ungeprueftes `unsafe` als Codehaertung implementieren; sichere vorgelagerte Startprozess-Sanitization und explizit gereinigte Child-Environments bevorzugen. Keine Kindprozesse, die Secrets erben.
3. An beiden `key_from_env`-Aufrufen in `main.rs` ca. Z. 90–91 dieselbe Adresse, denselben Signer und die unveraenderten Launch-/BUY-RLP-Hashes verlangen; niemand darf einen falschen Key verschlucken oder bei fehlendem Key fail-open starten. Die bestehenden Guards/Latch-/Capture-/Egress-Order bleiben unveraendert.
4. **G0.7-Source-/Probe-Hash-Scope bewerten:** Der bereits gepinnte Test-/Quellstand und die genauen Freigabe-Hashes muessen nach einer Aenderung von `inputs.rs` ueberprueft werden. G0.7 jetzt nicht aus kosmetischen Gruenden re-signieren oder die alten DRY-/TX-Hashes neu ableiten. Falls ein Sicherheitsfix am eingefrorenen Laufzeitcode vor der spaeteren wirtschaftlichen Freigabe wirklich erforderlich ist, **gezielt requalifizieren**, mit neuem Code-/Plan-Hash und expliziter Dokumentation der unveraenderten signed-byte-Identitaet. Alternativ streng kontrolliert bis nach dem wirtschaftlichen Gate vertagen, aber die erwartete Key-Hygiene als offenes Sicherheitsrisiko dokumentieren. Ein Testfix allein darf nicht still die Vertragsautoritaet wechseln.

**Gezielte Offline-Tests:** gute/fehlende/falsche Hex-Key-Inputs und unterschiedliche Laengen ohne Secret-String in Fehlermeldungen; isolierte Startup-/Child-Environment-Pruefung mit *Dummy*-Keys; gleiche abgeleitete Signeradresse und exakt gleiche synthetische RLP-TX-Bytes; keine neuen Threads/IO im gemessenen Hot-Path, Guard `UNARMED`. Memory-Nullung nicht durch einen Test vortaeuschen, der ausschliesslich `raw.zeroize()` im happy path prueft; RAII-/Error-Exit-Pfade und ggf. eine kontrollierte Codeinspektion erforderlich.

### Quellreview ohne neu bestaetigten Implementierungsfehler

Die Fable-Etappe nennt Kurvenarithmetik (`supply = TOTAL + h - ceil(k*10^18/(r+reserve))`), Rundungen, Fee-Split, `floor`-Slippage, RLP-Long-Form/kanonische Integers, EIP-155-Signieren/Recover und kanonischen `newTokenV6`-ABI-Decoder. Bei der **gegengeprueften Implementierungsstruktur und vorhandenen Testcases** fand sich kein belastbarer neuer Defekt, der ein Ticket rechtfertigt. Die konkrete G0.7-Economics-Ausgabe `176693134169499757962` ist in der vorhandenen DRY-Requal-/Quote-Evidenz dokumentiert. **Nicht** aus Code-Lesen oder historisch berichteten GREEN-Tests ein neues staerkeres `VERIFIED` fuer beliebige Betragsprofile, veraenderte Tax-Konfigurationen oder ungepruefte externe Launches machen. Es wurde hier kein externer ERC20-/BNB-Geldkauf, kein EVM-Neulauf, keine neue mathematische Vollverifikation und kein kryptographisch unabhaengiger Signaturbeweis durchgefuehrt.

### Etappe-4a-Ziel und Agent-Abnahme

- **Jetzt im Scope:** E4-3 enger Test-/Evidenzfix, mit historischen Signed-Byte-Goldens und No-Send-Gates; E4-4 gesondert vor Freigabe pruefen/gezielt haerten, wenn Source-/Plan-Bindung kontrollierbar ist. Kein neuer Signer, kein RLP-Refactor, keine Codeaenderung an `transactions.rs::sign_legacy`.
- **Explizit nicht jetzt:** E4-1 Admission/Betrags-/Tax-Freigabe (Produktionsphase nach G0.7/G0.8); E4-2 ist bereits E3-1; E4-4 nicht pauschal ohne sicheren Rust2024-/Concurrency-Plan einbauen.
- **Vor jedem CODE-READY-Verdikt:** Nachweis der byteidentischen G0.7-/Production-Signer-Ausgaben mit synthetischen, identischen Inputs und der weiterhin eingefrorenen G0.7-TX-Hashes im Qualifikationspfad; keine Secrets in Testevidence; kein LIVE-Send; unveraenderte Limit-/Builder-/Wirtschaftsbindung. Der cross-test soll **nicht** als Ersatz des echten G0.7→G0.8 Erwerbs-/Producer-Witness-Gates bezeichnet werden.
- **Status:** 4a **REVIEWED**, keine neuen Economics-/ABI-/RLP-Implementierungsbugs nachgewiesen, aber E4-3+E4-4 **OPEN**; 4b (Intent, Submission-Policy, Flap-Binding, Chain-State) folgt separat.

**Zieltestkommandos fuer den Implementierer (in diesem Review nicht ausgefuehrt):**

~~~bash
cd bsc/engine
cargo test --locked -p darkbot-bsc-execution --features g07-mainnet --bin g07_mainnet_self_launch_buy
cargo test --locked -p darkbot-bsc-trading
cargo test --locked -p darkbot-bsc-execution --features g07-mainnet --test g07_no_send
cargo fmt --all -- --check
~~~

Falls Tests zusaetzlich `test-support` oder explizite Testtargets benoetigen, die tatsaechliche Feature-Matrix lesen und den Scope sauber dokumentieren. Alle Tests **ohne** Mainnet-Send, reale Wallet-Secrets, GUARD-Arming oder Erhoehung der Freigabegrenzen; keine langen R0/DRY-Neulaeufe ohne einen wirklich invalidierten Input. Vor einer produktiven Profiloeffnung muss erst der verbindliche G0.7/G0.8-Mainnet-Versuch spaeter **separat** vom Nutzer freigegeben und abgeschlossen werden.


## 5. Nachfolgende Etappen

Weitere durch den Nutzer gelieferte Fable-Etappen werden in **dieselbe Datei** aufgenommen, aber erst nach unabhaengigem Code-/Phasenplanabgleich. Bereits behandelte Behauptungen nicht doppelt als Bug anlegen. Auch nach erfolgreicher Umsetzung bleibt eine Etappe „FIXED / TESTED“ erst mit konkretem Commit und Tests/Evidence belegbar; sonst `OPEN`.

| Etappe | Geplanter Gegenstand | Stand |
| --- | --- | --- |
| 1 | G0.7 Guard/Vertrag/Readiness/Requal | **REVIEWED / FIXES OPEN** |
| 2 | G0.8 Verifikation/Witness/Evidence | **REVIEWED / FIXES OPEN** |
| 3 | G0.6a DRY-/Requal-Werkzeuge | **REVIEWED / FIXES OPEN; SALT A/B UNDECIDED** |
| 4 | Trading/Signer/RLP/Intent | **PARTIAL: 4a REVIEWED / E4-3 & E4-4 OPEN; 4b AWAITING INPUT** |
| 5 | Execution/Submission/TLS | AWAITING INPUT |
| 6 | geth_ipc Rust | AWAITING INPUT |
| 7 | Go-geth Overlay/Rust-Paritaet | AWAITING INPUT |
| 8 | Journal/Records/Legacy | AWAITING INPUT |
