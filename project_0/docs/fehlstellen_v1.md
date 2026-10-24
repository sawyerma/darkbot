# Project_0 — fehlstellen_v1.md: Gegenprüfung der Fehlstellen-Inventur (Audit 24.10.) und eigene Funde

**Erstellt:** 2026-10-25 · **Geprüfter Code:** `origin/main` `15c34471` (identisch mit dem Audit-Pin; sauberer Export, nicht der Arbeitsbaum) · **Scope:** nur `project_0/` (Rust-Crates, `operations/`, `scripts/`, `tests/regression`), kein `bsc/` · **Art:** statische Code-Lesung mit Aufrufpfad-Verfolgung; **keine Codeänderung, kein Test, kein Benchmark, kein Netz** · **Ablage:** `project_0/docs/fehlstellen_v1.md` neben `BUGS.md` und `bug_v2.md`.

**Eingaben:** (1) `Project_0_Fehlstellen_Audit_2026-10-24.md` (A01–A15, B01–B12, C01–C08, D01–D11, E01–E10); (2) zwei Begleittexte („Mock“-Liste §1–§6, „Vollständige Inventur“). Auftrag: prüfen, ob alles stimmt, weitere Fehlstellen finden, Plan ableiten. Fixes erst nach Freigabe.

**Hinweis Worktree:** Der Project_0-Worktree enthielt während der Prüfung 22 uncommittete fremde Dateien einer parallelen Session (JIT-Paket). Alle Zeilenangaben beziehen sich auf den sauberen Export von `15c34471`.

---

## 0. Gesamturteil

1. **Codefakten des Audits stimmen fast durchgehend** (41 von 46 Positionen bestätigt oder teilweise bestätigt); drei Behauptungen sind widerlegt (B04-Aufruferwirkung, D10, A15 „automatisch“) und eine ist veraltet (JIT-String aus dem Begleittext, vom Audit selbst unter E05 korrigiert).
2. **Die JFI-Schwere ist überzeichnet.** Die produktive Kette `p0ctl run start → econ4h-launcher → p0-longtail → p0ctl post-capture` enthält **keine** interne JSON-Kante (keine Python-Beteiligung, native `strict_driver`/`report`/`qualify`, Ausgabe `.p0j`). A02–A09 sind explizite Report-/Verify-/Export-Seiteneingänge oder tote Legacy-Werkzeuge (JFI-Klasse C/H), keine Klasse I. Der Inventartest ist trotzdem strukturell schwach (A10).
3. **Drei echte Fehlstellen fehlen im Audit und wiegen schwerer als die meisten gelisteten:** der direkte Shadow-Start über `capture.sh shadow` läuft **ohne Provider-Budget** (kein Pacing, keine 429-Behandlung, F-01); `p0ctl verify` ohne `--mode` landet still im Python-Legacy-Verifier (F-02); der RAM-Reconcile-Worker feuert je Signatur bis zu 90 RPC-Calls im Sekundentakt, obwohl sein Ergebnis nur Telemetrie ist (F-04).
4. **D07 ist ein echter Fail-open**, wenn auch praktisch unerreichbar: `wall_unix_s().unwrap_or(0)` führt beim FE-04-Replay über `roll_day(0)` dazu, dass realisierter Tages-PnL und Fehlversuche nicht gebucht werden.
5. **Nicht als Fehler zählen:** gepinnte Schema-Leser, `txv1`-Feature-Gate (Launcher baut immer mit `txv1`), `live`-Block, Generationen 0, Program-Data-Pin-Mismatch (beabsichtigter P1A-Block), Test-Mocks (alle `#[cfg(test)]`).

---

## 1. Maßstab: die produktive Kette am Pin

| Pfad | Aufruf | JSON? | Budget aktiv? |
|---|---|---|---|
| Normaler Lauf | `scripts/capture.sh econ4h` → `p0ctl run start` (`p0ctl.rs:171-345`, `boot_audit::start_config` native_only) → `capture.sh econ4h-launcher` (`:153-302`): `p0-measurement-plan --out measurement_plan.p0j` (`:260`), `p0-capacity`, `p0-longtail` (`startup.rs:69` native_only), dann `p0ctl post-capture` (`post_capture.rs:32-90`: `strict_driver::cli` in-process, `report::report`, `qualification::qualify` → `qualification-<id>.p0j`) | **nein** | ja (`startup.rs:79` wegen Args; `run_econ4h` `:195` unbedingt) |
| Direkter Shadow | `capture.sh shadow` (`:385-416`): `P0_MODE=shadow`, `cargo run --bin p0-longtail` **ohne Argumente**, kein `P0_PROFILE`/`P0_OPERATIONAL_CONFIG_FILE` in `scripts/` oder `.env.example` | nein | **nein** (`startup.rs:50-53` übersprungen; `run_shadow` aktiviert nicht) → F-01 |
| Seiteneingänge | `p0ctl verify` (Default `--mode index` → `verify.sh` → Python), `p0ctl report` (`--format json`/`--report-provenance index`/Runs ohne native Records → `report.sh`), `p0ctl export`, `p0-journal renders`, `preflight.sh`, `report.sh`, `verify.sh` | ja (Export/Legacy-Leser) | — |

