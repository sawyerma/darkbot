# Solana / Project 0 — Measurement & Evidence Ledger

Status: **POST-WIRING BUILD/TEST GATE PASS through `[FIX-70]`; first real Shadow/Simulation qualification next; LIVE remains disabled**  
Updated: 2026-10-02

This is the single human-readable measurement ledger for the new Solana / Project 0 Liquidator.

Primary economic objective:

> **maximize sustainable realized net profit while avoiding unnecessary recurring portal/SaaS costs.**

No Project 0 latency, capture-rate, winner-position, provider-benefit or realized-PnL measurement is currently recorded here. Values remain **N/A** until produced by the actual implementation.

## 0. Rules

- Never write an unmeasured value as 0.
- Separate local processing latency from network/inclusion/landing evidence.
- Submission ACK or Jito bundle ID is not landing evidence.
- Use the same timing vocabulary across replay, shadow and LIVE.
- Every result records source commit, config/source manifest, sample count and raw-evidence path.
- Gate/benchmark code must exercise production components, not duplicate business logic.
- Paid provider cost must be measured alongside its incremental benefit.
- Do not invent p(win), p(land) or competitor-response probabilities.
- Failed-attempt cost is submission-path/outcome dependent.
- Provider convenience is not evidence of economic necessity.

## 1. Timing vocabulary

~~~text
T_rx               update received by Rust ingress
T_detect           candidate classified as liquidatable/actionable
T_ready            complete execution plan/transaction(s) ready
T_sign             signing complete
T_handoff          payload handed to selected submission adapter
T_observed_landed  landing observed/reconciled from chain evidence
~~~

Derived local intervals may include:

~~~text
T_rx → T_detect
T_detect → T_ready
T_ready → T_sign
T_sign → T_handoff
T_rx → T_handoff
~~~

Landing evidence additionally records:

~~~text
signature
landed slot
transaction index where derivable
confirmation status
submission/bundle id
transaction version
serialized bytes
submission path
single tx / bundle
actual fees
actual tips
observed receiver/token deltas
realized PnL
~~~

## 2. Phase evidence table

| Phase | Measurement | Status | Evidence |
|---|---|---|---|
| S0 | timing/telemetry + provider abstraction contract | WORKSPACE TEST PASS on Mac under `crate_health` and `crate_health txv1` — see 2026-10-02 session | project_0/native/crates/darkbot-common |
| S1-LT | WsRpcControlSource, event model, v1 detection | COMPILES + UNIT TESTS PASS in workspace feature runs; live provider behavior not yet qualified | darkbot-solana/src/ingress |
| S2 | state reconciliation/fork/gap recovery | UNIT TESTS PASS in workspace feature runs; Mainnet recovery evidence still N/A | darkbot-solana/src/state |
| S3-CORE | v0 builder, v1 capability measurement, signer, RPC+Jito adapter, reconciliation | BUILD/UNIT TEST baseline PASS; v1 Sign/Wire/Submission remains unavailable | darkbot-solana/src/{tx,submit} |
| S4 | Python↔Rust Pump.fun parity + native baseline | N/A | not started |
| C0 | account census (types/sizes) | TOOLING WRITTEN, selftest PASS offline, mainnet run NOT done (no egress) — see §9 | project_0/scripts/c0_census.py |
| H0 | historical Project 0 liquidation/composition/competition | N/A | not started |
| P2 | reverse/threshold index, premium+tag scheduler | UNIT TEST baseline PASS in workspace feature runs; P3-02 thresholds still open | darkbot-project0/src/{index,schedule}.rs |
| P3 | health via marginfi crate | `crate_health` compiles/tests PASS against pinned marginfi tag; exact thresholds P3-02 still open | darkbot-project0/src/health.rs |
| P4A | deterministic economics/ranking, premium curve | Prior unit-test baseline PASS; concrete ProtocolSizingProvider now wired; failed-cost evidence still open/fail-closed | darkbot-project0/src/{economics,sizing}.rs |
| P5 | receivership sequence/allowlist/bundle rules | Crate-based builders/planner prior tests PASS; runtime ProtocolSizer + Jupiter V2 RouteProvider + P0ReceivershipBuilder are now wired and CURRENT HEAD compiles | darkbot-project0/src/{planner,receivership}.rs + darkbot-runner |
| P6 | full hotpath + shadow race | N/A | not started |
| P7 | capture/economics/competition/provider-value evidence | N/A | not started |
| P4B | empirical p(win)/tip/stopping ranking | N/A | blocked on P7 |
| P8 | controlled LIVE reconciliation | N/A | not started |
| P9 | production profit/provider economics | N/A | not started |

## 3. Required record shape

Every measurement entry should state:

~~~text
date/time
phase
repo commit SHA
protocol/source manifest
runtime config
provider/region
provider monthly cost
provider usage cost
hardware/runtime environment
sample count
metric definitions
p50
p95
p99
max
raw evidence path
PASS / ADAPT / STOP
known limitations / non-claims
~~~

Where economic evidence applies also record:

~~~text
gross liquidation value
protocol/flat fees
swap/DEX fees
slippage
priority fees
Jito tips
failed-attempt cost
capital used
realized net PnL
provider cost
incremental opportunities attributable to provider/path
incremental realized/expected profit attributable to provider/path
~~~

## 4. Submission-path failure matrix

Every qualified submission path must eventually populate this matrix from real evidence:

| Path / outcome | Base fee | Priority fee | Tip | Other cost | Evidence |
|---|---:|---:|---:|---:|---|
| RPC not landed | N/A | N/A | N/A | N/A | not measured |
| RPC landed success | N/A | N/A | N/A | N/A | not measured |
| RPC landed revert/fail | N/A | N/A | N/A | N/A | not measured |
| Jito single-tx not landed | N/A | N/A | N/A | N/A | not measured |
| Jito single-tx landed success | N/A | N/A | N/A | N/A | not measured |
| Jito single-tx landed revert/fail | N/A | N/A | N/A | N/A | not measured |
| Jito bundle not landed | N/A | N/A | N/A | N/A | not measured |
| Jito bundle landed success | N/A | N/A | N/A | N/A | not measured |
| Jito bundle landed/reverted | N/A | N/A | N/A | N/A | not measured |

No economics code may replace unknown cells with zero.

## 5. Provider economics matrix

Every paid provider considered for permanent production use must eventually have:

| Provider/path | Monthly cost | Usage cost | Coverage benefit | Latency benefit | Incremental captured profit | Keep/remove |
|---|---:|---:|---|---|---:|---|
| N/A | N/A | N/A | N/A | N/A | N/A | not measured |

Qualification rule:

~~~text
keep paid provider only if:
incremental sustainable net profit
>
full recurring/usage/dependency cost
~~~

## 6. Probability-model gate

Before P7:

~~~text
p(win) = UNKNOWN
p(land) = UNKNOWN unless directly measured for the path
optimal_wait = UNKNOWN
~~~

P4A uses deterministic economics only.

P4B may start only when H0/Shadow/P7 provide a versioned empirical dataset sufficient to estimate and validate:

~~~text
p(win | opportunity, premium, tag_age, tip, submission_path)
~~~

## 7. Raw evidence location

A dedicated raw-evidence directory does not yet exist because Project 0 implementation has not started.

When created, use one canonical location and link it here. Do not scatter Project 0 raw measurements across the older Pump.fun P0_* files.

## 8. Current status

~~~text
Project 0 production code:        POST-WIRING build/tests PASS through `[FIX-70]`
Project 0 measurements:           Compiler/test evidence only; real Shadow/Mainnet simulation evidence remains open
Project 0 LIVE:                   NOT ENABLED
permanent paid provider approved: NONE
probabilistic ranking approved:   NO
~~~

## 9. Session log

### 2026-10-01 — Sitzung 1 (Fable, Chat-Container ohne Egress)

**Umgebung:** kein `cargo`/`rustc`/`protoc`; Egress-Proxy blockt github.com und mainnet.helius-rpc.com (HTTP 403). Alle Netzwerk-Tickets sind in dieser Umgebung nicht ausführbar.

**BLOCKER (echt, dokumentiert nach Work-Order-Regel):**
- P0-01, P0-02, P0-04: Source-/Lizenz-Lesen von `0dotxyz/eva01`, `mrgnlabs/mary`, `solana-dex-superagg` nicht möglich (403). Betroffene Quelle: GitHub. Minimale Option: Tickets in Claude Code auf dem Mac ausführen; kein Workaround über Sekundärquellen zulässig.
- P1A-01: on-chain ProgramData nicht abrufbar (403). Program-ID `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` und Group `4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8` sind **Sekundärwissen, PRÜFEN**.
- C0-01 Mainnet-Lauf: nicht möglich (403). Tooling geschrieben und offline getestet.

**S0 (S0-01..05) — CODE WRITTEN:**
- `native/Cargo.toml` (workspace, resolver 2), `project_0/native/crates/darkbot-common` (edition 2021, **null externe Dependencies**).
- Module: `timing` (Clock monoton, Stage T_rx..T_observed_landed, Timeline mit Intervallen; fehlender Punkt → None, nie 0; kein Overwrite), `histogram` (log-linear 8 Sub-Buckets, p50/p95/p99 konservativ als obere Bucket-Grenze, max/min/count exakt, leer → N/A), `channel` (bounded über sync_channel, Overflow explizit gezählt, Element zurückgegeben), `lifecycle` (Shutdown, RunState fail-closed: Halted behält ersten Grund, Ready nur über Reconciling), `journal` (Record mit seq/t_rx/source/kind/payload; SeqGap → Fehler; MemJournal + Replay), `config` (Pflichtwerte → Fehler, explizite Defaults), `error` (HaltReason-Taxonomie).
- Unit-Tests in jedem Modul (14 Tests). **Nicht ausgeführt** — Gate S0 bleibt offen bis `cd project_0/native && cargo test -p darkbot-common` auf dem Mac PASS meldet. Ergebnis hier eintragen (Toolchain-Version, Test-Count, PASS/FAIL).
- Bewusst nicht gebaut: lock-freier SPSC-Ring (erst nach Messung), Datei-Journal (S1).

