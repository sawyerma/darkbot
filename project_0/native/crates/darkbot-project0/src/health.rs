//! P3: Health-Engine = offizielles `marginfi`-Crate (Apache-2.0). **Keine eigene Formel.**
//!
//! API aus dem gepinnten Marginfi-0.1.12-Source, BELEGT:
//! ```text
//! get_health_components(&MarginfiAccount, &MarginfiGroup, &[AccountInfo], RequirementType,
//!                       &mut Option<&mut HealthCache>, HealthPriceMode,
//!                       &mut Option<&mut PremiumScratch>) -> Result<(I80F48 assets, I80F48 liabs)>
//! HealthPriceMode::Client(Clock)   ← Off-Chain-Modus mit eigener Clock
//! RequirementType::{Initial, Maintenance, Equity}
//! ```
//! Eligibility: Maintenance-Health (assets − liabs) < 0 (check_pre_liquidation_condition…).
//! Seize-Grenze (liquidate_end.rs): seized_equity ≤ repaid_equity × (1 + max(FeeState.liquidation_max_fee, 0.05)),
//! Post-Health (Maintenance, Cached) ≥ Pre-Health; Equity-Assets < 5 USD → Closeout erlaubt.
//!
//! remaining_accounts: aktive Banks **absteigend** nach Pubkey, je Bank `bank` + Oracle-/Integrations-
//! Accounts gemäß `crate_impl::bank_observation_keys` (= p0-cli::bank_observation_keys).
//!
//! Status: Feature `crate_health` — compiler/unit-test qualified against pinned 0.1.12 source;
//! Mainnet runtime qualification remains separate evidence.

use crate::index::Side;
use solana_sdk::pubkey::Pubkey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthView {
    /// Maintenance: assets − liabs (I80F48-Bits: i128 mit 48 Fraktionsbits).
    pub maintenance_health: i128,
    pub maint_assets: i128,
    pub maint_liabs: i128,
    pub equity_assets: i128,
    pub equity_liabilities: i128,
    pub is_bad_debt: bool,
    pub closeout_allowed: bool,
    pub liquidatable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountThresholds {
    pub thresholds: Vec<(Pubkey, Side, u64)>,
}

#[derive(Debug)]
pub enum HealthError {
    Unsupported(String),
    Stale(String),
    Crate(String),
    MissingAccount(Pubkey),
    Unavailable,
}

impl std::fmt::Display for HealthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthError::Unsupported(s) => write!(f, "health unsupported (fail closed): {}", s),
            HealthError::Stale(s) => write!(f, "health stale: {}", s),
            HealthError::Crate(s) => write!(f, "health crate: {}", s),
            HealthError::MissingAccount(k) => write!(f, "health missing account in state: {}", k),
            HealthError::Unavailable => write!(f, "health engine unavailable (feature crate_health off)"),
        }
    }
}
impl std::error::Error for HealthError {}

pub trait HealthEngine {
    fn health(&self, account: &Pubkey, now_unix_s: i64, slot: u64) -> Result<HealthView, HealthError>;
    /// Protocol eligibility that must hold before receivership planning.
    ///
    /// Default keeps test/fake engines source-compatible. Production CrateHealth overrides this
    /// with the pinned marginfi circuit-breaker gate. [FIX-88]
    fn receivership_eligibility(&self, _account: &Pubkey, _now_unix_s: i64, _slot: u64) -> Result<(), HealthError> {
        Ok(())
    }
    /// Observation-Accounts in Programm-Reihenfolge (remaining_accounts für Start/End/Withdraw).
    fn observation_accounts(&self, account: &Pubkey) -> Result<Vec<Pubkey>, HealthError>;
    fn thresholds(&self, account: &Pubkey) -> Result<AccountThresholds, HealthError>;
    fn premium_decay_per_second(&self, account: &Pubkey) -> Result<u128, HealthError>;
}

/// Ohne Feature: fail closed.
pub struct NoHealth;

impl HealthEngine for NoHealth {
    fn health(&self, _: &Pubkey, _: i64, _: u64) -> Result<HealthView, HealthError> { Err(HealthError::Unavailable) }
    fn observation_accounts(&self, _: &Pubkey) -> Result<Vec<Pubkey>, HealthError> { Err(HealthError::Unavailable) }
    fn thresholds(&self, _: &Pubkey) -> Result<AccountThresholds, HealthError> { Err(HealthError::Unavailable) }
    fn premium_decay_per_second(&self, _: &Pubkey) -> Result<u128, HealthError> { Err(HealthError::Unavailable) }
}

/// Zugriff auf den RAM-State (S2); der Runner implementiert das über `StateCore::get`.
pub trait AccountSource {
    fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])>; // (owner, lamports, data)
}

