# C0 Einheit 1, Revision 2 — tatsächliche Prüfung, 2026-10-03

## Ergebnis

**Einheit 1 kompiliert und besteht alle 27 Tests im Debug- und Release-Modus sowie Clippy mit `-D warnings`.** Der echte Differentialtest gegen `alloy-rlp` besteht mit 27.624 Decoder-Eingaben. Kein neuer funktionaler Fehler in RLP/Legacy/Signer nachgewiesen. Formatierung des gelieferten Originals besteht noch nicht; `cargo fmt --all` behebt die Layoutabweichungen in der isolierten Prüfumgebung, danach Format PASS. Cargo.lock wurde offline erzeugt und liegt bei.

ZIP und Produktionsrepo bleiben getrennt. Kein Import, keine Änderungen an Gate 0, Solana oder Project 0, kein LIVE-Send. Neue BUY-/Signier-Latenzen wurden nicht gemessen. Das vorhandene Testnet-Ergebnis ist kein Benchmark dieser Bibliothek.

Eingabe: `/Users/sawyer_ma/Desktop/Delta_Mac/files (20).zip`
SHA256: `bb16216b79cd37f21002565ea3c540dd79547e79adea89755b83871ec1e8bcd1`.
Prüfworkspace: `/tmp/darkbot_c0_review_rev2_20261003/bsc/native`.
Compiler: Homebrew rustc 1.93.1, aarch64-apple-darwin, LLVM 21.1.8. Die tatsächlich verwendete Version passt zum neuen Pin. Homebrew allein erzwingt eine rustup-Toolchain-Datei nicht; CI muss den Pin ebenfalls tatsächlich verwenden.

## Ausgeführt

```bash
cargo generate-lockfile --offline
cargo fmt --all -- --check                         # Original: Exit 1
cargo test --workspace --locked --offline          # Exit 0, 27 PASS
cargo test --workspace --release --locked --offline # Exit 0, 27 PASS
cargo clippy --workspace --all-targets --locked --offline -- -D warnings # Exit 0
cargo fmt --all                                    # nur Prüfumgebung
cargo fmt --all -- --check                         # danach Exit 0
```

Logs und `command_exit_codes.json` liegen neben diesem Review. Tests wurden vor rustfmt auf dem gelieferten Code ausgeführt; rustfmt änderte ausschließlich Layout. Keine weiteren algorithmischen Korrekturen nötig.

## Bestätigte Verbesserungen

- Fremde Signature-Encoding-API crate-privat; private Prehash-Signierung. Öffentlicher Produktionspfad signiert und kodiert dieselbe Transaktion.
- Erwartete Chain-ID wird vor Sender-Recovery geprüft. Eine separate erwartete Sender-/Intent-Bindung bleibt Aufgabe des Send-Pfads; Recovery allein beweist keinen gewünschten Sender.
- Output enthält genau eine Transaktion; Fehler hinterlassen leeren Output. Scratch/Output-Kapazität und Zeiger bleiben im neuen 1.024-Signaturen-Test unverändert.
- Die konservativen Größenobergrenzen decken getestete maximale Feldbreiten und große Calldata ab. Zulässige Calldata-Grenzen muss die künftige Runtime zusätzlich festlegen. Das ist kein vollständiger Allokationsnachweis aller Abhängigkeiten.
- Exakte direkte Dependency-Pins, Compiler/MSRV, keine I/O im Core, korrigierte bsc-geth-Baseline und eingefrorene historische Gate-0-Programme sind konsistent.
- `gas_price: u64` ist für diese eigene BSC-Call-API ausdrücklich begrenzt. E5 kann für diesen Scope als entschieden dokumentiert werden; kein offener funktionaler Blocker, kein zwingender U256-Umbau.

## Noch vor Einfrieren des Event-/Clock-Vertrags korrigieren

Diese Punkte betreffen den **Vorschlag für spätere Verbraucher**, nicht einen nachgewiesenen Defekt der kompilierten Einheit 1. Kein Architektur-Neustart nötig.

### 1. Eindeutige Uhrreferenz und Einheiten

