//! Overflow-safe 256-bit arithmetic.
//!
//! Consolidates the `wide` / `narrow` / `mul_div` / `ceil_div` copies of the
//! Gate-0 harness (`g03`, `native_bnb_quote` and four `g05` binaries). The
//! Gate-0 versions asserted; these return an error. Products are formed in
//! 512 bits, so an intermediate product never wraps.

use alloy_primitives::{U256, U512};

/// Reason an exact integer operation has no representable result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArithError {
    /// Division by zero.
    DivisionByZero,
    /// The result does not fit in 256 bits.
    Overflow,
    /// The subtrahend exceeds the minuend.
    Underflow,
}

impl core::fmt::Display for ArithError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::DivisionByZero => "arith: division by zero",
            Self::Overflow => "arith: result exceeds 256 bits",
            Self::Underflow => "arith: subtraction below zero",
        })
    }
}

impl std::error::Error for ArithError {}

/// Widens to 512 bits.
#[inline]
pub fn wide(x: U256) -> U512 {
    U512::from(x)
}

/// Narrows to 256 bits, failing instead of truncating.
#[inline]
pub fn narrow(x: U512) -> Result<U256, ArithError> {
    if x > U512::from(U256::MAX) {
        Err(ArithError::Overflow)
    } else {
        Ok(U256::from(x))
    }
}

/// `a + b`.
#[inline]
pub fn add(a: U256, b: U256) -> Result<U256, ArithError> {
    narrow(wide(a) + wide(b))
}

/// `a - b`.
#[inline]
pub fn sub(a: U256, b: U256) -> Result<U256, ArithError> {
    if a < b {
        Err(ArithError::Underflow)
    } else {
        Ok(a - b)
    }
}

/// `a * b`.
#[inline]
pub fn mul(a: U256, b: U256) -> Result<U256, ArithError> {
    narrow(wide(a) * wide(b))
}

/// `floor(a * b / d)` with a 512-bit intermediate product.
#[inline]
pub fn mul_div_down(a: U256, b: U256, d: U256) -> Result<U256, ArithError> {
    if d.is_zero() {
        return Err(ArithError::DivisionByZero);
    }
    narrow(wide(a) * wide(b) / wide(d))
}

/// `ceil(a * b / d)` with a 512-bit intermediate product.
#[inline]
pub fn mul_div_up(a: U256, b: U256, d: U256) -> Result<U256, ArithError> {
    if d.is_zero() {
        return Err(ArithError::DivisionByZero);
    }
    let numerator = wide(a) * wide(b);
    let denominator = wide(d);
    let quotient = numerator / denominator;
    if numerator % denominator == U512::ZERO {
        narrow(quotient)
    } else {
        narrow(quotient + U512::ONE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widened_product_does_not_wrap() {
        // Gate-0 g03 `widened_product_does_not_wrap`.
        let two = U256::from(2u64);
        assert_eq!(mul_div_down(U256::MAX, two, two), Ok(U256::MAX));
        assert_eq!(mul_div_up(U256::MAX, two, two), Ok(U256::MAX));
        assert_eq!(mul_div_down(U256::MAX, U256::MAX, U256::MAX), Ok(U256::MAX));
    }

    #[test]
    fn overflow_underflow_and_zero_divisor_are_errors() {
        let one = U256::from(1u64);
        assert_eq!(add(U256::MAX, one), Err(ArithError::Overflow));
        assert_eq!(add(U256::MAX, U256::ZERO), Ok(U256::MAX));
        assert_eq!(sub(U256::ZERO, one), Err(ArithError::Underflow));
        assert_eq!(sub(one, one), Ok(U256::ZERO));
        assert_eq!(mul(U256::MAX, U256::from(2u64)), Err(ArithError::Overflow));
        assert_eq!(mul(U256::MAX, one), Ok(U256::MAX));
        assert_eq!(
            mul_div_down(U256::MAX, U256::from(2u64), one),
            Err(ArithError::Overflow)
        );
        assert_eq!(
            mul_div_down(one, one, U256::ZERO),
            Err(ArithError::DivisionByZero)
        );
        assert_eq!(
            mul_div_up(one, one, U256::ZERO),
            Err(ArithError::DivisionByZero)
        );
    }

    #[test]
    fn rounding_direction_is_exact() {
        let n = |v: u64| U256::from(v);
        assert_eq!(mul_div_down(n(7), n(1), n(2)), Ok(n(3)));
        assert_eq!(mul_div_up(n(7), n(1), n(2)), Ok(n(4)));
        assert_eq!(mul_div_down(n(8), n(1), n(2)), Ok(n(4)));
        assert_eq!(mul_div_up(n(8), n(1), n(2)), Ok(n(4)));
        assert_eq!(mul_div_up(n(0), n(5), n(3)), Ok(n(0)));
        // The rounded-up quotient itself can exceed 256 bits.
        assert_eq!(
            mul_div_up(U256::MAX, U256::MAX, U256::MAX - n(1)),
            Err(ArithError::Overflow)
        );
    }
}