#[cfg(feature = "crate_health")]
pub mod crate_impl {
    use super::*;
    use crate::protocol; // [FIX-50] used by closeout threshold below
    use marginfi::state::{
        bank::BankImpl,
        marginfi_account::get_health_components,
        price::OraclePriceFeedAdapter,
    };
    use marginfi_type_crate::constants::CIRCUIT_BREAKER_ENABLED;
    use marginfi_type_crate::types::{
        Bank, BankOperationalState, HealthCache, HealthPriceMode, MarginfiAccount, MarginfiGroup,
        OraclePriceType, RequirementType,
    };
    use bytemuck::Zeroable;
    use solana_sdk::account::Account;
    use solana_sdk::account_info::IntoAccountInfo;
    use solana_sdk::clock::Clock;

    /// [FIX-27] 1:1-Semantik von `p0-cli::bank_observation_keys`. Unbekannte Variante → fail closed.
    pub fn bank_observation_keys(bank: &Bank) -> Result<Vec<Pubkey>, HealthError> {
        use marginfi_type_crate::types::OracleSetup as O;
        let keys = &bank.config.oracle_keys;
        let mut out = match bank.config.oracle_setup {
            O::None | O::Fixed => vec![],
            O::FixedKamino | O::FixedDrift | O::FixedJuplend => vec![keys[1]],
            O::StakedWithPythPush => {
                let onramp = if keys[3] != Pubkey::default() {
                    keys[3]
                } else if bank.integration_acc_1 != Pubkey::default() {
                    marginfi_type_crate::pdas::derive_staked_onramp_from_vote(bank.integration_acc_1)
                } else {
                    Pubkey::default()
                };
                vec![keys[0], keys[1], keys[2], onramp]
            }
            O::PythLegacy | O::SwitchboardV2 | O::PythPushOracle | O::SwitchboardPull | O::Scope | O::PTFixed => vec![keys[0]],
            O::KaminoPythPush | O::KaminoSwitchboardPull | O::DriftPythPull | O::DriftSwitchboardPull
            | O::SolendPythPull | O::SolendSwitchboardPull | O::JuplendPythPull | O::JuplendSwitchboardPull
            | O::ScopeKamino | O::ScopeJuplend => vec![keys[0], keys[1]],
            O::PythMSOL | O::PythLST | O::PTPyth => vec![keys[0], keys[1]],
            O::KaminoMSOL | O::JuplendMSOL | O::KaminoLST | O::JuplendLST => vec![keys[0], keys[1], keys[2]],
            #[allow(unreachable_patterns)]
            other => return Err(HealthError::Unsupported(format!("OracleSetup {:?} (0.1.12-Typ? → P1B)", other))),
        };
        out.retain(|pk| *pk != Pubkey::default());
        Ok(out)
    }