`C0_EVENT_CLOCK_CAPTURE_CONTRACT.md:15–44`: `domain` bezeichnet aktuell eine Uhrklasse, während ClockInfo nur einmal je Session/Domain vorgesehen ist. Bei mehreren Produzenten/Quellen reicht das nicht, um den gemeinten Boot/Ursprung eindeutig zu referenzieren. Ergänzen: `clock_id` pro konkreter Uhrinstanz mit Host/Node-ID, Boot-ID und OS-Uhr-ID; ClockStamp referenziert sie. Gleiche Klasse allein erlaubt keine Differenz. Konstante Metadaten einmal schreiben, nicht auf jeden Hotpath-Record kopieren.

`ticks_ns` muss für Header-Millisekunden und Event-Sekunden explizit normalisiert werden oder die ursprüngliche Einheit tragen. Rohwert und Herkunft erhalten, Konvertierungsüberlauf behandeln. Sekunden→Nanosekunden schafft keine Nanosekundenauflösung.

### 2. Producer-Witness und konservatives Acceptance-Ergebnis

`§1 Regel 2`, `§4`, `§7`: „gleicher Ausführungsversuch“ ist richtig, aber dafür fehlt noch der konkrete Record-Vertrag. Der vorhandene Testplan verlangt Node/Boot, Execution-Attempt-ID, Parent/Block-Hash, TX-Hash/Index, semantischen Messpunkt, Zeitklammern und Erwerbsnachweis. Diesen **Minimalvertrag jetzt definieren**, Implementierung/Produzentenzugang dürfen später erfolgen.

Eine einzelne feste `uncertainty_ns >= max(resolution, read_overhead)` beweist keine Gesamtunsicherheit des Hooks. Bracketing, Overhead und variable Restunsicherheit ausweisen. Acceptance gemäß bestehendem Testplan: späteste BUY-Completion minus frühestes gültiges T0 plus verbleibende Unsicherheit < 1.000.000 ns. Fehlende Quelle bleibt UNKNOWN. Reorgs und verworfene Produktionsversuche dürfen nicht zusammengeführt werden.

### 3. Adapter-spezifische tatsächliche Send-Grenze

`§3:76`: „Write und Flush zurückgekehrt“ reicht als pauschale Definition nicht. Ein gepufferter TLS-/HTTP-Writer kann Erfolg melden, ohne dass der gesamte Payload die tatsächliche Socket-Grenze erreicht hat; mmap-Commit ist nur Queue/IPC.

Pro Adapter konkrete Layer/Grenze festlegen und messen: queue_commit, write_start, vollständiger Transport-Handoff, ACK. Bei geth separat IPC-Commit und geth/network-Handoff. Partial Write/Flush-Fehler bleiben UNKNOWN_OUTCOME/HALT, kein heimlicher Retry. Keine neue Task-/Queue-Grenze einführen. Weder Socket-Handoff noch ACK bedeuten Erwerb.

### 4. Hash-only-Felder und Vertrauen vor dem Send

`§4:89–101`: HASH_ONLY/FEED_TARGET erlauben leere Rawbytes, verlangen aber gleichzeitig sender/nonce/to/value/input. Fehlende Felder müssen als fehlend repräsentiert werden, nicht als Nullwerte. Variantentyp oder Presence-/Completeness-Vertrag definieren.

Nur Daten mit bereits gesicherter Bindung dürfen actionable sein. Im geth-Hook kann diese Bindung aus derselben dekodierten TX stammen; beim kontrollierten bekannten Hash aus einer vollständig vorbereiteten passenden Launch-Fixture. Bei fremdem unbekanntem Hash keine Launch-Felder erfinden. Spätere Recorder-Prüfung ist ein Audit, keine nachträgliche Legitimation eines bereits gesendeten BUY. Frühzeitige Hash→Full-Vervollständigung muss eigene Zeit und Provenienz behalten.

### 5. Capture-Grenzen und Datenlücken

