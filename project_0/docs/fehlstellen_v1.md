# Project_0 — fehlstellen_v1.md: Gegenprüfung der Fehlstellen-Inventur (Audit 24.10.), eigene Funde und verbindliche Korrektur je Fundstelle

**Version 1.1 (2026-10-25):** je Fundstelle die **verbindliche Korrektur nach `PHASENPLAN.md` § P0-PROD-CTRL-01…12** (ab Z. 5389) ergänzt. Version 1.0 = reine Gegenprüfung.

**Geprüfter Code:** `origin/main` `15c34471` (identisch mit dem Audit-Pin; sauberer Export, nicht der Arbeitsbaum; seitdem nur `PHASENPLAN.md` geändert) · **Scope:** nur `project_0/` (Rust-Crates, `operations/`, `scripts/`, `tests/regression`), kein `bsc/` · **Art:** statische Code-Lesung mit Aufrufpfad-Verfolgung; **keine Codeänderung, kein Test, kein Benchmark, kein Netz** · **Ablage:** `project_0/docs/fehlstellen_v1.md` neben `BUGS.md` und `bug_v2.md`.

**Eingaben:** (1) `Project_0_Fehlstellen_Audit_2026-10-24.md` (A01–A15, B01–B12, C01–C08, D01–D11, E01–E10); (2) zwei Begleittexte („Mock“-Liste §1–§6, „Vollständige Inventur“). Auftrag: prüfen, weitere Fehlstellen finden, Plan ableiten. Fixes erst nach Freigabe.

**Hinweis Worktree:** Der Project_0-Worktree enthielt während der Prüfung 22 uncommittete fremde Dateien einer parallelen Session (JIT-Paket). Alle Zeilenangaben beziehen sich auf den sauberen Export von `15c34471`.

---

## 0. Gesamturteil

1. **Codefakten des Audits stimmen fast durchgehend** (41 von 46 Positionen bestätigt oder teilweise bestätigt); drei Behauptungen sind widerlegt (B04-Aufruferwirkung, D10, A15 „automatisch“) und eine ist veraltet (JIT-String aus dem Begleittext, vom Audit selbst unter E05 korrigiert).
2. **Die JFI-Schwere ist überzeichnet.** Die produktive Kette `p0ctl run start → econ4h-launcher → p0-longtail → p0ctl post-capture` enthält **keine** interne JSON-Kante. A02–A09 sind explizite Report-/Verify-/Export-Seiteneingänge oder tote Legacy-Werkzeuge (JFI-Klasse C/H), keine Klasse I. Nach P0-PROD-CTRL-02/09 sind sie **trotzdem zu entfernen oder sauber als historisch abzugrenzen**, nicht als „zulässig“ zu belassen.
3. **Drei echte Fehlstellen fehlen im Audit:** der direkte Shadow-Start über `capture.sh shadow` läuft **ohne Provider-Budget** (F-01); `p0ctl verify` ohne `--mode` landet still im Python-Legacy-Verifier (F-02); der RAM-Reconcile-Worker feuert je Signatur bis zu 90 RPC-Calls im Sekundentakt für reine Telemetrie (F-04).
4. **D07 ist ein echter Fail-open:** `wall_unix_s().unwrap_or(0)` führt beim FE-04-Replay über `roll_day(0)` dazu, dass Tages-PnL und Fehlversuche nicht gebucht werden.
5. **Nicht als Fehler zählen:** gepinnte Schema-Leser, `txv1`-Feature-Gate, `live`-Block, Generationen 0, Program-Data-Pin-Mismatch (beabsichtigter P1A-Block), Test-Mocks (alle `#[cfg(test)]`). Sie unterliegen dennoch CTRL-03/08/11, sobald sie GUI-schaltbar oder betrieblich änderbar werden.

---

## 0a. Verbindlicher Korrekturmaßstab (PHASENPLAN § P0-PROD-CTRL)

Für **jede** Fundstelle unten gilt dieselbe Vorgabe; die Spalte „Verbindliche Korrektur“ nennt nur die anzuwendende Regel und die konkrete Ausprägung:

| Regel | Kurzform | Bedeutung für dieses Register |
|---|---|---|
| **CTRL-01** | kein produktiver Mock/Stub | leere Rückgaben, unverdrahtete Module, hart kodierte Zustände werden durch die **echte Funktion** ersetzt; ein Schalter macht fehlenden Code nicht vollständig |
| **CTRL-02** | kein Legacy/internes JSON | Python→Rust-JSON-Handoffs, JSONL-Replay-Eingaben, JSON-Fallbacks raus; historische Leser nur isoliert, read-only, nie aus `p0ctl`-Default |
| **CTRL-03** | erst volle Implementierung, dann Schalter | ein kanonischer nativer Controller; `p0ctl` und Gateway-Command = identische Native-Action; OFF/OBSERVE/ACTIVE für Quellen/Capabilities, START/STOP/CANCEL/STATUS für Jobs; requested/effective/qualification/block_reason/version getrennt |
| **CTRL-04/05** | Status-Hinweg bis zum Reiter, Command-Rückweg bis zum Owner | Rust → Projektion → `darkbot.project0.v1` Protobuf → Reporter → Gateway → WebSocket → Adapter → Store → Reiter; zurück über `POST /api/v1/command` → Gateway → Engine-IPC → Controller (CAS/Version, Gates) → Intent/Receipt → STATE |
| **CTRL-06/07** | bestehende Verträge, feste Reiterzuordnung | Topic-IDs 10–29, keine neue SPA/Gateway/Store; Ingress/Provider → Capture; JIT/Jito/Funding/Admission → Liquidator; Inventory/Holds → Liquidator/Wallets; Backtest/Optimizer/Replay → Analytics; Run/Verifikation → Capture; Liveness → Overview; F0-VAR-Zeile je Variable/Action |
| **CTRL-08** | kein verstecktes Hardcoding, UNKNOWN ≠ 0 | betriebliche Werte als validierte Config-/Descriptor-Felder (Typ, Einheit, Default/Min/Max, Quelle, Restart-/Hot-Regel, Run-Manifest); nie 0/false als „gemessen“ |
| **CTRL-09** | kein doppelter/toter Produktivpfad | im selben Ticket alten Pfad entfernen oder auf denselben Einstieg routen; tote nur getestete Produktfunktionen konsolidieren |
| **CTRL-10/11/12** | zwei Vetoprüfer, End-to-End-DoD, bei Fund sofort anhalten | je Ticket: echte Funktion, ein Controller, Config versioniert, Status-Hinweg, Command-Rückweg, Positiv-/Negativtests (Moduswechsel, invalid, version conflict, forbidden, Restart, Reconnect), beide Prüfagenten; externe Qualifikation fehlt ⇒ ehrliches BLOCKED |

**Abnahme je Ticket (CTRL-11, Nutzerfassung 1–7):** (1) echte Funktion produktiv erreichbar, (2) `p0ctl` = Gateway-Command = derselbe Controller, (3) Protobuf-Command erreicht den Rust-Owner, (4) Änderung validiert/versioniert/durabel, (5) STATE-Projektion erreicht den bestehenden React-Reiter, (6) OFF/OBSERVE/ACTIVE, Ablehnung, Neustart, Reconnect nachweislich korrekt, (7) beide Prüfagenten haben Code- und Datenfluss geprüft.

---

## 1. Maßstab: die produktive Kette am Pin

| Pfad | Aufruf | JSON? | Budget aktiv? |
|---|---|---|---|
| Normaler Lauf | `scripts/capture.sh econ4h` → `p0ctl run start` (`p0ctl.rs:171-345`, `boot_audit::start_config` native_only) → `capture.sh econ4h-launcher` (`:153-302`): `p0-measurement-plan --out measurement_plan.p0j` (`:260`), `p0-capacity`, `p0-longtail` (`startup.rs:69` native_only), dann `p0ctl post-capture` (`post_capture.rs:32-90`: `strict_driver::cli` in-process, `report::report`, `qualification::qualify` → `qualification-<id>.p0j`) | **nein** | ja (`startup.rs:79`; `run_econ4h` `:195`) |
| Direkter Shadow | `capture.sh shadow` (`:385-416`): `P0_MODE=shadow`, `cargo run --bin p0-longtail` **ohne Argumente**, kein `P0_PROFILE`/`P0_OPERATIONAL_CONFIG_FILE` in `scripts/` oder `.env.example` | nein | **nein** (`startup.rs:50-53` übersprungen) → F-01 |
| Seiteneingänge | `p0ctl verify` (Default `--mode index` → `verify.sh` → Python), `p0ctl report` (`--format json`/`--report-provenance index`/Runs ohne native Records → `report.sh`), `p0ctl export`, `p0-journal renders`, `preflight.sh`, `report.sh`, `verify.sh` | ja (Export/Legacy-Leser) | — |

`capture.sh:206`/`p0ctl.rs:214` blockieren `--chain-verify index|crosscheck` im produktiven Lauf. `JsonlSink` ist `#[cfg(test)]` (`sink.rs:45-47`). Produktive Frames sind bincode (`payload.rs:13,43`); die `legacy_json`-Zweige (`summary.rs:280-286`, `econ_docs.rs:383-392`) laufen nur bei `is_human_readable()`.

---

## 2. Audit-Block A — JSON/JFI

Klassen: **W** externer Wire · **H** historischer Read-only-Leser · **C** expliziter Befehl außerhalb des produktiven Laufs · **I** interne produktive JSON-Kante.