**C0 — TOOLING WRITTEN:**
- `project_0/scripts/c0_census.py`: ein `getProgramAccounts` (confirmed, withContext), JSONL-Dump mit SHA-256, Typ-Census über Anchor-Discriminators, Größenverteilung pro Typ, unbekannte Discriminators gezählt. Health-Klassifikation steht explizit als N/A bis P1A-Layout.
- `--selftest` offline: PASS.
- Discriminators (sha256("account:<Name>")[:8]): MarginfiAccount 43b2826d7e721c2a · Bank 8e31a6f2324261bc · MarginfiGroup b617adf097ceb643 · FeeState 3fe01055c124ebdc · LiquidationRecord 5f74178459d2f5a2. PRÜFEN gegen IDL des deployed Tags (P1A-01). Ein unbekannter Discriminator mit hoher Anzahl im Mainnet-Lauf bedeutet: Typname im Script falsch, nicht Chain-Anomalie.
- Mainnet-Lauf: `HELIUS_API_KEY=... python3 project_0/scripts/c0_census.py --out project_0/data/c0/` → Report + Rohdaten-Pfad + SHA hier eintragen. Credits: ein gPA-Call; Rohdatenmenge (bytes_total) hier dokumentieren.

**Gate-Entscheidungen:** keine. S0 PASS offen (cargo test). C0-GO/STOP offen (Mainnet-Lauf + Health-Klassifikation).

**Nächstes Ticket in einer Umgebung mit Egress:** P0-01, dann P1A-01 (Program-ID verifizieren), dann C0-01 Mainnet-Lauf, dann S0 `cargo test`.

### 2026-10-01 — Sitzung 2 (Fable, Chat-Container ohne Egress) — CHECKPOINT

Auftrag: FIRST EURO PATH ohne Stopp bei fehlenden Testmöglichkeiten; alles **UNTESTED / VERIFY**.
Kein `cargo`, kein Netzwerk. Nichts kompiliert. Offline ausgeführt: nur `c0_census.py --selftest` (Sitzung 1).

**Geschrieben (Rust, ~3.760 LOC inkl. Tests):**
- `darkbot-common` (S0, Sitzung 1) — unverändert.
- `darkbot-solana` (S1-LT, S2, S3-CORE): `ingress/{mod,ws_rpc}.rs` (IngressSource-Contract, Event-Modell, programSubscribe/accountSubscribe/slot/root, T_rx vor Parse, Reconnect → Gap+Reconnected, Sink-Overflow → Gap, v1-Erkennung config→versioned→legacy für JSON und Bytes), `state/{mod,snapshot,blockhash}.rs` (Bootstrap-Puffer, Snapshot-Install mit "Updates > S", Ordnungsregeln slot/write_version mit Zählern, Root-Policy, Reconcile-Report → HALT bei Mismatch, explizite Freigabe; RPC-Snapshot; Blockhash-Cache ohne RPC im Hotpath), `tx/mod.rs` (v0-Builder mit ComputeBudget, 64-Lock-Limit vor Compile, Größenlimit, v1-Stub hinter Feature `txv1` mit dokumentierter API, Signer, Keypair-Loader), `submit/{mod,rpc,jito}.rs` (SubmissionAdapter, Ledger-§4-Outcome-Taxonomie, RPC-Submit + Reconciliation mit maxSupportedTransactionVersion=1, Jito sendTransaction/sendBundle(≤5)/getTipAccounts/getBundleStatuses).
- `darkbot-project0` (P1A-Konstanten, P2, P3-Stub, P4A, P5-Regeln): `protocol.rs` (Discriminators, Dual-Size-Bank 1856/3904 → sonst UNKNOWN_LAYOUT), `index.rs` (ReverseIndex 4 Richtungen; ThresholdIndex pro Bank/Seite mit BTreeMap-Range-Lookup + margin), `schedule.rs` (TimeScheduler Min-Heap mit Invalidation; premium_boundary), `economics.rs` (Premium-Kurve 0.1.12: Basis max(FeeState,5 %), 1 h Grace, linear bis 100 % nach 7 d, blockiert bei Bad Debt/EMODE; Umkehrfunktion time_for_premium; FailedCostMatrix mit Option → Unknown nie 0; evaluate/actionable/rank), `health.rs` (HealthEngine-Contract, NoHealth fail-closed, Crate-Anbindung als Feature `crate_health` offen), `planner.rs` (validate_receivership: End letzte Ix ohne Ausnahme, Allowlist vor/zwischen Start–End, NO_SWAP verbietet Swap, Paid nur nach P7; validate_bundle: kein Start/End-Split, ≤5).

**Bewusst NICHT gebaut:** eigene Health-Formel (Regel 3), lock-freier SPSC-Ring (erst nach Messung), GeyserSource (S1-RACE), Race-S3-Tickets, Direct TPU, Deshred, Instruction-Bau für Receivership/Tagger (braucht Crate/IDLs aus P0/P1A), TX-Template-Cache (P2-06; Invalidation-Liste in schedule.rs dokumentiert).

**VERIFY-Schwerpunkte für den Mac (in dieser Reihenfolge):**
1. `cd project_0/native && cargo build` — Dependency-Versionen in beiden Cargo.toml sind geraten (solana-* 3.x, tokio-tungstenite 0.24, reqwest 0.12, bincode 1). Erwartete Reibung: `Keypair::from_bytes` vs `try_from`, `RpcProgramAccountsConfig.sort_results`, `get_program_accounts_with_config` ohne Context-Slot (siehe Kommentar in snapshot.rs), `compute_units_consumed` OptionSerializer, `Message::Text(.into())` bei tungstenite.
2. `cargo test --workspace` — 40+ Unit-Tests, alle offline.
3. `c0_census.py` Mainnet-Lauf → Report ins Ledger.
4. P0-01/P0-02 (GitHub) → Crate-Tag bestätigen → Feature `crate_health` anbinden (P3-01).

**Gate-Entscheidungen:** keine. Alle Gates offen bis Build/Test/Mainnet-Lauf.

### 2026-10-01 — Sitzung 3 (Fable, Chat-Container ohne Egress) — CHECKPOINT nach Astra-Review

**Korrigiert (materiell, aus Astras Review):**
1. `schedule.rs`: `Trigger` jetzt `Hash` (Compile-Fehler).
2. `economics.rs`: Rampe = 7 d − 1 h → 100 % genau 7 d nach Tagging; VERIFY Bezugspunkt in `liquidate_start.rs`.
3. `state/mod.rs`: `confirm_ready()` nur nach sauberem `reconcile()` nach dem letzten `install_snapshot()` (Flag `reconciled_clean`); Tests angepasst.
4. `state/snapshot.rs`: Context-Slot aus **derselben** gPA-Antwort (`send(GetProgramAccounts)` + `OptionalContext`), kein separates `getSlot`.
5. `install_snapshot`: per-Account-Slots bleiben erhalten; `snapshot_slot` = untere Schranke; gepufferte Deltas per Account-Vergleich. `multiple_accounts` liefert min-Slot statt max.
7. `ws_rpc.rs`: fehlender `context.slot` → Protocol-Fehler; unlesbarer Frame → Session-Ende (Gap/Reconnect), kein `continue`.
12. `jito.rs`: `getTipAccounts` auf `/api/v1/getTipAccounts`; `bundle_status` normalisiert zu `BundleStatus {NotFound|Pending|Landed|Failed}`.
15. `index.rs`: `set_bank_oracles`/`set_bank_integration` entfernen alte Zuordnungen (Invalidation), Test ergänzt.
24. `c0_census.py` Selftest: Bank-Rohlänge 1864 (8 + 1856), konsistent mit `protocol.rs`.

**Neu gebaut (quellenunabhängig):**
- S1-01: `IngressEvent::Transaction` (+ `TransactionObserved`, Version in Pflichtreihenfolge) und `IngressEvent::Health` (`SourceHealth`, alle 5 s). `transactionSubscribe` als Provider-Erweiterung markiert (VERIFY).
- S1-07/S2-04: `state/recovery.rs` — `RecoveryDriver`: Gap → HALT → Snapshot A → install → Snapshot B → reconcile → confirm_ready; begrenzte Versuche, bleibt HALTED bei Mismatch; injizierter Snapshot-Fetch, offline getestet (2 Tests).
- S3-01: `BuildVersion::Legacy` + `build_legacy`; Resource-Ixs gemeinsam.
- P2-06/07: `prepared.rs` — `PreparedCache` mit `InvalidationKey` (account_slot, oracle_slots, tagged_at, premium_flags, lut_version, fee_state_version); Hit/Miss/Invalidation-Zähler.
- P4A: `t_profitable()`-Solver: net(p) > min ⇔ p > Bedarf, gedeckelt durch `premium_cap_bps` (Receivership-Regel), Umkehr über `time_for_premium`; Tests.
- P8-LT: `safety.rs` — Limits (exposure, daily loss, failed attempts, tip, slippage, outstanding), Tagesrollover, HALT ohne Auto-Reset, `record_outcome` mit realisiertem PnL; Tests.
- Runner: `darkbot-runner` — `pipeline.rs` (Health → Plan → Economics → Planner-Validierung → Build → Sign → Send-Boundary [Shadow] | Safety → Submit [Live]; jede Stufe setzt Timing-Punkt; Fehler = Record mit LossReason), `record.rs` (OpportunityRecord, Decision, LossReason inkl. THRESHOLD_TOO_HIGH), `main.rs` (`p0-longtail`: Env-only-Config, Ingress-Thread → bounded channel → State + RecoveryDriver; Live-Modus verweigert Start, bis P8-LT im Ledger PASS ist).