`§6:146`: `armed_at` ist Host-monoton, Header-Zeit eine andere Domain. Keine arithmetische Differenz ohne Mapping. Host-Beobachtungsfenster und kanonische Blockauswahl getrennt festlegen; Backfill durch den konservativen Horizont plus Randblöcke, tatsächliche Abdeckung dokumentieren. Mit Producer-T0 benötigen andere Quellen ein gemessenes Mapping, sonst keine COMPLETE-Behauptung für ein exaktes Fenster.

Die vorhandene Pre-Trigger-/15-s-/Reorg-/INCOMPLETE-Regel ist richtig. Gap-Zähler allein rekonstruieren nicht automatisch den ersten fehlenden seq-Bereich. Pro Ring/Quelle Sequenz- und Verlustvertrag festlegen; vollständige Zuordnung oder explizit unbekannte Spanne. Journal-Provenienz muss Boot/Session/Quelle erhalten, auch bei Restart. Aufnahmeverlust blockiert nicht den BUY, verhindert aber COMPLETE.

## Gate-0-Lock-Nebenbefund: tatsächlich bestätigt

Nur read-only im echten Repo geprüft:

| Datei | Pakete | reth-* | tokio | alloy-rlp |
|---|---:|---:|---|---|
| `HEAD:bsc/gates/gate0/Cargo.lock` | 267 | 0 | fehlt | 0.3.16 |
| lokale, bereits geänderte Datei | 670 | 42 | vorhanden | 0.3.13 |

Manifest fordert 12 reth-Abhängigkeiten und alloy-rlp `=0.3.13`. Der committete Lock passt deshalb nicht. Der lokale Lock ist ein plausibler Kandidat; seine Gesamtkorrektheit wurde hier nicht durch einen neuen Gate-0-Build bewiesen. Nicht automatisch committen oder ersetzen: Der aktuelle Auftrag ist ZIP-Prüfung, und dieser Lock war bereits vor der Prüfung lokal verändert.

## Übergabe und nächster Schritt

Beiliegendes normalisiertes Prüf-ZIP enthält rustfmt-Ausgabe und den tatsächlich verwendeten Cargo.lock, **keine Targets**, keine neuen Produktionsalgorithmen, keine importierten Repo-Dateien. SHA256SUMS wurde für diese Prüfkopie erneuert. Inventory und Clock-Vertrag bleiben als Fables Vorschläge unverändert; dieser Review beschreibt die noch notwendigen Präzisierungen.

Fable soll die fünf Vertragspräzisierungen übernehmen, den ausgeführten Compilerstatus im Inventar aktualisieren und Lock/Format übernehmen. Danach final geprüfte Einheit 1 importieren; anschließend Einheit 2 (Flap-ABI/Calldata/CREATE2/Launch-Decode), ohne historische Vollmigration. Minimalvertrag vor seinen ersten Verbrauchern einfrieren, Minimal-Capture vor integriertem echten BUY.

**Keine neue komplette Rust-Neuentwicklung nötig. Kein Gate-PASS, Produktions-LIVE oder <1-ms-Kaufnachweis durch diese Unit-Tests.**

## Erweiterte Abnahme: Git-Evidence → Revision 2 → Produktionsziel

Auf zusätzlichen Nutzerauftrag wurde die Prüfung von Compilerkorrektheit auf Anforderungsabdeckung erweitert. Referenz ist Git-HEAD `24e50ca9685bec27885dee7c551569882d33178a`; jüngster BSC-Commit `0ab1322`. Währenddessen liefen andere Projektarbeiten; die 22 dokumentierten Referenzdateien stimmen bytegleich mit diesem Git-Stand überein (`git_alignment_provenance.json`). Die bereits lokal modifizierte Preflight-Datei und der lokale Gate-0-Lock sind keine neue normative Evidence dieser Prüfung. ZIP-Hash bleibt der oben dokumentierte.

**Gesamturteil:** Einheit 1 trägt geprüfte Encoding-/Signer-Evidence korrekt weiter. Das ZIP-Inventar ist jedoch eine Funktions-Zuordnung, noch kein vollständiger verbindlicher Übernahmevertrag aller Gate-0-Erkenntnisse. Für vollständige Zielkonformität müssen die unten genannten Planlücken geschlossen werden. Unimplementierte spätere Phasen sind nicht automatisch Fehler in Einheit 1; fehlende Anforderungen an deren Übernahme sind Review-Funde.