| ID | Urteil | Klasse | Produktiv erreichbar? | Befund | **Verbindliche Korrektur (P0-PROD-CTRL)** |
|---|---|---|---|---|---|
| A01 | **bestätigt** | — | ja (Feld ungenutzt) | `ws_rpc.rs:765` `reconnects: 0`, `attempt` in Scope (`:580`); Konsument `market/core.rs:834-850` liest das Feld nicht; `health_reconnects` in `state_owner/sources.rs:126` ist ein anderer Zähler. | **CTRL-08 + 04/07:** echter Reconnect-Zähler aus `attempt` (bzw. Session-Generation); nie 0 als gemessen; `SourceHealth` typisiert in die Capture-/Health-Projektion → Protobuf → Capture-Reiter (Source-Liveness). Test: Reconnect ⇒ `reconnects == 1` im STATE. |
| A02 | teilweise | C | nein (nur `report.sh` → `market_report.py:428-433`) | `econ_canonical.rs:2213` liest JSONL, `:2245` schreibt `canonical_market_shadow.jsonl`, `:2279-2283` JSON-Summaries; einziger Aufrufer Bin `p0-econ-replay`; `capture.sh:248` baut es, ruft es nie. | **CTRL-02/03/09:** JSONL-Eingang und JSONL/JSON-Ausgang entfernen; **nativer Replay-Job** liest `.p0j` (Plan + Market-Frames), schreibt `.p0j`; als Analytics-Job (START/STOP/CANCEL/STATUS) über den Controller, `p0ctl` und den Analytics-Reiter; Bin `p0-econ-replay` aus dem Build (F-11); Python-Report nur isolierter historischer Leser, nie aus `p0ctl`. |
| A03 | bestätigt (Legacy) | C | nein | `p0-econ-replay.rs:19-21` `from_reader(plan.json)`; Aufrufer nur `market_report.py:432`. | **CTRL-02/09:** mit A02 ersetzen; `MeasurementPlan` nur aus `measurement_plan.p0j` (`journal::read_typed_value`). |
| A04 | teilweise | H/C | nein, **aber** `p0ctl verify` ohne `--mode` landet hier (F-02) | `measurement_verify.py:110-120` schreibt `job.json`, ruft `p0-strict-verify JOB --compat`, liest JSON-stdout. | **CTRL-02/03/05:** Python→Rust-Handoff entfernen; Verifikationslevel `index`/`crosscheck` **nativ** implementieren (Controller-Action `verify` mit Level-Parameter) oder als historisch isolieren; Ergebnis als typisierter Verification-Record → Capture-Reiter (Verifikation); `p0ctl verify` = dieselbe Native-Action. |
| A05 | bestätigt | nativ (`--run`) / H (`JOB`) | nativ ja; `JOB` nein | `p0-strict-verify.rs:30-32` nativ, `:38` `from_reader`, `:55` `render_compat` exportiert JSONL. | **CTRL-02/09:** `JOB/--compat`-Zweig entfernen (oder bis zur A04-Migration hinter Feature `legacy-python-driver`), `render_compat` nur read-only historisch. |
| A06 | bestätigt (Legacy-Brücke) | C | nein | Nutzer: `measurement_verify.py:152`, `fe_readonly_preflight.py:72`, `measurement_cli.py:36` (tot); `capture.sh:165`, `runtime_config.rs:777-781` verweigern JSON-Config. | **CTRL-02:** `--export-environment`/JSON-stdout entfernen, sobald `preflight.sh`/`verify.sh` nativ sind (A04, A14); Preflight als native Controller-Action mit Projektion → Capture-Reiter. |
| A07 | bestätigt, **tot** | C | nein | `measurement_cli.py:48-50` kann nie laufen (`capture.sh:188-199`, `:155`). | **CTRL-09:** löschen inkl. `delete/scripts`-Shim; `measurement_contract.py` nur als historischer Leser. |
| A08 | bestätigt | C/H | nur explizit | Delegation: `report` bei `--report-provenance index`/`--format json`/Run ohne native Records (`p0ctl.rs:595-599`); `verify` bei `mode ∉ {strict, off}` oder ohne `run_manifest.p0j` (`:645-650`). | **CTRL-02/03/09:** keine implizite Script-Delegation; `report`/`verify`/`export` als native Controller-Actions (Analytics/Capture); `--format json`/`index` nur mit explizitem `--legacy` read-only; Default `strict`. |
| A09 | bestätigt | H | nur explizit | `market_report.py:117` SQLite, `:320/351`, `:409-421`; `audit_market_quick.py:761/1074-1102`. | **CTRL-02:** isoliert historisch; neue Reports nativ aus `.p0j` mit Projektion → Analytics-Reiter; kein Aufruf aus `p0ctl`. |
| A10 | **bestätigt** | — | strukturell | `test_jfi04_json_inventory.py:123-143`: nur Zählstand, Kategorien ⊆ `WRHCI`, `internal == []`, Summe 354; keine Aufruferprüfung. | **CTRL-02/10/11:** Aufrufgraph-Assertion (`post_capture.rs`, `startup.rs`, `state_owner/*`, Controller referenzieren weder `renders::`, `journal::export*` noch `serde_json`) plus Run-Dir-Test (natives Post-Capture erzeugt kein `*.json`/`*.jsonl`); Kategorie-Spalte „Ausführungspfad“ maschinell. |
| A11 | teilweise | — | — | Evidence `a23_09_jfi04_inventory.md:61` konsistent; `:77` veraltet (`JsonlSink` ist `#[cfg(test)]`). | **CTRL-10:** Evidence mit A10-Assertion neu erzeugen, beide Prüfagenten. |
| A12 | **bestätigt, verschärft** | — | ja (immer falsch) | `p0ctl.rs:484-489,570` prüfen `reports/post_capture_result.json`; einziger Schreiber war `measurement_cli.py:27` (tot); natives Post-Capture schreibt `qualification-<id>.p0j`. | **CTRL-01/04:** `run status`/`runs list` lesen den typisierten Qualification-Record; Post-Capture-Status als Projektion → Capture-Reiter; kein Dateiexistenz-Signal. |
| A13 | bestätigt | H (JSON-Zweig) | `.p0j` ja; JSON nein | `measurement.rs:133-138`; `capture.sh:255-261` erzwingt `.p0j`. | **CTRL-02:** JSON-Plan-Zweig entfernen; historische Pläne nur über `p0-journal import`. |
| A14 | bestätigt | H | nein (`p0-fe-qualify --replay`) | `fe_replay.rs:252/274/297-303`, Quelle `LEGACY_READ_FILES`. | **CTRL-02:** nur hinter explizitem historischem `--legacy-reads`; produktive FE-Qualifikation ausschließlich `.p0j`; Preflight nativ (A06). |
| A15 | „automatisch“ widerlegt, explizit bestätigt | R/C | nur `p0ctl export`, `p0-journal renders`, `--compat` | `renders.rs:255-258`; `strict_driver.rs:265-268` nur Ausschlussmenge. | **CTRL-02/09:** Renders nur `p0ctl export --legacy-views` (Feature-Gate), keine produktive Abhängigkeit, kein Default-Render. |

