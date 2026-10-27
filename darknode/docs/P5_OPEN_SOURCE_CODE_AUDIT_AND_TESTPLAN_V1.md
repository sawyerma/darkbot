# DarkNode P5 – Open-Source-Code-Audit und einheitlicher Sicherheits-Gesamttest V1.2

**Stand:** 27.10.2026 · **Status:** Quellcode-Recherche zum fixierten Stand, keine P5-Laufzeitabnahme · **P5 = NOT_RUN, GATE 1 = OPEN, SIGN/ARM/LIVE verboten.**  
**Bindend:** [PHASENPLAN_P5_V1.md](PHASENPLAN_P5_V1.md), besonders S0–S7 und P5-A01–A28. P3 bezeichnet die Indikator-/Node-Engine, P5 die Private Execution. Keine Änderungen an BSC/Project_0, keine parallele Architektur.

## 1. Source-Baseline (exakt gepinnte Upstream-Commits; verifizierte Git-Refs)

| System | Upstream-SHA | Lizenz laut GitHub-Repository-Metadaten | Nutzen/Status |
|---|---|---|---|
| [NautilusTrader](https://github.com/nautechsystems/nautilus_trader) | `6a64881b9f54f3ca79a322e980dbef51fd5482a9` | LGPL-3.0 | Typisierte Risk-/Execution-Engine und Persistenz-/In-flight-Prinzip, **nicht** kopieren ohne Lizenz-/Kompatibilitätsprüfung |
| [Freqtrade](https://github.com/freqtrade/freqtrade) | `dafa0b6f4e1af30a80aedf730b5509ca59d0d7cc` | GPL-3.0 | On-exchange-Stop-Management, Emergency-Exit, Exchange-Integrationsmatrix; **Architektur- und Testreferenz**, keine direkte Übernahme GPL-Code |
| [Hummingbot](https://github.com/hummingbot/hummingbot) | `9af100d6822da7d2d0291a906c730ef172284ee2` | Apache-2.0 | Active/Cached/Lost Order-Tracker, In-Flight-Trades, späte Fills; Codeübernahme nur bei konkret qualifizierter, erforderlicher Funktion |
| [CCXT](https://github.com/ccxt/ccxt) | `133fd945e5dabea9b08a3d6b1012cefe1ef9713b` | MIT | Exchange-/Instrumentpräzision, Leverage/Margin/Position-Methodik; **Bitget-UTA-V3-Kompatibilität pro Methode prüfen**, kein ungeprüfter Classic-V2-Fallback |
| [barter-rs](https://github.com/barter-rs/barter-rs) | `9770b27a83f844472b93b593b08063affc974b0d` | MIT | Rust typed exchange/event/portfolio-pattern, **Bitget-UTA-P5-Abdeckung nicht nachgewiesen** |

**Wichtig:** Die SHAs pinnen den Repository-HEAD am Abrufzeitpunkt. Die unten ausdrücklich genannten Fundstellen in konkreten Quelldateien wurden gelesen; der übrige komplette Codebestand dieser fünf Projekte wurde **noch nicht vollständig statisch/ausführbar auditiert**. Die Upstream-SHAs allein belegen keine Funktionsparität. Als nächste Stufe exakte Datei-Blob-SHAs und Testfall-Referenzen beilegen, wo eine Implementation tatsächlich übernommen wird.

## 2. Konkret gelesener Open-Source-Code und übertragbare Mechanismen

| Quelle / exakte Codelocation | Beobachtetes echtes Verhalten | Für DarkNode zu übernehmen | Was NICHT bewiesen ist |
|---|---|---|---|
| [Nautilus live Rust config `crates/live/src/node/config.rs`](https://github.com/nautechsystems/nautilus_trader/blob/6a64881b9f54f3ca79a322e980dbef51fd5482a9/crates/live/src/node/config.rs) `LiveRiskEngineConfig`, etwa Zeile 189 ff. | Getrennte `max_order_submit_rate`, `max_order_modify_rate`, `max_notional_per_order` pro Instrument; Konfiguration enthält auch explizites `bypass`-Flag | Zentraler eigentümergebundener Risk-Owner mit nominalen Limits/Rate-Gates, **kein erlaubter Prod-Bypass** | Dass alle Edge-Cases der Upstream Risk-Engine sicher sind; konfigurierbarer `bypass` ist gerade ein Konfigurationsrisiko |
| [Nautilus live config](https://github.com/nautechsystems/nautilus_trader/blob/6a64881b9f54f3ca79a322e980dbef51fd5482a9/crates/live/src/node/config.rs), `LiveExecutionEngineConfig`, etwa Zeile 409 ff. | `snapshot_orders`, `snapshot_positions`, periodische Position-Snapshots, Cache-Restore-Optionen | Persistente Zustandsprojektion und verifizierbarer Restart-Reconcile | Kein beweisbar crash-atomarer SQLite/Datei-Dual-Commit ohne eigenen Test |
| [Freqtrade `freqtrade/freqtradebot.py`](https://github.com/freqtrade/freqtrade/blob/dafa0b6f4e1af30a80aedf730b5509ca59d0d7cc/freqtrade/freqtradebot.py) `handle_stoploss_on_exchange` ~1637 | Sucht existierende SL-Orders an der Börse und prüft/neuerstellt deren Zustand; TODO zu Liquidationspreis ausdrücklich im Code | Schutzorders gegen **Exchange-Orderstatus + Position** prüfen, statt nur lokales Flag | Bitget UTA V3 Preset-SL-/Partial-Fill-Semantik nicht durch Freqtrade bewiesen |
| [Freqtrade `freqtradebot.py`](https://github.com/freqtrade/freqtrade/blob/dafa0b6f4e1af30a80aedf730b5509ca59d0d7cc/freqtrade/freqtradebot.py) ~341, `_exit_lock` | Lock zur Vermeidung von Kollision zwischen Exit und Stoploss-Recreation aus anderem Thread | Strikte serialisierte Ownership/CAS über Close/Stop-Replacement und Reconcile | Kein automatischer Schutz vor allen verteilten Timeout-/late-Fill-Races |
| [Freqtrade `freqtradebot.py`](https://github.com/freqtrade/freqtrade/blob/dafa0b6f4e1af30a80aedf730b5509ca59d0d7cc/freqtrade/freqtradebot.py) `handle_trailing_stoploss_on_exchange` ~1734 | Preispräzision und ggf. Cancel-/Replace-Verarbeitung mit Exchange-/Zeit-Gates | Monotoner Stop-Schutz, Cancel/Replace als eigene mögliche Deckungslücke behandeln | Exchange-native Trailing-OCO ist nicht automatisch identisch |
| [Hummingbot `client_order_tracker.py`](https://github.com/hummingbot/hummingbot/blob/9af100d6822da7d2d0291a906c730ef172284ee2/hummingbot/connector/client_order_tracker.py) ~59–117 | `_in_flight_orders`, `_cached_orders`, `_lost_orders`; `all_fillable_orders` schließt **auch verlorene** Orders ein; `all_updatable_orders` schließt lost ein | „Lokale FAILED/LOST“-Order bleibt für späte Fills/Status-Events correlatable | Unterschiedliche Connectoren können zusätzliche Bugs enthalten |
| [Hummingbot `position_executor.py`](https://github.com/hummingbot/hummingbot/blob/9af100d6822da7d2d0291a906c730ef172284ee2/hummingbot/strategy_v2/executors/position_executor/position_executor.py) ~364–383 und ~699–718 | Close-Order gilt lokal als „nicht gefunden“ → `failed_orders` und `_close_order=None`; auch bei Failure-Event wird `_close_order` freigegeben | **Negativtest**: verspäteter Fill nach verloren/fehlgeschlagenen Close darf niemals zweite risikosteigernde oder doppelte Close auslösen; persistente alte ID für späte Fills behalten | Die konkrete Fehlerfolge aus Issue #7295 war an einem anderen Connector, nicht als Bitget-Nachweis übertragen |
| [Hummingbot `position_executor.py`](https://github.com/hummingbot/hummingbot/blob/9af100d6822da7d2d0291a906c730ef172284ee2/hummingbot/strategy_v2/executors/position_executor/position_executor.py) ~481–500 | `place_close_order_and_cancel_open_orders` berücksichtigt `amount_to_close` vs Mindestgröße und storniert offene Orders | Teilfills/Restexposure/Close-Quantity samt Positionsbestätigung berechnen, nicht vom Order-ACK | Nachweis echter Bitget `reduceOnly`/Hedge `posSide` nötig |
| [CCXT Bitget Adapter, SHA-pinned](https://github.com/ccxt/ccxt/blob/133fd945e5dabea9b08a3d6b1012cefe1ef9713b/python/ccxt/bitget.py) (ca. 9581–9730) | set_leverage: UTA V3 nur bei uta=True; sonst Classic V2. set_margin_mode: ausschließlich Classic V2; set_position_mode: UTA V3 nur mit uta=True; fetch_positions: Default Classic V2 | V2/V3-Routing-Tests und offizieller UTA-V3-Contract erforderlich, kein stiller V2-Fallback | CCXT nicht unbesehen als fertigen UTA-V3-Private-Adapter einsetzen |
| [barter-rs, gepinnter Tree](https://github.com/barter-rs/barter-rs/tree/9770b27a83f844472b93b593b08063affc974b0d) | Im gesamten gepinnten Tree 0 Dateipfade mit Bitget; Execution-Code für andere Exchanges unter barter-execution/src/client | Rust-Patterns und Traits nach Kompatibilitätsprüfung | Keine fertige Bitget-UTA-Orderimplementation nachgewiesen |

**Reuse-Regel:** Exakte testbare Datenmodelle und fail-closed Zustandsübergänge übernehmen, *nicht* vollständige Fremd-Bot-Architektur mit konkurrierendem Exchange-/Risk-Owner einbetten. LGPL/GPL/MIT/Apache-Lizenz- und Copyright-Auflagen vor Codekopie separat prüfen. Was Freqtrade/Hummingbot belegen, ist primär ein **bewährtes Muster und regressionsrelevanter Fehlerfall**, nicht automatisch ein sicheres Bitget-UTA-Produkt.


### 2a. Geprüfte Upstream-Testdateien (konkrete Fälle)

| Upstream-Testdatei (exakter SHA) | Tatsächlich gefundene Tests | DarkNode-Gates |
|---|---|---|
| [Freqtrade stoploss tests](https://github.com/freqtrade/freqtrade/blob/dafa0b6f4e1af30a80aedf730b5509ca59d0d7cc/tests/freqtradebot/test_stoploss_on_exchange.py), Blob \`abd2f562078ee80bca2e50c21f6650f1433f146c\` | 16 Funktionen einschließlich \`test_handle_stoploss_on_exchange_emergency\`, \`test_handle_stoploss_on_exchange_partial\`, \`test_handle_sle_cancel_cant_recreate\`, \`test_create_stoploss_order_insufficient_funds\`, \`test_handle_stoploss_on_exchange_trailing_error\` | A17–A20/A28; Port als Rust-Assertions, keine GPL-Codekopie |
| [Freqtrade Bitget exchange tests](https://github.com/freqtrade/freqtrade/blob/dafa0b6f4e1af30a80aedf730b5509ca59d0d7cc/tests/exchange/test_bitget.py), Blob \`28059ead8b8d20725011e9aec6117589709e3394\` | 9 Funktionen einschließlich \`test_fetch_stoploss_order_bitget\`, \`test_cancel_stoploss_order_bitget\`, \`test_dry_run_liquidation_price_cross_bitget\`, \`test__lev_prep_bitget\` | A03/A15/A16/A19, aber kein echter Bitget-UTA-V3-Demo-PASS |
| [Hummingbot order tracker tests](https://github.com/hummingbot/hummingbot/blob/9af100d6822da7d2d0291a906c730ef172284ee2/test/hummingbot/connector/test_client_order_tracker.py), Blob \`859f9a71c14465d72ab464b7ed74d96c9e12d110\` | 35 Methoden, darunter \`test_lost_orders_returned_in_all_fillable_orders\`, \`test_lost_order_removed_when_fully_filled\`, \`test_lost_order_not_removed_when_updated_with_non_final_states\`, \`test_updating_order_states_with_both_process_order_update_and_process_trade_update\` | A08/A21/A22/A27: Failed/Lost bis Exchange-Endstatus und späte Fills identifizierbar halten |
| [Nautilus Rust Risk Tests](https://github.com/nautechsystems/nautilus_trader/blob/6a64881b9f54f3ca79a322e980dbef51fd5482a9/crates/risk/src/engine/tests.rs), Blob \`2fccc7e2f73de480ef770aeb664ad8623e7a2e95\` | Testmodul gefunden; noch keine reproduzierte Ausführung oder vollständige Fallzahl-Auszählung | Rate-/Notional-/Risk-State-Assertions konkret mappen und mit Rust-P5 laufen lassen |

**Zusätzlicher Upstream-Sicherheitsbefund:** NautilusTrader hat ein konfigurierbares Risk-\`bypass\`-Flag; die dokumentierten Trading-State-Checks sind bei aktivem Bypass nicht wirksam. In DarkNode dürfen produktive Risk-Gates keinen konfigurierbaren Bypass besitzen. Die geprüften Upstream-Testdateien wurden gelesen, aber nicht hier ausgeführt.

## 3. Ein gemeinsames ausführbares Abnahmeprogramm statt 28 einzelner Terminal-Häppchen

**Gesamttest-ID: `DN-P5-ALL-001`.** Ein **einziger** Runner gegen fixiertes Git-Commit und versionierte Szenario-/Exchange-Captures; Ausgabe einer einzigen menschlich lesbaren Test-/Fehlermatrix und reproduzierbarer DBREC-Evidenz. Die Testlogik ist **noch nicht implementiert**, also **`NOT_RUN`**. Ein Script, das blind die Strings `PASS` für noch nicht vorhandene Testfälle druckt, ist verboten.

Die gleiche Testmatrix läuft in getrennten, expliziten Sicherheitsumgebungen:

| Lane | Zugriff | Aufgaben | Freigabe-/Resultatregel |
|---|---|---|---|
| **A – Source + Contract** | Read-only Git, API-Docs, Parser/Signer off-line | P5-A01–A28 gegen Dateien/Funktionen/Test-IDs und dokumentierte UTA-Endpunkte mappen; API-Signatur/Decimals/modes verifizieren | `PASS/FAIL/NOT_IMPLEMENTED/NOT_RUN`, jedes Urteil mit Datei+Zeile+Commit |
| **B – Recorded Fault Replay** | **Kein Netzwerk, kein Credential**; nur echte versionierte Captures + explizite negative Eingangsmanipulation | ACK verloren, Send/Commit-Kill, late fill, Partial-fill, Replace/Cancel, unknown Orders, Stop verloren, Drift, Underflow/Overflow, Leverage falsch, Mark-vs-Last, Restart-Reconcile | Golden State-/Exposure-/No-Duplicate-Hashes; synthetisch **nur als gekennzeichnete Fault-Injection**, nicht als Beweis realer Börsenfähigkeit |
| **C – Bitget Public Read-Only** | Kein Key | Vollständiger USDT-Futures-Markt-/Precision-Katalog, Instrumentstatus, Update-/Rate-Limits/Depth/Trade-Referenzen | Reale API-Antworten + Capture-Hashes, keine Order |
| **D – Private Read-Only Demo** | **Eigene, restriktive Demo-Read-Credentials** nach Nutzerkonfiguration | `settings`, `assets`, Positionen, unfilled Orders, Stops und private WS; Balance/Mode/Leverage-Drift/No-Position-Readback | Kein Mutation-Request; `BLOCKED` ohne Key, **nie** ersatzweise Mainnet-Trade-Key |
| **E – Mutierende Bitget-Demo-Akzeptanz** | Separater Demo-Trade/Management-Key, dediziertes Testkonto, **explizite Freigabe vor Aktivierung** | Hebel/Mode setzen, Market/Limit/IOC/FOK, Teilfill, TP/SL/Stop-Replace, Cancel, late ACK, Hostkill, Emergency, CountdownCancel gesondert | Jede mutation ordnungsgemäß journalisiert und über Bitget wirklich bestätigt, keine automatische Freigabe durch diesen Plan |
| **F – Betrieb + Stress/Soak** | Echten freigegebenen Demo-Host, externer Alarmkanal | WS-Ausfälle, Clock-Drift, Rate-Limits, Disk-full, Outbox, Alarm-SLA, p50/p95/p99, Restart, 24/7 / mehrtägiges Soak | Separater Dauerzeitnachweis; nicht in wenige Minuten zusammenfassen |

**Kritisch:** Die Lane E ist **kein** Auftrag, jetzt Orders oder Key-Uploads auszuführen. A–C sind ohne private Rechte vorplanbar; D benötigt eigene Zugangsdaten, E ausdrückliche Freigabe nach A–D und unabhängige Sicherheitsbedingungen.

### Einheitliche Negativ-Szenarien (an P5-Axx gebunden)

| Bereich | Cases im DN-P5-ALL-001 | Erwarteter Sicherheitsbeweis |
|---|---|---|
| Kapital + Risk | P5-A13/A14; 1–1000× konfigurierter Hebel, falsches Contractmultiplier/Decimal, Qty/Notional-Overflow, freie Margin vs volle Risk-Exposure, unbekannte Reservation, parallele Bot-Intents | **0** über Budget/Nominalwert/Menge genehmigte Entries, selbst bei falsch konfiguriertem Hebel |
| Konto/Hebel/Liquidation | A02/A03/A04/A09/A15/A16; falsche UTA-/Margin-/Hold-Mode, Position nach vermeintlichem FLAT, Response OK aber Readback falsch, fehlerhafte/fehlende Liquidation | `NO_ENTRY/HALTED`, kontrollierter Reduction-/Stop-Policy-Pfad |
| Stop/Emergency | A07/A17/A18/A19/A20/A28; Partial-fill mit inkorrekter Stop-Qty, abgesagter Stop, Marktgap, stop-limit nicht gefüllt, stale Private WS, externer Cancel | Keine still ungeschützte Position als `ACTIVE`; kontrollierter Replacement-/Emergency-Intent + Alarm nach erklärtem Ausgang |
| ACK/Fills/Cancel/Recovery | A06/A08/A12/A21/A22/A27; Send committed/no ACK, Cancel ACK aber late FILL, FAILED close→später Fill, Restart exakt an jedem Persistenzpunkt, Fremdorder, Duplicate private WS | Keine doppelte Entry-/Close-/Flip-Order, alle bestätigten Exchange-Fills genau einmal journalisiert |
| Trading-State + UI | A10/A23/A24/A26; HALTED, REDUCING, manuelles Emergency, unautorisierte Gateway-Commands, Browserausfall, Risk-bypass-Attempt | Nur autorisierte, mengenbegrenzte Reduktionsaktion; kein `ACTIVE` bei unsicherem Exchange-/Stop-Zustand |
| CountdownCancel | A25; unbekannte API-Freischaltung, Stop-/Strategy-Order-Cancel unter Deadline und Hostausfall | Funktion **OFF**, bis echte Demo-Beweise zeigen, dass nötiger Schutz bleibt; ansonsten bei offenem Schutz deaktiviert |

### Evidenzformat – pro Szenario verpflichtend

`source_commit, upstream_sha, test_id, lane, mode, instrument_version, account_mode, fixture_id/hash, exchange_event_id/clientOid, input_canonical_hash, state_before/state_after, pos_before/after, worst_exposure, stop_coverage, flags, assertions, status, reason, output_hash, timestamp`. Secrets, Passphrase, volle Keys, personbezogene Handelsdaten nicht loggen oder in Git schreiben. Intern Typen/Protobuf/DBREC, **kein JSON in produktivem Runtime-Hot-Path**. Offline-Testreport darf lesbare Markdown/DBREC erzeugen.

Für **jeden** A01–A28: `PASS/FAIL/BLOCKED/NOT_RUN` basierend auf tatsächlich gelaufenen Testcases. Fehlender Code = `NOT_IMPLEMENTED/NOT_RUN`, fehlende erforderliche Demo-Rechte = `BLOCKED` **nur für diese Lane**, nicht für Source-/Offline-Test. Mehrere Gates dürfen nicht durch einen generischen „16 PASS“/Wrapper-Exit kaschiert werden.

**P5-Ausgang:** Alle erforderlichen A01–A28 in den festgelegten Lanes PASS, Coverage für relevante Bitget-UTA-Modi nachgewiesen, keine UNKNOWN-Order, kein ungeschützter gemeldeter READY-Trade, Not-Aus-Fälle getestet; abschließende Demo-Stress-/Soak-Evidenz. Erst dann separate Prüfung der LIVE-Berechtigung gemäß P9; P5-PASS alleine erteilt **kein** LIVE.

## 3a. Erweiterte Negativfälle – Ergänzung 27.10.2026

Die P5-V1.2-Präzisierungen in [PHASENPLAN_P5_V1.md](PHASENPLAN_P5_V1.md#5b-präzisierungen-v12--p5-sicherheit-identische-a01a28-27102026) erweitern **bestehende** A01–A28; **keine zusätzlichen fiktiven PASS-Tests**, kein neues Testprogramm.

| Proof-Lane und bestehende Test-IDs | Neu zu prüfende Assertions | Status |
|---|---|---|
| B · A11/A14 | `Qty = floor_step(min(risk_budget / loss_per_base, notional_cap/entry, qty_cap, ...))`: Preis-/Mengen-Einheiten, Gebühren, Slippage-Reserve, Funding, Min-Notional und falscher Hebel; 00:00-Equity mit/ohne externe Cashflows, fehlende Snapshots | NOT_RUN |
| A/B/D/E · A17–A22/A27 | `clientOid` bei `type=tpsl`, sechs Stunden **keine garantierte Idempotenz**, keine ID-Annahme bei anderen Typen; REST/WS-Vergleich von `orderId`, Stopmenge, `full/partial`, Stop-Modify und nachgelieferten Fills | NOT_RUN |
| B/D/E · A08/A20/A26/A27 | ADL/liquidation/forced-reduce oder externe Positionsänderung ohne eigenen Fill, verzögerte WS-Events, Reboot, Stop-Überdeckung und Phantom-FLAT | NOT_RUN |
| A/B/E · A02–A07/A15/A28 | Kontomodus nur nach Readback freischalten; blockierte Ordertypen/Modi fail-closed, `mark`-/`market`-Trigger, Stopgap, Fees-/Funding-Schock | NOT_RUN |
| P4/P8/P9 · Wirtschaftlichkeit, **separat von DN-P5-ALL-001** | OOS/WFA netto nach Gebühren/Spread/Slippage/Funding, Non-Fills und tatsächlicher Positionsgröße; echte Demo-Fills nicht als Live-Slippage behandeln; vorab fixiertes Kriterium für ökonomische LIVE-Qualifikation | NOT_RUN |

Die 28 A-IDs sind der **technische** Sicherheits-Gesamttest. Ökonomische Strategiequalifikation ist die **eigene** P4/P8/P9-Abnahme, nicht durch eine weitere P5-Runner-Flag automatisch erfüllt. Der `DN-P5-ALL-001`-Runner wird damit nicht als bereits lauffähig ausgewiesen.


## 3b. Latenz- und Transportabnahme – P5 V1.3 / P8

**Verbindliche Referenz:** [P5 V1.3, Abschnitt 5c](PHASENPLAN_P5_V1.md). `DN-P5-ALL-001` bleibt ein Runner gegen A01–A28, **nicht** ein fiktiver zusätzlicher PASS-Test. `NOT_RUN`, bis ein produktiver Rust-Owner, versionierte Fixtures, qualifizierter Demo-Host und Messwerte vorliegen.

- **Lane A:** Order-/Risk-/WS-/Journal-Aufrufpfad und vorhandene Rust-Tokio-Komponenten zeilengenau auditieren; keine blockierenden Locks/DB/GUI-Zugriffe in reiner Signalrechnung; keine ungeprüfte Default-/biased-`select!`-Priorität; keine getrennten konkurrierenden Owner; REST- und WS-Case-Semantik für `stopLoss`/`stoploss` und ACK vs Fill typisiert.
- **Lane B:** deterministisches Replay mit Burst/Backpressure, Journal-`fsync`-Stall/Disk-full, Emergency gegen Partial-Fill/late-Fill, WS Disconnect unmittelbar nach Netzwerk-Send, Konkurrenz von Entry und Stopverlust; pro Failure die `NO_ENTRY`-/`UNKNOWN`-/Exposure-/Stop-Invarianten prüfen.
- **Lane D/E:** explizit genehmigte Bitget-Demo-Capabilities getrennt für REST-Place-Order und private WS-Place-Order mit Preset-SL/Reduce-only, deduplizierbarem `clientOid` und tatsächlich vorhandenem Stop; erst dann latenzbezogene Transportentscheidung; kein automatischer REST-Fallback nach unbekanntem WS-Send.
- **Lane F/P8:** pro `L1–L9` aus P5-§5c min/p50/p95/p99/p99.9/max mit Queue-Wait, `fsync`, Netzwerk-Send, ACK, Fill und Stop-Bestätigung, unter Host-Last/Netzausfall und über den vorgesehenen Soak. Grenzwerte **vor** Abnahme festlegen; fehlende p99-Assertions ⇒ `NOT_RUN/BLOCKED`, niemals `PASS`.
- **Safety-vor-Speed:** Niemand darf die Intent-Persistenz, Risk-Reserve, Exchange-Reconciliation oder Stop-Bestätigung abschalten, um eine Latenzmetrik zu verbessern. Gute CPU-Zahlen ohne getestete Exchange-Latenz oder fehlender Stopdeckung sind **kein** erfolgreicher Gesamttest.

## 3c. Wallet-/Account-Synchronisation – Pflichtassertions in bestehenden P5-A-Tests

Verbindliches Dokument: [WALLET_ACCOUNT_SYNC_V1.md](WALLET_ACCOUNT_SYNC_V1.md). **Alle folgenden Nachweise NOT_RUN.** In `DN-P5-ALL-001` ergänzen statt neuer Test-ID: **A02/A13** Account-Identität/DEMO/USDT-vs-USD; **A08/A12/A26/A27** WS-Reconnect/REST-Snapshot-Barrier, Order-Kanal ohne initiale vollständige Liste, verspätete Event-Reihenfolge, Bitget-App-/Transfer-/Fremdorders und Resync ohne Doppelorder; **A11/A14** Equity/Available/Reservierung/Positionsgrößenkontrolle inkl. fremder Exchange-Exposure; **A17–A20/A28** echte Stopdeckung gegenüber Exchange-Position; **A24/A26/P7** Wallet/Overview/Live aus **demselben** Rust-Protobuf-Account-State, Stale-Anzeige und Browserunabhängigkeit. **P8** misst private WS→Risk und WS→Wallet-Publish sowie Reconnect-/REST-Budget-Last. Ein erfolgreicher WS-Subscribe alleine ist **kein** `PASS`.

## 4. Fehlende Vorarbeiten, bevor der Gesamttest echt laufen kann

1. **Aktueller DarkNode-Codezustand:** Git `darknode/` hat bisher Dokumentation/Diagramme, aber keine bestätigte produktive P5-Rust-Implementierung. Deshalb können A13–A28 nicht gegen echte P5-Ausführungszustände bestanden werden.
2. **CCXT/Barter codenah geprüft:** CCXT-Methoden routen teilweise nach Classic V2, Barter enthält im gepinnten Tree keinen Bitget-Adapter. Exakten UTA-V3-Contract pro benötigter Methode und Testfall festschreiben; keine pauschale Connector-Übernahme.
3. **Nautilus-/Freqtrade-/Hummingbot-Cases:** oben identifizierte reale Upstream-Tests in unsere Rust-P5-Zustandsmaschine als Assertions übertragen, Race-/Bypass-/Recovery-Matrix vervollständigen; Upstream-Code und Tests wurden noch nicht in DarkNode ausgeführt.
4. **Bitget-Demo-Capability-Matrix:** privates UTA Read-only, `preset stoploss` für Partial-fill und `countdownCancel`/Strategy-Order-Abdeckung; ohne Demo-Zugang nicht als PASS abnehmen.
5. **All-in-One Runner implementieren, erst nach echten Interfaces:** Testfallkatalog, Zustandssnapshots, Fixtures, Guards/Nonces, nachvollziehbarer Overall-Status. Eigenständige Rust-P5-Ausführung separat phasentreu implementieren; keine Schein-Mocks im Produktcode und keine echten Order-Requests ohne Freigabe.

## 5. Entscheidung

**Wiederverwenden:** Risk-Gate-/Rate-/Notional-Pattern aus Nautilus, Stop-/Emergency-/Lock-Pattern aus Freqtrade, InFlight/Cached/Lost/late-Fill-Pattern aus Hummingbot, Instrument/Precision-/Exchange-Adapter-Vertrag aus CCXT, typed Rust-Patterns aus Barter. **Nicht ersetzen:** vorhandenen DarkBot-Gateway-/Graphite-/Protobuf-Stack, nicht neue Exchange-/Risk-Architektur beginnen. **Nicht übernehmen:** Lizenzpflichtigen Fremdcode ohne Compliance; unbestätigte Classic-V2-private Bitget-Methoden; „gesicherte“ Verlustgarantien.

**Gesamtbewertung 27.10.2026:** Source-Referenzen teilweise geprüft, P5-Testplan spezifiziert. **`DN-P5-ALL-001=NOT_RUN`, `P5-A01..A28=NOT_RUN`, `P5=NOT_RUN`, `GATE 1=OPEN`.**
