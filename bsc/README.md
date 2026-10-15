# BSC — schlanke Produktionsarchitektur und verbindlicher Umstrukturierungsplan

**Status: STRUKTURREFORM R1–R8 UMGESETZT (2026-10-15) — ABNAHME UND COMMIT DURCH DEN NUTZER AUSSTEHEND.** Ist-Stand, Abweichungen und Prüfprotokoll: §7.  
**Stand: 2026-10-15.** Maßgeblich sind der tatsächliche Git-Stand und die existierenden Nachweise, nicht diese Zielgrafik als vermeintlicher Ist-Zustand.

> **Auftrag für Claude Code:** Das bestehende BSC-Projekt **vollständig und schrittweise** in eine schlanke, funktionsfähige Architektur umstrukturieren. Alles weiterhin Benötigte aus `bsc/gates/gate0/` und anderen Altpfaden übernehmen, alle Referenzen korrigieren, **jede umgebaute Gruppe sofort prüfen** und erst nach PASS mit der nächsten Gruppe fortfahren. Keine zweite aktive Gate0-Engine, keine bloße Ablage von altem Code an einer anderen Stelle. Historisches nach `prestudy/bsc/` **außerhalb des produktiven BSC-Projekts**. Kein Codeverlust, keine stillen Änderungen an Handelssemantik, Nachweisen, Sicherheitsgates oder Hot-Path.

## 1. Ziel und feste Architekturentscheidungen

1. **Genau ein produktiver Cargo-Workspace:** `bsc/native/Cargo.toml` mit `bsc/native/Cargo.lock` und `bsc/native/rust-toolchain.toml`. **Kein** zusätzliches Root-`Cargo.toml` in `bsc/`; das eigenständige `bsc/gates/gate0/Cargo.toml` und sein Lockfile entfallen **nach der qualifizierten Migration**.
2. **Eine produktive Implementierung pro Verantwortung:** Fachliche Semantik in Core, Wire-Encoding in Wire, geth↔Rust-Hot-Path in Fastlane, Signierung/Transport/Dispatch im Runtime-Crate, Capture/Replay in Capture, unabhängige reine Qualifikation in Qualify. Bestehende Funktionen wiederverwenden; keine zweite BUY-, Quote-, Decoder- oder State-Engine erzeugen.
3. **G0.7/G0.8 funktionieren nach dem Umbau weiterhin.** Die Namen bezeichnen fachliche Qualifikationsschritte, **keinen dauerhaft eigenen Workspace**. G0.7 ist ein streng bewachter, kontrollierter Testpfad, nicht der generische LIVE-Bot. G0.8 und der Producer-Witness-Prüfer sind unabhängig read-only.
4. **Kein überflüssiger Baum:** Keine generischen Ordner `operations/`, `scripts/`, `gates/`, `migrations/`, `src/`, `exports/` oder ein zweites zentrales `tests/` als Endzustand. Keine leeren Zukunftsordner. Terminal-Helfer unter `tools/`, Crate-Tests bei ihren Crates.
5. **Historisches außerhalb `bsc/`:** `prestudy/bsc/` enthält ältere Harnesses, nicht mehr genutzte Prototypen, alte Handoffs, Originaldaten und Referenzquellen. Die Prestudie ist **kein Build-Member**, kein Laufzeitimport und kein Standard-Suchpfad des Bots.
6. **Phasenplan ≠ Verzeichnisstruktur:** G0.1–G0.8, C0–C3 sowie D–K bleiben als Entwicklungs-/Abnahmeschritte bestehen, erzeugen aber **keine Ordner pro Phase**. Keine Phase durch bloßes Verschieben als PASS werten.
7. **Bestehende Produktvorgaben bleiben maßgeblich:** Direkter On-Chain-BUY, kein Website-/WSS-/JSON-Zwang im zeitkritischen Hot-Path; erfolgreicher Token-Erwerb ist etwas anderes als Signatur, Submission, Builder-ACK oder lokaler Handoff. `T0 → T_buy < 1 ms` bleibt eine **nicht belegte Produktanforderung**. Die aktive BSC-Roadmap priorisiert G0.7/G0.8 vor weiterer Produktion/Optimierung. Dieser Strukturplan ist **keine Freigabe** für Mainnet-Sends, Funding oder LIVE.
8. **Andere Projekte nicht verändern:** `project_0/`, `frontend/` und sonstige Bereiche bleiben in dieser Strukturreform unverändert. Eine bestehende reine Frontend-Verknüpfung wird geprüft, aber nicht neu konzipiert.

## 2. Verbindlicher Zielbaum — nach vollständiger Migration