**Begleittext „fünf JSON-Kanten“:** (1) Schema-1-Decoder: Read beim Öffnen alter Journale (H, bleibt nach CTRL-02 zulässig). (2) `JsonlSink`: `#[cfg(test)]`. (3) `legacy_view`-Renders: nur explizit (A15). (4) `runtime_config::json()/export_private`: nur `--export-environment`/`--print-effective-config` (A06). (5) `read_legacy` Wallet-Import: **einzige JSON-Lesung im laufenden FE-04-Betrieb** (`one_shot.rs:718-728`, hash-gebunden, danach Archiv `archive.rs:91-93`) → F-13.

---

## 3. Audit-Block B — Pflichtfunktionen und Gates

| ID | Urteil | Befund | **Verbindliche Korrektur (P0-PROD-CTRL)** |
|---|---|---|---|
| B01 | bestätigt | `inventory.rs:104` `rebalance()` ohne produktiven Aufrufer; `setup.rs:885-890` parst Targets/Funding-Bank; `detect.rs:380-385` nutzt `reserve` nur als Band. | **CTRL-01/03/07/11:** Rebalancing **vollständig** implementieren (echte Bestände, Jupiter-Quotes, Kosten, Holds/Reservations, Receipt) als Capability „Inventory/Rebalancing“ im Controller mit OFF/OBSERVE/ACTIVE und typisierten Parametern (Bänder, Reserven, Max-Kosten); Projektion/Commands → Liquidator-/Wallets-Reiter; DoD 1–7. Kein Dauer-Stub. |
| B02 | bestätigt | `admission.rs:74-75` verweigert jeden Plan mit `prerequisite_txs`; fehlend: Bundle-Fee-Quote (`jito.rs:216-221`), Multi-TX-SOL-Debit inkl. Tip, Multi-TX-Hold, Bundle-Simulation; **`JitoAdapter::new` ohne produktiven Aufrufer** (F-20). | **CTRL-01/03/12:** Bundle-Fee-Quote, Multi-TX-Debit/Hold/Reservation und Bundle-Simulation real implementieren; FE-04-Bundle-Admission anschließen; Capability „JIT/Jito“ im Controller OFF/OBSERVE/ACTIVE mit requested/effective/block_reason → Liquidator-Reiter; ohne genehmigte Provider-Qualifikation effective = BLOCKED sichtbar, kein künstlicher PASS. |
| B03 | bestätigt | `p0ctl.rs:1013-1030` ohne `backtest`/`optimize` (Frontend-Vertrag §4 B/C/E). | **CTRL-01/03/06/07:** native `AnalysisJob`/`BacktestJob`/`OptimizationJob` (START/STOP/CANCEL/STATUS) im Controller, `p0ctl backtest|optimize` und Analytics-Reiter über denselben Pfad; Eingabe ausschließlich versiegelte `.p0j`; bestehende Rust-Economics, kein zweiter Pfad; Ergebnisse typisiert mit Verifikationsstatus. |
| B04/E06 | Stub bestätigt, Aufruferwirkung widerlegt | `route.rs:515-523` `quote_exact_out → Ok(None)`; Trait-Methode (`builder.rs:568`) ohne Aufrufer; JIT nutzt ExactIn-Solver. | **CTRL-01/09:** leere Capability entfernen; falls ExactOut benötigt: echte Implementierung mit typisiertem `Unsupported`, sonst Trait-Methode löschen. |
| B05 | bestätigt | `jito.rs:241-244` Unavailable ohne `bundle_sim_rpc`; `P0_BUNDLE_SIM_RPC_URL` wird nirgends gelesen. | **CTRL-01/08:** `bundle_sim_rpc` als validiertes Config-Feld (Descriptor, Run-Manifest), Fehlertext nur mit real existierendem Feld; mit B02/F-20. |
| B06/E07 | bestätigt, ohne Wirkung | `tx/mod.rs:420-427` nur ohne Feature; Launcher baut immer `txv1`. | **CTRL-11:** Feature-Matrix im Gate; Capability „V1-Messages“ als effective-by-build in der Liquidator-Projektion. |
| B07 | bestätigt | `tx_index_in_slot: None` (`ws_rpc.rs:356`, `rpc.rs:500/527/644`); `econ_canonical.rs:432` sortiert `None` zuerst; Winner (`winner.rs:484`) entscheidet nur Slot-Gleichheit. | **CTRL-08/01:** Index aus `getBlock` im Post-Capture real füllen oder typisiert `Unknown` führen und in Sortierung/Winner-Projektion sichtbar machen; nie `None`-Sortierung als Ordnung. |
| B08/E04 | bestätigt, Zusatz F-05 | `state/mod.rs:392-402`: Same-Slot ohne `write_version` ⇒ „später gewinnt“. | **CTRL-08:** siehe F-05; Zähler `same_slot_no_wv` in die Capture-Health-Projektion. |
| B09 | bestätigt | VERIFY/NOT_RUN-Marker: `submit/mod.rs:9-10,458`; `jito.rs:1,3,14,151,162,229`; `submit/rpc.rs:5,365,380`; `ws_rpc.rs:15,207`; `geyser.rs:12,971`; `sources.rs:134`; `blockhash.rs:4`; `snapshot.rs:6`; `tx/mod.rs:12,571`; `health.rs:17`; `protocol.rs:13,15,26,120`; `economics.rs:19`; `route.rs:31`; `reporter/lib.rs:973`. | **CTRL-12:** je Marker ein Qualifikationsticket mit genehmigter realer Provider-Evidence; bis dahin `qualification = NOT_RUN`/`BLOCKED` in der jeweiligen Projektion; keine Aktivierung per Schalter. |
| B10 | bestätigt | `setup.rs:106-110` `live` ⇒ Err. | **CTRL-03/05:** LIVE bleibt gesperrt; künftige Freigabe nur als Controller-Gate (requested=ACTIVE, effective=BLOCKED, block_reason=P8-LT/FE-06) mit Nutzerfreigabe, nie als Env-Schalter. |
| B11/E10 | bestätigt | `identity.rs:16/18` Generationen 0; Konsument `identity.rs:175`, `telemetry.rs:166`. | **CTRL-03:** vor jedem Hot-Change echte Config-/Policy-Generationen mit CAS/Version/Receipt; Generation in der STATE-Projektion. |
| B12 | bestätigt | P6MT-J1/R1, bug_v2 V2-05/V2-07. | **CTRL-09/03:** MT8/MT10-Entkopplung; Remote-Klassen als typisierte Budget-Parameter (CTRL-08). |

