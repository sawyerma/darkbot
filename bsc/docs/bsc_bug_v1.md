# BSC Bugfix V1 — gepruefte Korrekturauftraege fuer Astra / Claude

**Stand:** 2026-10-20 · **Abdeckung:** Etappen 1–8 (einschliesslich 4a/4b und 7a/7b) · **Ausgangs-Codepruefstand:** `cac786e66725f4ca23b4bb01a65209770099c2c1` (`main`) · **Review-Abschluss:** `5f4b18baedf3a71f3860dca5a88b9eb6df19946c`

**Zweck:** Verbindlicher, quellcodegestuetzter Korrekturauftrag. Dieses Dokument ist **kein neuer Phasenplan**, kein Test-PASS und keine Live-Freigabe. Die acht Fable-Etappen wurden in mehreren quellcodegebundenen Pruefschritten abgeglichen; pro Abschnitt stehen der jeweilige gepruefte Commit und die offenen Grenzen. REVIEWED bedeutet nicht, dass alle Dateien des gesamten Repositories gelesen, Codefixes implementiert oder Tests ausgefuehrt wurden. Bei einem neuen HEAD muss der Implementierer die relevanten Stellen neu vergleichen, bevor er aendert.

## Implementierungsstand — 2026-10-21, Paket E4

- **FIXED / TESTED:** E4-3/4. G0.7-vs-Trading RLP/Hash-Parität für synthetische Launcher/BUY-Rollen und Null-/Long-RLP-Fall, Canonical-Decoding/Low-s/Sender-Recovery; RAII-Zeroizing für rohe Key-Strings und Decode-Puffer; ENV-Cleanup im ausdrücklich einthreadigen Startup vor RPC/TLS/Workern. `sign_legacy` unverändert, keine echten Wallet-Keys geladen.
- **Tests:** 27 G0.7-Unit-, 11 No-Send-/Startup-Order- und ein unabhängiger bestehender Trading-Golden-Test PASS, `--locked --offline`, Feature `g07-mainnet`, rustc 1.93.1. Key-Cleanup mit isoliertem Environment-Modell inklusive fehlender/ungültiger Keys; realer Child-Environment-Clear-Test PASS. Kein unsafe ENV-Mutieren im parallelen Testharness. Nullung ist RAII-Codegarantie, kein behaupteter Speicherforensik-Test.
- **Neue Quellbindung:** `inputs.rs` `8f810861fe472baf0d6005e83ca92da28e753515ba7747187434ca3e9e35afb8`; `main.rs` `5aa77bc6d8054efb63a2336ea772d4bbe0049d61e37e99aede63a6d8688dbfd5`; `transactions.rs` `2da8c1dd3ee9f09bc8e481e72b129e8a65eb8cbcbd321a7489593a2da5e827a0`. Offline erzeugter Probe-Plan `20c4cfd212121a26d0e228ce4d8bb0f1bbf3e2ad119cf20f63f235fecd8e2b8a`; Ausgabe bestätigt keine Verbindung/Sendung. Die Quellen/Lockfile ändern den Plan: alte Probe-Plan-Freigaben gelten hierfür **nicht**. Eine spätere freigegebene Netzwerk-Probe ist hier nicht ausgeführt/qualifiziert. Historische Signed-Byte-Evidence bleibt unverändert; synthetische Parität ist kein erneutes Signieren des echten Hashpaars.
- E4-1 und E4b-1/2 bleiben **DEFERRED** bis zur zulässigen Produktionsphase; E4b-3 bleibt UNRESOLVED/No-Retry, kein Nonce-Burn. G0.7 **UNARMED**.

## Implementierungsstand — 2026-10-21, Paket E3 / E8-3

- **FIXED / TESTED:** E3-2/3/4/5/6 und E8-3. Explizite Child-Environment-Allowlist, Keys ausschließlich für Simulation/DRY-Signierer; DBREC-Fehler vor Vergleich/Export terminal, verpflichtende Werte/Typen/Hashes; überlaufsicheres aktuelles Attempt-Limit; präziser struktureller Zero-Egress-Scope; Workspace-Toolchain im Wrapper; Legacy-Probes vor IO gesperrt.
- **Tests:** 5 isolierte Shell-Regressionen und Bash-Syntax PASS; 3 Requal- und 2 DRY-Binary-Tests PASS (inklusive alter/neuer Scope-Notiz und fremder Notiz STOP). Foreign-CWD-Wrapper: Workspace/Pin und Buildfehler-Weitergabe PASS, echter Offline-Build PASS, ungültiges Argument erwartungsgemäß Exit 2. Tatsächlich verwendetes rustc 1.93.1 (Homebrew); rustup nicht installiert. Kein echter DRY-/RPC-Lauf.
- **E3-1 / E4-2 BLOCKED:** ausdrückliche Salt-Entscheidung A/B vor wirtschaftlicher Freigabe fehlt; historische Vertraulichkeitsaussage präzisiert. Kandidat und alte Evidence unverändert. E3-7 bleibt dokumentiertes Provenienzlimit, weitergehendes Pinning DEFERRED.

## Implementierungsstand — 2026-10-20, Paket E1

- **FIXED / TESTED:** E1-1 bis E1-7. Requal-Verifier/PASS, sieben eindeutige Name/Hash-Bindungen (einschließlich Quote), sechs Einzelchecks und aliasfreie Lauf-/Frozen-/Policy-Pfade; eingefrorene DRY-/Wallet-/Nonce-/Funding-Identität; V4-Prosa; explizite Guard-Attempt-ID; Root-/Latch-Prüfung; einzelne DRY-Flags; UTF-8-Bytegrenze.
- **FIXED:** E1-8 Provenienzkommentar; keine zusätzliche Live-Aussage des konstanten TAX-Felds.
- **Tests:** 26 Readiness/shared Unit-Tests, 9 Guard/Vertrags- und 5 Readiness-CLI-Tests PASS; 1 gezielter Requal-UTF-8-Test PASS. Nach Pfadkompatibilitätskorrektur Vertrags-Mutationsregression erneut PASS. Ausschließlich offline, synthetische Fixtures ausdrücklich ohne Chain-Autorität.
- **E1-9 BLOCKED (unabhängige Artefaktverfügbarkeit):** Originale Requal-Records und bisheriger V4 bleiben unverändert lokal/untracked. Neuer inaktiver Review-Vorschlag: `bsc/config/g07_attempt1_acceptance_v4.e1-review.dbrec`, SHA `82f862c2046c656496378a288c03e14a17c1a58c19f81a98b8c932b637ddb278`. Mit vorhandener echter PASS-Evidence offline erzeugt; keine neuen R0-/DRY-Läufe. Approved-Variante ausschließlich berechnet: `6f877ae0f08bc1d795f4ec8370b033b83259cdbaf4e6296550d30d3c24cecce6`. Die alten `5da0068b…` / `4308f2cb…` sind **SUPERSEDED**, keine Freigabereferenz. Der bisherige Operator-Vorschlag wurde nicht ersetzt; vor Freigabe muss der neue geprüfte Vorschlag ausdrücklich übernommen werden. Keine sichere externe Artefaktablage eingerichtet, daher keine unabhängige Verifizierbarkeit behauptet.
- G0.7 bleibt UNARMED / NOT_EXECUTED; signed-byte-Hashes und 481163500000000-wei-Limit unverändert. E1-OPTION bleibt DEFERRED.

## Implementierungsstand — Paket E2

- **FIXED / TESTED:** E2-2, E2-5, E2-6b/c, E2-7. Begrenzte read-only RPC-Recovery, Receipt-Fehleranzahl/letzte Ursache statt verschlucktem Fehler; terminale persistierte UNRESOLVED-Diagnosen; UTF-8-Bytegrenze und Providertext-Redaktion; Kostenfehler/Overflow fail-closed; Timing-Ausgabepfad verpflichtend, zusätzlicher gespeicherter Self-Launch-NON-CLAIM. Reporter samt Proto-Build hinter nicht-default `status-reporter`; bestehender Installer-Build explizit angepasst.
- **Tests:** 13 G0.8-Verifier-, 5 Timing-, 5 Binary-Vertrags-, 2 G0.7-Evidence-Handoff- und 3 Reporter-Tests PASS. BSC-only `cargo check --locked --offline` ohne Root-`protocol/` PASS. Zwei veraltete Test-Erwartungen (redigierter Fehlertext, Anzahl NON-CLAIMs) korrigiert; keine Mainnet-Probe.
- **E2-1 DEFERRED:** sichere bestehende UNRESOLVED-/No-Retry-Semantik erhalten; keine unbewiesene Expiry-/Non-Inclusion-Autorität ergänzt.
- **E2-3/E2-4 BLOCKED:** kein qualifiziertes originales v5.25-Tax-Receipt verfügbar. Fehlende State-/Capture-/Competitor-Daten führen nun UNRESOLVED statt ADAPT; die fachliche harte Event-Mismatch-Regel bleibt bis realem ABI-/Tax-Nachweis ungeändert und nicht als FIXED deklariert.
- E2-6a widerlegt, kein Patch; E2-8 und nicht beauftragte E2-9-Verschärfungen DEFERRED. G0.8 weiterhin NOT_EXECUTED, kein Timing-/Erwerbs-PASS behauptet.

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
 
**E2-6c — ausdrueckliche NON-CLAIM im dauerhaft gespeicherten Timing-Record (konkrete Implementierungsanweisung):** `g08_producer_timing.rs` schreibt bereits `v::AUTHORITY="ORIGINAL_SUCCESSFUL_PRODUCER_EXECUTION_ATTEMPT"` und wiederholte `v::EXPLICITLY_NOT_AUTHORITY`-Texte in `record/src/schema/g08.rs::timing_verdict`. Beim E2-6b-Fix soll der Coding-Agent mindestens einen weiteren expliziten, unveraendert mit dem Resultat gespeicherten Text aufnehmen: **"SELF_LAUNCH_BUNDLE_ONLY; DOES_NOT_MEASURE_FOREIGN_LAUNCH_REACTION_T_SIGNAL_TO_T_SEND"**. Die Aussage muss die konkrete Gate-0-Self-Launch-Scope-Bindung treffen, ohne die autoritative Definition `T_buy - T0` abzuwerten. Eine spaetere echte Fremdlaunch-Reaktionsmetrik benoetigt **eigene** Hook-/Clock-/Source-Evidence. Bestehendes wiederholtes Textfeld verwenden, sofern die validierte Field-Length-/Schema-Semantik dafuer passt; nicht wegen eines einzelnen Textzusatzes unnoetig eine neue Record-Version erfinden. Binary-Encode/Decode-Golden und positive wie negative Scope-Tests fuer die neue NON-CLAIM anpassen. **Diese Dokumentation ist noch kein Codefix und kein Original-Producer-Witness.**

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
- Offizielle Flap-Referenz: `https://github.com/flap-sh/flap-skills/blob/main/launch-bnb-token-on-flap/references/salt-finding.md`. Der Token wird nach dem Protokoll ueber CREATE2 aus Portal (Deployer), Salt und Clone-Initcode mit TAX_V3-Implementation berechnet; der offizielle Salt-Finder nutzt einen frischen Zufalls-Seed. **Der historische Read-only-Fremdcaller-Pfad ist inzwischen belegt; eine real inkludierte fremde Mainnet-Transaktion ist es nicht.** Die konkrete bestehende Kollisionsgefahr wird im folgenden Zusatzbefund eingrenzt. Kein Mainnet-Launch darf zur Pruefung dieser Behauptung ausgefuehrt werden.

**Ergaenzung 2026-10-20 — historischer Cross-Caller-Nachweis (E3-1, kein neuer LIVE-Test):** In `bsc/tools/g06a_economics_evm_crosscheck.cjs` ist der lokale Test-`LAUNCHER` auf `0x000000000000000000000000000000000000a11c` gepinnt und unterscheidet sich vom wirklichen G0.7-Launcher `0x3e7B487Ae5e1c49aB1cF6222BDb5cf016e86aacd`. Die im Repository enthaltene historische `economics_crosscheck.json` dokumentiert beim eingefrorenen Mainnet-Parentblock `mainnet_estimate_gas_launch_success=true`, `mainnet_eth_call_launch_success=true` und `mainnet_eth_call_predicted_token_match=true` fuer den Dummy-Caller; dies ist **echte read-only Ausfuehrung gegen die damalige Chain-State-Quelle**, aber **keine echte inkludierte Transaktion**. `local_simulation.json` dokumentiert `launch_success=true` mit dem tatsaechlichen Launcher und **derselben** vorhergesagten CREATE2-Token-Adresse. Auch `predictedToken(portal,impl,salt)` enthaelt in dieser Implementierung **keine Caller-Adresse**. Damit ist die frueher nur hypothetisch bezeichnete *Caller-Unabhaengigkeit der Tokenableitung und akzeptierte Dummy-Caller-Launch-Semantik in diesem historischen Blockzustand* deutlich staerker gestuetzt. **Nicht bewiesen** sind eine bereits erfolgte fremde Chain-Inclusion, unveraenderte aktuelle Portal-/State-/Freigabebedingungen oder der Erfolg bei konkurrierender Publikation/Builder-Order. Fuer die weitere Entscheidung A/B zuerst diese vorhandene Evidenz verwenden; externe Neupruefung nur bei einer tatsaechlich veraenderten Vertrags-/State-Annahme, **keinen** neuen Test-Launch senden.

**Risiko:** Ein Dritter kann Kandidat und Token-Adresse offline im Voraus kennen. Falls die aktuelle `newTokenV6`-Deployment-Semantik einen anderen Caller nicht an der Nutzung der bekannten CREATE2-Adresse hindert, kann ein fremder Vorab-Launch die gewaehlte Adresse belegen; G0.7 STOP vor dem Latch oder ein Race/Revert beim Release ist moeglich. Der tatsaechliche fruehere Fremd-Launch bzw. eine bereits belegte Adresse sind **nicht belegt**. Der vorhandene G0.7-Test `rpc_code(token)` mit `predicted token already exists` ca. Z. 185–191 findet eine **bereits belegte** Token-Adresse, garantiert aber **nicht** die Abwesenheit einer spaeteren Race-/Builder-Propagation. Fables „A: vor Latch Token frei pruefen“ ist insofern **schon weitgehend implementiert**; erfordert allenfalls nachgewiesene Verkuerzung/Aktualitaetspruefung, keinen neuen Preflight-Mechanismus.

**Keine automatische Codeentscheidung:**

- **A — eingefrorenen Salt behalten:** Exaktes Launch-/BUY-Hashpaar, Vertrags-/Requal-Bindung und wirtschaftliche Obergrenzen bleiben unveraendert. Bei spaeterer wirtschaftlicher Freigabe muss das dokumentierte Salt-/Adress-Collision-/Front-run-Risiko explizit akzeptiert werden. Vor dem Latch und moeglichst nahe am autorisierten Release weiterhin Code-/Head-/Nonce-/Chain-Gates ausfuehren. **Auch ein frisch leerer Account ist keine Garantie** gegen Fremdausfuehrung nach dem Check. Kein neuer unterschwelliger Salt-Wechsel.
- **B — neuer kryptographisch zufaelliger Salt (mit Vanity-Suche):** nur nach neuer ausdruecklicher Entscheidung; neue Token-Adresse, neue Launch-Calldata, neues signiertes Launch-/BUY-Hashpaar, notwendige DRY-/EVM-/Economics-/Guard-/Contract-V4-(oder Folgevertrags-)Bindung und neue Approved-Variant-SHA-256. Die bisherigen PASS-Evidenzen bleiben historische Belege, autorisieren aber **nicht** den neuen Kandidaten. **Besonderer Pflichtpunkt:** blosse Ersetzung von `chooseSalt` beseitigt nicht das Leakage: alle externen `eth_estimateGas`-/`eth_call`-Pfade mit echten sensitive Launch-Daten und die G0.7-Laufzeitpruefung muessen fuer eine *Geheimhaltungs-Behauptung* ebenfalls entfernt/auf unverfaengliche Kontrollfaelle bzw. lokale EVM-Qualifikation umgestellt werden. Jeden Verifikationsverlust explizit bewerten; keine G0.7-Safety-Checks ersatzlos loeschen.
- Kein `Option B` im laufenden Bugfix stillschweigend umsetzen. Zu A/B **keinen** neuen R0-Scan oder Echtgeldversuch veranlassen. Zuerst den bereits versionierten historischen Dummy-Caller-`eth_call`-Nachweis und die Real-Launcher-DRY-Simulation als bestehende Basis auswerten; nur die heute noch ungeprueften Portal-/State- und Wettlauf-Annahmen zielgerichtet read-only nachpruefen. Danach Benutzerentscheidung mit konkretem Risiko.

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


## 5. Etappe 4b — Intent-Lane, Deployment, Final-Binding, Submission-Policy und Release

