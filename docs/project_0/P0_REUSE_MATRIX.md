# P0-01 — Referenz-Studie eva01 / marginfi-v2 (Sitzung 4, Quellen als Zip gelesen)

Stand der Quellen: `marginfi-v2-main` (Programm-Crate **0.1.11**, Type-Crate 0.1.10, Apache-2.0), `eva01-main` (keine LICENSE → nur lesen).

## Oracle-Modell (P0-03) — BELEGT
Project 0 preist Pyth-Banks aus der Pyth-gesponserten `PriceUpdateV2` an `PDA([0, feed_id], pyth-push-oracle)` und lehnt sie ab, sobald sie älter als `bank.oracle_max_age` ist (eva01 `utils/pyth_cranker.rs`, Kopfkommentar). Preise kommen also **als On-Chain-Account-Updates** → kein `PriceSource` in S1 nötig. eva01 betreibt zusätzlich einen Cranker, der bei ausbleibendem Sponsor-Push selbst Hermes-VAA postet. Für Stufe 1: optionaler Pre-TX-Schritt, kein Hotpath-Bestandteil; Health auf stalem Oracle → `Stale` → SKIP.

## Allowlist / Introspection (P1A-02) — BELEGT aus `liquidate_start.rs`
- Erlaubte Top-Level-Programme in der Receivership-TX: ComputeBudget, marginfi, Kamino (`KLend2g3…`), Drift (`dRifty…`), JupLend (`jup3YeL8…`), Jupiter v6 (`JUP6Lkb…`), Titan (`T1TANp…`), Associated-Token.
- **Kein System-Programm → kein SOL-Transfer in der Receivership-TX → Jito-Tip in separater Bundle-TX.** Kein Token-Programm Top-Level.
- Erlaubt **vor** Start: Kamino RefreshReserve/RefreshReservesBatch/RefreshObligation, init_liquidation_record, Drift UpdateSpotMarketCumulativeInterest, JupLend UpdateRate.
- Marginfi-Ixs ausschließlich: start, end, init_liquidation_record, withdraw, repay, kamino/drift/juplend withdraw. `end` letzte Ix; `start` nicht letzte; kein CPI.

## Economics (P4A) — BELEGT aus `liquidate_end.rs` / `constants.rs`
- `seized = pre_assets_equity − post_assets_equity`, `repaid = pre_liabs_equity − post_liabs_equity` (EMA-Werte).
- `seized ≤ repaid × (1 + max(FeeState.liquidation_max_fee, 0.05))`, sonst `LiquidationPremiumTooHigh`.
- Post-Maintenance-Health ≥ Pre-Health (Cached-Preise), sonst `WorseHealthPostLiquidation`.
- Equity-Assets < 5 USD → Closeout ohne Grenze. `FeeState.liquidation_flat_sol_fee` (u32) per System-Transfer in `end`.
- Tagging/Premium-Kurve: **nicht in main (0.1.11)** → P1B.

## Health-API (P3) — BELEGT
`get_health_components(&MarginfiAccount, &MarginfiGroup, &[AccountInfo], RequirementType, &mut Option<&mut HealthCache>, HealthPriceMode) -> (I80F48, I80F48)`; `HealthPriceMode::Client(Clock)` ist der Off-Chain-Modus. Remaining-Accounts: Banks absteigend nach Pubkey; je Bank gemäß `get_remaining_accounts_per_bank` (Fixed 1; +Oracle 2; +Integration 3; Staked 5; MSOL/LST/PT 3–4). eva01 baut `AccountInfo` per `(pk, &mut Account).into_account_info()` aus seinem Cache — dieselbe Technik in `health::crate_impl`.

## Reuse-Matrix
| eva01-Modul | Entscheidung | DarkBot |
|---|---|---|
| `cache/*`, `cache_loader.rs` (gPA-Bootstrap, Account-Cache) | LERNEN | S2 StateCore + snapshot.rs (eigen) |
| `geyser.rs` | LERNEN | S1-RACE später |
| `liquidator.rs` (Health via Crate, Pair-Auswahl, max liquidatable amount) | LERNEN; Crate-Aufruf identisch (Protokoll-API) | P3 crate_impl; P4A-03 Amount-Sizing noch offen |
| `marginfi_ixs.rs`, `kamino_ixs.rs`, `drift_ixs.rs`, `juplend_ixs.rs` | NICHT KOPIEREN; Account-Reihenfolge aus Programm-Structs | P5-06 receivership.rs (aus Programm-Source gebaut) |
| `execution/inventory.rs` | LERNEN | P4A-05 Inventar-Modell |
| `utils/jito.rs` | LERNEN | S3-04 jito.rs |
| `utils/pyth_cranker.rs` | LERNEN; Pre-TX-Option | später (optional) |
| `rebalancer.rs` | — (Rebalance-Keeper, eigener Einnahmestrom) | Kandidat nach P9 |
| `wrappers/*` | LERNEN | — |
| `marginfi` Crate (Apache-2.0) | **REUSE** als Health-Engine | P3 |
| `marginfi-type-crate` | **REUSE** Typen/PDAs/Discriminators | P1A/P2/P5 |

## Offen nach P0
- P0-02: `solana-dex-superagg`-Lizenz, Jupiter/Titan-API-Key-Kosten (nicht im Zip).
- P0-04: Staging-Program-ID `stag8sTKds2h4KzjUw3zKTsxbqvT4XKHdaR9X9E6Rct` (id-crate; Feature-Zuordnung VERIFY).
- P4A-03: max liquidatable amount (eva01 `compute_max_liquidatable_amounts_with_banks`): eigene Implementierung über Crate-Werte + `calc_amount`.

---