**Weiterhin offen — nur mit Quellen/Compiler lösbar (Begründung, keine Ausrede):**
- S2-02 Fork-Graph: `SlotUpdate.parent` wird gehalten, aber kein Branch-State/Rollback. Stufe 1 läuft auf `confirmed`; Processed-Verarbeitung ist Race (S1-RACE). Als PARTIAL geführt.
- S3-02 v1-Builder: API `v1::Message::try_compile_with_config` nur mit solana-sdk ≥ 4.2 verifizierbar.
- P2-01 RAM-Modell der P0-Typen, P3-01..05 Health, P5-02/05/06/07 Instruction-Bau, T (Tag-Ix): brauchen `marginfi`-Crate/IDL (GitHub gesperrt).
- C0-02..04, S4, P6-LT-Lauf: brauchen RPC bzw. Build.

**Zählung:** ~4.790 Zeilen Rust, 60+ Unit-Tests im Source, **nichts kompiliert**.

### 2026-10-01 — Sitzung 4 (Quellen marginfi-v2-main + eva01-main als Zip gelesen) — CHECKPOINT

**Jetzt BELEGT aus Source (vorher PRÜFEN):** Program-ID Mainnet `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA` (id-crate), Receivership-Allowlist (8 Programme), Refresh-Allowlist, Exclusive-Ix-Liste, Ix-Discriminators, Seeds (`liq_record`, `liquidity_vault_auth`, `feestate`), Seize-Grenze `seized ≤ repaid×(1+max(fee,5 %))` in Equity-Werten, Post-Health-Regel, Closeout < 5 USD, Flat-SOL-Fee in `end`, Health-API `HealthPriceMode::Client(Clock)`, Remaining-Accounts-Layout, Oracle-Modell (Pyth Push PDAs on-chain). **Kein System-Programm in der Receivership-TX → Jito-Tip in separater Bundle-TX** (P5-Konsequenz, neu).

**Neu gebaut:** `protocol.rs` (komplett aus Source), `receivership.rs` (P5-06: init_liquidation_record, start, withdraw, repay, end mit exakter Account-Reihenfolge; `build_receivership`), `health.rs` `crate_impl` (P3-01: Observation-Accounts in Programm-Reihenfolge, `get_health_components` Maintenance+Equity über Client-Clock, HealthView mit Closeout/BadDebt), `economics::max_seizable_for_repay`, `state/fork.rs` (S2-02 Fork-Tracker, Orphan-Erkennung, `StateCore::invalidate_slots` → HALT), `IngressHalt` (Out-of-Band), Recovery-Barrier (`drain`) + Dedupe (Gap hält, Reconnected recovert), `begin_reconcile` aus Ready, Lock-Zählung auf finaler Ix-Liste, Safety mit Gesamtkapital/Inventar/Concurrency/Asset-/Klassen-Allowlist + `Pipeline::reconcile_outcome`, Runner: `AccountSource` über StateCore, Trigger (a) auf MarginfiAccount-Updates → Health-Record (JSONL) im Shadow, Feature `crate_health` durchgereicht.

**Astras 10 Punkte:** 1–9 umgesetzt; 10 teilweise (Trigger a → Health-Record verdrahtet; Threshold-Index/Scheduler/Pipeline-Evaluate-Vollverdrahtung folgt mit P3-02 Schwellen und P4A-03 Sizing).

**Weiterhin offen (jetzt mit Quelle, aber ohne Compiler):** P3-02 Preisschwellen (`thresholds()` fail-closed), P4A-03 max liquidatable amount, P5-02 Refresh-Planner (Kamino/Drift/JupLend-Ixs aus deren IDLs in `marginfi-v2/idls`), P5-05 allowlisted Swap-Ix, T (Tagging erst 0.1.12), v1, S4, P6-LT-Lauf, C0-02 (Health-Klassifikation kann jetzt über Crate laufen — braucht Build + RPC).

**Zählung:** ~5.550 Zeilen Rust, 63 Unit-Tests im Source, nichts kompiliert. Ticketnummern angeglichen: NO_SWAP = P5-03, Receivership-Builder = P5-06.

### 2026-10-01 — Sitzung 5 (Merge eva01 / marginfi-v2 / DarkBot) — CHECKPOINT

- `receivership.rs`: manueller Builder **entfernt**; Instructions über `marginfi::accounts::*.to_account_metas(Some(true))` + `marginfi::instruction::*.data()` (BELEGT an `p0-cli/src/processor/account.rs`); Init-Record inline vor Start; EndLiquidation mit `fee_payer: None`, `global_fee_wallet` aus FeeState.
- `execution.rs` neu: No-Route-Quarantäne, exponentielles Target-Backoff mit Drop, Simulate-Gate (nur `Ok` sendet; RPC-Fehler ist kein Freifahrtschein), Bundle-Status → Chain-Confirmation-Regel, sequenzieller Fallback nur bei einer TX. Verhalten aus eva01 adaptiert, kein Code kopiert. 3 Tests.
- Workspace-Pins aus marginfi-v2 übernommen: `solana-sdk =3.0.0`, `solana-client =3.1.12`, `anchor-lang 1.0.2` (Feature `crate_health`).
- `P0_REUSE_MATRIX.md`: Merge-Tabelle mit DIRECT_DEPENDENCY / REUSE_ADAPT / KEEP_DARKBOT / DROP für alle Komponenten.
- Neuer Edge-Kandidat dokumentiert: In-Receivership-Swap (kapitalfrei, vom Protokoll erlaubt) + Tx v1 bei >1232 Bytes — eva01 fährt stattdessen JIT-Buy aus USDC-Reserve. VERIFY: Bytes per Staging-Simulation.
- Offen (Quellen vorhanden, Compiler fehlt): P5-02 Refresh-Ixs (Kamino/Drift/JupLend aus Mocks-IDLs), P5-04/05 JIT-Buy-Fallback + superagg-Economics-Gate, Bundle-Bau mit Tip-TX, P4A-03 Sizing, P3-02 Schwellen, Runner-Vollverdrahtung.
- Zählung: ~5.640 Zeilen Rust, 66 Tests im Source, nichts kompiliert.

### 2026-10-01 — Sitzung 5b (Astra-Zusatz: Funding Ladder, Drift-Muster, ZERO_PREFUND-Fixture)

- `funding.rs`: `FundingClass` {ZERO_PREFUND_SAME_TX, NO_SWAP_INVENTORY, FLASH_FUNDED, JIT_SHORTFALL, PREHELD_INVENTORY}. **Keine feste Ausführungspräferenz:** `feasible_classes()` emittiert alle technisch/protokollarisch machbaren Klassen; P4A + TX-Feasibility + Safety wählen die beste ausführbare Variante. FLASH_FUNDED bleibt für P0 ungültig (Allowlist). `capital_required()`/`tx_count()` bleiben Teil von Economics/Telemetry.
- `execution.rs` erweitert (keep-rs-Muster, adaptiert): `Freshness` (Opportunity-Age, Execution-Deadline), `select_route()` über parallele Quotes nach **realisierbarem** Netto, `priority_fee_percentile()`. Safety: `release_without_outcome()` für deterministische Kapitalfreigabe vor Submission.
- Pipeline: Deadline-Check vor Send-Boundary; Funding-Klasse im Record.
- `fixture.rs` (Feature `crate_health`): ZERO_PREFUND-Harness — vollständige Jupiter-Route (`tokenLedger`, setup, other, swap, cleanup) plus `addressLookupTableAddresses`; v0 lädt **echte LUT-Accounts**, v1 nutzt keine LUTs. Allowlist-Check und vollständige Receivership-Ix-Liste aus Crate-Buildern; Messung bytes/locked/ix-count. Simulation + Staging-Lauf bleibt Mac-Aufgabe (`simulateTransaction`, sigVerify=false).
- **v1 korrigiert:** Solana-3.x (marginfi) und modulare Solana-4.x-v1-Crates können im selben Cargo-Workspace koexistieren; kein zweiter Business-/Execution-Core und kein marginfi-Upgrade ist architektonisch nötig. `darkbot-solana` hat einen isolierten v1-Capability-Adapter nach `solana-foundation/transaction-v1-examples` (`solana-message 4.4.1`, `solana-transaction 4.1.6` Referenz). `measure_v1()` kompiliert die v1-Message/TransactionConfig und misst 4096/64/64-Gates; der finale signierte v1-Wire-/Submission-Adapter bleibt **UNTESTED/VERIFY** bis zur Compiler-Runde.
- Candidate-Auswahl in der Pipeline: alle vom Planner gelieferten Funding-/Route-Klassen werden einzeln auf Protocol-Validität, P4A-Economics, LIVE-Safety und TX-Baubarkeit geprüft; höchster deterministischer Netto-Profit gewinnt, bei Gleichstand höhere Kapital-Effizienz. Shadow wird nicht von `live_enabled=false` blockiert.
- Offline-Evidence dieses Imports: `python3 -m py_compile project_0/scripts/c0_census.py` PASS; `python3 project_0/scripts/c0_census.py --selftest` PASS. Rust weiterhin nicht kompiliert (kein `cargo` in dieser Umgebung).
- Zählung: ~6.400 Zeilen Rust im importierten Stand; Rust-Tests im Source, **nichts als Rust PASS deklariert**, bis Mac-Compilerlauf grün ist.

### 2026-10-01 — Sitzung 6 (Fable-Audit von 87a9dd6 + Korrekturen, Zip) — CHECKPOINT