~~~~text
DarkBot/
├── bsc/                                 # NUR das aktive BSC-Produkt
│   ├── README.md                        # dieses Dokument / Einstieg
│   ├── .gitignore                       # lokale Daten, Secrets, Build-Outputs
│   │
│   ├── native/                          # EINZIGER BSC-Rust-Workspace
│   │   ├── Cargo.toml                   # Workspace-Mitglieder, gemeinsame Pins
│   │   ├── Cargo.lock                   # einziger produktiver Dependency-Lock
│   │   ├── rust-toolchain.toml          # aktueller geprüfter Rust-Pin 1.93.1
│   │   ├── crates/
│   │   │   ├── core/                    # State, Venues, Quote, Economics, Intent
│   │   │   ├── wire/                    # RLP / binäre TX-Verträge
│   │   │   ├── fastlane/                # geth↔Rust mmap/SPSC
│   │   │   ├── runtime/                 # Start, Signieren, BUY, Transport, Guards
│   │   │   ├── capture/                 # Journal, Aufnahme, Replay
│   │   │   └── qualify/                 # read-only Analyse, G0.8, Prüf-Binaries
│   │   │       # JEDES Crate: Cargo.toml + src/ + nur benötigte tests/
│   │   └── geth/                        # gepinnter bsc-geth / Go-Overlay
│   │       ├── README.md
│   │       └── overlay/
│   │
│   ├── config/                          # aktive, nichtgeheime Konfiguration
│   │   └── wallet.public.env
│   │       # Vertrags-/Schema-Dateien nur wenn aktive Verbraucher sie benötigen
│   │
│   ├── tools/                           # tatsächliche Shell/Node/Python-Helfer
│   │
│   └── docs/                            # AKTUELLE, maßgebliche Dokumentation
│       ├── PHASENPLAN.md               # fachliche Roadmap und Abnahmebedingungen
│       ├── STATUS.md                   # Code / Nachweise / LIVE getrennt
│       ├── ARCHITECTURE.md             # Datenfluss, Ownership, Schnittstellen
│       └── EVIDENCE.md                 # unveränderliche Nachweise/Originalpfade
│
└── prestudy/                             # GETRENNT, nicht im BSC-Build
    └── bsc/
        ├── README.md                   # Inventar, Herkunft, Commit, Grenzen
        ├── experiments/                # frühere Gate0- und sonstige Versuche
        ├── references/                 # alte Prototypen / Quellvergleiche
        └── evidence/                   # historische unveränderte Artefakte
~~~~

**Keine Platzhalter anlegen:** Falls `config/`, `tools/` oder ein Crate-Unterordner fachlich keine Inhalte benötigt, keine neue Unterstruktur erfinden. Lokale Laufzeitdaten können unter `bsc/data/` entstehen, gehören aber **nicht** als getrackter Baum in Git. Historische Evidence und Daten niemals versehentlich in lokale Build-/Testordner mischen.

### Cargo-Vertrag — identische Platzierung wie Project_0

- Cargo **immer vom Workspace-Root** starten: `cd bsc/native`. Dort wirkt auch `rust-toolchain.toml`; allein `--manifest-path` von einem anderen Arbeitsverzeichnis garantiert nicht dieselbe Rustup-Auswahl.
- Die sechs existierenden Crates werden **als Verzeichnisse** von `darkbot-wire`, `darkbot-core`, `darkbot-fastlane`, `darkbot-runner`, `darkbot-capture` und `darkbot-qualify` auf die obigen kurzen Namen umgestellt. **Cargo-Paketnamen und Rust-Crate-Namen zunächst behalten**, damit kein vermeidbarer Import-/API-Bruch entsteht. Erst nach separater Begründung umbenennen.
- `native/Cargo.toml`-`members`, sämtliche `path = ...`-Abhängigkeiten, Include-/Fixture-Pfade, Build-Skripte, Manifeste, CLI-Aufrufe und Dokumentenlinks vollständig aktualisieren.
- Bestehende `edition = "2024"`, `resolver = "3"`, Rust-Pin `1.93.1` und bewährte Versions-Pins nicht beiläufig ändern. Genau **ein** produktives Lockfile; `--locked` als Normalfall. Keine willkürlichen Dependency-Upgrades oder ungeprüften Release-Profil-Tunings.

## 3. Vor jeder Verschiebung: Datei- und Verbraucherentscheidung

**Jede einzelne bisherige Datei erhält einen Eintrag in einer Migrationsmatrix** (temporär während des Umbaus, am Ende als kompakter Nachweis in der README oder `docs/EVIDENCE.md`):