### Welche Tests und Messdaten sind wie berücksichtigt?

| Frühere Evidence / Anforderung | Befund der bisherigen Arbeit | Revision 2 / verbindliche Übernahme |
|---|---|---|
| Solana/PAPER-Parität | 287 Tests, p99 receive→enqueue 2,9925 ms; ausschließlich Altpfad | Nicht in Rust-BSC umgedeutet. Solana bleibt separat erhalten; kein Eingriff oder Symmetrie-Umbau. |
| Solana Research-Latenzscan | Modell-Entry ≤3 ms positiv, 4–5 ms etwa Break-even; kein BSC-Execution-Nachweis | Nicht als bewiesene BSC-EV-Grenze verwenden. Das Nutzerziel BUY <1 ms bleibt verbindlich unabhängig davon. |
| G0.2 CREATE2 | Historisches V6/TAXED_V3-Modell, sieben EIP-1014-Vektoren, echte Adresse reproduziert | Einheit 2 geplant; diese Vektoren und konkrete Deployment-/Init-Code-Bindung ausdrücklich übernehmen. |
| G0.2 unabhängige Semantik | 65 Zwischen-TXs, 237 Callframes, 100 Logs, Erwerb exakt; historische Scope-Grenzen | Nur allgemein im Inventar referenziert. Semantische Golden-Fälle mit Quellen/Hashes als Acceptance für Einheit 2/3 binden. |
| G0.2 Hotpath-Matrix | 17 Feldgruppen, 277 Storage-Abhängigkeiten, 497 klassifizierte Zeilen | Nicht vollständig in den konkreten Einheitenvertrag übernommen; siehe P1 State-/Scope-Fund unten. |
| Historische Mainnet-TX G0.3 | Raw 343 Bytes, Hash 0x8aff…, richtige Chain/Sender | Direkt in Einheit-1-Tests übernommen und tatsächlich PASS. |
| EIP-155 und kurze r/s | Standardvektor, früherer RLP-Signaturfehler, 1.024 Signaturfälle | In Einheit 1 tatsächlich getestet, PASS; weiterhin verpflichtende Golden-Regression. |
| Unmittelbarer DOGE-Direct-BUY | Rust/EVM Erwerb 30669575743216741084405716, minOutput exakt / +1-Revert, before-launch-Revert | Einheit 3 als enger Fixturepfad geplant. Nicht mit historischem späterem Direct-BUY-Erwerb oder beliebigen Tickpfaden vermischen. |
| Native-BNB Curve-20 | Unabhängige EVM-Quote: 0,001 BNB →176664965031639901123273; Testnet 0,0001→17669034223232267628301 | Einheit 3 geplant, getrennt von DOGE. Fee-/Tax-Schalter und aktuelle Curve-/Code-Bindung mit übernehmen. |
| G0.4 Pending | Fünf beobachtete V6-Launches Legacy; kein vollständiger frozen-scope Coverage-Nenner | Im Inventar korrekt eingeschränkt; Coverage bleibt PARTIAL, keine Null-Miss-Garantie. |
| G0.5 lokale/remote Diagnosen | Mehrere Aufbauten, teils Tail >1 ms; keine einheitliche neue Bibliothekslatzenz | Allgemeines „G0.5 PASS“ im Inventar präzisieren: welcher Aufbau/Scope, keine pauschale Transport-/Inclusion-Freigabe. |
| Prepared Self-Launch | 48Club p99 0,584291 ms; BlockRazor p99 0,526584 ms; je 100 Samples, absichtlich ungültige Transport-Probes | Vorbereitung allgemein genannt. Zwei explizite Produktionspfade und deren getrennte Benchmarks fehlen als verbindliche Acceptance. |
| Intent-/Nonce-/Dedup | 8.000 Signale→1.000 Intents; Reihenfolge, kein Retry nach UNKNOWN | Einheit 4 geplant, nicht bereits im ZIP implementiert. Semantik erhalten, Datenstruktur nicht blind übernehmen. |
| Builder-Write-Grenze | Permit vor Write, ACK ≠ confirm, Partial/Flush-Fehler HALT | Im Inventar korrekt übernommen. Multi-Adapter-Fan-out braucht ergänzenden Lifecycle-Vertrag. |
| Künstliches HoL | 25 ms absichtlich injiziert; keine reale BUY-Verletzung bewiesen | Single-Sender beibehalten. Kein Multi-Sender- oder Architektur-Umbau aus diesem Versuch ableiten. |
| Restart-Fence | 26 Offline-Tests, alle vier Zustände quarantänisiert | Richtig als Fail-closed geplant; kanonischer Resume/Reconciler weiterhin spätere, eigene Acceptance. |
| Recipient-Binding | 256 finale Envelopes; Ticket/Chain/Nonce/Token/Portal/Recovered Sender | Noch nicht portiert; Einheit 2/4 muss diesen konkreten Fall gegen den neuen Signer/Decoder ausführen. Acht Test-Sender sind keine Produktions-Multi-Lane-Anforderung. |
| G0.6a ordered Bundle | Identische `[launch_A,buy_B]`, zwei Wallets, expiry/budget/gas, zero egress | Im Inventar nur Funktionsnamen/Payload allgemein; Pflichtoptionen und Fan-out fehlen, siehe unten. |
| Reale Testnet-BUYs | Früher N/N+1 im selben Block; jüngster Known-Hash-BUY 0,045125 ms lokal, Folgeblock, Erwerb positiv | Als Referenz erhalten. Kein allgemeiner Fremdlaunch-/Mainnet-/T0→T_buy-PASS. Known-Hash-Abkürzung braucht exakte vorbereitete TX-Bindung. |
| 15 Sekunden Raw Capture | 2026-10-03 explizit verlangt; noch nicht als Funktion implementiert | Im ZIP vorgeschlagen, vor integriertem BUY. Vollständige Economic-/Producer-/Journal-Felder noch präzisieren. |
| Phase-C-Gegenbeispiele | Frühestes First-Seen, Nonce-Konkurrenz, Canonical unabhängig Pending, Reorg, Backfill, Receipt/Log-Identität | Normativ vorhanden; im ZIP noch kein konkretes Test→Owner-Mapping für C2/Recorder. |
| Finaler D/E/G/H-Pfad | Gleicher Core in Replay/PAPER/LIVE; komplette Strategie-/State-/Transport-Requalifikation vor LIVE | Normativ vorhanden, im ZIP noch keine vollständige Acceptance-Kette; späteren Gate-0-Prototyp-PASS nicht als Waiver nutzen. |