Audit-Ergebnis: 5 Blocker, 4 High, 3 Medium (Details im Chat-Protokoll / Commit-Message). Korrigiert, markiert als `[FIX-n]` im Code (`grep -rn "\[FIX-" native`):
- FIX-1 `runner/main.rs`: Pipeline in `p0-longtail` eingehängt (Shadow): Keypair, Blockhash-Thread, Safety aus Env, Allowlist aus Source, RpcSubmitter (nur Simulation), Records als JSON. Builder = `NoPlans` (fail closed), bis P4A-03-Sizing existiert.
- FIX-2 `runner/builder.rs` neu: `P0ReceivershipBuilder` mit `SizingProvider`/`RouteProvider`-Traits; ein Plan je machbarer FundingClass; JIT als Prerequisite-TX; Init-Record inline nur wenn PDA fehlt.
- FIX-3 `pipeline.rs`: `ReceivershipPlan.prerequisite_txs`; Kandidaten mit Prerequisites nur über Bundle-Pfad.
- FIX-4 `submit/mod.rs`: `build_tip_tx` (legacy, System-Transfer) + `SubmissionAdapter::submit_bundle`; `jito.rs` implementiert es; `pipeline.rs` baut `[prereqs…, receivership, tip]`.
- FIX-5 `runner/pending.rs` neu + `main.rs`: offene Submissions → `reconcile` → `Safety::record_outcome` (Landed/Failed/Timeout 90 s); `pnl_complete=false`, bis Token-Deltas (P8-05) gebucht werden.
- FIX-6 `submit/mod.rs` `SimResult` + `SubmissionAdapter::simulate`; `rpc.rs` `simulate_transaction_with_config` auf derselben signierten TX; `pipeline.rs` Gate vor Submit, `simulated_cu` im Record.
- FIX-7 `pipeline.rs`: Submit-Ablehnung → `release_without_outcome` statt Fehlversuch.
- FIX-11 `fixture.rs`: Allowlist-Check auf finaler Ix-Liste. FIX-12 `HaltReason::LimitBreached`.
Offen (unverändert): P4A-03 Sizing (numerisch), P3-02 Schwellen, Trigger (b)/(c) im Runner, v1 Sign/Wire, Token-Delta-PnL. Nichts kompiliert.

### 2026-10-01 — Sitzung 7 (Astra-Review des FIX-Zips: 7 Blocker + High) — CHECKPOINT
Korrigiert, markiert `[FIX-13]`…`[FIX-26]`:
13 SDK-3.x: `solana_message::AddressLookupTableAccount`, `solana_commitment_config`, `solana_compute_budget_interface`, `solana_system_interface::instruction::transfer`, `Keypair::try_from`; Component-Crates in Cargo (Versionen VERIFY am marginfi-Graph).
14 `SubmissionAdapter::supports_bundle()` (Jito true); Pipeline nutzt Capability, nicht `path()`.
15 `simulate_bundle` (Jito `simulateBundle`, VERIFY Format); Bundle-Kandidaten werden als Bundle simuliert, Einzel-TX einzeln; Unavailable → fail closed.
16 Kapital/Exposure in USD-Micro (`liability_needed_value`, `liability_inventory_value`); Token-Mengen nur fürs Routing; `profit_per_capital_bps` jetzt einheitenkonsistent.
17 JIT: `funding_bank`/`funding_available_value`; Buy-Route = Funding-Bank → Liability über `shortfall_token_amount`.
18 `SubmitAck.signatures` + Bundle-ID; `PendingSubmission` reconciled alle Signaturen, Gebühren summiert; Sentinel = Receivership-TX.
19 `builder.bank()` prüft Owner/Discriminator/Länge (kein Panic).
20 ZERO_PREFUND als v0- und v1-Kandidat (v1 bleibt bis Sign/Wire `V1Unavailable`, geht aber nicht verloren).
21 `Freshness::is_stale` vor Planung.
22 Globale Top-Level-Allowlist in der Pipeline, unabhängig von den Kategorien.
23 `SubmitError::Transport` nach Handoff → `AmbiguousSubmission`, kein Release, Reconciliation über lokale Signaturen.
24 `record_outcome(realized_pnl: Option<i128>)`: None = UNKNOWN → HALT(UnknownEconomics); NOT_LANDED = Some(0) gemessen; gelandet ohne Token-Deltas = None (P8-05 offen).
25 Allowlist semantisch getrennt (swap: JUP/Titan; integration/refresh: Kamino/Drift/JupLend; refresh + Programm für init_liquidation_record).
26 `health::dependency_keys` + main: Vor-Snapshot → Oracle-/Integrations-/Mint-Keys → accountSubscribe + gemeinsamer Snapshot/Reconcile.
Offen: P4A-03, P3-02, v1 Sign/Wire, ZERO_PREFUND Staging, P8-05 Token-Delta-PnL. Nichts kompiliert.

### 2026-10-01 — Sitzung 8 (Astra-Review v2: 8 Punkte) — CHECKPOINT
Korrigiert `[FIX-27]`…`[FIX-34]`:
27 Observation-/Dependency-Keys **exakt wie `p0-cli::bank_observation_keys`** (am Source im Zip verifiziert): `OracleSetup`-Enum, `keys[0..]` direkt, keine Pyth-PDA-Ableitung, keine u8-Magie; `protocol::remaining_layout` entfernt. Astra hatte recht, meine Ableitung aus eva01 war falsch.
28 `reconcile()`: Vergleich gegen `r.slot` des jeweiligen Remote-Accounts statt globalem Lower-Bound; Regressionstest mit gemischten Batch-Slots.
29 `RouteProvider::{quote_exact_in, quote_exact_out}`; JIT nutzt ExactOut mit Liability-Fehlbetrag als Output; `Route.amount_in/out` vom Router.
30 P0-Allowlist nur auf Receivership-TX; Prerequisite-TXs eigene Allowlist (Swap, System, Token, Token-2022, ATA, ComputeBudget).
31 RPC-Sendefehler klassifiziert: RpcError/Serde/Signing/TransactionError → Rejected; Rest → Transport (Ambiguous).
32 `JitoAdapter::new(block_engine, bundle_sim_rpc: Option, timeout)`; `simulateBundle` nur gegen `P0_BUNDLE_SIM_RPC_URL`, sonst Bundle fail closed.
33 Pending: Pfad aus Record (JITO_BUNDLE/JITO_SINGLE/RPC); `getInflightBundleStatuses` beendet `Failed` sofort; on-chain bleibt authoritative.
34 Dynamische Dependency-Subscription: Bank-Update mit neuen externen Keys → HALT(StateGap) → Ingress-Session neu mit erweiterter Subscription → Recovery → READY. `Producer` Clone.
Offen (unverändert): P3-02, P4A-03, v1 Sign/Wire, Staging, P8-05. Nichts kompiliert.

### 2026-10-01 — Sitzung 9 (letzter Pre-Build-Patch) — CHECKPOINT
`[FIX-35]`…`[FIX-39]`:
35 Dependency-Prüfung **nach** `drv.on_event` (neue Bank-Konfiguration); kein 500-ms-Sleep mehr — Bootstrap und Resubscribe laufen über die StreamReady-Barriere.
36 `ws_rpc`: `Reconnected` = StreamReady, nur nach allen Subscription-ACKs (`pending.is_empty()`), genau einmal pro Session; im Fehlerpfad nur OOB-Halt + Gap.
37 `Snapshot.missing` (Key + Batch-Slot) aus `getMultipleAccounts`; `SnapshotData.missing_required` → Recovery nicht sauber, kein READY; Test.
38 `SubmissionAdapter::supports_tx_version`: Default v1=false (Jito-Bundle/BAM unbelegt), RPC true; Pipeline verwirft v1-Kandidaten auf nicht-fähigen Pfaden.
39 Component-Crates auf `=3.0.0` / `solana-system-interface =2.0.0` mit Feature `bincode` gepinnt.
Vorgemerkt (P4A-03): Candidate = FundingClass × TxVersion × SubmissionPath (Tips/Failed-Cost/Latenz pfadabhängig).
Offen: P3-02, P4A-03, v1 Sign/Wire, Staging, P8-05, v1-Bundle-Capability. Nächster Schritt: die vier Logs.

### 2026-10-01 — Sitzung 10 (P6-LT Messstrukturen M1–M3) — CHECKPOINT
- `project_0/docs/PHASE_P6LT_SHADOW.md`: Phase definiert (Gates G1–G5, Messfelder, PASS/ADAPT/STOP).
- [M1] `pipeline.rs`: Shadow baut, signiert und **simuliert** identisch zu LIVE; Reservierung nur in LIVE (`reserved`-Flag, `release_if`); neue Decision `ShadowSimFailed`; `sign_slot` im Record.
- [M2] `runner/winner.rs`: `find_winner` (getSignaturesForAddress → getTransaction json v1 → END_LIQUIDATION-Discriminator, receiver = 4. Account), `classify` (INGRESS/STATE_RACE/BUILD/THRESHOLD_TOO_HIGH/ECONOMIC_SKIP), `is_shadow_win`; Test. `main.rs`: Watchlist, Prüfung nach ≥ 60 s, Aufgabe nach 15 min, Race-Zeile im Record-Log.
- [M3] `runner/shadow_ledger.rs`: Tagesaggregat (Updates, Health, liquidierbar, fail-closed, Pläne, Sim PASS/FAIL, Shadow-Wins, Losses je Grund, Summe erwarteter Netto der Wins, Latenz-Histogramme p50/p95/p99), `pnl_complete=false`; Tageswechsel schreibt `daily_summary.jsonl`; Test.
- Dependencies runner: solana-commitment-config, solana-transaction-status, bs58.
Offen unverändert: Build/Tests, C0, P4A-03 Sizing, Route, v1 Sign/Wire, Staging, P8-05.

