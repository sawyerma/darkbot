# Solana / Project 0 — Measurement & Evidence Ledger

Status: **R4 PASS, R5 PASS; first full Mainnet receivership simulation proven; R6 integration/projection work PARTIAL; latest compile blocker in FIX-111d corrected by FIX-111e but not yet compiler-verified; LIVE remains disabled**  
Updated: 2026-10-04

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
Project 0 production code:        Full Rust Long-Tail path active on Mainnet Shadow; fixes through [FIX-105] committed
Bootstrap/recovery:               MAINNET PASS (StreamReady → Snapshot A/B → root catch-up → reconcile clean → READY)
Exact Health/Sizing:              MAINNET exercised; bankruptcy/integration cases typed fail-closed
Build + Sign + simulation call:   MAINNET PROVEN (R97/R99/R101)
Successful receivership sim:      OPEN — no ShadowTechnicalReady yet
Current immediate blocker:        ZERO_PREFUND Jupiter route rejection reason; [FIX-105] committed, R105 pending
Liquidator payer:                 FUNDED / VERIFIED; dedicated Project_0 signer
Required Trrq ATAs:               WSOL + USDC PREWARMED and confirmed on Mainnet
R4 hot-path isolation:            PASS — R108 runtime-qualified
Integration refresh completeness: OPEN (Kamino/Drift/JupLend paths)
Failed-cost / realized economics: OPEN / UNKNOWN by design
72 h P6-LT Shadow:                NOT STARTED
Project 0 LIVE:                   NOT ENABLED
First realized net profit:        NOT PROVEN
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


### 2026-10-03 — FIX-94 deterministic SOL cost bank — IMPLEMENTED / VERIFY

Mainnet evidence after FIX-93:
- `CwSs...` no longer fails on the closeout premium bound;
- the next error is `JuplendLendingStale`;
- the generic ProtocolSizer itself excludes integration banks;
- inventory value is zero for the empty Shadow wallet and performs no price lookup;
- therefore the remaining post-sizing integration-price access is the Flat-SOL-fee valuation through `sol_bank`.

Root cause:
- runner selected `sol_bank` as the first Marginfi bank with the WSOL mint;
- multiple same-mint banks may exist, including integration banks;
- cost valuation could therefore accidentally depend on stale JupLend/Kamino state even for an otherwise generic liquidation candidate.

[FIX-94]:
- replaces first-match WSOL selection with deterministic selection among generic supported banks only:
  `ASSET_TAG_DEFAULT | ASSET_TAG_SOL`;
- requires an operational state supported by ProtocolSizer;
- sorts candidates by pubkey before selecting;
- logs selected cost bank, asset tag, oracle setup and operational state at startup.

Status: **IMPLEMENTED / VERIFY**.


### 2026-10-03 — FIX-94 Mainnet PASS / first post-sizing route evidence

Startup selected:

```text
sol_cost_bank: CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh
asset_tag=1
oracle_setup=PythPushOracle
operational_state=Operational
```

This confirms FIX-94 no longer selects an integration WSOL bank for SOL-denominated cost valuation.

Mainnet bootstrap-probe distribution after FIX-93/FIX-94:

```text
HEALTH: 61
DECISIONS: 61

LOSSES
55 Bankruptcy
4 Routing
1 MissingAta
1 Sizing

REASONS
55 bad debt with zero equity assets
2 Jupiter HTTP 429 Too Many Requests
1 ATA:MISSING_ASSET:So11111111111111111111111111111111111111112
1 ROUTING:INSUFFICIENT_ROUTE_OUTPUT
1 SIZING:NO_VALID_PROTOCOL_PAIR ... coeff_nonpositive=2
1 Jupiter HTTP 400 No routes found
```

Important progression:
- FIX-93 removed the false closeout premium-bound rejection.
- FIX-94 removed the accidental JupLend cost-valuation dependency.
- At least one Mainnet candidate now passes Exact Health + Protocol Sizing + route acquisition and reaches the Receivership builder far enough to fail specifically on the liquidator asset ATA.
- Therefore A6 / R2 (Shadow wallet ATA prerequisites) is now an observed blocker, not a hypothetical one.
- Jupiter provider errors are now also observed separately: rate-limit (429), no-route (400), and insufficient route output.

Status:
- FIX-94: **MAINNET PASS**.
- R2/A6: **CONFIRMED BLOCKER**.
- First Build/Sign/Simulation evidence remains OPEN.


### 2026-10-03 — FIX-95 in-TX ATA creation — IMPLEMENTED / VERIFY

Mainnet evidence after FIX-94 exposed a real `ATA:MISSING_ASSET:So111...` blocker after Health + Sizing + route acquisition.

Pinned marginfi introspection was verified:
- before `StartLiquidation`, only explicit refresh/init discriminators are allowed;
- the Associated Token Program is in the allowed top-level program set;
- therefore ATA creation must be **after StartLiquidation** and before the dependent Withdraw/Swap/Repay;
- `EndLiquidation` remains the final instruction.

[FIX-95]:
- adds `post_start_setup` to the shared receivership assembler;
- derives missing liquidator ATAs with the actual mint token program;
- inserts `create_associated_token_account_idempotent` after Start and before Withdraw;
- deduplicates identical asset/liability ATA creation;
- represents the setup as `Step::Other(ATA_PROGRAM)`, already covered by the existing planner allowlist;
- does not create a separate prerequisite transaction.

Economic caveat:
- ATA rent is not yet measured into `rent_and_token_usd_micro`;
- this is acceptable only for current Shadow technical qualification because FailedCostMatrix/economics remain UNKNOWN and LIVE is disabled.

Status: **IMPLEMENTED / VERIFY**.


### 2026-10-03 — FIX-96 recovery catch-up barrier — IMPLEMENTED / VERIFY

Observed failure during long-running Mainnet Shadow:
- WSS reconnect completed and StreamReady re-established;
- Snapshot A/B fetches intermittently failed on `liability_gpa` transport/response decoding;
- successful Snapshot A/B pairs could still reconcile dirty with 10–23 account mismatches and no missing local/remote accounts;
- mismatch pubkeys changed between attempts.

Interpretation:
- RPC snapshots and WSS account subscriptions both use `confirmed`;
- Snapshot B can therefore contain confirmed state whose corresponding WSS account notifications are still in flight / queued locally;
- immediate reconcile after a single drain can compare a newer Snapshot B against a locally lagging WSS-applied state and produce false mismatches.