`ALT-PFAD | AKTIVER VERBRAUCHER/REFERENZ | KLASSE | NEU-PFAD | GESCHÜTZTE HASH-/ABI-/SCHEMA-BINDUNG | PRÜFUNG | RESULTAT`

Klassen:

- **ACTIVE:** im Produkt oder in G0.7/G0.8 derzeit ausgeführter Code / zwingender Vertrag → nach `bsc/native/`, `bsc/config/` oder `bsc/tools/`.
- **TEST:** tatsächlich ausgeführte Regression, Golden-Vector oder notwendiger Counterexample → zu den Tests des zuständigen Crates; unabhängige EVM-Querprüfung als weiterhin aktives Werkzeug behalten.
- **PRESTUDY:** historischer Harness, alter Test, reiner Audit-/Nachweisdump oder referenzierte Altversion → `prestudy/bsc/`, Originalinhalt und Herkunft erhalten.
- **DELETE-CANDIDATE:** doppelt, leer oder nachweislich ohne Nutzer/Consumer → erst nach Repository-/Import-/Manifest-/Script-/Dokumentationssuche und Abnahme entfernen. `git mv` vor Löschen bevorzugen; bei Zweifel erhalten, nicht raten.

**1.062 Dateien unter `bsc/tests/` sind überwiegend JSON-/Evidence-Dateien, keine 1.062 aktiven Unit-Tests.** Aktive kleine Fixtures selektiv übernehmen; die große historische Sammlung nach `prestudy/bsc/evidence/`. Bestehende Originalnachweise in `bsc/gates/gate0/evidence/` ebenso behandeln. Keine Massenübernahme in neue aktive `tests/`- oder `docs/`-Unterbäume.

### Konkrete Pflichtabhängigkeiten — schon im Quellcode nachgewiesen

| Ausgangspunkt | Zielverantwortung | Vor dem Umzug verbindlich |
|---|---|---|
| `bsc/gates/gate0/src/bin/g07_mainnet_self_launch_buy.rs` | `native/crates/runtime/src/bin/` | Benötigte `darkbot_gate0`-Bibliotheksfunktionen durch **echte** Runtime/Core/Capture-APIs ersetzen; kein blinder Namenswechsel |
| `bsc/gates/gate0/src/bin/g08_mainnet_verify.rs` | `native/crates/qualify/src/bin/` | Kanonische Token-/Buyer-/Gas-/Ordering-Verifikation unverändert; ausschließlich read-only |
| `bsc/gates/gate0/src/bin/g08_producer_timing.rs` | `native/crates/qualify/src/bin/` | Nur ursprüngliche Producer-Ausführungszeugen anerkennen, kein ACK/Blockzeitstempel als Sub-ms-Beweis |
| `bsc/gates/gate0/src/acceptance_guard.rs` | `native/crates/runtime/`, mit zwingend zugänglicher Prüfschnittstelle | **`UNARMED` bleibt `UNARMED`**, genaue v2-Bytes, Hash-/Budget-/Wallet-/Pfadbindung und Negativtests erhalten |
| `bsc/gates/gate0/src/qualification_capture.rs` | `native/crates/capture/` / Runtime-Adapter | Bisheriger `#[path = "../../../native/crates/darkbot-capture/src/lib.rs"]`-Import darf nicht bloß mit neuem Pfad weitergeführt werden; echte Crate-Abhängigkeit schaffen |
| `bsc/gates/gate0/src/native_bnb_quote.rs` | bestehender `native/crates/core/src/economics/flap/native_curve20.rs` | Parität beweisen, keine zweite produktive Kurven-/Quote-Logik |
| `bsc/gates/gate0/src/bin/g06a_self_launch_bundle.rs` | nur benötigtes DRY-Werkzeug/qualify | DRY1–DRY4, Null-Egress und Byteidentität erhalten |
| `bsc/gates/gate0/src/bin/g06a_native_bnb_quote.rs` | qualify/Test gegen Core | dieselben Quote-Eingaben/Ausgaben, keine versteckte zweite Formel |
| `bsc/gates/gate0/tools/g06a_preflight_readonly.cjs` | `bsc/tools/` | RPC nur read-only, Pfade/Schemas aktualisieren |
| `bsc/gates/gate0/tools/g06a_economics_evm_crosscheck.cjs` und `g06a_local_simulate.cjs` | `bsc/tools/` | **Unabhängige** lokale EVM-Prüfung erhalten, nicht durch eigene Rust-Quote ersetzen |
| `bsc/tools/g06a_final_dry.sh`, `g07_find_min_executable_buy.sh`, `g07_min_spend_probe.sh`, `g07_pre_live_readiness.sh` | `bsc/tools/` | Alle alten Gate0-Pfade, relativen Arbeitsverzeichnisse, Skriptaufrufe und Guards aktualisieren |
| `bsc/gates/gate0/spec/` und ausgewählte `evidence/*.json` | aktive Verträge nach `bsc/config/`; historische Artefakte nach `prestudy/` | Schema-Inhalt, Originalhashes, Vertragsscope, Consumer und Pfadidentität korrekt übernehmen |
| `bsc/gates/gate0/src/p2p_ingress.rs` und alte G03/G05/G06-Binaries | nur bei **wirklicher** Produkt-/Regressionsverwendung behalten | Veralteten Reth-P2P-Sensor **nicht** als neue Produktionspflicht einbauen; historische Tests in Prestudie |
| `bsc/tests/legacy_gate0/.../node_modules`-bezogene Prüfskripte | aktiver Node/EVM-Werkzeugsatz in `bsc/tools/` | Reproduzierbare `package.json`/Lock-Abhängigkeiten statt Import aus altem Archiv; unabhängige EVM-Parität nachweisen |
| `bsc/src/`, `bsc/migrations/`, `bsc/exports/`, leerer `bsc/data/`-Platzhalter | entfernen, soweit ungenutzt | Consumer- und Git-Ignore-Prüfung |
| veralteter Root-Pfad `gates/bsc/gate0/src/p2p_ingress.rs` | Prestudie oder selektiv Regression | Altbestand außerhalb `bsc/` mitprüfen; **nicht** als unbemerkt aktive zweite Quelle zurücklassen |