### 2026-10-01 — Sitzung 11 (Astra-Review v5: 10 Punkte) + Rebase auf `project_0/`-Struktur — CHECKPOINT
`[FIX-40]`…`[FIX-47]`:
40 `SlotWatch` (AtomicU64, lockfrei) im Ingress-Contract; ws_rpc aktualisiert bei Slot-/Account-Updates; Pipeline liest `sign_slot` exakt nach dem Signieren. Trigger-Slot ist nur Update-Provenienz.
41 Recovery: `b.missing_required` → HALT(ReconcileFailure), kein READY; Regressionstest `snapshot_b_missing_required_blocks_ready`.
42 Timing: `T_detect` beim ersten actionablen Kandidaten **vor** dem TX-Build; `T_ready` nach Build; `T_rx→T_detect` enthält keinen Build mehr.
43 Verlustgrund strukturell: `ThresholdTooHigh` nur, wenn ein Kandidat positiv aber ≤ MIN_PROFIT war; sonst `EconomicSkip`; kein String-Parsing.
44 Winner-Matcher deterministisch: nur erfolgreiche TXs (Signature-err und meta.err), früheste Landing ≥ since_slot, Pagination bis unter since_slot, Deckel 10 Seiten → `Err` (fail closed).
45 `RaceOutcome {ReadyBeforeWinner | SameSlotUnknown | Loss}`: Win nur bei `ShadowReady` ∧ `sign_slot < winner_slot`; gleicher Slot zählt separat; `ShadowSimFailed`/`Skip` nie Win.
46 Ledger: `plans_built` zählt ShadowReady + ShadowSimFailed; `same_slot_unknown`; letzter (Teil-)Tag beim Shutdown geschrieben + flush.
47 `classify`-Kollision in main.rs per Alias `classify_account` behoben.
Repo-Struktur: `project_0/{native,docs,scripts,data}`; Default-Pfade `project_0/data/shadow/*.jsonl`; Phase-Dokument unter `project_0/docs/PHASE_P6LT_SHADOW.md`.
Offen: Build/Tests (vier Logs), C0, P4A-03 Sizing, RouteProvider, v1 Sign/Wire, Staging, P8-05.


### 2026-10-02 — Mac compiler/test gate — PASS

**Environment:** user MacBook Pro, local repo root `/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot`. Exact `rustc`/`cargo` versions were not captured in the chat and are therefore **N/A**, not inferred.

**Source baseline:** final reported green run after `[FIX-58]`, commit `870aecdf4d63b33ace2e38e50f37c25748d9cfaf`.

**Commands/results reported by the user:**
- `cargo build --manifest-path project_0/native/Cargo.toml --features crate_health` → **PASS**, warning-free after `[FIX-54]`.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health` → **PASS** after test corrections `[FIX-55..57]`.
- `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → **PASS** after `[FIX-58]`.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → **PASS**.
- Per-test-target counts for the final two successful workspace runs were not fully pasted; do not infer them from earlier failed runs.

**Compiler/test fixes closed in this gate:** `[FIX-48]` dependency alignment, `[FIX-49]` Solana component versions, `[FIX-50..53]` Project_0 compile blockers / trait-object support, `[FIX-54]` warning cleanup, `[FIX-55]` planner test construction, `[FIX-56]` histogram `u64::MAX` overflow/bucket bound, `[FIX-57]` stable reconciliation test owner with owner-mismatch regression, `[FIX-58]` canonical v1 message-size API.

**What this PASS proves:** the workspace compiles and its current unit-test suite passes in both required feature combinations on the user's Mac.

**What this PASS does NOT prove:** Mainnet/Staging correctness, route availability, exact Sizing, P3-02 thresholds, v1 signing/wire/submission, landing behavior, token-delta reconciliation, realized PnL, capture rate, or LIVE profitability.

**Gate decision:** compiler/test baseline **PASS**. Next functional closure remains:
`Exact Health → Sizing → RouteProvider → Funding → P0ReceivershipBuilder → Economics → Build → Sign → Simulate`.
`NoPlans` must not be removed until that real path is wired and fail-closed.


### 2026-10-02 — Sitzung 12 (Compiler closure → exact sizing → Jupiter V2 → real runner wiring) — HANDOFF CHECKPOINT

**Purpose of this checkpoint:** this section is the handoff source for the next chat/session. Do not infer completion beyond the explicit PASS statements below.

**Current repository state at documentation time:** `7a55366182c07f1f99951427d5a98d7d8937a76b` on `main` after `[FIX-70]`. Parallel BSC commits were preserved and never reverted.

#### A. What is conclusively complete

1. **Original Fable compiler/unit-test closure is complete for the pre-wiring baseline.**
   - `cargo build --manifest-path project_0/native/Cargo.toml --features crate_health` → PASS.
   - `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health` → PASS.
   - `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → PASS.
   - `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → PASS.
   - This closed the initial compile blockers through `[FIX-58]`.
   - Therefore the old Fable requirement **"compile/test the written Rust instead of leaving it UNTESTED"** was satisfied for that baseline.

2. **Current post-wiring HEAD compiles with both important features enabled.**
   User-reported command:
   `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"`
   → **PASS** on 2026-10-02 after `[FIX-59..69]`; only one unused-import warning remained.
   `[FIX-70]` removed that feature-gated warning after the reported build.
   A fresh build after `[FIX-70]` is still required to record the CURRENT HEAD as warning-free.

3. **Compiler/test fixes already closed:** `[FIX-48..58]`.
   Important verified corrections include Solana dependency alignment, trait-object AccountSource support, reconciliation-test ownership semantics, histogram `u64::MAX` overflow handling, and v1 message-size API.

#### B. Functional work added after the original green baseline

**[FIX-59] Exact protocol receivership sizing**
- New `darkbot-project0/src/sizing.rs`.
- Uses the pinned Marginfi crate/source instead of an independent health formula.
- Enumerates protocol-valid Asset×Liability pairs rather than hardcoding one "largest" pair.
- Uses Marginfi Maintenance vs Equity price semantics and e-mode reconciliation.
- Uses on-chain `FeeState.liquidation_max_fee`.
- Keeps protocol sizing distinct from executable market realizability.

**[FIX-60..64] Correct economics/route boundaries**
- Protocol oracle/equity value is not treated as executable market value.
- Economics monetary fields made explicitly USD-micro.
- Raw lamports were separated from USD-micro economics fields.
- Trigger slot is carried into sizing.
- RouteProvider changed to mint-based routing.
- Executable realizable value derives from route output relative to protocol liability value.
- Jupiter output already incorporates AMM fees/price impact, so those are not double-subtracted.
- ZERO_PREFUND requires route output ≥ actual repay amount.

**[FIX-65] Jupiter Swap API V2 RouteProvider**
- Concrete `JupiterV2RouteProvider` uses `/swap/v2/build`.
- DarkBot retains transaction ownership: blockhash, CU limits, signing, simulation and submission stay in our core.
- Parses raw setup/swap/cleanup/other instructions and ALT address contents.
- Only Marginfi-allowed top-level Jupiter/ATA route instructions are accepted; incompatible routes fail closed.
- ExactOut/JIT remains disabled until a documented/qualified provider path exists.
- Optional API key; implementation does not require a paid subscription.

**[FIX-66..69] Runtime bridge and actual runner wiring**
- Token / Token-2022 IDs and native SOL mint added.
- Protocol-consistent USD valuation method added for native token amounts using pinned Marginfi equity pricing.
- `ProtocolSizingProvider` bridges exact protocol amounts into runtime economics.
- Liquidator ATA balances are read from reconciled RAM state.
- Token and Token-2022 ATA derivations are supported.
- ATA subscriptions are optional; missing ATAs disable candidates rather than blocking recovery.
- Required oracle/mint dependencies remain fail-closed.
- WSOL Marginfi bank is resolved to value SOL-denominated costs.
- Real `P0ReceivershipBuilder` is wired in `p0-longtail` when `crate_health` is enabled.
- `NoPlans` remains only as the compile-time fallback when `crate_health` is disabled.
- ZERO_PREFUND is **not** assumed qualified: default false; requires `P0_ZERO_PREFUND_QUALIFIED=true`.
- JIT remains fail-closed because ExactOut is not yet qualified.
- `FailedCostMatrix::default()` remains Unknown, deliberately preventing fake-zero economics.

**[FIX-70]**
- Removed the feature-gated unused `ReceivershipBuilder`/`ReceivershipPlan` warning from the real `crate_health` build path.

#### C. Current functional path

With `crate_health` the intended Shadow path is now:

```text
WSS update
→ reconciled StateCore
→ exact Marginfi Health
→ ProtocolSizer
→ ATA inventory from RAM state
→ Jupiter V2 executable route
→ P0ReceivershipBuilder
→ deterministic Economics
→ TX Build
→ Sign
→ Simulate
→ Shadow Send-Boundary
```

This is materially beyond the earlier Fable state where `NoPlans` blocked planning.

#### D. What is NOT complete yet

Do **not** call Project_0 finished. The following are still open:

1. **Post-wiring full test rerun.**
   After `[FIX-59..70]`, run both:
   - `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"`
   - `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health`
   The previous green unit-test baseline predates the new sizing/router/runtime wiring.

2. **Fresh warning-free build after [FIX-70].**
   Re-run:
   - `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"`

3. **Failed-attempt cost matrix.**
   Current cells are unknown by design. Economics therefore stays non-actionable until real path/outcome costs are measured or otherwise qualified. Unknown must not become zero.

4. **Actual Shadow/Mainnet qualification.**
   Current compile success does not prove:
   - Jupiter route availability for live Marginfi opportunities,
   - full receivership simulation success,
   - ZERO_PREFUND feasibility,
   - capture/winner performance,
   - landing behavior,
   - realized token deltas/PnL.

5. **v1 execution remains partial.**
   v1 capability/message measurement compiles, but final v1 sign/wire/submission support is still not closed.