`capture.sh:206`/`p0ctl.rs:214` blockieren `--chain-verify index|crosscheck` im produktiven Lauf. `JsonlSink` ist `#[cfg(test)]` (`sink.rs:45-47`). Produktive Frames sind bincode (`payload.rs:13,43`); die `legacy_json`-Zweige in `summary.rs:280-286`/`econ_docs.rs:383-392` laufen nur bei `is_human_readable()`, also nur im expliziten Export.

---

## 2. Audit-Block A — JSON/JFI (Urteile)

Klassen: **W** externer Wire · **H** historischer Read-only-Leser · **C** expliziter CLI-/Export-Befehl außerhalb des produktiven Laufs · **I** interne produktive JSON-Kante (Verstoß).

| ID | Urteil | Klasse | Im produktiven Lauf erreichbar? | Befund / Maßnahme |
|---|---|---|---|---|
| A01 | **bestätigt** | — | ja (Feld ungenutzt) | `ws_rpc.rs:765` `reconnects: 0`, `attempt` in Scope (`:580`). Konsument `market/core.rs:834-850` liest das Feld nicht; `health_reconnects` in `state_owner/sources.rs:126` ist ein anderer Zähler. Fix: `attempt.saturating_sub(1)` oder Feld streichen; Test: Reconnect ⇒ `reconnects == 1`. Schwere: Telemetrie, nicht HIGH. |
| A02 | teilweise | C | nein (nur `report.sh` → `market_report.py:428-433`) | `econ_canonical.rs:2213` liest JSONL, `:2245` schreibt `canonical_market_shadow.jsonl`, `:2279-2283` JSON-Summaries. Einziger Aufrufer: Bin `p0-econ-replay`. `capture.sh:248` **baut** das Bin, ruft es nie. Fix: Bin aus dem produktiven Build nehmen, Replay an die `.p0j`-Frames binden oder als Legacy markieren. |
| A03 | bestätigt (Legacy) | C | nein | `p0-econ-replay.rs:19-21` `from_reader(plan.json)`; Aufrufer nur `market_report.py:432`. |
| A04 | teilweise | H/C | nein, **aber** `p0ctl verify --run X` ohne `--mode` landet hier (F-02) | `measurement_verify.py:110-120` schreibt `job.json`, ruft `p0-strict-verify JOB --compat`, liest JSON-stdout. `post-capture` verweigert index/crosscheck (`post_capture.rs:33-37`). |
| A05 | bestätigt | nativ (`--run`) / H (`JOB`) | nativ ja (in-process `strict_driver::cli`, `p0ctl.rs:631`, `post_capture.rs:56`); `JOB` nein | `p0-strict-verify.rs:30-32` nativ, `:38` `from_reader`, `:55` `render_compat` exportiert JSONL-Views. Fix optional: `JOB/--compat` hinter Feature `legacy-python-driver`. |
| A06 | bestätigt (Legacy-Brücke) | C | nein (`capture.sh:165` „no JSON configuration handoff“; `runtime_config.rs:777-781` verweigert JSON-Config) | Nutzer: `measurement_verify.py:152`, `fe_readonly_preflight.py:72`, `measurement_cli.py:36` (tot). Fix: `--export-environment` abschalten, sobald `verify.sh`/`preflight.sh` nativ sind. |
| A07 | bestätigt, **tot** | C | nein | `measurement_cli.py:48-50` ruft `capture.sh econ4h-launcher --measurement-config`, das `capture.sh:188-199` ablehnt und `:155` `P0_CTL_LAUNCH_RUN_ID` verlangt ⇒ kann nie laufen. Fix: löschen (plus `delete/scripts`-Shim). |
| A08 | bestätigt | C/H | nur explizit | Delegation exakt bei: `report`: `--report-provenance index` oder `--format json` oder Run ohne native Records (`p0ctl.rs:595-599`); `verify`: `mode ∉ {strict, off}` oder kein `run_manifest.p0j` (`:645-650`). Fix: Default `strict|off`, `--format json` nur mit `--legacy`. |
| A09 | bestätigt | H | nur explizit | `market_report.py:117` SQLite, `:320/351` `all_records.jsonl`, `:409-421`; `audit_market_quick.py:761/1074-1102`. Isoliert halten, keine Default-Delegation. |
| A10 | **bestätigt** | — | strukturell | `test_jfi04_json_inventory.py:123-143` prüft nur Zählstand je Datei, Kategorien ⊆ `WRHCI`, `internal == []`, Summe 354; keine Aufruferprüfung. Heute kein False-PASS (alle C/R/H-Dateien ohne produktive Kante), aber nichts verhindert einen. Fix: Aufrufgraph-Assertion (`post_capture.rs`, `startup.rs`, `state_owner/*` referenzieren weder `renders::`, `journal::export*` noch `serde_json`) plus Run-Dir-Test: natives Post-Capture erzeugt kein `*.json`/`*.jsonl`. |
| A11 | teilweise | — | — | Evidence `a23_09_jfi04_inventory.md:61` („no I“) ist für die produktive Kette konsistent; `:77` („JsonlSink built only in tests“) veraltet, es ist jetzt `#[cfg(test)]`. Ausführungspfad-Spalte ist Prosa, nicht maschinell geprüft. |
| A12 | **bestätigt, verschärft** | — | ja (immer falsch) | `p0ctl.rs:484-489`, `:570` prüfen `reports/post_capture_result.json`; einziger Schreiber war je `measurement_cli.py:27` (tot). Natives Post-Capture schreibt `qualification/qualification-<id>.p0j`. `run status` meldet für jeden nativen Lauf „not yet written“, `runs list` `post_capture=no`. Fix: typisierten Qualification-Record prüfen. |
| A13 | bestätigt | H (JSON-Zweig) | `.p0j` ja; JSON nein (`capture.sh:255-261` setzt/erzwingt `.p0j`) | `measurement.rs:133-138`. Fix: Nicht-`.p0j` nur mit explizitem Legacy-Flag. |
| A14 | bestätigt | H | nein (`p0-fe-qualify --replay` via `preflight.sh`) | `fe_replay.rs:252/274/297-303`, Quelle `LEGACY_READ_FILES`. |
| A15 | „automatisch“ **widerlegt**, explizit bestätigt | R/C | nur `p0ctl export`, `p0-journal renders`, `--compat` | `renders.rs:255-258` → `to_writer`; `strict_driver.rs:265-268` nutzt `detect/views` nur zum Ausschluss abgeleiteter Pfade. Kein Aufruf aus `startup.rs`, `state_owner/*`, `post_capture.rs`. |