[FIX-96]:
- RecoveryDriver optionally waits after Snapshot B until the observed WSS root reaches the maximum per-account context slot in Snapshot B;
- ingress is continuously drained while waiting;
- one final drain occurs after the root barrier;
- only then does byte-for-byte reconciliation run;
- production runner enables this barrier with a 30 s timeout;
- timeout remains fail-closed as ReconcileFailure;
- provider transport/decode errors remain normal retry failures.

Status: **IMPLEMENTED / VERIFY**.


### 2026-10-03 — FIX-96 compiler/workspace qualification — PASS

Qualified commit:
`e8b9143a8e5957ce33832929b11e4beceedab5e2` ([FIX-96c], including FIX-96a/b).

Commands executed:

```text
cargo build \
  --manifest-path project_0/native/Cargo.toml \
  --features "crate_health txv1"

cargo test \
  --manifest-path project_0/native/Cargo.toml \
  --workspace \
  --features "crate_health txv1"
```

Observed results:

```text
darkbot-common      19 passed; 0 failed
darkbot-project0    34 passed; 0 failed
darkbot-runner       3 passed; 0 failed
darkbot-solana      30 passed; 0 failed
doc-tests             0 failed
total                86 passed; 0 failed
```

The only compiler warning was the unused test helper `JupiterV2RouteProvider::with_base_url`; it is functionally irrelevant to FIX-96 and does not block qualification.

Decision: **FIX-96 compiler/workspace PASS**.

Remaining FIX-96 evidence is Mainnet-only: observe the production recovery path with the enabled 30 s root catch-up barrier and require

```text
StreamReady
→ Snapshot A/B
→ root_catchup ok
→ reconcile=clean
→ recovery: Ready, run_state=Ready
```

Any `root_catchup timeout`, `reconcile=dirty`, or exhausted recovery keeps FIX-96 Mainnet qualification open/fail-closed.


### 2026-10-03 — FIX-96 Mainnet recovery qualification — PASS

Qualified runtime commit:
`24e50ca` (code under qualification remains `e8b9143`; `24e50ca` adds only the prior compiler/workspace ledger entry).

Mainnet Shadow bootstrap evidence:

```text
[ws_rpc] session attempt=1 StreamReady
[recovery] attempt=1/3 snapshot=A ok slot=453025148 accounts=83266 missing_required=0
[recovery] attempt=1/3 snapshot=B ok slot=453025183 accounts=83266 missing_required=0
[recovery] attempt=1/3 root_catchup start target=453025227 current=Some(453025198)
[recovery] attempt=1/3 root_catchup ok target=453025227 root=453025227 elapsed_ms=7704
[recovery] attempt=1/3 reconcile=clean snapshot_slot=453025183
recovery: Ready, run_state=Ready
bootstrap_probe: queued=61 candidates from was_liquidatable
```

Snapshot details:

```text
Snapshot A: slot=453025148, liability_accounts=82678, banks=204,
required=384, optional=0, missing_required=0, total=83266, elapsed_ms=9530

Snapshot B: slot=453025183, liability_accounts=82678, banks=204,
required=384, optional=0, missing_required=0, total=83266, elapsed_ms=12257
```

Root catch-up details:

```text
target root: 453025227
initial observed root: 453025198
caught-up root: 453025227
elapsed_ms: 7704
result: reconcile=clean
```

Decision: **FIX-96 Mainnet PASS**.

The root catch-up hypothesis is validated for this Mainnet recovery/bootstrap instance: the runner did not reconcile until the observed root reached the fixed Snapshot-B account-context target, the final reconcile was clean, and the system transitioned fail-closed to `Ready` only after that clean reconcile.

FIX-96 is therefore complete: compiler/workspace qualification PASS + Mainnet recovery qualification PASS.

Next qualification target: continue the same Mainnet Shadow path through the queued generic candidates and obtain downstream evidence for `ShadowTechnicalReady` or `ShadowTechnicalSimFailed`, or capture the next concrete Build/Sign/Sim blocker.


### 2026-10-03 — FIX-97 R3 funding/market decoupling — compiler/workspace PASS

Qualified code commit:
`9797cb399603272eb188b58bb196288c9131e91c` — `[FIX-97] decouple inventory funding from market quote`.

Local workspace test execution completed successfully and returned to the shell with the final Cargo test result `ok`.

Qualified behavior in code:
- `NoSwapInventory` and `PreheldInventory` no longer require an ExactIn market quote for technical feasibility/building.
- `ZeroPrefundSameTx` alone requests the collateral→liability ExactIn route it actually executes.
- Unknown seized-collateral realizable value is represented explicitly as `None`; economics evaluates it as `Unknown`, never as zero and never as an oracle substitute.
- Route-provider failure cannot invalidate already-built inventory/JIT candidates.
- Unit coverage includes inventory feasibility without a market route and fail-closed economics for unknown realizable value.

Decision: **R3/FIX-97 compiler/workspace PASS**.

Remaining evidence: Mainnet Shadow must demonstrate that a generic candidate is no longer stopped solely by the old unconditional ExactIn quote and can progress to the next real Build/Sign/Simulation boundary.


### 2026-10-03 — R97 Mainnet census / R5 first Build→Sign→Simulation boundary

Run:
`r97_20261003T193304Z`

Bootstrap/recovery remained clean:
`StreamReady → Snapshot A/B → root_catchup ok → reconcile=clean → Ready`.

The 61 queued `was_liquidatable` bootstrap candidates resolved as:

```text
55  Bankruptcy
 2  StateRace
 2  Routing
 1  Sizing
 1  ShadowTechnicalSimFailed
61  total
```

The first generic candidate to cross the transaction boundary was:

```text
account=Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7
decision=ShadowTechnicalSimFailed
funding=ZERO_PREFUND_SAME_TX
tx_version=V0
serialized_bytes=1224
locked_accounts=35
sign_slot=453028817
simulation=UiTransactionError(AccountNotFound)
logs=[]
```

Interpretation:
- **R5 Build + Sign + call to simulation is now proven on Mainnet Shadow.**
- Simulation success is **not** proven; R5 remains open.
- The current concrete blocker is a Solana account-loading/debit failure before program execution.
- This run does not directly exercise an Inventory-funded candidate, so the R3/FIX-97 Mainnet inventory-path proof remains separate from its already-passed compiler/unit qualification.
- Jupiter route failures on other candidates remain legitimate ZERO_PREFUND-specific evidence and no longer imply that Inventory feasibility is coupled to ExactIn routing.