**Review:** 2026-10-20 · **Fable-Pruefstand:** \`cac786e\` · **aktueller direkt gepruefter BSC-Code:** \`df888d71a5f922a91699de8638c4caacac719c2f\` (\`main\`). Keine BSC-Codeaenderungen zwischen diesen Quellstaenden festgestellt; HEAD zwischenzeitlich um Project_0-Dokumentation und Etappen-Reviews fortgeschrieben. **Alle Implementierungen bleiben unveraendert.** Etappe 4b dient der fachlichen Einordnung und spaeteren Produktionsqualifikation; sie beweist keinen neuen Bug in der aktuellen G0.7-Binary.

**Gepruefte Quellen:** \`trading/venues/flap/deployment.rs\`, \`binding.rs\`, \`intent/mod.rs\`, \`intent/lane.rs\`, \`submission_policy/plan.rs\`, \`release.rs\`, \`capability.rs\` (betroffene Faehigkeits-/Release-Stellen), \`chain_state/canonical.rs\`, \`economics/policy.rs\`, \`execution/src/startup.rs\`, \`execution/Cargo.toml\`, \`G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md\`, \`G07_REQUALIFICATION_RESULT_20261020.md\`. Die grossen separaten Testdateien wurden fuer diesen Bericht nicht neu vollstaendig durch einen Rust-Testlauf qualifiziert. **Keine Cargo-/RPC-/Mainnet-Tests ausgefuehrt.**

### Etappe-4b-Urteil

| ID | Befund | Urteil | Jetzt umsetzen? |
| --- | --- | --- | --- |
| E4b-1 | Produktionsdeployment kennt nur v5.24.0, Mainnet ist laut neuer Requal v5.25.0 | **BESTAETIGT; FAIL-CLOSED / ZUKUENFTIGE PRODUKTIONS-QUALIFIKATION** | **Nein**, erst spaetere C2-/Produktionsphase nach Gate-0 |
| E4b-2 | Signieren plus finaler Public-Key-Recovery kann lokale Latenz verbrauchen | **BESTAETIGTER POTENZIELLER KOSTENPFAD; GESCHAETZTE NS/MS NICHT GEMESSEN; NOTWENDIGKEIT NACH T0 NICHT ALLGEMEIN** | **Nein**, erst gemessene C2/C3-Instrumentierung |
| E4b-3 | Legacy-BUY ohne Zeitlimit, Builder-Expiry entwertet disclosed Bytes nicht | **BESTAETIGT; BEREITS UNTER E2-1 BERUECKSICHTIGT** | Kein neuer Nonce-/Retry-/Burn-Patch |
| E4b-4 | \`runner-startup\` immer auf dem \`execution\`-Abhaengigkeitspfad; Konstruktor oeffentlich | **BESTAETIGTE API-EXPOSITION, ABER KEIN FALSCHER AUFRUFER GEFUNDEN** | Nur weiter kontrollieren, kein ungepruefter Refactor |
| E4b-AUDIT | Intent, Binding, Plan, Release und Canonical-Typen weitgehend fail-closed | **KEIN NEUER REPRODUZIERBARER IMPLEMENTIERUNGSFEHLER** | Keine unnötigen Aenderungen |

### E4b-1 — Produktions-Deployment-Identitaet von Flap ist veraltet (nicht G0.7)

**Source- und Evidence-Bindung:** \`bsc/engine/crates/trading/src/venues/flap/deployment.rs::bsc_mainnet_v5_24_0\` ca. Z. 182–221 ist die **einzige** in diesem Modul gelieferte gepinnte Mainnet-Deployment-Variante. Sie erwartet \`portal_version="v5.24.0"\`, Implementation \`0x27a0f9da7f5bead616fe0bf02bf0c793bece4e20\` und deren alten SHA-256. \`FlapDeployment::validate\` ca. Z. 136–167 stoppt bereits bei abweichender \`portal_version\` mit \`DeploymentError::PortalVersion\`, danach bei Implementation-/Fingerprint-Drift. \`G07_REQUALIFICATION_RESULT_20261020.md\` und \`G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md\` dokumentieren Mainnet v5.25.0, Implementation \`0xe13227370996f892a56d2ff4a30f6026887b48fb\`, neuen Implementation-SHA-256 (Praefix \`42c91172\`), identische Token-TAX_V3-Implementation. Das ist **historische, quellgestuetzte** Feststellung, keine in diesem Review erfolgte aktuelle Mainnet-Abfrage.

**Konsequenz:** Der heutige produktive Trading-Core kann einen Live-Deployment-Snapshot von v5.25.0 durch das alte \`ValidatedDeployment\` nicht freigeben; folglich keine \`classify_launch\`-/\`ExpectedBuy\`-Kette fuer diese Live-Version. **Absichtlich sichere Sperre**; G0.7 besitzt getrennt requalifizierte, eingefrorene Flap-v5.25.0-Preflights, Transaktionsbytes und Guard. Die G0.7-Requal reicht **nicht** zum Freischalten beliebiger Fremd-Launches im Produkt.

**Spaeterer Auftrag (explizit nur nach Gate-0):** \`bsc_mainnet_v5_25_0()\` mit **vollstaendigen** tatsaechlich gepinnten Proxy-/Implementation-/Tax-Bytecode-Fingerprints, korrekter address/generation/token-derivation und Current-Chain-Beobachtung ergaenzen. Bestehende v5.24.0-Variante **als historische Identitaet erhalten**, keine stillen Werte ersetzen. Goldens fuer bekannte Versionen plus Drift-/Reorg-/Codehash-Negativtests. Erst dann gesonderte native-Tax-/Fee-/Quote-/Metadata-/Beneficiary-/Input-Amount-C2-Qualifikation (siehe E4-1); keinen Release aus dem blossen Hinzufuegen einer Versionskonstante ableiten. Kein neues Baseline-R0, keine DRY-Requal-Wiederholung, solange deren eigener Scope unveraendert bleibt.

### E4b-2 — Signatur und Recovery: tatsaechliche Timinggrenzen statt Schaetzwerte

**Quellstellen:** \`trading/src/signer/mod.rs::Signer::sign_legacy\` und \`trading/src/venues/flap/binding.rs::verify_final_buy\` (ca. Z. 241–281) mit \`recover_legacy_sender\`. Der Binding-Vertrag validiert zuletzt den zugehoerigen Snapshot, alle signierten Sender-/Nonce-/Gas-/Chain-/Portal-/Wert-/Token-/Minimum-Felder. Eine Public-Key-Recovery ist heute tatsaechlich im \`verify_final_buy\`-Pfad; \`sign_legacy\` fuehrt secp256k1-Signierung aus. **Die von Fable angegebenen ca. 0.1–0.3 ms wurden nicht gemessen** und duerfen nicht als belegte Latenz im Ledger stehen.

**Wichtige Korrektur der Fable-Interpretation:** Der Satz „der Salt ist erst ab T0 bekannt und deshalb muss die BUY-Signatur immer danach erfolgen“ ist **generell falsch**. Nach \`BSC_BIRTH_ENTRY_ARCHITECTURE.md\` und \`GATE0_TESTPLAN_SHORT.md\` kann eine vollstaendige Launch-Transaktion bereits vor dem ersten tradable Execution-Zeitpunkt sichtbar sein, insbesondere beim kontrollierten Self-Launch. Dann sind Salt, Token-Ableitung und BUY-Felder moeglicherweise vor T0 qualifizierbar und die Signatur kann vorher erstellt werden. Die Binding-Doku sagt explizit: „for prepared payloads it runs before the trigger“. Das gilt **nicht** fuer unbekannte/private Launches ohne vorherige Daten; bei solchen Kandidaten muss der schnelle Reaktionspfad separat gemessen werden.

**Spaetere C2/C3-Arbeitsanweisung:** auf dem Zielhost und realer, bezeugter Fixture \`T_pre -> sign_ready -> verify_final_buy -> permit -> T_send\` messen, pro signiertem finalem Feld und pro Payload-Size/Cache-Warm-/Cold-Zustand, P50/P95/P99/MAX, allocations, Monotonic-Clock, ohne RPC im Hotpath. Zwei Faelle streng trennen: (A) vorbereitete Bytes, Signierung/Recovery vor dem Trigger; (B) erst zum Trigger vollstaendig bekannte Felder. Erst **nach** reproduzierbarem Bottleneck-Nachweis zwischen sicherheitserhaltenden Alternativen entscheiden (z.B. andere Signatur-Bibliothek oder Provenienz-gebundener interner Signer-Trust). Eine bloess durch den Sender gesetzte Adresse ist **kein Ersatz** fuer \`recover_legacy_sender\`; ohne starke kryptographisch gebundene Erzeuger-/Byte-Provenienz die volle Final-Bindung unveraendert lassen. Jede Optimierung muss unveraenderte RLP-Bytes und Same-Intent-/Nonce-/Snapshot-/Fallthrough-Negativtests beweisen. **Kein Patch jetzt.**

### E4b-3 — Disclaimer \`NoTimeBound\` und Disclosed-Bytes bestaetigt E2-1

**Quellstellen:** \`trading/src/submission_policy/plan.rs::TransactionValidity\` ca. Z. 388–393: \`NoTimeBound\`; \`SubmissionPlan::buy_validity\` ca. Z. 844–847 liefert immer \`NoTimeBound\`. \`Disclosure::may_still_execute\` ca. Z. 1091–1102 meldet nach jeglicher moeglichen Byte-Offenlegung selbst bei \`Rejected\`, \`ExpiredAfterDisclosure\` und \`Unknown\` \`true\`. \`capability.rs\` trennt \`expiry_revokes_disclosed_bytes=false\` von der API-Expiry. Keine qualifizierte Cancel-Funktion.

**Folgerung:** Builder-\`maxBlock\` und Transport-/Anfrage-Expiry sind **keine** On-Chain-TX-Zeitgueltigkeitsschranke. Ein bereits disclosed Legacy-TX kann grundsaetzlich solange spaeter ausgefuehrt werden, wie er durch Nonce/State/Fees/Chain weiter gueltig ist und irgendwo gehalten wird. Deshalb bestaetigt E4b-3 die Etappe-2-Korrektur gegen Fables pauschales \`UNRESOLVED -> ADAPT\` nach Head-/Nonce-Snapshot. Nonce-Gleichstand und temporär fehlende TX auf einem Knoten liefern allein keinen sicheren Negativbeweis; keine Wiederholung, keine zweite wirtschaftliche Transaktion, kein eigenmaechtiger \`Nonce-Burn\`. Erst nach quellgebundenem Reconciliation-/Exposure-Nachweis darf ein abgeschlossener Ausgang korrekt klassifiziert werden. **Keine neue E4b-3-Duplikat-Ticketnummer in der Reparaturliste; E2-1 bleibt der Owner.**

### E4b-4 — \`runner-startup\` / \`ArmedFence::from_durable_startup\`: exakter Aufrufstand

**Quellen:** \`bsc/engine/crates/execution/Cargo.toml\`: \`darkbot-bsc-trading = { path = "../trading", features = ["runner-startup"] }\` in normaler \`[dependencies]\` ohne Ausnahme fuer G0.7-Bins. \`trading/src/intent/mod.rs\` ca. Z. 417–451 exponiert \`#[cfg(feature="runner-startup")] pub fn ArmedFence::from_durable_startup(...)\`; die Annotation \`#[doc(hidden)]\` blendet nur Dokumentation aus und erzwingt **keine** Laufzeitautorisierung. Zugleich ist \`IntentLane::armed\` dadurch potentiell mit einem mintbaren Fence erreichbar.

**Zusaetzlicher eigenstaendiger Quellscan, ueber Fable 4b hinaus:** Alle **32 Rust-Quelldateien** unter \`bsc/engine/crates/execution/src/**\` am angegebenen HEAD einzeln auf \`ArmedFence\`, \`from_durable_startup\` und \`IntentLane::armed\` durchsucht. **Genau ein echter Produktionsaufrufer gefunden:** \`execution/src/startup.rs::ValidatedStartup::arm\` ca. Z. 281–293. Dort wird zuerst \`durable_fence(root,...)\` erfolgreich durchgefuehrt und **erst danach** \`ArmedFence::from_durable_startup\` aufgerufen. \`durable_fence\` ca. Z. 677–723: \`create_new\`, write, file \`sync_all\`, parent-directory \`sync_all\`, Fehler => fail-closed bzw. Restart-Quarantaene. **Keine** Verwendung des Konstruktors in G0.7, \`g07_builder_probe\` oder \`g07_pre_live_readiness\` gefunden. Dies ist eine konkrete negative Caller-Inventur, kein blosses Vertrauen in den Kommentar.

**Einordnung:** Kein gegenwaertiger Beweis fuer einen Bypass, der ohne durable Startup zulassige Transaktionen sendet. Aber weil der oeffentliche Konstruktor durch das global aktive Feature auch in nicht-Runner-Kontexten kompiliert, ist seine **API-Herkunftssicherung noch keine harte Typgrenze**. Falls die Produktionsarchitektur spaeter einen Ausbruch aus dem \`ValidatedStartup::arm\`-Pfad ermoeglicht, koennte ein fremder Codepfad denselben Fence direkt praegen. Das ist ein gezielt zu beobachtender **Defense-in-depth-/Modulgrenzenpunkt**, kein Anlass, G0.7-/Readiness-Module jetzt umzubauen oder wegen eines reinen hypothetischen Callers den kompletten Core zu refaktorieren.

**Etappe-5-Folgepruefung:** Bei jeder weiteren Execution-Aenderung Quell-/Modul-Grenze und Call-Graph erneut pruefen; keine neue \`from_durable_startup\`-Callsite ohne reviewed und durable \`StartupAuthorization\`-/\`create_new\`-/fsync-Proof. In der spaeteren produktiven Integrationsphase Konstruktion soweit sinnvoll hinter einen eng begrenzten Owner-/Token-Zugang oder ein Production-Runner-Feature stellen. Keine Proxy-/Fake-Fence-Fixtures ausser in strikt getrennten Offline-Tests als Produktions-PASS deklarieren.

### Ergebnis ohne neue Implementierungsbugs

Fables positive Bewertung der Intent-Lane/Final-Binding/Submission-Plan-/Release-Policy ist mit den inspizierten Quellen **vereinbar**: \`IntentLane\` bindet wirtschaftliche Identitaet und noncegeordnete Permits, \`verify_final_buy\` validiert einen echten signierten BUY mit Snapshot-Identitaet, \`SubmissionPlan::bind_dispatch\` bindet Intent-Ticket, Buy-Hash und Adapter-Set, \`release_blocker\` verlangt qualifizierte Ordnung/Atomizitaet/Same-Block/No-Leak/\`T_send\`. Die \`capability.rs\`-Quellen bestaetigen, dass **heute kein Providerverhalten als release-qualified gilt**, also kein voreiliges Release. **Aus Code-Lesung folgt nicht der Beweis, dass alle 55k/86k grossen Testdateien ausgefuehrt wurden oder keine versteckten Implementierungsfehler existieren.** Keine neue Fixpflicht fuer diese Kernlogik ohne konkrete Gegenbeweise.

**Etappe-4b-Abnahme:** 4b **REVIEWED / NO CURRENT-PHASE FIX**; E4b-1 spaeterer produktiver Deployment-/Profile-Nachzug, E4b-2 spaetere C2/C3-Latenzmessung ohne Safety-Bypass, E4b-3 zu E2-1 verlinkt, E4b-4 current Caller-Scope nachgewiesen und als API-Grenze offen. G0.7/G0.8 ohne LIVE, keine Veraenderung an signierten Bytes, keine Wiederaufnahme C1/C2 vor Gate 0.


## 6. Etappe 5 — Produktions-Execution: Startup, Budget, Dispatch, TLS, Builder-Probe

**Review:** 2026-10-20 · **Fable-Referenz:** `cac786e` · **unabhaengig geprueft:** `main`, `84bd285877c2d189b87a87ebb80ebd49afa51bb4`; seit `cac786e` wurde fuer die hier bewerteten BSC-Codepfade keine Implementierungsaenderung festgestellt. **Kein Codepatch, kein Cargo-/Mainnet-Test und keine Netzwerk-Submission in diesem Review ausgefuehrt.** Die bereits dokumentierten Etappen 1–4b bleiben unangetastet.

**Autorisierter Scope:** Nach `BSC_DUAL_VENUE_ROADMAP.md` und `G07_PORTAL_V5_25_REQUALIFICATION_PLAN.md` (bindender Update 2026-10-20) hat die qualifizierte G0.7-G0.8-Kette Vorrang; produktive C2/Unit-6-Runtime, GUI, generische Architektur-Refaktoren und Latenzoptimierung sind **nicht** Bestandteil der aktuellen G0.7-Code-Ready-Abnahme. `FROZEN_APPROVED_V4_SHA256=UNARMED`; unveraendertes Transaktionshashpaar, ein wirtschaftlicher Versuch, kein LIVE, kein Retry, kein Nonce-Burn. `BSC_PRODUCTION_BUILD_MASTER_PLAN.md` §§7–8/13/Unit-6 und `BSC_C1_PARALLEL_FAST_LANE_PLAN.md` regeln die spaetere single-owner- und Runner-Integration.

**Pruefumfang ueber den Fable-Text hinaus:** `execution/src/startup.rs`, `submission/send.rs`, `submission/tls/{owner,transport,connection,config}.rs`, die relevanten Stellen aus `intent/lane.rs`, `economics/policy.rs` und `submission_policy/plan.rs`; Dateiquelltext und Test-Inventar zu `execution/tests/{dispatch,tls,framing,hotpath_alloc,release,responses}.rs`, `submission/release_tests.rs` und `g07_builder_probe/plan.rs`. **Tests nur gelesen, nicht ausgefuehrt.** Die unabhaengige Originalbehauptung, der gesamte Submission-/Q5A-Pfad sei vollstaendig getestet, wird damit **nicht** aufgestellt. Der Rest von Q5A und jeder in Fable explizit als „nicht gelesen“ bezeichnete Pfad bleibt ausserhalb der behaupteten Vollstaendigkeitspruefung.

### Etappe-5-Urteil

| ID | Schwere / Phase | Evidenzbewertung | Behandlung |
| --- | --- | --- | --- |
| E5-1 | **Mittel–Hoch fuer spaeteres Produktions-LIVE**, Unit 6/C2 | **BESTAETIGTE FUNKTIONSLUECKE:** pro-Intent-Budget, kein kumulativ autorisiertes Session-Exposure | Pflichtgate **vor produktivem LIVE**, nicht G0.7 |
| E5-2 | **Mittel, lokaler Secret-Schutz**, Startup | **BESTAETIGTER IMPLEMENTIERUNGSFEHLER:** str-Slicing kann auf nicht-ASCII-UTF-8 paniken und Key-Daten im Panic-Text zeigen | Enger sicherer Fix empfohlen, offline testen; nicht G0.7 |
| E5-3 | **Mittel, Autorisierungsinvalidation**, Startup | **BESTAETIGTER GRENZFEHLER:** `saturating_add` bei `u64::MAX` macht Widerruf unwirksam | Enger Pflichtfix; Fables blosses `expect()` ist unter Nebenlaeufigkeit **nicht** automatisch fail-closed |
| E5-4 | **Niedrig**, Startup | **BESTAETIGTER VALIDIERUNGSWIDERSPRUCH:** `10000` bps Startup PASS, `BuyPolicy::new` STOP | Einzeiliger Fix + Boundary-Test |
| E5-5 | **Design/Verfuegbarkeit**, Runner C2 | **BESTAETIGTES BEWUSSTES VERHALTEN**, kein derzeitiger Sicherheitsbug | Kein automatischer Permit-/HALT-Refactor |
| E5-6 | **Latenz-/Wartungs-Risiko**, Runner C2/C3 | **BESTAETIGTE BLOCKIERENDE CONNECT-SEMANTIK**, reale Disconnect-Rate und Millisekundenwert **UNVERIFIZIERT** | Runner-Wartung neben Hotpath und Messung |
| E4b-4 | Fence-Herkunft | **ABGESCHLOSSENE QUELLCODE-CALLER-INVENTUR:** genau ein Produktionsaufruf in `ValidatedStartup::arm` nach durable Fence | Kein aktueller Bypass nachgewiesen, Gate nicht lockern |

### E5-1 — Kumulatives wirtschaftliches Budget fuer eine Produktions-Session fehlt

**Beweis:** `execution/src/startup.rs::StartupEconomics` ca. Z. 61–75 bietet `max_buy_input_wei`, `max_buy_gas_wei`, `max_provider_payment_wei` und `max_total_wei` fuer **eine** konfigurierte Submission. `validate_facts` Z. 595–612 verlangt `balance_wei >= max_total_wei` (Startfinanzierung einer einzelnen Maximalausgabe), `validate_economics` Z. 614–639 prueft `buy_amount + signed_max_gas + provider_bid <= max_total_wei`. `trading/submission_policy/plan.rs::SubmissionPlan::new` prueft wiederum das Budget des **einen** Intents. `IntentLane` fuehrt Dedup nach Token und noncegeordnete Intents; keinen kumulativen wirtschaftlichen Budget-Owner ueber mehrere Kauefe. Die vorallokierte Lane kann bis zu **65536** Plaetze zulassen. Keine sichtbare Einheit verhindert damit `N * einzelner_maximalbetrag` ueber die gesamte Laufzeit, soweit Nonces/Guthaben die Kauefe ermoeglichen.

**Bewertung:** Ein grosser Produktions-LIVE-Risikofehler **bei spaeterer Mehrfachausfuehrung**, aber nicht Beweis fuer heutige Ueberschreitung und **nicht** auf G0.7 uebertragbar (dort bindende max. ein Attempt, konkretes Budget und Guard). Auch ist `max_total_wei` nach aktuellem Typ **kein** bereits vereinbartes Session-Budget; es darf nicht heimlich semantisch umgedeutet werden.

**Pflichtauftrag vor Produktions-LIVE (Unit-6/C2, NICHT jetzt):**
1. Gesonderte und ausdruecklich freigegebene **Session-/Tages-/Run-Exposure-Obergrenze** als validierte Betreiberpolicy einfuehren (keine erfundene Zahl). Zu einem zentralen, ausschliesslichen Economic-Risk-/Nonce-/Intent-Owner gehoert eine **atomare, overflow-sichere Reservierung vor wirtschaftlicher Admission bzw. vor erstem potenziellen Egress**. Jedes beabsichtigte identische Multi-Adapter-Fanout zaehlt **einmal** fuer den einen wirtschaftlichen Intent; eigenes Launch-Gas, BUY-Wert/Gas und reale vertraglich zulaessige Bid-/Tip-/Provider-Kosten separat aufnehmen. Neben dem Session-Cap bleiben alle einzelnen Budgetobergrenzen bindend.
2. Ein disclosed Intent bleibt mit maximaler moeglicher Gesamtbelastung **reserviert**, solange Execution/Abrechnung ungewiss ist; weder Builder-`maxBlock`, HTTP-Reject, lokale Expiry noch Prozessneustart geben Geld oder Nonce frei. Nur nach typisiertem, quellengebundenem kanonischem Chain-/Reconciliation-Beleg Exposure-Abschluss verbuchen. Bei Reorg/replaced/late inclusion keine Doppelbuchung, keine Entlastung ohne sichere Ableitung; konservativ OVERRESERVE oder STOP.
3. Runtime-Neustart/Crash muss Session-Budget-/disclosed-Intent-Zustand sicher wiederherstellen bzw. quarantinen. **Keinen blockierenden Datei-fsync im BUY-Hotpath** einfuehren; stattdessen vorab dauerhaft qualifizierte Risk-/Ledger-/Admission-Strategie und klar dokumentierte Crash-Boundary, die mit dem bereits bestehenden Single-Writer- und Restart-Fence verifiziert wird. Bei fehlendem Budgetnachweis kein neuer BUY.
4. Tests: 1/2/3 Intents unter-/genau auf/ueber Cap; gleichzeitige Kandidaten; zwei Adapter dieselben Bytes; teilweiser Write/UNKNOWN; Reorg; Late BUY nach Providerablauf; Crash vor/nach Egress; u128/U256-Ueberlauf; fremder Token mit gleicher wirtschaftlicher Absicht; Nonce-Quarantaene. Negative Faelle sicher verweigern, kein zweiter Buy nach Budget-STOP.

**Abnahme:** dokumentierte maximale kumulative Kapitalexposition einer gesamten Session, einschliesslich weiterhin ausfuehrbarer offener Rohtransaktionen und Reorg-/Recovery-Faelle; pro-Intent- und Gesamtrisiko bleiben beide Grenzen. Kein pauschaler „Wallet-Guthaben ist Session-Budget“-Ersatz.

### E5-2 — Nicht-ASCII-Key-Datei verursacht Panic mit Secret-/String-Leak und umgeht Wipe

**Code:** `execution/src/startup.rs::read_signer_secret` Z. 642–668. Fuer nicht genau 32 Rohbytes wird `bytes` als UTF-8 eingelesen, getrimmt, dann `hex.len()==64` (**Byteanzahl**) geprueft. Anschliessend schneidet `&hex[index*2..index*2+2]` einen UTF-8-String an den numerischen ASCII-Bytegrenzen. Eine 64 Byte lange Zeichenfolge wie `"a"*61 + "é" + "a"` besteht die Byte-Laengenpruefung, trifft spaeter aber eine Nicht-Char-Boundary und **panikt statt `InvalidSignerEncoding`**. Rusts Str-Slicing-Panic kann die betroffene Eingabe inkl. Hex-artigem Key-Praefix in stderr oder Crash-Logs ausgeben. Die geplante Bereinigung `bytes.fill(0)` steht **nach** dem Parse-Closure; auf einem Panic-Unwind wird sie nicht ausgefuehrt. Die vorhandenen `SecretSources::Debug`-/`ValidatedStartup::Debug`-Redaktionen schuetzen diesen Panic-Pfad nicht.

**Pflichtfix:** Vor erstem `&str`-Index/Slicing: `hex.len()==64 && hex.as_bytes().iter().all(u8::is_ascii_hexdigit)` (bzw. die semantisch identische `hex.bytes().all(|b| b.is_ascii_hexdigit())`-Variante) pruefen und sonst **immer** `Err(StartupError::InvalidSignerEncoding)` ohne Secret-/Pfadinhalt zurueckgeben. Alternativ die Hex-Paare ausschliesslich als validierte ASCII-`&[u8]` parsen. Fables Patch 1 ist fuer das Grenzproblem tauglich, aber die Klasse **alle Fehler- und Panic-Pfade der Key-Kopien** sollte mit RAII-/`Zeroizing`-Schutz abgedeckt werden, soweit ohne neue Instabilitaet moeglich; die aktuelle `bytes.fill(0)` auf normalem `Err` beibehalten. Keine Text-/Debug-Ausgabe des gelesenen Secretwerts, keine `set -x`-Skripttests und kein echter Wallet-Key in der Fixture. Die legitime Dateiform **exakt 32 rohe Bytes** weiterhin erhalten. Nicht `StartupError::InvalidSignerEncoding` um geheime Eingaben erweitern.

**Tests:** 64 ASCII-Hex-Bytes OK, 64 Bytes mit Nicht-ASCII an ungerader und gerader Byteposition -> definierter `Err`, keine Panic; ungueltiges Hex bei 64 ASCII-Bytes -> `Err`; raw 32 Bytes -> Erfolg wie vorher; 63/65 Bytes/ungültige UTF-8 -> `Err`; Hex-Praefix `0x`/Whitespace wie zuvor definiert; Test darf Secret-Werte nie in Ausgabe schreiben. Catch-unwind (ausschliesslich mit Dummy-Daten) darf bei allen fehlerhaften Texten keine Panic finden. Validation und Signeradresse desselben legitimen Eingangs muessen identisch bleiben.

**Abnahme:** Kein schaedlicher UTF-8-Grenzen-Panic mehr; alle nicht wohldefinierten Keys fail-closed, keine Entschluesselungs-/Logging-Ausweitung; Single-startup-Build unveraendert; keine G0.7-Eingriffe.

### E5-3 — `GenerationOwner::bump` kann bei MAX eine alte Freigabe gueltig lassen

**Code:** `execution/src/startup.rs::bump` ca. Z. 173–185: `current.saturating_add(1)`, CAS. Bei `u64::MAX` ist `next == current`, `compare_exchange` kann trotzdem Erfolg melden. `GenerationOwner::revoke_authorization` liefert MAX, aber `StartupAuthorization::is_current` ca. Z. 196–204 vergleicht unveraendert denselben `GenerationSnapshot` und kann weiter `true` ergeben. `validate_config` ca. Z. 520–530 lehnt nur **Null**, nicht MAX ab. Das ist ein **wirklicher sicherheitsrelevanter Grenzfehler**, kein blosser Testfall fuer ein unrealistisches normales Laufzeitinkrement.

**Pflichtfix:** Strikter Nicht-Saettigungs-Vertrag. Bereits bei `validate_config` alle vier Generationen mit eindeutig dokumentierter maximaler sicherer Spanne oder reserviertem Terminalbereich **vor Startup-Validierung/Arming** ablehnen, statt `MAX` als gueltige Startgeneration zu akzeptieren. Zusätzlicher harter Runtime-Schutz gegen dennoch erreichtes MAX (race-sicher mit `Ordering::Acquire/AcqRel`): **Die bisherige Autorisierung darf nach einem Revocation-Versuch niemals weiter `is_current==true` bleiben**, auch bei erschöpftem Zähler. Eine reine `checked_add(1).expect(...)`-Panic **genuegt nicht automatisch**: wenn sie nur einen Hintergrund-Thread beendet, kann ein anderer Thread weiterhin mit der unveraenderten Generation traden. Implementierer muss entweder einen vorab geprueften/unueberwindbaren Wertebereich plus processweiten, unvermeidbar fail-closed Abbruch beweisen oder einen **sticky terminal revoked/halted**-Status in jedem Admission-/Send-/Authorization-Check beruecksichtigen. Keine Saturation, kein Wrap auf 0, kein stiller Weiterbetrieb.

**Zu Fables Patch 2:** Die zusaetzliche `GenerationOutOfRange`-Fehlervariante kann zweckmaessig sein, aber `u64::MAX/2` ist eine willkuerliche **gewaltige** Reserve; sie ist keine Sicherheitsbegruendung fuer den Umgang mit einer tatsaechlichen Ueberlauf-Kondition. Keine Aenderung der public `revoke_authorization() -> u64`-API ohne Caller-/Protokoll-Review; ggf. eigene getestete Fallible-Revoke-Semantik plus terminalen STOP. Das ist ein minimaler Safety-Fix, nicht ein neuer Runtime-Refactor.

**Tests:** Startgeneration 0/MAX/MAX-1 und nahe reserviertem Limit; verschiedene Generationsarten; Revocation am Limit, mehrfach und nebenlaeufig; alter `StartupAuthorization` muss nach Revocation `!is_current` ergeben oder der **gesamte** Runtime-Release/Egress irreversibel fail-closed sein. Konkurrenz von invalidation und `ValidatedStartup::arm` darf keinen Fence aus veralteter Autorisierung minten; keine false PASS in einem Panic-Thread.

### E5-4 — Startup und BuyPolicy widersprechen sich bei Slippage 10000 bps

**Code:** `execution/src/startup.rs::validate_economics` Z. 614–622: `e.slippage_bps > 10000`. Dagegen `trading/src/economics/policy.rs::BuyPolicy::new` Z. 55–81 verweigert `slippage_bps >= BPS_DENOMINATOR (=10000)`. Es handelt sich um einen **fail-closed aber zu spaeten** Startup-Abbruch; bis zur spaeteren Policy-Bildung koennte scheinbar valide Konfiguration angelegt werden.

**Fix:** Startup ebenfalls `e.slippage_bps >= 10000`; vorzugsweise gegen das exakt gleiche freigegebene `BPS_DENOMINATOR`-Wertmodell statt eine neue widerspruechliche Konstante. Keine Ausgabe null-Minimum, keine Aufweitung von zulaessigen Slippage-Profilen. Tests bei `9999` erlaubt soweit die restliche Policy stimmt, `10000` und `10001` STOP; `BuyPolicy::new` und `validate_startup` identische Grenzentscheidung. Nicht schon bei normalem Eintritt einer Fehlkonfiguration die Live-Armierung erlauben.

### E5-5 — Ein fehlender Adapter nach Admission haelt die ganze Lane

**Code:** `execution/src/submission/send.rs::run` ca. Z. 679–832. Nur **vor** der wirtschaftlichen Admission ist `!any_ready`/abgelaufene Initialdeadline ein vorbereiteter, nicht gehaltener SKIP. Sobald `lane.begin_send` Permits ausgegeben hat, bleibt nach der Fanout-Schleife mit `stopped || unstarted>0 || !started` ein `HaltReason::External` zurueck. Bei einem Teil-Fanout mit einem HandOff und einem Stale-/Deadline-SKIP werden die anderen Permits unterdrueckt, kein zweites wirtschaftliches Send und kein neuer BUY erlaubt. `execution/tests/dispatch.rs` hat **gezielte Regressionstests**: `a_late_pre_permit_check_after_another_adapter_was_sent_leaves_no_permit`, `a_connection_that_is_not_ready_is_skipped_and_leaves_no_permit`, `a_deadline_passing_after_the_admission_stops_the_lane_without_disclosure`. Das ist **vorhandenes beabsichtigtes Safety-Design**, keine versehentliche if-Klausel.

**Entscheidung:** Jetzt **nicht** aus Verfuegbarkeitsgruenden aufweichen. Fables hypothetische Alternative, ungestartete Permits ohne Lane-HALT zu unterdruecken, darf spaeter in C2 nur nach Beweis aller wirtschaftlichen, Nonce-/Late-execution-/Dedupe-/Reorg- und restart-/journalbezogenen Invarianten erwaegt werden; insbesondere nach einem disclosed Teilversuch bleibt der gesendete BUY bis zur Chain-Reconciliation potentiell wirksam. Keine neuen Intents/NONCEs freigeben, nur weil ein zweiter Builder nicht Ready war. Wahrheitsgemaess als Tradeoff `AVAILABILITY_REDUCED_BY_SAFETY_GATE` dokumentieren. **Kein Patch in Gate 0.**

### E5-6 — `maintain()` verbindet synchron; Wartung darf HOT0 niemals blockieren

**Code:** `execution/src/submission/tls/transport.rs::state` ca. Z. 145–161 setzt `Stale` bei `last_activity.elapsed()>max_idle`, pending Ciphertext oder anderer Unfertigkeit. `execution/src/submission/tls/owner.rs::maintain` ca. Z. 119–139 ruft bei Stale/Disconnected/NotReady synchron `connect()`; `WarmTlsConnection::establish` (`tls/connection.rs`) kann synchron DNS/TCP/Handshake ausfuehren, begrenzt durch jeweilige Policy-Timeouts. `owner.rs::connect` dokumentiert: **„never call it while a dispatch could use the transport“**. `execution/tests/tls.rs::an_idle_connection_is_not_eligible` bestaetigt als Offline-Testfall den `Stale`-Zustand und `maintain`-Ersatz; andere Tests pruefen Reconnect nach fehlerhafter Antwort und Inflight/Close.

**Bewertung:** Blockierender Maintenance-Pfad steht fest. **Nicht** nachgewiesen sind die im Fable-Text genannten „staendig neu“ aufgebauten Verbindungen, typische konkrete Provider-Idle-Timeouts oder **100–500 ms** pro Reconnect; das ist abhaengig von Provider/Netz/TLS-Resumption und wurde hier nicht gemessen. Aus diesem Modul folgt auch nicht, dass der **spaetere Runner** `maintain` faelschlich im Entscheidungsthread aufrufen wird – diese Integration ist noch ausstehend. Kein unqualifizierter Keepalive-HTTP-Request als vermeintlich kostenloses Heilmittel: er koennte Provider-Quoten, Auth, Reply-Correlations und den genau-eine-Anfrage-pro-Verbindung-Vertrag verletzen.

**Verbindliche C2/C3-Anforderung:** TLS-Connect/DNS/Close-/Response-Wartung auf ausdruecklichen **nicht-HOT0-Owner** legen, exklusiven Transport-Handoff/Lebenszyklus beweisen und warmen Eligibility-Snapshot ohne Queue-/Lock-/Netzwerkwartezeit am Dispatch nutzen. Vor `T0` verbinden und vorwaermen, `Stale` ist reiner Fail-closed-Skip statt blockierender Reconnect **im** BUY. Reconnect-Haeufigkeit/-Dauer/Idle-P99, misslungene Reconnects, Prozent Dispatch mit `Ready`, blinde Intervalle und `T_signal -> T_send` auf Zielhardware messen. Auf Service-Realitaet abgestimmter Keepalive nur nach offiziellem API-/TLS- und Timing-Nachweis; kein nicht-adressierter Extra-Request auf in-flight Verbindung. **Kein Patch jetzt.**

### E4b-4 — Fence-Aufrufer nochmals bestaetigt

Aus dem bereits abgeschlossenen eigenstaendigen Scan aller 32 `execution/src/**/*.rs`-Quellen: **einziger** `ArmedFence::from_durable_startup`-Aufruf in `startup.rs::ValidatedStartup::arm` ca. Z. 281–293 und **nach** `durable_fence` mit `create_new`, Datei-/Directory-`sync_all`. Kein aktueller Konstruktor-Benutzer in G0.7-, Probe-/Readiness-Entrypoints. Featureweite API-Erreichbarkeit (`runner-startup` global) bleibt ein spaeter pruefbarer Modularisierungsaspekt, nicht neuer akuter Bypass. Kein Doppel-Ticket.

### Umsetzungsscope, priorisierte Tests und Release-Grenzen

**Innerhalb der bestehenden Phase und ohne Ausweitung des Gate-0-Codes:** E5-2 (ASCII-Hex-/Secret-Panic), E5-3 (monotone Invalidation mit garantiertem globalem fail-closed Verhalten), E5-4 (Slippage-Boundary) koennen vom Coding-Agent als **gezielte Startup-Codehaertung** in `startup.rs` mit minimalen Tests bearbeitet werden. Sie sind **keine** Blocker des unveraenderten G0.7-Binaries und duerfen nicht als Grund fuer ein neues R0-/DRY-Fenster ausgegeben werden. Wenn eine breitere Revision den eingefrorenen Source-/Probe-Scope unnoetig invalidieren wuerde, nachweislich qualifikationsneutral isolieren oder gesonderten Auftrag abwarten.

**Vor einem spaeteren produktiven Mehrfach-BUY zwingend:** E5-1 kumulatives Risk-Budget; E5-5/6 Safety-/Availability-/latency-taugliches Runner-Eigentum. Die produktive Release-Kette ist heute wegen qualifikationsloser Capability-Records und alter v5.24.0-Deployment-Profile absichtlich BLOCKED (E4b-1); weder Fable-Review noch Startup-Fixes heben diese Sperre auf.

**Kommandos fuer Coding-Agent (hier nicht gelaufen):**

~~~bash
cd bsc/engine
cargo test --locked -p darkbot-bsc-execution --lib startup
cargo test --locked -p darkbot-bsc-execution --test dispatch
cargo test --locked -p darkbot-bsc-execution --test tls
cargo test --locked -p darkbot-bsc-execution --test hotpath_alloc
cargo test --locked -p darkbot-bsc-execution --test release
cargo fmt --all -- --check
~~~

Gezielte **neue** Tests fuer E5-2 (Unicode/InvalidHex/Panic-Freiheit), E5-3 (MAX/nearMAX + Revocation/Thread) und E5-4 (9999/10000/10001) in `startup.rs` direkt anlegen. Berichte `PASS` nur bei tatsaechlich ausgefuehrten Tests samt Commit-/Toolchain-/Feature-Scope; vorhandene Testnamen liefern keine neue Verifikationsmessung. Keine G0.7-Arming-Aktion, keine Freigabe, kein Echtgeld und kein automatisches Runner-C2-Refactoring.

**Etappe-5-Abnahme:** E5-1 als spaeteres LIVE-Blocking-Risk-Gate registriert, E5-2/3/4 als implementierbare Fixes spezifiziert, E5-5/6 als spaeter messpflichtige Architektur-Tradeoffs statt angebliche aktuelle Mainnet-Bugs klassifiziert; `ArmedFence`-Herkunft unveraendert. `STAGE_5_REVIEWED`, **FIXES OPEN**, G0.7 `UNARMED`, C1/C2 noch `PAUSED`.


## 7. Etappe 6 — geth_ipc Ingress/Outbound, HOT0 und SPSC-Wire-V2

**Pruefstand:** 2026-10-20 · **Fable-Referenz:** `cac786e` · **unabhaengig gepruefter Git-`main`-HEAD:** `6e0e5876f86308f166b609deacc325842ad10eef`. Die in dieser Etappe bewerteten BSC-Codepfade wurden seit `cac786e` nicht im aktiven Review geaendert. **Kein Produktionscode editiert, kein Cargo-/Go-Test ausgefuehrt, keine Mainnet-Transaktion ausgelöst.**

**Autoritative Phase:** `bsc/docs/PHASENPLAN.md` -> `BSC_DUAL_VENUE_ROADMAP.md`, `GATE0_TESTPLAN_SHORT.md`, `BSC_PRODUCTION_BUILD_MASTER_PLAN.md`, `BSC_C1_PARALLEL_FAST_LANE_PLAN.md` und der `BSC_C1_5_TO_C1_10_QUALIFICATION_RUNBOOK.md`. **C1/C2 erst nach G0.7→G0.8**; ein historischer lokaler C1.4/C1.5-Test-PASS ist keine gesamte Mainnet-Coverage, keine reale BUY-Inclusion und keine <1-ms-Produzentenmessung. Fast Lane darf keine zweite wirtschaftliche Admission-/Nonce-/Final-Binding-/Egress-Autoritaet etablieren. Die betroffene Produktionsschnittstelle ist in der spaeteren C1/C2/C3-Integration zu qualifizieren.

**Eigene Code-Abdeckung:** `geth_ipc/src/{ingress,outbound,hot0}.rs`, `protocol_v2/{producer,consumer,guards,mapping,mod}.rs`, die gezielten `protocol_v2/{c15_tests,tests}.rs`-Tests und Dokumentationsstellen des C1-Plans/Runbooks; genauer Quell- und Testbestand erhoben. **Offen:** Vollpruefung von `host.rs`, `protocol_v1.rs`, `protocol_v2/{arena,lifecycle,peer_pool,role_lease,layout,lanes,error}.rs` und dem Rest der grossen HOT0-/V2-/Stress-Testfaelle; Go-Overlay-Offset-Pruefung ist Etappe 7. Etappe 6 ist daher ein **teilweiser, klar abgegrenzter Audit**, keine behauptete Komplettverifikation des gesamten Crates.

### Etappe-6-Urteil

| ID | Klassifikation | Quellcode-Urteil | Implementierung |
| --- | --- | --- | --- |
| E6-1 | **Produktrelevant/C1-C2 Coverage** | **BESTAETIGT:** Scope nur direkter Portal-v6-Selector; Audit verwendet denselben Scope und kann ausserhalb davon keine False Negatives feststellen | **C1/C2 Pflicht zur produktweiten Coverage-Definition und unabhaengiger Denominator-Evidence**, keine G0.7-Aenderung |
| E6-2 | **Produktrelevant/C1-C2 Safety-Integration** | **BESTAETIGT:** `publish_buy` akzeptiert `hot0::IntentSlot::Sealed` ohne `trading::IntentLane::start_write`-Permit; Integrationsgrenze noch nicht zusammengefuehrt | **Vor jeder produktiven LocalGeth-Egress-Qualifikation zwingend vereinheitlichen** |
| E6-3 | **C1.5/7 Protokoll-Haertung** | **TEILWEISE BESTAETIGT:** fehlende Barrieren bei freiwilliger Rewrite-Diagnostik; **die von Fable vorgeschlagenen zwei Fences sind nicht als korrekte/ausreichende Loesung bewiesen** | Go/Rust-Speichermodell-/Layout-/Race-Pruefung in Etappe 7, keine unqualifizierte Wire-Aenderung |
| E6-POS | Vorhandene SPSC-Sicherheit | **QUELLCODE-KONSISTENT** im zugesicherten ein-Producer/ein-Consumer-/Guard-Lebenszeit-Vertrag; Testbelege vorhanden | Kein unnoetiger Rework |
| E6-REST | Offene Dateien / Interop | **NICHT VOLL GEPRUEFT** | Gezielter Folgeaudit C1, kein umfassender PASS |

### E6-1 — Getrennte Begriffe fuer engen Prefilter-Audit und echte Launch-Abdeckung

**Beleg:** `bsc/engine/crates/geth_ipc/src/ingress.rs` ca. Z. 226–294 definiert `C14_FLAP_SCOPE` mit genau **Portal-Proxy** `0xe2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0` und **`newTokenV6`**-Selector `0x8cb5772c`. `PrefilterScopeV2::matches` verlangt `tx.to=Portal` und passenden ersten Selector. `IngressAuditV2::judge_txpool_record` ca. Z. 602–673 dekodiert die Txpool-Huelle, fuehrt denselben `scope.matches`-Test aus und liefert fuer alles ausserhalb **`NotRelevant`**. Die in `IngressAuditStatsV2` ermittelten `relevant/false_negatives` beziehen sich daher auf **diesen** eingefrorenen direkten Caller/Selector-Scope. Die `txpool`-Bestaetigung kann nur einen Peer-Miss fuer eine als relevant klassifizierte TX finden. `C14_SCOPE_CANONICAL_HEX` und CRC32C-Fingerprint binden genau diesen Narrow-Scope an die Go-/Core-Schnittstelle.

**Tragweite / keine ueberdehnte Behauptung:** Erzeugungen via Creator-/Router-/Multicall-/Proxy-Vertrag mit anderem `to` oder abweichende Methodenselektoren werden **von diesem Filter nicht als Launch erfasst**. Ob das gepinnte Portal v5.25.0 solche mittelbaren oder anderen Launch-Semantiken auf Mainnet tatsaechlich erlaubt und wie oft sie auftreten, ist hier **NICHT NACHGEWIESEN**; Beispielklassen sind potenzielle Coverage-Luecken, keine gemessenen Verluste. `C1.4 PASS` kann nur `C1.4-SCOPE_PASS` sein, **nicht** „alle Flap-Launches erfasst“. Dabei gilt der C1-Plan §7: Der Go-Prefilter ist lediglich ein konservatives **Superset der qualifizierten** Venue-Rollen und selbst keine wirtschaftliche Authority. Eine fachlich nicht unterstuetzte Launch-Art darf nicht automatisch als handelbar aktiviert werden.

**C1/C2-Pflichtauftrag:**
1. Formal zwei Grundgesamtheiten versionieren: **(A) qualifizierte direkte `newTokenV6`-TXs, die in der Txpool-/Peer-Beobachtung des verfuegbaren Netzs auftauchen**, und **(B) alle tatsaechlich im beobachteten kanonischen Mainnet-Blockfenster vom bezeugten Portal erzeugten Tokens/Launches**. Ein C1.4-Scoped-Txpool-Audit ist nur die A-Negativquote, nicht die B-Abdeckung.
2. Fuer B eine **vom C1.4-Prefilter unabhaengige**, read-only Block-/Receipt-/Log-basierte Ground Truth aus dem bezeugten Portal-`TokenCreated`-Event aufbauen. Event-ABI, Adresse, Codegeneration, Receipt/Blockhash und kanonische Finalitaet aus gepinntem Flap-Code pruefen; `TokenCreated` nicht aus einem nicht verifizierten Event-Namen oder frei erfundenen Topic0 ableiten. Event -> tatsaechliche erzeugende Parent-TX, Ursprung-`to`, Selector und vermittelnde Contract-Calls aufloesen, dann gegen **fruehe** Peer-/TXPOOL-Beobachtung abgleichen.
3. Explicit `scope_outside_count`, `direct_in_scope_count`, `event_backed_launches`, `early_observed_launches`, `missed_launches`, `unknown_parent` sowie Gaps/Clock-/Archiv-/Traces-Fehler mit nachpruefbarer Bezugsmenge, Fenster, Dekoderversion und Coverage-Denominator reporten. Keine pauschal zusammengeworfenen Prozente fuer alle Launches. Begrenzungen bei historischen Mempool-Daten und unvollstaendiger Peer-Anbindung gelten als UNKNOWN/INCOMPLETE.
4. **Wichtige Grenze:** Ein `TokenCreated`-Event kann ausschliesslich **in inkludierten erfolgreichen Receipts** vorkommen. Es zaehlt weder private nie inkludierte Launches noch vorzeitig verworfene oder noch ausstehende TXs. Das Event ist deshalb eine kanonische **realisierte Launch-Ground-Truth**, aber **kein kompletter Nenner aller moeglichen Pending-Launches**. Keine falsche Aussage „nur so wird jeder Prefilter-Miss messbar“. Gleichzeitig ist eine erfolgreiche Event-Erkennung **kein** Nachweis, dass die eigene Fast Lane den Launch *vor* Konkurrenz gesehen hat.
5. Nur bei durch reale Mainnet-Fixtures nachgewiesenem Fehl-Scope und nach eigener wirtschaftlicher Profilqualifikation alternative Caller-/Selector-Faelle gezielt dem Go/Rust-Filter hinzufuegen. `C14_SCOPE_CANONICAL_HEX`, CRC32C, Go-/Rust-/Core-Goldens, falsche Matches, C1.4a-Feasibility und Cross-Lane-Coverage erneut beurteilen. Keine neue `ROLE_LAUNCH`-Autoritaet ohne Code-/Profil-Evidence.

**Abnahme:** Nicht nur „innerhalb des Filters 0 False Negatives“, sondern explizite Denominator-/Scope-Angaben und beweisbare Differenz zu den bezeugten tatsaechlichen Launches. Ohne Mainnet-Sample `COVERAGE_UNKNOWN`, nicht Marketing-PASS. Keine G0.7-Neuqualifikation fuer einen noch getrennten C1-Filter.

### E6-2 — HOT0-Egress modelliert einen zweiten signierten-Bytes-Ausgang

**Codebeleg:** `geth_ipc/src/outbound.rs::publish_buy` ca. Z. 120–142 hat den oeffentlichen Aufruf `(&mut ProducerV2, &IntentSlot, commit_ns)`, verlangt lediglich `slot.state()==IntentState::Sealed` und monotone Clock ungleich 0; schreibt `slot.buy_bytes()` mit dem vom Slot gelieferten `slot.buy_hash()` in `RecordKind::OutboundSignedTx`. `geth_ipc/src/hot0.rs::IntentSlot::seal` ca. Z. 239–267 akzeptiert ein `IntentBuy` mit separatem `intent_id/generation/policy_generation/nonce/adapter_permit_mask/buy_hash/signed_buy`, prueft an der Stelle aber nur State, Nichtleere und Kapazitaet, **nicht** ein verifiziert verbrauchtes Adapter-Permit und auch nicht eigenstaendig die Cryptographic-Hash-zu-RLP-Byte-Bindung. Dies ist eine vorallokierte C1.5-Egress-Speicherstruktur, kein Produktions-`trading::intent`-Owner.

Demgegenueber verbietet `trading/src/intent/mod.rs` Invarianten 4–8 das Entnehmen signierter Bytes ausser ueber ein **einmaliges `IntentLane::start_write`** nach `begin_send`, mit einem Nonce/Intent, Snapshot und fail-closed UNKNOWN/HALT bei jedem Egress-Zweifel. `execution/src/submission/send.rs` konsumiert tatsaechlich ein Lane-Permit pro Adapter. `geth_ipc/Cargo.toml` hat derzeit nur `libc` als Abhaengigkeit und `hot0` ist bewusst eine vom Trading-Core getrennte C1-Infrastruktur. **Aktuelle externe Rust-API laesst einen anderen Egress-Vertrag zu, wenn man sie spaeter ohne Bridge im Runner verdrahtet.**

**Urteil:** Produktrelevante **Architektur-/Integrationsluecke**, aber **kein belegter aktuell ausfuehrender Bypass**, denn `execution/src/startup.rs` laesst `LocalGeth` im bestehenden Release-Startup **nicht** als auswaehlbaren Adapter zu und der spaetere vereinte Runtime-Risk-/Nonce-/Egress-Owner ist noch nicht integriert. Ein lokaler P2P-Broadcast waere oeffentlich und darf nicht als privates G0.7-Bundle-Equivalent behandelt werden.

**C1/C2/C3-Pflichtfix vor produktivem Egress:**
1. **Ein** Source-of-Truth-Owner fuer Intent/Nonce/Exposure/Dedup/Signer-/BUY-Bytes. Entweder LocalGeth-Egress nur ueber eine **vom einmalig konsumierten `IntentLane::start_write` erteilte, unaustauschbare typisierte Permit-/Byte-Capability** aufrufen oder `hot0::IntentSlot` ausschliesslich als Speicher hinter demselben Lane-Permit-Owner einsetzen. Keine implizite Nonce-/Permit-Wahrheit aus `hot0::IntentBuy` uebernehmen. Unvermeidbare getrennte Datenstrukturen muessen eine **eindeutige** wirtschaftliche Identitaet und einen gemeinsamen Egress-Status besitzen.
2. Vor erstem Wire-Publish finale RLP/EIP-155-/Sender-/Chain-/Portal-/Nonce-/Gas-/Value-/Token-/Minimum-/Snapshot-Bindung erhalten; BUY-Hash aus den **tatsaechlichen signierten Bytes** checken. Fuer jeden Adapter dieselben signierten Bytes und Nonce, getrennte Wire-Frames/Handoff-Evidence aber nur **ein** wirtschaftlicher BUY. Der adapterbezogene Permit wird exakt einmal verbraucht; ein zweiter `publish_buy` mit demselben Slot darf kein zweites Wire-Publish bewirken, ausser dieselbe bewusst erlaubte Fanout-Operation des einen Intents wurde einmalig authorisiert und deren Evidence gebunden.
3. `publish_buy`-Fehlervariante `QueueFull`/nach Partial-/unbestimmtem Commit darf den wirtschaftlichen Intent **nicht** still freigeben: vor garantierter Disclosure nur qualifizierter SKIP, ab ungewisser moeglicher Offenlegung immer UNKNOWN/HALT. Auch geth-Evidence `DISPATCHED`/`HANDED_OFF` beweist nur lokalen P2P-Transport, nicht Netzempfang, Inclusion, Erwerb oder Zeitpunkt `T_buy`. Status-/Sequence-/Peer-Identity-Records versionieren.
4. C1/C2- und Runner-Tests: `Sealed` ohne Permit STOP; zweite Publikation STOP; mismatched raw/hash STOP; falscher Nonce/Signer/Token/Codegen/Policygen STOP; alle Adapter bytegleich; partial-Wire-Write UNKNOWN; queue-full mit expliziter Disclosure-Grenze; Reorg, Late Execution, Quarantäne/Crash/Restart und kumulatives E5-1-Budget. Fake-Keys / Offline-Frames; **kein** echter geth-Broadcast.

**Abnahme:** Einheitliche `IntentLane`-Invariante 5 auf **alle** lokalen und Builder-Egress-Adapter angewandt, ohne neues Nonce-/Retry-/Exposure-Loch und ohne das SPSC-Zero-Copy-Latenzziel aufzugeben. Die Typgrenze muss im Integrationscode nachgewiesen werden, nicht nur per Kommentar.

### E6-3 — Memory Ordering: Two-Fence-Vorschlag ist keine bewiesene regelwidrige Rewrite-Sicherheit

**Feste Beobachtung:** `protocol_v2/producer.rs` ca. Z. 51–79 setzt `SLOT_OFF_COMMITTED` bei Slot-Reuse mit `store(EMPTY_SEQUENCE, Ordering::Relaxed)`, schreibt Header und Payload danach als **nichtatomare Bytes in ein MAP_SHARED-mmap** und publiziert `committed_sequence` / `write_sequence` mit `Release`. `protocol_v2/consumer.rs` nimmt `write_sequence` / `committed_sequence` mit `Acquire` auf, prueft CRC/epoch/Bounds und laesst einen `RecordGuardV2` die referenzierten Bytes leihen. `protocol_v2/guards.rs::check_stable` liest `committed` spaeter mit `Acquire` erneut. **Der vorgeschlagene Code von Fable wuerde nur `fence(Ordering::Release)` hinter dem leeren Store und `fence(Ordering::Acquire)` unmittelbar vor der abschliessenden Commit-Lesepruefung einsetzen.**

**Zwei unterschiedliche Garantien trennen:**
- **Regulaeres SPSC-Protokoll:** Der Producer darf denselben Slot waehrend eines gehaltenen `RecordGuardV2` **nicht** aendern. Er muss vor Wiederverwendung den vom Consumer mit `Release` erhoehten `read_sequence` per `Acquire` beachten. Diese Eigentumsfolge bietet die beabsichtigte **gueltige** Synchronisierung fuer plain slot data. Bestehende Tests `protocol_v2/c15_tests.rs::a_held_slot_cannot_be_reused_and_a_released_slot_can` und doctests demonstrieren SPSC- und Borrow-Regeln; `a_corrupt_record_is_refused_and_never_released` prueft eine **vor** Consumer-Acquire manipulierte Payload.
- **Regelwidriges gleichzeitiges Ueberschreiben:** Der bestehende Kommentar verspricht per `committed_sequence`-Recheck erkennbaren Rewrite, waehrend dabei **nichtatomare** Slotbytes moeglicherweise parallel gelesen/geschrieben werden. Eine solche Race-Situation liegt **ausserhalb des SPSC-Eigentumsvertrags** und ist durch nachtraegliches Checken der Commit-Word-Identitaet alleine **nicht sicher auszuschliessen**. `protocol_v2/c15_tests.rs` hat einen Test mit manuell gesetztem `EMPTY_SEQUENCE` nach dem Guard-Acquire; das testet **sichtbare Commit-Manipulation**, nicht das gleichzeitige, lockfreie Lesen/Schreiben einer Plain-Byte-Payload. CRC hilft bei finaler Integritaet, nicht gegen alle konkurrierenden Zwischenschreibungen oder eine unsound Rust-`&[u8]`-Aliasing-Situation. Nach Rust-Atomics-Dokumentation synchronisieren Fences nur unter definierten Atomic-Operationen/Happens-Before-Bedingungen. **Die zwei Fences allein beweisen keine Race-Freedom fuer den vertragsverletzenden Writer.**

**Entscheidung:** E6-3 **nicht** mit einem ungeprueften „2 Zeilen, Kosten vernachlaessigbar“ als FIXED etikettieren. Der vorgeschlagene Patch kann eine **zusaetzliche** Ordnungsgrenze im gewuenschten Kontrollfluss markieren; ob genau `Release` hinter dem Clear bzw. `Acquire` vor dem Recheck und die Go-Entsprechungen hinreichende *gegenseitige* Read-/Write-Ordnungen liefern, bedarf einer eigenen Proof-/Toolchain-/ISA-Ueberpruefung (ARM64 Apple M4 und tatsaechlicher Linux-Produktionshost). **Zwingend** den Sonderfall „Writer ignoriert Full/read_sequence und schreibt parallel in geliehene nichtatomare mmap-Bytes“ entweder mit einer staerkeren Ownership-/No-Aliasing-Garantie **strukturell unmoeglich machen**, oder separate atomare/konsistente Snapshot-Strategie fuer die Ausnahme spezifizieren. Keine behauptete unbedingte Erkennung/Memory-Safety durch einen zweiten Commit-Load. Fable-Patch 1 **NICHT BLIND ANWENDEN** und Go-Seite bei Etappe 7 verpflichtend offsets/layout/atomics abgleichen.

**C1-Folgeauftrag:** Dokumentierte Annahmen `SPSC-owner, one producer, guard lifetime, read_sequence Release, producer Acquire, no concurrent slot overwrite` explizit an Go/Rust-Grenze und Lifecycle-/Pool-Leases pinnen. Bei jeder geplanten Fence-Aenderung Rust/Go-Golden-/Backpressure-/Quarantäne- und ABI-Tests sowie einen kontrollierten Stress-/Concurrency-Test fuer ARM64 laufen lassen; falls „detect malicious concurrent rewrite“ als Produktgarantie erhalten bleibt, formalen Happens-Before-/Aliasing-Nachweis bzw. sicherer struktureller Ausschluss verlangt. Testlast und unveraenderte HOT0-P99-Allokations-/Latenzwerte dokumentieren. Keine Protokollfelder ohne gepinnte Migrations-/Kompatibilitaetsentscheidung aendern.

**Relevante externe Sprachgrundlage (keine Mainnet-Evidence):** Rust `std::sync::atomic::fence` https://doc.rust-lang.org/std/sync/atomic/fn.fence.html : Atomics-Fences synchronisieren unter definierten Atomic-Bedingungen und ersetzen nicht den Beweis fuer den Schutz nichtatomarer Daten.

### Positive Evidenz und verbleibender Scope

- `ProducerV2::publish`: Read-Sequence `Acquire`, Write-Sequence `Release`, Capacity-/Payload-Bounds, Lane-State und Shutdown-Gates. `ConsumerV2::acquire_slot`: Write-/Committed-`Acquire`, Record-CRC, Epoch und Quarantaene bei Fehler; Borrow-Guard released Read-Sequence erst nach Lifecycle-Pruefung. **Im validen, gueltig implementierten SPSC-Protokoll konsistent**; die angesprochenen Kommentare/Rechecks sind keine Lizenz zum ueberlappenden Plain-Byte-Write.
- `IngressAuditV2`: fruehester Peer-`T_ingress`, striktes Time-Domain-/Record-Contract-Checking, Peer nach Txpool-Accept zaehlt nicht als frueher Hit; `qualification` benoetigt geschlossene, geleerte und quellenkonforme Evidence. Diese Aussagen gelten **nur fuer Scope und bezeugtes Fenster**.
- `Outbound`: Evidence wird nach Sender-/Worker-/Status-/Timing-Provenienz klassifiziert, `SendErrorUnknown` ist **nie** „nicht gesendet“. Ein `T_queue` oder lokaler geth-Peer-Handoff ist **keine** On-Chain-BUY-Ausfuehrung, und der Provider-Send-Handoff darf nicht als G0.7/G0.8- oder <1-ms-Beweis benutzt werden.
- Vorliegende Offline-Tests und historische C1-REV2-Berichte duerfen als **existierende** Artefakte angegeben werden, nicht als in dieser Etappe neu ausgefuehrte PASS-Evidence. Der C1-Runbook verlangt nach betroffenen Source-Aenderungen Gate-Order und relevante Regressionsneuauswertung; keine umfassende C1-Freeze ohne die fehlenden C1.9-/Hook-/Coverage-Nachweise.

### Etappe-6-Tests und Abnahmegrenze

**Bestehende zielgerichtete Kommandos fuer spaetere C1-Implementierung (hier nicht ausgefuehrt):**

~~~bash
cd bsc/engine
cargo test --locked --offline -p darkbot-bsc-geth-ipc
cargo test --locked --offline -p darkbot-bsc-geth-ipc --test c15_hot0_alloc
cargo test --locked --offline -p darkbot-bsc-geth-ipc --test c18_stress
cargo test --locked --offline -p darkbot-bsc-geth-ipc --release
cargo fmt --all -- --check
~~~

**Zusatz vor C1/C2-Inclusion:** echte Scope-/Ground-Truth-Fixtures mit Block-/Event-Gegenbeweis, intern nicht verfaelschbare typed-permit-to-egress-Testcases und Go/Rust-Speicherordnungs-/Aliasing-Review mit kontrollierter Laufzeit-/Stress-Evidence. Kein synthetischer Record ist ein Ersatz fuer echte Mainnet-Coverage; kein erfolgreicher local-geth-Handoff steht fuer ein vom Producer innerhalb <1 ms ausgefuehrtes Token-BUY.

**Etappe-6-Status:** **REVIEWED (PARTIAL CODE SCOPE) / E6-1 & E6-2 FUTURE INTEGRATION GATES / E6-3 REQUIRES MEMORY-MODEL PROOF**, Produktions-Fixes offen, G0.7-Code-Ready davon **nicht** blockiert. G0.7 `UNARMED`, G0.8 `NOT_EXECUTED`, C1/C2 `PAUSED`. Etappen **7** (Go↔Rust Offset-/ABI-/Atomics-Paritaet) und **8** (Journal/Record/Legacy) noch offen.


## 8. Etappe 7 — Go-Overlay gegen Rust, Teil 1: Txpool-Coverage, Chain-Hooks, Wire-Layout, Unsynced-Gate

**Review:** 2026-10-20, **Fable-Basis:** `cac786e`, **unabhaengig gepruefter HEAD:** `main` `df52f8f423e42b48e7f348d0a56543bc90ea32ac`. Kein Quellcode editiert, keine Go-/Rust-/Mainnet-Tests ausgefuehrt. Beibehaltung der bindenden C1.1–C1.10-Gate-Reihenfolge und der G0.7→G0.8-Vorrangstellung; C1/C2 weiter pausiert. **Etappe 7 ist hier ausschliesslich Teil 1 / PARTIAL REVIEW**, kein abschliessender Go-/Rust-Parallelitaetsbeweis.

**Neu aufgeklaerte Files, die Fable wegen Dateinamen-404 nicht lesen konnte:** Das vollstaendige Overlayverzeichnis `bsc/geth/overlay/darkbot/fastlane/` enthaelt die 15 Dateien
`chain_v2.go`, `chain_v2_test.go`, `fastlane.go`, `fastlane_test.go`, `ingress.go`, `ingress_fixtures_test.go`, `ingress_test.go`, `outbound.go`, `outbound_test.go`, `qos.go`, `qos_darwin.go`, `qos_fallback.go`, `stress_test.go`, `v2.go` und `v2_test.go`. **`v2.go`** (1530 Zeilen) implementiert `arenaV2`, `laneV2`, `publishChecked`, `tryConsume`, Pool-/Lifecycle-/Role-Lease- und Wire-V2-Mapping-Funktionen; **`ingress.go`** (1013 Zeilen) implementiert `IngressV2` und die unsynchronisierte Txpool-Freigabe. Keine weitere Dateiliste oder Nutzer-Terminalhilfe fuer Teil 2 erforderlich.

### Etappe-7-Teil-1-Urteil

| ID | Status nach unabhaengiger Pruefung | Fix-/Gate-Konsequenz |
| --- | --- | --- |
| E7-1 | **BESTAETIGTE C1.4-QUALIFIKATIONSLUECKE:** Txpool schreibt full raw TX in 3840-Byte-Payload und zaehlt Oversize/Full auch ausserhalb C1.4-Launch-Scope; **„Mainnet kann praktisch nie PASS“ ist ohne Messung NICHT BELEGT** | Coverage-/Denominator-/Backpressure-Redesign erst C1, keinesfalls unbesehen Fehlzaehler ignorieren |
| E7-2 | **TEILWEISE BESTAETIGT:** Overlay-Patch instrumentiert zwei Canonical- und einen Reorg-Hook; andere Head-Mutationen im vollstaendigen gepinnten geth-Build NICHT gegen den realen Source-Baum bewiesen | C2 Chain-Owner muss Gap-/Resync-Semantik qualifizieren; reine Parent-Hash-Ungleichheit kein vollstaendiger Beweis |
| E7-3 | **WIDERLEGT ALS AKTUELLER BUG:** `C19bUnsyncedAcceptTxsAllowed(56)` verweigert explizit jede unsynced-Freigabe mit gesetztem Arm; bestehende Negativtests | **Kein Patch**. Bestehende Gate-Checks und Tests behalten; keine neue Mainnet-Option |
| E7-4 | **BESTAETIGT, DOC-ONLY:** veralteter `bsc/native/tools/c14_ingress_interop.sh`-Pfad | Referenzen zu tatsaechlich vorhandener `bsc/tools/c14_ingress_interop.sh` korrigieren |
| E7-WIRE-OFFSETS | **BESTAETIGTE POSITIVE PARITAET:** 50 direkt abgeglichene Layoutgroessen/Offsets Go `v2.go` und Rust `protocol_v2/layout.rs` identisch | Keine Layout-Aenderung; restliche SC-Atomics-/Epoch-/Ownership-Paritaet in Teil 2 separat |
| E6-3-FOLLOWUP | **OFFEN:** beide Sprachen setzen `committed` vor Plain-Byte-Schreiben; Rust `Relaxed`, Go `sync/atomic` SC; keine bewiesene atomare Rewrite-/Aliasing-Sicherheit | Kein „2 Fences => FIXED“, Interop- und Memory-Model-Nachweis in Teil 2 |

### E7-1 — Txpool akzeptiert laengere Transaktionen, die Audit-Evidence-Lane jedoch nicht

**Harter Quellbefund:** `bsc/geth/overlay/darkbot/fastlane/ingress.go` ca. Z. 60–63 legt `c14TxpoolLaneSlotStride=4096`, `c14TxpoolLaneCapacity=4096` fest. `v2.go` verwendet `v2SlotHeaderSize=256`, also Nutzlast **maximal 3840 Bytes**. `ingress.go::PublishTxpoolAcceptedV2` / `publishTxpool` ca. Z. 852–891 ruft fuer **jede** vom lokalen Txpool beobachtete TX `serializeTxV2(tx)` auf und uebergibt ihre vollen signierten RLP-/Typed-Bytes an `txpool.publish`. Ausserhalb des Filter-Scopes stehende, zu lange TXs erzeugen `v2LHOversize` und `txpoolRefused`; wenn der Consumer bei Bursts zurueckliegt, kann `v2LHDroppedFull` entstehen. Der Go-Record-Code hat keinen separaten vorab klassifizierten In-Scope-only-Handoff.

**Rust-Qualification:** `bsc/engine/crates/geth_ipc/src/ingress.rs::IngressAuditV2::qualification` ca. Z. 708–779 verlangt neben closed/drained/bound und null False-Negatives **auch** `p.refused==0`, `lane.dropped_full==0`, `lane.oversize==0` und `published==write_sequence==txpool_judged`. Eintretender Oversize **irgendeiner** TX des Fensters ergibt `Incomplete` selbst wenn `judge_txpool_record` fuer sie bei erhaltener Payload `NotRelevant` liefern wuerde. `ingress.rs::txpool_lane_drops_and_oversize_refusals_never_pass` prueft diese Branches historisch offline. Damit ist die Belastung durch out-of-scope-Records **als Mechanismus nachgewiesen**.

**Scharfe Einschraenkung:** Weder eine Mainnet-Lastmessung noch eine echte Groessenverteilung/Zeitfensterstatistik liegen dieser Etappe bei; Fables „praktisch nie PASS“ ist eine **Vermutung**, nicht ein belegtes Mainnet-Resultat. Blob-Transaktionen mit Sidecars duerfen nicht unbesehen mit uebertragenen Txpool-RLP-Laengen gleichgesetzt werden; massgeblich sind tatsaechliche `serializeTxV2`-Bytes. Vor einer Aenderung Bandbreite, Ingress-Rate, Burst, Consumer-Drain, Slot-Full- und Oversize-Denominator auf exakt gepinnter BSC-Mainnet-Quelle messen. Keine fiktive Pass-Quote.

**C1-Designauftrag, nicht sofort in G0.7 patchen:**
1. **A) Vollstaendiger TX-Pfad** und **B) zweckgebundener, verlustlos klassifizierbarer Kurzrecord-Pfad** explizit vergleichen, bevor `Incomplete`-Gates gelockert werden. Eine full-record-Lane mit groesserem Slotstride oder dimensioniertem Backpressure-Budget kann den bisherigen Evidenzvertrag erhalten, erhoeht aber RAM/Cache-/Headroom-Kosten. `max_stride` ist in `v2.go` begrenzt; neue Werte nur nach gemessener Dimensionierung.
2. Kurzrecord (Hash, original TX-Laenge, canonical `to`, Selector, tx type, Clock-/Source-/Status-Identity) **nicht** in einen angeblich vollstaendigen `TXPOOL_ACCEPTED`-Record des bestehenden ABI schmuggeln. Neuer versionierter RecordKind/Semantik oder separates Qualifikationsformat, mit explizitem `FULL` vs `SUMMARY`-Provenance-Flag, Source-Sequence-, Korrelation-/Hash- und Fehler-/Unknown-Klassen; Go und Rust gemeinsam; jede Entscheidung muss fuehrende Vertragstypen 0–4 und no-`to`-Creation korrekt behandeln. Fuer nicht parsebare vollstaendige TXs und Fehler beim Kurzrecord-Decoder bleibt `INCOMPLETE`, nicht `NotRelevant`.
3. Alternativ `oversize` **nur im unzweideutig aus voller/korrekter Original-Envelope extrahierten Out-of-Scope-Fall** aus der engeren Launch-Audit-Denominator herausnehmen, aber **zusätzliche** Roh-Txpool-/Capture-Loss-Metrik fuer alle TXs erhalten. Niemals der gesamten Txpool-Lane einen pauschalen PASS attestieren, wenn full-record-Daten fehlen. In-Scope-Refusals, Provider-/Clock-/Serialize-Gaps, Consumer-Stall oder unbekannte Scope-Klassifikation bleiben hart fail-closed.
4. Regressionen: >3840 Byte out-of-scope, >3840 Byte Portal/NewToken in-scope, 3840/3841-Grenze, NULL `to`/ContractCreation, Router-/Multicall-Faelle, Typed-TX mit Blob-Sidecar (korrekte wire vs sidecar Abgrenzung), hohes Aufkommen/Full-Lane/Backpressure, NaN-/missing Source-/Clock-Fields, closed/drained-/Gap-Accounting, hash-/selector-Manipulation. Historische Scope-Golden-Bytes und C1.4a/C1.9-Hotpath-Timing neu qualifizieren falls Wire-Record geaendert wird.

**Abnahme:** Ein PASS ist stets an **exakt benannte** in-scope-Semantik und vollstaendige Coverage-/Loss-Denominator gebunden. Out-of-scope-Last darf keinen nachgewiesenen In-scope-Hit verschweigen; keine per Counter-Inhibition erfundene Mainnet-Qualifikation.

### E7-2 — Chain-Evidence-Hooks sind enger als die moeglichen Head-Zustandsaenderungen

**Quellbefund:** `bsc/tools/c1_apply_geth_overlay.py` ca. Z. 360–407 setzt `PublishCanonicalV2` in `BlockChain.writeBlockAndSetHead` sowie `BlockChain.SetCanonical`, und `PublishReorgV2` in `BlockChain.reorg`. Der statische Verifier ca. Z. 526–585 verlangt im gegenwaertigen Scope genau diese Hook-Callsites. **Andere** moegliche Head-Mutationen wie `SetHead` oder weitere Sync-/Reset-Pfade sind in dieser ausgerollten Overlay-Patchdatei nicht explizit instrumentiert. Fable hat selbst nicht den vollstaendigen, gepinnten BSC-geth-Quellbaum dieser Methoden gelesen. Deshalb ist eine **moegliche Observability-Gap** belegt, jedoch **nicht** die tatsaechliche Ausfuehrbarkeit einer von diesen Callpaths unabgedeckten Head-Mutation im gepinnten Build.

**Bereits implementierte Chain-Format-Hilfe:** `bsc/geth/overlay/darkbot/fastlane/chain_v2.go::PublishCanonicalV2` ca. Z. 150–162 schreibt `header.ParentHash` in `aux` und den neuen Headerhash in `hash`, Blockhoehe und TX-Anzahl in `payload`. `PublishReorgV2` publiziert Old-/New-/Common-Ancestor-Identity. Fables Vorschlag „Parent-Hash in CANONICAL aufnehmen“ ist damit **schon umgesetzt**. **Parent != vorheriger Head** ist **ein Indiz**, kein universell exakter GAP-Detektor: Bei normalem Sprung ueber mehrere Bloecke, gepufferter/ausgelassener Chain-Evidence, Restart, unterschiedlichen Source-/Clock-Scopes oder einem uninstrumentierten Rewind kann er false positives oder unter Umstaenden false negatives liefern. Nicht einfach `GAP` zaehlen und gleichzeitig einen erdachten dazwischenliegenden Blockpfad akzeptieren.

**C2-Vertragsfix, spaeter:** Gegen den **tatsaechlich gepinnten** geth-BSC-Quellstand jede durch External API, Startup-Recovery, Debug-`SetHead`, Header-/Snap-Sync, Canonical-/Rollback- und Reorg-Transition erreichbare Head-Mutation statisch inventarisieren und ihre V2-Evidenz-Kausalitaet nachweisen; ggf. zentralen, synchron unter `chainmu` oder anderem nachgewiesenem Owner gepinnten Hook nachrüsten, ohne doppelte/unerlaubte Producer. C2 Rust-single-state/reconciler muss Blockhash/Parent/Height/Sequence/Fork- und Canonical-Evidence **kontinuierlich** validieren; bei echten unbekannten Diskontinuitaeten `CHAIN_STATE_GAP / INCOMPLETE / HALT`, unabhaengige kanonische Read-only Reconciliation vor irgendeinem RESUME. Eine Luecke nicht durch nur positive Sequenz-Arithmetik als geschlossen erklaeren. Regressionen mit `SetHead`-Rewind, Reorg, Startup-Repair, Snap-Sync, Kopf-Spruengen und Chain-Lane-Publisher-Drops. Keine zweite Canonical-State-Authority in C1/HOT0.

### E7-3 — C1.9b-Unsynced-Override ist schon mainnet-fail-closed

**Vollstaendiger Quellbeleg:** `bsc/geth/overlay/darkbot/fastlane/ingress.go::C19bUnsyncedAcceptTxsAllowed` ca. Z. 209–237. Unset ENV => `false,nil`; jedes andere Arm-Token ausser `YES` => **Fehler**; `YES` nur mit `DARKBOT_FASTLANE_V2_SCOPE=c19b-testnet`, dann **nur mit `networkID==97`** => `true,nil`. Bei `networkID=56` ist dies stets `false,error` statt unsynced-Zulassung. `bsc/tools/c1_apply_geth_overlay.py::patch_c19b_testnet_accept_txs` ca. Z. 413–457 fuegt in `eth/handler.go` eine fruehe `if err != nil { return nil, fmt.Errorf(...) }`-Sperre vor `h.acceptTxs.Store(true)` ein. `ingress_test.go` prueft `mainnet_scope_with_arm_fails_closed` und `testnet_scope_on_wrong_network_fails_closed`.

**Entscheidung:** Keine Produktion-Sicherheitsluecke aus Fables Nachfrage; E7-3 **WIDERLEGT ALS AKTUELLER DEFECT**. Die Mainnet-ID-56-Sperre darf **nicht** durch reine ENV-Scope-Abfrage oder Startup-Fallback entfallen. Tests und Original-Overlay-Check weiter verpflichtend. Kein neues Ticket zum Umschreiben eines bereits korrekten Guards.

### E7-4 — Veraltete Pfadangabe im Go-/Rust-Interop

**Quellen:** `bsc/engine/crates/geth_ipc/src/ingress.rs` ca. Z. 1816 und Z. 1899/1989 verweist in Kommentar bzw. `#[ignore]`-Testlabeln auf `bsc/native/tools/c14_ingress_interop.sh`. `bsc/geth/overlay/darkbot/fastlane/ingress_test.go` ca. Z. 938/952 wiederholt dies. Tatsaechlich versioniert: **`bsc/tools/c14_ingress_interop.sh`**. Das ist Doku-/Testlabel-Drift ohne behauptete Aenderung der eigentlichen Datenbits.