**Begleittext „fünf JSON-Kanten in der Produktion“:** (1) Schema-1-Decoder: nur Read beim Öffnen alter Journale (H). (2) `JsonlSink`: `#[cfg(test)]`, unerreichbar. (3) `legacy_view`-Renders: nur explizit (C). (4) `runtime_config::json()/export_private`: nur `--export-environment` und `p0-fe-qualify --print-effective-config` (C). (5) `read_legacy` Wallet-Import: **die eine JSON-Lesung, die im laufenden FE-04-Betrieb auftreten kann** — `one_shot.rs:718-728` parst `<wallet>.json` beim `Journal::open`, einmalige hash-gebundene Migration, danach Archivierung (`archive.rs:91-93`). Klasse H; Fix: hinter explizites Migrationsflag (F-13).

---

## 3. Audit-Block B — Pflichtfunktionen und Gates

| ID | Urteil | Befund |
|---|---|---|
| B01 | bestätigt | `inventory.rs:104` `rebalance()` ohne produktiven Aufrufer; `setup.rs:885-890` parst nur Targets/Funding-Bank, `detect.rs:380-385` nutzt `reserve` als Band. Kein Rebalancing existiert. |
| B02 | bestätigt | `admission.rs:74-75` verweigert jeden Plan mit `prerequisite_txs` (JIT-Buy, `builder.rs:1641`). Fehlend: Bundle-Fee-Quote (`jito.rs:216-221` → Unavailable), Multi-TX-SOL-Debit inkl. Tip, Multi-TX-Hold, Bundle-Simulation. Zusätzlich: **`JitoAdapter::new` hat keinen produktiven Aufrufer** (nur Tests `jito.rs:390/412`); Produktion = `ObserveOnly(RpcSubmitter)` (`setup.rs:510`). Der Jito-Pfad ist komplett unverdrahtet (F-20). |
| B03 | bestätigt | `p0ctl.rs:1013-1030`: doctor, preflight, run start/status/stop/preflight, runs list, verify, report, export, qualify, post-capture, source, approval, fe04. `backtest`/`optimize` fehlen (Frontend-Vertrag §4 B/C/E). |
| B04/E06 | Stub bestätigt, **Aufruferwirkung widerlegt** | `route.rs:515-523` `quote_exact_out → Ok(None)`; die Trait-Methode (`builder.rs:568`) hat keinen produktiven Aufrufer, JIT nutzt den ExactIn-Shortfall-Solver (`builder.rs:620/1578`). Toter Code, kein stiller NoRoute. |
| B05 | bestätigt | `jito.rs:241-244` ohne `bundle_sim_rpc` Unavailable; `P0_BUNDLE_SIM_RPC_URL` kommt nur im Fehlertext vor, nichts liest die Env. Mit B02/F-20 ohne Wirkung. |
| B06/E07 | bestätigt, **ohne Wirkung** | `tx/mod.rs:420-427` `V1Unavailable` nur ohne Feature; `capture.sh:248/315/371/415` baut immer `--features "crate_health txv1"`. |
| B07 | bestätigt | `ws_rpc.rs:356`, `submit/rpc.rs:500/527/644` `tx_index_in_slot: None`. Wirkung: `econ_canonical.rs:432` sortiert `(slot, transaction_index, event_index)`, `None` sortiert vor `Some` ⇒ Slot-interne Reihenfolge gemischter Quellen willkürlich. Winner: `winner.rs:484` entscheidet nur auf Slot-Gleichheit (`SameSlotUnknown`), der Index wird nie konsultiert. Fix: `getBlock`-Anreicherung im Post-Capture oder expliziter Mixed-None-Marker. |
| B08/E04 | bestätigt, **Zusatzbefund** | `state/mod.rs:392-402`: gleicher Slot, `(Some n, Some e)` mit `n ≤ e` ⇒ Stale; **jede andere Kombination** ⇒ „später gewinnt“ (`same_slot_no_wv`). Ein späteres WSS-Update ohne `write_version` überschreibt ein früheres Geyser-Update mit `write_version` im selben Slot (F-05). |
| B09 | bestätigt | VERIFY/NOT_RUN-Marker in Produktionscode (Auszug): `submit/mod.rs:9-10,458`; `jito.rs:1,3,14,151,162,229`; `submit/rpc.rs:5,365,380`; `ws_rpc.rs:15,207`; `geyser.rs:12,971`; `sources.rs:134`; `blockhash.rs:4`; `snapshot.rs:6`; `tx/mod.rs:12,571`; `health.rs:17`; `protocol.rs:13,15,26,120`; `economics.rs:19`; `route.rs:31`; `reporter/lib.rs:973`. Jeder Marker ist ein offener Qualifikationsvertrag, keiner ein Mock. |
| B10 | bestätigt | `setup.rs:106-110` `live` ⇒ Err. Beabsichtigt. |
| B11/E10 | bestätigt, keine Wirkung | `identity.rs:16/18` Generationen 0; Konsument `identity.rs:175` (nur Recovery/Program variieren), `telemetry.rs:166`. Vor Runtime-Toggling echte Zähler. |
| B12 | bestätigt (bekannt) | P6MT-J1/R1, bug_v2 V2-05/V2-07. |