Next: **FIX-98** — fail fast at startup if the configured liquidator/fee-payer pubkey has no confirmed on-chain account, and print its lamport balance/owner when present. This distinguishes an unfunded/nonexistent payer from a deeper transaction-account problem before another full census.


### 2026-10-03 — FIX-98 payer preflight — Mainnet diagnosis PASS

Qualified code:
`143e554e0d82e6b9e7db810aadcd912f7017ceab` — `[FIX-98] fail fast on missing Shadow fee payer`.

Mainnet Shadow startup result:

```text
PAYER_ACCOUNT_NOT_FOUND:
liquidator=GqRxBgvH94QKFJQVyDv82fK6jxaKrcyjyjSPkN7ue3zd
has no confirmed on-chain account
```

Decision: **FIX-98 diagnostic PASS.**

This explains the prior R97 `UiTransactionError(AccountNotFound)` with empty logs: the configured signer/fee-payer itself did not yet exist as a funded Mainnet account, so the transaction failed during account loading/debit before any Marginfi/Jupiter instruction could execute.

No further code change is required for this blocker. The exact fee-payer pubkey must be funded with enough SOL for transaction fees and any rent-bearing accounts created by the Shadow transaction. Then rerun Mainnet Shadow and continue R5 from simulation.


### 2026-10-03 — R99 funded payer / recovery PASS

Run:
`r99_20261003T202439Z`

Mainnet payer preflight:

```text
pubkey=GqRxBgvH94QKFJQVyDv82fK6jxaKrcyjyjSPkN7ue3zd
lamports=10000000
owner=11111111111111111111111111111111
executable=false
```

Bootstrap/recovery evidence:

```text
StreamReady
Snapshot A slot=453040250 accounts=83268 missing_required=0
Snapshot B slot=453040279 accounts=83268 missing_required=0
root_catchup target=453040308 root=453040308
reconcile=clean
run_state=Ready
bootstrap_probe queued=61
```

Decision:
- **Funded fee-payer preflight PASS.**
- **Recovery/bootstrap remains PASS with the funded signer.**
- The prior R97 `AccountNotFound` payer blocker is removed.
- R5 simulation outcome is still pending from the 61 queued candidates; do not mark R5 simulation PASS until the JSONL decisions show a successful `ShadowTechnicalReady` / `ShadowReady` or a new concrete simulation failure.


### 2026-10-03 — R99 simulation / FIX-100 root cause

R99 candidate `Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7` progressed past account loading and into Marginfi. Simulation failed at instruction 2:

```text
Instruction: MarginfiAccountInitLiqRecord
Allocate: account 8DP6CQt14htc34gTB38hzYzbDKQ7rUBkx96oP99y84v7 already in use
InstructionError(2, Custom(0))
```

Root cause: the builder inferred liquidation-record existence from whether the PDA happened to be present in DarkBot RAM. That PDA was not part of the subscribed state, so an existing on-chain record was incorrectly treated as missing.

Protocol source confirms the correct zero-RPC signal: `MarginfiAccount.liquidation_record` is set to the record PDA when initialized and reset to `Pubkey::default()` when the record is closed.

FIX-100 changes the builder to use that canonical field:
- default link → include InitLiquidationRecord;
- exact derived PDA → do not initialize;
- any other non-default pubkey → fail closed with LIQUIDATION_RECORD_MISMATCH.

This keeps the decision on reconciled MarginfiAccount state and avoids adding a second RPC/source to the hot path.

### 2026-10-03 — CURRENT HANDOFF / Chatwechsel-Checkpoint — after [FIX-105]

**Purpose:** This is the authoritative short resume point when continuing Project_0 in a new chat. Read this section first. Do not infer readiness beyond what is explicitly marked PASS.

#### Where we are

Project_0 is **materially progressing** and is no longer a demo/planning-only path. The production Rust Long-Tail code has run against Mainnet Shadow through real state bootstrap/recovery, exact Marginfi health/sizing, transaction build, signing and an actual RPC simulation call.

What is still not proven: **no complete receivership simulation has succeeded yet, no liquidation transaction has been submitted by the runner, and no realized profit has been proven.**

Roadmap position:

```text
S1-LT / S2 / S3-CORE     substantially implemented and Mainnet exercised
P3 exact health           Mainnet exercised
P4A sizing/economics      implemented; unknown economics stays UNKNOWN
P5 receivership builder   implemented and repeatedly Mainnet exercised
R0/R1/R3                  compiler/workspace gates PASS
R5                        PARTIAL: Build + Sign + simulation call proven; simulation success OPEN
R4                        PASS: remote winner/reconciliation RPC isolated and R108-qualified
R6                        OPEN: integration refresh/withdraw completeness
R7                        OPEN: failed-cost/economics/edge qualification
P6-LT 72 h Shadow         NOT STARTED
P8-LT LIVE                BLOCKED intentionally
first realized net profit NOT PROVEN
```

#### Mainnet evidence already proven

1. **Recovery / state integrity**
   - filtered MarginfiAccount + Bank program streams;
   - subscription ACK barrier reaches StreamReady;
   - Snapshot A/B;
   - root catch-up barrier;
   - byte-for-byte reconcile clean;
   - `run_state=Ready`;
   - bootstrap probe repeatedly queues about 61 `was_liquidatable` candidates.
   - FIX-96 Mainnet PASS.

2. **Real transaction boundary reached**
   - R97 target candidate `Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7` reached Build + Sign + RPC simulation.
   - First failure was `AccountNotFound` before program execution.
   - FIX-98 proved the configured fee payer did not yet exist on Mainnet.
   - Dedicated Project_0 payer `GqRxBgvH94QKFJQVyDv82fK6jxaKrcyjyjSPkN7ue3zd` was funded and verified.

3. **Liquidation-record state bug fixed**
   - R99 progressed into Marginfi and failed because DarkBot tried to initialize an already-existing liquidation-record PDA.
   - FIX-100 now uses canonical `MarginfiAccount.liquidation_record` from reconciled state:
     default → init; exact expected PDA → reuse; other non-default pubkey → fail closed.
   - FIX-101 removed the stale liquidation-record resubscription reference after the clean repository rebuild.

