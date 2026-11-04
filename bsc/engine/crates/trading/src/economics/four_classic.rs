//! Four Classic (TokenManager2) native-BNB curve: exact BUY and SELL
//! arithmetic and the curve a launch starts with (R12).
//!
//! The implementations are not source-verified, so every rule here is an
//! execution fact, not a reading of code
//! (`fixtures/fourmeme/classic_parity_20261104.txt`, its header names the
//! calls and blocks): `buyTokenAMAP` and `sellToken` executed on mainnet
//! state (a probe through `eth_call`, the tokens and wei that moved), the
//! launch receipts' creator purchase, executed direct trades, and the
//! venue's quote helper (`Helper3.tryBuy`/`trySell`), which omits the
//! token's tax.
//!
//! The curve is a constant product of `K` over the virtual token reserve
//! `T` (both stored per token, `_tokenInfos`): the virtual quote reserve is
//! `y = K·1e18 / T`. `rate` is the venue's trading fee, `tax` the token's
//! (both in basis points of the curve amount).
//!
//! - BUY of `funds` wei: the budget `c = funds·1e4 / (1e4 + rate + tax)`,
//!   the tokens `floor_gwei(T − ceil(K·1e18 / (y + c)))`, the cost
//!   `K·1e18 / (T − tokens) − y`, the fee `cost·rate / 1e4`, the tax
//!   `cost·tax / 1e4`; the rest of `funds` is refunded. (The closed form
//!   `floor_gwei(T·c / (y + c))`, the helper's, is one gwei-unit more in 7
//!   of the 29 executed probes and equal in every helper row; the new
//!   reserve rounded down or up is the same in every witness, up is the
//!   smaller amount.)
//! - SELL of `tokens`: the curve pays `gross = y − K·1e18 / (T + tokens)`,
//!   less the fee `gross·rate / 1e4` and the tax `gross·tax / 1e4`.
//! - A launch of `totalSupply` S, `maxOffers` O and `maxRaising` R starts
//!   with `a = O·(S−O)·100 / (98·O − 100·(S−O))`, `T = O + a`,
//!   `K = floor(T/1e9) · floor(a·R / O / 1e9)`: the curve whose end price,
//!   after the O offered tokens, is 98 % of `R / (S−O)`. The creator's
//!   BUY inside the launch is a BUY of its `args` creator value (word 10).
//!
//! Every division floors but the BUY's new token reserve (rounded up).
//! Pure functions, typed errors, no admission: they accept any terms and
//! say nothing about whether a token, state or amount is qualified;
//! `venues::fourmeme::profile` gates what is priced.

use alloy_primitives::U256;

use super::arith::{ArithError, add, mul, mul_div_down, mul_div_up, sub};
use super::fees::BPS_DENOMINATOR;

/// Token amounts are whole gwei-units of the 18-decimal token (`1e9`).
const GWEI: u64 = 1_000_000_000;

/// The quote side's fixed point (`1e18`).
const ONE: u64 = 1_000_000_000_000_000_000;

/// The end price of the curve in percent of `maxRaising` over the reserved
/// tokens (the launch rule above).
const END_PRICE_PERCENT: u64 = 98;

/// Why a Classic quote has no result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassicQuoteError {
    /// The input amount is zero.
    ZeroInput,
    /// A state input is zero or inconsistent (no offers, `T` not above the
    /// offers, fee and tax at or above the whole, a launch whose terms give
    /// no curve).
    InvalidState,
    /// The quote is zero tokens or zero wei.
    ZeroOutput,
    /// The BUY takes every remaining offer: TokenManager2 caps it and adds
    /// the liquidity (graduation), which is not quoted.
    ReachesMaxOffers,
    /// A SELL of more tokens than the curve sold.
    ExceedsSold,
    /// A minimum trading fee above zero: no witness says how it combines.
    UnwitnessedMinFee,
    Arith(ArithError),
}

impl From<ArithError> for ClassicQuoteError {
    fn from(e: ArithError) -> Self {
        Self::Arith(e)
    }
}

