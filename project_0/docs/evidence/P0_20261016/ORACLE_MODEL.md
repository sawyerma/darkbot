# P0-06 — Oracle model

Source: `0dotxyz/marginfi-v2` tag `mrgn-0.1.11-rc1`, commit `33c67987a661f59c6683a65485ebb4f0ef030b77` (the protocol pin of
`native/Cargo.toml`), read locally from the Cargo git checkout on 2026-10-16. Line references are to that commit.
The program version deployed on Mainnet *today* is a P1A question (current ProgramData parity), not a P0 one.

## How marginfi prices a bank

| Topic | Protocol behaviour (source) |
|---|---|
| Oracle setups | `OracleSetup` (`type-crate/src/types/bank.rs:363-391`), 27 values: `None`; deprecated `PythLegacy`, `SwitchboardV2`; `PythPushOracle`; `SwitchboardPull`; `StakedWithPythPush`; integration setups `KaminoPythPush`, `KaminoSwitchboardPull`, `DriftPythPull`, `DriftSwitchboardPull`, `SolendPythPull`, `SolendSwitchboardPull`, `JuplendPythPull`, `JuplendSwitchboardPull`; fixed prices `Fixed`, `FixedKamino`, `FixedDrift`, `FixedJuplend`, `PTFixed`; `Scope`; LST/MSOL/PT variants `PythMSOL`, `KaminoMSOL`, `JuplendMSOL`, `PythLST`, `KaminoLST`, `JuplendLST`, `PTPyth`. |
| Pyth push price account | `PriceUpdateV2` at `PDA([shard_id (u16 LE), feed_id], PYTH_PUSH_ORACLE_ID)` (`programs/marginfi/src/state/price.rs:2376-2385`). Prices therefore arrive as ordinary on-chain account updates. |
| Integration banks | price = underlying oracle × integration exchange rate read from the integration account (Kamino reserve, Drift spot market, JupLend lending, Solend reserve); the integration accounts are part of the bank's observation accounts. |
| Freshness | `bank.config.oracle_max_age` (seconds); `validate()` enforces `oracle_max_age >= ORACLE_MIN_AGE = 10` (`type-crate/src/constants.rs:46`, `bank_config.rs:176-179`); a stale price fails the price fetch. |
| Pyth confidence | interval = `conf × CONF_INTERVAL_MULTIPLE (2.12)`; the fetch fails if it exceeds `price × oracle_max_confidence / u32::MAX`, default 10 % when the bank sets 0 (`price.rs:2302-2330`, `constants.rs:60`). |
| Switchboard confidence | `price × oracle_max_confidence / u32::MAX`, clamped to `MAX_CONF_INTERVAL = 5 %`, 0 disables it (`price.rs:2057-2081`, `constants.rs:62`). |
| Bias in health | assets use the low band (`price − interval`), liabilities the high band (`price + interval`) (`marginfi_account.rs:514`, `547`, `1616`, `1685`). |
| Price type | Maintenance health uses the real-time price; Initial and Equity use the time-weighted (EMA) price (`type-crate/src/types/mod.rs:67-71`). Liquidation eligibility is a Maintenance question, so the real-time price and its confidence decide. |

## DarkBot decisions (already implemented; no own price formula)

| Question (PHASENPLAN P0.4) | Answer | Where |
|---|---|---|
| Which prices does DarkBot use? | exactly the protocol's: Health is computed by the marginfi crate (`get_health_components`, `HealthPriceMode::Client(Clock)`) over the bank's own observation accounts | `liquidation/src/health.rs` (`crate_impl::CrateHealth`), `bank_observation_keys` |
| Does DarkBot post price updates? | no. Oracle accounts are subscribed and snapshotted like any other account; a missing or stale update makes Health fail closed (`Stale` → SKIP) | `dependency_keys` (FIX-27, FIX-109), `state/*` |
| Pyth Receiver / Pythnet | used only as the on-chain account written by the sponsored Pyth push (and anyone posting Hermes VAAs); DarkBot neither fetches Hermes nor posts VAAs. Optional pre-transaction cranking (eva01 `pyth_cranker`) stays REUSE_ADAPT, only after measured staleness (see reuse matrix) | `docs/architecture/p0_reuse_matrix.md` |
| Scope | `OracleSetup::Scope` banks price through the Scope account named in the bank config; it is an observation account like any other | `bank_observation_keys` |
| Integration oracles | integration accounts (`integration_acc_1..3`) are always in the canonical state and the FE-01 observation scope (tested: `integration_accounts_of_target_banks_are_observed_in_the_same_scope`) | `health.rs:830-841`, `fe_qualification.rs` |
| Freshness/confidence rules | not re-implemented; the crate applies `oracle_max_age`, the confidence limits and the bias exactly as on chain, evaluated at the observed chain clock | `health::crate_impl` |
| Race relevance | price moves reach the bot as oracle account writes: the oracle path is the detection path. The reverse index oracle → bank → account (FIX-133) and threshold triggers (P2/P3-02) react to them; a competitor may post a fresher VAA inside its own transaction, which an observe-only bot cannot see beforehand (race risk, P7/S1-RACE) | `index.rs`, `state_owner/events.rs` |

## Open for later phases (not P0)

- Exact current oracle setups and ages of the banks that matter: P1A/P3 with current Mainnet state (FE-01 read-only observation, separate permission).
- Measured oracle staleness and whether a pre-transaction crank pays off: P3/P6 evidence.