4. **StartLiquidation / ATA ordering bug fixed**
   - R101 reached `StartLiquidation` and failed with Marginfi `StartNotFirst` / error 6086.
   - Root cause: ATA create/idempotent instructions were inside the receivership transaction. The pinned Marginfi validator scans all non-ComputeBudget top-level instructions and rejects the short ATA instruction encoding.
   - FIX-102 removed ATA creation from the receivership TX and requires prewarmed token accounts outside it.
   - ATA setup is no longer accepted as a receivership top-level instruction.

5. **Required token accounts are really present on Mainnet**
   - FIX-103 exposed the exact missing accounts.
   - Target `Trrq...` requires:
     - WSOL ATA `GT3wnkEdqeHtc9zB76GubCmt1m47ybTbpqtWtqGWWuG1`
     - USDC ATA `DoNVGHiCNWy1y5iR4FqV56z8gLbuQAf9nMREU1ydXjR3`
   - FIX-104 added guarded `p0-prewarm-atas`.
   - Dry-run Mainnet simulation PASS: `units_consumed=31330`.
   - Explicit send created both ATAs.
   - Each confirmed with 1,488,440 lamports rent.
   - Post-create payer balance: 7,018,120 lamports.
   - Follow-up: both `PREWARM_PRESENT`; `PREWARM_READY all requested ATAs already exist`.

#### Latest observed blocker

R104, after both ATAs were present, no longer failed with `MissingAta` and no longer showed `StartNotFirst`. The target candidate stopped before Build with:

```text
account=Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7
decision=Skip
loss=Routing
reason=no feasible execution candidates
```

That reason is too generic because the Jupiter adapter had several materially different fail-closed `Ok(None)` paths.

[FIX-105] commit:
`8386c49ab3efa8eacc12df5d804c7ddda311175a`

FIX-105 preserves fail-closed behavior but exposes the exact ZERO_PREFUND route rejection cause:
- local zero/identical-mint rejection;
- HTTP 404 / no executable Jupiter route;
- invalid input/output amounts;
- route incompatible with Marginfi top-level validation, including offending program IDs and instruction data lengths.

**FIX-105 is committed but not yet runtime-qualified.**

#### Exact next action

Build/test current `main`, then run **R105 Mainnet Shadow** and inspect the same `Trrq...` decision.

The next record must identify one concrete routing cause rather than generic `no feasible execution candidates`.

Decision after R105:
- true no-route → ZERO_PREFUND unavailable for that opportunity; continue with other funding classes/opportunities;
- incompatible Jupiter setup instruction → adapt route/prewarm handling without weakening Marginfi validation;
- valid route → continue Build → Sign → Simulate and capture the next exact protocol result;
- `ShadowTechnicalReady` → R5 technical simulation success proven for that candidate, while landing/PnL/LIVE still remain open.

#### Mandatory gate before 72 h Shadow

Even after the first successful simulation, **do not start the 72 h P6-LT qualification until R4 is fixed.**

Current R4 issue: the main ingress/detection loop still performs synchronous remote work for winner/reconciliation paths (`find_winner(...)` and pending submission reconciliation). These must move to bounded worker channels so remote RPC cannot stall the detection/ingress hot path.

After R4, continue R6/R7:
- integration refresh/withdraw planning for Kamino/Drift/JupLend;
- failed-attempt cost matrix from real evidence;
- rounding/repay-all/Jupiter account-size cases;
- threshold triggers b/c;
- 72 h final-code Shadow;
- controlled P8-LT LIVE gates only after all required qualification.

#### Non-claims

- Project_0 has not proven that it earns money.
- No successful Project_0 liquidation landing exists yet.
- No realized PnL exists.
- No p(win)/capture-rate is approved.
- ZERO_PREFUND remains a Shadow technical probe, not a globally qualified LIVE funding class.
- LIVE remains hard-disabled.

Key recent commits:

```text
9797cb3  [FIX-97]  decouple inventory funding from market quote
143e554  [FIX-98]  fail fast on missing Shadow fee payer
7d73ba1/
20ddcac  [FIX-100] canonical liquidation-record existence
87a7151  [FIX-101] remove stale liquidation-record resubscription filter
52423e8..
876fff0  [FIX-102] prewarmed ATA model / remove invalid in-TX ATA setup
6f091e6  [FIX-103] exact ATA prewarm diagnostics
1b233df/
b67f479  [FIX-104] guarded Mainnet ATA prewarm tool
8386c49  [FIX-105] expose exact Jupiter route rejection reason
```

### 2026-10-03 — R105 exact Jupiter ATA blocker / FIX-106 implemented

R105 target:
`Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7`

Observed after WSOL + USDC ATA prewarm:

```text
decision=Skip
loss=Routing
reason=ROUTING:exact-in So11111111111111111111111111111111111111112→EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v:
jupiter route incompatible with receivership top-level validator:
ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL(data_len=1)
```

Interpretation:
- FIX-105 successfully exposed the exact blocker.
- The required WSOL/USDC ATAs are already present in reconciled Mainnet state.
- Jupiter still emits an idempotent Associated Token setup instruction.
- Marginfi rejects that 1-byte top-level instruction inside the receivership transaction.

FIX-106:
`4cbf40945239c76e7988f27e15cbc34a9fc91f01`

Behavior:
- route planning receives the exact prewarmed ATA identities proven by reconciled state;
- Jupiter setup instructions are compared against a locally constructed canonical
  `create_associated_token_account_idempotent` instruction;
- the setup ix is removed only if program, data, account metas, wallet, mint, token program,
  and derived ATA all match an already-proven prewarmed ATA exactly;
- any other ATA instruction remains fail-closed;
- after filtering, any non-Jupiter top-level instruction still rejects the route;
- unit coverage added for exact-match removal and mismatched-taker rejection.

Status: **IMPLEMENTED / VERIFY**.
Next gate: workspace build/test, then R106 Mainnet Shadow. The required evidence is that the previous
`AToken...(data_len=1)` route blocker disappears without reintroducing `StartNotFirst`.

### 2026-10-03 — R106 deeper Mainnet simulation / FIX-107

R106 target:
`Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7`

Observed result:

```text
decision=ShadowTechnicalSimFailed
loss=Build
funding=ZERO_PREFUND_SAME_TX
tx_version=V0
serialized_bytes=1041
locked_accounts=32
```

The transaction now executes successfully through:
`StartLiquidation → LendingAccountWithdraw → Jupiter RouteV2/Swap → LendingAccountRepay`.