---

## 4. Audit-Block C — Doppelte und unverdrahtete Pfade

| ID | Urteil | Befund | **Verbindliche Korrektur (P0-PROD-CTRL)** |
|---|---|---|---|
| C01 | bestätigt | `receipts.rs:217` `observe_finalized` nur Test; produktiv `fetch_settlement_evidence`. | **CTRL-09:** entfernen oder als einzigen Einstieg verdrahten (bug_v2 V2-01). |
| C02 | **bestätigt, verschärft** | `detect.rs:716` → `ReconcileWork` nach jeder Admission; `remote_workers.rs:333-368` (`Workload::Reconcile`); `remote_apply.rs:113` 1-s-Kadenz bis `RECONCILE_UNRESOLVED_AFTER_S = 90` (`pending.rs:108`); finanziell wirkungslos (`pending.rs:166` `exposure_released: 0`). | **CTRL-09:** ein Reconcile-/Settlement-Pfad: FE-04-Settlement-Evidence speist die Telemetrie; RAM-Worker entfernen; Budgetverbrauch → Capture-/Provider-Projektion (F-04). |
| C03 | bestätigt | `pipeline.rs:409` `evaluate()` ohne produktiven Aufrufer. | **CTRL-09:** `#[cfg(test)]` oder löschen. |
| C04/C05 | bestätigt | `index.rs:228/132/116/123` nur Tests oder 0 Referenzen. | **CTRL-09:** entfernen oder im Threshold-Controller verdrahten (bug_v2 V2-03/V2-13). |
| C06 | bestätigt | `transport.rs:179-184` dupliziert `unknown()`. | **CTRL-09:** ein Helfer. |
| C07 | **bestätigt, HIGH** | `provider.rs:680-681` `!enabled()` ⇒ nackter `RpcClient` (`:703-705`); erreichbar über `capture.sh shadow` (F-01); weitere unbudgetierte Bins `p0-prewarm-atas.rs:58`, `p0-probe-v1.rs:11`. | **CTRL-01/03/08:** Budget **immer** aktiv; `!enabled()` fail-closed; Provider-/Budget-Parameter (rps, burst, Klassen, Fallback) als validierte Config; Budget-Status (enabled, 429, Cooldown) → Capture-Reiter. |
| C08 | bestätigt | bug_v2 V2-13. | **CTRL-09/03:** eine Admission-Policy für Sweep und Threshold; Modus OFF/OBSERVE/ACTIVE im Controller → Liquidator. |

---

## 5. Audit-Block D — Konstanten und Null-Fallbacks

