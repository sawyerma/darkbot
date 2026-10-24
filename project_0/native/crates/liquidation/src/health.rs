//! P3: Health-Engine = offizielles `marginfi`-Crate (Apache-2.0). **Keine eigene Formel.**
//!
//! API aus `programs/marginfi/src/state/marginfi_account.rs` (main, 0.1.11), BELEGT:
//! ```text
//! get_health_components(&MarginfiAccount, &MarginfiGroup, &[AccountInfo], RequirementType,
//!                       &mut Option<&mut HealthCache>, HealthPriceMode) -> Result<(I80F48 assets, I80F48 liabs)>
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
//! Status: Feature `crate_health` — UNTESTED / VERIFY (Compile gegen Tag; Pyth-PDA-Ableitung).

use crate::index::Side;
use solana_sdk::pubkey::Pubkey;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// (bank, side, maintenance risk-price threshold) where price is positive raw I80F48 bits.
    pub thresholds: Vec<(Pubkey, Side, u128)>,
}

/// C10: marker of a stale integration reserve in the error text (the record's reason), set only
/// by the typed check of the pinned error code (`crate_impl::integration_stale`). Q5: the text is
/// for readers; classification uses `sizing::SizingErrorKind::IntegrationStale`.
pub const INTEGRATION_STALE: &str = "INTEGRATION_STALE:";

#[derive(Debug)]
pub enum HealthError {
    Unsupported(String),
    /// V19: a strict pinned decoder refused the account layout (discriminator, length or
    /// version); fails closed like `Unsupported` and reads the same.
    UnknownLayout(String),
    Stale(String),
    /// C10: the pinned program reports a stale integration reserve (refresh prerequisite).
    IntegrationStale(String),
    Crate(String),
    MissingAccount(Pubkey),
    Unavailable,
}

impl std::fmt::Display for HealthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthError::Unsupported(s) | HealthError::UnknownLayout(s) => {
                write!(f, "health unsupported (fail closed): {}", s)
            }
            HealthError::Stale(s) => write!(f, "health stale: {}", s),
            HealthError::IntegrationStale(s) => {
                write!(f, "health integration reserve stale: {}", s)
            }
            HealthError::Crate(s) => write!(f, "health crate: {}", s),
            HealthError::MissingAccount(k) => write!(f, "health missing account in state: {}", k),
            HealthError::Unavailable => {
                write!(f, "health engine unavailable (feature crate_health off)")
            }
        }
    }
}
impl std::error::Error for HealthError {}

pub trait HealthEngine {
    fn health(
        &self,
        account: &Pubkey,
        now_unix_s: i64,
        slot: u64,
    ) -> Result<HealthView, HealthError>;
    /// Protocol eligibility that must hold before receivership planning.
    ///
    /// Default keeps test/fake engines source-compatible. Production CrateHealth overrides this
    /// with the pinned marginfi circuit-breaker gate. [FIX-88]
    fn receivership_eligibility(
        &self,
        _account: &Pubkey,
        _now_unix_s: i64,
        _slot: u64,
    ) -> Result<(), HealthError> {
        Ok(())
    }
    /// Observation-Accounts in Programm-Reihenfolge (remaining_accounts für Start/End/Withdraw).
    fn observation_accounts(&self, account: &Pubkey) -> Result<Vec<Pubkey>, HealthError>;
    fn thresholds(
        &self,
        account: &Pubkey,
        now_unix_s: i64,
        slot: u64,
    ) -> Result<AccountThresholds, HealthError>;
    /// 0.1.12: how fast pending premium lowers the maintenance health (I80F48 bits per second)
    /// at the given clock's prices.
    fn premium_decay_per_second(
        &self,
        account: &Pubkey,
        now_unix_s: i64,
        slot: u64,
    ) -> Result<u128, HealthError>;
}

/// Zugriff auf den RAM-State (S2); der Runner implementiert das über `StateCore::get`.
pub trait AccountSource {
    fn account(&self, key: &Pubkey) -> Option<(Pubkey, u64, &[u8])>; // (owner, lamports, data)
}

#[cfg(feature = "crate_health")]
pub mod crate_impl {
    use super::*;
    use crate::protocol; // [FIX-50] used by closeout threshold below

    /// C10: the pinned program's error codes of a stale integration reserve (Kamino, Solend,
    /// Drift, JupLend), classified by code, never by message text.
    pub fn integration_stale(e: &anchor_lang::error::Error) -> bool {
        use marginfi::errors::MarginfiError as M;
        let code = match e {
            anchor_lang::error::Error::AnchorError(a) => a.error_code_number,
            anchor_lang::error::Error::ProgramError(p) => match p.program_error {
                anchor_lang::prelude::ProgramError::Custom(n) => n,
                _ => return false,
            },
        };
        matches!(
            M::from(code),
            M::ReserveStale
                | M::SolendReserveStale
                | M::DriftSpotMarketStale
                | M::JuplendLendingStale
        )
    }

