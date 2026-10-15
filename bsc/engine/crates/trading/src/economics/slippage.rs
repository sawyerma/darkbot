//! Minimum-output policy.
//!
//! `min_output = floor(expected * (10000 - slippage_bps) / 10000)`, as in the
//! Gate-0 `g03` variant that bounds the slippage (the four `g05` copies
//! without the bound are not carried over).
//!
//! The result is always strictly positive. A zero quote, a slippage that
//! rounds the minimum to zero, 10,000 bps and anything above are rejected;
//! there is no default and no zero minimum. The slippage value itself is
//! validated runtime policy, not a constant of this crate.

use alloy_primitives::U256;

use super::arith::{ArithError, mul_div_down};
use super::fees::BPS_DENOMINATOR;

/// Reason no admissible minimum output exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlippageError {
    /// Slippage above 10,000 bps.
    InvalidSlippageBps,
    /// The expected output is zero.
    ZeroQuote,
    /// The minimum rounds to zero.
    ZeroMinOutput,
    /// Arithmetic failure.
    Arith(ArithError),
}

impl From<ArithError> for SlippageError {
    fn from(e: ArithError) -> Self {
        Self::Arith(e)
    }
}

impl core::fmt::Display for SlippageError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidSlippageBps => f.write_str("slippage: above 10000 bps"),
            Self::ZeroQuote => f.write_str("slippage: expected output is zero"),
            Self::ZeroMinOutput => f.write_str("slippage: minimum output rounds to zero"),
            Self::Arith(e) => write!(f, "slippage: {e}"),
        }
    }
}

impl std::error::Error for SlippageError {}

/// Strictly positive minimum output for `expected_output` at
/// `slippage_bps`.
pub fn min_output(expected_output: U256, slippage_bps: u16) -> Result<U256, SlippageError> {
    if slippage_bps > BPS_DENOMINATOR {
        return Err(SlippageError::InvalidSlippageBps);
    }
    if expected_output.is_zero() {
        return Err(SlippageError::ZeroQuote);
    }
    let minimum = mul_div_down(
        expected_output,
        U256::from(BPS_DENOMINATOR - slippage_bps),
        U256::from(BPS_DENOMINATOR),
    )?;
    if minimum.is_zero() {
        return Err(SlippageError::ZeroMinOutput);
    }
    Ok(minimum)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(v: u128) -> U256 {
        U256::from(v)
    }

    const DOGE_EXPECTED: u128 = 30_669_575_743_216_741_084_405_716;
    const NATIVE_EXPECTED: u128 = 176_664_965_031_639_901_123_273;

    #[test]
    fn golden_minimums_at_100_bps() {
        // G0.3 `minimum_keeps_exact_integer_bound`.
        assert_eq!(
            min_output(n(DOGE_EXPECTED), 100),
            Ok(n(30_362_879_985_784_573_673_561_658))
        );
        // G0.6a EVM crosscheck `rust_min_output_100bps`.
        assert_eq!(
            min_output(n(NATIVE_EXPECTED), 100),
            Ok(n(174_898_315_381_323_502_112_040))
        );
    }

    #[test]
    fn slippage_bounds_0_1_and_9999_bps() {
        assert_eq!(min_output(n(DOGE_EXPECTED), 0), Ok(n(DOGE_EXPECTED)));
        assert_eq!(
            min_output(n(DOGE_EXPECTED), 1),
            Ok(n(30_666_508_785_642_419_410_297_275))
        );
        assert_eq!(
            min_output(n(DOGE_EXPECTED), 9_999),
            Ok(n(3_066_957_574_321_674_108_440))
        );
    }

    #[test]
    fn ten_thousand_bps_and_above_have_no_admissible_minimum() {
        assert_eq!(
            min_output(n(DOGE_EXPECTED), 10_000),
            Err(SlippageError::ZeroMinOutput)
        );
        assert_eq!(
            min_output(n(DOGE_EXPECTED), 10_001),
            Err(SlippageError::InvalidSlippageBps)
        );
        assert_eq!(
            min_output(n(DOGE_EXPECTED), u16::MAX),
            Err(SlippageError::InvalidSlippageBps)
        );
    }

    #[test]
    fn zero_quote_and_zero_rounded_minimum_are_rejected() {
        assert_eq!(min_output(U256::ZERO, 100), Err(SlippageError::ZeroQuote));
        assert_eq!(min_output(n(1), 1), Err(SlippageError::ZeroMinOutput));
        assert_eq!(
            min_output(n(9_999), 9_999),
            Err(SlippageError::ZeroMinOutput)
        );
        assert_eq!(min_output(n(10_000), 9_999), Ok(n(1)));
    }

    #[test]
    fn maximum_quote_does_not_wrap() {
        assert_eq!(min_output(U256::MAX, 0), Ok(U256::MAX));
        assert!(min_output(U256::MAX, 100).is_ok());
    }
}