Failure occurs only at `EndLiquidation`:

```text
AnchorError: AccountOwnedByWrongProgram (3007)
Left:  rec2HHDDnjLfj4kE7VyEtFA1HPGQLK33259532cRyHp
Right: MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA
```

Source verification against pinned `0dotxyz/marginfi-v2@mrgn-0.1.11-rc1` found the exact mismatch:
- `StartLiquidation` uses complete bank+oracle observation metas with active banks writable;
- `EndLiquidation` uses `HealthPriceMode::Cached` and therefore expects **banks only**, with those bank accounts writable;
- pinned `p0-cli` explicitly builds:
  - `start_metas = load_observation_account_metas_with_bank_writable(..., true)`
  - `end_metas = load_observation_bank_only_metas(..., true)`
- DarkBot incorrectly reused the full bank+oracle observation list for End, so an oracle/receiver account was parsed as a Bank and failed Anchor ownership validation.

FIX-107:
`77b744fe3f21e15333eea64ee1457a37e03620c8`

Changes:
- distinct Start and End observation layouts in `ReceivershipParams`;
- Start gets full bank+oracle metas, with active banks writable;
- End gets active banks only, writable, descending by raw pubkey order;
- fixture updated to the same semantics.

Status: **IMPLEMENTED / VERIFY**.
Next gate: workspace build/test, then R107 Mainnet Shadow. Expected evidence is that EndLiquidation no longer fails with AccountOwnedByWrongProgram.



### 2026-10-03 — R107 full simulation PASS

Run: `r107_20261003T221630Z`

```text
account=Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7
decision=ShadowTechnicalReady
funding=ZERO_PREFUND_SAME_TX
tx_version=V0
serialized_bytes=963
locked_accounts=28
simulated_cu=256993
```

R5 technical simulation gate: **PASS**.

The full Mainnet Shadow path completed through StartLiquidation, withdraw, Jupiter swap, repay and EndLiquidation with a successful simulation.

Still open: failed-attempt economics, landed execution, realized PnL, R4 hot-path isolation, R6/R7 qualification, 72 h Shadow, and LIVE.

### 2026-10-03 — FIX-108 R4 hot-path isolation implemented

Commits:
- `3fa97ad` — remote worker module stub
- `3bc5bea` — bounded remote RPC worker implementation
- `2d2bf88` — expose worker module
- `9584b3b` — wire workers into `p0-longtail`

Architecture after FIX-108:
- `find_winner(...)` no longer executes in the ingress/state/detection loop;
- `try_reconcile(...)` no longer executes in the ingress/state/detection loop;
- winner lookup owns a dedicated `RpcClient` in thread `winner-rpc`;
- submission reconciliation owns a dedicated `RpcClient` in thread `reconcile-rpc`;
- both directions use bounded channels;
- queue-full returns ownership to the state loop for retry; no silent work-item drop;
- worker result queues may block only the worker, never the ingress/detection thread;
- worker disconnect is fail-closed at dispatch.

Static verification on current `main`:
```text
rpc_recon in main.rs:          absent
synchronous find_winner:       absent
synchronous try_reconcile:     absent
RemoteWorkers::start:          present
winner bounded dispatch:       present
reconcile bounded dispatch:    present
result drains on state thread: present
```

Status: **IMPLEMENTED / COMPILE+RUNTIME VERIFY**.

Next gate:
1. workspace build/test with `crate_health txv1`;
2. R108 Mainnet Shadow;
3. require normal StreamReady/Snapshot/Reconcile/READY and a successful technical candidate path without worker disconnect/panic;
4. then mark R4 PASS if runtime shows no regression.

### 2026-10-03 — R108 validates FIX-108 / R4 PASS

Run:
`r108_20261003T223313Z`

Target:
`Trrq3M5iixykEzCgCV4LuPvW4inagYTTNy3cmYrtnn7`

Result:

```text
decision=ShadowTechnicalReady
loss=null
reason=technical shadow probe: failed-attempt economics unknown
funding=ZERO_PREFUND_SAME_TX
tx_version=V0
serialized_bytes=963
locked_accounts=28
simulated_cu=257259
```

Bootstrap/recovery also remained clean:
- StreamReady reached;
- Snapshot A/B complete;
- required dependencies complete;
- two optional prewarmed ATAs present;
- root catch-up PASS;
- reconcile clean;
- run_state=Ready;
- bootstrap probe queued 61 candidates.

FIX-108 runtime result:
- no synchronous winner RPC remains in the ingress/state/detection loop;
- no synchronous submission-reconciliation RPC remains in that loop;
- the full technical receivership path still reaches `ShadowTechnicalReady` after worker isolation.

Gate decision: **R4 PASS**.

Next mandatory work is R6/R7 qualification:
1. integration refresh/withdraw completeness for Kamino/Drift/JupLend;
2. remaining rounding / repay-all / Jupiter account-size cases;
3. failed-attempt cost matrix and deterministic economics thresholds;
4. then 72 h final-code P6-LT Shadow.

### 2026-10-03 — R6 integration completeness started (FIX-109/FIX-110)

Implemented source-grounded R6 foundation:

```text
09ae89e  [FIX-109a] pin integration layout crates for R6
760566d  [FIX-109b] add deterministic integration refresh planner
129b01e  [FIX-109c] expose integration planner
789b1b4  [FIX-109d] subscribe canonical integration state
5cf73af  [FIX-109e] plan account-wide integration refresh prefix
2bf02d5  [FIX-109f] wire integration refresh prefix into receivership
d3e1c31  [FIX-110] add pinned integration withdraw builders
```

Pinned source: `0dotxyz/marginfi-v2@mrgn-0.1.11-rc1`.

Current implementation:
- Project_0 directly pins the canonical Kamino/Drift/JupLend layout/client crates from the same Marginfi tag.
- Integration state accounts `integration_acc_1/2/3` are now explicit snapshot/WSS dependencies.
- Deterministic pre-Start refresh planning exists for:
  - Kamino: `refresh_reserve` + `refresh_obligation`, using the Reserve's own oracle configuration;
  - Drift: `update_spot_market_cumulative_interest`;
  - JupLend: `update_rate`.
- The receivership builder now prepends the account-wide integration refresh prefix before `StartLiquidation`.
- Pinned Marginfi-specific Kamino/Drift/JupLend withdraw instruction builders are implemented.

