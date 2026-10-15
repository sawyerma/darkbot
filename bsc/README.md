# BSC — Produktionsarchitektur, fachliche Benennung und Refactoring-Aufträge

**Stand: 2026-10-16.** **R1–R8 wurde umgesetzt und als `c8362c6` nach `main` gepusht.** Seit `1c99127` verwendet der Iststand `bsc/engine/` mit den sechs fachlich benannten Crates (`bsc/native/` enthält nur noch gebundene Evidence). **R9–R12 ist als `1c99127` committet. C1.5–C1.8 sind integriert und Wire V2 ist in `protocol_v2/` gegliedert (§10), committet als `0d538b5`. Der C1-Pfad ist noch **nicht** an die produktive `execution`-Engine angeschlossen; C1.9/C1.9b-Qualifikation und C1.10-Freeze sind offen.**

> **CLAUDE CODE — ZUERST §8 LESEN.** §8 ist die verbindliche **neue** Zielarchitektur und Arbeitsanweisung. §§1–4, 6–7 dokumentieren die **abgeschlossene** Migration R1–R8 und sind kein Auftrag, diesen Umbau noch einmal auszuführen. Die Sicherheits- und Datenregeln aus §5 bleiben zwingend gültig. Der fachliche Phasenplan und die technischen Verträge bleiben maßgeblich; keine Umstrukturierung autorisiert LIVE oder eine value-bearing Transaktion. **Nur `bsc/` und bei nachgewiesenem Bedarf `prestudy/bsc/` ändern; `project_0/`, Frontend und andere Projekte niemals anfassen.**

## 1. R1–R8: bisherige Architekturentscheidungen (abgeschlossen, historisch)

1. **Genau ein produktiver Cargo-Workspace:** `bsc/native/Cargo.toml` mit `bsc/native/Cargo.lock` und `bsc/native/rust-toolchain.toml`. **Kein** zusätzliches Root-`Cargo.toml` in `bsc/`; das eigenständige `bsc/gates/gate0/Cargo.toml` und sein Lockfile entfallen **nach der qualifizierten Migration**.
2. **Eine produktive Implementierung pro Verantwortung:** Fachliche Semantik in Core, Wire-Encoding in Wire, geth↔Rust-Hot-Path in Fastlane, Signierung/Transport/Dispatch im Runtime-Crate, Capture/Replay in Capture, unabhängige reine Qualifikation in Qualify. Bestehende Funktionen wiederverwenden; keine zweite BUY-, Quote-, Decoder- oder State-Engine erzeugen.
3. **G0.7/G0.8 funktionieren nach dem Umbau weiterhin.** Die Namen bezeichnen fachliche Qualifikationsschritte, **keinen dauerhaft eigenen Workspace**. G0.7 ist ein streng bewachter, kontrollierter Testpfad, nicht der generische LIVE-Bot. G0.8 und der Producer-Witness-Prüfer sind unabhängig read-only.
4. **Kein überflüssiger Baum:** Keine generischen Ordner `operations/`, `scripts/`, `gates/`, `migrations/`, `src/`, `exports/` oder ein zweites zentrales `tests/` als Endzustand. Keine leeren Zukunftsordner. Terminal-Helfer unter `tools/`, Crate-Tests bei ihren Crates.
5. **Historisches außerhalb `bsc/`:** `prestudy/bsc/` enthält ältere Harnesses, nicht mehr genutzte Prototypen, alte Handoffs, Originaldaten und Referenzquellen. Die Prestudie ist **kein Build-Member**, kein Laufzeitimport und kein Standard-Suchpfad des Bots.
6. **Phasenplan ≠ Verzeichnisstruktur:** G0.1–G0.8, C0–C3 sowie D–K bleiben als Entwicklungs-/Abnahmeschritte bestehen, erzeugen aber **keine Ordner pro Phase**. Keine Phase durch bloßes Verschieben als PASS werten.
7. **Bestehende Produktvorgaben bleiben maßgeblich:** Direkter On-Chain-BUY, kein Website-/WSS-/JSON-Zwang im zeitkritischen Hot-Path; erfolgreicher Token-Erwerb ist etwas anderes als Signatur, Submission, Builder-ACK oder lokaler Handoff. `T0 → T_buy < 1 ms` bleibt eine **nicht belegte Produktanforderung**. Die aktive BSC-Roadmap priorisiert G0.7/G0.8 vor weiterer Produktion/Optimierung. Dieser Strukturplan ist **keine Freigabe** für Mainnet-Sends, Funding oder LIVE.
8. **Andere Projekte nicht verändern:** `project_0/`, `frontend/` und sonstige Bereiche bleiben in dieser Strukturreform unverändert. Eine bestehende reine Frontend-Verknüpfung wird geprüft, aber nicht neu konzipiert.