---

## 4. Audit-Block C — Doppelte und unverdrahtete Pfade

| ID | Urteil | Befund |
|---|---|---|
| C01 | bestätigt | `receipts.rs:217` `observe_finalized` nur Testaufrufer (`:472`); produktiv `fetch_settlement_evidence` (bug_v2 V2-01). |
| C02 | **bestätigt, verschärft** | `detect.rs:716` legt nach jeder Admission `ReconcileWork` an; `remote_workers.rs:333-368` Thread `reconcile-rpc` (`Workload::Reconcile`, 30 s); Kadenz `remote_apply.rs:113` `now + 1` s bis `RECONCILE_UNRESOLVED_AFTER_S = 90` (`pending.rs:108`); je Versuch `getSignatureStatuses` + `getTransaction`. Finanzwirkung: keine (`remote_apply.rs:105-110` nur Evidence, `pending.rs:166` `exposure_released: 0`). Parallel FE-04-Worker alle 10 s. ⇒ bis ~90 Calls je Signatur für Telemetrie (F-04). |
| C03 | bestätigt | `pipeline.rs:409` `evaluate()` ohne produktiven Aufrufer. `#[cfg(test)]`. |
| C04/C05 | bestätigt | `index.rs:228` `crossed_candidates`, `:132` `accounts_for_oracle`, zusätzlich `:116` `banks_for_account` (0 Referenzen), `:123` `banks_for_integration` nur Tests. |
| C06 | bestätigt | `transport.rs:179-184` dupliziert `unknown()` (`:203-208`, genutzt bei `:249`). |
| C07 | **bestätigt, Schwere HIGH** | `provider.rs:680-681` `!enabled()` ⇒ nackter `RpcClient` („no permit, no byte accounting, no provider degradation/backoff, no fallback endpoint“, `:703-705`). Erreichbar über `capture.sh shadow` (F-01). Weitere unbudgetierte Bins: `p0-prewarm-atas.rs:58`, `solana/bin/p0-probe-v1.rs:11`. |
| C08 | bestätigt | bug_v2 V2-13. |