6. **P3-02 threshold scheduler remains open.**
   `HealthEngine::thresholds()` is still fail-closed/Unsupported; trigger (b)/(c) work remains separate from trigger (a).

7. **ExactOut/JIT is not qualified.**
   Jupiter V2 provider currently returns `None` for ExactOut, therefore JIT_SHORTFALL is intentionally disabled.

8. **Integration-bank refresh/withdraw completeness.**
   Generic banks are handled; unsupported integration-bank paths remain skipped/fail-closed until their refresh/withdraw instruction planning is fully wired and qualified.

9. **P8 token-delta reconciliation / realized PnL remains open.**
   Landing alone is not sufficient economic evidence.

10. **LIVE remains deliberately disabled.**
    `P0_MODE=live` still refuses startup until P8-LT qualification is recorded.

#### E. Assessment against Fable's earlier requested work

**Compiler/build/unit-test problem from Fable:** substantially **closed** for the original baseline and current code **builds** after the major wiring changes.

**Fable's broader production objective:** **not closed yet**. Compilation was only one gate. The remaining work is now predominantly functional qualification/evidence rather than basic Rust dependency repair.

Current readiness summary:

```text
Dependency/compiler repair             DONE
Pre-wiring workspace unit tests         DONE
Exact Marginfi Health                   DONE for trigger-a path
Protocol Sizing                         IMPLEMENTED + COMPILES
Jupiter V2 RouteProvider                IMPLEMENTED + COMPILES
P0ReceivershipBuilder runtime wiring    IMPLEMENTED + COMPILES
NoPlans in crate_health runtime         REMOVED
Post-wiring workspace tests             DONE
Failed-cost evidence                    OPEN
Shadow simulation on real opportunities OPEN
ZERO_PREFUND qualification              OPEN
v1 sign/wire/submission                 OPEN
P3-02 thresholds / triggers b,c         OPEN
ExactOut/JIT                            OPEN
P8 realized PnL/reconciliation          OPEN
Controlled LIVE                         BLOCKED intentionally
Production profitability                UNPROVEN
```

#### F. Exact next commands

From repository root:

```bash
cd "/Users/sawyer_ma/Desktop/Firma/2_DarkMa/6_DarkBot/DarkBot"

git pull --ff-only

cargo build \
  --manifest-path project_0/native/Cargo.toml \
  --features "crate_health txv1"

cargo test \
  --manifest-path project_0/native/Cargo.toml \
  --workspace \
  --features "crate_health txv1"

cargo test \
  --manifest-path project_0/native/Cargo.toml \
  --workspace \
  --features crate_health
```

If all three are green, update this ledger again and then proceed to the first real Shadow simulation/qualification gate rather than adding more unmeasured architecture.

### 2026-10-02 — Post-wiring compiler/test gate — PASS

**Source line:** `[FIX-59..70]` functional wiring is now compiler/test qualified on the user Mac.

User-reported requested sequence:
- `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → **PASS**.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → **PASS**.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features crate_health` → **PASS**.

The visible final terminal line was a zero-test target (`0 passed; 0 failed`), which is normal and does not represent the total workspace test count. No exact aggregate count is recorded because it was not pasted.

**Gate decision:** dependency/compiler/unit-test work requested by the earlier Fable phase is now **CLOSED for the current wired code**, not merely for the old baseline.

**Next gate:** actual Shadow/Mainnet functional qualification:
`real account update → exact health → exact sizing → executable route → receivership build → sign → simulate → record evidence`.

**Important blocker before Shadow can be called qualified:** `FailedCostMatrix` remains unknown by design. Shadow must not convert unknown failed-attempt cost to zero merely to reach simulation. The qualification path must preserve unknown economics while still allowing technical build/sign/simulate evidence, or first populate those cells from real evidence.


### 2026-10-02 — [FIX-71] Technical Shadow Probe path

Problem: `FailedCostMatrix` is intentionally Unknown until real failed-attempt costs are measured. The previous pipeline therefore rejected every Unknown-economics candidate before TX build/sign/simulation, blocking the first real Shadow qualification.

Resolution:
- LIVE behavior is unchanged: only `Evaluation::Known` with `net > min_profit` can ever become actionable.
- Shadow may now admit `Evaluation::Unknown` only as a **technical probe**.
- Unknown economics are never coerced to zero and `net_profit` stays N/A.
- Separate record decisions: `ShadowTechnicalReady` and `ShadowTechnicalSimFailed`.
- Technical probes count toward build/sign/simulation evidence only.
- They are excluded from virtual winner/PnL accounting; only economically known `ShadowReady` may count as a virtual win.
- When no economically actionable candidate exists, technical probes are selected deterministically by lower serialized bytes, then fewer locked accounts; this is not an economic ranking.

Commits: `5ec5df6`, `2a9f536`, `ffa69d0`, `7ad9a87`.

Next gate: compile/tests for [FIX-71], then run a real Mainnet Shadow session and capture the first `ShadowTechnicalReady` or simulation failure with its exact reason.


### 2026-10-02 — [FIX-71] build/test gate — PASS

User-reported current-head verification after [FIX-71]:
- `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → PASS.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → PASS.
- Final visible line was a zero-test target (`0 passed; 0 failed`), not the workspace aggregate.

Gate decision: technical-shadow path compiles/tests. Next step is the first real Mainnet Shadow run.


### 2026-10-02 — [FIX-72] Mainnet Shadow launcher / ZERO_PREFUND probe separation

- Added `P0_ZERO_PREFUND_SHADOW_PROBE`: it may enable ZERO_PREFUND only while `P0_MODE=shadow`.
- `P0_ZERO_PREFUND_QUALIFIED` remains false by default and is not implied by a technical probe.
- Added executable launcher: `project_0/scripts/run_shadow_mainnet.sh`.
- Launcher stores no secrets; RPC/WSS, group and keypair path must come from environment.
- Mainnet Marginfi program defaults to the source-verified program ID.
- Slippage is required explicitly; no guessed trading value is inserted.
- Next roadmap gate before the 72 h P6-LT run remains G2/C0 Mainnet census, followed by G5 bootstrap/reconcile and first technical Shadow simulation.

Commits: `d0f07e5`, `b2d39ba`.


### 2026-10-02 — C0-01 Mainnet census — PASS

Observed on Mainnet via Helius free RPC:

- context_slot: `452723812`
- accounts_total: `172742`
- bytes_total (full account-space sum): `387399978`
- MarginfiAccount: `165597`
- LiquidationRecord: `6330`
- Bank: `437`
- MarginfiGroup: `162`
- FeeState: `1`
- StakedSettings: `1`
- unknown-discriminator rows: `214`
- compact getProgramAccounts duration: `61.843 s`
- raw evidence: `project_0/data/c0/accounts_slot452723812.jsonl`
- report: `project_0/data/c0/c0_census_slot452723812.json`

Unknown-discriminator review against pinned `mrgn-0.1.11-rc1`:
- `31cf1f2243e1a9ba` ×204 = `BankMetadata` (source-confirmed)
- `86addfb94d561c33` ×1 = `Order` (source-confirmed)
- `184662bf3a907b9e` ×9 = still unidentified auxiliary account type; not classified as MarginfiAccount/Bank and therefore ignored by the liquidation core

**C0-01 decision:** PASS. The program/account census is real Mainnet evidence, not fixture data.

**G2 is not yet final GO:** the first scan aggregates all 162 Marginfi groups. `[FIX-75]` adds a second compact scan for the configured group using `MarginfiAccount.group` plus on-chain `IndexerFlags` (`has_liability`, `was_liquidatable`, `was_underwater`, activity flags). That group-specific result is the remaining C0/G2 evidence.

**[FIX-76] runner scope:** Project_0 now filters Bank dependencies and MarginfiAccount triggers to the configured `P0_GROUP_ID`; foreign groups no longer enter the liquidation pipeline.


### 2026-10-02 — C0-02 target-group census / G2 Shadow GO

Second Mainnet census, target group `4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8`:

- overall context_slot: `452727107`
- group_context_slot: `452727389`
- group MarginfiAccounts: `165123`
- has_liability: `82680`
- lending_only: `82443`
- single_borrower: `67556`
- empty: `4662`
- has_ever_been_liquidated: `5114`
- was_liquidatable_last_pulse: `61`
- was_underwater_last_pulse: `60`
- active_30d: `9954`
- active_60d: `9964`
- trivial_balance: `9`
- group scan duration: `48.198 s`

Evidence:
- `project_0/data/c0/group_accounts_slot452727389.jsonl`
- `project_0/data/c0/c0_census_slot452727107.json`

**G2 decision for Shadow qualification: GO.**
The target group has a large liability population and directly observed recent liquidatable/underwater accounts, therefore there is enough real Mainnet activity to justify the Shadow qualification run.

**Not a profitability claim:** C0 does not prove that realizable liquidation edge exceeds all execution/failure costs. That remains for Shadow simulation, winner matching, failed-cost measurement and realized-PnL evidence.

Follow-up hardening:
- `[FIX-77]` adds filtered Program-account snapshots.
- `[FIX-78]` changes bootstrap/recovery from full-program (~387 MB account-space census) to target-group Banks + target-group liability accounts + explicit required/optional dependencies. MarginfiGroup and FeeState are explicitly required so filtering does not silently drop them.
- `[FIX-79]` applies the same target-group liability/Bank filters to WSS `programSubscribe`.
- Lending-only accounts are excluded from the liquidator runtime state because they cannot be liquidated while they have no liabilities.

Next gate: compile/tests for [FIX-77..79], then G5 Mainnet Bootstrap:
`StreamReady → filtered Snapshot → Reconcile → READY → sample exact Health → first ShadowTechnicalReady/ShadowTechnicalSimFailed`.


### 2026-10-02 — [FIX-77..79] filtered bootstrap/WSS build-test gate — PASS

User-reported current-head verification:
- `cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"` → PASS.
- `cargo test --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"` → PASS.
- Final visible line was a zero-test target (`0 passed; 0 failed`), not the aggregate workspace count.