**Fix:** ausschliesslich Kommentare und Test-`ignore/Skip`-Messages auf den realen Skriptpfad aktualisieren; keine Dateien umziehen, keine Wrapper-Duplikate, kein neues `bsc/native/tools`-Verzeichnis erfinden. Enger grep-/Test-Label-Check. Kein Wire-/Egress-Codepatch.

### Erstes echtes Go↔Rust-Offset-Audit (gegenueber Fables nicht aufgeloestem 404)

**Durchgefuehrt:** `bsc/geth/overlay/darkbot/fastlane/v2.go` Konstanten und `bsc/engine/crates/geth_ipc/src/protocol_v2/layout.rs` offsetweise gelesen und mit 50 namentlich gemappten Werten **automatisch verglichen**. Ergebnis: **50/50 exakt gleich, 0 Abweichungen**. Darin enthalten:
- SuperHeader/Directory/LaneHeader/SlotHeader-Groessen (`256/128/384/256`);
- alle **12** SuperHeader-Feld-Offsets (Magic, Version, Endian, HeaderSize, Generation, LaneCount, DirSize, LaneHeaderSize, SlotHeaderSize, Alignment, CRC-Kind, TotalBytes);
- **8** Directory-Offsets (LaneID/Direction/Class/Flags/Capacity/SlotStride/LaneOffset/LaneBytes);
- **14** Lane-Header-Offsets (write/read, dropped/oversize, producer/corrupt/drained, generation/epoch/state, finalWrite/shutdown, owner/quarantineReason);
- **12** Slot-Header-Offsets (committed/kind/flags/payloadlen/timestamp/clock/source/peer/hash/aux/epoch/crc32c).