| ID | Urteil | Befund | **Verbindliche Korrektur (P0-PROD-CTRL)** |
|---|---|---|---|
| D01–D06 | bestätigt | `const`-Literale ohne Config: `BLOCKHASH_MAX_AGE_NS` 30 s (`state_owner.rs:269`), `FE04_SETTLE_RETRY` 10 s (`fe04.rs:54`), `HEALTHY_RESET` 10 s (`ws_rpc.rs:36`), `REQUEST_TIMEOUT` 1,5 s (`route.rs:22`), `PROJECTION_REQUEUE_MAX` 2 (`candidates.rs:212`), `LANE_WDRR_START_WEIGHTS` 8:3:1 (`:554`, „Provisional until MT0“), `PREPARED_CANDIDATES` 1024 (`setup.rs:1090`); dazu `setup.rs:607` Winner-Watch-TTLs, `market/configuration.rs:43/94`. | **CTRL-08 (+ bug_v2 V2-08):** jeder Wert als validiertes `Limits`-/Descriptor-Feld (Typ, Einheit, Default/Min/Max, Quelle, Restart-/Hot-Regel, Run-Manifest); Descriptor → Protobuf → Reiter (Blockhash-Alter, Requeue, WDRR, Jupiter-Timeout → Liquidator; Healthy-Reset → Capture; Settle-Retry → Wallets); CLI und Frontend nutzen dieselbe Rust-Autorität. Protokoll-/Deployment-Konstanten (`protocol.rs`) bleiben versioniert fest. |
| D07 | **bestätigt, Fail-open** | `setup.rs:884` `wall_unix_s().unwrap_or(0)` → `safety.rs:193` `roll_day(0)`, `:194` ⇒ Tages-PnL/Fehlversuche **nicht gebucht**; `wall_unix_s` (`accounts.rs:408-413`) Err nur bei Zeit vor 1970; `remote_workers.rs:343-347` behandelt denselben Fehler korrekt. | **CTRL-08:** `wall_unix_s()?` ⇒ Start verweigert mit `CLOCK_UNAVAILABLE`; Clock-Status → Overview. Test: `Fe04Start::start(.., now=0)` mit settled Attempt ⇒ Err. |
| D08 | bestätigt | `market/time.rs:40-45` → 0; Aufrufer `capture.rs:209…749`, `market_gap.rs:245`, `MarketConfig::from_lookup` (`capture.rs:320`); gleiche Faltung `status_tap.rs:16`, `boot_network.rs:28`, `control.rs:131`, `boot_audit.rs:346`, `strict_driver.rs:254`. | **CTRL-08:** `Result`-Clock; Capture-Start verweigert; Zeitstempel nie 0; Clock-Unavailable in der Capture-Projektion. |
| D09 | teilweise | `accounting.rs:293-299`: fehlender `held`/`reserve` ⇒ 0 ⇒ Spendable nicht reduziert; `withdrawable` ohne produktiven Aufrufer. | **CTRL-08/09:** fehlende Map-Einträge ⇒ `Unknown`; `withdrawable` verdrahten (Wallets-Projektion) oder entfernen. |
| D10 | **widerlegt** | `exit.rs:354-361` fail-closed. | keine Korrektur; Test behalten. |
| D11 | nicht geprüft | zentrale Defaults. | **CTRL-08:** Defaults nur über `RuntimeConfig`-Descriptor, keine Doppel-Defaults. |

---

## 6. Audit-Block E — Gegenbelege

E01 (`reporter/Cargo.toml:21-27` `required-features = ["test-fixture"]`), E02 (`new_mock` nur in `cfg(test)`: `pending.rs:255`, `receipts.rs:464`, `rpc.rs:666/697`, `snapshot.rs:184/221`), E03 (Schema-1-Encoder `journal.rs:351-352` `#[cfg(test)]`; Decoder nur Read), E04, E05 (`funding.rs:41` = `JIT_SHORTFALL_EXACT_IN` = `one_shot.rs:932`; Begleittext §4 veraltet), E06, E07, E08 (`protocol.rs:11-27`), E09 (`provider_evidence.rs:39-58` nur `p0-journal import`/`p0-fe-qualify --execute`; Strict-Gate typisiert `strict_driver.rs:212-213`), E10: **alle bestätigt**. Nach CTRL-01 bleiben strikt isolierte Test-Doubles und gepinnte historische Leser zulässig; sie dürfen nie Produktivpfad oder Abnahmenachweis sein.

**Program-Data-Pin (Begleittext §3):** bestätigt und beabsichtigt. `protocol.rs:316` `SOURCE_TAG = mrgn-0.1.12-rc2`, `:326` `PROGRAM_DATA_REFERENCE_SOURCE_TAG = mrgn-0.1.11-rc1`, `pin_against` (`:420-434`) ⇒ für Mainnet immer `NotReference` ⇒ `setup.rs:680-681` `program_upgrade_latched = true` und `boot_network.rs:306` `PROGRAM_IDENTITY_MISMATCH`. P1A-Block (`UPCOMING_0_1_12.md` Schritte 5–7). **CTRL-04/12:** Pin-Status (latched, Referenz-Tag, beobachteter Hash) als ehrliches BLOCKED in der Overview-Projektion.

---

## 7. Neue Fehlstellen (nicht im Audit)