    /// C10: a crate error as sizing string; a stale integration reserve carries the stable marker.
    pub fn crate_error_text(context: &str, e: &anchor_lang::error::Error) -> String {
        if integration_stale(e) {
            format!("{}{context}: {e}", super::INTEGRATION_STALE)
        } else {
            format!("{context}: {e}")
        }
    }
    use fixed::types::I80F48;
    use marginfi::state::{
        bank::BankImpl,
        marginfi_account::{calc_value, get_health_components},
        price::{OraclePriceFeedAdapter, PriceAdapter},
    };
    use marginfi_type_crate::constants::{BANK_SAME_ASSET_EMODE_ELIGIBLE, CIRCUIT_BREAKER_ENABLED};
    use marginfi_type_crate::types::{
        compute_same_asset_emode_weight, reconcile_emode_configs, u32_to_basis, BalanceSide, Bank,
        BankOperationalState, HealthPriceMode, MarginfiAccount, MarginfiGroup, OraclePriceType,
        OracleSetup, PriceBias, ReconciledEmodeConfig, RequirementType, RiskTier,
    };
    use solana_sdk::account::Account;
    use solana_sdk::account_info::IntoAccountInfo;
    use solana_sdk::clock::Clock;
    use std::collections::HashMap;

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
                    marginfi_type_crate::pdas::derive_staked_onramp_from_vote(
                        bank.integration_acc_1,
                    )
                } else {
                    Pubkey::default()
                };
                vec![keys[0], keys[1], keys[2], onramp]
            }
            O::PythLegacy
            | O::SwitchboardV2
            | O::PythPushOracle
            | O::SwitchboardPull
            | O::Scope
            | O::PTFixed => vec![keys[0]],
            O::KaminoPythPush
            | O::KaminoSwitchboardPull
            | O::DriftPythPull
            | O::DriftSwitchboardPull
            | O::SolendPythPull
            | O::SolendSwitchboardPull
            | O::JuplendPythPull
            | O::JuplendSwitchboardPull => vec![keys[0], keys[1]],
            O::PythMSOL | O::PythLST | O::PTPyth => vec![keys[0], keys[1]],
            O::KaminoMSOL | O::JuplendMSOL | O::KaminoLST | O::JuplendLST => {
                vec![keys[0], keys[1], keys[2]]
            }
            #[allow(unreachable_patterns)]
            other => {
                return Err(HealthError::Unsupported(format!(
                    "OracleSetup {:?} (0.1.12-Typ? → P1B)",
                    other
                )))
            }
        };
        out.retain(|pk| *pk != Pubkey::default());
        Ok(out)
    }

    /// N8: the balances whose banks form the maintenance EMODE set — active and with a
    /// non-empty liability side, the condition of the pinned program's balance walk
    /// (`marginfi_account.rs`, `!balance.is_empty(BalanceSide::Liabilities)` over active
    /// balances). Health and sizing select through this one filter.
    pub fn emode_liability_balances(
        acc: &MarginfiAccount,
    ) -> impl Iterator<Item = &marginfi_type_crate::types::Balance> + '_ {
        acc.lending_account
            .balances
            .iter()
            .filter(|b| b.is_active() && !b.is_empty(BalanceSide::Liabilities))
    }

    /// B12: the maintenance EMODE configuration of an account's liability banks exactly as the
    /// pinned program reconciles it, including the group's same-asset EMODE. Health and sizing
    /// both use this one function, so their asset/liability weights cannot diverge.
    pub fn maintenance_emode_config_of(
        liability_banks: &[&Bank],
        group: &MarginfiGroup,
    ) -> ReconciledEmodeConfig {
        let mut configs = Vec::new();
        let leverage = u32_to_basis(group.same_asset_emode_maint_leverage);
        let same_asset_enabled = leverage > I80F48::ONE;

        let mut shared_mint = None;
        let mut shared_oracle_key = None;
        let mut shared_feed_family = None;
        let mut shared_fixed_price: Option<I80F48> = None;
        let mut lowest_liab_weight: Option<I80F48> = None;
        let mut same_asset_valid = true;

        for bank in liability_banks.iter().copied() {
            configs.push(bank.emode.emode_config);

            if !same_asset_enabled || !same_asset_valid {
                continue;
            }

            if !matches!(
                bank.config.oracle_setup,
                OracleSetup::PythPushOracle
                    | OracleSetup::SwitchboardPull
                    | OracleSetup::StakedWithPythPush
                    | OracleSetup::PythMSOL
                    | OracleSetup::PythLST
                    | OracleSetup::PTPyth
            ) {
                same_asset_valid = false;
                lowest_liab_weight = None;
                continue;
            }

            let Some(feed_family) = bank.config.oracle_setup.feed_family() else {
                same_asset_valid = false;
                lowest_liab_weight = None;
                continue;
            };
            if !bank.get_flag(BANK_SAME_ASSET_EMODE_ELIGIBLE)
                || bank.config.oracle_keys[0] == Pubkey::default()
            {
                same_asset_valid = false;
                lowest_liab_weight = None;
                continue;
            }

            let mint = bank.mint;
            let oracle_key = bank.config.oracle_keys[0];
            let fixed_price: I80F48 = bank.config.fixed_price.into();
            let liab_weight = bank
                .config
                .get_weight(RequirementType::Maintenance, BalanceSide::Liabilities);

            match shared_mint {
                Some(existing)
                    if existing != mint
                        || shared_oracle_key != Some(oracle_key)
                        || shared_feed_family != Some(feed_family)
                        || shared_fixed_price != Some(fixed_price) =>
                {
                    same_asset_valid = false;
                    lowest_liab_weight = None;
                }
                Some(_) => {
                    if lowest_liab_weight.is_none_or(|w| liab_weight < w) {
                        lowest_liab_weight = Some(liab_weight);
                    }
                }
                None => {
                    shared_mint = Some(mint);
                    shared_oracle_key = Some(oracle_key);
                    shared_feed_family = Some(feed_family);
                    shared_fixed_price = Some(fixed_price);
                    lowest_liab_weight = Some(liab_weight);
                }
            }
        }

        let mut reconciled = reconcile_emode_configs(configs, RequirementType::Maintenance);
        if same_asset_enabled && same_asset_valid {
            if let (
                Some(mint),
                Some(oracle_key),
                Some(feed_family),
                Some(fixed_price),
                Some(liab_weight),
            ) = (
                shared_mint,
                shared_oracle_key,
                shared_feed_family,
                shared_fixed_price,
                lowest_liab_weight,
            ) {
                reconciled.same_asset.mint = mint;
                reconciled.same_asset.oracle_key = oracle_key;
                reconciled.same_asset.feed_family = Some(feed_family);
                reconciled.same_asset.fixed_price = fixed_price;
                reconciled.same_asset.asset_weight =
                    compute_same_asset_emode_weight(leverage, liab_weight);
            }
        }
        reconciled
    }

    pub struct CrateHealth<'s, S: AccountSource + ?Sized> {
        // [FIX-53] supports &dyn AccountSource
        pub state: &'s S,
        pub group: Pubkey,
    }

    /// B2: strict pinned decoders (discriminator, exact length, alignment-independent); an
    /// unknown or resized layout is UNKNOWN_LAYOUT, never a prefix decode.
    fn load_account(data: &[u8]) -> Result<std::borrow::Cow<'_, MarginfiAccount>, HealthError> {
        crate::protocol::decode_marginfi_account(data)
            .map_err(|e| HealthError::UnknownLayout(e.to_string()))
    }
    fn load_group(data: &[u8]) -> Result<std::borrow::Cow<'_, MarginfiGroup>, HealthError> {
        crate::protocol::decode_group(data).map_err(|e| HealthError::UnknownLayout(e.to_string()))
    }

    /// [P1B-02] Strict Bank layout: a 0.1.12-resized bank is UNKNOWN_LAYOUT, never a prefix decode.
    fn load_bank(data: &[u8]) -> Result<std::borrow::Cow<'_, Bank>, HealthError> {
        crate::protocol::decode_bank(data).map_err(|e| HealthError::UnknownLayout(e.to_string()))
    }

    impl<'s, S: AccountSource + ?Sized> CrateHealth<'s, S> {
        fn raw(&self, k: &Pubkey) -> Result<(Pubkey, u64, &'s [u8]), HealthError> {
            self.state.account(k).ok_or(HealthError::MissingAccount(*k))
        }

        /// [FIX-27] Observation-Accounts **exakt wie `p0-cli/src/utils.rs::bank_observation_keys`**
        /// (mrgn-0.1.11-rc1): Banks absteigend nach Pubkey; je Bank `bank` + Keys aus
        /// `bank.config.oracle_keys` gemäß `OracleSetup`-Enum — keine eigene PDA-Ableitung, keine u8-Magie.
        pub fn observation(&self, acc: &MarginfiAccount) -> Result<Vec<Pubkey>, HealthError> {
            let mut banks: Vec<Pubkey> = acc
                .lending_account
                .balances
                .iter()
                .filter(|b| b.is_active())
                .map(|b| b.bank_pk)
                .collect();
            banks.sort_by(|a, b| b.cmp(a));
            let mut out = Vec::with_capacity(banks.len() * 4);
            for bk in banks {
                let (_, _, data) = self.raw(&bk)?;
                let bank = load_bank(data)?;
                let bank: &Bank = &bank;
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
        ) -> Result<(i128, i128), HealthError> {
            let [c] = self.components_of(acc, group, obs, [req], clock)?;
            Ok(c)
        }

        /// The health components of every requested requirement type over one copy of the
        /// observation accounts (the pinned crate reads them through `AccountInfo`).
        fn components_of<const N: usize>(
            &self,
            acc: &MarginfiAccount,
            group: &MarginfiGroup,
            obs: &[Pubkey],
            reqs: [RequirementType; N],
            clock: &Clock,
        ) -> Result<[(i128, i128); N], HealthError> {
            let mut owned: Vec<(Pubkey, Account)> = Vec::with_capacity(obs.len());
            for k in obs {
                let (owner, lamports, data) = self.raw(k)?;
                owned.push((
                    *k,
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
                .map(|(k, a)| (&*k, a).into_account_info())
                .collect();
            let mut out = [(0i128, 0i128); N];
            for (slot, req) in out.iter_mut().zip(reqs) {
                // 0.1.12: projected premium debt is part of the liabilities at the clock; the
                // premium scratch only collects snapshot updates (none off chain).
                let (assets, liabs) = get_health_components(
                    acc,
                    group,
                    &ais,
                    req,
                    &mut None,
                    HealthPriceMode::Client(clock.clone()),
                    &mut None,
                )
                .map_err(|e| {
                    if integration_stale(&e) {
                        HealthError::IntegrationStale(e.to_string())
                    } else {
                        HealthError::Crate(e.to_string())
                    }
                })?;
                *slot = (assets.to_bits(), liabs.to_bits());
            }
            Ok(out)
        }

        fn maintenance_emode_config(
            &self,
            acc: &MarginfiAccount,
            group: &MarginfiGroup,
        ) -> Result<ReconciledEmodeConfig, HealthError> {
            let mut banks = Vec::new();
            for balance in emode_liability_balances(acc) {
                let (_, _, bdata) = self.raw(&balance.bank_pk)?;
                banks.push(load_bank(bdata)?);
            }
            let refs: Vec<&Bank> = banks.iter().map(|b| &**b).collect();
            Ok(maintenance_emode_config_of(&refs, group))
        }

        fn bank_maintenance_prices(
            &self,
            bank: &Bank,
            clock: &Clock,
        ) -> Result<(I80F48, I80F48), HealthError> {
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
            // 0.1.12: PT emergency pricing is the risk admin's only (`false` for a liquidator).
            let adapter = OraclePriceFeedAdapter::try_from_bank(bank, &ais, clock, false)
                .map_err(|e| HealthError::Crate(format!("threshold oracle: {e}")))?;
            let price_type = RequirementType::Maintenance.get_oracle_price_type();
            let low = adapter
                .get_price_of_type(
                    price_type,
                    Some(PriceBias::Low),
                    bank.config.oracle_max_confidence,
                )
                .map_err(|e| HealthError::Crate(format!("threshold asset price: {e}")))?;
            let high = adapter
                .get_price_of_type(
                    price_type,
                    Some(PriceBias::High),
                    bank.config.oracle_max_confidence,
                )
                .map_err(|e| HealthError::Crate(format!("threshold liability price: {e}")))?;
            Ok((low, high))
        }

        /// Current maintenance risk prices for one Bank. These are exactly the two price axes
        /// consumed by Marginfi health: confidence-biased effective price (including integration
        /// multiplier), Low for collateral and High for liabilities.
        pub fn maintenance_risk_price_bits(
            &self,
            bank_key: &Pubkey,
            now_unix_s: i64,
            slot: u64,
        ) -> Result<(u128, u128), HealthError> {
            let (_, _, data) = self.raw(bank_key)?;
            let bank = load_bank(data)?;
            let bank: &Bank = &bank;
            let clock = Clock {
                slot,
                epoch_start_timestamp: 0,
                epoch: 0,
                leader_schedule_epoch: 0,
                unix_timestamp: now_unix_s,
            };
            let (low, high) = self.bank_maintenance_prices(bank, &clock)?;
            let low_bits = low.to_bits();
            let high_bits = high.to_bits();
            if low_bits < 0 || high_bits <= 0 {
                return Err(HealthError::Unsupported(format!(
                    "non-positive maintenance risk price bank={} low={} high={}",
                    bank_key, low, high
                )));
            }
            Ok((low_bits as u128, high_bits as u128))
        }

        pub(super) fn first_true_boundary<F>(
            current: u128,
            mut predicate: F,
        ) -> Result<Option<u128>, HealthError>
        where
            F: FnMut(u128) -> Result<bool, HealthError>,
        {
            let max = i128::MAX as u128;
            let mut high = current.max(1);
            if !predicate(high)? {
                loop {
                    if high == max {
                        return Ok(None);
                    }
                    high = if high > max / 2 { max } else { high * 2 };
                    if predicate(high)? {
                        break;
                    }
                }
            }
            let mut low = 0u128;
            while low < high {
                let mid = low + (high - low) / 2;
                if predicate(mid)? {
                    high = mid;
                } else {
                    low = mid.saturating_add(1);
                }
            }
            Ok(Some(low))
        }

        /// [FIX-133c] Source-backed per-bank maintenance thresholds.
        ///
        /// We keep Marginfi's own oracle adapter, balance amount conversion, weights and e-mode
        /// reconciliation. The indexed price is the exact confidence-biased effective Maintenance
        /// price used by the risk engine, not raw oracle spot. This makes integration multipliers
        /// and confidence changes first-class price dependencies without inventing a second formula.
        pub fn thresholds_at(
            &self,
            account: &Pubkey,
            now_unix_s: i64,
            slot: u64,
        ) -> Result<AccountThresholds, HealthError> {
            let (_, _, adata) = self.raw(account)?;
            let acc = load_account(adata)?;
            let acc: &MarginfiAccount = &acc;
            let (_, _, gdata) = self.raw(&self.group)?;
            let group = load_group(gdata)?;
            let group: &MarginfiGroup = &group;
            let clock = Clock {
                slot,
                epoch_start_timestamp: 0,
                epoch: 0,
                leader_schedule_epoch: 0,
                unix_timestamp: now_unix_s,
            };
            let reconciled = self.maintenance_emode_config(acc, group)?;

            let obs = self.observation(acc)?;
            let (official_assets_bits, official_liabs_bits) =
                self.components(acc, group, &obs, RequirementType::Maintenance, &clock)?;
            let official_assets = I80F48::from_bits(official_assets_bits);
            let official_liabs = I80F48::from_bits(official_liabs_bits);
            let official_health = official_assets
                .checked_sub(official_liabs)
                .ok_or_else(|| HealthError::Crate("threshold official health overflow".into()))?;

            #[derive(Clone)]
            struct Component {
                amount: I80F48,
                weight: I80F48,
                decimals: u8,
            }
            struct Position {
                bank: Pubkey,
                side: Side,
                current_price: I80F48,
                components: Vec<Component>,
                contribution: I80F48,
            }

            let mut positions: HashMap<(Pubkey, Side), Position> = HashMap::new();
            let mut bank_sides: HashMap<Pubkey, (bool, bool)> = HashMap::new();
            let mut derived_assets = I80F48::ZERO;
            let mut derived_liabs = I80F48::ZERO;

            for balance in acc
                .lending_account
                .balances
                .iter()
                .filter(|b| b.is_active())
            {
                let (_, _, bdata) = self.raw(&balance.bank_pk)?;
                let bank = load_bank(bdata)?;
                let bank: &Bank = &bank;
                let Some(balance_side) = balance.get_side() else {
                    continue;
                };
                let (low, high) = self.bank_maintenance_prices(bank, &clock)?;

                let (side, price, amount, weight) = match balance_side {
                    BalanceSide::Assets => {
                        if bank.config.risk_tier != RiskTier::Collateral {
                            continue;
                        }
                        (
                            Side::Asset,
                            low,
                            bank.get_asset_amount(balance.asset_shares.into())
                                .map_err(|e| {
                                    HealthError::Crate(format!("threshold asset amount: {e}"))
                                })?,
                            bank.get_asset_weight(RequirementType::Maintenance, &reconciled),
                        )
                    }
                    // 0.1.12: projected premium is a liability that grows with time alone: such an
                    // account has no price threshold, it is evaluated on every price update.
                    BalanceSide::Liabilities
                        if bank.flags & marginfi_type_crate::constants::PREMIUM_ACTIVE != 0
                            && (balance.premium_rate_snapshot != 0
                                || I80F48::from(balance.premium_outstanding) != I80F48::ZERO) =>
                    {
                        return Err(HealthError::Unsupported(format!(
                            "threshold fallback required: premium accrues on bank={}",
                            balance.bank_pk
                        )));
                    }
                    BalanceSide::Liabilities => (
                        Side::Liability,
                        high,
                        bank.get_liability_amount(balance.liability_shares.into())
                            .map_err(|e| {
                                HealthError::Crate(format!("threshold liability amount: {e}"))
                            })?,
                        bank.config
                            .get_weight(RequirementType::Maintenance, BalanceSide::Liabilities),
                    ),
                };
                if amount <= I80F48::ZERO || price <= I80F48::ZERO {
                    continue;
                }

                let contribution =
                    calc_value(amount, price, bank.get_balance_decimals(), Some(weight))
                        .map_err(|e| HealthError::Crate(format!("threshold contribution: {e}")))?;

                match side {
                    Side::Asset => {
                        derived_assets =
                            derived_assets.checked_add(contribution).ok_or_else(|| {
                                HealthError::Crate("threshold asset sum overflow".into())
                            })?;
                        bank_sides.entry(balance.bank_pk).or_default().0 = true;
                    }
                    Side::Liability => {
                        derived_liabs =
                            derived_liabs.checked_add(contribution).ok_or_else(|| {
                                HealthError::Crate("threshold liability sum overflow".into())
                            })?;
                        bank_sides.entry(balance.bank_pk).or_default().1 = true;
                    }
                }

                let entry = positions
                    .entry((balance.bank_pk, side))
                    .or_insert(Position {
                        bank: balance.bank_pk,
                        side,
                        current_price: price,
                        components: Vec::new(),
                        contribution: I80F48::ZERO,
                    });
                if entry.current_price != price {
                    return Err(HealthError::Crate(format!(
                        "threshold same-bank price mismatch bank={} side={:?}",
                        balance.bank_pk, side
                    )));
                }
                entry.components.push(Component {
                    amount,
                    weight,
                    decimals: bank.get_balance_decimals(),
                });
                entry.contribution =
                    entry
                        .contribution
                        .checked_add(contribution)
                        .ok_or_else(|| {
                            HealthError::Crate("threshold grouped contribution overflow".into())
                        })?;
            }

            if let Some((bank, _)) = bank_sides.iter().find(|(_, (asset, liab))| *asset && *liab) {
                return Err(HealthError::Unsupported(format!(
                    "threshold fallback required: bank={} appears on both asset and liability sides",
                    bank
                )));
            }

            let positions: Vec<Position> = positions.into_values().collect();

            if derived_assets != official_assets || derived_liabs != official_liabs {
                return Err(HealthError::Crate(format!(
                    "threshold parity mismatch official=({}, {}) derived=({}, {})",
                    official_assets, official_liabs, derived_assets, derived_liabs
                )));
            }

            let mut thresholds = Vec::with_capacity(positions.len());
            for position in positions {
                let signed = match position.side {
                    Side::Asset => position.contribution,
                    Side::Liability => -position.contribution,
                };
                let other_health = official_health
                    .checked_sub(signed)
                    .ok_or_else(|| HealthError::Crate("threshold other-health overflow".into()))?;
                let current_bits = position.current_price.to_bits();
                if current_bits <= 0 {
                    continue;
                }

                let threshold = match position.side {
                    Side::Asset => {
                        let at_zero = other_health;
                        // If collateral can fall to zero and health remains non-negative, this
                        // Bank can never be the price trigger for liquidation.
                        if at_zero >= I80F48::ZERO {
                            None
                        } else {
                            Self::first_true_boundary(current_bits as u128, |bits| {
                                let price = I80F48::from_bits(bits as i128);
                                let mut value = I80F48::ZERO;
                                for component in &position.components {
                                    let part = match calc_value(
                                        component.amount,
                                        price,
                                        component.decimals,
                                        Some(component.weight),
                                    ) {
                                        Ok(v) => v,
                                        // At an unrepresentably large positive price the monotone
                                        // asset contribution is necessarily beyond the crossing.
                                        Err(_) => return Ok(true),
                                    };
                                    value = match value.checked_add(part) {
                                        Some(v) => v,
                                        None => return Ok(true),
                                    };
                                }
                                Ok(other_health
                                    .checked_add(value)
                                    .map(|h| h >= I80F48::ZERO)
                                    .unwrap_or(true))
                            })?
                        }
                    }
                    Side::Liability => {
                        if other_health <= I80F48::ZERO {
                            Some(0)
                        } else {
                            Self::first_true_boundary(current_bits as u128, |bits| {
                                let price = I80F48::from_bits(bits as i128);
                                let mut value = I80F48::ZERO;
                                for component in &position.components {
                                    let part = match calc_value(
                                        component.amount,
                                        price,
                                        component.decimals,
                                        Some(component.weight),
                                    ) {
                                        Ok(v) => v,
                                        Err(_) => return Ok(true),
                                    };
                                    value = match value.checked_add(part) {
                                        Some(v) => v,
                                        None => return Ok(true),
                                    };
                                }
                                Ok(other_health
                                    .checked_sub(value)
                                    .map(|h| h <= I80F48::ZERO)
                                    .unwrap_or(true))
                            })?
                        }
                    }
                };
                if let Some(price) = threshold {
                    thresholds.push((position.bank, position.side, price));
                }
            }

            Ok(AccountThresholds { thresholds })
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
            let acc = load_account(adata)?;
            let acc: &MarginfiAccount = &acc;
            let clock = Clock {
                slot,
                epoch_start_timestamp: 0,
                epoch: 0,
                leader_schedule_epoch: 0,
                unix_timestamp: now_unix_s,
            };

            for balance in acc
                .lending_account
                .balances
                .iter()
                .filter(|b| b.is_active())
            {
                let (_, _, bdata) = self.raw(&balance.bank_pk)?;
                let bank = load_bank(bdata)?;
                let bank: &Bank = &bank;

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
                        false,
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
        fn health(
            &self,
            account: &Pubkey,
            now_unix_s: i64,
            slot: u64,
        ) -> Result<HealthView, HealthError> {
            let (_, _, adata) = self.raw(account)?;
            let acc = load_account(adata)?;
            let acc: &MarginfiAccount = &acc;
            let (_, _, gdata) = self.raw(&self.group)?;
            let group = load_group(gdata)?;
            let group: &MarginfiGroup = &group;
            let obs = self.observation(acc)?;
            let clock = Clock {
                slot,
                epoch_start_timestamp: 0,
                epoch: 0,
                leader_schedule_epoch: 0,
                unix_timestamp: now_unix_s,
            };
            // B7: one copy of the observation accounts for both requirement types.
            let [(ma, ml), (ea, el)] = self.components_of(
                acc,
                group,
                &obs,
                [RequirementType::Maintenance, RequirementType::Equity],
                &clock,
            )?;
            let five_usd_bits: i128 =
                (protocol::LIQUIDATION_CLOSEOUT_DOLLAR_THRESHOLD as i128) << 48;
            Ok(HealthView {
                maintenance_health: ma - ml,
                maint_assets: ma,
                maint_liabs: ml,
                equity_assets: ea,
                equity_liabilities: el,
                is_bad_debt: el > ea,
                closeout_allowed: ea < five_usd_bits,
                liquidatable: ma < ml && ml > 0,
            })
        }

        fn receivership_eligibility(
            &self,
            account: &Pubkey,
            now_unix_s: i64,
            slot: u64,
        ) -> Result<(), HealthError> {
            self.receivership_cb_gate(account, now_unix_s, slot)
        }

        fn observation_accounts(&self, account: &Pubkey) -> Result<Vec<Pubkey>, HealthError> {
            let (_, _, adata) = self.raw(account)?;
            let acc = load_account(adata)?;
            self.observation(&acc)
        }

        fn thresholds(
            &self,
            account: &Pubkey,
            now_unix_s: i64,
            slot: u64,
        ) -> Result<AccountThresholds, HealthError> {
            self.thresholds_at(account, now_unix_s, slot)
        }

        /// 0.1.12: every liability on a premium-active bank accrues `principal × rate / year`
        /// of premium per second (the crate's simple-interest projection), a liability valued at
        /// the bank's high maintenance price and liability weight like the principal; zero
        /// without one. An unreadable bank or price is an error, never zero.
        fn premium_decay_per_second(
            &self,
            account: &Pubkey,
            now_unix_s: i64,
            slot: u64,
        ) -> Result<u128, HealthError> {
            use marginfi_type_crate::constants::{PREMIUM_ACTIVE, SECONDS_PER_YEAR};
            let (_, _, adata) = self.raw(account)?;
            let acc = load_account(adata)?;
            let clock = Clock {
                slot,
                epoch_start_timestamp: 0,
                epoch: 0,
                leader_schedule_epoch: 0,
                unix_timestamp: now_unix_s,
            };
            let overflow = || HealthError::Crate("premium decay overflow".into());
            let mut decay = I80F48::ZERO;
            for balance in acc
                .lending_account
                .balances
                .iter()
                .filter(|b| b.is_active() && b.premium_rate_snapshot != 0)
            {
                if !matches!(balance.get_side(), Some(BalanceSide::Liabilities)) {
                    continue;
                }
                let (_, _, bdata) = self.raw(&balance.bank_pk)?;
                let bank = load_bank(bdata)?;
                if bank.flags & PREMIUM_ACTIVE == 0 {
                    continue;
                }
                let principal = bank
                    .get_liability_amount(balance.liability_shares.into())
                    .map_err(|e| HealthError::Crate(format!("premium decay principal: {e}")))?;
                let per_second = principal
                    .checked_mul(marginfi_type_crate::types::u32_to_milli(
                        balance.premium_rate_snapshot,
                    ))
                    .and_then(|v| v.checked_div(SECONDS_PER_YEAR))
                    .ok_or_else(overflow)?;
                let (_, high) = self.bank_maintenance_prices(&bank, &clock)?;
                let weight = bank
                    .config
                    .get_weight(RequirementType::Maintenance, BalanceSide::Liabilities);
                let value = calc_value(per_second, high, bank.get_balance_decimals(), Some(weight))
                    .map_err(|e| HealthError::Crate(format!("premium decay value: {e}")))?;
                decay = decay.checked_add(value).ok_or_else(overflow)?;
            }
            Ok(u128::try_from(decay.to_bits()).unwrap_or(0))
        }
    }
}

#[cfg(all(test, feature = "crate_health"))]
mod bank_layout_tests {
    use super::crate_impl::CrateHealth;
    use super::{AccountSource, HealthEngine, HealthError};
    use crate::protocol;
    use marginfi_type_crate::types::MarginfiAccount;
    use solana_sdk::pubkey::Pubkey;
    use std::collections::HashMap;

    /// u64-backed accounts keep payloads 8-byte aligned like real account data.
    struct Source(HashMap<Pubkey, (Pubkey, Vec<u64>, usize)>);
    impl AccountSource for Source {
        fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            self.0
                .get(k)
                .map(|(o, w, n)| (*o, 1, &bytemuck::cast_slice::<u64, u8>(w)[..*n]))
        }
    }
    fn aligned(bytes: &[u8]) -> (Vec<u64>, usize) {
        let mut w = vec![0u64; bytes.len().div_ceil(8)];
        bytemuck::cast_slice_mut::<u64, u8>(&mut w)[..bytes.len()].copy_from_slice(bytes);
        (w, bytes.len())
    }

    /// P1B-02/03: a non-resized 0.1.11 bank must not reach the pinned 0.1.12 Health crate (the
    /// program fails every transaction touching it as well).
    #[test]
    fn unresized_bank_fails_health_observation_closed() {
        let program = Pubkey::new_unique();
        let (account, bank) = (Pubkey::new_unique(), Pubkey::new_unique());
        let mut acc: MarginfiAccount = bytemuck::Zeroable::zeroed();
        acc.lending_account.balances[0].active = 1;
        acc.lending_account.balances[0].bank_pk = bank;
        let mut adata = protocol::DISC_MARGINFI_ACCOUNT.to_vec();
        adata.extend_from_slice(bytemuck::bytes_of(&acc));
        let (aw, an) = aligned(&adata);
        let mut bdata = protocol::DISC_BANK.to_vec();
        bdata.resize(8 + protocol::BANK_PAYLOAD_LEN_0_1_11, 0);
        let (bw, bn) = aligned(&bdata);
        let state = Source(HashMap::from([
            (account, (program, aw, an)),
            (bank, (program, bw, bn)),
        ]));
        let engine = CrateHealth {
            state: &state,
            group: Pubkey::new_unique(),
        };
        match engine.observation_accounts(&account) {
            Err(HealthError::UnknownLayout(e)) => {
                assert!(e.starts_with("UNKNOWN_LAYOUT: bank layout 0.1.11"))
            }
            other => panic!("non-resized bank must fail closed, got {other:?}"),
        }
        assert!(super::dependency_keys(&state, &program, &[bank]).is_empty());
    }

    /// B16: every bank that cannot contribute a complete dependency set is reported with its
    /// cause (missing, foreign owner, not a bank, unknown layout); a valid bank has no gap and
    /// the same keys as `dependency_keys`.
    #[test]
    fn b16_dependency_gaps_are_reported_not_skipped() {
        use super::DependencyGap as G;
        use marginfi_type_crate::types::Bank;
        let program = Pubkey::new_unique();
        let [valid, missing, foreign, other, unresized] = [(); 5].map(|_| Pubkey::new_unique());
        let mut bank: Bank = bytemuck::Zeroable::zeroed();
        bank.mint = Pubkey::new_unique();
        let mut good = protocol::DISC_BANK.to_vec();
        good.extend_from_slice(bytemuck::bytes_of(&bank));
        let mut small = protocol::DISC_BANK.to_vec();
        small.resize(8 + protocol::BANK_PAYLOAD_LEN_0_1_11, 0);
        let mut not_bank = protocol::DISC_MARGINFI_ACCOUNT.to_vec();
        not_bank.resize(64, 0);
        let entry = |owner: Pubkey, bytes: &[u8]| {
            let (w, n) = aligned(bytes);
            (owner, w, n)
        };
        let state = Source(HashMap::from([
            (valid, entry(program, &good)),
            (foreign, entry(Pubkey::new_unique(), &good)),
            (other, entry(program, &not_bank)),
            (unresized, entry(program, &small)),
        ]));
        let scan = super::dependency_scan(
            &state,
            &program,
            &[valid, missing, foreign, other, unresized],
        );
        assert_eq!(scan.keys, vec![bank.mint]);
        assert_eq!(
            scan.keys,
            super::dependency_keys(&state, &program, &[valid])
        );
        let codes: Vec<_> = scan.gaps.iter().map(|(k, g)| (*k, g.code())).collect();
        assert_eq!(
            codes,
            [
                (missing, G::Missing.code()),
                (foreign, G::ForeignOwner.code()),
                (other, G::NotABank.code()),
                (unresized, "DEPENDENCY_BANK_UNKNOWN_LAYOUT"),
            ]
        );
        assert!(matches!(&scan.gaps[3].1, G::UnknownLayout(e) if e.contains("0.1.11")));
    }
}

#[cfg(all(test, feature = "crate_health"))]
mod threshold_search_tests {
    use super::crate_impl::CrateHealth;
    use super::{AccountSource, HealthError};
    use solana_sdk::pubkey::Pubkey;

    struct EmptySource;
    impl AccountSource for EmptySource {
        fn account(&self, _: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            None
        }
    }

    #[test]
    fn first_true_boundary_finds_exact_integer_crossing() {
        let _engine = CrateHealth {
            state: &EmptySource,
            group: Pubkey::new_unique(),
        };
        let got = CrateHealth::<EmptySource>::first_true_boundary(50, |x| Ok(x >= 73)).unwrap();
        assert_eq!(got, Some(73));
    }

    #[test]
    fn first_true_boundary_expands_above_current() {
        let got = CrateHealth::<EmptySource>::first_true_boundary(10, |x| Ok(x >= 10_000)).unwrap();
        assert_eq!(got, Some(10_000));
    }

    #[test]
    fn first_true_boundary_propagates_fail_closed_error() {
        let got = CrateHealth::<EmptySource>::first_true_boundary(10, |x| {
            if x >= 20 {
                Err(HealthError::Crate("boom".into()))
            } else {
                Ok(false)
            }
        });
        assert!(got.is_err());
    }
}

/// [FIX-26] Externe Abhängigkeiten aller Banks im State (Oracle-PDAs/-Keys, Integrations-Accounts,
/// Mints, Extra-Keys) — damit Ingress/Snapshot sie **vor** der ersten Health-Berechnung halten.
/// Kein RPC hier; nur Ableitung aus dem RAM-State.
/// B16: why a bank contributes no, or no complete, dependency set. Reported by
/// `dependency_scan` instead of skipped silently; Health for such a bank fails closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DependencyGap {
    /// No account under this key in the state.
    Missing,
    /// The account is not owned by the configured program (source identity).
    ForeignOwner,
    /// Program-owned, but not a Marginfi bank.
    NotABank,
    /// A bank layout the pinned decoder does not qualify (e.g. 0.1.12 resized).
    UnknownLayout(String),
    /// The oracle setup has no qualified observation keys; mint/integration keys stay listed,
    /// the bank is never priced.
    UnsupportedOracle(String),
}
impl DependencyGap {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing => "DEPENDENCY_BANK_MISSING",
            Self::ForeignOwner => "DEPENDENCY_BANK_FOREIGN_OWNER",
            Self::NotABank => "DEPENDENCY_NOT_A_BANK",
            Self::UnknownLayout(_) => "DEPENDENCY_BANK_UNKNOWN_LAYOUT",
            Self::UnsupportedOracle(_) => "DEPENDENCY_ORACLE_UNSUPPORTED",
        }
    }
}

/// Dependency keys of a bank set plus every bank that could not contribute a complete set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DependencyScan {
    pub keys: Vec<Pubkey>,
    pub gaps: Vec<(Pubkey, DependencyGap)>,
}