**Wichtig:** Das ist ein **statischer Konstantenvergleich**, kein durchgefuehrter Live-Interop-Test, kein Proof der dynamischen Go-/Rust-Publisher-/Consumer-Reihenfolge, kein geth-Code-Hook-Qualifikations-PASS. `v2.go` verwendet `sync/atomic.Load/Store` (Go-sequentiell konsistente Atomics) im v2-Publish/Consume, Rust dagegen `Ordering::Relaxed` bei Empty-Clear, `Release` bei Publish und `Acquire` bei Consumer. Im regulaeren **SPSC mit Lease/ReadSequence-Eigentum** ist der unterschiedliche atomare Default nicht per se ein Layoutfehler; bei regelwidrig concurrent Plain-Byte-Overwrite bleibt die unter E6-3 dokumentierte Aliasing-/Race-Frage offen. **Keine** Garantie, dass ein seqlock-aehnlicher 2-Fence-Patch dieses Problem behebt.

**Teil 2 bleibt sinnvoll:** Go/Rust-V2-Interop-Goldens (RecordKind, Epoch, Checksum-Byte-Domain, Direction/LaneClass, Alignment/Endian, Wire-Version, Role-Lease, Quarantine-Grund, CRC32C) und dynamische SPSC-Publish/Consume-/Lifecycle-/Backpressure-/Stress-/Cross-Process-Memory-Ordering-Pruefung anhand der jetzt identifizierten `v2.go`, `v2_test.go`, `ingress.go` und ihrer Rust-Gegenstücke. Tests unter realem gepinnten geth-Checkout, insbesondere C1.3/4/4a-Freeze-Scope und Go `-race` bzw. ARM64-bezogenen kontrollierten Lauf; solche Messungen wurden **noch nicht** gestartet.