Still deliberately OPEN:
- exact projected post-refresh state semantics (R6 A4/B2);
- enabling integration banks in ProtocolSizer;
- runner-side account discovery for all optional Drift reward / Kamino farm cases;
- integration-specific Mainnet simulation matrix.

No fake freshness was introduced. Integration sizing remains fail-closed until exact post-refresh projection is qualified.

Status: **R6 PARTIAL / compiler verification required before further wiring**.

### 2026-10-04 — FIX-111 exact post-refresh projection foundation

Compiler checkpoint before this step: workspace build/test returned to prompt without visible failure.

Commits:
- `8a816cb` — [FIX-111a] exact signed simulation can request selected post-execution accounts;
- `eaa7348` — [FIX-111b] read-only `ProjectedState` overlay;
- `d66ad9b` — [FIX-111c] expose projection module;
- `5152993` — [FIX-111d] dedicated bounded `projection-rpc` worker.

Design:
- canonical reconciled RAM state is never mutated by simulation;
- refresh projection uses the exact signed refresh transaction;
- only writable refresh accounts are requested back from `simulateTransaction`;
- returned account bytes overlay the canonical state for candidate-local Health/Sizing;
- projection RPC is isolated in its own bounded worker so R4's no-remote-RPC detection-loop invariant remains intact;
- empty/duplicate/zero-slot projections fail closed.

Still OPEN before R6 can pass:
- wire projection request/result lifecycle into the runner candidate state machine;
- reject stale projection results when the candidate account advanced meanwhile;
- evaluate Health/Sizing on `ProjectedState`;
- enable integration-bank sizing only on a qualified projected state;
- integration-specific Mainnet simulation matrix.

Status: **FIX-111a-d IMPLEMENTED / compiler verification required; R6 remains PARTIAL**.

### 2026-10-04 — CURRENT PROJECT STATUS / authoritative handoff after FIX-111e

Current proven gates:

```text
R4 hot-path isolation                    PASS  (R108 runtime-qualified)
R5 generic Mainnet receivership sim      PASS  (R107/R108 ShadowTechnicalReady)
R6 integration completeness              PARTIAL
R7 edge/economics qualification          OPEN
72 h P6-LT final-code Shadow              NOT STARTED
P8-LT LIVE                               DISABLED
realized PnL / landed liquidation         NOT PROVEN
```

Most important Mainnet evidence already proven:
- R107 and R108 both reached `ShadowTechnicalReady`;
- complete simulated receivership path:
  `StartLiquidation → Withdraw → Jupiter swap → Repay → EndLiquidation → simulation success`;
- R108 result:
  `ZERO_PREFUND_SAME_TX`, V0, 963 bytes, 28 locked accounts, 257259 simulated CU;
- payer, recovery, root catch-up, reconcile, prewarmed ATA and Jupiter/EndLiquidation blockers are cleared for the generic path.

R6 work currently present in `main`:
- FIX-109: pinned Kamino/Drift/JupLend layouts, deterministic refresh planners, canonical integration dependencies, pre-Start refresh prefix;
- FIX-110: pinned integration-specific withdraw builders;
- FIX-111a: signed simulation can request post-execution account state;
- FIX-111b/c: read-only `ProjectedState` overlay exposed;
- FIX-111d: bounded `projection-rpc` worker added so R4's no-remote-RPC detection-loop invariant remains intact;
- FIX-111e: compiler ownership bug fixed by cloning the RPC URL before moving it into the reconcile worker closure.

Latest local compiler result before FIX-111e:

```text
error[E0382]: borrow of moved value: rpc_url
remote_workers.rs
reconcile-rpc closure moved rpc_url before projection-rpc cloned it
```

FIX:
`6eefae63869754d3b1b3a4c22246319b7293eda8`
`[FIX-111e] fix RPC URL ownership across remote workers`

Status of FIX-111e: **IMPLEMENTED, compiler/test verification still required**.

Next exact steps:
1. pull `main`;
2. workspace build/test with `crate_health txv1`;
3. if green, continue FIX-111 runner lifecycle:
   - dispatch refresh projection work;
   - reject stale projection results if the account/state advanced;
   - evaluate candidate Health/Sizing against `ProjectedState`;
   - only then enable integration-bank sizing;
4. run integration-specific Mainnet Shadow matrix for Kamino/Drift/JupLend;
5. only after R6 PASS proceed to R7.

Do not claim:
- integration banks are production-qualified;
- R6 is complete;
- landed executions or realized profit exist;
- LIVE is enabled.

### 2026-10-04 — FIX-111f-k projection lifecycle wired

Compiler checkpoint before this batch: FIX-111e build/test returned to prompt without visible failure.

Commits:
- `5688c00` — [FIX-111f] allow projected `AccountSource` in receivership builder;
- `8197c58` — [FIX-111g] define coherent integration projection targets;
- `7fdcd51` — [FIX-111h] carry candidate identity/hash through projection worker;
- `f763995` — [FIX-111i] add projection lifecycle state to runner;
- `39def2b` — [FIX-111j] requeue successful projections without mutating canonical state;
- `789d8ab` — [FIX-111k] project integration state before candidate evaluation.

Lifecycle now implemented:
1. candidate with no integration refresh continues on the existing path;
2. candidate requiring Kamino/Drift/JupLend refresh is intercepted before Health/Sizing;
3. exact refresh prefix is built and signed from cached blockhash;
4. bounded `projection-rpc` worker simulates it and requests a coherent account set back;
5. returned candidate account bytes must match the exact canonical account hash/slot that requested projection;
6. stale/mismatched projection results are discarded;
7. valid results are one-shot and requeued as `integration_projection` without writing simulated bytes into `StateCore`;
8. `ProjectedState` overlays candidate/banks/observations/integration state for Health/Sizing/builder evaluation;
9. final receivership still contains the refresh prefix, so final Shadow simulation remains the authoritative transaction check.

Safety/non-claims:
- canonical reconciled StateCore is never mutated by simulation;
- projection work remains outside the detection loop behind a bounded worker;
- integration banks are **still not enabled in ProtocolSizer** yet;
- exact venue-specific collateral/share → underlying sizing and withdraw routing remain the next R6 task;
- no integration Mainnet matrix has passed yet.

Status: **FIX-111 lifecycle IMPLEMENTED / compiler verification required; R6 remains PARTIAL**.

### 2026-10-04 — FIX-112 integration amount/withdraw semantics implemented

Compiler checkpoint before this batch: FIX-111f-k build/test returned to prompt without visible failure.

