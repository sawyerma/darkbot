//! Flap bonding-curve supply function.
//!
//! `LibCurve::estimateSupply` semantics, as reproduced by both witnessed
//! profiles:
//!
//! ```text
//! supply(reserve) = TOTAL_SUPPLY + h - ceil(k * 1e18 / (r + reserve))
//! ```
//!
//! `reserve` is the quote reserve in 18-decimal units.

use alloy_primitives::U256;

use super::QuoteError;
use crate::economics::arith::{add, mul_div_up};

/// 1e18.
pub(crate) const WAD: u128 = 1_000_000_000_000_000_000;

/// Total token supply: 1e9 tokens with 18 decimals.
pub(crate) const TOTAL_SUPPLY: u128 = 1_000_000_000_000_000_000_000_000_000;

/// Curve parameters in 18-decimal fixed point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Curve {
    /// Virtual quote reserve.
    pub r: U256,
    /// Virtual token offset.
    pub h: U256,
    /// Curve constant.
    pub k: U256,
}

/// Circulating supply at quote reserve `reserve_wad`.
pub fn supply(curve: Curve, reserve_wad: U256) -> Result<U256, QuoteError> {
    let total_plus_h = add(U256::from(TOTAL_SUPPLY), curve.h)?;
    let virtual_token_reserve = mul_div_up(curve.k, U256::from(WAD), add(curve.r, reserve_wad)?)?;
    if virtual_token_reserve > total_plus_h {
        return Err(QuoteError::InvalidCurveState);
    }
    Ok(total_plus_h - virtual_token_reserve)
}