# Merge-Entscheidung (Sitzung 5) — DIRECT_DEPENDENCY / REUSE_ADAPT / KEEP_DARKBOT / DROP

| Komponente | Entscheidung | Stand im Code |
|---|---|---|
| Project-0 Typen, PDAs, Discriminators | **DIRECT_DEPENDENCY** `marginfi-type-crate` | `protocol.rs` behält nur Allowlist/Layout-Konstanten aus Source; Discriminators dort redundant → beim ersten Build gegen Type-Crate ersetzen |
| Health (Maintenance/Equity, Observation-Reihenfolge) | **DIRECT_DEPENDENCY** `marginfi` (`get_health_components`, `HealthPriceMode::Client`) | `health::crate_impl` |
| Receivership-Instructions (init/start/withdraw/repay/end) | **DIRECT_DEPENDENCY** `marginfi::accounts/instruction` wie p0-cli | `receivership.rs` (manueller Builder **DROP**, ersetzt) |
| Kamino/Drift/JupLend Refresh/Withdraw | **DIRECT_DEPENDENCY** (`marginfi` + Mocks-IDLs) + Eva-Parity für Account-Order | offen (P5-02) |
| Liquidations-Sizing | Protokoll-Grenzen aus `liquidate_end.rs` (BELEGT) + Eva-Parity; Evas `PROFIT_SHARE=0.085` **nicht** normativ | `economics::max_seizable_for_repay`; P4A-03 offen |
| S0 Foundation | **KEEP_DARKBOT** | `darkbot-common` |
| S1-LT WSS/RPC | **KEEP_DARKBOT** | `ingress/ws_rpc.rs` |
| S1-RACE Geyser | **REUSE_ADAPT** (Eva `geyser.rs` als Referenz) + unser Event-Contract | später |
| S2 Canonical State (Snapshot→Delta→Reconcile, Fork/Root, OOB-Halt) | **KEEP_DARKBOT** | `state/*` |
| Reverse Dependencies Oracle→Bank→Account | **MERGE** (Eva-Logik als Referenz) + ThresholdIndex | `index.rs` |
| Premium-/Tag-Scheduler, `t_profitable` | **KEEP_DARKBOT** | `schedule.rs`, `economics.rs` |
| PreparedCache / Prebuild | **KEEP_DARKBOT** (+ Evas Temporary-LUT nur Fallback) | `prepared.rs` |
| P4A Economics | **KEEP_DARKBOT** | `economics.rs` |
| NO_SWAP | **KEEP_DARKBOT** | `planner.rs` |
| JIT-Liability-Buy (Eva `inventory.rs`) | **REUSE_ADAPT** als Fallback; Remote-Router nur nach Economics-Gate | offen (P5-04/05) |
| In-Receivership-Swap (Withdraw→Swap→Repay, kapitalfrei) | **KEEP_DARKBOT** (vom Protokoll erlaubt; v1 bei >1232 B) | Planner-Regel vorhanden; Ix-Bau offen |
| No-route Quarantine, Target-Backoff, Simulate-first, Bundle-Status/Chain-Confirm, sequenzieller Fallback nur Single-TX | **REUSE_ADAPT** (Verhalten, kein Code) | `execution.rs` |
| Jito-Tip in separater Bundle-TX | **REUSE_ADAPT** + BELEGT (kein System-Programm in Receivership-TX) | `protocol.rs` Doku; Bundle-Bau offen |
| Integration-State-Fetch mit gepatchten Account-Bytes (u64::MAX) | **DROP** — nicht in Canonical State; projected view getrennt | — |
| Pyth-Cranker | **REUSE_ADAPT optional** (nur bei gemessener Staleness) | später |
| Rebalancer | **REUSE_ADAPT später** (Inventar-Verwertung / zweiter Einnahmestrom) | nach P9 |
| P6 Shadow / Loss-Taxonomie, P8 Safety, Ledger | **KEEP_DARKBOT** | `runner`, `safety.rs` |
| Evas Cache-/Geyser-State-Layer (unbounded channel, Cache-dann-Geyser-Reihenfolge) | **DROP** | — |

Versionen (BELEGT, marginfi-v2 Workspace): `solana-sdk =3.0.0`, `solana-client =3.1.12`, `anchor-lang 1.0.2` — im Workspace gepinnt.


## Merge-Korrekturen vor Compiler-Runde

- `FundingClass` ist eine **Execution-Dimension, keine Präferenzliste**. Alle machbaren Klassen werden als eigene Kandidaten bewertet; P4A entscheidet nach deterministischem Netto-Profit, bei Gleichstand Kapital-Effizienz.
- `ZERO_PREFUND_SAME_TX` bleibt ein Project-0-Edge-Kandidat, aber nur nach realer TX-Feasibility/Simulation. Keine automatische Bevorzugung gegenüber `NO_SWAP_INVENTORY` oder `JIT_SHORTFALL`.
- Solana-3.x (marginfi) und modulare Solana-4.x-v1-Crates dürfen im selben Workspace koexistieren. Die Grenze ist ein versionsneutraler DarkBot-Plan/Instruction-Contract; es entsteht **kein zweiter State/Health/Economics/Execution-Core**.
- v0-ZERO_PREFUND wird mit den echten Jupiter `addressLookupTableAddresses` und geladenen LUT-Accounts gemessen. v1 hat keine LUTs und trägt Compute-/Priority-Konfiguration im `TransactionConfig`.
- Offizieller v1-Referenzpfad: `solana-foundation/transaction-v1-examples`; verwendete Referenz u. a. `solana-message 4.4.1`, `solana-transaction 4.1.6`. Finale Rust-API-Verifikation bleibt Compiler-Gate.