    pub struct CrateHealth<'s, S: AccountSource + ?Sized> { // [FIX-53] supports &dyn AccountSource
        pub state: &'s S,
        pub group: Pubkey,
    }

    fn load<T: bytemuck::Pod>(data: &[u8]) -> Result<&T, HealthError> {
        let payload = data.get(8..).ok_or_else(|| HealthError::Crate("short account".into()))?;
        let n = std::mem::size_of::<T>();
        let slice = payload.get(..n).ok_or_else(|| HealthError::Crate(format!("payload {} < {}", payload.len(), n)))?;
        bytemuck::try_from_bytes::<T>(slice).map_err(|e| HealthError::Crate(format!("pod cast: {:?}", e)))
    }

    impl<'s, S: AccountSource + ?Sized> CrateHealth<'s, S> {
        fn raw(&self, k: &Pubkey) -> Result<(Pubkey, u64, &'s [u8]), HealthError> {
            self.state.account(k).ok_or(HealthError::MissingAccount(*k))
        }

        /// [FIX-27] Observation-Accounts **exakt wie `p0-cli/src/utils.rs::bank_observation_keys`**
        /// (mrgn-0.1.11-rc1): Banks absteigend nach Pubkey; je Bank `bank` + Keys aus
        /// `bank.config.oracle_keys` gemäß gepinntem `OracleSetup`-Enum — keine u8-Magie.
        pub fn observation(&self, acc: &MarginfiAccount) -> Result<Vec<Pubkey>, HealthError> {
            let mut banks: Vec<Pubkey> = acc.lending_account.balances.iter().filter(|b| b.is_active()).map(|b| b.bank_pk).collect();
            banks.sort_by(|a, b| b.cmp(a));
            let mut out = Vec::with_capacity(banks.len() * 4);
            for bk in banks {
                let (_, _, data) = self.raw(&bk)?;
                let bank: &Bank = load(data)?;
                out.push(bk);
                out.extend(bank_observation_keys(bank)?);
            }
            Ok(out)
        }

        fn components(
            &self,
            acc: &MarginfiAccount,
            group: &MarginfiGroup,
            obs: &[Pubkey],
            req: RequirementType,
            clock: &Clock,
            health_cache: &mut Option<&mut HealthCache>,
        ) -> Result<(i128, i128), HealthError> {
            let mut owned: Vec<(Pubkey, Account)> = Vec::with_capacity(obs.len());
            for k in obs {
                let (owner, lamports, data) = self.raw(k)?;
                owned.push((*k, Account { lamports, data: data.to_vec(), owner, executable: false, rent_epoch: 0 }));
            }
            let ais: Vec<_> = owned.iter_mut().map(|(k, a)| (&*k, a).into_account_info()).collect();
            let (assets, liabs) = get_health_components(
                acc,
                group,
                &ais,
                req,
                health_cache,
                HealthPriceMode::Client(clock.clone()),
                &mut None,
            )
            .map_err(|e| HealthError::Crate(e.to_string()))?;
            Ok((assets.to_bits(), liabs.to_bits()))
        }

        /// [P1B-0.1.12] Computes the same fresh maintenance HealthCache that
        /// StartLiquidation writes on-chain. EndLiquidation reads EMODE_BOOSTED from that cache
        /// when deciding whether tag-grown liquidation premium is allowed.
        pub fn health_with_emode(
            &self,
            account: &Pubkey,
            now_unix_s: i64,
            slot: u64,
        ) -> Result<(HealthView, bool), HealthError> {
            let (_, _, adata) = self.raw(account)?;
            let acc: &MarginfiAccount = load(adata)?;
            let (_, _, gdata) = self.raw(&self.group)?;
            let group: &MarginfiGroup = load(gdata)?;
            let obs = self.observation(acc)?;
            let clock = Clock {
                slot,
                epoch_start_timestamp: 0,
                epoch: 0,
                leader_schedule_epoch: 0,
                unix_timestamp: now_unix_s,
            };

            let mut maintenance_cache = HealthCache::zeroed();
            let (ma, ml) = self.components(
                acc,
                group,
                &obs,
                RequirementType::Maintenance,
                &clock,
                &mut Some(&mut maintenance_cache),
            )?;
            let (ea, el) = self.components(
                acc,
                group,
                &obs,
                RequirementType::Equity,
                &clock,
                &mut None,
            )?;
            let five_usd_bits: i128 =
                (protocol::LIQUIDATION_CLOSEOUT_DOLLAR_THRESHOLD as i128) << 48;
            let view = HealthView {
                maintenance_health: ma - ml,
                maint_assets: ma,
                maint_liabs: ml,
                equity_assets: ea,
                equity_liabilities: el,
                is_bad_debt: el > ea,
                closeout_allowed: ea < five_usd_bits,
                liquidatable: ma < ml && ml > 0,
            };
            Ok((view, maintenance_cache.is_emode_boosted()))
        }

        /// [FIX-88] Off-chain parity for the receivership circuit-breaker gate.
        ///
        /// The on-chain helpers use Clock::get(), which is not usable as an authoritative
        /// off-chain clock. The control flow below is intentionally identical to pinned
        /// marginfi `run_cb_price_gate` / `any_balance_bank_is_cb_halted`, while all
        /// actual breaker/oracle math remains in the official BankImpl / Oracle adapter.
        pub fn receivership_cb_gate(
            &self,
            account: &Pubkey,
            now_unix_s: i64,
            slot: u64,
        ) -> Result<(), HealthError> {
            let (_, _, adata) = self.raw(account)?;
            let acc: &MarginfiAccount = load(adata)?;
            let clock = Clock {
                slot,
                epoch_start_timestamp: 0,
                epoch: 0,
                leader_schedule_epoch: 0,
                unix_timestamp: now_unix_s,
            };

            for balance in acc.lending_account.balances.iter().filter(|b| b.is_active()) {
                let (_, _, bdata) = self.raw(&balance.bank_pk)?;
                let bank: &Bank = load(bdata)?;

                if bank.is_cb_halted(now_unix_s)
                    || bank.config.operational_state == BankOperationalState::CircuitBroken
                {
                    return Err(HealthError::Unsupported(format!(
                        "CB_HALTED bank={}",
                        balance.bank_pk
                    )));
                }

                if !bank.get_flag(CIRCUIT_BREAKER_ENABLED) {
                    continue;
                }

                let keys = bank_observation_keys(bank)?;
                let mut owned: Vec<(Pubkey, Account)> = Vec::with_capacity(keys.len());
                for key in keys {
                    let (owner, lamports, data) = self.raw(&key)?;
                    owned.push((
                        key,
                        Account {
                            lamports,
                            data: data.to_vec(),
                            owner,
                            executable: false,
                            rent_epoch: 0,
                        },
                    ));
                }
                let ais: Vec<_> = owned
                    .iter_mut()
                    .map(|(key, account)| (&*key, account).into_account_info())
                    .collect();

                let (_, cache_price) =
                    OraclePriceFeedAdapter::get_price_and_confidence_and_cache_of_type(
                        bank,
                        &ais,
                        &clock,
                        OraclePriceType::RealTime,
                    )
                    .map_err(|e| {
                        HealthError::Crate(format!(
                            "CB_ORACLE bank={} error={}",
                            balance.bank_pk, e
                        ))
                    })?;
                let observation = cache_price.cb_observation().map_err(|e| {
                    HealthError::Crate(format!(
                        "CB_OBSERVATION bank={} error={}",
                        balance.bank_pk, e
                    ))
                })?;
                bank.cb_price_gate(observation).map_err(|e| {
                    HealthError::Unsupported(format!(
                        "CB_PRICE_GATE bank={} error={}",
                        balance.bank_pk, e
                    ))
                })?;
            }
            Ok(())
        }
    }

    impl<'s, S: AccountSource + ?Sized> HealthEngine for CrateHealth<'s, S> {
        fn health(&self, account: &Pubkey, now_unix_s: i64, slot: u64) -> Result<HealthView, HealthError> {
            self.health_with_emode(account, now_unix_s, slot).map(|(view, _)| view)
        }

        fn receivership_eligibility(&self, account: &Pubkey, now_unix_s: i64, slot: u64) -> Result<(), HealthError> {
            self.receivership_cb_gate(account, now_unix_s, slot)
        }

        fn observation_accounts(&self, account: &Pubkey) -> Result<Vec<Pubkey>, HealthError> {
            let (_, _, adata) = self.raw(account)?;
            self.observation(load::<MarginfiAccount>(adata)?)
        }

        /// P3-02: Preisschwelle pro Bank — erst nach erstem Compile gegen Tag; bis dahin fail closed,
        /// damit der ThresholdIndex nie mit falschen Schwellen gefüllt wird.
        fn thresholds(&self, _account: &Pubkey) -> Result<AccountThresholds, HealthError> {
            Err(HealthError::Unsupported("P3-02 thresholds not implemented".into()))
        }

        /// [P1B-0.1.12] Premium decay is no longer a protocol-wide zero. Until the exact
        /// per-liability projection is wired from the 0.1.12 premium helpers, fail closed rather
        /// than reusing the obsolete 0.1.11 assumption.
        fn premium_decay_per_second(&self, _account: &Pubkey) -> Result<u128, HealthError> {
            Err(HealthError::Unsupported(
                "P1B 0.1.12 variable-borrow premium decay not yet qualified".into(),
            ))
        }
    }
}