**Besonders kritisch:** `V2_RELATIVE_PATH` im Acceptance-Guard ist aktuell ein **exakt geprüfter** alter Pfad nach `bsc/gates/gate0/evidence/g01_mainnet_acceptance_proposal_v2.json`. Dieser Pfad, seine Tests sowie fest verdrahtete Shell-/Node-Referenzen müssen **zusammen** migriert werden. Die v2-Vertragsdatei selbst darf sich dabei nicht stillschweigend ändern. Bestehende Hashes in historischen Nachweisen unverändert lassen; neue Pfade separat nachvollziehbar indizieren. Falls eine Pfadidentität selbst Teil eines eingefrorenen Beweises ist, nicht behaupten, die bloße Pfadverschiebung sei beweisidentisch; benötigte neue Qualifikation ausdrücklich durchführen.

## 4. Acht Migrationsgruppen — JEDE Gruppe separat prüfen

**Regel:** Vor jeder Gruppe gezielt Verbraucher und alte/neu geplante Pfade inventarisieren. Dann ausschließlich die definierte Gruppe ändern, Referenzen **in derselben Gruppe** korrigieren, zielgerichtet testen, PASS/FAIL mit betroffenen Dateien dokumentieren. **Kein Weitermachen bei FAIL.** Keine parallele Bearbeitung derselben Cargo-/Guard-/Phasenplan-Dateien durch mehrere Agenten.

