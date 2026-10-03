//! Fee split with the witnessed Flap rounding.
//!
//! ```text
//! combined_fee = floor(q * (protocol_bps + effective_tax_bps) / 10000)
//! protocol_fee = floor(q * protocol_bps / 10000)
//! tax_fee      = combined_fee - protocol_fee
//! ```
//!
//! The tax is the remainder of the combined fee, not an independent floor:
//! at 100/100 bps it can exceed `floor(q / 100)` by one unit (G0.3 fixture:
//! protocol 1208801652, tax 1208801653).

use alloy_primitives::U256;

use super::arith::{ArithError, mul_div_down};

/// Basis-point denominator.
pub const BPS_DENOMINATOR: u16 = 10_000;

/// Fees charged on a quote-side amount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeSplit {
    /// Protocol fee plus tax.
    pub combined: U256,
    /// Protocol fee.
    pub protocol: U256,
    /// Tax: the remainder of the combined fee.
    pub tax: U256,
}

/// Reason a fee split is not defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeeError {
    /// Protocol fee plus tax is 100 % or more.
    RateTooHigh,
    /// Arithmetic failure.
    Arith(ArithError),
}

impl From<ArithError> for FeeError {
    fn from(e: ArithError) -> Self {
        Self::Arith(e)
    }
}

impl core::fmt::Display for FeeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RateTooHigh => f.write_str("fees: combined rate is 100 % or more"),
            Self::Arith(e) => write!(f, "fees: {e}"),
        }
    }
}

impl std::error::Error for FeeError {}

/// Splits the fees on `amount` at `protocol_bps` and `effective_tax_bps`.
pub fn split_fee(
    amount: U256,
    protocol_bps: u16,
    effective_tax_bps: u16,
) -> Result<FeeSplit, FeeError> {
    let combined_bps = u32::from(protocol_bps) + u32::from(effective_tax_bps);
    if combined_bps >= u32::from(BPS_DENOMINATOR) {
        return Err(FeeError::RateTooHigh);
    }
    let denominator = U256::from(BPS_DENOMINATOR);
    let combined = mul_div_down(amount, U256::from(combined_bps), denominator)?;
    let protocol = mul_div_down(amount, U256::from(protocol_bps), denominator)?;
    // combined >= protocol because combined_bps >= protocol_bps.
    Ok(FeeSplit {
        combined,
        protocol,
        tax: combined - protocol,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g03_fixture_tax_exceeds_one_percent_floor_by_one() {
        let split = split_fee(U256::from(120_880_165_276u64), 100, 100).unwrap();
        assert_eq!(split.protocol, U256::from(1_208_801_652u64));
        assert_eq!(split.tax, U256::from(1_208_801_653u64));
        assert_eq!(split.combined, U256::from(2_417_603_305u64));
    }

    #[test]
    fn native_fixture_splits_evenly() {
        let split = split_fee(U256::from(1_000_000_000_000_000u64), 100, 100).unwrap();
        assert_eq!(split.protocol, U256::from(10_000_000_000_000u64));
        assert_eq!(split.tax, U256::from(10_000_000_000_000u64));
    }

    #[test]
    fn zero_tax_and_zero_amount() {
        let split = split_fee(U256::from(999u64), 100, 0).unwrap();
        assert_eq!(split.protocol, U256::from(9u64));
        assert_eq!(split.tax, U256::ZERO);
        let zero = split_fee(U256::ZERO, 100, 100).unwrap();
        assert_eq!(zero.combined, U256::ZERO);
    }

    #[test]
    fn combined_rate_of_one_hundred_percent_is_rejected() {
        assert_eq!(
            split_fee(U256::from(1u64), 5_000, 5_000),
            Err(FeeError::RateTooHigh)
        );
        assert_eq!(
            split_fee(U256::from(1u64), u16::MAX, u16::MAX),
            Err(FeeError::RateTooHigh)
        );
        assert!(split_fee(U256::from(1u64), 4_999, 5_000).is_ok());
    }

    #[test]
    fn maximum_amount_does_not_wrap() {
        let split = split_fee(U256::MAX, 100, 100).unwrap();
        assert_eq!(split.combined, U256::MAX / U256::from(50u64));
        assert_eq!(split.protocol, U256::MAX / U256::from(100u64));
    }
}
