//! Validated operating policy of a BUY.
//!
//! The values are operating configuration: they come from the runner's
//! validated configuration and carry the generation under which they were
//! validated. This crate defines no defaults.
//!
//! The generation is what makes a policy change effective. Admission, the
//! expectation and the final check before send all compare it with the
//! generation of the state snapshot they work on, so a quote, an
//! expectation or prepared signed bytes built under an older policy cannot
//! be sent after the policy changed.

use alloy_primitives::U256;

use super::fees::BPS_DENOMINATOR;

/// Reason a policy is not valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    /// The input amount is zero.
    ZeroInputAmount,
    /// Slippage of 10,000 bps or more can never yield a positive minimum.
    InvalidSlippageBps,
    /// Gas price is zero.
    ZeroGasPrice,
    /// Gas limit is zero.
    ZeroGasLimit,
}

impl core::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ZeroInputAmount => "buy policy: input amount is zero",
            Self::InvalidSlippageBps => "buy policy: slippage is 10000 bps or more",
            Self::ZeroGasPrice => "buy policy: gas price is zero",
            Self::ZeroGasLimit => "buy policy: gas limit is zero",
        })
    }
}

impl std::error::Error for PolicyError {}

/// The signature-bound operating values of a BUY and their generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyPolicy {
    generation: u64,
    input_amount: U256,
    slippage_bps: u16,
    gas_price: u64,
    gas_limit: u64,
}

impl BuyPolicy {
    /// Validates the operating values of generation `generation`.
    pub fn new(
        generation: u64,
        input_amount: U256,
        slippage_bps: u16,
        gas_price: u64,
        gas_limit: u64,
    ) -> Result<Self, PolicyError> {
        if input_amount.is_zero() {
            return Err(PolicyError::ZeroInputAmount);
        }
        if slippage_bps >= BPS_DENOMINATOR {
            return Err(PolicyError::InvalidSlippageBps);
        }
        if gas_price == 0 {
            return Err(PolicyError::ZeroGasPrice);
        }
        if gas_limit == 0 {
            return Err(PolicyError::ZeroGasLimit);
        }
        Ok(Self {
            generation,
            input_amount,
            slippage_bps,
            gas_price,
            gas_limit,
        })
    }

    /// Generation of the validated configuration these values belong to.
    #[inline]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Native amount to spend, in wei.
    #[inline]
    pub fn input_amount(&self) -> U256 {
        self.input_amount
    }

    /// Slippage in basis points.
    #[inline]
    pub fn slippage_bps(&self) -> u16 {
        self.slippage_bps
    }

    /// Gas price in wei.
    #[inline]
    pub fn gas_price(&self) -> u64 {
        self.gas_price
    }

    /// Gas limit.
    #[inline]
    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_policy_keeps_its_values_and_generation() {
        let policy = BuyPolicy::new(3, U256::from(1000u64), 100, 310_000_000, 1_500_000).unwrap();
        assert_eq!(policy.generation(), 3);
        assert_eq!(policy.input_amount(), U256::from(1000u64));
        assert_eq!(policy.slippage_bps(), 100);
        assert_eq!(policy.gas_price(), 310_000_000);
        assert_eq!(policy.gas_limit(), 1_500_000);
        assert!(BuyPolicy::new(3, U256::from(1u64), 0, 1, 1).is_ok());
        assert!(BuyPolicy::new(3, U256::from(1u64), 9_999, 1, 1).is_ok());
    }

    #[test]
    fn invalid_values_are_rejected() {
        let one = U256::from(1u64);
        assert_eq!(
            BuyPolicy::new(1, U256::ZERO, 100, 1, 1),
            Err(PolicyError::ZeroInputAmount)
        );
        assert_eq!(
            BuyPolicy::new(1, one, 10_000, 1, 1),
            Err(PolicyError::InvalidSlippageBps)
        );
        assert_eq!(
            BuyPolicy::new(1, one, u16::MAX, 1, 1),
            Err(PolicyError::InvalidSlippageBps)
        );
        assert_eq!(
            BuyPolicy::new(1, one, 100, 0, 1),
            Err(PolicyError::ZeroGasPrice)
        );
        assert_eq!(
            BuyPolicy::new(1, one, 100, 1, 0),
            Err(PolicyError::ZeroGasLimit)
        );
    }
}