---

## 5. Audit-Block D — Konstanten und Null-Fallbacks

| ID | Urteil | Befund |
|---|---|---|
| D01–D06 | bestätigt | Alle sechs sind `const`-Literale, keines aus `Limits`/Env: `BLOCKHASH_MAX_AGE_NS` 30 s (`state_owner.rs:269`, C9, „a change needs evidence“), `FE04_SETTLE_RETRY` 10 s (`fe04.rs:54`), `HEALTHY_RESET` 10 s (`ws_rpc.rs:36`), `REQUEST_TIMEOUT` 1,5 s (`route.rs:22`), `PROJECTION_REQUEUE_MAX` 2 (`candidates.rs:212`), `LANE_WDRR_START_WEIGHTS` 8:3:1 (`:554`, Kommentar `:552` „Provisional until MT0“), `PREPARED_CANDIDATES` 1024 (`setup.rs:1090`). Verstoß gegen die verbindliche No-Hardcoding-Regel; Fix: `Limits`-Felder mit abgeleiteten Defaults (zusammen mit bug_v2 V2-08). Weitere provisorische Literale: `setup.rs:607` Winner-Watch-TTLs, `market/configuration.rs:43/94`. |
| D07 | **bestätigt, Fail-open** | `setup.rs:884` `wall_unix_s().unwrap_or(0)` → `Fe04Start::start` → `safety.restore_attempt(now=0, ..)` → `safety.rs:193` `roll_day(0)` setzt `day_index = 0`; `:194` `s.unix_s / 86_400 != 0` ⇒ Rückkehr nach Inventar ⇒ **realisierter Tages-PnL und Fehlversuche werden nicht gebucht**. `wall_unix_s` (`accounts.rs:408-413`) liefert `Err` nur bei Systemzeit vor 1970; `remote_workers.rs:343-347` behandelt denselben Fehler korrekt als `RECONCILE_CLOCK_UNAVAILABLE`. Fix: `wall_unix_s()?` ⇒ Startfehler; Test: `Fe04Start::start(.., now=0)` mit heutigem settled Attempt ⇒ Err (heute Ok, Tages-PnL 0). |
| D08 | bestätigt | `market/time.rs:40-45` → 0; Aufrufer `capture.rs:209…749`, `market_gap.rs:245`, `MarketConfig::from_lookup(.., unix_now_s())` (`capture.rs:320`, Fenster ab Epoche 0); gleiche Faltung `status_tap.rs:16`, `boot_network.rs:28`, `control.rs:131` (Lock `started_unix_s=0`), `boot_audit.rs:346` (nicht-eindeutiger Dateiname), `strict_driver.rs:254`; `boot_audit.rs:705` ist geschützt. Fix: `Result`-Clock, Abbruch beim Capture-Start. |
| D09 | teilweise | `accounting.rs:293-299`: Deltas aus `realized_deltas` (Unknown bei unsettled); fehlender Asset ⇒ Gewinn 0 (konservativ); fehlender `held`/`reserve` ⇒ 0 ⇒ Spendable **nicht** reduziert, keine Vollständigkeits-Invariante. Entschärfung: `withdrawable` hat keinen produktiven Aufrufer. Fix: `held.get(asset).ok_or(Unknown)`. |
| D10 | **widerlegt** | `exit.rs:354-361`: fehlender Mint ⇒ `sold = 0` ⇒ `sold != -amount_in` ⇒ `EXIT_PARTIAL_OR_MISMATCHED`; fehlender SOL ⇒ Floor-Check scheitert. Fail-closed. |
| D11 | — | nicht geprüft (Defaults zentral, zulässig). |