## 2. R1–R8: damaliger Zielbaum (umgesetzt; für R9–R13 durch §8 abgelöst)

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

### Bisheriger Cargo-Vertrag aus R1–R8 (nur Iststand bis zur R9–R13-Migration)

- Cargo **immer vom Workspace-Root** starten: `cd bsc/native`. Dort wirkt auch `rust-toolchain.toml`; allein `--manifest-path` von einem anderen Arbeitsverzeichnis garantiert nicht dieselbe Rustup-Auswahl.
- Die sechs existierenden Crates werden **als Verzeichnisse** von `darkbot-wire`, `darkbot-core`, `darkbot-fastlane`, `darkbot-runner`, `darkbot-capture` und `darkbot-qualify` auf die obigen kurzen Namen umgestellt. **Cargo-Paketnamen und Rust-Crate-Namen zunächst behalten**, damit kein vermeidbarer Import-/API-Bruch entsteht. Erst nach separater Begründung umbenennen.
- `native/Cargo.toml`-`members`, sämtliche `path = ...`-Abhängigkeiten, Include-/Fixture-Pfade, Build-Skripte, Manifeste, CLI-Aufrufe und Dokumentenlinks vollständig aktualisieren.
- Bestehende `edition = "2024"`, `resolver = "3"`, Rust-Pin `1.93.1` und bewährte Versions-Pins nicht beiläufig ändern. Genau **ein** produktives Lockfile; `--locked` als Normalfall. Keine willkürlichen Dependency-Upgrades oder ungeprüften Release-Profil-Tunings.

## 3. R1–R8: historische Datei- und Verbraucherentscheidung

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

## 4. R1–R8: abgeschlossene Migrationsgruppen (nicht erneut ausführen)

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

### Historisches Testbudget R1–R8: höchstens 10 Durchläufe insgesamt

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

## 6. R1–R8: damalige Abnahmekriterien (für R9–R13 siehe §8)

**Fertig nur, wenn alle Bedingungen erfüllt sind:**

1. Der reale Repository-Baum entspricht dem Ziel; kein aktiver `bsc/gates/`-Workspace und keine zweite produktive BUY-Implementierung.
2. `bsc/native/Cargo.toml` ist die **einzige** produktive BSC-Workspace-Wurzel; `Cargo.lock` und Toolchain stimmen; alle sechs Crates bauen.
3. G0.7-, G0.8-, DRY-, EVM-/Quote- und Evidence-Funktionen sind an neuen Orten **nachweisbar weiterhin vorhanden und aufrufbar**. Nicht ausgeführte Echtgeldtests werden **nicht** als PASS bezeichnet.
4. Die `prestudy/bsc/` ist vom Produkt getrennt; alle archivierten Artefakte sind über Originalpfad, Commit/Hash auffindbar; keine produktive Abhängigkeit zeigt hinein.
5. Keine verwaisten aktiven Pfade in Rust, Cargo, Shell, Node, Python, Docs oder README; absolute/relative Arbeitsverzeichnisse stimmen.
6. Produkt-Contracts, gebundene Beweise, Nicht-Send-Verhalten, Guards, Scope und Wirtschaftlichkeits-/Zeitdefinitionen sind erhalten; kein stiller LIVE-Statuswechsel.
7. Jeder der acht Gruppen liegt ein verständlicher, kurzer Prüfbericht mit Ziel, Diff, Testergebnis und nächster Freigabe zugrunde. Am Ende ist die Gesamtprüfung grün.

**Historische Anweisung R1–R8 (erledigt):** Die damalige sequenzielle Migration wurde durchgeführt; Arbeitsbeginn für den **neuen** Auftrag und seine gesonderten Abnahmekriterien stehen in §8. Kein automatischer Commit/Push, kein überflüssiges Testing und niemals eine value-bearing Transaktion.

## 7. Umsetzungsstand R1–R8 (2026-10-15)

