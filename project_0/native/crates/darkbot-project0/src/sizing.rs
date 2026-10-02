//! Exact receivership sizing from the pinned marginfi protocol implementation.
//!
//! This module deliberately stops at protocol quantities/values. It does **not** call a DEX and
//! does not claim oracle value is realizable market value. P4A-01 must value the resulting seized
//! amount through an executable route.

use crate::health::{crate_impl::bank_observation_keys, AccountSource, CrateHealth, HealthEngine};
use crate::protocol;
use fixed::types::I80F48;
use marginfi::state::{
    bank::BankImpl,
    marginfi_account::{calc_amount, calc_value},
    price::{OraclePriceFeedAdapter, PriceAdapter},
};
use marginfi_type_crate::constants::{ASSET_TAG_DEFAULT, ASSET_TAG_SOL};
use marginfi_type_crate::types::{
    reconcile_emode_configs, BalanceSide, Bank, BankOperationalState, EmodeConfig, FeeState,
    MarginfiAccount, OraclePriceType, PriceBias, RequirementType,
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
    /// Native token units passed to LendingAccountRepay.
    pub repay_amount: u64,
    /// Equity/EMA values after native-unit rounding, in USD-micro.
    pub seized_equity_value_micro: u64,
    pub repay_equity_value_micro: u64,
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
    equity_value: I80F48,
    maintenance_unweighted_value: I80F48,
}