| Gruppe | Was Claude umsetzt | Pflichtprüfung vor der nächsten Gruppe |
|---|---|---|
| **R1 — Inventur/Baseline** | Aktuelle Git-SHAs, schreibgeschützte Datei-/Verbrauchermatrix, aktive G07/G08-Binärziele, Guards, Dependencies, Testkommandos und Beweishashes erfassen; **noch nichts verschieben** | Ist-Workspace-Cargo-Metadaten; geprüfte Offline-Baseline und anwendbare Tests; keine Änderung außerhalb Dokumentations-/Inventurdateien |
| **R2 — Verträge und Referenzpfade** | Autoritative Konfiguration, v2-Acceptance, Builder-Floor, Schemas und aktive Evidenzverweise klassifizieren; Zielpfade, Pfad-/Hash-Übergang und alle Nutzer festlegen; atomare Dokument-/Schema-Migration | JSON-/Schema-/Pfadkonsistenz, Guard-Negativfälle, Hash-Parität der unveränderten Originaldateien, Tool-Syntax |
| **R3 — Unterstützende Funktionen** | Gate0-Quote, Capture-Adapter, DRY-/EVM-Prüfungen und benötigte Libraries in die bestehenden produktiven Crates bzw. `bsc/tools/` übernehmen; unabhängige EVM-Implementierung als Gegenprüfung behalten | Cargo-Check betroffener Crates, Quote-/DRY-/Journal-Parität, EVM-Crosscheck (soweit lokal reproduzierbar) ohne Live-Send |
| **R4 — G0.7** | Kontrollierten Mainnet-BUY-Binary in Runtime-Workspace übernehmen, mit vorhandenen sicheren Bibliotheken verbinden; alle alten Manifest-/Pfad-Referenzen korrigieren | Zielbinary baut; Guard bleibt `UNARMED`; negative Autorisierungs-, Budget-/Hash-/Wallet-/Nonce-/Gas- und No-Send-Tests; exakt gebundene DRY-Parameter |
| **R5 — G0.8** | Read-only Acquisition-/Receipt-/Ordering-Prüfer und unabhängigen Producer-Timing-Validator in Qualify übernehmen | Beide Binaries bauen; synthetische PASS/FAIL/UNKNOWN-Gegenbeispiele; Erwerb ≠ Receipt/ACK, Producer-Witness-Zuordnung, Capture-Parität |
| **R6 — Historische Trennung** | Nur tatsächlich aktive Test-Fixtures zu Crate-Tests; alte G03–G06-Harnesses, Daten, Berichte, Duplikate, alte Root-`gates/`-Restbestände und Referenzen nach `prestudy/bsc/`; zweiten Gate0-Workspace entfernen | Git-Suche nach **allen** alten Pfaden; komplette notwendige Offline-G07/G08-Toolkette aus neuem Ort; kein verbleibender aktiver Import aus `prestudy/` |
| **R7 — Cargo-/Tool-/Doku-Vereinheitlichung** | Sechs Crate-**Ordner** in `core/wire/fastlane/runtime/capture/qualify` umbenennen (Paketnamen zunächst gleich); Workspace-`members`, `path`, Includes und Tests umstellen; Doku und `bsc/README.md` mit tatsächlichem Stand synchronisieren | `cd bsc/native`, Toolchain, Cargo-Metadaten, `cargo check --locked --workspace`, gezielte Integration/Tool-Syntax; keine Funktionalitäts- oder Hot-Path-Drift |
| **R8 — Abschluss** | Überflüssige Platzhalter löschen; `bsc/gates/`/`bsc/tests/`/Alt-Skripte vollständig beseitigen, soweit sicher migriert; aktive normativ bindende Dokumente verlustfrei konsolidieren; finale Ist-Baum-Navigation und Nachweismatrix | Gesamte reproduzierbare Offline-Regression, alle relevanten Binaries, Format/Clippy, Gate-Guards, Evidence-/Hash-Konsistenz, Null-Egress; Diff ausschließlich im erlaubten Scope |

**Reihenfolge bei echter Abhängigkeit anpassen, aber nie Prüfgrenzen überspringen.** R6 darf keine noch von R4/R5 benötigte Datei aus dem Zugriff reißen. Eine Hilfsfunktion darf provisorisch vorhanden sein, aber am Ende bleibt pro Semantik nur **ein produktiver Eigentümer**. Nicht jeder ursprüngliche Gate0-Binary wird als Produktionsbinary übernommen.

### Prüfökonomie — verbindliches Limit: maximal 10 Testdurchläufe INSGESAMT

- **Umbau → passende Prüfung → nächster Umbau → passende Prüfung.** Nach jeder Gruppe den veränderten Pfad durch Build, Syntax-/Referenzprüfung und – wenn ausführbarer Code betroffen ist – durch einen gezielten Funktionstest absichern. Bei FAIL ausschließlich die betroffene Gruppe korrigieren; niemals fehlerhaft weiterbauen.
- **Harte Obergrenze: höchstens ZEHN (10) tatsächliche Testdurchläufe insgesamt für R1–R8**, nicht zehn volle Suiten plus beliebig viele Einzeltests. Jeder Aufruf von `cargo test`, `pytest`, `unittest`, einem Integrations-/Regressionstest oder einem ausführbaren Testharness zählt als **ein Testdurchlauf**, auch wenn intern viele Testfälle enthalten sind. Tests nach Möglichkeit pro Gruppe sinnvoll bündeln. Kompilierung (`cargo check`), Metadaten, `git diff --check`, `bash -n`, `node --check` und `cargo fmt --check` sind kurze Struktur-/Syntaxprüfungen, keine vollständigen Testdurchläufe; auch diese nicht unnötig wiederholen.
- **Budgetplanung:** maximal ein gezielter Testdurchlauf pro R1–R8-Gruppe und höchstens zwei zusätzliche Tests für integrationskritische Übergänge bzw. eine finale Workspace-Regression. R1 kann sich bei unverändertem Code auf die Baseline stützen, damit ein Durchlauf frei bleibt. Pro Gruppe schriftlich vermerken: `TEST_COUNT=x/10`, konkrete Befehle, PASS/FAIL, was ungetestet blieb. **Kein Testbudget mit immer gleichen Wiederholungen verbrauchen.** Wenn zehn Durchläufe ausgeschöpft sind, keine weiteren ohne neue Nutzerfreigabe; offene Risiken transparent melden. Keine endlosen 50–100 Läufe, keine unangeforderten Stresstests, Benchmarks oder Live-/Mainnet-Tests.
- Bis zur Abschlussphase **keine Performance-Optimierung und keine Modulzerlegung allein wegen Dateilänge**. `fastlane/src/v2.rs` und andere Hot-Path-Layouts zunächst semantisch unverändert verschieben; Timing nur bei gemessener, qualifikationspflichtiger Beeinflussung separat prüfen.
- Fehlende RPC-State-Daten sind `RPC_STATE_UNAVAILABLE`/`INCOMPLETE`, **kein** Grund für stillschweigende Schutzumgehung, künstlichen PASS oder unendliche Wiederholungen. Read-only Online-Abgleiche nur soweit tatsächlich für freigegebene Qualifikation benötigt.