impl core::fmt::Display for ClassicQuoteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ZeroInput => f.write_str("four classic: input amount is zero"),
            Self::InvalidState => f.write_str("four classic: curve state outside the rule"),
            Self::ZeroOutput => f.write_str("four classic: output is zero"),
            Self::ReachesMaxOffers => f.write_str("four classic: takes every remaining offer"),
            Self::ExceedsSold => f.write_str("four classic: sells more than the curve sold"),
            Self::UnwitnessedMinFee => f.write_str("four classic: minimum trading fee unwitnessed"),
            Self::Arith(e) => write!(f, "four classic: {e}"),
        }
    }
}

impl std::error::Error for ClassicQuoteError {}

/// The terms of a Classic launch (its `createToken` arguments).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchTerms {
    pub total_supply: U256,
    pub max_offers: U256,
    pub max_raising: U256,
}

/// One token's curve (`_tokenInfos`: `K`, `T`, `offers`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicCurve {
    pub k: U256,
    /// The virtual token reserve.
    pub t: U256,
    /// Tokens still offered on the curve.
    pub offers: U256,
    /// Tokens the curve sold (`maxOffers − offers`).
    pub sold: U256,
}

/// The fee terms of a token: the venue's (`Helper3.getTokenInfo`) and the
/// token's tax (its template word's, measured by execution).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicFee {
    /// `tradingFeeRate`, in basis points.
    pub rate_bps: u16,
    /// `minTradingFee`, in wei.
    pub min_fee: U256,
    /// The token's tax on either side, in basis points.
    pub tax_bps: u16,
}

/// A BUY on the curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicBuyQuote {
    /// Tokens received; strictly positive, below the remaining offers.
    pub amount: U256,
    /// Wei the curve takes for them.
    pub cost: U256,
    /// Trading fee, in wei.
    pub fee: U256,
    /// The token's tax, in wei.
    pub tax: U256,
    /// Wei the BUY takes (`cost + fee + tax`); the rest of the funds is
    /// refunded.
    pub spent: U256,
    /// The curve after the BUY.
    pub after: ClassicCurve,
}

/// A SELL on the curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicSellQuote {
    /// Wei the curve pays before the fee and the tax.
    pub gross: U256,
    /// Trading fee, in wei.
    pub fee: U256,
    /// The token's tax, in wei.
    pub tax: U256,
    /// Wei the seller receives; strictly positive.
    pub funds: U256,
    /// The curve after the SELL.
    pub after: ClassicCurve,
}

fn floor_gwei(x: U256) -> U256 {
    let g = U256::from(GWEI);
    x / g * g
}

/// `floor(x · bps / 1e4)`.
fn part(x: U256, bps: u16) -> Result<U256, ClassicQuoteError> {
    Ok(mul_div_down(
        x,
        U256::from(bps),
        U256::from(BPS_DENOMINATOR),
    )?)
}

/// `K·1e18 / T`: the virtual quote reserve.
fn virtual_quote(curve: &ClassicCurve) -> Result<U256, ClassicQuoteError> {
    Ok(mul_div_down(curve.k, U256::from(ONE), curve.t)?)
}

fn checked(curve: &ClassicCurve) -> Result<(), ClassicQuoteError> {
    if curve.k.is_zero() || curve.offers.is_zero() || curve.t <= curve.offers {
        return Err(ClassicQuoteError::InvalidState);
    }
    Ok(())
}

/// The fee and the tax together, in basis points, below the whole.
fn charged_bps(fee: &ClassicFee) -> Result<u16, ClassicQuoteError> {
    if !fee.min_fee.is_zero() {
        return Err(ClassicQuoteError::UnwitnessedMinFee);
    }
    match fee.rate_bps.checked_add(fee.tax_bps) {
        Some(charged) if charged < BPS_DENOMINATOR => Ok(charged),
        _ => Err(ClassicQuoteError::InvalidState),
    }
}