Quelle der Scope-/Messwerte: Master-Ledger, Testplan, Execution-Checklist, Runtime-/Roadmap-Verträge und ausgewählte unveränderte Raw-Resultate. Diese Prüfung führt historische Netzwerk-/LIVE-Versuche nicht erneut aus. Unterschiedliche historische Testcounts sind zeitliche Stände, keine addierbaren unabhängigen Gesamt-Testzahlen.

### P1 — Ordered-Inclusion-Vertrag in Einheit 5 ist zu schwach

Fund: Inventar §3 „Payload-Bytes“ und §9 Einheit 5. `eth_sendBundle`-Optionen sind nur allgemein genannt. Dagegen verlangen Git-Checklist und vorhandener `g06a_self_launch_bundle.rs` konkret:

- öffentliche vollständige Launch-TX: originales `[raw_signed_launch, raw_signed_buy]`, keine Re-Signierung fremder Launches;
- 48Club für den kontrollierten Qualifikationspfad: `noMerge=true`, `positionFirst=true`, enger `maxBlockNumber`;
- identische TX-Bytes/Hash/Nonce auf 48Club und BlockRazor, Anbieter-Envelope gemäß dessen aktueller verifizierter Semantik;
- privates Target separat: `backrunTarget`/hinreichende Simulationsfakten, kein Hotpath-RPC und kein geratenes öffentliches Launch-Raw;
- kontrollierten Mainnet-Self-Launch nicht vorher öffentlich broadcasten;
- Gas-/Bid-/Expiry-/Budgetprofil vor Send verifiziert. Ein niedriger Gas-Acceptance-Floor garantiert kein N+1; 1 gwei ist gespeicherte Stage-1-Policy, kein bewiesenes 48Club-Minimum.