| ID | Schwere | Pfad | Befund | Fix / Test | **Verbindliche Korrektur (P0-PROD-CTRL)** |
|---|---|---|---|---|---|
| **F-01** | **HIGH Safety/Liveness** | `scripts/capture.sh:385-416`, `startup.rs:50-53,79`, `provider.rs:680-705` | `capture.sh shadow` ohne Args/Profil ⇒ `enable_measurement_budget()` nie ⇒ Shadow-Worker (`remote_workers.rs:263/297/333/384/448`) ohne Pacing, 429-Backoff, Byte-Accounting, Fallback. `p0ctl`-Pfad ist budgetiert. | Budget unbedingt in `startup::run()`; `rpc_client()` bei `!enabled()` Err. Fail-before: argloser Start ⇒ `enabled()`. | **CTRL-01/03/08/12:** ein Launcher-Pfad (`p0ctl run start`), `capture.sh shadow` auf denselben Controller routen oder entfernen; Budget-Parameter typisiert; Budget-Status → Capture-Reiter. Sofort anhalten, Ursache dokumentieren, beide Prüfagenten. |
| **F-02** | MEDIUM Prozess | `p0ctl.rs:645-650` | `verify` ohne `--mode` ⇒ `index` ⇒ `verify.sh` ⇒ Python ⇒ `job.json`. | Default `strict`; `index`/`crosscheck` nur `--legacy`. | **CTRL-02/03/05:** `verify` = native Controller-Action (Level-Parameter), identisch für `p0ctl` und Capture-Reiter-Command; Legacy-Level nativ nachbauen oder historisch isolieren. |
| **F-03** | MEDIUM Status | `p0ctl.rs:484-489,570` | = A12. | typisierter Qualification-Record. | **CTRL-01/04:** wie A12. |
| **F-04** | MEDIUM Budget | `detect.rs:716`, `remote_apply.rs:113`, `pending.rs:108`, `remote_workers.rs:333-368` | RAM-Reconcile 1 s × 90 s je Signatur, nur Telemetrie, parallel FE-04 (10 s). | eine Quelle oder Kadenz = `FE04_SETTLE_RETRY`; E2E: Reconcile-Calls ≤ Settlement-Calls. | **CTRL-09/08:** Pfad konsolidieren (C02); Kadenzen als Config-Felder. |
| **F-05** | LOW Safety | `state/mod.rs:392-402` | `(None, Some(_))` gleicher Slot ⇒ „später gewinnt“. | `StaleWriteVersion`/Ambiguous; Test: `Some(5)` dann `None` ⇒ kein Overwrite. | **CTRL-08:** Ordnung nie aus fehlender Information ableiten; Zähler → Capture-Health. |
| **F-06** | LOW Safety | `setup.rs:884`, `safety.rs:193-196,256-263` | = D07. | `wall_unix_s()?`. | **CTRL-08:** wie D07. |
| **F-07** | LOW latent finanziell | `exit.rs:148` | `valuation.get(mint).unwrap_or(0)`: unbewertetes Inventar = 0. | Unknown statt 0. | **CTRL-08:** Wallet-/Inventarwert nie 0 als gemessen; `Unknown` in der Wallets-Projektion. |
| **F-08** | LOW | `candidates.rs:157-165` | `loss.expect(..)` an impliziter Invariante. | Typ/`?`. | **CTRL-01:** Invariante typisieren; kein erreichbarer Panic im Owner. |
| **F-09** | FE-06-Lücke | `exit_admission.rs:40`, `admission.rs` `Fe04Admission::admit` | beide Admission-Einstiege ohne produktiven Aufrufer (`detect.rs:541` `fe04: None`). | FE-06 (bug_v2 V2-15). | **CTRL-03/05/11:** FE-04-Admission (Attempt und Exit) als Controller-Capability mit Approval-Receipt, Single-Use, Durability; Projektion/Commands → Liquidator-/Wallets-Reiter; LIVE-Gate bleibt (B10). |
| **F-10** | LOW Dev-Script | `scripts/smoke.sh:72-90` | erwartet JSON-Dateien, die der native Lauf nie schreibt. | `.p0j` lesen. | **CTRL-02:** Smoke nativ (`.p0j`-Reader), kein `p0ctl export` als Voraussetzung. |
| **F-11** | LOW Hygiene | `scripts/capture.sh:248` | Build enthält `p0-econ-replay`. | entfernen. | **CTRL-09:** mit A02. |
| **F-12** | LOW tot | `operations/cli/measurement_cli.py` | = A07. | löschen. | **CTRL-09.** |
| **F-13** | LOW/H | `one_shot.rs:718-728` | `read_legacy` beim produktiven `Journal::open`. | Migrationsflag. | **CTRL-02:** einmalige Migration als explizite Controller-Action (`fe04 migrate`) mit Receipt; ohne Flag Refusal; danach Archiv. |
| **F-14** | Hygiene | diverse | öffentliche Funktionen ohne produktiven Aufrufer: `economics.rs:31/197/373`, `execution.rs:42-157`, `index.rs:60/116/123/132/228`, `integration.rs:710`, `protocol.rs:121`, `safety.rs:517/522`, `schedule.rs:102`, `sizing.rs:604`, `accounting.rs:276`, `builder.rs:515`, `identity.rs:132`, `inventory.rs:95/104`, `one_shot.rs:687/700`, `archive.rs:110`, `exit.rs:380`, `exit_admission.rs:40`, `receipts.rs:217`, `settlement.rs:347`, `pipeline.rs:359/409`; kein `todo!`/`unimplemented!`; einziges `allow(dead_code)` `census.rs:12`. | je Funktion entscheiden. | **CTRL-09:** je Funktion im zugehörigen Ticket verdrahten, `#[cfg(test)]` oder löschen; keine Sammel-Refaktorierung; Kapital-/UNKNOWN-/Recovery-Semantik erhalten. |
| **F-15** | LOW | `sizing.rs:918`, `sweep.rs:21/82/158` | `u64::try_from(clock.unix_timestamp).unwrap_or(0)`. | gemeinsamer Clock-Accessor. | **CTRL-08:** Chain-Clock-Fehler ⇒ Unknown/Refusal, nie 0. |
| **F-16** | Telemetrie | `setup.rs` 17×, `startup.rs` 9×, `ws_rpc.rs` 5×, `provider.rs` 4× | `eprintln!` in Owner/Ingress/Provider. | Console-Sink. | **CTRL-04:** Diagnose als typisierte Projektion (Capture/Overview) statt stderr; stderr nur Operator-Text. |
| **F-20** | Design | `jito.rs`, `setup.rs:510` | `JitoAdapter` produktiv nie konstruiert. | eigenes Ticket. | **CTRL-01/03/12:** wie B02/B05; Jito-Capability nur mit echter Verdrahtung und genehmigter Qualifikation, sonst BLOCKED sichtbar. |