**Referenzbefehle für den einzigen Cargo-Workspace** (lokal und ohne beabsichtigte value-bearing Transaktion):

~~~~bash
cd bsc/native
rustup show active-toolchain
cargo metadata --locked --no-deps --format-version 1
cargo check --locked --workspace
# Nach festgestellter Test-Sicherheit und nur an passenden Prüfpunkten:
cargo test --locked --workspace
cargo fmt --all -- --check
~~~~

Bei echten Änderungen an einem Rust-Modul sind dessen eigene Tests zu verwenden; bei Skriptumzügen u.a. `bash -n`, `node --check`, Python-Syntax-/Unit-Tests und mindestens ein repräsentativer **Offline-Aufruf** über die neue Pfadstruktur. Exakte Toolaufrufe ergeben sich aus der R1-Inventur und dürfen nicht frei erfunden werden.

## 5. Harte Sicherheits-, Semantik- und Datenregeln

- **Keine Echtgeld-/Mainnet-Sends, kein Funding, keine LIVE-Autorisierung** durch den Refactor. Insbesondere `FROZEN_APPROVED_V2_SHA256 = "UNARMED"`, `approved=false` und `execution_ready=false` bleiben unverändert; niemand darf die Guard-Prüfungen zum Zweck des Umbaus lockern.
- `g07_mainnet_self_launch_buy` ist strikt getrennt vom regulären produktiven Runtime-Modus; kein autonomer Retry oder Probe-Send. Eine spätere werttragende G0.7-Ausführung verlangt eigene explizite Freigabe, aktuelle DRY-Qualifikation und vollständig fixierten Spend.
- `g08_mainnet_verify` und `g08_producer_timing` dürfen nicht senden; echte Tokenakquisition, Reihenfolge und Gebühren sind separat nachzuweisen; **keinen** lokalen Sende- oder Blockzeitpunkt als `T_buy` ausgeben.
- Signaturbytes, Kaufparameter, Profit-/Gas-Grenzen, Nonce, Slippage, Provider-/Bundle-Semantik, Memory-Layout, SPSC-/Wire-Versionen, Preis-/Quote-Arithmetik, Crash-/Restart-Verhalten und alle relevanten Version-Pins unverändert halten, außer eine explizit dokumentierte qualifizierte Migration erfordert das Gegenteil.
- Historische Evidence **niemals inhaltlich bearbeiten** und keine Hashes neu erfinden. Bei Standortwechsel Quellcommit, ursprünglichen Pfad, Inhaltshash, neuen Pfad und neue Consumer-Referenzen manifestieren. Referenzen, die einen unveränderlichen alten Pfad meinen, bleiben historisch als solche erkennbar.
- Git-Hygiene: keinen `git clean`, keinen Hard-Reset, keine massenhaften `git add .`/`git add -A`, keine ungewollten Änderungen an lokalen Rohdaten/`node_modules`/Secrets. Gezielt `git mv` und nur erlaubte Pfade stagen.
- **Nur vom Nutzer autorisierte Git-Commits/Pushs:** Claude berichtet je Gruppe geänderte Dateien, konkrete Checks, PASS/FAIL, offene Risiken und nächsten Schritt; Commit/Push durch den Nutzer bzw. nach eigenständiger Freigabe.
- Die aktuelle fachliche Roadmap [BSC_DUAL_VENUE_ROADMAP.md](docs/BSC_DUAL_VENUE_ROADMAP.md), [GATE0_TESTPLAN_SHORT.md](docs/GATE0_TESTPLAN_SHORT.md), [BSC_PRODUCTION_BUILD_MASTER_PLAN.md](docs/BSC_PRODUCTION_BUILD_MASTER_PLAN.md) und technische Verträge bleiben **solange autoritativ**, bis die vollständige nachweisbare Übernahme in den neuen Phasenplan bestätigt ist. Eine kurze Zusammenfassung **ersetzt keinen** vollständigen verbindlichen Vertrag.
- **Verbindliche Rangfolge:** Der bestehende fachliche BSC-Phasenplan und seine technischen Verträge sind die **inhaltliche Basis und fachliche Autorität**; diese README legt **ausschließlich** das neue Datei-/Cargo-/Test-Layout und den schrittweisen Umzug fest. Bei einem Konflikt darf Claude den Phasenplan weder abschwächen noch stillschweigend umdeuten; die betroffene Stelle mit Lösung dokumentieren. Die ausdrückliche Nutzeranweisung erlaubt jetzt den **reinen Strukturumbau R1–R8 als begrenzte Ausnahme von der bisherigen Arbeitsreihenfolge**, nicht aber vorgezogene C1/C2-Featurearbeit oder eine Änderung der fachlichen G0.7/G0.8-Anforderungen. Der Umbau ist keine LIVE-, Budget-, Mainnet- oder Wirtschaftsfreigabe.