Ohne diese Übernahme kann Einheit 5 formal korrekte Requests produzieren und dennoch das First-BUY-Ziel verlieren. Reale Platzierung entscheidet, nicht bloßer Methoden-/TLS-PASS. Fehlender Zugang bleibt explizit nicht qualifiziert, ohne Einheit 1/2 zu blockieren.

### P1 — Einmaliger Intent-Permit versus mehrere Builder-Submissions

Fund: ZIP fordert mehrere Adapter mit gleicher intent_id, Inventar will zugleich die alte `guarded_transport_write`-Semantik übernehmen. Der alte `IntentLane::begin_send()` wechselt einmal von Prepared auf Sending; jeder weitere Aufruf vor kanonischer Bestätigung liefert NotReady. Zwei unabhängige Writes, die jeweils dieselbe Lane erneut konsumieren, würden den zweiten Builder nicht bedienen. Zweite Lane/Nonce würde dagegen den Economic-Dedup-Vertrag unterlaufen.

Für Einheit 4/5 explizit trennen: **eine Economic-Intent-/Nonce-Admission, begrenzte einmalige Write-Berechtigung je ausgewähltem Adapter für dieselben finalen Bytes**. Pro Adapter Verlauf/ACK/Partial-Write/UNKNOWN erfassen; Gesamtstatus und Fail-closed-Policy definieren, kein blindes Retry und keine neue BUY-TX. Fan-out nicht von der ACK/Resolution des ersten Builders abhängig machen. Der derzeitige single-sink Gate-0-Test beweist diesen neuen Mehrfach-Transport-Lifecycle nicht.

Acceptance: zwei Builder erhalten identische geordnete TXs, nonce nur einmal reserviert; reversed ACK, ein Adapterfehler, Partial-Write, Duplicate-Signal und Crash ändern nie die Economic-Identität. Single-Sender bleibt.

### P1 — Pre-resident State aus G0.2 ist zu allgemein übertragen

Die neue „versionierte Konstanten/stale-state“-Formulierung reicht nicht als konkrete Übernahme der 497-zeiligen Matrix. Einheit 2/3 muss aus der vorhandenen Matrix einen scope-genauen Input-/Admission-Vertrag bekommen; nicht 277 Slots in jeder Entscheidung einzeln durchlaufen.

Mindestens verbindlich: `block_hash + ordered_state_cursor + pending_candidate_identity + policy/codehash_generation`, konsistent veröffentlichter Snapshot; Registry-/Proxy-/Code-Invalidierung; Quote-/Pool-/Tick-Fenster; Extension-/Oracle-Digest, erlaubte Rolle und Deadline-Reserve; Fee-Exemption/Tax/Caller-Effekte; Wallet/Nonce/Funding; Creator-Postlaunch-State; Salt-/Deployment-Kollision. Unbekannte oder unqualifizierte Branches → SKIP/UNSUPPORTED, kein geratenes 0 und kein Hotpath-Nachladen.

Historische DOGE/V3-0,15-BNB-Evidence ist single-segment und caller-/stategebunden. Native-BNB Curve-20 ist anderer Scope. Weder „newTokenV6“ noch „TOKEN_TAXED_V3“ allein qualifiziert jeden Modus. Produktions-Slippage bleibt explizite Policy, kein automatisch übernommener historischer/Testwert.

### P1 — Alte IntentLane-Datenstruktur ist kein vorallokierter Produktionspfad

Fund: `IntentLane` benutzt `BTreeMap::new()` und `entries.insert()` bei jeder Reserve. Der vorreservierte HashSet verhindert diese Map-Allokationen nicht. Inventar „Zustandsautomat übernehmen“ muss deshalb Semantik und Layout trennen.