/// [FIX-26] Externe Abhängigkeiten aller Banks im State (Oracle-PDAs/-Keys, Integrations-Accounts,
/// Mints, Extra-Keys) — damit Ingress/Snapshot sie **vor** der ersten Health-Berechnung halten.
/// Kein RPC hier; nur Ableitung aus dem RAM-State.
#[cfg(feature = "crate_health")]
pub fn dependency_keys<S: AccountSource + ?Sized>(state: &S, program: &Pubkey, banks: &[Pubkey]) -> Vec<Pubkey> {
    use crate::protocol;
    use marginfi_type_crate::types::Bank;
    let mut out: Vec<Pubkey> = Vec::new();
    for bk in banks {
        let Some((owner, _, data)) = state.account(bk) else { continue };
        if &owner != program || protocol::classify(data) != protocol::AccountType::Bank { continue; }
        let need = 8 + std::mem::size_of::<Bank>();
        if data.len() < need { continue; }
        let Ok(bank) = bytemuck::try_from_bytes::<Bank>(&data[8..need]) else { continue };
        out.push(bank.mint);
        // [FIX-27] dieselbe Quelle wie die Health-Observation (p0-cli-Semantik).
        if let Ok(keys) = crate_impl::bank_observation_keys(bank) { out.extend(keys); }

        // [FIX-109][R6] Integration state is part of canonical planning state, not an RPC
        // lookup performed later in the decision path. Keep all non-default integration accounts
        // subscribed/snapshotted even when a particular oracle setup does not list all of them.
        for key in [
            bank.integration_acc_1,
            bank.integration_acc_2,
            bank.integration_acc_3,
        ] {
            if key != Pubkey::default() {
                out.push(key);
            }
        }
    }
    out.sort_unstable_by_key(|k| k.to_bytes());
    out.dedup();
    out
}