## 6. Abnahme: Wann gilt die Strukturreform als fertig?

**Fertig nur, wenn alle Bedingungen erfüllt sind:**

1. Der reale Repository-Baum entspricht dem Ziel; kein aktiver `bsc/gates/`-Workspace und keine zweite produktive BUY-Implementierung.
2. `bsc/native/Cargo.toml` ist die **einzige** produktive BSC-Workspace-Wurzel; `Cargo.lock` und Toolchain stimmen; alle sechs Crates bauen.
3. G0.7-, G0.8-, DRY-, EVM-/Quote- und Evidence-Funktionen sind an neuen Orten **nachweisbar weiterhin vorhanden und aufrufbar**. Nicht ausgeführte Echtgeldtests werden **nicht** als PASS bezeichnet.
4. Die `prestudy/bsc/` ist vom Produkt getrennt; alle archivierten Artefakte sind über Originalpfad, Commit/Hash auffindbar; keine produktive Abhängigkeit zeigt hinein.
5. Keine verwaisten aktiven Pfade in Rust, Cargo, Shell, Node, Python, Docs oder README; absolute/relative Arbeitsverzeichnisse stimmen.
6. Produkt-Contracts, gebundene Beweise, Nicht-Send-Verhalten, Guards, Scope und Wirtschaftlichkeits-/Zeitdefinitionen sind erhalten; kein stiller LIVE-Statuswechsel.
7. Jeder der acht Gruppen liegt ein verständlicher, kurzer Prüfbericht mit Ziel, Diff, Testergebnis und nächster Freigabe zugrunde. Am Ende ist die Gesamtprüfung grün.

**Arbeitsbeginn für Claude:** Der Nutzer führt zuerst `git pull --ff-only origin main` aus (nur bei geeignetem Git-Status). Danach liest Claude dieses README, den verbindlichen fachlichen BSC-Phasenplan, den aktuellen HEAD, beide Cargo-Manifeste sowie die tatsächlichen Verbraucher von G0.7/G0.8. **R1 bis R8 in der beschriebenen Reihenfolge umsetzen, jeweils umbauen, Referenzen prüfen, höchstens einen gezielten Test pro Gruppe und erst bei PASS weiterarbeiten.** Maximal zehn Tests insgesamt. Kein selbstständiger Commit/Push, kein überflüssiges Testing und niemals eine value-bearing Transaktion.

## 7. Umsetzungsstand R1–R8 (2026-10-15)