Commits:
- `6373ff8` — [FIX-112a] separate integration bank units from underlying output;
- `e234850` — [FIX-112b] enable projected integration assets in protocol sizing;
- `073355a` — [FIX-112c] discover Drift reward markets from canonical user state;
- `164e756` — [FIX-112d] expose Drift reward market discovery;
- `abf447b` — [FIX-112e] route integration assets through exact withdraw builders.

Semantics now encoded from pinned Marginfi source:
- Kamino:
  - Marginfi bank units = collateral-token units;
  - Kamino withdraw instruction amount = collateral-token units;
  - post-withdraw Jupiter input = projected reserve `collateral_to_liquidity(...)` underlying amount.
- Drift:
  - Marginfi bank units = scaled-balance units;
  - withdraw instruction amount = underlying native token amount derived from projected cumulative deposit interest;
  - one-unit Drift round-up edge is reduced exactly as pinned program semantics require;
  - reward markets are discovered from the canonical Drift user account, capped at the two supported reward positions.
- JupLend:
  - Marginfi bank units = fToken shares;
  - underlying output = floor(shares × token_exchange_price / 1e12);
  - instruction amount = underlying;
  - burned shares are recomputed with ceil semantics and must not exceed the sized fToken units.

ProtocolSizer:
- integration banks are now eligible only on the asset/collateral side;
- liabilities remain generic-only because pinned Kamino/Drift/JupLend configs are non-borrowable;
- this path is intended to run only against the FIX-111 ProjectedState.

Receivership builder:
- selects generic/Kamino/Drift/JupLend withdraw instruction by asset bank tag;
- ZERO_PREFUND Jupiter ExactIn uses actual underlying output, not wrapped/scaled/share units;
- JupLend CPI accounts and Drift reward markets are derived from pinned protocol state;
- Kamino farm accounts are included when the reserve declares a collateral farm.

Status: **FIX-112a-e IMPLEMENTED / compiler verification required**.
R6 remains **PARTIAL** until compiler passes and the integration-specific Mainnet Shadow matrix proves Kamino/Drift/JupLend paths.

### 2026-10-04 — FIX-112f runner integration crate linkage

Local compiler after FIX-112a-e failed with four unresolved-crate errors in
`darkbot-runner/src/builder.rs` for `kamino_mocks`, `drift_mocks`, and
`juplend_mocks`.

Root cause:
- the integration crates were pinned only in `darkbot-project0`;
- FIX-112e also decodes those canonical layouts directly inside `darkbot-runner`;
- transitive dependencies are not directly nameable from the runner crate.

Fix:
- `darkbot-runner/Cargo.toml` now pins the same three crates at
  `0dotxyz/marginfi-v2@mrgn-0.1.11-rc1`;
- `crate_health` enables those optional direct dependencies.

Commit:
- `5685480` — [FIX-112f] link pinned integration layouts into runner.

Status: **FIX-112f IMPLEMENTED / compiler verification required**.
R6 remains PARTIAL until the full FIX-112 batch compiles and the integration Mainnet Shadow matrix passes.

### 2026-10-04 — FIX-113 integration Shadow matrix instrumentation

Compiler checkpoint before this step: FIX-112f build/test returned to prompt without visible failure.

Commits:
- `c376b38` — [FIX-113a] expose integration-kind census;
- `fb31425` — [FIX-113b] emit integration matrix census in shadow.

Purpose:
- identify which bootstrap-liquidatable accounts actually contain Kamino, Drift, and/or JupLend balances;
- attach `integration_kinds` to projection-pending and projection-ready records;
- print bootstrap counts:
  `bootstrap_probe_integrations: kamino=N drift=N juplend=N`.

This turns the R6 Mainnet qualification into an explicit matrix instead of guessing from account addresses.

Required R6 Mainnet evidence per venue:
1. at least one bootstrap/account-update candidate with that integration kind;
2. refresh projection reaches `status=ready`;
3. projected candidate proceeds through Health/Sizing;
4. venue-specific withdraw builder is selected;
5. final exact receivership simulation succeeds (`ShadowTechnicalReady`) or yields a venue-specific blocker that can be fixed deterministically.

Status: **FIX-113 instrumentation IMPLEMENTED / compiler + R109 runtime verification required**.

### 2026-10-04 — R109 integration census exposed a semantic gap; FIX-114 applied

R109 Mainnet Shadow:
- build/release PASS;
- StreamReady PASS;
- Snapshot A/B PASS;
- required dependencies=493, optional ATAs=2, missing_required=0;
- root catch-up PASS;
- reconcile clean;
- bootstrap probe queued 61 candidates;
- emitted:
  `bootstrap_probe_integrations: kamino=0 drift=0 juplend=0`.

Important interpretation:
- the zero counts only covered **asset_tag-based withdraw integrations**;
- this did **not** prove that no candidate depends on Kamino/Drift/JupLend state for pricing/freshness;
- historical Mainnet evidence already contained stale Kamino/JupLend oracle-wrapper failures, so the R109 census was incomplete by construction.

Pinned Marginfi semantics confirm two distinct concepts:
1. **withdraw integration** — encoded by asset tags Kamino/Drift/JupLend;
2. **pricing integration** — encoded by `OracleSetup` variants such as
   `KaminoPythPush`, `FixedKamino`, `DriftPythPull`, `FixedDrift`,
   `JuplendPythPull`, `FixedJuplend`, `KaminoMSOL/LST`, `JuplendMSOL/LST`, etc.

Fixes:
- `738e91b` — [FIX-114a] separate integration withdraw and pricing footprints;
- `9fa55aa` — [FIX-114b] drive refreshes from pricing integration semantics;
- `bcaf303` — [FIX-114c] distinguish withdraw and pricing integrations in shadow census.

Behavior after FIX-114:
- integration-specific withdraw math/builders still use asset-tag semantics;
- refresh/projection selection uses pricing integration semantics first, with asset-tag fallback;
- Shadow bootstrap now prints both:
  `withdraw_kamino/drift/juplend` and
  `pricing_kamino/drift/juplend`;
- projection records carry both `integration_kinds` and
  `pricing_integration_kinds`.

Status:
**R109 recovery/runtime baseline PASS; R6 integration matrix not yet qualified because R109's original census was incomplete. FIX-114 compiler/runtime verification required.**

### 2026-10-04 — R110 integration census

Run:
`r110_20261004T001002Z`