---

## 6. Audit-Block E — Gegenbelege

E01 (`reporter/Cargo.toml:21-27` `required-features = ["test-fixture"]`), E02 (`new_mock` nur in `cfg(test)`-Regionen: `pending.rs:255`, `receipts.rs:464`, `rpc.rs:666/697`, `snapshot.rs:184/221`), E03 (Schema-1-Encoder `journal.rs:351-352` `#[cfg(test)]`; Decoder nur Read), E04, E05 (`funding.rs:41` = `JIT_SHORTFALL_EXACT_IN` = `one_shot.rs:932`; der Begleittext §4 ist **veraltet**), E06, E07, E08 (`protocol.rs:11-27` gepinnte Konstanten), E09 (`provider_evidence.rs:39-58` nur `p0-journal import`/`p0-fe-qualify --execute`; Strict-Gate liest typisiert `strict_driver.rs:212-213`), E10: **alle bestätigt**.

**Program-Data-Pin (Begleittext §3):** bestätigt und beabsichtigt. `protocol.rs:316` `SOURCE_TAG = mrgn-0.1.12-rc2`, `:326` `PROGRAM_DATA_REFERENCE_SOURCE_TAG = mrgn-0.1.11-rc1`, `pin_against` (`:420-434`) liefert `Reference` nur bei gleichem Tag ⇒ für Mainnet derzeit immer `NotReference` ⇒ `setup.rs:680-681` `program_upgrade_latched = true` (Detection aus) und `boot_network.rs:306` `PROGRAM_IDENTITY_MISMATCH`. Das ist der P1A-Block (`UPCOMING_0_1_12.md` Schritte 5–7 offen), kein Mock.

---

## 7. Neue Fehlstellen (nicht im Audit)

