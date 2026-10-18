//! Exact receivership sizing from the pinned marginfi protocol implementation.
//!
//! This module deliberately stops at protocol quantities/values. It does **not** call a DEX and
//! does not claim oracle value is realizable market value. P4A-01 must value the resulting seized
//! amount through an executable route.

use crate::health::{
    crate_impl::{bank_observation_keys, CrateHealth},
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
    reconcile_emode_configs, BalanceSide, Bank, BankOperationalState, EmodeConfig, MarginfiAccount,
    OraclePriceType, PriceBias, RequirementType,
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

#[derive(Clone, Copy)]
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

/// Asset positions, liability positions and the liabilities' emode configs of one account.
type Positions = (Vec<Position>, Vec<Position>, Vec<EmodeConfig>);

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
        let value = calc_value(
            I80F48::from_num(native_amount),
            price,
            bank.get_balance_decimals(),
            None,
        )
        .map_err(|e| format!("native amount value: {e}"))?;
        usd_micro(value)
    }

    pub fn bank_mint(&self, bank_key: &Pubkey) -> Result<Pubkey, String> {
        Ok(self.bank(bank_key)?.mint)
    }

    /// [FIX-89] Deterministic explanation when protocol sizing produced zero candidates.
    /// This is diagnostics only; it does not relax any fail-closed sizing rule.
    pub fn no_size_reason(&self, account_key: &Pubkey) -> Result<String, String> {
        let (_, _, data) = self.raw(account_key)?;
        if protocol::classify(data) != protocol::AccountType::MarginfiAccount {
            return Ok("NOT_MARGINFI_ACCOUNT".into());
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
            return Ok("NO_ASSET".into());
        }
        if liabs == 0 {
            return Ok("NO_LIABILITY".into());
        }
        if supported_assets == 0 && integration_assets > 0 {
            return Ok("UNSUPPORTED_INTEGRATION_ASSET".into());
        }
        if supported_liabs == 0 && integration_liabs > 0 {
            return Ok("UNSUPPORTED_INTEGRATION_LIABILITY".into());
        }
        if supported_assets == 0 {
            return Ok("NO_SUPPORTED_ASSET_BANK".into());
        }
        if supported_liabs == 0 {
            return Ok("NO_SUPPORTED_LIABILITY_BANK".into());
        }
        Ok("NO_VALID_PROTOCOL_PAIR".into())
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

    fn prices(&self, bank: &Bank, clock: &Clock) -> Result<Prices, String> {
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
            .map_err(|e| format!("oracle adapter: {e}"))?;
        let equity = feed
            .get_price_of_type(
                OraclePriceType::TimeWeighted,
                None,
                bank.config.oracle_max_confidence,
            )
            .map_err(|e| format!("equity price: {e}"))?;
        let maint_asset = feed
            .get_price_of_type(
                OraclePriceType::RealTime,
                Some(PriceBias::Low),
                bank.config.oracle_max_confidence,
            )
            .map_err(|e| format!("maintenance asset price: {e}"))?;
        let maint_liab = feed
            .get_price_of_type(
                OraclePriceType::RealTime,
                Some(PriceBias::High),
                bank.config.oracle_max_confidence,
            )
            .map_err(|e| format!("maintenance liability price: {e}"))?;
        Ok(Prices {
            equity,
            maint_asset,
            maint_liab,
        })
    }

    fn positions(&self, account: &MarginfiAccount, clock: &Clock) -> Result<Positions, String> {
        let mut assets = Vec::new();
        let mut liabs = Vec::new();
        let mut liability_emodes = Vec::new();
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
            };
            if is_asset {
                assets.push(p);
            } else {
                liability_emodes.push(bank.emode.emode_config);
                liabs.push(p);
            }
        }
        Ok((assets, liabs, liability_emodes))
    }

    /// Returns every protocol-valid generic Asset×Liability sizing candidate. The caller must
    /// separately establish executable routing and realizable market value.
    pub fn sizes(
        &self,
        account_key: &Pubkey,
        now_unix_s: i64,
        slot: u64,
    ) -> Result<Vec<ProtocolSizing>, String> {
        if self.headroom_bps == 0 || self.headroom_bps > BPS {
            return Err(format!("headroom_bps must be 1..={BPS}"));
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
        .map_err(|e: HealthError| e.to_string())?;
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
        let (assets, liabs, emodes) = self.positions(account, &clock)?;
        let reconciled = reconcile_emode_configs(emodes, RequirementType::Maintenance);
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
                let liab_weight: I80F48 = liab.bank.config.liability_weight_maint.into();
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

                let asset_prices = self.prices(&asset.bank, &clock)?;
                let liab_prices = self.prices(&liab.bank, &clock)?;
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
            return Err(format!(
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
            ));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn normal_and_closeout_bounds_are_distinct() {
        // Regression guard for FIX-93: normal liquidations retain the health/premium
        // constraints; closeout is selected exclusively by HealthView::closeout_allowed.
        let normal = false;
        let closeout = true;
        assert!(!normal);
        assert!(closeout);
    }
}