### Einordnung / naechster Phasenschritt

**E7-1/E7-2** sind C1/C2-Coverage-/State-Provenienzauftraege, keine begruendete Abkuerzung zur G0.7-LIVE-Freigabe. **E7-3 ist bereits behoben/sicher** und darf nicht zur kuenstlichen Bugzahl addiert werden. **E7-4 ist eine kleine Doku-Korrektur.** E6-3 bleibt nach realem Go-Source-Einblick weiter in der Speichermodell-Pruefung. Die User-Frage nach Dateinamen ist aufgeloest, Fable kann Teil 2 ohne neue Terminal-Kommandos lesen.

**Etappe 7 Status:** **TEIL 1 REVIEWED / TEIL 2 (komplette Go/Rust-Runtime-Interop) OPEN**. C1/C2 `PAUSED` bis G0.7/G0.8, G0.7 `UNARMED`. Kein Codefix implementiert, kein Test-PASS erfunden.


### Etappe 7, Teil 2 — Wire-V2-Go/Rust-Interop, Dateisystem-Safety, Txpool-Last, Atomics

**Pruefung am:** 2026-10-20 · **Fable-Referenz:** `cac786e` · **direkt verifizierter `main`-Stand:** `ad6376baaaf950f94ea4e7f55d5905e8b564af5d`. Seit dem Vorgaenger-Review nur BSC-Bugdokumentation geaendert, nicht die hier referenzierten Go-/Rust-Runtimepfade. **Kein Cargo-/Go-/Mainnet-Test ausgefuehrt; keine produktive Aenderung und kein G0.7-Armierungsschritt.** Phasenfolge G0.7→G0.8 **vor** C1/C2 bleibt bindend.