/// The curve a launch with `terms` starts with.
pub fn launch_curve(terms: &LaunchTerms) -> Result<ClassicCurve, ClassicQuoteError> {
    let offers = terms.max_offers;
    let reserved = sub(terms.total_supply, offers).map_err(|_| ClassicQuoteError::InvalidState)?;
    if offers.is_zero() || reserved.is_zero() || terms.max_raising.is_zero() {
        return Err(ClassicQuoteError::InvalidState);
    }
    let hundred = U256::from(100u8);
    let scaled_offers = mul(U256::from(END_PRICE_PERCENT), offers)?;
    let scaled_reserved = mul(hundred, reserved)?;
    if scaled_offers <= scaled_reserved {
        return Err(ClassicQuoteError::InvalidState);
    }
    let a = mul_div_down(
        mul(offers, reserved)?,
        hundred,
        scaled_offers - scaled_reserved,
    )?;
    let t = add(offers, a)?;
    let quote = mul_div_down(a, terms.max_raising, offers)?;
    let g = U256::from(GWEI);
    let k = mul(t / g, quote / g)?;
    let curve = ClassicCurve {
        k,
        t,
        offers,
        sold: U256::ZERO,
    };
    checked(&curve)?;
    Ok(curve)
}

/// The BUY `buyTokenAMAP(token, funds, minAmount)` with `msg.value = funds`
/// makes (and the creator's BUY inside `createToken` with its value):
/// `spent <= funds`, the rest refunded.
pub fn quote_buy(
    curve: &ClassicCurve,
    fee: &ClassicFee,
    funds: U256,
) -> Result<ClassicBuyQuote, ClassicQuoteError> {
    if funds.is_zero() {
        return Err(ClassicQuoteError::ZeroInput);
    }
    let charged = charged_bps(fee)?;
    checked(curve)?;
    let whole = U256::from(BPS_DENOMINATOR);
    let budget = mul_div_down(funds, whole, whole + U256::from(charged))?;
    let y = virtual_quote(curve)?;
    // The virtual token reserve after the BUY, rounded up.
    let reserve = mul_div_up(curve.k, U256::from(ONE), add(y, budget)?)?;
    let amount = if reserve < curve.t {
        floor_gwei(curve.t - reserve)
    } else {
        U256::ZERO
    };
    if amount.is_zero() {
        return Err(ClassicQuoteError::ZeroOutput);
    }
    if amount >= curve.offers {
        return Err(ClassicQuoteError::ReachesMaxOffers);
    }
    let t = curve.t - amount;
    let cost = sub(mul_div_down(curve.k, U256::from(ONE), t)?, y)?;
    if cost.is_zero() {
        return Err(ClassicQuoteError::ZeroOutput);
    }
    let fee_wei = part(cost, fee.rate_bps)?;
    let tax = part(cost, fee.tax_bps)?;
    let spent = add(add(cost, fee_wei)?, tax)?;
    if spent > funds {
        return Err(ClassicQuoteError::InvalidState);
    }
    Ok(ClassicBuyQuote {
        amount,
        cost,
        fee: fee_wei,
        tax,
        spent,
        after: ClassicCurve {
            k: curve.k,
            t,
            offers: curve.offers - amount,
            sold: add(curve.sold, amount)?,
        },
    })
}

/// The SELL of `amount` tokens (`sellToken(token, amount)`): at most what
/// the curve sold is taken back.
pub fn quote_sell(
    curve: &ClassicCurve,
    fee: &ClassicFee,
    amount: U256,
) -> Result<ClassicSellQuote, ClassicQuoteError> {
    if amount.is_zero() {
        return Err(ClassicQuoteError::ZeroInput);
    }
    charged_bps(fee)?;
    checked(curve)?;
    if amount > curve.sold {
        return Err(ClassicQuoteError::ExceedsSold);
    }
    let y = virtual_quote(curve)?;
    let t = add(curve.t, amount)?;
    let gross = sub(y, mul_div_down(curve.k, U256::from(ONE), t)?)?;
    let fee_wei = part(gross, fee.rate_bps)?;
    let tax = part(gross, fee.tax_bps)?;
    let funds = sub(gross, add(fee_wei, tax)?)?;
    if funds.is_zero() {
        return Err(ClassicQuoteError::ZeroOutput);
    }
    Ok(ClassicSellQuote {
        gross,
        fee: fee_wei,
        tax,
        funds,
        after: ClassicCurve {
            k: curve.k,
            t,
            offers: add(curve.offers, amount)?,
            sold: curve.sold - amount,
        },
    })
}

#[cfg(test)]
mod tests;