Ausgangscommit `7f3c08b`. Vollständige Migrationsmatrix, Originalpfade, Hash-Nachweise und Prüfprotokoll: [docs/EVIDENCE.md](docs/EVIDENCE.md). Einstieg: [docs/PHASENPLAN.md](docs/PHASENPLAN.md), [docs/STATUS.md](docs/STATUS.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

**Ist-Baum** (entspricht §2 bis auf die begründeten Abweichungen unten):

~~~~text
bsc/
├── README.md · .gitignore            # /.local/ (lokale Secrets-Ablage/Backups), /data/ (Laufdaten)
├── native/                           # einziger Cargo-Workspace
│   ├── Cargo.toml · Cargo.lock · rust-toolchain.toml (1.93.1)
│   ├── crates/{core,wire,fastlane,runtime,capture,qualify}   # Paketnamen unverändert (darkbot-*)
│   ├── geth/                         # README, Pin, Wire-V1/V2-Spezifikation, overlay/
│   └── evidence/                     # Abweichung: pfadgebundene Q5A-/C1-Nachweise
├── config/                           # v2-Acceptance, Builder-Floor, Preflight-Baseline, Witness-Schemas, wallet.public.env
├── tools/                            # G0.6a/G0.7-Skripte, EVM-Crosscheck (+ npm-Locks), C1-Interop/-Freeze, lock_qualify
└── docs/                             # PHASENPLAN, STATUS, ARCHITECTURE, EVIDENCE + alle gültigen Verträge, ACCEPTANCE_MAP.json
prestudy/bsc/{experiments,references,evidence}/   # historisch, kein Build-Member
~~~~

**Begründete Abweichungen vom Zielbaum**

- `native/evidence/` bleibt: Pfad und SHA-256 der Q5A-Evidence sind als Qualifikationsidentität im Runtime-Transport einkompiliert (`Q5A_TLS_EVIDENCE_PATH`, Generation 1); ein Umzug wäre eine Neuqualifikation.
- `native/geth/` enthält neben `README.md`/`overlay/` die aktiven Wire-Spezifikationen und den geth-Pin.
- Aktive Verträge bleiben als eigene Dateien in `docs/`; die vier Einstiegsdokumente fassen sie nicht verbindlich neu.

**Funktionen am neuen Ort**

| Funktion | Ort | Aufruf (aus `bsc/native` bauen, aus dem Repo-Root starten) |
|---|---|---|
| G0.7 (kontrolliert, `UNARMED`) | `crates/runtime/src/bin/g07_mainnet_self_launch_buy.rs` | `cargo build --locked -p darkbot-runner --features g07-mainnet --bin g07_mainnet_self_launch_buy` |
| v2-Acceptance-Guard | `crates/runtime/src/acceptance_guard.rs` | Feature `acceptance-guard`; Tests `crates/runtime/tests/acceptance_guard.rs` |
| G0.8 Acquisition/Ordering, Producer-Timing | `crates/qualify/src/bin/g08_mainnet_verify.rs`, `g08_producer_timing.rs` | `cargo build --locked -p darkbot-qualify --bins` |
| G0.6a DRY (Quote über Core, Bundle) | `crates/qualify/src/bin/g06a_native_bnb_quote.rs`, `g06a_self_launch_bundle.rs` | `bash bsc/tools/g06a_final_dry.sh` (benötigt Secrets + read-only RPC) |
| Unabhängige EVM-Gegenprüfung | `tools/g06a_economics_evm_crosscheck.cjs`, `g06a_local_simulate.cjs` | `npm ci` in `tools/evm-audit-tools/` und `tools/abi-tools/` |

**Prüfprotokoll (10-Test-Budget, 9 genutzt):** T1 Baseline · T2 Guard (20+6) · T3 Quote-Byte-Parität 7/7 · T4 G0.7/Capture/Guard (21+6+4) · T5 G0.8/qualify (3+4+6+26) · T6 Workspace **426 PASS / 0 FAIL / 5 ignored** · T7 C1-Tool-Tests PASS · T8 Nachtest nach `cargo fmt` (154 PASS) · T9 Offline-Toolkette **17/17**. Struktur: `cargo metadata --locked`, `cargo check --locked --workspace`, `cargo fmt --all -- --check` sauber, `cargo clippy` ohne Fehler (6 Stil-Warnungen im unverändert übernommenen G0.7/G0.8-Code), `bash -n`/`node --check`/Python-Syntax aller Tools.

**Nicht ausgeführt, daher nicht PASS:** voller G0.6a-DRY, EVM-Crosscheck gegen RPC, `g07_pre_live_readiness.sh`, jede G0.7-Ausführung, G0.8 gegen echte Blöcke. Vor einer Scharfschaltung ist ein neuer DRY-Lauf nötig (der DRY-Report enthält den neuen v2-Pfad).

**Offen:** flaky `fastlane`-Test `role_leases_are_exclusive_per_role_and_coexist_across_roles` (einmal rot in der Baseline vor jeder Änderung); nicht eingebundene C1-Vorarbeit (untracked Fastlane-/Go-Dateien, Stash „local-before-p2p-fix-20261011“ `5753f70`) ist erhalten, muss nach dem Commit der Reform beim Einspielen auf die neuen Ordner (`crates/fastlane`, `tools/`) abgeglichen werden.