**Zusaetzlich wirklich eingelesen/geprueft:** `bsc/geth/overlay/darkbot/fastlane/{v2,ingress,fastlane}.go` (kritische Pfade), `v2_test.go` und `ingress_test.go` (relevante Testfaelle), `bsc/engine/crates/geth_ipc/src/protocol_v2/{role_lease,arena,mapping,producer,consumer,guards,layout}.rs` (betroffene Sicherheits-, Memory- und mmap-Pfade), `c15_tests.rs`, `bsc/tools/c1_apply_geth_overlay.py`, C1-Phasenplan und Runbook. Die Fable-Angabe, Rust-`role_lease.rs`/`arena.rs` seien ungelesen, gilt **nicht** fuer diesen unabhaengigen Befundabgleich. Ein Lauf durch alle Tests und eine Source-vollstaendige C1.10-Qualifikation wird **nicht** behauptet.

#### Befundstatus Teil 2 und Korrekturen zu Teil 1

| ID | Befund | Bewertungsstand | Phasenentscheidung |
| --- | --- | --- | --- |
| E7-1 | 3840-Byte-Grenze, jede Out-of-Scope-Oversize blockiert scoped C1.4-PASS | **BESTAETIGT** (Mechanismus), Mainnet-Haeufigkeit **UNBEWIESEN** | bereits unter Teil 1 dokumentiert, ohne voreiligen Kurzrecord-Patch |
| E7-3 | Mainnet-Unsynced-`acceptTxs` moeglich | **WIDERLEGT/ZURUECKGEZOGEN**: `YES` + `c19b-testnet` + Netz-ID `97` zwingend | **KEIN PATCH**; Tests/Guard beibehalten |
| E7-5 | V2-Dir nur `Stat`, Lock-Datei folgt Symlink, Rust-Arena oeffnet Pfad ohne Eigentumsprovenienz | **BESTAETIGTER SECURITY-BEFUND BEI UNVERTRAUENSWUERDIGEN PFADEN** | **Pflichtfix vor produktivem C1-Betrieb**, Go **und** Rust; Fable-Go-Patch allein unzureichend |
| E7-6 | Volles TX-RLP wird vor dem Ring-Full-/Oversize-Check serialisiert | **BESTAETIGTE UNNOETIGE ARBEIT**, reale ms-Auswirkung ungemessen | C1-Ingress-/Node-Last messen, minimaler phasentreuer Optimierungs-/Capture-Mode-Auftrag |
| E7-7 | Fehlende „Seqlock-Fences“ auf Go-Seite bei regelwidrigem Rewrite | **REWRITE-GARANTIE NICHT BEWIESEN; FABLES KONKRETE GO-ATOMICS-CHARAKTERISIERUNG UNGENAU** | Mit E6-3 klaeren; **kein** Swap-/Fence-Schnellfix |
| E7-PARITY | Go/Rust Slot-/Lane-Layout | Bereits in Teil 1 **50/50 statische Konstanten identisch**; weitere Strukturen und Normal-SPSC-Pfade qualitativ plausibel | Kein ABI-Bug hier nachgewiesen; echte Race-/Lifecycle-Interop-Gates weiter erforderlich |

#### E7-5 — Fast-Lane-Dateisystem erlaubt bedingte lokale Symlink-, Lock- und Arena-Provenienz-Angriffe

**Go:**
- `ingress.go::startIngressRuntimeV2Scoped` ca. Z. 437–452 prueft `os.Stat(cfg.Dir)` + `info.IsDir()`, **aber nicht** den Eigentuemer, Gruppen-/Andere-Schreibrechte, Symlink des letzten Pfadsegments oder die Sicherheitslage der Elternkomponenten. Das `Stat` folgt bereits Symlinks.
- `v2.go::acquireRoleLeaseV2` ca. Z. 1390–1420 oeffnet `<dir>/geth-owner.lock` mittels `os.OpenFile(O_RDWR|O_CREATE, 0600)`, dann `flock(LOCK_EX|LOCK_NB)`, `Truncate(0)` und schreibt Diagnosefelder. Ein vom Angreifer in einem **schreibbaren** Verzeichnis gesetzter Symlink kann zu einer **vom geth-User beschreibbaren** Fremddatei aufloesen und diese nach erfolgreichem Lock **kuerzen**. Das ist eine reale Pfadfolge im Code, aber **kein Beweis**, dass ein Angreifer aktuell einen solchen Verzeichniseintrag auf dem Nutzerhost setzen kann.
- `v2.go::createArenaV2` ca. Z. 368–469 benutzt `O_CREATE|O_EXCL` und `0600` fuer die neue generationengebundene Arena. Das verhindert eine vorhandene gleichnamige Datei bei **Erstellung**, **nicht** einen spaeteren `rename`-/`unlink`-/Pfadersatz durch jemanden mit Schreibrechten im umgebenden Verzeichnis. `openArenaV2` ca. Z. 471 ff. und `probeArenaV2` ca. Z. 1290 ff. oeffnen dagegen uebliche Pfade ohne No-Follow/Owner-/Mode-Fingerprinting. Eine neu eintretende Rust-Mapping-Instanz kann deshalb bei austauschbarem Verzeichniseintrag eine vom Angreifer vorbereitete, formal korrekte mmap-Arena lesen.
- `fastlane.go` enthaelt zusaetzlich die historische V1-`os.Stat(dir)`- und mmap-Pfade. Deren **produktive** Erreichbarkeit nach C1-V2-Umstieg ist hier nicht qualifiziert; vor einer sicherheitsbezogenen Gesamtbehauptung separate V1-Retirement-/Restaufrufpruefung, **kein** ungerichteter V1-Refactor.

**Rust, entgegen der „noch nicht gelesen“-Einschraenkung von Fable jetzt bestaetigt:**
- `geth_ipc/src/protocol_v2/role_lease.rs::RoleLeaseV2::acquire` ca. Z. 36–70 oeffnet den Lock ueber `OpenOptions::open(&path)`, `flock`, danach `file.set_len(0)`. **Dieselbe Symlink-zu-Datei-Truncation-Klasse** ist also fuer die Rust-Rolle moeglich.
- `protocol_v2/arena.rs::ArenaV2::open` ca. Z. 96–120 und `open_lane` ca. Z. 394–413 verwenden `OpenOptions::open(path)` + `File::metadata()` + `MappingV2::map`, ohne `O_NOFOLLOW`, gepruefte File-Owner/Mode/Inode-Identity oder sicheren Parent-Directory-Descriptor. `probe_arena_v2` und `stale_arenas_v2` verwenden ebenfalls Pfadnamen; Probe ist read-only, darf aber keine geheimhaltungs- oder sicherheitsrelevante Autoritaet begruenden.
- `MappingV2::map` nutzt `MAP_SHARED` mit Schreib-/Leserechten. Eine valide Generation, Magic, Directory-/Lane-Layout und CRC sind **Integritaetsformat-Pruefungen**, keine kryptographische Echtheitspruefung der Datenquelle. Ein lokaler Pfad-Angreifer mit Zugriff auf den Arena-Inode oder den Verzeichniseintrag kann synthetische `EARLY_TX_SEEN`-/Chain-Records mit scheinbar validem Scope erzeugen. Ob aus diesen Daten ein tatsaechlicher BUY wird, haengt **zusaetzlich** an den getrennten Trading-/Safety-/LocalGeth-Release-Gates (E6-2) und ist **nicht** als vorhandener Echtgeld-Exploit nachgewiesen.

**Warum Fables Patch 1 NICHT vollstaendig ist:** `os.Lstat(cfg.Dir)`, `perm&077==0`, `Uid==geteuid()` pruefen nur die **momentane letzte Pfadkomponente**; Elternverzeichnisse koennen weiterhin tauschbar oder Symlinks sein. Zwischen `Lstat` und der spaeteren `OpenFile`-Operation besteht ein **TOCTOU-Fenster**. `O_NOFOLLOW` am Lock-Open verbietet nur Symlinks **in der letzten Komponente** dieses Opens, weder einen anderen nachtraeglich unter gleichem Namen liegenden Inode noch austauschbare Elternkomponenten und andere Arena-Opener. `os.OpenFile(...|unix.O_NOFOLLOW)` ist als Idee plausibel, aber **kein** hier kompilierter bzw. auditierter Komplettpatch. `mode 0700` fuer ein bestehendes Go/Rust-*gleiches UID*-Deployment ist ein sinnvoller Default; bei bewusst getrennten geth-/Rust-UIDs muss ein eigenes verbindliches, sicher geprueftes Zugriffskonzept her, keine heimliche 0700-Annahme, die Rust aussperrt.

**Verbindliche minimale Sicherheits-Reparatur VOR C1-Deployment (nicht im G0.7-Hotfix):**
1. Ein **klarer Directory-Trust-Contract**: vertrauenswuerdige Elternkomponenten (auch kein austauschbarer Symlink-/World-writable-Zwischenpfad), verifizierter Owner/ACL-/Group-/Others-Zugriff und gebundene process-UID/GID. Betriebs- und Startskripte muessen ihn fail-closed vor jeder Runtime-/Test-Autorisierung durchsetzen. Path-Binding ueber einen offen gehaltenen, mit `fstat` geprueften Verzeichnis-FD, nicht blosses `Stat` plus anschliessendes pfadbasiertes Open.
2. **Go und Rust** oeffnen Role-Leases, Arena-Probe/Create/Open und Lane-Consumer ueber diesen Directory-FD bzw. `openat`-aehnliche OS-gesicherte relative Opens mit `O_NOFOLLOW|O_CLOEXEC` (und bei Creation `O_CREAT|O_EXCL`, kontrolliertem `0600`-Modus), bevor ein Kindprozess oder mmap die Daten nutzen darf. Auf macOS/Linux die jeweilige portable, wirklich kompilierte Variante bestimmen. Nicht blind `openat2` (Linux-only) als Mac-Loesung voraussetzen.
3. **Nach** dem Open auf dem tatsaechlichen FD `fstat`: regulaerer Dateityp, erwarteter Owner, schreibbare Gruppen-/Andere-Rechte und ACLs gemaess Policy gesperrt, plausibler Link-/Inode-Status, erwartete Generation/Laenge/Layout, und soweit zur Quellbindung erforderlich `st_dev/st_ino`-Identitaet zwischen Creator, wiedereroeffnendem geth und Rust-C1-Runner/Evidence. Wird eine Datei oder der Name unter dem offenen FD ausgetauscht, kein neu geoeffneter anderer Arena-Inode ohne kontrollierten Rebind-/Restart-Prozess; Quarantaene/STOP. Ein bereits offenes mmap haengt an seinem Inode, aber ein spaeteres unabhaengiges Open kann sonst auf einen anderen Inode zeigen.
4. `flock` weiter auf **demselben geprueften Lock-FD** durchgehend halten; bei `LockHeld`, Symlink, Hardlink-/unerwartetem Typ, falschem Owner, fremdem Lease-Record oder unerwartetem Lock-Inode niemals `Truncate`/Rollover ausfuehren. Diagnoseinhalt ist keine echte kryptographische Lock-Autoritaet. Leases nicht ploetzlich loeschen/umbenennen und keinen zweiten Role-Owner herstellen.
5. Vergleiche lassen vorhandene MAP_SHARED-/SPSC-ABI-Bytes, Epoch-/Owner-/Generation-/CRC-/RecordKind-Struktur **unveraendert**. Keine Rekonstruktion von gefaelschten Records aus numerischen Timestamps; Capture-/Latency-Evidence bleibt bis zur verifizierten trusted-Runtime-Quelle **UNQUALIFIED**. Betriebsdokumentation fuer geth/rust gleiche UID oder separate UIDs und alle Einstiegspunkte, nicht nur `startIngressRuntimeV2Scoped`, aktualisieren.

**Offline-/Security-Tests (ohne Echtgeld/echten geth-Broadcast):** Go-/Rust-Lock-Datei als Symlink auf harmlose Dummy-Victim-Datei, vorgepflanzte Role-Lease in unsicherem Verzeichnis, world-/group-writable Directory, falscher UID/ACL, symlinkisierte Elternkomponente, austauschbarer letzter Pfad, doppelter geth-Owner trotz Lock, Arena-`rename` zwischen Create/Open, manipulierte aber formal valide mmap-Header/Record/CRC, neuer Inode gleicher Generation, saubere `0700`-Temp-Dir und erlaubte Go/Rust-Owner-Variante, macOS+Linux. In jedem Angriff darf die Dummy-Datei **nicht** gekuerzt, ein untrusted Arena-Record **nicht** zum PASS und ein ungebundener Prozess **nicht** zum zweiten Lease-Owner werden. Benutze `MkdirTemp`-Fixtures mit Fake-Dateien; keine privaten echten Pfade/Keys.

**Abnahme E7-5:** die Herkunft der geoeffneten gemeinsamen Arena und der beiden Role-Leases ist identitaetsgebunden und auf beiden Seiten durchgesetzt. Auf einem sicher besessenen privaten 0700-Directory ist der Angreifer aus einem **anderen** Unix-User grundsaetzlich ausgeschlossen; bei kompromittierter gleicher UID ist dies keine garantierte Sicherheitsgrenze. **Keine** Ausweisung als bereits ausgenutzter Angriff.

#### E7-6 — Txpool-Lane serialisiert auch bei absehbarer Vollbelegung

**Go-Nachweis:** `ingress.go::publishTxpool` ca. Z. 863–890 laeuft synchron im durch `bsc/tools/c1_apply_geth_overlay.py` eingehakten `eth/handler.go::txBroadcastLoop`. Es iteriert ueber jede angenommene TX und ruft zuerst `serializeTxV2(tx)` auf (typischerweise `types.Transaction.MarshalBinary`), **erst danach** `r.txpool.publish(record)`. In `v2.go::laneV2.publishChecked` ca. Z. 800–821 werden `len(record.Payload) <= SlotStride - 256`, `shutdown`, `write_sequence` gegen `read_sequence` geprueft. Die Txpool-Kapazitaet ist 4096 Records. Bei aktiver Fast-Lane ohne Txpool-Consumer wird sie spaetestens nach **4096 erfolgreich publizierten** Records voll, danach wird jeder weitere Input trotzdem vorher serialisiert, bevor `QueueFull` abgelehnt wird. Die Funktion `PublishTxpoolAcceptedV2` macht bei `ingressActiveV2 == nil` bereits **no-op**; die zusaetzliche Arbeit tritt nur bei aktivem V2-Runtime-Modus auf. Die konkrete Zahl an Txpool-TXs/s, Marshal-Kosten und geth-Broadcast-Verzoegerung wurde hier **nicht gemessen**.

**C1-Fix ohne Evidence-Faelschung:**
1. Vor teurem `MarshalBinary` einen **konservativen atomaren Kapazitaets-/Shutdown-/State-Check** auf dem nur einen Txpool-Lane-Producer ausfuehren. Es gibt weiter genau einen Producer. Danach im tatsaechlichen `publishChecked` **erneut** verbindlich alle Checks ausfuehren: Vorpruefungen sind Optimierung, keine Permit-/Write-Autorisierung.
2. Fuer jeden wegen Full/Shutdown/precheck unterdrueckten Txpool-Record die **gleichen oder explizit versioniert gleichwertigen** Refusal-/Drop-/Clock-/Source-Counters erzeugen; die C1.4-Audit-Qualification bleibt `INCOMPLETE`, sobald ein fuer den benoetigten Denominator relevanter Eintrag fehlt. Kein vorzeitiges `return` ohne loss/gap count und keine heimliche „alles out-of-scope“-Annahme, falls Envelope nicht dekodiert wurde.
3. Bei absehbarem Oversize optional ein **konservatives billiges Groessen-Upper-Bound-Signal** oder explizit eigenstaendige Kurzrecord-/Scope-Evidence nutzen. Weder `tx.Size()` noch ein anderes gecachtes Laengenfeld ohne Identitaet zu den wirklichen `serializeTxV2`-Bytes als garantiert kanonischen Record-Ersatz behandeln; nach Precheck bleibt finaler `publishChecked` massgeblich. Alternative: Txpool-Hintergrund-Capture bewusst nur waehrend qualifizierter Gate-Fenster aktivieren, dann **keine** C1.4-Laufzeit-Coverage ausserhalb dieser Fenster behaupten. Nie eine ungebundene Queue in den geth-Txpool-Hotpath einfuehren.
4. Belastungstests: voller Ring (4096), Consumer-Race beim Freimachen, langer Batch, hohe RLP-Groesse, nichtmarshalbare Transaktion, aktive/nichtaktive Fast Lane, Slow-Consumer und closed/drained-Audit; gleichzeitig geth-`txBroadcastLoop`-Durchsatz, p50/p95/p99 und maximalen Blockieranteil mit gepinntem Build messen. Verlust-Counter und Gate-Ergebnisse bei gleichen Fixture-Inputs muessen korrekt und nachvollziehbar bleiben.

**Abnahme E7-6:** weniger unnoetige Marshal-Arbeit bei erkennbar Full/Shutdown **ohne** falschen C1.4-PASS, ohne ungezählte verlorene Txpool-Daten und ohne neue blocking Queue. Performance nur als `IMPROVED` etikettieren, wenn auf gleicher Hardware mit gleichen TX-Fixtures gemessen.

#### E7-7 — Go-Atomic-Reihenfolge, Rust-Guard und behauptete Seqlock-Rewrite-Erkennung

**Go-Quellen:** `v2.go::laneV2.publishChecked` ca. Z. 825 schreibt `committed=EMPTY_SEQUENCE` mittels `atomic.StoreUint64`, dann plain-Byte-Header/Payload und schliesslich `committed=write+1` und `write_sequence=write+1` erneut via `atomic.StoreUint64`. `laneV2.tryConsume` ca. Z. 854–926 nutzt `atomic.LoadUint64` auf Sequences und Commit, danach liest/kopiert es Slotbytes in **einen privaten Slice** und veroeffentlicht `read_sequence` erst nach dem Copy. Der Outbound-Worker haelt daher im gueltigen SPSC-Pfad keine fremd geteilte mmap-Payload nach erfolgreichem Consume fest; `job.raw` ist eine eigene Kopie.

**Rust:** `producer.rs` benutzt `Relaxed` fuer das geloeschte Commit, `Release` fuer gefuellte Commit-/Write-Sequence. `consumer.rs` `Acquire` und `guards.rs::check_stable` nochmals `Acquire`. Der Guard leiht Plain-Byte-Slices nur, solange `read_sequence` noch nicht freigegeben wurde. Fables Behauptung „Go `atomic.StoreUint64` ist nur ein Release-Store“ ist **sprachlich zu schwach**: Go `sync/atomic` ist nach Go-Memory-Model fuer diese Operationen **sequentiell konsistent**. Ein Wechsel zu `atomic.SwapUint64` wuerde alleine dennoch **keinen** Beweis liefern, dass ein vertragsverletzender Producer gleichzeitig Plain-Bytes schreiben duerfte. Die `&[u8]`-Borrow-/C-Race-/Go-Data-Race-Sicherheit bleibt separat. Auf ARM64 abweichende Maschineninstruktionen/Performance nicht als pauschale Garantie interpretieren.

**Urteil, E6-3 gemeinsam:** Unter den vereinbarten SPSC-/Lease-/Guard-Eigentumsinvarianten ist die Publish/Consume-Happens-Before-Reihenfolge plausibel; keine konkret nachgewiesene regulaere Auslieferung falscher Bytes. Eine **zusaetzliche** Garantie „wir erkennen auch jeden verbotenen Concurrent Rewrite“ ist durch die aktuelle Commit-Word-Recheck-Logik nicht umfassend bewiesen. `protocol_v2/c15_tests.rs::a_slot_rewritten_under_the_guard_is_detected_and_not_released` manipuliert `committed` **sequenziell im Test**, kein gleichzeitiger Plain-Byte-Write unter einem geliehenen Rust-Slice. Ein Fence-/Swap-Patch ist damit weder als erforderlich fuer den regulaeren Ablauf noch als hinreichender Beweis fuer die Zusatzgarantie freigegeben.