Bootstrap/recovery:
- release build PASS;
- StreamReady PASS;
- Snapshot A/B PASS;
- required dependencies=493;
- optional ATAs=2;
- missing_required=0;
- root catch-up PASS;
- reconcile clean;
- bootstrap probe queued=61.

Integration census:
```text
withdraw_kamino=0
withdraw_drift=0
withdraw_juplend=0
pricing_kamino=0
pricing_drift=0
pricing_juplend=0
```

Interpretation:
- the current 61 bootstrap-liquidatable accounts contain no Kamino/Drift/JupLend withdraw integrations and no Kamino/Drift/JupLend pricing integrations;
- therefore this R110 sample cannot qualify the integration-specific R6 matrix;
- this is not a failure of FIX-114; it means the qualification sample lacks integration candidates.

Next R6 action:
build a targeted Mainnet integration census/qualification path across Marginfi accounts/banks rather than waiting for the 61 bootstrap-liquidatable sample to contain an integration candidate.

### 2026-10-04 — FIX-115 / R111 targeted integration census

FIX-115:
- `88e3f61` — [FIX-115a] targeted integration qualification probe.

R111 targeted Mainnet census:

```text
integration_target_census:
  withdraw_kamino=195
  withdraw_drift=0
  withdraw_juplend=44
  pricing_kamino=195
  pricing_drift=0
  pricing_juplend=44
  scan_errors=0

integration_target_probe:
  queued_unique=16
  kamino_sample=8
  drift_sample=0
  juplend_sample=8
  limit_per_kind=8
```

Interpretation:
- the target-group liability population contains real Kamino and JupLend integration accounts;
- no Drift integration accounts were present in this current target-group liability snapshot;
- Kamino/JupLend withdraw and pricing populations are identical in this snapshot;
- the prior 61-account `was_liquidatable` bootstrap set was simply not representative of integration coverage;
- 16 deterministic qualification candidates were queued through the normal candidate/projection pipeline.

R6 evidence still required:
- inspect the queued R111 records for projection status and downstream decisions;
- at least one Kamino and one JupLend path must reach exact projected Health/Sizing and final receivership simulation;
- Drift cannot be Mainnet-qualified from this target-group snapshot because no Drift candidate exists; this must be documented as unavailable evidence rather than falsely marked PASS.

Status: **R111 census PASS; Kamino/JupLend qualification candidates found; downstream R111 records pending review.**

### 2026-10-04 — R111 downstream review: projection jobs queued, run stopped before results

R111 record review of the 16 targeted integration samples showed:

```text
8 Kamino-targeted samples
8 JupLend-targeted samples
0 Drift samples

all 16:
  decision=Skip
  loss=IntegrationRefresh
  reason="integration projection pending"
```

Some JupLend samples also contain Kamino balances, so the 16 selected accounts are not mutually exclusive by venue.

No `projection.status=ready|failed|stale` records were present in the reviewed output before the run was interrupted.

Interpretation:
- FIX-115 successfully discovered and dispatched all 16 targeted candidates;
- this evidence does **not** yet qualify or fail the projection worker;
- the Mainnet process was stopped before the asynchronous projection worker returned terminal results;
- R6 remains PARTIAL.

Next exact evidence required:
1. rerun the targeted probe;
2. keep it alive after `integration_target_probe: queued_unique=16`;
3. wait for 16 terminal projection result records (`ready|failed|stale`);
4. allow ready projections to re-enter the candidate pipeline and capture their subsequent Health/Sizing/Build/Simulation records.

### 2026-10-04 — R112 projection results / FIX-116

R112 targeted Mainnet projection results:
- 16 targeted integration candidates returned terminal projection records;
- 4 projections reached `status=ready`:
  - 3 Kamino;
  - 1 JupLend;
- the ready projections re-entered the pipeline and exact Health reported `liquidatable=false`, therefore each failed closed with `decision=Skip`, `reason="not liquidatable"`;
- the remaining projection attempts failed at the RPC response layer because the post-account request exceeded the provider's returned-account limit;
- one transport error record also exposed the configured provider URL in the local evidence log. No secret value is repeated here.

Root cause:
- FIX-111g requested a broad coherent post-account set (candidate + banks + observations + integration accounts);
- only writable accounts in the refresh prefix can actually change;
- `ProjectedState` already falls through to canonical RAM for every unchanged readonly account.

Fixes:
- `81de18f` — [FIX-116a] request only mutated post-refresh accounts;
- `d750b1c` — [FIX-116b] validate projection freshness against canonical candidate slot+hash instead of requiring the readonly candidate in the returned post-account set;
- `4ef7f8f` — [SEC-04] redact provider URLs from projection RPC errors.

R6 interpretation:
- Kamino projection path: **runtime proven to projection-ready**;
- JupLend projection path: **runtime proven to projection-ready**;
- downstream exact Health on the sampled ready accounts: **not liquidatable**, so no integration withdraw/final receivership simulation could be exercised from this sample;
- Drift remains unavailable in the current target-group liability population;
- R6 remains PARTIAL until a liquidatable Kamino/JupLend candidate reaches integration-specific sizing/withdraw and final exact receivership simulation.

Next gate:
1. compiler/test FIX-116;
2. rerun targeted probe;
3. confirm provider account-limit failures are gone;
4. then search the broader integration population for currently liquidatable Kamino/JupLend candidates rather than repeatedly probing arbitrary integration accounts.

### 2026-10-04 — FIX-117 targeted projection sweep completion telemetry

Compiler checkpoint before FIX-117: FIX-116 workspace build/test PASS.

Commit:
- `09c468b` — [FIX-117a] report targeted projection sweep completion.

Behavior:
- tracks the exact set of targeted integration probe accounts;
- counts terminal projection outcomes `ready|failed|stale`;
- emits one deterministic completion line only after every queued targeted projection has returned:

```text
integration_target_projection_complete: total=N ready=R failed=F stale=S
```

Purpose:
- removes manual guesswork about when it is safe to stop the targeted Mainnet qualification run;
- preserves the existing bounded projection worker and canonical-state immutability;
- does not alter trading eligibility, LIVE state, or profitability logic.

Next:
- compile/test;
- run targeted probe;
- wait for the completion line before stopping;
- inspect `integration_projection` records to see whether any projected Kamino/JupLend account is currently liquidatable and, if so, whether it reaches venue-specific sizing/withdraw/final simulation.