| ID | Schwere | Pfad | Befund | Fix / Test |
|---|---|---|---|---|
| **F-01** | **HIGH Safety/Liveness** | `scripts/capture.sh:385-416`, `startup.rs:50-53,79`, `provider.rs:680-705` | `capture.sh shadow` startet `p0-longtail` ohne Args und ohne Profil-Env ⇒ `enable_measurement_budget()` wird nie aufgerufen ⇒ alle Shadow-Worker (`fe04-settlement` `remote_workers.rs:263`, `winner` `:297`, `reconcile` `:333`, Projection/Oracle-`RpcSubmitter` `:384/448`) nutzen den nackten `RpcClient` ohne Pacing, 429-Backoff, Byte-Accounting, Fallback. Die P6-MT-Pacing-Analyse (bug_v2 V2-05) gilt für diesen Pfad nicht. Der `p0ctl`-Pfad ist budgetiert. | Budget am Anfang von `startup::run()` unbedingt aktivieren; `rpc_client()` bei `!enabled()` für Reconcile/Winner/Simulation **Err** statt nackter Client. Fail-before: Start ohne Args ⇒ `provider::enabled()` true. |
| **F-02** | MEDIUM Prozess | `p0ctl.rs:645-650` | `p0ctl verify` ohne `--mode` ⇒ `index` ⇒ Delegation an `verify.sh` ⇒ `measurement_verify.py` ⇒ `job.json` → `p0-strict-verify JOB --compat`. Der native Pfad läuft nur bei explizitem `strict|off`. | Default `strict`; `index|crosscheck` nur mit `--legacy`. Test: `verify --run X` ohne Mode delegiert nicht. |
| **F-03** | MEDIUM Status | `p0ctl.rs:484-489,570` | = A12: Datei wird von keinem nativen Pfad je geschrieben. | typisierter Qualification-Record. |
| **F-04** | MEDIUM Budget | `detect.rs:716`, `remote_apply.rs:113`, `pending.rs:108`, `remote_workers.rs:333-368` | RAM-Reconcile: 1-s-Kadenz × bis 90 s je Signatur, 2 RPC je Versuch, Ergebnis nur Telemetrie; parallel FE-04-Worker (10 s). Zusammen mit F-01 im Shadow unbudgetiert. | Reconcile-Record aus der FE-04-Settlement-Evidence speisen (ein Fetch) oder Kadenz = `FE04_SETTLE_RETRY`. Test: Owner-E2E mit einer Admission ⇒ Reconcile-Calls ≤ Settlement-Calls. |
| **F-05** | LOW Safety | `state/mod.rs:392-402` | Same-Slot: `(None, Some(_))` ⇒ „später gewinnt“: WSS ohne `write_version` überschreibt Geyser mit `write_version`. Nur bei gemischten Quellen. | `(None, Some(_))` im gleichen Slot ⇒ `StaleWriteVersion`/Ambiguous. Test: apply `Some(5)` dann `None` gleicher Slot ⇒ kein Overwrite. |
| **F-06** | LOW Safety | `setup.rs:884`, `safety.rs:193-196,256-263` | = D07, Fail-open der Tagesbuchung bei Clock-Fehler. | `wall_unix_s()?`. |
| **F-07** | LOW latent finanziell | `exit.rs:148` | `valuation.get(mint).unwrap_or(0)`: unbewertetes offenes Inventar wird mit 0 angesetzt (Exit-Sicht unterbewertet). | Unknown statt 0. |
| **F-08** | LOW | `candidates.rs:157-165` | `loss.expect("follow-on cause")` hängt an impliziter Invariante (`follow_on ⇒ loss.is_some()`), unkommentiert. | Invariante als Typ oder `?`. |
| **F-09** | FE-06-Lücke | `exit_admission.rs:40` `admit_exit`, `admission.rs` `Fe04Admission::admit` | Beide FE-04-Admission-Einstiege haben **keinen produktiven Aufrufer** (`detect.rs:541` `fe04: None`). Konsistent mit „FE-06 nicht verdrahtet“, aber die Exit-Kette ist damit ebenfalls nur offline qualifiziert. | FE-06-Integration (bug_v2 V2-15). |
| **F-10** | LOW Dev-Script | `scripts/smoke.sh:72-90` | Nach dem nativen Launcher werden `economics_final`, Market-Summary und `coverage.jsonl` per `json.loads` erwartet, die der native Lauf nie schreibt (kein `p0ctl export` davor). Smoke-Assertion ist defekt. | `.p0j` lesen oder `p0ctl export` voranstellen. |
| **F-11** | LOW Hygiene | `scripts/capture.sh:248` | Produktiver Build enthält `--bin p0-econ-replay` (Legacy, nur `report.sh`). | aus dem Launcher-Build entfernen. |
| **F-12** | LOW tot | `operations/cli/measurement_cli.py` | = A07, nie lauffähig. | löschen. |
| **F-13** | LOW/H | `one_shot.rs:718-728` | `read_legacy` JSON-Wallet-Import beim produktiven `Journal::open`, einmalig, hash-gebunden. | explizites Migrationsflag, sonst Refusal. |
| **F-14** | Hygiene | diverse | Öffentliche Funktionen ohne produktiven Aufrufer (nur Tests oder 0 Referenzen): `economics.rs:31/197/373`, `execution.rs:42-157` (select_route, priority_fee_percentile, quarantine, simulate_gate …), `index.rs:60/116/123/132/228`, `integration.rs:710`, `protocol.rs:121` `pyth_push_price_pda`, `safety.rs:517/522`, `schedule.rs:102`, `sizing.rs:604`, `accounting.rs:276` `withdrawable`, `builder.rs:515`, `identity.rs:132`, `inventory.rs:95/104`, `one_shot.rs:687/700`, `archive.rs:110`, `exit.rs:380`, `exit_admission.rs:40`, `receipts.rs:217`, `settlement.rs:347`, `pipeline.rs:359/409`. Kein `todo!`/`unimplemented!`; einziges `allow(dead_code)` `census.rs:12` (feature-bedingt). | je Funktion: verdrahten, `#[cfg(test)]` oder löschen; keine Sammel-Refaktorierung. |
| **F-15** | LOW | `sizing.rs:918`, `sweep.rs:21/82/158` | `u64::try_from(clock.unix_timestamp).unwrap_or(0)`: negative Chain-Clock ⇒ Zeit 0. Owner-Pfad durch `accounts.rs:386` geschützt, Sizing nutzt die rohe Clock. | gemeinsamer geprüfter Clock-Accessor. |
| **F-16** | Telemetrie | `setup.rs` 17×, `startup.rs` 9×, `ws_rpc.rs` 5×, `provider.rs` 4× | `eprintln!` in Owner/Ingress/Provider (bug_v2 V2-14 P3-03). `execution/`, `liquidation/`, `state/`, `submit/`: 0. | Console-Sink. |
| **F-20** | Design | `jito.rs`, `setup.rs:510` | `JitoAdapter` wird produktiv nie konstruiert; B02/B05/B09-Jito-Verträge betreffen einen unverdrahteten Pfad. | vor Jito-Nutzung: Adapter-Verdrahtung + Provider-Qualifikation als eigenes Ticket. |

---