**C1.5/7-Folgeauftrag:** gemeinsame Go/Rust-Owner-/Lease-/Lane-Epoch-/Guard-Happens-Before-/Cache- und Device-Inode-Annahmen schriftlich und per kontrollierter Interop-/Stressprobe ueberpruefen. Fuer den explizit behaupteten illegalen Rewrite entweder die Situation **strukturell unmoeglich** machen (einiger sicherer Owner, sichere Role-Leases E7-5, atomare Lifecycle-Transitions, kein Borrow ausserhalb Guard) oder eine valide konsistente Snapshot-/atomare Read-Strategie mit nachgewiesener Datenrace-Freiheit entwerfen. Auf macOS/ARM64 und Ziel-Linux kompiliert vergleichen, `go test -race` **ergänzend**, nicht als alleiniger Cross-Process-Shared-Mmap-Beweis. CRC-/Sequence-/Aliasing-/Wrap-/Quarantine-/Backpressure-/Go↔Rust-Goldens sowie HOT0-P99/Allokationen unter gleicher Fixture-Scope messen. Keine Aenderung des eingefrorenen Wire-V2-Layouts und kein unqualifizierter `atomic.Swap`-Hotpath-Zuschlag. E6-3 und E7-7 sind **ein gemeinsamer Memory-Model-Verifikationsauftrag**, keine zwei voneinander unabhaengigen bewiesenen Laufzeitfehler.

#### Positive Querverifikation und verbleibende Etappe

- **Go ↔ Rust:** Die bereits im Teil-1-Review statisch verglichenen 50 Header-/Directory-/Lane-/Slot-Offsets bleiben **50/50 identisch**. `v2.go`/Rust `layout.rs` stimmen bei Magic `DBFLN002`, Version 2, Little-Endian-Vertrag, Slot-Header 256, Lane-Header 384, Offset-Gruppen, CRC32C-Feldern, Quarantine-Codes und Eigentuemer-/Epoch-/Status-Modell strukturell ueberein. Dies ist **kein** erneuter Live-Interop-Test.
- **Go-Consumer-Payload-Lifetime:** `tryConsume` benutzt `append([]byte(nil), slot[payload]...)` vor `atomic.StoreUint64(read_sequence, read+1)`. Das beseitigt eine moegliche konkrete Slot-Lifetime-Unsicherheit in der Go-Outbound-Weitergabe, solange Worker das erhaltene eigene Slice respektieren.
- **K14/K15-Lifecycle:** Go/Rust haben `FREE→BOUND→CLOSING→FREE` nur mit Owner/Epoch/Drain, terminale Quarantaene, role leases `flock` und generationengebundene Dateinamen; deren **inhaltliche Ownership-/Pfad-Sicherheit** bleibt bis E7-5 fix/offline-negativgetestet unqualifiziert. Ein richtiger Lease-Epoch-Wert kann keine manipulierte Dateiidentitaet ersetzen.
- **C19b Unsynced Mainnet:** Explizit `networkID==97` und Testnet-Arm/Scope erforderlich; Mainnet verweigert. **E7-3 bleibt geschlossen (NOT A BUG)**.
- **Restliches Review:** `fastlane.go` (Legacy-V1-Erreichbarkeit), `qos*.go`, das gesamte Testkorpus und der vollstaendige gepinnte geth-Source-Flow inkl. Head-Mutationen sind **nicht als abschliessend geprueft** auszugeben. Bis fehlende C1.4a/C1.9b/1.10- und Mainnet-Coverage-Evidence wirklich vorhanden sind, C1 `NOT FROZEN`/C2 `PAUSED`.

**Gezielte Regressionen fuer Coding-Agent (nicht in diesem Review ausgefuehrt):**

~~~bash
# In exakt gepinntem geth-BSC-Checkout nach kontrolliertem Overlay-Apply:
go test ./darkbot/fastlane -count=1
go test -race ./darkbot/fastlane

# Im Projekt-Repo:
cd bsc/engine
cargo test --locked --offline -p darkbot-bsc-geth-ipc
cargo test --locked --offline -p darkbot-bsc-geth-ipc --test c15_hot0_alloc
cargo test --locked --offline -p darkbot-bsc-geth-ipc --test c18_stress
cargo fmt --all -- --check
~~~

**Danach** C1.3/C1.4-Go↔Rust-Interop ueber das tatsaechliche `bsc/tools/c14_ingress_interop.sh`, C1.5–C1.10 nur in der verbindlichen Runbook-Reihenfolge mit **echten** qualifizierten Mess-/Evidenzrecords. E7-5-Sicherheitsnegativtests muessen auf Go und Rust fehlschlagende Opens und unveraenderte harmlose Dummy-Dateien belegen; E7-6 positive P99-Messung erst nach Implementierung. Nicht einen zufaelligen Test auf einem nicht gepinnten geth als PASS fuer die eingefrorene Source-Kette melden.

**Gesamturteil Etappe 7:** **TEIL 1 + TEIL 2 REVIEWED (C1/C2-Scope); E7-5 SECURITY FIX OPEN, E7-6 PERFORMANCE/ACCOUNTING OPEN, E7-7/E6-3 MEMORY-MODEL PROOF OPEN; E7-3 NO BUG; Go/Rust static layout 50/50 PASS**. Es bleibt **Etappe 8** (Journal, Records/JS, Legacy-Import). Keine G0.7-v4- oder wirtschaftliche Freigabe.


## 9. Etappe 8 — Journal, DBREC-/JS-Werkzeuge und Legacy-Import (letzter Fable-Report)

**Review:** 2026-10-20 · **Fable-Quellstand:** `cac786e` · **unabhaengig verifizierter aktueller Git-`main`-Stand:** `d2088bc0dcd7e59fccc3a37ea843b66cb1c2391c`. Die hier bewerteten Journal-, Record-/JS-, Import- und DRY-Tool-Pfade sind gegenüber dem Fable-Code unveraendert. Keine Produktionscode-Aenderung, keine Cargo-/Mainnet-/DRY-Tests ausgefuehrt. **Alle acht gelieferten Etappen sind fachlich erfasst, nicht alle Repositorydateien vollumfaenglich auditiert und kein dokumentierter Codefix dadurch als implementiert zu behandeln.**

**Source-Scope:** `bsc/engine/crates/journal/src/{format,recorder,replay,lib}.rs`, `bsc/engine/crates/record/src/js.rs` und `src/bin/{dbrec_compare,dbrec_show}.rs`, `bsc/tools/g06a_final_dry.sh`, **zusätzlich gefunden und direkt geprueft:** `bsc/engine/crates/legacy_import/src/{lib,main}.rs`, beide Crate-Manifeste, das versionierte Verzeichnis `bsc/native/evidence/g06a/g07_binary_import_20261016/` und relevante vorhandene Unit-Tests. Korrektur zu Fable: `legacy_import` existiert **als eigenstaendiges Crate**, nicht unter `record/src`. Ein GitHub-404 bei vermutetem Dateinamen war **kein** Nachweis fuer fehlenden Importcode.

**Phasentreue:** `BSC_DUAL_VENUE_ROADMAP.md` und `BSC_PRODUCTION_BUILD_MASTER_PLAN.md` bleiben fuer die Produktions-/C2-/Evidence-Infrastruktur verbindlich. Hier werden **keine** G0.7-Live-/Preis-/Signer-/Freigabefelder veraendert; Guard `UNARMED`, G0.7/G0.8 `NOT_EXECUTED`, C1/C2 `PAUSED`. Der kleine DRY-Shell-Fix E8-3 gehoert funktional zur Gate-0-Tool-Haertung und darf nur gezielt, ohne automatische Vollrequal oder Echtgeld, bearbeitet werden. E8-1/E8-2/E8-4 gehoeren zum noch nicht produktiv integrierten Capture-/C2-Vertrag.

### Etappe-8-Urteil (einschliesslich eines eigenstaendig gefundenen Befunds)

| ID | Gewicht | Nachgewiesener Zustand | Fix-Auftrag |
| --- | --- | --- | --- |
| E8-1 | **Mittel-Hoch fuer C2-Capture** | **BESTAETIGT:** Recorder verbraucht Sequenz bei `QueueFull`; Writer/Reader verbieten spaetere Sequenzluecke und stoppen. Anfangsverluste sind ohne Startsequenz-Vertrag nicht nachweisbar. | **Pflicht vor C2:** wahrheitsgemaesse Gap-Evidence, Version/Kompatibilitaet und Ordering |
| E8-2 | **Mittel fuer persistierte Safety-Evidence** | **BESTAETIGT:** nach Header-/Payload-Teilwrite bleibt Writer weiter verwendbar; `flush()` garantiert kein `fsync`. | Writer terminal poison nach potenziell partiellem Write, separater Durable-Commit-Owner |
| E8-3 | **Niedrig-Mittel, DRY-Tooling** | **BESTAETIGT:** `export X="$(get...)"` maskiert Exit; auch Compare-Substitutionen koennen Fehler maskieren. Heute nachgelagerte STOP-Gates, kein nachgewiesener Live-Bypass. | Enger fail-closed Shell-Fix, zusammen mit E3-2-Key-Isolation |
| **E8-4 (neu)** | **Mittel-Hoch, C2-Ordering** | **ZUSAETZLICH BESTAETIGT:** `try_record(&self)` verteilt Sequenzen atomar **vor** `try_send`. Zwei gleichzeitig aufrufende Threads koennen diese Sequenzen in umgekehrter Reihenfolge in die Queue geben **ohne QueueFull**. | **Zusammen mit E8-1** herstellen: linearisiertes Ordering oder strikt typisierter SPSC-Owner, keine fiktiven Gap-Frames |
| E8-LEGACY | Positiver Quellenfund | **VORHANDEN / SOURCE REVIEWED:** separates `darkbot-bsc-legacy-import`-Crate, 6 hashgebundene Legacy-Inputs und bytegenaue Offline-Paritaetspruefung; kein konkreter neuer Defekt belegt | **Kein Patch**; bestehende Evidence/Goldens erhalten |
| E8-RECORD | Bestehende Werkzeuge | `js.rs` prueft gepinntes Schema-Modul; `dbrec_compare` liefert dokumentierte 0/1/2-Logik; `dbrec_show --get` liefert 3 bei fehlendem Feld | Keine ad-hoc-Codec-/Schema-Aenderung |

### E8-1 + E8-4 — Queue-Verluste, Journal-Gaps und konkurrierende Producer korrekt unterscheiden

**Quellbeweis A — echte Queue-Verluste:** `journal/src/recorder.rs::Recorder::try_record` (ca. Z. 73–108) prueft zuerst Oversize, dann `self.counters.next_sequence.fetch_add(1, Ordering::AcqRel)`, allokiert den Record und fuehrt `self.sender.try_send(record)` aus. Ein `Full` nach der Vergabe laesst `sequence` verbraucht und erhoeht `dropped_full`. Spätere Aufrufe vergeben groessere Nummern. `journal/src/format.rs::JournalWriter::append` ca. Z. 81–95 verlangt strikt `record.sequence == last+1`; `journal/src/replay.rs::JournalReader::next_record` ca. Z. 72–83 ebenso. Die vorhandenen Einzeltests `recorder_never_waits_and_overflow_is_counted_with_sequence` und `sequence_gaps_are_rejected_by_writer_and_replay` testen die beiden Hälften **getrennt**. **Kein End-to-End-Test** verbindet QueueFull -> Drain -> Writer -> Replay. Wenn nach QueueFull ein weiterer Record ankommt, erzeugt der Writer reproduzierbar `SequenceGap`. Die Behauptung „das Journal kann die Luecke beim Replay sichtbar machen“ ist so nicht erfuellt.

**Quellbeweis B — neue Concurrency-Gefahr (E8-4):** Der Typ `Recorder` besitzt nur `SyncSender<CaptureRecord>`, einen `usize` und Atomic-Counters und bietet `try_record(&self)`, somit ist seine API **nicht** auf genau einen seriellen Aufrufer eingeschraenkt. Beispielsweise: Thread A reserviert Sequenz 0 und pausiert; Thread B reserviert Sequenz 1 und ruft `try_send` zuerst auf; A sendet erst danach. Der Channel kann den Record `1` **vor** `0` liefern, ohne dass irgendein Record verloren ist. Der Writer kann beim allerersten Frame willkuerlich mit 1 starten (bestehender Roundtrip-Test startet bei 7), und verweigert anschliessend 0. Fables vorgeschlagener automatischer `Gap(0, span=1)` wuerde sogar eine **frei erfundene** Drop-Evidence erzeugen, obwohl der spaeter ankommende Record 0 vorhanden ist. Ein direkter Gap-Insert aufgrund nur von `record.sequence > expected` ist fuer diese API deshalb **sicherheitsfachlich falsch**, bis die Send-Reihenfolge verbindlich garantiert wird.

**Quellbeweis C — erster verlorener Record:** `JournalWriter::new` initialisiert `last_sequence=None`; sein erster Append erlaubt jede `sequence` (Tests 1 und 7). Wenn `Recorder::try_record` Sequenzen 0..N-1 verliert und der erste aufgezeichnete Record N hat, entsteht **nie** ein Gap-Record allein durch `last_sequence`. Der Recorder hat zwar `next_sequence` und `dropped_full`-Zaehler, aber ein Journal ohne gebundene Session-`start_sequence`-/Recorder-Snapshot-Provenienz darf fehlende Anfangsereignisse nicht als „garantiert nicht verloren“ ausgeben.

**Verbindlicher Design-Fix vor Umsetzung irgendeiner GAP-Ergaenzung:**
1. **Ordering-Owner definieren.** Entweder diese Capture-Queue ist **strikt SPSC** und ihr Producer hat einen einzigen typisiert erzwungenen Owner (keine geklonten/parallel verfuegbaren Recorder-Handles), **oder** Multi-Producer wird explizit unterstuetzt und jede Sequenz mit der tatsaechlichen linearen Enqueue-/Reservation-Ordnung gebunden (z.B. qualifizierter bounded MPSC-Ring mit per-slot-Sequenzzustand, Abschluss-/Drop-Markern und maximalem Reorder-Budget). **Keine blockierende Mutex-/fsync-Operation in HOT0** nur um die Zahlenfolge zu retten. Ein Test mit Barrier A reserviert 0 / B sendet 1 / A sendet 0 MUSS entweder wegen SPSC-Typvertrag nicht kompilieren oder die richtige Ereignisfolge ohne fiktiven Verlust konservativ erhalten.
2. **Wahrheitsgemaesse Verlustgrundlage:** Ein in-band GAP darf ausschliesslich eine **nachweisbar verlorene** zusammenhaengende Recorder-Sequenzspanne vertreten. `QueueFull`, echter `Disconnected`, Oversize **vor** Vergabe, producer-lokaler Fehler und unbekannter Abschluss muessen unterschiedliche diagnostische Counter-/Scope-Evidence besitzen. Bei nicht beweisbarer Zuordnung STOP/INCOMPLETE, **nicht** einfach eine fehlende Nummer als QueueFull behaupten. Nach Crash/Restart ist der gebundene Start-/Endcursor mit Generation/Writer-Owner/Journal-Manifest zu verifizieren. Anfangsverluste nur bei expliziter Recorder-Startsequenz 0 oder gesondertem bindenden Session-Cursor als Gap schreiben; der generische Writer darf seine bisher erlaubte beliebige `first_sequence`-Semantik nicht stillschweigend aendern.
3. **Bestehendes `RecordKind::Gap` ist nicht unbenutzt!** `JOURNAL_MAGIC=*b"DBJNL001"`, `JOURNAL_VERSION=1`, `RecordKind::Gap=5`. Heute ist die Gap-Payload **nicht** als `u64 little-endian span` normiert; `journal/src/lib.rs::manifest_hash_is_over_exact_journal_bytes` schreibt legal `kind=Gap, payload=b"drop=1"`. Fables neues `gap_span(payload)` verlangt 8 Bytes `u64>0` und aendert die Reader-Sequenzfortschreibung. **Ohne Wire-/Format-Versionsabgrenzung wuerde ein vorhandener gueltiger V1-Gap-Frame zum Fehler, und alte Reader wuerden nach einem neuen Multi-Sequence-GAP den folgenden Record faelschlich als Sequenzsprung ablehnen.** Weder eine beibehaltene `JOURNAL_VERSION=1` noch das blosse Umschreiben dieses Tests darf als rueckwaertskompatibel gelten.
4. **Versionierten, eng abgegrenzten Vertrag entwerfen:** z.B. ein neues DBJNL-Format mit eigener `VERSION` bzw. neuem dedizierten Span-Record mit exakt definiertem Payload, oder eine nachweisbar wohldefinierte V1-kompatible Extension **mit golden-geprueften alten Journalbytes und Reader-Kompatibilitaetsmatrix**. Neue Reader muessen die historischen V1-GAP-Frames exakt als die alten einzelnen Sequenzrecords behandeln und bei einem neuen `GAP_SPAN` eindeutig wissen, dass `sequence + span - 1` die letzte verlorene logische Sequenz ist; `span=0`, Ueberlauf, falsche Laenge, duplizierte/ueberlappende Spans, out-of-order und Reorg-/Generation-Mix ablehnen. Alte V1-Evidence **nicht** umschreiben oder historische `SHA256SUMS` neu ausgeben. Wenn im Repo kein reales altes Capturefile gepinnt ist (der aktuelle Git-Tree zeigt nur Source-/Unit-Tests von `darkbot-bsc-journal`), dann diesen Evidenzmangel offen nennen und bei kompatiblen V1-Kontrollframes vorhandene Testvektoren wenigstens explizit fuer die Altsemantik pinnen.
5. **Manifest-/Counter-Vertrag anpassen, nicht verbergen:** `JournalManifest.records` ist aktuell die **Zahl der physisch geschriebenen Frames**, `last_sequence` die letzte Record-Sequence, und `sha256`/Bytes beziehen sich ausschliesslich auf die exakten Journalbytes. Bei Span-Frames physische Frame-Anzahl vs. logische Ereignisanzahl bzw. `lost_count` eindeutig getrennt ausweisen; CRC32 und SHA256 ueber **tatsaechlich serialisierte Bytes** fuer GAP und alle anderen Frames; `first_sequence`/letzte konsumierte **logische** Sequenz versioniert und crashfest reporten. Die Reader-`max_payload`-Grenze bleibt vor Allokation aktiv; keine endlosen Replay-Schleifen ueber `span` hochzaehlen.

**Pflicht-Regressionen:**
- Vollstaendige gebundene Kette `Recorder::bounded(1,64)` -> Seq 0 erfolgreich, Seq 1 `QueueFull`, Drain 0, Seq 2 erfolgreich -> Drain/Writer/Reader/Manifest. **Nur der tatsaechlich verlorene 1er-Bereich** erscheint als Gap.
- Zwei und viele aufeinanderfolgende `QueueFull`; erste 0..N-1 verloren, naechster Erfolg N; `Disconnected`-/Oversize-Scope; leerer/stillgelegter Producer; Generation-Neustart und End-of-stream.
- Gleichzeitige Producer in nachweislich **umgekehrter Enqueue-Reihenfolge** ohne Verluste, testbar mit Barrieren; im neuen erlaubten Interface entweder Compile-Fail fuer Mehrfachaufrufer oder exakte Ordnung trotz Parallelitaet. Kein `DROP`-Record bei nur vertauschter Reihenfolge.
- Historischer V1-`Gap`-Frame mit Payload `b"drop=1"` **muss bei V1-Reader weiterhin lesbar bleiben**; neuer `Span`-Frame innerhalb seiner eigenen Versionsdomäne; fieser Gap mit 0, u64-MAX-Overflow, manipuliertem CRC, Truncation, Duplikat, Reorder, ungültiger End-Sequence; Roundtrip alle RecordKinds; dokumentierter `manifest.records` vs `lost_count` und SHA-256-Golden.
- Keine Live-Submission und keine falschen PASS-Snapshots bei unvollstaendigen Capture-Daten.