Ausgangscommit `7f3c08b`. Vollständige Migrationsmatrix, Originalpfade, Hash-Nachweise und Prüfprotokoll: [docs/EVIDENCE.md](docs/EVIDENCE.md). Einstieg: [docs/PHASENPLAN.md](docs/PHASENPLAN.md), [docs/STATUS.md](docs/STATUS.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

**R1–R8-Ist-Baum am Commit `c8362c6`** (vor dem neuen Refactoring gemäß §8):

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

**Offen im R1–R8-Prüfbericht:** flaky `fastlane`-Test `role_leases_are_exclusive_per_role_and_coexist_across_roles` (einmal rot in der Baseline); C1-Vorarbeit und der lokale Stash „local-before-p2p-fix-20261011“ (`5753f70`) erfordern gesonderte Bestandsprüfung. Im GitHub-Iststand `c8362c6` liegen `fastlane/src/{host,hot0,outbound}.rs` und entsprechende Go-Overlay-Dateien bereits im Repository, sind aber in `fastlane/src/lib.rs` nicht als Module eingebunden. **Vorhandensein bedeutet weder Kompilierung noch Integration oder Qualifikation.** Kein blindes `git stash pop`.

## 8. NEUER AUFTRAG R9–R13 — lesbare BSC-Architektur nach EVA01

**Gültigkeit:** Dieser Abschnitt ersetzt für alle folgenden **Struktur- und Benennungsarbeiten** den alten Zielbaum in §2. Er ersetzt **nicht** den fachlichen [BSC-Phasenplan](docs/PHASENPLAN.md), [BSC_DUAL_VENUE_ROADMAP.md](docs/BSC_DUAL_VENUE_ROADMAP.md), [Runtime-Vertrag](docs/BSC_NATIVE_SINGLE_RUNTIME_CONTRACT.md), Acceptance-Guard, Sicherheitsregeln aus §5 oder Nachweise. **Die hier gezeichnete Struktur ist ein Ziel, nicht der derzeitige Git-Baum.** Nach erfolgreichem Umbau die README zum geprüften Iststand aktualisieren.

### 8.1 Vorbild und Entwurfsprinzip

Referenz: [0dotxyz/eva01](https://github.com/0dotxyz/eva01), insbesondere `src/main.rs`, `src/cache/` und `src/execution/{mod,executor,inventory}.rs`. Vorbild ist **fachliche Lesbarkeit**: `cache`, `geyser`, `execution`, `liquidator`, `rebalancer`, `config`, `cli`. **Nicht** übernehmen: Ein-Crate-Zwang, fremde Solana-Handelslogik, unscharfe Sammelnamen wie `utils`/`wrappers` oder für BSC unpassende Netzwerkpfade.

**Entwurfsregeln:**
1. **Kurz und eindeutig:** Verzeichnis/Modul nennt die wirkliche Verantwortung, nicht einen Meilenstein oder eine Marketing-Eigenschaft. `geth_ipc` statt `fastlane`, `journal` statt `capture`. Kein `manager`/`utils`/`common` als Universal-Sammelplatz.
2. **Nur sechs Crates**, solange keine belegte neue Dependency-, ABI-, Sicherheits- oder Ownership-Grenze ein weiteres erfordert. Innerhalb der Crates nach Funktion gliedern, nicht je Phase ein Crate.
3. **Keine Dekorationsordner:** Ein kleines eigenständiges Modul bleibt `name.rs`; ein Verzeichnis entsteht nur bei mehreren echten Teilverantwortungen. Keine leeren Platzhalter.
4. **Ein Eigentümer je Funktion:** Domain/Trading trifft wirtschaftliche Entscheidungen, Execution versendet nur zulässige Pläne, IPC transportiert nur Records/Bytes, Qualification bewertet Nachweise ohne Kauf. Keine zweite Signierung, Quote, BUY- oder Release-Engine.
5. **Berechtigte Versionsnamen erhalten:** `WireV1`/`WireV2` sind echte ABI-/Protokollversionen. `protocol_v1.rs` und `protocol_v2/` sind fachliche Namen, keine Löschung der Versionierung.
6. **Keine Semantik- oder Latenzänderung:** Der neue Baum ist für Menschen besser lesbar, aber erzeugt keine gemessene Hot-Path-Verbesserung. Keine zusätzlichen WSS-/JSON-/RPC-Schritte in der kritischen Kette.

### 8.2 Verbindliche Namenszuordnung (ALT → ZIEL)

| Heute in `c8362c6` | Ziel | Eindeutige Verantwortung |
|---|---|---|
| `bsc/native/` | `bsc/engine/` | Einziger Rust-Cargo-Workspace |
| `crates/core/` | `crates/trading/` | Chain-State, Venues, Economics, Intent, Transaktionen, Signer, Submission-Policy; reine Fachlogik, keine externen Sends |
| `crates/wire/` | `crates/tx_encoding/` | Binäre TX-/RLP-Kodierungsprimitive, keine doppelte TX-Geschäftslogik |
| `crates/fastlane/` | `crates/geth_ipc/` | geth↔Rust Shared Memory / mmap / SPSC / Peer-Lanes / geth-Outbound-Evidence |
| `crates/runtime/` | `crates/execution/` | Startup, Transporte, Builder-Adapter und zulässiger Submission-/BUY-Dispatch |
| `crates/capture/` | `crates/journal/` | Journalformat, begrenzte Aufzeichnung, Replay |
| `crates/qualify/` | `crates/qualification/` | Read-only-Probes, Messungen, Evidenz, Abnahme-/Verdiktlogik |
| `trading/src/inclusion/` (heute `core/src/inclusion/`) | `trading/src/submission_policy/` | Provider-Capabilities, Release-Profile, unveränderliche Submission-Pläne; **kein Transport** |
| `execution/src/inclusion/` (heute `runtime/src/inclusion/`) | `execution/src/submission/` | Provider-Framing, TLS/Transport, Response, Versand; **keine Handelsentscheidung** |
| `geth_ipc/src/v2.rs` (heute `fastlane/src/v2.rs`) | `geth_ipc/src/protocol_v2/` | Wire-V2-Layout und Peer-/Lane-Lifecycle in getrennten Modulen, Byte-Layout bleibt exakt gleich |
| `geth_ipc/src/lib.rs`-V1-Implementierung | `geth_ipc/src/protocol_v1.rs` (+ ggf. `shared_memory/`) | V1-Protokoll unter sprechendem Namen; keine ungewollte API-/ABI-Abweichung |
| `bsc/native/geth/` | `bsc/geth/` | Gepinnter Go-geth-Overlay und seine **weiterhin geltenden** Wire-Spezifikationen/Pin-Dateien |

**Crate-Paketnamen:** Konsistentes Schema `darkbot-bsc-trading`, `darkbot-bsc-tx-encoding`, `darkbot-bsc-geth-ipc`, `darkbot-bsc-execution`, `darkbot-bsc-journal`, `darkbot-bsc-qualification` **als Ziel**, nicht vorschnell vor der Import-/Consumer-Inventur ändern. Die daraus resultierenden Rust-Importnamen und Cargo-Features samt `-p`-Aufrufen vollständig korrigieren. Bestehende CLI-Namen, die vertraglich durch Skripte/Nachweise gebraucht werden (`g07_mainnet_self_launch_buy`, `g08_mainnet_verify`, `g08_producer_timing`), zunächst stabil lassen; ggf. eigene dokumentierte spätere CLI-Migration. Fachbegriffe `economics`, `venues/flap`, `intent` beibehalten: `pricing` oder `launchpads` wären teils zu eng. `signer.rs` nur wenn nach Aufteilung wirklich eine Datei genügt.

**Code-Namenskonvention:** Module/Pfade `snake_case`; Rust-Typen/Traits `UpperCamelCase`; Funktionen beschreibend `verb_object` (z. B. `verify_submission_plan`); Testnamen nennen überprüfte Bedingung (z. B. `rejects_stale_lane_epoch`). `G0.7`/`C1.7` sind **Nachweis-/Phasenkennungen**, keine fachlichen Modulverzeichnisse. Vorhandene öffentliche API-/ABI-/CLI-Namen nur mit vollständiger Verbraucheranalyse und nachgewiesener Kompatibilität ändern.

### 8.3 Zielbaum R9–R13 (fachlich, keine Platzhalter-Vorgabe)

```text
DarkBot/
├── bsc/
│   ├── README.md
│   ├── .gitignore
│   ├── engine/                           # EINZIGER produktiver Rust-Workspace
│   │   ├── Cargo.toml
│   │   ├── Cargo.lock
│   │   ├── rust-toolchain.toml
│   │   └── crates/
│   │       ├── trading/
│   │       │   └── src/
│   │       │       ├── lib.rs
│   │       │       ├── chain_state/
│   │       │       ├── venues/flap/
│   │       │       ├── economics/
│   │       │       ├── intent/
│   │       │       ├── transactions/
│   │       │       ├── signer.rs         # oder kleines Modul, wenn nötig
│   │       │       └── submission_policy/
│   │       ├── tx_encoding/
│   │       │   └── src/{lib.rs,rlp.rs}
│   │       ├── geth_ipc/
│   │       │   └── src/
│   │       │       ├── lib.rs
│   │       │       ├── protocol_v1.rs
│   │       │       ├── protocol_v2/      # Layout, Lanes, Lifecycle, Checksums
│   │       │       ├── shared_memory/    # nur bei echter Untergliederung
│   │       │       ├── ingress.rs
│   │       │       ├── outbound.rs
│   │       │       └── host.rs
│   │       ├── execution/
│   │       │   └── src/
│   │       │       ├── lib.rs
│   │       │       ├── startup.rs
│   │       │       ├── submission/       # club48, blockrazor, local_geth,
│   │       │       │                     # transport, TLS, framing, response, send
│   │       │       ├── acceptance_guard.rs
│   │       │       └── bin/              # G0.7: schmaler kontrollierter Einstieg
│   │       ├── journal/
│   │       │   └── src/{lib.rs,format.rs,recorder.rs,replay.rs}
│   │       └── qualification/
│   │           └── src/
│   │               ├── lib.rs
│   │               ├── probes/          # nur wenn mehrere Module
│   │               ├── measurements/    # nur wenn mehrere Module
│   │               ├── evidence/        # nur wenn mehrere Module
│   │               ├── verdict.rs
│   │               └── bin/             # G0.8, DRY, Producer-Timing
│   ├── geth/                             # Go-Overlay, Pin, Wire-V1/V2-Spezifikationen
│   │   ├── README.md
│   │   └── overlay/
│   ├── config/
│   ├── tools/                            # fachlich sprechende CLI-/Prüfnamen
│   ├── docs/
│   └── evidence/                         # NUR wenn vertraglich freigegeben
└── prestudy/bsc/                         # historisch, nie im Live-Build
```

Der Baum illustriert Zuständigkeiten, **nicht** die Pflicht, für jeden Eintrag sofort einen Ordner oder eine Datei anzulegen. `lib.rs`, `mod.rs`, Cargo-Manifeste und Tests müssen real korrekt verkabelt sein; Module dürfen nicht als unkompilierte Schattenimplementierung bestehen.

**Getrennte Rust/Go-Platzierung:** `bsc/engine/` ist der Rust-Workspace; `bsc/geth/` der Go-geth-Overlay, kein zweiter Rust-Workspace. **Wichtige Ausnahme:** In R1–R8 wurde `bsc/native/evidence/` wegen `Q5A_TLS_EVIDENCE_PATH` und eingebundener SHA-256-Identität ausdrücklich nicht verschoben (§7). `bsc/evidence/` ist deshalb **nur ein bedingter Zielort**. **Kein** stillschweigendes Verschieben, Neueinbetten, Ändern des Qualifikationsstatus oder fälschliches Beibehalten der alten Freigabe. Erst gebundene Bytes, Pfade, Hashes, Consumer und die benötigte Requalifikation/Neubaseline klären. Bis dahin die betroffenen Evidence-Dateien unverändert bewahren; falls das einen restlichen `native/evidence`-Pfad oder einen Stopp der `native → engine`-Migration erzwingt, den Konflikt explizit melden statt mit Symlinks, neu erfundenen Hashes oder scheinbarer PASS-Meldung zu verdecken.

### 8.4 Konkrete Ist-Code-Befunde: zuerst bearbeiten, nicht blind umbenennen

1. `bsc/native/crates/fastlane/src/v2.rs` umfasst ca. **148 kB**; Protocol-Wire-Layout, Atomics/Shared-Memory und Peer-Lifecycle sind eng verbunden. Nach belegten Verantwortungen in `protocol_v2/` zerlegen. Byteoffsets, `#[repr]`, Alignment, SPSC-Sequenzierung, Speicherordnung, Struct-Größen, Record-Arten, CRC und Wire-Version unverändert. Rust↔Go ABI-/Fixture-/Offset-Parität beweisen.
2. `fastlane/src/{host,hot0,outbound}.rs` sowie `geth/overlay/darkbot/fastlane/{chain_v2,outbound,qos}*.go` sind im aktuellen GitHub-Baum vorhanden; `fastlane/src/lib.rs` exportiert jedoch nur `ingress` und `v2`. **Datei im Repo ≠ im Cargo-Build integriert.** Nicht mit `pub mod` blind live schalten: Abhängigkeiten, API, Testfälle, fehlende C1-Abnahme und Stash `5753f70` gesondert inventarisieren. **HOT0:** geliehene Receive-Slots/IPC ins `geth_ipc`-Crate, Domain-Admit-/Launch-Entscheidungen ausschließlich nach `trading` bzw. der zulässigen `execution`-Orchestrierung; erst nach präziser Schnittstellenprüfung festlegen.
3. `runtime/src/bin/g07_mainnet_self_launch_buy.rs` umfasst ca. **48 kB**. Kontrollierten Einstieg in `execution/src/bin/` erhalten; fachlich getrennte Komponenten nur bei echtem Nutzen auslagern, ohne zweite BUY-Logik, Freigabeumgehung, Timingänderung oder neue Sends. G0.7-Binary-Name und `g07-mainnet`-Feature zunächst erhalten.
4. `capture/src/lib.rs` bündelt Journal, bounded Recorder und Replay. In `journal/src/{format,recorder,replay}.rs` zerlegen, soweit die interne Ownership sauber belegbar ist; auf unveränderte Journalbytes und Fehler-/Overflow-Semantik testen.
5. `core/src/inclusion/` enthält Capability-/Release-/Plan-Semantik, `runtime/src/inclusion/` enthält reale Transport-/Submission-Adapter. Deshalb **zwei verschiedene Namen** `submission_policy` und `submission` – keine gemeinsame Riesenschicht.
6. `bsc/tools/` enthält viele `g06a_*`-/`c19_*`-Namen. Ein konsistentes `action_object`-Schema für **aktive** Tools bestimmen; wenn Skripte, Evidence oder Betreiber diese als stabile CLI nutzen, bestehende Namen erhalten bis zur nachweisbaren Änderung sämtlicher Consumer. Phasen-IDs und Golden-Reportnamen nicht als wertloses Legacy löschen.

### 8.5 Ausführung in fünf abgegrenzten Gruppen

| Gruppe | Umsetzung | Exit-Kriterium |
|---|---|---|
| **R9 — Inventar und Architekturentscheidungen** | Aktuellen HEAD, lokalen Git-Status (inkl. Stash), aktive vs. nur vorhandene Module, Crate-Graph, Cargo Features, Rust/Go IPC-ABI, Imports, Testziele, `native/evidence`/Q5A-Pfad-Hashes, Werkzeuge, Konsumenten und historische Bindungen erfassen. Matrix `ALT → NEU → VERBRAUCHER → RISIKO → PRÜFUNG`. Noch keine Hot-Path-Verschiebung. | Für **jede** aktive Datei und öffentliche Schnittstelle ein neuer Besitzer; blockierende Q5A-/ABI-Fragen ausdrücklich geklärt oder gestoppt. Keine Vermischung mit Projekt_0. |
| **R10 — Workspace, Crate- und Go-Pfade** | `native → engine`, die sechs Crate-Verzeichnisse und erforderliche Cargo-Paketnamen/Importe konsistent migrieren, `native/geth → geth` nur mit aktualisierter Go-/Tool-/Pin-Referenz. Evidence-Bindung gem. §8.3 unbedingt respektieren. | Genau ein Workspace, ein produktives Lockfile; alle Manifeste, Skripte/Imports, Go-Pins und referenzierten Pfade stimmen; `cargo metadata --locked`, `cargo check --locked --workspace`. |
| **R11 — Fachliche Modulgrenzen** | `inclusion → submission_policy/submission`; Protocol V1/V2 fachlich strukturieren, `journal` gliedern, G0.7-Entrypoint entschlacken, HOT0/Ingress/Outbound nach überprüfter Ownership behandeln. Nie unqualifizierte C1-Module automatisch aktivieren. | Kein Doppel-Owner, keine zirkuläre Crate-Abhängigkeit, keine Änderung an Wire-/Signing-/BUY-Bytes, Offsets, Memory-Order, Guards, Admission oder Scheduling; relevante Rust-/Go-/Journal-Regressionen PASS. |
| **R12 — Tools, API und Dokumentation** | Aktive Tool-Namen/CLI-Verbraucher und `docs/{ARCHITECTURE,STATUS,EVIDENCE,PHASENPLAN}.md` auf tatsächlichen Iststand synchronisieren; alte Phasenreferenzen als historische Quellen erhalten; keine generischen Platzhalter. | Jede fachliche Funktion aus dem Pfad erkennbar, alle aktiven Referenzen auflösbar, relevante read-only/DRY-/EVM-Tools reproduzierbar. |
| **R13 — Abnahme/Freigabevorlage** | Endgültige Migrationstabelle, reale Verzeichnisstruktur und Testbilanz erstellen; Guards, Evidence, Script-Resolver, Build/Pins, Hot-Path, fehlende LIVE-Nachweise prüfen; explizite Nutzerprüfung **vor** Commit/Push. | Sechs klar benannte Crates und der getrennte Go-geth-Overlay; kein zweiter Codepfad; keine offenen stillen ABI-/Evidence-/Security-Drifts, keine Tests als PASS erfinden. |

**Jede Gruppe erst abschließen, wenn ihre maßgeblichen Checks PASS sind.** Bei Problemen nur die betroffene Gruppe korrigieren, keinen blind vorbereiteten Stash einspielen, keine anderen Projekte anfassen. Die bereits dokumentierten **9/10 Testdurchläufe gehören ausschließlich zu R1–R8**; für R9–R13 einen **eigenen, maximal 10 Durchläufe umfassenden Testplan** vor der Umsetzung festlegen. Ein `cargo test`-/Integrations-/Regressionstest-Aufruf zählt als ein Durchlauf; `cargo check`, `cargo metadata`, `cargo fmt --check`, `bash -n`, `node --check` und Git-Diff-Prüfungen sind Struktur-/Syntaxchecks. Keine unnötigen Stresstests, keine LIVE-/Mainnet-Sends. Wenn eine erforderliche Paritätsprüfung innerhalb des Testlimits nicht belegt werden kann: **nicht als abgeschlossen erklären** und um weitere explizite Freigabe bitten.

### 8.6 Abnahmeliste und Arbeitsanweisung für Claude

- **Funktionsparität:** TX-/RLP-Bytes, Signer/Nonce/Gas/Slippage/Quote/Builder-Budget, Release-Plan/Provider-Capability, G0.7-Gate `FROZEN_APPROVED_V2_SHA256 = "UNARMED"`, `approved=false`, `execution_ready=false`, G0.8 read-only, G0.6a DRY und unabhängige EVM-Gegenprüfung unverändert.
- **IPC-/Journal-Parität:** V1/V2-Wire-Formate, ABI/Go-Rust-Offsets, Atomics/Memory-Ordering, per-peer SPSC, Outbound-Evidence, bounded Recording/Replay, Crash-/Restart-/Quarantäne-Verhalten unverändert; nicht integrierte Module dürfen nicht als aktive Runtime-Leistung verkauft werden.
- **Keine Performancebehauptung durch Refactoring:** Ziel `T0 → T_buy < 1 ms` bleibt unbewiesen, keine WSS/JSON im kritischen Hot-Path, Tokenkauf ≠ lokale Übergabe/ACK/Receipt/Inclusion.
- **Keine LIVE-/Mainnet-Freigabe:** Refactor ist keine Autorisierung von Funding, SEND, Wallet-/Builder-Kosten oder Handelsstrategie. Kein G0.7-Binary ausführen, solange nicht gesondert freigegeben.
- **Projektgrenzen:** Ausschließlich `bsc/`, bei Bedarf `prestudy/bsc/`. `project_0/` ausdrücklich **nicht** ändern, verschieben, stagen, entstagen, committen oder sonst beeinflussen. Auch die bereits eventuell dort gestagten Änderungen unangetastet lassen. Keine generellen Git-Clean-/Reset-/Add-Befehle.
- **Arbeitsweise:** Zuerst `git status -sb`, `git log -1`, dieses README und die fachlichen Verträge lesen. Bestehende lokale uncommitted Arbeit/Secrets/Stashes sichern, nicht einfach `git pull` über einen schmutzigen Baum. Arbeitsstand auf `c8362c6` oder neueren geprüften HEAD beziehen. R9–R13 nacheinander durchführen, am Ende ein **kompaktes, menschenlesbares** Ergebnis: echter Zielbaum, Alt→Neu-Matrix, Prüfergebnisse, klare Restmängel. Ohne eigene Nutzerfreigabe **kein Commit/Push**.

## 9. Umsetzungsstand R9–R13 (2026-10-15, Ausgangscommit `c8362c6`)

Vollständige Alt→Neu-Matrix und Prüfprotokoll: [docs/EVIDENCE.md](docs/EVIDENCE.md) §5–§6; Baum und Ownership: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

**Realer Baum**

~~~~text
bsc/
├── engine/                     # einziger Rust-Workspace (Cargo.toml, Cargo.lock, rust-toolchain.toml 1.93.1)
│   └── crates/
│       ├── trading/            # darkbot-bsc-trading: chain_state, venues/flap, economics, intent, transactions, signer, submission_policy
│       ├── tx_encoding/        # darkbot-bsc-tx-encoding: rlp
│       ├── geth_ipc/           # darkbot-bsc-geth-ipc: protocol_v1, v2, ingress (+ nicht integriert: host, hot0, outbound)
│       ├── execution/          # darkbot-bsc-execution: startup, submission/, acceptance_guard, qualification_capture, bin/g07_mainnet_self_launch_buy/
│       ├── journal/            # darkbot-bsc-journal: format, recorder, replay
│       └── qualification/      # darkbot-bsc-qualification: probe, measure, evidence, identity, stats, verdict, bin/
├── geth/                       # Go-geth-Overlay, Pin, Wire-V1/V2-Spezifikationen
├── native/evidence/            # gebundene Q5A-/C1-Nachweise (siehe unten)
├── config/ · tools/ · docs/
prestudy/bsc/                   # historisch, kein Build-Member
~~~~

**Ergebnis je Gruppe**

| Gruppe | Ergebnis |
|---|---|
| R9 | Inventur, Entscheidungen, Testplan (max. 10). Befunde: C1-Testziele `c15_hot0_alloc`, `c18_stress`, Beispiele `c16_host_profile`, `c19_consumer`, `c19b_capture` kompilieren schon am HEAD nicht (brauchen Stash `5753f70`); kein Go-Toolchain auf diesem Rechner |
| R10 | `native → engine`, sechs Crate-Ordner und Paketnamen `darkbot-bsc-*`, Imports, `native/geth → geth`, Tools/Skripte; Lock nur Mitgliedsnamen geändert. U1 **426/0/5**, U2 **20/20** |
| R11 | `inclusion → submission_policy` (trading) / `submission` (execution), `tx → transactions`, Journal in `format`/`recorder`/`replay`, Wire V1 in `protocol_v1.rs` (Root-Re-Export), G0.7 in `main.rs` + `inputs`/`transactions`/`chain_reads`/`builder_submission`; alle Verschiebungen wörtlich (Textnachweis), normalisierter Diff ohne Codeänderung. U3 **426/0/5**. **Offen:** `v2.rs → protocol_v2/` |
| R12 | Docs (`ARCHITECTURE`, `STATUS`, `PHASENPLAN`, `EVIDENCE`, Runbook, Reports) synchronisiert; Werkzeugnamen als stabile CLI erhalten; C1-Freeze-Scope wieder inkl. `bsc/tools`. U4 Offline-Toolkette **17/17**, U5 **20/20** |
| R13 | `cargo metadata --locked`, `cargo check --workspace`, `cargo fmt --check` sauber, `cargo clippy` ohne Fehler (6 bekannte Stil-Warnungen im übernommenen G0.7/G0.8-Code); kein BSC-Eintrag im Git-Index; `project_0/` unberührt |

**Testbilanz R9–R13:** 5 von 10 Läufen (U1–U5).

**Gemeldete Konflikte / offene Punkte**

- `bsc/native/evidence/` bleibt (Q5A-Pfad + SHA-256 einkompiliert; Umzug = Neuqualifikation). `bsc/evidence/` wurde daher **nicht** angelegt.
- `v2.rs → protocol_v2/`: nicht ausgeführt. Der Stash „local-before-p2p-fix-20261011“ (`5753f70`) ändert `v2.rs` um ca. 950 Zeilen und ist Voraussetzung für die committeten C1-Testziele; eine Zerlegung macht ihn mechanisch unanwendbar. Rust↔Go-Parität ist ohne Go-Toolchain nicht prüfbar.
- Nicht ausgeführt, daher nicht PASS: Go-Overlay-Tests, Interop `c13_*`/`c14_*`, voller G0.6a-DRY, EVM-Crosscheck gegen RPC, jede G0.7-/G0.8-Ausführung gegen Mainnet.
- Flaky: `geth_ipc` `v2::tests::role_leases_are_exclusive_per_role_and_coexist_across_roles`.

## 10. C1-Integration und `protocol_v2` (2026-10-16, Ausgangscommit `1c99127`)

Auf ausdrückliche Nutzeranweisung (Ausnahme von der Roadmap-Stop-Regel; **keine C1-Phasenabnahme**) wurde der C1-Rev2-Stand aus dem Stash `5753f70` dateiweise integriert und Wire V2 anschließend gegliedert. Details, Herkunft und Prüfprotokoll: [docs/EVIDENCE.md](docs/EVIDENCE.md) §7.

- `geth_ipc`: `BorrowedRecordV2`, `RecordGuardV2`, `LaneSetV2`, `try_acquire`, `hot0`, `outbound`, `host` integriert; alle C1-Tests/-Beispiele kompilieren und bestehen (keine Ausblendung).
- `protocol_v2/`: 13 Fachdateien (Layout, Prüfsummen, Fehler, Mapping, Lanes, Records, Guards, Arena, Producer, Consumer, Lifecycle, Peer-Pool, Role-Lease) als **ein** Modul (`include!`), weil echte Untermodule den Release-Hot-Path-Code änderten; mit `include!` ist der Maschinencode instruktionsidentisch (98/98 Funktionen).
- Rust↔Go-Parität: Go 1.25.0 lokal, offline; Go-Race-Tests 86/0/5 SKIP; Interop c13/c14 PASS vor und nach der Gliederung.
- Workspace-Regression ohne Filter: **479 PASS / 0 FAIL / 5 ignored**. Testbudget: 9 von 10.
- Offen: C1-Phasenabnahme gemäß [BSC_C1_5_TO_C1_10_QUALIFICATION_RUNBOOK.md](docs/BSC_C1_5_TO_C1_10_QUALIFICATION_RUNBOOK.md); Entscheidung über den nun vollständig übernommenen Stash (nur `g07_testnet_birth_buy.rs`, heute Prestudie, nicht übernommen).
