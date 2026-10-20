//! Exact receivership sizing from the pinned marginfi protocol implementation.
//!
//! This module deliberately stops at protocol quantities/values. It does **not** call a DEX and
//! does not claim oracle value is realizable market value. P4A-01 must value the resulting seized
//! amount through an executable route.

use crate::health::{
    crate_impl::{
        bank_observation_keys, crate_error_text, emode_liability_balances, integration_stale,
        maintenance_emode_config_of, CrateHealth,
    },
    AccountSource, HealthEngine, HealthError,
};
use crate::protocol;
use fixed::types::I80F48;
use marginfi::state::{
    bank::BankImpl,
    marginfi_account::{calc_amount, calc_value},
    price::{OraclePriceFeedAdapter, PriceAdapter},
};
use marginfi_type_crate::constants::{
    ASSET_TAG_DEFAULT, ASSET_TAG_DRIFT, ASSET_TAG_JUPLEND, ASSET_TAG_KAMINO, ASSET_TAG_SOL,
    ZERO_AMOUNT_THRESHOLD,
};
use marginfi_type_crate::types::{
    BalanceSide, Bank, BankOperationalState, MarginfiAccount, OraclePriceType, PriceBias,
    RequirementType,
};
use solana_sdk::account::Account;
use solana_sdk::account_info::IntoAccountInfo;
use solana_sdk::clock::Clock;
use solana_sdk::pubkey::Pubkey;

const USD_MICRO: u64 = 1_000_000;
const BPS: u64 = 10_000;
/// Keep a small deterministic margin below the mathematical health boundary so integer rounding of
/// native token amounts does not accidentally cross to positive post-health before simulation.
/// This is a protocol-sizing safety margin, not slippage and not a profit assumption.
pub const DEFAULT_HEADROOM_BPS: u64 = 9_990;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolSizing {
    pub asset_bank: Pubkey,
    pub liab_bank: Pubkey,
    /// Native token units passed to LendingAccountWithdraw.
    pub withdraw_amount: u64,
    /// Native token units transferred into LendingAccountRepay.
    pub repay_amount: u64,
    /// Exact source-backed repay-all semantics. When true, `repay_amount` is ceil(current
    /// liability native amount), but EndLiquidation observes the exact liability equity delta.
    pub repay_all: bool,
    /// Equity/EMA values used by EndLiquidation after native-unit rounding / full-close semantics.
    pub seized_equity_value_micro: u64,
    pub repay_equity_value_micro: u64,
    /// Economic value of the actual SPL amount transferred for repayment. This can exceed
    /// `repay_equity_value_micro` on repay_all because Marginfi transfers ceil(liability).
    pub repay_transfer_value_micro: u64,
    /// Actual protocol premium implied by rounded amounts.
    pub premium_bps: u64,
    /// On-chain FeeState cap used by the sizing calculation.
    pub fee_state_max_bps: u64,
    pub flat_sol_fee_lamports: u64,
}

/// Q5: the kind of a sizing failure, typed where it arises. The text is the former message, so
/// reasons and journals read as before; classification never inspects it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizingErrorKind {
    /// No protocol-valid asset/liability pair (the `SIZING:NO_VALID_PROTOCOL_PAIR:` census).
    NoValidPair,
    /// C10: the pinned program reports a stale integration reserve (typed error code).
    IntegrationStale,
    /// `HealthError::Unavailable`: no health engine built in.
    HealthUnavailable,
    /// State, layout, oracle or arithmetic failure.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SizingError {
    pub kind: SizingErrorKind,
    pub text: String,
}

impl SizingError {
    /// A pinned-crate error in `context`; a stale integration reserve is typed by its error code
    /// (the text keeps the `INTEGRATION_STALE:` marker for readers of the reason).
    fn crate_error(context: &str, e: &anchor_lang::error::Error) -> Self {
        SizingError {
            kind: if integration_stale(e) {
                SizingErrorKind::IntegrationStale
            } else {
                SizingErrorKind::Other
            },
            text: crate_error_text(context, e),
        }
    }
}

impl From<HealthError> for SizingError {
    fn from(e: HealthError) -> Self {
        let kind = match e {
            HealthError::IntegrationStale(_) => SizingErrorKind::IntegrationStale,
            HealthError::Unavailable => SizingErrorKind::HealthUnavailable,
            HealthError::Unsupported(_)
            | HealthError::Stale(_)
            | HealthError::Crate(_)
            | HealthError::MissingAccount(_) => SizingErrorKind::Other,
        };
        SizingError {
            kind,
            text: e.to_string(),
        }
    }
}

impl From<String> for SizingError {
    fn from(text: String) -> Self {
        SizingError {
            kind: SizingErrorKind::Other,
            text,
        }
    }
}

impl From<&str> for SizingError {
    fn from(text: &str) -> Self {
        text.to_string().into()
    }
}

impl From<SizingError> for String {
    fn from(e: SizingError) -> Self {
        e.text
    }
}

impl std::fmt::Display for SizingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// Q5: why sizing produced no candidate, as a fixed code ([FIX-89] diagnostics).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoSizeReason {
    NotMarginfiAccount,
    NoAsset,
    NoLiability,
    UnsupportedIntegrationAsset,
    UnsupportedIntegrationLiability,
    NoSupportedAssetBank,
    NoSupportedLiabilityBank,
    NoValidProtocolPair,
}

impl NoSizeReason {
    pub fn code(self) -> &'static str {
        match self {
            NoSizeReason::NotMarginfiAccount => "NOT_MARGINFI_ACCOUNT",
            NoSizeReason::NoAsset => "NO_ASSET",
            NoSizeReason::NoLiability => "NO_LIABILITY",
            NoSizeReason::UnsupportedIntegrationAsset => "UNSUPPORTED_INTEGRATION_ASSET",
            NoSizeReason::UnsupportedIntegrationLiability => "UNSUPPORTED_INTEGRATION_LIABILITY",
            NoSizeReason::NoSupportedAssetBank => "NO_SUPPORTED_ASSET_BANK",
            NoSizeReason::NoSupportedLiabilityBank => "NO_SUPPORTED_LIABILITY_BANK",
            NoSizeReason::NoValidProtocolPair => "NO_VALID_PROTOCOL_PAIR",
        }
    }

    /// The account holds an integration bank this build cannot size.
    pub fn unsupported_integration(self) -> bool {
        match self {
            NoSizeReason::UnsupportedIntegrationAsset
            | NoSizeReason::UnsupportedIntegrationLiability => true,
            NoSizeReason::NotMarginfiAccount
            | NoSizeReason::NoAsset
            | NoSizeReason::NoLiability
            | NoSizeReason::NoSupportedAssetBank
            | NoSizeReason::NoSupportedLiabilityBank
            | NoSizeReason::NoValidProtocolPair => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeeSnapshot {
    pub global_fee_wallet: Pubkey,
    pub liquidation_max_bps: u64,
    pub liquidation_flat_sol_fee_lamports: u64,
}

pub struct ProtocolSizer<'a, S: AccountSource + ?Sized> {
    pub state: &'a S,
    pub program: Pubkey,
    pub group: Pubkey,
    pub headroom_bps: u64,
}

#[derive(Clone, Copy, Debug)]
struct Prices {
    /// RequirementType::Equity uses TimeWeighted (TWAP/EMA) in pinned marginfi source. [FIX-62]
    equity: I80F48,
    maint_asset: I80F48,
    maint_liab: I80F48,
}

#[derive(Clone, Copy)]
struct Position {
    bank_pk: Pubkey,
    bank: Bank,
    amount: I80F48,
    /// Amount on the opposite side of the same balance slot. For a liability this is normally
    /// zero, but pinned Marginfi permits sub-threshold asset dust during repay_all.
    opposite_amount: I80F48,
    equity_value: I80F48,
    maintenance_unweighted_value: I80F48,
    /// B7: the bank's prices at this sizing's clock, computed once in `positions`.
    prices: Prices,
}

/// B2: strict pinned MarginfiAccount (discriminator, exact length), never a prefix decode.
fn load_account(data: &[u8]) -> Result<std::borrow::Cow<'_, MarginfiAccount>, String> {
    protocol::decode_marginfi_account(data).map_err(|e| e.to_string())
}