**Abnahme E8-1/E8-4:** Kein Recorder-Drop blockiert den kontrollierten, vollstaendig gekennzeichneten Capture-Drain; keine echte Capture-Luecke wird zu vollstaendiger Evidence umetikettiert; kein fiktiver GAP bei interleaved Send und keine V1-Datenmigration ohne Versions-/Golden-Beweis. Fables Patch 1 **nicht** ohne diese Vorarbeiten einsetzen.

### E8-2 — Journal-Writer braucht terminales I/O-Fehler-Fence und separaten Durable-Commit

**Fundstelle:** `journal/src/format.rs::JournalWriter::append` ca. Z. 81–117 schreibt `header` und `payload` mit **zwei** `write_all`-Aufrufen. Bei einem Fehler kann der Sink 0 bis N Bytes bereits geschrieben haben. `hasher.update`, `records`, `last_sequence` und `bytes` werden erst nach erfolgreichen Writes aktualisiert. Bleiben Header/Teilpayload physisch auf dem Medium, ist der aktuelle `manifest.sha256` kein Hash der tatsaechlichen Dateibytes. **Aktuell gibt es keinen `poisoned`-Status**: die naechste `append`-Operation kann weiter schreiben und einen unkorrigierbar ungueltigen Stream fortsetzen. `JournalReader` erkennt beim spaeteren Lesen CRC-/Truncation-/Magic-Probleme, kann aber den Original-Inhalt nicht rekonstruieren.
 
**Pflichtfix fuer Capture-Evidenz:**
1. `JournalWriter` nach **jedem** partiell moeglichen I/O-Fehler bei Header-/Payload-`write_all` und nach `flush`-Fehler in terminalen `POISONED`-Zustand versetzen; spaetere `append` und ggf. `flush` melden stets `JournalError::Poisoned`. Reine Validierungsfehler **vor** erstem Write (Payload zu lang, unzulaessige Sequence, unzulaessiger Gap-Span) muessen nicht als partieller Write behandelt werden. Ein Frame-Write kann erst dann Zaehler und SHA aendern, wenn er **vollstaendig** durch den `Write`-Sink angenommen wurde; ein Fehler nach internem Puffern liefert noch **keine** Durability-Garantie.
2. Fables `fn write_frame`/Poison-Entwurf ist fuer die **Short-write-Fehlerklasse** ein brauchbarer Ausgangspunkt, aber muss mit der E8-1/E8-4-Verlust-/Formatsemantik integriert werden. Bei zwei zusammengehoerigen physischen Frames (z.B. Gap und nachfolgendes Record) ist die gesamte logische Operation ggf. **nicht atomar**: Zwischenstatus nur als beobachtetes PARTIAL/UNKNOWN, nie als fertige bezeugte Sequence-Serie melden. Reader und Recoverer brauchen einen expliziten Segmentabschluss-/Commit-Scope.
3. **`W: Write` kann generell kein `fsync` garantieren.** `self.inner.flush()` bewirkt nur `Write::flush`, nicht `File::sync_all`, und ein rein in-memory `Vec<u8>` ist ebenfalls ein legitimer Sink. Deshalb keine aus dem generischen Writer abgeleitete Aussage „durable“ machen. Bei produktiver Dateievidenz einen getrennten **File-backed durable Segment-/Manifest-Owner** vorsehen: buffered Writes + Flush + `File::sync_all` (und wo erforderlich Directory-fsync/atomarer Commit des Manifests) plus gepinnte Hash-/Bytes-/Sequence-Pruefung; im Hintergrund ausserhalb HOT0. Ob der File-Sink die Datei bei einem Fehler recovern oder ein neues immutable Segment beginnen darf, nur nach Crash-/Partial-Tail-/Generation-Reconciliation-Plan; **kein** automatisches blindes Ueberschreiben.
4. Die bestehende öffentliche `manifest()`-Signatur `-> JournalManifest` darf nicht durch schweigendes Bereitstellen eines formell „fertigen“ Hashes auf einem poisoned Sink zu einer Proof-Luege fuehren. Vertraglich unterscheiden zwischen *attempted/diagnostic manifest* und *durable/complete verified manifest*, oder passend gekapselte `finish/commit`-API mit getrennter Resultat-Autoritaet. Nach Crash dürfen ueberlebende ganze Frames nicht ohne Canonical-/Cursor-/Scope-Checks zu `PASS` erhoben werden.

**Gezielte Negativtests:** `Write`-Sink gibt Short-write nach einigen Headerbytes, nach komplettem Header und mitten im Payload, sowie Flush-Err zurueck; Manifest-/Reader-Zustand bleibt `INCOMPLETE` und alle spaeteren Appends `Poisoned`. Fehler bei erstem und bei zweitem Frame einer Gap+Record-Emission; physisch unvollstaendiger Tail, `sync_all`-/Manifest-Commit-Fehler im File-Sink, Unterbrechung vor/nach rename und fsync, u64-Counter-Overflow. Verifizierter Journalbyte-Hash muss exakt zu seinem persistenten Blob gehoeren.

**Abnahme:** Kein Teilwrite kann als fertige, spaeter unveraendert fortschreibbare Capture-Evidence gelten. Dauerhaftes Journal-PASS nur mit explizitem File-/Manifest-Commit, nicht wegen `flush()` allein.

### E8-3 — `g06a_final_dry.sh` muss jeden erforderlichen DBREC-Read-Exit pruefen

**Code:** `bsc/tools/g06a_final_dry.sh` startet mit `set -euo pipefail`, nutzt `get() { dbrec_show --get ...; }`. In den Zeilen um 127–149 und 159 stehen u.a.:
- `export DARKBOT_G06A_LAUNCHER_NONCE="$(get launcher_pending_nonce "$PREFLIGHT")"`
- `export DARKBOT_G06A_BUYER_NONCE="$(get buyer_pending_nonce "$PREFLIGHT")"`
- `export DARKBOT_G06A_SEND_HEAD="$(get latest_block_number "$PREFLIGHT")"`
- `export DARKBOT_G06A_LOCAL_EXPECTED_OUTPUT="$(get expected_output "$QUOTE")"`
- `export DARKBOT_G06A_SALT="$(get salt "$ECON")"`
- `export DARKBOT_G06A_BUILDER_FLOOR_EVIDENCE_SHA256="$(shasum -a 256 "$BUILDER_POLICY" | awk '{print $1}')"`

**Bestätigung:** `export X="$(command)"` hat als Shell-Builtin einen eigenen erfolgreichen Exitcode, der den urspruenglichen Exitstatus der Command-Substitution verdecken kann. Damit ist `set -e` fuer diese Zuweisungen **kein verlässlicher Fail-Closed-Mechanismus**. In den konkret beschriebenen Happy-Path-/Fehlerketten fuehren spaetere Rust-Parser-/Schema-/Hashchecks viele dieser Werte weiter auf STOP; **ein erfolgreicher unautorisierter LIVE-Kauf wurde nicht nachgewiesen**. Aber die Trust-Boundary soll ihre Fehler selbst erkennen, nicht sich auf zufaellige Folgefehler verlassen.

**Zusaetzlich gefundener Randfall:** Ausdruecke wie `[[ "$DARKBOT_G06A_SALT" == "$(get salt "$FROZEN/economics_crosscheck.dbrec")" ]]` und am Skriptende `[[ "$(get "$field" "$DRY")" == "$(get "$field" "$FROZEN/dry_result.dbrec")" ]]` pruefen **Gleichheit von stdout**, aber nicht unabhaengig den Exitcode beider `get`-Aufrufe. Wenn beide an dieser Stelle kein stdout liefern, koennen leere Werte identisch sein und das `[[ ... ]]`-Ergebnis `0` werden, obwohl das Lesen fehlgeschlagen ist. Weitere vorgelagerte Datei-/Schema-Checks duerfen diese Luecke nicht als lokale Korrektheit ersetzen.

**Minimaler Fix mit E3-2-Key-Isolation zusammen planen:**
1. Fuer jede kritische DBREC-Feldabfrage **erst** eine eigenstaendige, auf Exitcode gepruefte Shell-Zuweisung: `value="$(get field "$record")" || die "typed field unavailable: field"`, erst **danach** `export NAME="$value"` oder ein explizit gleichwertiger helper `get_required_field`. Fuer jedes technisch erforderliche Feld Schema-/Typ-/Format-/Nichtleer-Kontrakt fail-closed erzwingen; vor allem Nonces, Head, Quote, Salt und Builder-Hash. `CURRENT_IMPL="$(get ...)"` ist bereits eine reine Zuweisung, aber die korrekte Existenz auch dort in der dokumentierten Read-Matrix pruefen.
2. Bei **Gleichheitsvergleichen** die beiden Werte *getrennt* mit Exitpruefung lesen, dann erst vergleichen. Insbesondere `FROZEN`-Referenzrecord, gefrorene TX-Hashes, `transactions.token/min_output`, R0-Scan-Metadaten und individuelle DRY-Felder. Bei fehlendem Feld oder korruptem Record STOP, selbst wenn zufaellig zwei leere Strings gleich waeren. Die kanonische `dbrec_show --get`-Fehlersemantik (Exit 3 fuer absent) erhalten.
3. `sha256="$(shasum -a 256 "$BUILDER_POLICY" | awk '{print $1}')"` als **eigenen** durch `pipefail`/Exitstatus geprueften Schritt ausfuehren, Format `64 ASCII-Hex` pruefen, dann exportieren. Keine erfundene Fallback-SHA oder `|| true` in der Freigabekette. Nur der bewusst **diagnostische** `grep -c ... || true` zaehlt hier nicht als Autorisierung, solange er als unqualifizierter Count bezeichnet wird.
4. Bestehende Outputs, Hash-/Wallet-/Budget-/Nonce-/DRY-All-/Requal-STOP-Gates und die **genau eingefrorenen** signierten Bytes erhalten. E3-2 muss verhindern, dass Keys an die vielen `dbrec_show`-/`shasum`-/Node-/Rust-Helfer unnoetig vererbt werden. Keine erneute lange R0-/DRY-Kampagne nur fuer Shell-Exit-Gates; Offline-Fake-DBREC-/Stub-Tool-Simulation und enge Shell-Tests zuerst.

**Regressionen:** `get` Exit 3 mit leerem stdout; Exit 1/2; falsche Records; beide Vergleichsseiten fehlen; einer fehlt; korrekter Wert `0` (kein false absent), missing SHA, nicht-hex SHA, korrekte gute Fixture, `set -u`-Umgebung und Secret-Isolation. Kein `LIVE`, kein Send, keine Keys im Log. `bash -n`, danach eng begrenzter DRY-Tool-Qualifikationscheck im aktuellen Workspace.

**Abnahme:** Jede vertragsrelevante Quelldatei und jedes Feld muss **selbständig erfolgreich** gelesen und validiert sein, bevor sein Wert Signaturen, Vertrag oder Freigabeverdikt beeinflussen darf. Keine aus leeren fehlgeschlagenen Substitutionen erzeugten „PASS“-Vergleiche.

### E8-LEGACY — Separates `legacy_import`-Crate existiert und ist kein fehlendes Modul

**Gefunden und direkt geprueft:** `bsc/engine/crates/legacy_import/Cargo.toml` deklariert das Workspace-Crate `darkbot-bsc-legacy-import` und das Binary `g07_legacy_import`; `src/lib.rs` (434 Zeilen) und `src/main.rs` (64 Zeilen). **Die Fable-Aussage „unter `record/src` nicht vorhanden“ ist als Pfadangabe richtig, aber die Schlussfolgerung „Import in diesem Review nicht auffindbar“ kann aufgegeben werden.**

**Fachlich relevante Grenzen, im Quelltext bestaetigt:**
- `LEGACY` listet **exakt sechs** unveraenderliche, mit jeweils fest gepinntem SHA-256 gebundene Eingaben: `dry_result.json`, `preflight.json`, `local_simulation.json`, `economics_crosscheck.json`, ein altes `g06a_builder_floor_addendum_2026-10-11.json` und `g07_producer_witness_source_attempt1.json`. **Kein** beliebiger JSON-Parser in Hotpath/Runtimescope. Die alten JSON-Inputs bleiben historisch immutable.
- `import(root,legacy)` berechnet **zuerst** SHA-256 ueber die Originalbytes und verweigert jede Abweichung **vor** `serde_json::from_slice`; `leaves` verbietet unabbildbare verschachtelte Arrays/Keys, anschliessend wird jeder JSON-Leaf gegen die zugeordnete Rust-`Schema`-Feldliste geprueft. Die typisierte Konversion verweigert ungueltige/gerundete Dezimalzahlen, invalides Hex und nicht voll darstellbare Werte.
- Nach dem DBREC001-Encode wird das Ergebnis erneut decodeiert und pro Feld in kanonischer Darstellung gegen den JSON-Wert verglichen; `import_all` erzeugt die sechs `.dbrec`, einen nachvollziehbaren `PARITY.md`-Bericht und `SHA256SUMS`. `check(root)` reproduziert **jedes** dieser Artefakte bytegenau gegen das unter `bsc/native/evidence/g06a/g07_binary_import_20261016/` bereits in `main` vorhandene Paket. Der Import erstellt nur ein **neues** Verzeichnis (`fs::create_dir` und `create_new` pro Datei); existierende Evidenzen werden nicht still ueberschrieben.
- Der Unit-Test `the_committed_import_is_reproduced_byte_for_byte` ist im Repo vorhanden. **Er wurde in diesem Review nicht ausgefuehrt.** Aus der Quellpruefung folgt keine neue erfolgreiche Wiederholbarkeitsmessung. `--check` kann vom Implementierer gezielt offline ausgefuehrt werden.

**Urteil:** Bei der geprueften Importlogik ist **kein konkreter neuer Implementierungsfehler nachgewiesen**. Kein ad-hoc-Patch an JSON-Import-Schemas, historischen SHA-256-Bindungen oder v4-Freigabe aus Etappe 8 notwendig. Die neuen Gate-0-Records bleiben *binary-first*, die sechs Altdateien sind nur erlaubte Hash-gepinnte historische Ausnahme.

### Record/JS und DBREC-Werkzeuge — keine neuen Bugs im geprueften Scope

`bsc/engine/crates/record/src/js.rs::exported()` exportiert gezielt die drei JavaScript-relevanten nativen G0.6a-Schemas. Der Test `js_schema_module_is_current` vergleicht `module()` bytegenau mit `bsc/tools/dbrec_schemas.generated.cjs` und fordert `schema.fields.iter().all(|f| f.nested.is_none())`; der JavaScript-Codec behauptet also keinen Nested-Record-Support, den er nicht hat. `dbrec_compare` gibt bei unlesbaren Dateien Exit 2, bei Differences 1 und bei Equality 0; die Provenienz-Tags 1–4 sind sichtbar, zaehlen aber nicht als inhaltliche Unterschiede. `dbrec_show --get` gibt fuer ein fehlendes Feld Exit 3. **Kein qualifizierter Defekt im konkreten Vergleichs-/CLI-Quellpfad gefunden; Tests nicht neu ausgefuehrt.** Die Fehlercode-Maskierung sitzt im aufrufenden Bash-Skript (E8-3), nicht in `dbrec_show` selbst.

### Abnahme-/Testmatrix Etappe 8 und Gesamtuebergabe an Implementierer

**C2 zuerst sicher spezifizieren und dann implementieren:** E8-1 + E8-4 als **ein gemeinsames** Ordering-/Loss-/Journal-Wire-Versions-Design, anschliessend E8-2 Writer-Poison/Durable-Owner; jedes eingefuehrte Format braucht alte V1-Goldens und negatives Szenario-Coverage. **G0.7-Tool separat/eng:** E8-3 zusammen mit E3-2; keine unerlaubte Full-R0-Requal und keine JSON-Runtime-Abkuerzung. Bestehende `legacy_import`- und `record/js`-Quellen lediglich durch Regressionen abnehmen, keine „fehlende Importdatei“-Baustelle aufmachen.

**Gezielte Kommandos nach Implementierung (hier nicht ausgefuehrt):**

~~~bash
# Von Repo-Root
bash -n bsc/tools/g06a_final_dry.sh
(cd bsc/engine && cargo test --locked -p darkbot-bsc-journal)
(cd bsc/engine && cargo test --locked -p darkbot-bsc-record)
(cd bsc/engine && cargo test --locked -p darkbot-bsc-legacy-import)
(cd bsc/engine && cargo fmt --all -- --check)
# Der nur lesende Import-Reproduktionstest aus dem Repo-Root:
# bsc/engine/target/debug/g07_legacy_import --check
~~~

Wo fuer einen neuen GAP-Vertrag historische Goldenframes fehlen, diesen Blocker **offen** als `NO_HISTORICAL_GOLDEN` dokumentieren und keine Kompatibilitaet erfinden. Nicht alle Tests auf einmal blind als bestandenen C2-Gate deklarieren. Keine Session-/Wallet-/Token-Secrets und keine LIVE-Transaktion in Testprotokollen.

**Abschluss der acht Fable-Etappen:** Die *gelieferten* Etappen 1–8 sind mit ihren jeweils ausdrucklich dokumentierten Abdeckungsgrenzen auf konkreten `main`-Code abgebildet. **Dies ist eine technisch gepruefte Bug-/Fix-Spezifikation, nicht die Behauptung, dass Fixes bereits umgesetzt oder durch Testlaeufe verifiziert wurden.** Unverbindliche/teilweise widerlegte Fable-Patches (insbesondere E2-1 pauschal ADAPT, E2-6a „kein Produkt-Timing“, E6-3/E7-7 „zwei Fences genuegen“ und E8-1 unversionierter GAP-Span) duerfen **nicht** als sichere Copy-Paste-Reparatur angewandt werden.

**Nach dieser Etappe:** Coding-Agent soll vor jedem Paket echten HEAD und PHASENPLAN/Measurement-Ledger aktualisieren, einen minimalen Fixplan je betroffenem Owner liefern, passende Red-/Green-Tests offline ausfuehren, ausschliesslich auf `main` committen, bei G0.7/G0.8 `UNARMED` und No-Send aufrechterhalten und **nur nachweislich getestete** Tickets als FIXED markieren. C1/C2-Coverage-/Runtime-/Journal-Verbesserungen sind erst spaeter zu qualifizieren und duerfen den G0.7-Release nicht durch ungerechtfertigte Umplanung blockieren.


## 10. Nachfolgende Etappen

Weitere durch den Nutzer gelieferte Fable-Etappen werden in **dieselbe Datei** aufgenommen, aber erst nach unabhaengigem Code-/Phasenplanabgleich. Bereits behandelte Behauptungen nicht doppelt als Bug anlegen. Auch nach erfolgreicher Umsetzung bleibt eine Etappe „FIXED / TESTED“ erst mit konkretem Commit und Tests/Evidence belegbar; sonst `OPEN`.

| Etappe | Geplanter Gegenstand | Stand |
| --- | --- | --- |
| 1 | G0.7 Guard/Vertrag/Readiness/Requal | **REVIEWED / FIXES OPEN** |
| 2 | G0.8 Verifikation/Witness/Evidence | **REVIEWED / FIXES OPEN** |
| 3 | G0.6a DRY-/Requal-Werkzeuge | **REVIEWED / FIXES OPEN; SALT A/B UNDECIDED** |
| 4 | Trading/Signer/RLP/Intent | **4a + 4b REVIEWED; E4-3/E4-4 OPEN; 4b future C2/C3 items parked** |
| 5 | Execution/Submission/TLS | **REVIEWED / E5-2..E5-4 FIXES OPEN; E5-1 C2 GATE; E5-5/6 DEFERRED** |
| 6 | geth_ipc Rust | **PARTIAL REVIEWED / E6-1/E6-2 C1/C2 INTEGRATION OPEN; E6-3 MEMORY PROOF OPEN** |
| 7 | Go-geth Overlay/Rust-Paritaet | **PARTS 1+2 REVIEWED / E7-5 SECURITY FIX & E7-6 PERFORMANCE OPEN; E7-7/E6-3 MEMORY PROOF OPEN; E7-3 NO BUG** |
| 8 | Journal/Records/Legacy | **REVIEWED / E8-1/E8-4 ORDER+GAP OPEN; E8-2 WRITER OPEN; E8-3 SHELL OPEN; LEGACY_IMPORT FOUND/REVIEWED** |
