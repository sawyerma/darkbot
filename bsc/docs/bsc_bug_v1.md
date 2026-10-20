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


## 3. Nachfolgende Etappen

Weitere durch den Nutzer gelieferte Fable-Etappen werden in **dieselbe Datei** aufgenommen, aber erst nach unabhaengigem Code-/Phasenplanabgleich. Bereits behandelte Behauptungen nicht doppelt als Bug anlegen. Auch nach erfolgreicher Umsetzung bleibt eine Etappe „FIXED / TESTED“ erst mit konkretem Commit und Tests/Evidence belegbar; sonst `OPEN`.

| Etappe | Geplanter Gegenstand | Stand |
| --- | --- | --- |
| 1 | G0.7 Guard/Vertrag/Readiness/Requal | **REVIEWED / FIXES OPEN** |
| 2 | G0.8 Verifikation/Witness/Evidence | **REVIEWED / FIXES OPEN** |
| 3 | G0.6a DRY-/Requal-Werkzeuge | AWAITING INPUT |
| 4 | Trading/Signer/RLP/Intent | AWAITING INPUT |
| 5 | Execution/Submission/TLS | AWAITING INPUT |
| 6 | geth_ipc Rust | AWAITING INPUT |
| 7 | Go-geth Overlay/Rust-Paritaet | AWAITING INPUT |
| 8 | Journal/Records/Legacy | AWAITING INPUT |