---

## 8. Korrekturen an den Begleittexten

- **§4 „`JIT_SHORTFALL` ≠ `JIT_SHORTFALL_EXACT_IN`“:** veraltet, am Pin gleich (E05 richtig).
- **§1 `tx_index_in_slot`:** Winner entscheidet ohnehin nur auf Slot-Gleichheit; reale Wirkung liegt in der kanonischen Sortierung (B07).
- **§2 RAM-Reconcile:** bestätigt und quantifiziert (F-04).
- **§3 „fünf JSON-Kanten“:** nur `read_legacy` (F-13) kann im laufenden Betrieb auftreten; `JsonlSink` unerreichbar; drei explizite Befehle.
- **§4 `provider.rs !enabled()` „in `p0ctl`-Läufen nie aktiv“:** richtig für `p0ctl`, **falsch für `capture.sh shadow`** (F-01).
- **§6 Konstanten gegen SK-03:** SK-03 betrifft RAM-Budgets; maßgeblich ist jetzt CTRL-08. Sachbefund D01–D06 bleibt.
- **Inventur „46 Positionen“:** 41 bestätigt/teilweise, 3 widerlegt, 1 veraltet, 1 nicht geprüft; dazu 17 neue Positionen.

---

## 9. Priorisierte Maßnahmen (nach Freigabe; je Ticket CTRL-10/11: RED→GREEN, Status-Hinweg + Command-Rückweg, beide Prüfagenten, Gate einmal)

| Nr. | Ticket | Art | Gate / Reiter |
|---|---|---|---|
| 1 | **F-01/C07** Budget unbedingt, `!enabled()` fail-closed, ein Launcher-Pfad | Code + Config + Test | G-B / P6-MT.7 / Capture |
| 2 | **F-02/A04/A05/A08 + F-03/A12** `verify`/`report`/`status` als native Controller-Actions, keine Script-Delegation, Qualification-Record | Code + Protobuf/Publisher + 2 Tests | C0E1 / Capture |
| 3 | **D07/F-06, D08, F-07, F-15, D09** Clock/Wert-Fallbacks ⇒ Unknown/Refusal | Code + RED-Tests | FE-04 / Overview, Wallets |
| 4 | **F-04/C02** ein Reconcile-/Settlement-Pfad | Code + E2E-Zählung | P6-MT.7 / Capture |
| 5 | **A01, B07, F-05** echte Reconnect-/Index-/Same-Slot-Semantik in der Projektion | Code + 3 Tests | S1 / Capture, Analytics |
| 6 | **D01–D06 + V2-08** Literale → `Limits`/Descriptor → Protobuf → Reiter | Config + Round-trip-Test | P6-MT.13 / Liquidator, Capture, Wallets |
| 7 | **A10/A11** Aufrufgraph-/Run-Dir-Assertion, Evidence neu | Test | JFI-05 |
| 8 | **A02/A03/F-11, A06, A07/F-12, A13, A14, A15, F-10, F-13** Legacy raus oder `.p0j`-nativ; historische Leser isoliert | Scripts/Code | JFI-02/03/04 / Analytics |
| 9 | **F-14, C01, C03, C04/C05, C06, T4** toter Code je Funktion | Code | CTRL-09 |
| 10 | **B01** Rebalancing, **B02/B05/F-20** JIT/Jito, **B03** Backtest/Optimizer, **F-09** FE-06-Admission, **C08/V2-13** Admission-Policy | **vollständige Implementierung + Controller + Rundweg** (CTRL-01/03/04/05/11) | P4A/P5/FE-06/Frontend §4 / Liquidator, Wallets, Analytics |
| — | **B09** Marker → Qualifikationstickets mit genehmigter Provider-Evidence; bis dahin ehrliches BLOCKED (CTRL-12) | Policy | vor FE-06 |

---

## 10. Grenzen

- Statik auf einem Export; keine `cargo`-Läufe, keine Call-Graph-Werkzeuge; Aufrufpfade per Grep über alle Crates inklusive Inline-Testmodule.
- Nicht gelesen: `market/*` jenseits der zitierten Stellen, `storage/*`, `reporter/*` jenseits `lib.rs:973`, Python-Pins vollständig, `lookup_tables.rs`.
- Uncommittete parallele Änderungen (22 Dateien) nicht erfasst; vor jedem Fix neu pinnen.
- Keine Änderung an FREE_ONLY-, PERSIST-/JFI-, Provider-, LIVE- oder `ObserveOnly`-Freigaben durch dieses Dokument; P0-PROD-CTRL verschärft die Abnahme, lockert kein Gate.