Einheit 4: begrenzte vorallokierte Entry-/Dedup-Struktur, explizites Capacity-Fail-closed, exklusiver Hotpath-Owner ohne aus den Tests übernommenen Mutex. Alle vorhandenen Reihenfolge-/Dedup-/Cross-Chain-/UNKNOWN-Fälle mit der neuen Struktur erhalten. Allokationen/Locks im integrierten Pfad gezielt prüfen, ohne Optimierungserfolg vor Messung zu behaupten. Kein Multi-Sender-Umbau.

### P1 — Prepared-Pfad und On-Signal-Finalisierung benötigen getrennte Acceptance

Die normale neue `sign_legacy`-API ist sinnvoll; sie allein übernimmt noch nicht den bewiesenen vor-T0-Weg. Für vorbereitbare kontrollierte Launches ABI/Quote/Nonce/Signatur/JSON- bzw. Anbieter-Envelope vor dem Signal finalisieren, auf Signal nur unveränderlich gebundene Payload auswählen und senden. Für fremde Launches nur die wirklich unbekannten Felder nach Eingang finalisieren.

Beide verwenden dieselben Semantikfunktionen, keine alternative schnelle Crypto-/Quote-Implementierung. Invalidierung muss Header/Reorg, Nonce, Gas/Policy, Deployment/Mode, relevante State-Änderungen und Launch-Replacement abdecken. „HASH_ONLY“ ist ausschließlich actionable bei bereits vollständiger verifizierter Launch-Zuordnung; aus einem unbekannten Hash darf die alte Self-Launch-Abkürzung nicht entstehen.

Messung umfasst nach der gewählten Signalgrenze ALLE verbleibende Serialisierung/Queue/Transportarbeit. Vorbereitungszeit, Hash→Full und verpasste Signale separat erhalten. Ein benchmarked presigned Self-Launch-PASS darf den noch nicht qualifizierten externen Full-Launch-Pfad nicht freigeben.

`T_ready` ist die tatsächliche ursprüngliche Fertigstellung der exakten signierten Payload. Bei Pre-Signing liegt sie vor `T_signal`; ein späterer select-only-Timestamp darf sie nicht ersetzen. Auswahlzeit separat speichern, wiederverwendete Payload an Profil-/Snapshotgeneration binden. First-Seen und spätere Full/Enrichment-Stempel nicht überschreiben. Finales Engineering-Gate auf dedizierter Linux-Zielhardware nach Runtime-Contract §11, mit Rohsamples, Max, Deadline-Misses und erforderlichen Tail-Perzentilen; p99 <1 ms allein beweist keine harte Maximalgrenze. Deadline-Policy und Partial-Write-Verhalten am tatsächlichen Adapter festlegen, ohne einen bereits begonnenen Send als sicher ungesendet zu deklarieren.

### P1 — Mess- und Economic-Capture-Vertrag vollständig machen

Die fünf konkreten Clock-/Producer-/Socket-/Hash-/Fensterkorrekturen weiter oben bleiben nötig. Zusätzlich muss Minimal-Capture ausdrücklich aus Rohbelegen ableiten:

- tatsächlichen positiven Erwerb beim vorgesehenen Recipient und attributable debit inklusive Gas/Fees/Tips;
- Creator-/Launch-internen BUY separat, externe BUYs einschließlich Router-internen Portal-Events;
- vollständige TX-/Receipt-/Log-Abdeckung beider Randblöcke und des restlichen Fensters, log_index-Eindeutigkeit;
- atomare Beträge, Asset/Decimals-Provenienz, Fee/Tax, Fill-Preis versus Posttrade-Spotpreis;
- Submission je Adapter, alle Fehler/Skips/UNKNOWNs im Nenner;
- Producer-Uhr/Witness oder explizit UNKNOWN; tatsächliche Timestamp-Auflösung statt erfundener ms-Ticks.

Das jüngste Raw-Ergebnis hat bereits komplette Receipts beider Blöcke und genau ein Portal-TokenBought dieses Tokens. Die letzte Ledger-Korrektur zu Router-Events muss übernommen werden; den alten Top-Level-only-Scanner nicht erneut als vollständige Konkurrenzprüfung einsetzen.