/// EndLiquidation caps premium; it does not require taking the maximum. If the
/// maximum would worsen health, retain headroom below the health-neutral ratio.
fn healing_fee_factor(
    cap: I80F48,
    asset_per_usd: I80F48,
    liab_per_usd: I80F48,
    headroom: I80F48,
) -> Option<I80F48> {
    if cap < I80F48::ONE || asset_per_usd < I80F48::ZERO || liab_per_usd <= I80F48::ZERO {
        return None;
    }
    if liab_per_usd.checked_div(cap)? > asset_per_usd {
        return Some(cap);
    }
    if asset_per_usd == I80F48::ZERO {
        return None;
    }
    let reduced = liab_per_usd
        .checked_div(asset_per_usd)?
        .checked_mul(headroom)?
        .min(cap);
    (reduced >= I80F48::ONE && liab_per_usd.checked_div(reduced)? > asset_per_usd)
        .then_some(reduced)
}

fn rounded_health_valid(
    pre: I80F48,
    seized: I80F48,
    repaid: I80F48,
    asset_per_usd: I80F48,
    liab_per_usd: I80F48,
    closeout: bool,
) -> Option<bool> {
    let delta = repaid
        .checked_mul(liab_per_usd)?
        .checked_sub(seized.checked_mul(asset_per_usd)?)?;
    Some(delta >= I80F48::ZERO && (closeout || pre.checked_add(delta)? <= I80F48::ZERO))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RepayChoice {
    transfer_amount: u64,
    protocol_amount: I80F48,
    repay_all: bool,
}

/// [FIX-128][R7/C2] Pinned Marginfi parity for the near-full-liability rounding edge.
///
/// A normal repay receives an integer SPL amount and must not exceed the current liability.
/// `repay_all` is different: Marginfi closes the balance and transfers ceil(exact liability).
/// It is valid only when the requested rounded amount is exactly that full-close transfer amount
/// and any opposite-side asset is below the protocol ZERO_AMOUNT_THRESHOLD. We never clamp a
/// larger target to the liability silently; final health/premium are rechecked from
/// `protocol_amount`, which is the exact liability removed by EndLiquidation.
fn choose_repay(
    target_exact: I80F48,
    liability_amount: I80F48,
    opposite_asset_amount: I80F48,
) -> Result<Option<RepayChoice>, String> {
    let rounded = ceil_u64(target_exact)?;
    if rounded == 0 {
        return Ok(None);
    }
    if I80F48::from_num(rounded) <= liability_amount {
        return Ok(Some(RepayChoice {
            transfer_amount: rounded,
            protocol_amount: I80F48::from_num(rounded),
            repay_all: false,
        }));
    }

    let full_transfer = ceil_u64(liability_amount)?;
    if rounded != full_transfer
        || liability_amount <= ZERO_AMOUNT_THRESHOLD
        || opposite_asset_amount.abs() >= ZERO_AMOUNT_THRESHOLD
    {
        return Ok(None);
    }

    Ok(Some(RepayChoice {
        transfer_amount: full_transfer,
        protocol_amount: liability_amount,
        repay_all: true,
    }))
}

fn amount_from_equity(value: I80F48, price: I80F48, bank: &Bank) -> Result<I80F48, String> {
    calc_amount(value, price, bank.get_balance_decimals())
        .map_err(|e| format!("native balance amount: {e}"))
}

fn floor_u64(v: I80F48) -> Result<u64, String> {
    if v <= I80F48::ZERO {
        return Ok(0);
    }
    v.checked_floor()
        .and_then(|x| x.checked_to_num::<u64>())
        .ok_or_else(|| "positive I80F48 does not fit u64".to_string())
}

fn ceil_u64(v: I80F48) -> Result<u64, String> {
    if v <= I80F48::ZERO {
        return Ok(0);
    }
    v.checked_ceil()
        .and_then(|x| x.checked_to_num::<u64>())
        .ok_or_else(|| "positive I80F48 does not fit u64".to_string())
}

fn usd_micro(v: I80F48) -> Result<u64, String> {
    let scaled = v
        .checked_mul(I80F48::from_num(USD_MICRO))
        .ok_or_else(|| "USD-micro overflow".to_string())?;
    floor_u64(scaled)
}

fn fee_bps(v: I80F48) -> Result<u64, String> {
    let scaled = v
        .checked_mul(I80F48::from_num(BPS))
        .ok_or_else(|| "fee bps overflow".to_string())?;
    floor_u64(scaled)
}

fn supported_operational_state(bank: &Bank) -> bool {
    matches!(
        bank.config.operational_state,
        BankOperationalState::Operational
            | BankOperationalState::ReduceOnly
            | BankOperationalState::ReduceOnlyWithBorrowingPower
    )
}

fn supported_generic_bank(bank: &Bank) -> bool {
    matches!(bank.config.asset_tag, ASSET_TAG_DEFAULT | ASSET_TAG_SOL)
        && supported_operational_state(bank)
}

/// [FIX-112][R6] Integration banks are collateral-only in the pinned Marginfi source
/// (borrow_limit=0). They are eligible only on the asset side, and only after exact
/// refresh projection has been applied by the runner.
fn supported_asset_bank(bank: &Bank) -> bool {
    matches!(
        bank.config.asset_tag,
        ASSET_TAG_DEFAULT | ASSET_TAG_SOL | ASSET_TAG_KAMINO | ASSET_TAG_DRIFT | ASSET_TAG_JUPLEND
    ) && supported_operational_state(bank)
}

fn supported_liability_bank(bank: &Bank) -> bool {
    supported_generic_bank(bank)
}

/// Asset and liability positions of one account.
type Positions = (Vec<Position>, Vec<Position>);

impl<'a, S: AccountSource + ?Sized> ProtocolSizer<'a, S> {
    fn raw(&self, key: &Pubkey) -> Result<(Pubkey, u64, &'a [u8]), String> {
        self.state
            .account(key)
            .ok_or_else(|| format!("missing state account {key}"))
    }

    fn bank(&self, key: &Pubkey) -> Result<Bank, String> {
        let (owner, _, data) = self.raw(key)?;
        if owner != self.program {
            return Err(format!("UNKNOWN_LAYOUT: bank {key} owner {owner}"));
        }
        protocol::decode_bank(data)
            .map(std::borrow::Cow::into_owned)
            .map_err(|e| format!("{e}: {key}"))
    }

    /// B12: the maintenance EMODE configuration exactly as Health computes it — every active
    /// liability balance (also those this sizer cannot size) and the group's same-asset EMODE.
    fn maintenance_emode(
        &self,
        account: &MarginfiAccount,
    ) -> Result<marginfi_type_crate::types::ReconciledEmodeConfig, String> {
        let (owner, _, gdata) = self.raw(&self.group)?;
        if owner != self.program {
            return Err(format!(
                "UNKNOWN_LAYOUT: group {} owner {owner}",
                self.group
            ));
        }
        let group = protocol::decode_group(gdata).map_err(|e| format!("{e}: group"))?;
        let banks = emode_liability_balances(account)
            .map(|b| self.bank(&b.bank_pk))
            .collect::<Result<Vec<_>, _>>()?;
        let refs: Vec<&Bank> = banks.iter().collect();
        Ok(maintenance_emode_config_of(&refs, &group))
    }

    fn fee_snapshot_and_rate(&self) -> Result<(FeeSnapshot, I80F48), String> {
        let key = protocol::fee_state_pda(&self.program);
        let (owner, _, data) = self.raw(&key)?;
        if owner != self.program || protocol::classify(data) != protocol::AccountType::FeeState {
            return Err("UNKNOWN_LAYOUT: invalid FeeState".into());
        }
        let fee = protocol::decode_fee_state(data).map_err(|e| e.to_string())?;
        let f: I80F48 = fee.liquidation_max_fee.into();
        Ok((
            FeeSnapshot {
                global_fee_wallet: fee.global_fee_wallet,
                liquidation_max_bps: fee_bps(f)?,
                liquidation_flat_sol_fee_lamports: fee.liquidation_flat_sol_fee as u64,
            },
            f,
        ))
    }

    pub fn fee_snapshot(&self) -> Result<FeeSnapshot, String> {
        self.fee_snapshot_and_rate().map(|(snapshot, _)| snapshot)
    }

    /// Equity/TWAP USD-micro value of a native token amount for one Marginfi bank.
    /// Used for SOL-denominated execution costs and inventory valuation so every economics
    /// input uses the same pinned Marginfi price semantics. [FIX-66]
    pub fn native_amount_value_usd_micro(
        &self,
        bank_key: &Pubkey,
        native_amount: u64,
        now_unix_s: i64,
        slot: u64,
    ) -> Result<u64, SizingError> {
        if native_amount == 0 {
            return Ok(0);
        }
        let bank = self.bank(bank_key)?;
        let clock = Clock {
            slot,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: now_unix_s,
        };
        let price = self.prices(&bank, &clock)?.equity;
        let value = calc_value(
            I80F48::from_num(native_amount),
            price,
            bank.get_balance_decimals(),
            None,
        )
        .map_err(|e| format!("native amount value: {e}"))?;
        Ok(usd_micro(value)?)
    }

    pub fn bank_mint(&self, bank_key: &Pubkey) -> Result<Pubkey, String> {
        Ok(self.bank(bank_key)?.mint)
    }

    /// [FIX-89] Deterministic explanation when protocol sizing produced zero candidates.
    /// This is diagnostics only; it does not relax any fail-closed sizing rule.
    pub fn no_size_reason(&self, account_key: &Pubkey) -> Result<NoSizeReason, String> {
        let (_, _, data) = self.raw(account_key)?;
        if protocol::classify(data) != protocol::AccountType::MarginfiAccount {
            return Ok(NoSizeReason::NotMarginfiAccount);
        }
        let account = load_account(data)?;
        let account: &MarginfiAccount = &account;
        let mut assets = 0usize;
        let mut liabs = 0usize;
        let mut supported_assets = 0usize;
        let mut supported_liabs = 0usize;
        let mut integration_assets = 0usize;
        let mut integration_liabs = 0usize;

        for balance in account
            .lending_account
            .balances
            .iter()
            .filter(|b| b.is_active())
        {
            let Some(side) = balance.get_side() else {
                continue;
            };
            let bank = self.bank(&balance.bank_pk)?;
            let integration = matches!(
                bank.config.asset_tag,
                ASSET_TAG_KAMINO | ASSET_TAG_DRIFT | ASSET_TAG_JUPLEND
            );
            match side {
                BalanceSide::Assets => {
                    assets += 1;
                    if supported_asset_bank(&bank) {
                        supported_assets += 1;
                    }
                    if integration {
                        integration_assets += 1;
                    }
                }
                BalanceSide::Liabilities => {
                    liabs += 1;
                    if supported_liability_bank(&bank) {
                        supported_liabs += 1;
                    }
                    if integration {
                        integration_liabs += 1;
                    }
                }
            }
        }

        if assets == 0 {
            return Ok(NoSizeReason::NoAsset);
        }
        if liabs == 0 {
            return Ok(NoSizeReason::NoLiability);
        }
        if supported_assets == 0 && integration_assets > 0 {
            return Ok(NoSizeReason::UnsupportedIntegrationAsset);
        }
        if supported_liabs == 0 && integration_liabs > 0 {
            return Ok(NoSizeReason::UnsupportedIntegrationLiability);
        }
        if supported_assets == 0 {
            return Ok(NoSizeReason::NoSupportedAssetBank);
        }
        if supported_liabs == 0 {
            return Ok(NoSizeReason::NoSupportedLiabilityBank);
        }
        Ok(NoSizeReason::NoValidProtocolPair)
    }

    /// B4: conservative cost conversion — the bank's upper price bound (`conservative_cost_price`),
    /// rounded up; for debits whose value must not be understated (rent of a created account).
    pub fn native_amount_conservative_cost_usd_micro(
        &self,
        bank_key: &Pubkey,
        native_amount: u64,
        now_unix_s: i64,
        slot: u64,
    ) -> Result<u64, String> {
        if native_amount == 0 {
            return Ok(0);
        }
        let bank = self.bank(bank_key)?;
        let clock = Clock {
            slot,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: now_unix_s,
        };
        let price = self.conservative_cost_price(&bank, &clock)?;
        if price <= I80F48::ZERO {
            return Err("nonpositive cost conversion price".into());
        }
        let value = calc_value(
            I80F48::from_num(native_amount),
            price,
            bank.get_balance_decimals(),
            None,
        )
        .map_err(|e| format!("native amount value: {e}"))?;
        ceil_u64(
            value
                .checked_mul(I80F48::from_num(USD_MICRO))
                .ok_or("cost conversion overflow")?,
        )
    }

    /// Cost conversion rounds up; executable proceeds use the value method.
    pub fn native_amount_cost_usd_micro(
        &self,
        bank_key: &Pubkey,
        native_amount: u64,
        now_unix_s: i64,
        slot: u64,
    ) -> Result<u64, String> {
        if native_amount == 0 {
            return Ok(0);
        }
        let bank = self.bank(bank_key)?;
        let clock = Clock {
            slot,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: now_unix_s,
        };
        let price = self.prices(&bank, &clock)?.equity;
        if price <= I80F48::ZERO {
            return Err("nonpositive cost conversion price".into());
        }
        let value = calc_value(
            I80F48::from_num(native_amount),
            price,
            bank.get_balance_decimals(),
            None,
        )
        .map_err(|e| format!("native amount value: {e}"))?;
        ceil_u64(
            value
                .checked_mul(I80F48::from_num(USD_MICRO))
                .ok_or("cost conversion overflow")?,
        )
    }

    /// B4: upper bound of the bank's price under the pinned oracle adapter — the higher of the
    /// high-biased (`PriceBias::High`, price plus its confidence interval) time-weighted and
    /// real-time prices.
    fn conservative_cost_price(&self, bank: &Bank, clock: &Clock) -> Result<I80F48, SizingError> {
        self.with_feed(bank, clock, |feed| {
            let high = |price_type, what| {
                feed.get_price_of_type(
                    price_type,
                    Some(PriceBias::High),
                    bank.config.oracle_max_confidence,
                )
                .map_err(|e| SizingError::crate_error(what, &e))
            };
            Ok(
                high(OraclePriceType::TimeWeighted, "time-weighted high price")?
                    .max(high(OraclePriceType::RealTime, "real-time high price")?),
            )
        })
    }

    fn prices(&self, bank: &Bank, clock: &Clock) -> Result<Prices, SizingError> {
        self.with_feed(bank, clock, |feed| {
            let equity = feed
                .get_price_of_type(
                    OraclePriceType::TimeWeighted,
                    None,
                    bank.config.oracle_max_confidence,
                )
                .map_err(|e| SizingError::crate_error("equity price", &e))?;
            let maint_asset = feed
                .get_price_of_type(
                    OraclePriceType::RealTime,
                    Some(PriceBias::Low),
                    bank.config.oracle_max_confidence,
                )
                .map_err(|e| SizingError::crate_error("maintenance asset price", &e))?;
            let maint_liab = feed
                .get_price_of_type(
                    OraclePriceType::RealTime,
                    Some(PriceBias::High),
                    bank.config.oracle_max_confidence,
                )
                .map_err(|e| SizingError::crate_error("maintenance liability price", &e))?;
            Ok(Prices {
                equity,
                maint_asset,
                maint_liab,
            })
        })
    }

    /// The pinned oracle adapter of `bank` over its observation accounts at `clock`.
    fn with_feed<T>(
        &self,
        bank: &Bank,
        clock: &Clock,
        f: impl FnOnce(&OraclePriceFeedAdapter) -> Result<T, SizingError>,
    ) -> Result<T, SizingError> {
        let keys = bank_observation_keys(bank).map_err(|e| e.to_string())?;
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
        let feed = OraclePriceFeedAdapter::try_from_bank(bank, &ais, clock)
            .map_err(|e| SizingError::crate_error("oracle adapter", &e))?;
        f(&feed)
    }

    fn positions(
        &self,
        account: &MarginfiAccount,
        clock: &Clock,
    ) -> Result<Positions, SizingError> {
        let mut assets = Vec::new();
        let mut liabs = Vec::new();
        for balance in &account.lending_account.balances {
            if !balance.is_active() {
                continue;
            }
            let Some(side) = balance.get_side() else {
                continue;
            };
            let bank = self.bank(&balance.bank_pk)?;
            // [FIX-112][R6] Integration banks are eligible only as collateral assets.
            // Liability sizing remains restricted to generic banks because the pinned integration
            // configs are non-borrowable. The runner guarantees that integration candidates reach
            // this sizer only through a qualified ProjectedState.
            let supported = match side {
                BalanceSide::Assets => supported_asset_bank(&bank),
                BalanceSide::Liabilities => supported_liability_bank(&bank),
            };
            if !supported || bank.check_utilization_ratio().is_err() {
                continue;
            }
            let prices = self.prices(&bank, clock)?;
            let (amount, opposite_amount, maint_price, is_asset) = match side {
                BalanceSide::Assets => (
                    bank.get_asset_amount(balance.asset_shares.into())
                        .map_err(|e| format!("asset amount: {e}"))?,
                    bank.get_liability_amount(balance.liability_shares.into())
                        .map_err(|e| format!("opposite liability amount: {e}"))?,
                    prices.maint_asset,
                    true,
                ),
                BalanceSide::Liabilities => (
                    bank.get_liability_amount(balance.liability_shares.into())
                        .map_err(|e| format!("liability amount: {e}"))?,
                    bank.get_asset_amount(balance.asset_shares.into())
                        .map_err(|e| format!("opposite asset amount: {e}"))?,
                    prices.maint_liab,
                    false,
                ),
            };
            if amount <= I80F48::ZERO {
                continue;
            }
            let equity_value = calc_value(amount, prices.equity, bank.get_balance_decimals(), None)
                .map_err(|e| format!("equity value: {e}"))?;
            let maintenance_unweighted_value =
                calc_value(amount, maint_price, bank.get_balance_decimals(), None)
                    .map_err(|e| format!("maintenance value: {e}"))?;
            let p = Position {
                bank_pk: balance.bank_pk,
                bank,
                amount,
                opposite_amount,
                equity_value,
                maintenance_unweighted_value,
                prices,
            };
            if is_asset {
                assets.push(p);
            } else {
                liabs.push(p);
            }
        }
        Ok((assets, liabs))
    }

    /// Returns every protocol-valid generic Asset×Liability sizing candidate. The caller must
    /// separately establish executable routing and realizable market value.
    pub fn sizes(
        &self,
        account_key: &Pubkey,
        now_unix_s: i64,
        slot: u64,
    ) -> Result<Vec<ProtocolSizing>, SizingError> {
        if self.headroom_bps == 0 || self.headroom_bps > BPS {
            return Err(format!("headroom_bps must be 1..={BPS}").into());
        }
        let (_, _, data) = self.raw(account_key)?;
        if protocol::classify(data) != protocol::AccountType::MarginfiAccount {
            return Err("not a MarginfiAccount".into());
        }
        let account = load_account(data)?;
        let account: &MarginfiAccount = &account;
        let health = CrateHealth {
            state: self.state,
            group: self.group,
        }
        .health(account_key, now_unix_s, slot)
        .map_err(SizingError::from)?;
        if !health.liquidatable || health.maintenance_health >= 0 {
            return Ok(Vec::new());
        }
        let pre_health = I80F48::from_bits(health.maintenance_health);
        let (fee, onchain_fee_rate) = self.fee_snapshot_and_rate()?;
        let fee_rate = {
            let minimum = I80F48::from_num(protocol::LIQUIDATION_BONUS_FEE_MINIMUM_BPS)
                .checked_div(I80F48::from_num(BPS))
                .ok_or_else(|| "minimum fee division".to_string())?;
            if onchain_fee_rate > minimum {
                onchain_fee_rate
            } else {
                minimum
            }
        };
        let max_fee_factor = I80F48::ONE
            .checked_add(fee_rate)
            .ok_or_else(|| "fee factor overflow".to_string())?;
        let clock = Clock {
            slot,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: now_unix_s,
        };
        let (assets, liabs) = self.positions(account, &clock)?;
        let reconciled = self.maintenance_emode(account)?;
        let headroom = I80F48::from_num(self.headroom_bps)
            .checked_div(I80F48::from_num(BPS))
            .ok_or_else(|| "headroom division".to_string())?;
        let mut out = Vec::new();

        // [FIX-92] Pair-level rejection census. [FIX-93] adds only pinned closeout semantics;
        // all normal-liquidation sizing equations remain unchanged.
        let mut rej_asset_equity_nonpositive = 0u32;
        let mut rej_liab_equity_nonpositive = 0u32;
        let mut rej_coeff_nonpositive = 0u32;
        let mut rej_seize_nonpositive = 0u32;
        let mut rej_withdraw_zero = 0u32;
        let mut rej_repay_zero = 0u32;
        let mut rej_repay_above_liability = 0u32;
        let mut rej_repay_value_nonpositive = 0u32;
        let mut rej_premium_bound = 0u32;
        let mut pair_details = Vec::new();
        let mut rej_rounded_health = 0u32;

        for asset in &assets {
            for liab in &liabs {
                if asset.equity_value <= I80F48::ZERO {
                    rej_asset_equity_nonpositive += 1;
                    continue;
                }
                if liab.equity_value <= I80F48::ZERO {
                    rej_liab_equity_nonpositive += 1;
                    continue;
                }
                let asset_weight = asset
                    .bank
                    .get_asset_weight(RequirementType::Maintenance, &reconciled);
                // B12: the same API Health uses (pinned: exactly `liability_weight_maint`).
                let liab_weight: I80F48 = liab
                    .bank
                    .config
                    .get_weight(RequirementType::Maintenance, BalanceSide::Liabilities);
                let asset_ratio = asset
                    .maintenance_unweighted_value
                    .checked_div(asset.equity_value)
                    .ok_or_else(|| "asset maintenance/equity ratio".to_string())?;
                let liab_ratio = liab
                    .maintenance_unweighted_value
                    .checked_div(liab.equity_value)
                    .ok_or_else(|| "liability maintenance/equity ratio".to_string())?;
                let asset_per_usd = asset_ratio
                    .checked_mul(asset_weight)
                    .ok_or("asset health value overflow")?;
                let liab_per_usd = liab_ratio
                    .checked_mul(liab_weight)
                    .ok_or("liability health value overflow")?;
                let Some(fee_factor) =
                    healing_fee_factor(max_fee_factor, asset_per_usd, liab_per_usd, headroom)
                else {
                    rej_coeff_nonpositive += 1;
                    if pair_details.len() < 8 {
                        pair_details.push(format!(
                            "{}→{}:no_nonnegative_premium_healing_pair",
                            asset.bank_pk, liab.bank_pk
                        ));
                    }
                    continue;
                };
                let coeff = liab_per_usd
                    .checked_div(fee_factor)
                    .and_then(|x| x.checked_sub(asset_per_usd))
                    .ok_or("health coefficient overflow")?;
                let health_cap = (-pre_health)
                    .checked_div(coeff)
                    .ok_or_else(|| "health cap division".to_string())?;
                let liab_cap = liab
                    .equity_value
                    .checked_mul(fee_factor)
                    .ok_or_else(|| "liability cap overflow".to_string())?;
                // [FIX-93] Marginfi closeout parity:
                // end_liquidation sets ignore_healthy when pre-liquidation equity assets are
                // below LIQUIDATION_CLOSEOUT_DOLLAR_THRESHOLD. In that branch post-health may
                // cross above zero, so the normal health_cap must not constrain sizing.
                //
                // Keep liab_cap as a conservative strategy guard for now; FIX-93 does not
                // introduce a new repay-all/closeout optimizer.
                let mut seize_value = if health.closeout_allowed {
                    asset.equity_value.min(liab_cap)
                } else {
                    asset.equity_value.min(liab_cap).min(health_cap)
                };
                seize_value = seize_value
                    .checked_mul(headroom)
                    .ok_or_else(|| "headroom multiplication".to_string())?;
                if seize_value <= I80F48::ZERO {
                    rej_seize_nonpositive += 1;
                    continue;
                }

                // B7: the prices `positions` computed for exactly these banks and this clock.
                let (asset_prices, liab_prices) = (asset.prices, liab.prices);
                let withdraw_exact =
                    amount_from_equity(seize_value, asset_prices.equity, &asset.bank)?;
                let withdraw_amount = floor_u64(withdraw_exact)?;
                if withdraw_amount == 0 {
                    rej_withdraw_zero += 1;
                    if pair_details.len() < 8 {
                        pair_details.push(format!("{}→{}:withdraw_zero(exact_bits={},seize_bits={},asset_equity_bits={},liab_equity_bits={},price_bits={},decimals={})", asset.bank_pk, liab.bank_pk, withdraw_exact.to_bits(), seize_value.to_bits(), asset.equity_value.to_bits(), liab.equity_value.to_bits(), asset_prices.equity.to_bits(), asset.bank.get_balance_decimals()));
                    }
                    continue;
                }
                let seized_actual = calc_value(
                    I80F48::from_num(withdraw_amount),
                    asset_prices.equity,
                    asset.bank.get_balance_decimals(),
                    None,
                )
                .map_err(|e| format!("rounded seized value: {e}"))?;
                let repay_target = seized_actual
                    .checked_div(fee_factor)
                    .ok_or_else(|| "repay target division".to_string())?;
                let repay_exact = amount_from_equity(repay_target, liab_prices.equity, &liab.bank)?;
                let Some(repay_choice) =
                    choose_repay(repay_exact, liab.amount, liab.opposite_amount)?
                else {
                    if ceil_u64(repay_exact)? == 0 {
                        rej_repay_zero += 1;
                    } else {
                        rej_repay_above_liability += 1;
                        if pair_details.len() < 8 {
                            pair_details.push(format!(
                                "{}→{}:repay_target_not_source_valid_for_full_close(target_bits={},liability_bits={},opposite_asset_bits={})",
                                asset.bank_pk,
                                liab.bank_pk,
                                repay_exact.to_bits(),
                                liab.amount.to_bits(),
                                liab.opposite_amount.to_bits()
                            ));
                        }
                    }
                    continue;
                };
                let repay_amount = repay_choice.transfer_amount;

                // EndLiquidation measures the liability equity delta, not the SPL transfer.
                // For repay_all that is the exact liability amount; the ceil excess is an
                // economic transfer cost/insurance credit, not extra protocol-repaid equity.
                let repay_protocol_actual = calc_value(
                    repay_choice.protocol_amount,
                    liab_prices.equity,
                    liab.bank.get_balance_decimals(),
                    None,
                )
                .map_err(|e| format!("protocol repay value: {e}"))?;
                let repay_transfer_actual = calc_value(
                    I80F48::from_num(repay_amount),
                    liab_prices.equity,
                    liab.bank.get_balance_decimals(),
                    None,
                )
                .map_err(|e| format!("repay transfer value: {e}"))?;
                if repay_protocol_actual <= I80F48::ZERO || repay_transfer_actual <= I80F48::ZERO {
                    rej_repay_value_nonpositive += 1;
                    continue;
                }

                // repay_all also removes sub-threshold asset dust from the liability balance and
                // credits it to insurance. EndLiquidation observes that as additional seized
                // equity/maintenance loss even though the liquidator never receives the dust.
                let (repay_all_dust_equity, repay_all_dust_health) =
                    if repay_choice.repay_all && liab.opposite_amount > I80F48::ZERO {
                        let dust_equity = calc_value(
                            liab.opposite_amount,
                            liab_prices.equity,
                            liab.bank.get_balance_decimals(),
                            None,
                        )
                        .map_err(|e| format!("repay_all dust equity value: {e}"))?;
                        let dust_maint = calc_value(
                            liab.opposite_amount,
                            liab_prices.maint_asset,
                            liab.bank.get_balance_decimals(),
                            None,
                        )
                        .map_err(|e| format!("repay_all dust maintenance value: {e}"))?
                        .checked_mul(
                            liab.bank
                                .get_asset_weight(RequirementType::Maintenance, &reconciled),
                        )
                        .ok_or("repay_all dust weighted health overflow")?;
                        (dust_equity, dust_maint)
                    } else {
                        (I80F48::ZERO, I80F48::ZERO)
                    };
                // Re-evaluate after floor(withdraw)/ceil(repay). Headroom alone cannot
                // guarantee a near-zero normal account stays nonpositive after rounding.
                let base_health_valid = rounded_health_valid(
                    pre_health,
                    seized_actual,
                    repay_protocol_actual,
                    asset_per_usd,
                    liab_per_usd,
                    health.closeout_allowed,
                )
                .ok_or("rounded health overflow")?;
                let health_with_dust_valid = if repay_all_dust_health > I80F48::ZERO {
                    let delta = repay_protocol_actual
                        .checked_mul(liab_per_usd)
                        .and_then(|x| x.checked_sub(seized_actual.checked_mul(asset_per_usd)?))
                        .and_then(|x| x.checked_sub(repay_all_dust_health))
                        .ok_or("repay_all dust health overflow")?;
                    delta >= I80F48::ZERO
                        && (health.closeout_allowed
                            || pre_health
                                .checked_add(delta)
                                .ok_or("repay_all post health overflow")?
                                <= I80F48::ZERO)
                } else {
                    base_health_valid
                };
                if !health_with_dust_valid {
                    rej_rounded_health += 1;
                    continue;
                }
                // [FIX-93] Normal receivership must obey the exact EndLiquidation premium
                // bound. Marginfi explicitly skips this check for closeout accounts
                // (ignore_healthy=true), so applying it there would reject valid closeouts.
                if !health.closeout_allowed {
                    let max_seized = repay_protocol_actual
                        .checked_mul(fee_factor)
                        .ok_or_else(|| "rounded premium bound overflow".to_string())?;
                    let protocol_seized = seized_actual
                        .checked_add(repay_all_dust_equity)
                        .ok_or("repay_all seized equity overflow")?;
                    if protocol_seized > max_seized {
                        rej_premium_bound += 1;
                        continue;
                    }
                }
                let protocol_seized = seized_actual
                    .checked_add(repay_all_dust_equity)
                    .ok_or("repay_all seized equity overflow")?;
                let premium = protocol_seized
                    .checked_div(repay_protocol_actual)
                    .and_then(|x| x.checked_sub(I80F48::ONE))
                    .unwrap_or(I80F48::ZERO)
                    .max(I80F48::ZERO);
                out.push(ProtocolSizing {
                    asset_bank: asset.bank_pk,
                    liab_bank: liab.bank_pk,
                    withdraw_amount,
                    repay_amount,
                    repay_all: repay_choice.repay_all,
                    seized_equity_value_micro: usd_micro(protocol_seized)?,
                    repay_equity_value_micro: usd_micro(repay_protocol_actual)?,
                    repay_transfer_value_micro: usd_micro(repay_transfer_actual)?,
                    premium_bps: fee_bps(premium)?,
                    fee_state_max_bps: fee.liquidation_max_bps,
                    flat_sol_fee_lamports: fee.liquidation_flat_sol_fee_lamports,
                });
            }
        }

        if out.is_empty() && !assets.is_empty() && !liabs.is_empty() {
            return Err(SizingError {
                kind: SizingErrorKind::NoValidPair,
                text: format!(
                "SIZING:NO_VALID_PROTOCOL_PAIR:asset_eq_nonpositive={};liab_eq_nonpositive={};coeff_nonpositive={};seize_nonpositive={};withdraw_zero={};repay_zero={};repay_above_liability={};repay_value_nonpositive={};premium_bound={};rounded_health={};pair_details={}",
                rej_asset_equity_nonpositive,
                rej_liab_equity_nonpositive,
                rej_coeff_nonpositive,
                rej_seize_nonpositive,
                rej_withdraw_zero,
                rej_repay_zero,
                rej_repay_above_liability,
                rej_repay_value_nonpositive,
                rej_premium_bound,
                rej_rounded_health,
                pair_details.join(" | "),
            ),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B4: the conservative cost conversion takes the bank's upper price bound under the pinned
    /// adapter — max(time-weighted + confidence, real-time + confidence) — converts the exact
    /// native amount and rounds up; zero is free, a zero price or a missing bank/oracle is no
    /// cost (error), never 0. The Pyth case is a PriceUpdateV2 account decoded by the pinned
    /// adapter, with an EMA above spot: the bound exceeds both the EMA and the high spot price.
    #[test]
    fn b4_conservative_cost_uses_the_upper_price_bound_and_rounds_up() {
        use marginfi_type_crate::types::{BankOperationalState, OracleSetup, WrappedI80F48};
        use std::collections::HashMap;
        struct Source(HashMap<Pubkey, (Pubkey, Vec<u8>)>);
        impl AccountSource for Source {
            fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
                self.0.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
            }
        }
        let (program, group, sol, oracle) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let now = 1_700_000_000i64;
        let bank_of = |setup: OracleSetup, price: I80F48| {
            let mut b: Bank = bytemuck::Zeroable::zeroed();
            b.group = group;
            b.mint_decimals = 9;
            b.config.operational_state = BankOperationalState::Operational;
            b.config.oracle_setup = setup;
            b.config.fixed_price = WrappedI80F48::from(price);
            b.config.oracle_keys[0] = oracle;
            [protocol::DISC_BANK.as_slice(), bytemuck::bytes_of(&b)].concat()
        };
        // PriceUpdateV2 (pyth-solana-receiver-sdk): Anchor discriminator, write authority,
        // VerificationLevel::Full, PriceFeedMessage, posted slot — little-endian Borsh.
        let price_update = |price: i64, conf: u64, ema: i64, ema_conf: u64| {
            let mut d = solana_sdk::hash::hash(b"account:PriceUpdateV2").to_bytes()[..8].to_vec();
            d.extend_from_slice(&[7u8; 32]);
            d.push(1);
            d.extend_from_slice(&[9u8; 32]);
            d.extend_from_slice(&price.to_le_bytes());
            d.extend_from_slice(&conf.to_le_bytes());
            d.extend_from_slice(&(-8i32).to_le_bytes());
            d.extend_from_slice(&now.to_le_bytes());
            d.extend_from_slice(&now.to_le_bytes());
            d.extend_from_slice(&ema.to_le_bytes());
            d.extend_from_slice(&ema_conf.to_le_bytes());
            d.extend_from_slice(&10u64.to_le_bytes());
            d
        };
        let state = |bank: Vec<u8>, oracle_data: Option<Vec<u8>>| {
            let mut m = HashMap::from([(sol, (program, bank))]);
            if let Some(d) = oracle_data {
                m.insert(oracle, (Pubkey::new_unique(), d));
            }
            Source(m)
        };
        fn sizer(s: &Source, program: Pubkey, group: Pubkey) -> ProtocolSizer<'_, Source> {
            ProtocolSizer {
                state: s,
                program,
                group,
                headroom_bps: 0,
            }
        }
        let clock = Clock {
            slot: 10,
            epoch_start_timestamp: 0,
            epoch: 0,
            leader_schedule_epoch: 0,
            unix_timestamp: now,
        };
        // Spot 149 ± 0.5, EMA 150 ± 1 (×2.12 confidence multiple of the pinned adapter).
        let pyth = state(
            bank_of(OracleSetup::PythPushOracle, I80F48::ZERO),
            Some(price_update(
                149_0000_0000,
                5000_0000,
                150_0000_0000,
                1_0000_0000,
            )),
        );
        let s = sizer(&pyth, program, group);
        let bank = s.bank(&sol).unwrap();
        let prices = s.prices(&bank, &clock).unwrap();
        let bound = s.conservative_cost_price(&bank, &clock).unwrap();
        assert!(
            bound > prices.equity && bound > prices.maint_liab,
            "{bound} {prices:?}"
        );
        assert!(
            (bound - I80F48::from_num(152.12)).abs() < I80F48::from_num(1e-6),
            "{bound}"
        );
        // Spot above the EMA: the real-time bound (155 + 1.06) is the upper one.
        let spot_high = state(
            bank_of(OracleSetup::PythPushOracle, I80F48::ZERO),
            Some(price_update(
                155_0000_0000,
                5000_0000,
                150_0000_0000,
                1_0000_0000,
            )),
        );
        let s2 = sizer(&spot_high, program, group);
        let bound2 = s2
            .conservative_cost_price(&s2.bank(&sol).unwrap(), &clock)
            .unwrap();
        assert!(
            (bound2 - I80F48::from_num(156.06)).abs() < I80F48::from_num(1e-6),
            "{bound2}"
        );
        let cost = |src: &Source, lamports: u64| {
            sizer(src, program, group)
                .native_amount_conservative_cost_usd_micro(&sol, lamports, now, 10)
        };
        // 4_510_080 lamports (record rent) × 152.12 USD/SOL = 0.68607336... USD, rounded up.
        let expected = (I80F48::from_num(4_510_080u64) * bound / I80F48::from_num(1_000u64))
            .ceil()
            .to_num::<u64>();
        assert_eq!(cost(&pyth, 4_510_080), Ok(expected));
        assert_eq!(expected, 686_074);
        // Fixed feed (no confidence): exactly price × amount, rounded up.
        let fixed = |p: f64| state(bank_of(OracleSetup::Fixed, I80F48::from_num(p)), None);
        assert_eq!(cost(&fixed(150.0), 4_510_080), Ok(676_512));
        assert_eq!(cost(&fixed(150.5), 4_510_080), Ok(678_768));
        assert_eq!(cost(&fixed(150.0), 0), Ok(0));
        assert!(cost(&fixed(0.0), 4_510_080).is_err());
        assert!(cost(&Source(HashMap::new()), 4_510_080).is_err());
        let no_oracle = state(bank_of(OracleSetup::PythPushOracle, I80F48::ZERO), None);
        assert!(cost(&no_oracle, 4_510_080).is_err());
    }

    /// B12: sizing uses the maintenance EMODE configuration Health computes — including the
    /// group's same-asset EMODE weight, which the former `reconcile_emode_configs` of the sized
    /// liabilities alone did not contain — and the same liability weight API.
    /// N8 (structure): Health and sizing select the EMODE banks through one filter — active
    /// balances with a non-empty liability side; an asset-only, an active-but-empty and an
    /// inactive liability balance are not part of it, and the sizer's EMODE equals the one
    /// reconciled from exactly the filtered banks.
    #[test]
    fn n8_health_and_sizing_share_one_emode_filter() {
        use marginfi_type_crate::types::{OracleSetup, WrappedI80F48};
        use std::collections::HashMap;
        struct Source(HashMap<Pubkey, (Pubkey, Vec<u8>)>);
        impl AccountSource for Source {
            fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
                self.0.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
            }
        }
        let (program, group_key) = (Pubkey::new_unique(), Pubkey::new_unique());
        let keys: Vec<Pubkey> = (0..4).map(|_| Pubkey::new_unique()).collect();
        let shares = |v: u64| WrappedI80F48::from(I80F48::from_num(v));
        let mut acc: MarginfiAccount = bytemuck::Zeroable::zeroed();
        let b = &mut acc.lending_account.balances;
        // 0: liability; 1: asset only; 2: active but empty; 3: inactive with liability shares.
        (b[0].active, b[0].bank_pk, b[0].liability_shares) = (1, keys[0], shares(5));
        (b[1].active, b[1].bank_pk, b[1].asset_shares) = (1, keys[1], shares(5));
        (b[2].active, b[2].bank_pk) = (1, keys[2]);
        (b[3].active, b[3].bank_pk, b[3].liability_shares) = (0, keys[3], shares(5));
        let selected: Vec<Pubkey> = emode_liability_balances(&acc).map(|b| b.bank_pk).collect();
        assert_eq!(selected, [keys[0]]);
        let mut state = HashMap::new();
        let mut banks = vec![];
        for (i, k) in keys.iter().enumerate() {
            let mut bank: Bank = bytemuck::Zeroable::zeroed();
            bank.config.oracle_setup = OracleSetup::Fixed;
            bank.emode.emode_config.entries[0].collateral_bank_emode_tag = i as u16 + 1;
            banks.push(bank);
            state.insert(
                *k,
                (
                    program,
                    [protocol::DISC_BANK.as_slice(), bytemuck::bytes_of(&bank)].concat(),
                ),
            );
        }
        let group: marginfi_type_crate::types::MarginfiGroup = bytemuck::Zeroable::zeroed();
        state.insert(
            group_key,
            (
                program,
                [
                    protocol::DISC_MARGINFI_GROUP.as_slice(),
                    bytemuck::bytes_of(&group),
                ]
                .concat(),
            ),
        );
        let state = Source(state);
        let sizer = ProtocolSizer {
            state: &state,
            program,
            group: group_key,
            headroom_bps: 0,
        };
        assert_eq!(
            sizer.maintenance_emode(&acc).unwrap(),
            maintenance_emode_config_of(&[&banks[0]], &group)
        );
        assert_ne!(
            sizer.maintenance_emode(&acc).unwrap(),
            maintenance_emode_config_of(&[&banks[0], &banks[1], &banks[2], &banks[3]], &group),
            "the unfiltered balances would form another EMODE set"
        );
    }

    #[test]
    fn b12_sizing_emode_and_liability_weight_equal_health() {
        use marginfi_type_crate::constants::BANK_SAME_ASSET_EMODE_ELIGIBLE;
        use marginfi_type_crate::types::{
            reconcile_emode_configs, MarginfiGroup, OracleSetup, WrappedI80F48,
        };
        use std::collections::HashMap;
        struct Source(HashMap<Pubkey, (Pubkey, Vec<u8>)>);
        impl AccountSource for Source {
            fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
                self.0.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
            }
        }
        let (program, group_key, mint, oracle) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let bank = |weight: f64| {
            let mut b: Bank = bytemuck::Zeroable::zeroed();
            b.mint = mint;
            b.config.oracle_setup = OracleSetup::PythPushOracle;
            b.config.oracle_keys[0] = oracle;
            b.flags = BANK_SAME_ASSET_EMODE_ELIGIBLE;
            b.config.liability_weight_maint = WrappedI80F48::from(I80F48::from_num(weight));
            b
        };
        let (bank_a, bank_b) = (bank(1.25), bank(1.1));
        let mut group: marginfi_type_crate::types::MarginfiGroup = bytemuck::Zeroable::zeroed();
        group.same_asset_emode_maint_leverage = u32::MAX / 10;
        let (ka, kb) = (Pubkey::new_unique(), Pubkey::new_unique());
        let mut acc: MarginfiAccount = bytemuck::Zeroable::zeroed();
        for (i, k) in [ka, kb].into_iter().enumerate() {
            let b = &mut acc.lending_account.balances[i];
            b.active = 1;
            b.bank_pk = k;
            b.liability_shares = WrappedI80F48::from(I80F48::from_num(1_000));
        }
        let data = |disc: [u8; 8], bytes: &[u8]| [disc.as_slice(), bytes].concat();
        let state = Source(HashMap::from([
            (
                ka,
                (
                    program,
                    data(protocol::DISC_BANK, bytemuck::bytes_of(&bank_a)),
                ),
            ),
            (
                kb,
                (
                    program,
                    data(protocol::DISC_BANK, bytemuck::bytes_of(&bank_b)),
                ),
            ),
            (
                group_key,
                (
                    program,
                    data(protocol::DISC_MARGINFI_GROUP, bytemuck::bytes_of(&group)),
                ),
            ),
        ]));
        let sizer = ProtocolSizer {
            state: &state,
            program,
            group: group_key,
            headroom_bps: 0,
        };
        let sized = sizer.maintenance_emode(&acc).unwrap();
        let health = maintenance_emode_config_of(&[&bank_a, &bank_b], &group);
        assert_eq!(sized, health);
        assert_eq!(health.same_asset.mint, mint, "same-asset EMODE active");
        let liabilities_only = reconcile_emode_configs(
            [bank_a.emode.emode_config, bank_b.emode.emode_config],
            RequirementType::Maintenance,
        );
        assert_ne!(sized, liabilities_only, "the former sizing path lacked it");
        let _: &MarginfiGroup = &group;
        for w in [0.0, 1.0, 1.1, 1.25, 3.5] {
            let b = bank(w);
            assert_eq!(
                b.config
                    .get_weight(RequirementType::Maintenance, BalanceSide::Liabilities),
                I80F48::from(b.config.liability_weight_maint)
            );
        }
    }

    #[test]
    fn near_full_rounding_uses_source_backed_repay_all() {
        let debt = I80F48::from_num(9.2);
        let target = I80F48::from_num(9.8);
        let c = choose_repay(target, debt, I80F48::ZERO)
            .unwrap()
            .expect("repay_all");
        assert_eq!(c.transfer_amount, 10);
        assert_eq!(c.protocol_amount, debt);
        assert!(c.repay_all);
    }

    #[test]
    fn repay_all_never_clamps_a_target_above_full_transfer() {
        let debt = I80F48::from_num(9.2);
        assert!(choose_repay(I80F48::from_num(10.1), debt, I80F48::ZERO,)
            .unwrap()
            .is_none());
    }

    #[test]
    fn repay_all_matches_protocol_opposite_dust_boundary() {
        let debt = I80F48::from_num(9.2);
        let target = I80F48::from_num(9.8);
        assert!(
            choose_repay(target, debt, ZERO_AMOUNT_THRESHOLD - I80F48::DELTA,)
                .unwrap()
                .unwrap()
                .repay_all
        );
        assert!(choose_repay(target, debt, ZERO_AMOUNT_THRESHOLD)
            .unwrap()
            .is_none());
    }

    #[test]
    fn ordinary_partial_repay_semantics_are_unchanged() {
        let c = choose_repay(I80F48::from_num(4.1), I80F48::from_num(9.2), I80F48::ZERO)
            .unwrap()
            .unwrap();
        assert_eq!(c.transfer_amount, 5);
        assert_eq!(c.protocol_amount, I80F48::from_num(5));
        assert!(!c.repay_all);
    }

    #[test]
    fn r121_pair_heals_below_maximum_premium() {
        let asset = I80F48::from_bits(281435444320915);
        let liab = I80F48::from_bits(281293154858195) * I80F48::from_bits(309622474381722);
        let cap = I80F48::from_num(1.1);
        assert!(liab / cap - asset < I80F48::ZERO);
        let selected = healing_fee_factor(
            cap,
            asset,
            liab,
            I80F48::from_num(9990) / I80F48::from_num(10000),
        )
        .unwrap();
        assert!(selected >= I80F48::ONE && selected < cap);
        assert!(liab / selected - asset > I80F48::ZERO);
        assert_eq!(
            healing_fee_factor(cap, I80F48::from_num(0.8), liab, I80F48::from_num(0.999)),
            Some(cap)
        );
        assert!(healing_fee_factor(
            cap,
            I80F48::from_num(1.2),
            I80F48::ONE,
            I80F48::from_num(0.999)
        )
        .is_none());
    }

    #[test]
    fn rounded_health_keeps_normal_boundary_and_closeout_exception() {
        let pre = I80F48::from_num(-0.01);
        let one = I80F48::ONE;
        let liab = I80F48::from_num(1.1);
        assert_eq!(
            rounded_health_valid(pre, one, one, one, liab, false),
            Some(false)
        );
        assert_eq!(
            rounded_health_valid(pre, one, one, one, liab, true),
            Some(true)
        );
        assert_eq!(
            rounded_health_valid(pre, one, one, liab, one, true),
            Some(false)
        );
    }

    #[test]
    fn drift_sizing_roundtrip_uses_scaled_balance_decimals() {
        use bytemuck::Zeroable;
        let mut bank = Bank::zeroed();
        bank.mint_decimals = 6;
        bank.config.asset_tag = ASSET_TAG_DRIFT;
        let amount = I80F48::from_num(1_000_000_000u64);
        let price = I80F48::from_num(2);
        let equity = calc_value(amount, price, bank.get_balance_decimals(), None).unwrap();
        assert_eq!(amount_from_equity(equity, price, &bank).unwrap(), amount);
        // The previous mint-decimal inverse returned 1/1000 of the bank balance units.
        assert_ne!(
            calc_amount(equity, price, bank.mint_decimals).unwrap(),
            amount
        );
        bank.config.asset_tag = ASSET_TAG_JUPLEND;
        let equity = calc_value(amount, price, bank.get_balance_decimals(), None).unwrap();
        assert_eq!(amount_from_equity(equity, price, &bank).unwrap(), amount);
    }

    #[test]
    fn integer_conversions_are_conservative() {
        assert_eq!(
            floor_u64(I80F48::from_num(12) + I80F48::from_num(1) / I80F48::from_num(2)).unwrap(),
            12
        );
        assert_eq!(
            ceil_u64(I80F48::from_num(12) + I80F48::from_num(1) / I80F48::from_num(2)).unwrap(),
            13
        );
        assert_eq!(
            usd_micro(I80F48::from_num(3) / I80F48::from_num(2)).unwrap(),
            1_500_000
        );
    }

    #[test]
    fn headroom_is_below_boundary() {
        let h = I80F48::from_num(DEFAULT_HEADROOM_BPS) / I80F48::from_num(BPS);
        assert!(h < I80F48::ONE && h > I80F48::from_num(99) / I80F48::from_num(100));
    }

    #[test]
    fn closeout_threshold_matches_protocol_units() {
        let threshold_bits = (protocol::LIQUIDATION_CLOSEOUT_DOLLAR_THRESHOLD as i128) << 48;
        assert_eq!(
            threshold_bits,
            I80F48::from_num(protocol::LIQUIDATION_CLOSEOUT_DOLLAR_THRESHOLD).to_bits()
        );
        // Mainnet evidence CwSs... was ~51M bits, orders of magnitude below 5 USD in Q48.
        assert!(51_500_000i128 < threshold_bits);
    }

    /// RAM state of one Marginfi account with one asset and one liability bank (pinned program
    /// id, so Health loads it through its Anchor owner check).
    struct TwoBank {
        state: std::collections::HashMap<Pubkey, (Pubkey, Vec<u8>)>,
        program: Pubkey,
        group: Pubkey,
        account: Pubkey,
    }

    impl AccountSource for TwoBank {
        fn account(&self, k: &Pubkey) -> Option<(Pubkey, u64, &[u8])> {
            self.state.get(k).map(|(o, d)| (*o, 1, d.as_slice()))
        }
    }

    impl TwoBank {
        fn sizes(&self) -> Result<Vec<ProtocolSizing>, SizingError> {
            ProtocolSizer {
                state: self,
                program: self.program,
                group: self.group,
                headroom_bps: 9_990,
            }
            .sizes(&self.account, 1_700_000_000, 10)
        }

        fn closeout(&self) -> bool {
            CrateHealth {
                state: self,
                group: self.group,
            }
            .health(&self.account, 1_700_000_000, 10)
            .unwrap()
            .closeout_allowed
        }
    }

    /// One asset (maintenance weight `asset_weight`) and one liability (weight 1.0), both 1 USD
    /// (Fixed), FeeState cap 10 %; `asset`/`liab` are native amounts (6 decimals).
    fn two_bank(asset_weight: f64, asset: u64, liab: u64) -> TwoBank {
        use marginfi_type_crate::types::{
            BankOperationalState, FeeState, MarginfiGroup, OracleSetup, WrappedI80F48,
        };
        let program: Pubkey = protocol::PROGRAM_ID_MAINNET.parse().unwrap();
        let (group, account, asset_bank, liab_bank) = (
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            Pubkey::new_unique(),
        );
        let num = |v: f64| WrappedI80F48::from(I80F48::from_num(v));
        let data = |disc: [u8; 8], bytes: &[u8]| [disc.as_slice(), bytes].concat();
        let bank = |asset_weight: f64| {
            let mut b: Bank = bytemuck::Zeroable::zeroed();
            b.group = group;
            b.mint = Pubkey::new_unique();
            b.mint_decimals = 6;
            b.asset_share_value = num(1.0);
            b.liability_share_value = num(1.0);
            b.config.asset_weight_init = num(asset_weight);
            b.config.asset_weight_maint = num(asset_weight);
            b.config.liability_weight_init = num(1.0);
            b.config.liability_weight_maint = num(1.0);
            b.config.operational_state = BankOperationalState::Operational;
            b.config.oracle_setup = OracleSetup::Fixed;
            b.config.fixed_price = num(1.0);
            data(protocol::DISC_BANK, bytemuck::bytes_of(&b))
        };
        let mut fee: FeeState = bytemuck::Zeroable::zeroed();
        fee.liquidation_max_fee = num(0.1);
        let mut a: MarginfiAccount = bytemuck::Zeroable::zeroed();
        a.group = group;
        // Pinned Marginfi keeps the balances sorted by bank key, descending.
        let (i, j) = if asset_bank > liab_bank {
            (0, 1)
        } else {
            (1, 0)
        };
        let bal = &mut a.lending_account.balances;
        (bal[i].active, bal[i].bank_pk, bal[i].asset_shares) = (1, asset_bank, num(asset as f64));
        (bal[j].active, bal[j].bank_pk, bal[j].liability_shares) = (1, liab_bank, num(liab as f64));
        let group_data = data(
            protocol::DISC_MARGINFI_GROUP,
            bytemuck::bytes_of(&<MarginfiGroup as bytemuck::Zeroable>::zeroed()),
        );
        TwoBank {
            state: std::collections::HashMap::from([
                (asset_bank, (program, bank(asset_weight))),
                (liab_bank, (program, bank(1.0))),
                (group, (program, group_data)),
                (
                    protocol::fee_state_pda(&program),
                    (
                        program,
                        data(protocol::DISC_FEE_STATE, bytemuck::bytes_of(&fee)),
                    ),
                ),
                (
                    account,
                    (
                        program,
                        data(protocol::DISC_MARGINFI_ACCOUNT, bytemuck::bytes_of(&a)),
                    ),
                ),
            ]),
            program,
            group,
            account,
        }
    }

    /// Q5: sizing failures are typed where they arise — the pair census of a real `sizes()` run
    /// without a valid pair is `NoValidPair` (text unchanged), a pinned-crate error is
    /// `IntegrationStale` only for the pinned stale-reserve codes (text with the reader's marker,
    /// as before), and every `HealthError` maps by its variant.
    #[test]
    fn q5_sizing_errors_are_typed_at_their_source() {
        // Asset and liability both weigh 1.0: no fee factor ≥ 1 heals (coefficient ≤ 0).
        let e = two_bank(1.0, 100_000_000, 110_000_000).sizes().unwrap_err();
        assert_eq!(e.kind, SizingErrorKind::NoValidPair);
        assert!(
            e.text.starts_with(
                "SIZING:NO_VALID_PROTOCOL_PAIR:asset_eq_nonpositive=0;liab_eq_nonpositive=0;\
                 coeff_nonpositive=1;"
            ),
            "{e}"
        );
        let crate_error = |code| {
            SizingError::crate_error(
                "oracle adapter",
                &anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::Custom(code)),
            )
        };
        let stale = crate_error(6206);
        assert_eq!(stale.kind, SizingErrorKind::IntegrationStale);
        assert!(
            stale.text.starts_with("INTEGRATION_STALE:oracle adapter: "),
            "{stale}"
        );
        let other = crate_error(6207);
        assert_eq!(other.kind, SizingErrorKind::Other);
        assert!(other.text.starts_with("oracle adapter: "), "{other}");
        let health = |e: HealthError| {
            let text = e.to_string();
            let typed = SizingError::from(e);
            assert_eq!(typed.text, text);
            typed.kind
        };
        assert_eq!(
            health(HealthError::IntegrationStale("kamino".into())),
            SizingErrorKind::IntegrationStale
        );
        assert_eq!(
            health(HealthError::Unavailable),
            SizingErrorKind::HealthUnavailable
        );
        for e in [
            HealthError::Unsupported("x".into()),
            HealthError::Stale("x".into()),
            HealthError::Crate("x".into()),
            HealthError::MissingAccount(Pubkey::new_unique()),
        ] {
            assert_eq!(health(e), SizingErrorKind::Other);
        }
        // An untyped message stays unclassified, whatever it says.
        assert_eq!(
            SizingError::from("INTEGRATION_STALE:looks typed but is not").kind,
            SizingErrorKind::Other
        );
    }

    /// Z17 (FIX-93) through the production `sizes()` path: one asset (maintenance weight 0.8)
    /// and one liability (weight 1.0), both 1 USD (Fixed), FeeState cap 10 %, headroom 99.9 %.
    /// At equity assets of 5 USD or more the sizing stays within the normal health bound
    /// (post-liquidation maintenance health ≤ 0, seized value ≤ the health cap); below 5 USD
    /// (pinned `LIQUIDATION_CLOSEOUT_DOLLAR_THRESHOLD`, strict `<`) the closeout drops the
    /// health cap: the seized value goes past it and the post-health becomes positive.
    #[test]
    fn normal_and_closeout_bounds_are_distinct() {
        let sized = |asset: u64, liab: u64| {
            let f = two_bank(0.8, asset, liab);
            let mut out = f.sizes().unwrap();
            assert_eq!(out.len(), 1);
            (f.closeout(), out.remove(0))
        };
        // USD micro: the normal health cap -pre / (1/1.1 - 0.8) and the post-liquidation
        // maintenance health pre + repaid - 0.8 × seized (prices 1 USD, micro = native).
        let health_cap = |asset: u64, liab: u64| (liab - asset * 8 / 10) as f64 / (1.0 / 1.1 - 0.8);
        let post = |asset: u64, liab: u64, s: &ProtocolSizing| {
            (asset * 8 / 10) as i128 - liab as i128 + s.repay_equity_value_micro as i128
                - (s.seized_equity_value_micro * 8 / 10) as i128
        };
        // Normal: 100/90 USD and exactly at the threshold, 5/4.5 USD.
        for (asset, liab) in [(100_000_000, 90_000_000), (5_000_000, 4_500_000)] {
            let (closeout, s) = sized(asset, liab);
            assert!(!closeout, "{asset}");
            assert!(
                s.seized_equity_value_micro as f64 <= health_cap(asset, liab),
                "{s:?}"
            );
            assert!(
                post(asset, liab, &s) <= 0,
                "{asset}: {} {s:?}",
                post(asset, liab, &s)
            );
            assert!(s.premium_bps <= 1_000, "{s:?}");
        }
        // Closeout: just below the threshold (4.999999/4.5 USD) and 4/3.6 USD.
        for (asset, liab) in [(4_999_999, 4_500_000), (4_000_000, 3_600_000)] {
            let (closeout, s) = sized(asset, liab);
            assert!(closeout, "{asset}");
            assert!(
                s.seized_equity_value_micro as f64 > health_cap(asset, liab),
                "{s:?}"
            );
            assert!(
                post(asset, liab, &s) > 0,
                "{asset}: {} {s:?}",
                post(asset, liab, &s)
            );
            // Still bounded by the sizer's own conservative liability cap (liability × fee
            // factor) — a strategy bound: under `ignore_healthy` the pinned `liquidate_end.rs`
            // skips the premium check entirely.
            assert!(s.seized_equity_value_micro <= liab * 11 / 10, "{s:?}");
        }
    }
}