Gate decision: filtered target-group/liability bootstrap, recovery and WSS subscription path compile/tests. Proceed to G5 real Mainnet bootstrap evidence.


### 2026-10-02 — Current Project_0 handoff / progress checkpoint

This checkpoint is the current source for continuation work after C0/G2 and before G5 Mainnet runtime qualification.

#### Evidence already closed

- Compiler/dependency closure through the current wired Project_0 path: PASS on the user Mac.
- Exact Marginfi Health for trigger (a): implemented against pinned Marginfi source.
- Exact protocol receivership sizing: implemented.
- Protocol sizing vs realizable route value: separated.
- Jupiter V2 raw-instruction RouteProvider: implemented.
- P0ReceivershipBuilder wired into the crate_health runtime; `NoPlans` is no longer used in that runtime.
- Technical Shadow path with unknown failed-attempt economics: implemented without converting Unknown to zero.
- C0-01 full-program Mainnet census: PASS.
- C0-02 configured-group census: PASS for Shadow activation.
- Filtered target-group/liability bootstrap, recovery and WSS path: build/test PASS.

#### Mainnet C0 evidence

Configured group: `4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8`

```text
group MarginfiAccounts             165123
has_liability                       82680
lending_only                        82443
single_borrower                     67556
has_ever_been_liquidated             5114
was_liquidatable_last_pulse            61
was_underwater_last_pulse              60
active_30d                           9954
active_60d                           9964
```

Decision: **G2 GO for Shadow qualification.**
This proves a real Mainnet liquidation population exists. It does **not** prove sustainable profit.

#### Recent implementation sequence

```text
[FIX-71] Unknown-economics technical Shadow probe path
[FIX-72] Shadow launcher + ZERO_PREFUND shadow-probe separation
[FIX-73] Census RPC/auth hardening
[FIX-74] Compact 8-byte discriminator census
[FIX-75] Configured-group IndexerFlags census
[FIX-76] Runtime trigger/bank scope restricted to P0_GROUP_ID
[FIX-77] Filtered Program-account snapshot primitive
[FIX-78] Bootstrap/recovery = liability accounts + group banks + explicit dependencies
[FIX-79] WSS programSubscribe uses the same liability/group filters
[FIX-80] Mainnet Shadow launcher derives normal Helius RPC/WSS endpoints from HELIUS_API_KEY
```

#### Current G-gate state

```text
G1 compiler/tests                         PASS
G2 C0 Mainnet market evidence             GO for Shadow
G3 exact sizing                           IMPLEMENTED + build/test qualified
G4 executable Jupiter route provider      IMPLEMENTED + build/test qualified
G5 real Mainnet bootstrap/reconcile       NEXT / NOT YET EVIDENCED
```

#### Exact current next gate

Run G5 on Mainnet using the production Shadow code:

```text
StreamReady
→ filtered target-group/liability Snapshot
→ required dependencies present
→ Recovery/Reconcile
→ READY
→ real MarginfiAccount updates
→ exact Health records
→ exact Sizing
→ executable Route
→ P0ReceivershipBuilder
→ Build
→ Sign
→ Simulate
→ ShadowTechnicalReady or ShadowTechnicalSimFailed
```

Do not mark G5 PASS until the real runtime output shows READY and valid Health evidence. Do not mark P6-LT qualified until a real opportunity reaches build/sign/simulation and the record is captured.

#### Still open after G5

- FailedCostMatrix: real failed-attempt cost evidence.
- ZERO_PREFUND: technical Shadow probe is allowed; LIVE qualification is still false.
- v1 final sign/wire/submission support.
- ExactOut/JIT qualification.
- Integration-bank refresh/withdraw completeness.
- P3-02 thresholds and triggers (b)/(c).
- P8 landing/token-delta reconciliation and realized PnL.
- Controlled LIVE remains intentionally disabled.
- Sustainable production profitability remains unproven.

#### Rough engineering position

Implementation completeness is now roughly **70–75%** of the intended technical product, but production qualification is materially lower because G5/P6/P8 evidence is still missing. This is an engineering estimate, not a measured KPI.


### 2026-10-02 — G5 Mainnet bootstrap evidence — PASS

Observed from the real Mainnet Shadow runner:

```text
dependencies: banks=204 required=384 optional_atas=328
bootstrap: waiting for StreamReady (all subscription ACKs)
recovery: Ready, run_state=Ready
```

What this proves:
- Helius WSS connection succeeded.
- All configured subscriptions reached the StreamReady ACK barrier.
- The filtered target-group/liability snapshot completed.
- Required dependency reconciliation completed.
- StateCore transitioned to `Ready` on real Mainnet state.

At the first observed READY window there were no filtered MarginfiAccount/Bank updates yet. Channel counters still advanced because Slot/Root/Health events were flowing, so the stream was alive.

**G5 bootstrap/recovery decision:** PASS for `StreamReady → Snapshot → Reconcile → READY`.

Still required before P6-LT Shadow qualification:
- receive a real target MarginfiAccount update after READY,
- compute exact Health from reconciled state,
- reach exact sizing and executable route,
- build/sign/simulate a real opportunity,
- capture `ShadowTechnicalReady` or `ShadowTechnicalSimFailed` with the exact reason.

[FIX-81] removes misleading repeated `updates: accounts=0 ...` log spam when the program-account update counter remains zero; reporting now occurs once per real 1000 account updates.


### 2026-10-02 — Real provider interruption observed during Shadow — RECOVERY EVIDENCE OPEN

Observed after the first successful Mainnet READY:

```text
blockhash refresh: error sending request for url (...)
[ws_rpc] session ended: ingress connect: idle timeout — reconnect in 500ms
```

Interpretation:
- HTTP RPC and WSS degraded in the same runtime window.
- The WSS idle watchdog correctly declared the stream unhealthy after the configured idle timeout.
- The reconnect path was entered automatically.
- Because no second `recovery: Ready, run_state=Ready` had yet been observed at documentation time, reconnect/recovery is **not** marked PASS yet.

Required evidence to close this item:
```text
reconnect
→ all subscription ACKs
→ filtered Snapshot
→ Reconcile
→ recovery: Ready, run_state=Ready
```

[FIX-82] redacts provider URLs/API keys from blockhash transport errors and emits a single `recovered` message when the blockhash poller becomes healthy again. Runtime fail-closed behavior is unchanged.


### 2026-10-02 — Real provider interruption recovery — PASS

Observed sequence on Mainnet:

```text
recovery: Ready, run_state=Ready
blockhash refresh: error sending request ...
[ws_rpc] session ended: ingress connect: idle timeout — reconnect in 500ms
recovery: Ready, run_state=Ready
```

Decision: **PASS** for real provider interruption recovery.

This proves the production Shadow runtime can:
`READY → detect feed loss → halt trusted state → reconnect → re-subscribe → filtered snapshot → reconcile → READY`.

This closes the open reconnect/recovery evidence item from the prior checkpoint. It does not yet prove a complete liquidation opportunity through Build/Sign/Simulate.


### 2026-10-02 — [FIX-84] Immediate bootstrap Shadow probe — IMPLEMENTED / VERIFY

Purpose: avoid waiting indefinitely for a new account update after G5 READY when the reconciled snapshot already contains accounts whose on-chain `IndexerFlags.was_liquidatable == 1`.

Implementation:
- after the first successful `RecoveryOutcome::Ready` in Shadow mode only,
- scan the already reconciled target-group MarginfiAccounts in RAM,
- select only accounts with `was_liquidatable == 1`,
- queue those account states back through the existing account-trigger path,
- therefore reuse the same exact Health → Sizing → Route → P0ReceivershipBuilder → Build → Sign → Simulate pipeline,
- run the bootstrap probe once per process; later recovery events do not requeue it.

No LIVE behavior is enabled or changed.

Commits:
- `1ef720f` selector
- `ab727b1` one-shot probe queue

Status: IMPLEMENTED, not yet compiler/Mainnet-qualified. Next evidence is build/test PASS followed by `bootstrap_probe: queued=N` and resulting Shadow records.


### 2026-10-02 — [FIX-84] Mainnet bootstrap probe — QUEUED / OUTCOMES OPEN

Observed on Mainnet Shadow after successful StreamReady and two-snapshot reconciliation:

```text
dependencies: banks=204 required=384 optional_atas=328
[ws_rpc] session attempt=1 connected; subscriptions=716
[ws_rpc] session attempt=1 subscription ACKs=716/716
[ws_rpc] session attempt=1 StreamReady
snapshot: done slot=452753636 liability_accounts=82685 banks=204 required=384 optional=0 missing_required=0 total=83273 elapsed_ms=7328
snapshot: done slot=452753663 liability_accounts=82685 banks=204 required=384 optional=0 missing_required=0 total=83273 elapsed_ms=6099
recovery: Ready, run_state=Ready
bootstrap_probe: queued=61 candidates from was_liquidatable
```

Evidence:
- all 716 subscriptions ACKed;
- StreamReady barrier passed;
- snapshot A/B completed with 0 missing required dependencies;
- reconciled state returned READY;
- the one-shot bootstrap probe selected and queued 61 `was_liquidatable` candidates from the reconciled snapshot.

Status: **QUEUED, not yet counted as fully processed.**
Completion evidence requires confirming the resulting per-account records in `project_0/data/shadow/opportunities.jsonl`.


### 2026-10-02 — [FIX-84] Bootstrap probe Health outcomes — PASS (61/61)

Observed from `project_0/data/shadow/opportunities.jsonl` for snapshot slot `452753636`:

```text
probe health records at slot 452753636: 61
liquidatable=true at slot 452753636: 61
```