### P2 — Journal/Replay und alte Gegenbeispiele brauchen Owner

C0 soll nicht nur einen späteren Logger erzeugen. Normativ verlangt: ein versioniertes append-only binäres Journal mit Sequenz, Checksum/Manifest und rohen Verlust-/Reorg-Belegen, deterministisches Replay durch denselben Rust-Core. Im ZIP-Minimalvertrag stehen Record-Arten, aber noch keine konkrete Abnahme für Integrität, abgeschnittene Frames, Replay-Identity und veränderliche Borrow-Lebensdauer im asynchronen Recorder. Zuständigkeit und Schritt festlegen; bei Minimal-Capture nicht heimlich eine JSON/SQLite-Primärquelle schaffen. Keine synchronen Writer-Waits im BUY-Pfad.

Für C2/Recorder die in `BSC_PHASE_C_PYTHON_RETIREMENT.md §3` gelisteten Gegenbeispiele einzeln zu Rust-Testfällen/Ownern zuordnen. Besonders: Beobachtungsreihenfolge beweist keine Nonce-Replacement-Entscheidung, late pending darf Canonical nicht umschreiben, Reorg muss Quote-/Nonce-/Intent-Provenienz invalidieren, keine stillen Raw-/Receipt-Lücken. Die Startsperre ersetzt keine vollständige kanonische Reconciliation.

### P2 — Dokumente/Phasen dürfen keine falsche Freigabe erzeugen

Roadmap und Handoff enthalten ältere BLOCKED/G0.2-PARTIAL-Zeilen; das Ledger enthält späteren Scope-PASS. Umgekehrt bleibt das reale numerische Ziel BUY <1 ms offen. Nutzerauftrag erlaubt jetzige Konsolidierung; keine erneute Genehmigungsfrage dafür, aber „Bau frei, Produktions-LIVE gesperrt“ und die aktuelle genaue Scope-Tabelle schriftlich vereinheitlichen.

Der kurze Handoff endet Gate 0 bei G0.9, die Roadmap führt zusätzlich G0.10/11: Nummerierung/Deliverables angleichen, keinen stillen Gate-Sprung. Vor finalem LIVE ist die volle D/E/G/H-Pipeline inklusive realem Transport erneut zu qualifizieren; nicht nur Einheit-1-Microbenchmark oder PAPER-Sink.

Solana bleibt erhalten und eigenständig. Flap/Four Classic/OpenFour nur entsprechend eigener Evidence erweitern. Strategy C später aus eigener BSC-State/Economics ableiten; Solana-Observation-T0 und BSC-Execution-T0 nicht gleichsetzen. Kein automatischer BSC-EV-PASS aus Solana-Latenzreplays oder erfolgreichem Erstkauf.

### Vollständige Rückgabe an Fable, ohne neue Evaluationsschleife

1. Compiler-/Format-/Lock-Ergebnisse dieser Prüfung übernehmen; Einheit 1 braucht keinen Neustart.
2. Inventar um diese fehlenden Input-/Lifecycle-/Capture-/Acceptance-Verträge samt Belegpfad und zuständiger Einheit ergänzen. Keine allgemeinen „G0.5 PASS“-Etiketten ohne Scope/Boundary/Samples.
3. Clock-/Minimal-Capture-Vorschlag wie oben präzisieren. Golden-Regressionen und negative Scope-/Stale-/Replay-Fälle je Verbraucher übernehmen, nicht historische Binaries komplett umbauen.
4. Danach saubere Revision prüfen/importieren, Einheit 2 weiterbauen. Noch nicht alle späteren Subsysteme jetzt implementieren. Die verbindliche Abnahmematrix verhindert deren falsche Umsetzung.

**Freigabeprinzip:** „Compiler grün“ schließt Einheit-1-Korrektheit ab; „Anforderung übernommen“ verlangt einen nachweisbaren Owner und Test; „wirklich gekauft <1 ms“ verlangt originale kanonisch gebundene Execution-Evidence. Keine dieser drei Ebenen ersetzt die andere.