fn load<T: bytemuck::Pod>(data: &[u8]) -> Result<&T, String> {
    let payload = data.get(8..).ok_or_else(|| "short account".to_string())?;
    let n = std::mem::size_of::<T>();
    let slice = payload.get(..n).ok_or_else(|| format!("payload {} < {}", payload.len(), n))?;
    bytemuck::try_from_bytes::<T>(slice).map_err(|e| format!("pod cast: {e:?}"))
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

fn supported_generic_bank(bank: &Bank) -> bool {
    matches!(bank.config.asset_tag, ASSET_TAG_DEFAULT | ASSET_TAG_SOL)
        && matches!(
            bank.config.operational_state,
            BankOperationalState::Operational
                | BankOperationalState::ReduceOnly
                | BankOperationalState::ReduceOnlyWithBorrowingPower
        )
}

impl<'a, S: AccountSource + ?Sized> ProtocolSizer<'a, S> {
    fn raw(&self, key: &Pubkey) -> Result<(Pubkey, u64, &'a [u8]), String> {
        self.state.account(key).ok_or_else(|| format!("missing state account {key}"))
    }

    fn bank(&self, key: &Pubkey) -> Result<Bank, String> {
        let (owner, _, data) = self.raw(key)?;
        if owner != self.program {
            return Err(format!("UNKNOWN_LAYOUT: bank {key} owner {owner}"));
        }
        if protocol::classify(data) != protocol::AccountType::Bank {
            return Err(format!("UNKNOWN_LAYOUT: {key} is not Bank"));
        }
        Ok(*load::<Bank>(data)?)
    }

    fn fee_snapshot_and_rate(&self) -> Result<(FeeSnapshot, I80F48), String> {
        let key = protocol::fee_state_pda(&self.program);
        let (owner, _, data) = self.raw(&key)?;
        if owner != self.program || protocol::classify(data) != protocol::AccountType::FeeState {
            return Err("UNKNOWN_LAYOUT: invalid FeeState".into());
        }
        let fee = load::<FeeState>(data)?;
        let f: I80F48 = fee.liquidation_max_fee.into();
        Ok((FeeSnapshot {
            global_fee_wallet: fee.global_fee_wallet,
            liquidation_max_bps: fee_bps(f)?,
            liquidation_flat_sol_fee_lamports: fee.liquidation_flat_sol_fee as u64,
        }, f))
    }

    pub fn fee_snapshot(&self) -> Result<FeeSnapshot, String> {
        self.fee_snapshot_and_rate().map(|(snapshot, _)| snapshot)
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
            .get_price_of_type(OraclePriceType::TimeWeighted, None, bank.config.oracle_max_confidence)
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
        Ok(Prices { equity, maint_asset, maint_liab })
    }

    fn positions(
        &self,
        account: &MarginfiAccount,
        clock: &Clock,
    ) -> Result<(Vec<Position>, Vec<Position>, Vec<EmodeConfig>), String> {
        let mut assets = Vec::new();
        let mut liabs = Vec::new();
        let mut liability_emodes = Vec::new();
        for balance in &account.lending_account.balances {
            if !balance.is_active() {
                continue;
            }
            let Some(side) = balance.get_side() else { continue };
            let bank = self.bank(&balance.bank_pk)?;
            // [FIX-59] Until integration withdraw/refresh builders are wired, fail closed by not
            // generating generic-withdraw candidates for integration banks.
            if !supported_generic_bank(&bank) || bank.check_utilization_ratio().is_err() {
                continue;
            }
            let prices = self.prices(&bank, clock)?;
            let (amount, maint_price, is_asset) = match side {
                BalanceSide::Assets => (
                    bank.get_asset_amount(balance.asset_shares.into())
                        .map_err(|e| format!("asset amount: {e}"))?,
                    prices.maint_asset,
                    true,
                ),
                BalanceSide::Liabilities => (
                    bank.get_liability_amount(balance.liability_shares.into())
                        .map_err(|e| format!("liability amount: {e}"))?,
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
    pub fn sizes(&self, account_key: &Pubkey, now_unix_s: i64, slot: u64) -> Result<Vec<ProtocolSizing>, String> {
        if self.headroom_bps == 0 || self.headroom_bps > BPS {
            return Err(format!("headroom_bps must be 1..={BPS}"));
        }
        let (_, _, data) = self.raw(account_key)?;
        if protocol::classify(data) != protocol::AccountType::MarginfiAccount {
            return Err("not a MarginfiAccount".into());
        }
        let account = load::<MarginfiAccount>(data)?;
        let health = CrateHealth { state: self.state, group: self.group }
            .health(account_key, now_unix_s, slot)
            .map_err(|e| e.to_string())?;
        if !health.liquidatable || health.maintenance_health >= 0 {
            return Ok(Vec::new());
        }
        let pre_health = I80F48::from_bits(health.maintenance_health);
        let (fee, onchain_fee_rate) = self.fee_snapshot_and_rate()?;
        let fee_rate = {
            let minimum = I80F48::from_num(protocol::LIQUIDATION_BONUS_FEE_MINIMUM_BPS)
                .checked_div(I80F48::from_num(BPS))
                .ok_or_else(|| "minimum fee division".to_string())?;
            if onchain_fee_rate > minimum { onchain_fee_rate } else { minimum }
        };
        let fee_factor = I80F48::ONE
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
        let reconciled = reconcile_emode_configs(emodes.into_iter(), RequirementType::Maintenance);
        let headroom = I80F48::from_num(self.headroom_bps)
            .checked_div(I80F48::from_num(BPS))
            .ok_or_else(|| "headroom division".to_string())?;
        let mut out = Vec::new();

        for asset in &assets {
            for liab in &liabs {
                if asset.equity_value <= I80F48::ZERO || liab.equity_value <= I80F48::ZERO {
                    continue;
                }
                let asset_weight = asset.bank.get_asset_weight(RequirementType::Maintenance, &reconciled);
                let liab_weight: I80F48 = liab.bank.config.liability_weight_maint.into();
                let asset_ratio = asset
                    .maintenance_unweighted_value
                    .checked_div(asset.equity_value)
                    .ok_or_else(|| "asset maintenance/equity ratio".to_string())?;
                let liab_ratio = liab
                    .maintenance_unweighted_value
                    .checked_div(liab.equity_value)
                    .ok_or_else(|| "liability maintenance/equity ratio".to_string())?;
                // Per $1 of equity-value seized at the protocol max premium, this is the change
                // in maintenance health. It must be positive for the pair to heal the account.
                let coeff = liab_ratio
                    .checked_mul(liab_weight)
                    .and_then(|x| x.checked_div(fee_factor))
                    .and_then(|x| x.checked_sub(asset_ratio.checked_mul(asset_weight)?))
                    .ok_or_else(|| "health coefficient overflow".to_string())?;
                if coeff <= I80F48::ZERO {
                    continue;
                }
                let health_cap = (-pre_health)
                    .checked_div(coeff)
                    .ok_or_else(|| "health cap division".to_string())?;
                let liab_cap = liab.equity_value
                    .checked_mul(fee_factor)
                    .ok_or_else(|| "liability cap overflow".to_string())?;
                let mut seize_value = asset.equity_value.min(liab_cap).min(health_cap);
                seize_value = seize_value
                    .checked_mul(headroom)
                    .ok_or_else(|| "headroom multiplication".to_string())?;
                if seize_value <= I80F48::ZERO {
                    continue;
                }

                let asset_prices = self.prices(&asset.bank, &clock)?;
                let liab_prices = self.prices(&liab.bank, &clock)?;
                let withdraw_exact = calc_amount(
                    seize_value,
                    asset_prices.equity,
                    asset.bank.mint_decimals,
                )
                .map_err(|e| format!("withdraw amount: {e}"))?;
                let withdraw_amount = floor_u64(withdraw_exact)?;
                if withdraw_amount == 0 {
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
                let repay_exact = calc_amount(
                    repay_target,
                    liab_prices.equity,
                    liab.bank.mint_decimals,
                )
                .map_err(|e| format!("repay amount: {e}"))?;
                let repay_amount = ceil_u64(repay_exact)?;
                if repay_amount == 0 {
                    continue;
                }
                // Never plan more than the actual liability native amount.
                if I80F48::from_num(repay_amount) > liab.amount {
                    continue;
                }
                let repay_actual = calc_value(
                    I80F48::from_num(repay_amount),
                    liab_prices.equity,
                    liab.bank.get_balance_decimals(),
                    None,
                )
                .map_err(|e| format!("rounded repay value: {e}"))?;
                if repay_actual <= I80F48::ZERO {
                    continue;
                }
                // Rounding must remain within the exact EndLiquidation premium bound.
                let max_seized = repay_actual
                    .checked_mul(fee_factor)
                    .ok_or_else(|| "rounded premium bound overflow".to_string())?;
                if seized_actual > max_seized {
                    continue;
                }
                let premium = seized_actual
                    .checked_div(repay_actual)
                    .and_then(|x| x.checked_sub(I80F48::ONE))
                    .unwrap_or(I80F48::ZERO)
                    .max(I80F48::ZERO);
                out.push(ProtocolSizing {
                    asset_bank: asset.bank_pk,
                    liab_bank: liab.bank_pk,
                    withdraw_amount,
                    repay_amount,
                    seized_equity_value_micro: usd_micro(seized_actual)?,
                    repay_equity_value_micro: usd_micro(repay_actual)?,
                    premium_bps: fee_bps(premium)?,
                    fee_state_max_bps: fee.liquidation_max_bps,
                    flat_sol_fee_lamports: fee.liquidation_flat_sol_fee_lamports,
                });
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_conversions_are_conservative() {
        assert_eq!(floor_u64(I80F48::from_num(12) + I80F48::from_num(1) / I80F48::from_num(2)).unwrap(), 12);
        assert_eq!(ceil_u64(I80F48::from_num(12) + I80F48::from_num(1) / I80F48::from_num(2)).unwrap(), 13);
        assert_eq!(usd_micro(I80F48::from_num(3) / I80F48::from_num(2)).unwrap(), 1_500_000);
    }

    #[test]
    fn headroom_is_below_boundary() {
        let h = I80F48::from_num(DEFAULT_HEADROOM_BPS) / I80F48::from_num(BPS);
        assert!(h < I80F48::ONE && h > I80F48::from_num(99) / I80F48::from_num(100));
    }
}