Decision:
- **61/61 queued bootstrap candidates were processed through Exact Health.**
- **61/61 were currently liquidatable according to the reconciled Mainnet state and current health engine.**
- This closes the bootstrap-probe Health-processing evidence.
- It does **not yet** classify the 61 downstream Pipeline outcomes because OpportunityRecord decision lines do not carry the snapshot `slot`; downstream outcome distribution must be counted separately.

Known sampled outcomes from the same run showed `bad_debt=true`, `equity_assets_bits=0` and therefore no receivership execution candidate. [FIX-85] separates these bankruptcy/no-collateral cases from Routing on the next build.


### 2026-10-02 — Bootstrap probe downstream outcomes — BLOCKED BEFORE BUILD

Observed distribution across all 61 processed Mainnet bootstrap probes from snapshot slot `452753636`:

```text
records: 61

DECISIONS
61 Skip

LOSSES
61 Routing

REASONS
54 no feasible execution candidates
7 oracle adapter: ReserveStale (Kamino reserve refresh required)
```

Interpretation:
- Exact Health reached all 61 candidates and marked 61/61 liquidatable.
- **0/61 reached Build, Sign, or Simulation.**
- 54/61 produced no protocol-executable receivership candidate.
- 7/61 were blocked during exact sizing/oracle access because the Kamino reserve data was stale and a `refresh_reserve` prerequisite is required.
- Sampled 54/61 no-plan cases showed `bad_debt=true` and `equity_assets_bits=0`, i.e. no-collateral bankruptcy cases rather than DEX routing failures.
- The pinned marginfi bankruptcy path is not permissionless for the production group, so those no-collateral bad-debt accounts are not valid receivership opportunities for this Shadow signer.

[FIX-85]/[FIX-86] improve classification:
- `Bankruptcy` for bad debt with zero equity assets,
- `IntegrationRefresh` for `ReserveStale`,
- bootstrap probes carry `bootstrap_snapshot_probe` trigger provenance.

Current P6-LT blocker: integration refresh completeness and obtaining at least one current liquidatable account with executable collateral/liability sizing. No Build/Sign/Sim evidence exists yet.


### 2026-10-03 — Fable audit remediation R0 / R1-A5 checkpoint

R0 compiler baseline after [FIX-87]:

```text
cargo build --manifest-path project_0/native/Cargo.toml --features "crate_health txv1"
cargo test  --manifest-path project_0/native/Cargo.toml --workspace --features "crate_health txv1"
...
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Decision: **R0 PASS**.

A5 circuit-breaker remediation is now implemented but not yet compiler/Mainnet-qualified:

- [FIX-88a] off-chain receivership CB gate using explicit Client Clock;
- all breaker/oracle math remains in pinned marginfi `BankImpl` / `OraclePriceFeedAdapter`;
- [FIX-88b] dedicated `LossReason::CircuitBreaker`;
- [FIX-88c] shared `HealthEngine::receivership_eligibility` contract;
- [FIX-88d] shared Pipeline invokes eligibility after liquidatable Health and before Builder/Sizing.

Status: **IMPLEMENTED / VERIFY**. Next gate is build/test green before structured planning-rejection diagnostics are introduced.


### 2026-10-03 — R1 implementation complete — VERIFY

R1-A5 build/test was reported green after [FIX-88].

[FIX-89] adds structured planning rejection evidence without relaxing fail-closed behavior:

- `SIZING:NO_ASSET`
- `SIZING:NO_LIABILITY`
- `SIZING:UNSUPPORTED_INTEGRATION_ASSET`
- `SIZING:UNSUPPORTED_INTEGRATION_LIABILITY`
- `SIZING:NO_SUPPORTED_ASSET_BANK`
- `SIZING:NO_SUPPORTED_LIABILITY_BANK`
- `SIZING:NO_VALID_PROTOCOL_PAIR`
- `ROUTING:NO_ROUTE`
- `ROUTING:INSUFFICIENT_ROUTE_OUTPUT`
- `ATA:MISSING_ASSET:<mint>`
- `ATA:MISSING_LIABILITY:<mint>`

Typed loss categories now include `CircuitBreaker`, `Sizing`, `UnsupportedIntegration`, `IntegrationRefresh`, `MissingAta`, `Funding`, and `Routing`.

Important: A1 (NoSwap/Preheld incorrectly coupled to a market quote) is **not fixed by FIX-89**. A `ROUTING:NO_ROUTE` result remains evidence of that known blocker until R3.

Status: **IMPLEMENTED / VERIFY**. Next gate: workspace build/test green, then Mainnet Shadow rerun to obtain the real 61-probe rejection distribution.


### 2026-10-03 — R1 PASS

Workspace build/test after FIX-88 + FIX-89:

```text
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Decision: **R1 PASS (compiler/workspace qualification).**

Included:
- exact shared receivership circuit-breaker eligibility gate before planning;
- typed rejection diagnostics for sizing, integration support, route, ATA, funding, and circuit-breaker failures;
- no relaxation of fail-closed protocol behavior;
- A1 remains intentionally unfixed so the next Mainnet Shadow rerun can measure the true rejection distribution before remediation.

Next gate: Mainnet Shadow rerun and 61-probe outcome census with typed reasons.


### 2026-10-03 — FIX-90 recovery diagnostics / subsequent Mainnet recovery — PASS

A preceding Mainnet Shadow start exhausted all three recovery attempts before completing Snapshot A. The old RecoveryDriver discarded fetch errors, so the exact provider/RPC cause was not observable.

[FIX-90] added:
- redacted per-attempt Snapshot A/B recovery error logging;
- stage labels for `liability_gpa`, `bank_gpa`, `required_accounts`, and `optional_accounts`;
- clean/dirty reconcile logging;
- no change to recovery/trading semantics.

On the immediately subsequent Mainnet run:

```text
[recovery] attempt=1/3 snapshot=A start
snapshot: done slot=452768986 liability_accounts=82686 banks=204 required=384 optional=0 missing_required=0 total=83274 elapsed_ms=6253
[recovery] attempt=1/3 snapshot=A ok slot=452768986 accounts=83274 missing_required=0
[recovery] attempt=1/3 snapshot=B start
snapshot: done slot=452769009 liability_accounts=82686 banks=204 required=384 optional=0 missing_required=0 total=83274 elapsed_ms=6273
[recovery] attempt=1/3 snapshot=B ok slot=452769009 accounts=83274 missing_required=0
[recovery] attempt=1/3 reconcile=clean snapshot_slot=452769009
recovery: Ready, run_state=Ready
bootstrap_probe: queued=61 candidates from was_liquidatable
```

Decision:
- Recovery remains Mainnet-qualified.
- The previous Exhausted event is classified as **transient / exact root cause unobserved**, because it occurred before FIX-90 exposed fetch errors.
- FIX-90 diagnostics are confirmed operational.
- No evidence currently supports changing retry count or recovery semantics.


### 2026-10-03 — R1 Mainnet typed rejection census — PASS

Mainnet Shadow bootstrap probe after R1 diagnostics:

```text
HEALTH RECORDS: 61
LIQUIDATABLE: 61
DECISION RECORDS: 61

LOSSES
55 Sizing
6 IntegrationRefresh

REASONS
53 SIZING:NO_ASSET
6 JuplendLendingStale
2 SIZING:NO_VALID_PROTOCOL_PAIR

DECISIONS
61 Skip
```

Interpretation:
- 53/61 are no-collateral cases at sizing input (`NO_ASSET`); sampled/previous evidence shows these are bad-debt bankruptcy states, not normal receivership opportunities.
- 6/61 are blocked by stale JupLend integration state; this independently confirms the missing integration-refresh path beyond the earlier Kamino evidence.
- 2/61 have both sides present on supported generic banks but still produce no protocol-valid Asset×Liability pair. These are the highest-value candidates for the next generic-path diagnosis because they reach deeper than the 53 no-asset and 6 integration-stale cases.
- 0/61 reached route/build/sign/simulation, so A1 is not yet exercised by this sample.

[FIX-91] changes bankruptcy classification to depend on protocol state (`bad_debt && equity_assets<=0`) rather than the prior planner loss category, so no-collateral cases are not mislabeled as Sizing merely because diagnostics improved.

Status:
- R1 diagnostics: **Mainnet PASS**.
- FIX-91: **IMPLEMENTED / VERIFY**.
- Next diagnostic target: explain the 2 `NO_VALID_PROTOCOL_PAIR` cases at pair-level before touching route/funding.


### 2026-10-03 — FIX-93 closeout sizing parity — IMPLEMENTED / VERIFY

Mainnet evidence for account `CwSsTePXHZUkiZgmKtXwxmEX2uU1ddY5aUjRKkjKvH6d`:

```text
liquidatable=true
bad_debt=true
equity_assets_bits≈51.4M
pair-level rejection: premium_bound=2
```

Protocol threshold:
`LIQUIDATION_CLOSEOUT_DOLLAR_THRESHOLD = 5 USD`, represented in Q48 as `5 << 48`.
The observed equity-assets value is therefore far below the closeout threshold.

Pinned marginfi semantics:
- `end_liquidation` sets `ignore_healthy=true` when pre-liquidation equity assets are below the closeout threshold;
- in closeout, the normal post-health `<= 0` requirement is bypassed;
- in closeout, the `seized <= repaid * max_fee` premium bound is also bypassed.

[FIX-93]:
- removes the normal `health_cap` only when `HealthView::closeout_allowed == true`;
- skips the premium-bound rejection only in that same closeout branch;
- leaves normal-liquidation sizing unchanged;
- retains the existing liability cap conservatively; no new repay-all strategy is introduced.

Important: this does **not** validate Fable C1 globally. C1 remains false for normal liquidations; FIX-93 implements only the explicit closeout exception from pinned protocol source.

Status: **IMPLEMENTED / VERIFY**.