## 8. Korrekturen an den Begleittexten

- **§4 „`JIT_SHORTFALL` ≠ `JIT_SHORTFALL_EXACT_IN`“:** veraltet, am Pin gleich (E05 des Audits ist richtig).
- **§1 `tx_index_in_slot`:** Winner-Vergleich läuft **nicht** über den Index „auf `SameSlotUnknown`“, sondern entscheidet ohnehin nur auf Slot-Gleichheit; die reale Wirkung liegt in der kanonischen Sortierung (B07).
- **§2 „P8-RAM-Reconcile kostet Budget für nichts Finanzielles“:** bestätigt und quantifiziert (F-04).
- **§3 „fünf JSON-Kanten in der Produktion“:** nur eine kann im laufenden Betrieb auftreten (`read_legacy`, F-13); `JsonlSink` ist unerreichbar; die übrigen drei sind explizite Befehle.
- **§4 `provider.rs !enabled()` „in `p0ctl`-Läufen nie aktiv“:** richtig für `p0ctl`, **falsch für `capture.sh shadow`** (F-01).
- **§6 Konstanten gegen SK-03:** SK-03 betrifft RAM-Budgets; die passende Regel ist die verbindliche No-Hardcoding-Vorgabe (Limits/Env). Sachbefund D01–D06 bleibt.
- **Inventur „46 Positionen“:** 41 bestätigt/teilweise, 3 widerlegt (B04-Wirkung, D10, A15-automatisch), 1 veraltet (JIT-String), 1 nicht geprüft (D11); dazu 17 neue Positionen (F-01…F-16, F-20).

---

## 9. Priorisierte Maßnahmen (nach Freigabe; je Paket RED→GREEN, 1–3 Tests, Gate einmal)

| Nr. | Ticket | Art | Gate |
|---|---|---|---|
| 1 | **F-01** Budget unbedingt aktivieren, `!enabled()` fail-closed | Code (2 Stellen) + Test | G-B/P6-MT.7 |
| 2 | **F-02 + F-03/A12** `p0ctl verify` Default `strict`, Status liest Qualification-Record | Code + 2 Tests | C0E1 |
| 3 | **D07/F-06** `wall_unix_s()?` im FE-04-Start | Code (1 Zeile) + RED-Test | FE-04/G-E |
| 4 | **F-04/C02** Reconcile-Kadenz/-Quelle | Code + E2E-Zählung | P6-MT.7 |
| 5 | **A01** `reconnects` real, **B07** None-Sortierung markieren, **F-05** Same-Slot-Regel | Code + 3 Tests | S1/Market |
| 6 | **D01–D06 + V2-08** Literale nach `Limits` | Config + Round-trip-Test | P6-MT.13 |
| 7 | **A10** Aufrufgraph-/Run-Dir-Assertion im JFI-Inventartest; **A11** Evidence aktualisieren | Test | JFI-05 |
| 8 | **A07/F-12, F-10, F-11, A02/A03** Legacy-Werkzeuge entfernen oder an `.p0j` binden; `--format json`/index nur mit `--legacy` | Scripts/Code | JFI-03/04 |
| 9 | **F-14, C01, C03, C04/C05, C06, T4** toter Code je Funktion | Code | Hygiene |
| 10 | **F-13** Migrationsflag; **D08** Result-Clock im Market-Modus; **F-07/D09** Unknown statt 0 | Code + Tests | SK/Market, FE-04 |
| — | **B01, B02/F-20, B03, F-09** fehlende Pflichtfunktionen (Rebalance, Bundle-Fees/Jito, backtest/optimize, FE-06-Admission) | Phasenpläne, keine Bugfixes | P4A/P5/FE-06/Frontend §4 |
| — | **B09** Marker → je ein Qualifikationsticket mit realer Provider-Evidence | Policy | vor FE-06 |

---

## 10. Grenzen

- Statik auf einem Export; keine `cargo`-Läufe, keine Call-Graph-Werkzeuge; Aufrufpfade per Grep über alle Crates inklusive Inline-Testmodule verfolgt.
- Nicht gelesen: `market/*` jenseits der zitierten Stellen, `storage/*`, `reporter/*` jenseits `lib.rs:973`, Python-Pins vollständig, `lookup_tables.rs`.
- Uncommittete parallele Änderungen (22 Dateien) nicht erfasst; vor jedem Fix neu pinnen.
- Keine Änderung an FREE_ONLY-, PERSIST-/JFI-, Provider-, LIVE- oder `ObserveOnly`-Freigaben durch dieses Dokument.