#[cfg(feature = "crate_health")]
pub fn dependency_keys<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    banks: &[Pubkey],
) -> Vec<Pubkey> {
    dependency_scan(state, program, banks).keys
}

#[cfg(feature = "crate_health")]
pub fn dependency_scan<S: AccountSource + ?Sized>(
    state: &S,
    program: &Pubkey,
    banks: &[Pubkey],
) -> DependencyScan {
    use crate::protocol;
    let mut out: Vec<Pubkey> = Vec::new();
    let mut gaps = Vec::new();
    for bk in banks {
        let Some((owner, _, data)) = state.account(bk) else {
            gaps.push((*bk, DependencyGap::Missing));
            continue;
        };
        if &owner != program {
            gaps.push((*bk, DependencyGap::ForeignOwner));
            continue;
        }
        if protocol::classify(data) != protocol::AccountType::Bank {
            gaps.push((*bk, DependencyGap::NotABank));
            continue;
        }
        // [P1B-02] Resized/unknown layouts get no dependencies; Health then fails closed.
        let bank = match protocol::decode_bank(data) {
            Ok(bank) => bank,
            Err(e) => {
                gaps.push((*bk, DependencyGap::UnknownLayout(e.to_string())));
                continue;
            }
        };
        out.push(bank.mint);
        // [FIX-27] dieselbe Quelle wie die Health-Observation (p0-cli-Semantik).
        match crate_impl::bank_observation_keys(&bank) {
            Ok(keys) => out.extend(keys),
            Err(e) => gaps.push((*bk, DependencyGap::UnsupportedOracle(e.to_string()))),
        }

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
    DependencyScan { keys: out, gaps }
}

#[cfg(all(test, feature = "crate_health"))]
mod c10_tests {
    use super::crate_impl::{crate_error_text, integration_stale};
    use super::INTEGRATION_STALE;
    use marginfi::errors::MarginfiError as M;

    /// C10: the stale integration reserves of the pinned program are recognized by error code,
    /// as Anchor error or as custom program error, never by message text.
    #[test]
    fn integration_stale_is_classified_by_the_pinned_error_code() {
        for m in [
            M::ReserveStale,
            M::SolendReserveStale,
            M::DriftSpotMarketStale,
            M::JuplendLendingStale,
        ] {
            let code = anchor_lang::error::Error::from(m).to_string();
            assert!(
                integration_stale(&anchor_lang::error::Error::from(m)),
                "{code}"
            );
        }
        for m in [M::RiskEngineInitRejected, M::PythPushStalePrice] {
            assert!(!integration_stale(&anchor_lang::error::Error::from(m)));
        }
        let custom =
            anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::Custom(6206));
        assert!(integration_stale(&custom));
        let other =
            anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::Custom(6207));
        assert!(!integration_stale(&other));
        assert!(crate_error_text("oracle adapter", &custom).starts_with(INTEGRATION_STALE));
        assert!(!crate_error_text("oracle adapter", &other).starts_with(INTEGRATION_STALE));
    }
}
