//! Native-BNB Flap bonding-curve quote math for the frozen G0.6a scope.
//!
//! This module performs no networking and no JSON parsing. Runtime/mainnet configuration
//! is bound outside the hot path. The constants below are valid only for Flap curve id 20
//! (CURVE_RH_BNB) and must be guarded by current preflight evidence.

use alloy_primitives::{U256, U512};

pub const CURVE_RH_BNB_ID: u8 = 20;
pub const BPS_DENOMINATOR: u64 = 10_000;

const WAD_U128: u128 = 1_000_000_000_000_000_000;
const TOTAL_SUPPLY_U128: u128 = 1_000_000_000_000_000_000_000_000_000;
const CURVE20_R_U128: u128 = 6_140_000_000_000_000_000;
const CURVE20_H_U128: u128 = 107_036_752_000_000_000_000_000_000;
const CURVE20_K_U128: u128 = 6_797_205_657_280_000_000_000_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Curve {
    pub r: U256,
    pub h: U256,
    pub k: U256,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyQuote {
    pub input_amount: U256,
    pub protocol_fee: U256,
    pub bonding_curve_tax: U256,
    pub combined_fee: U256,
    pub net_reserve_input: U256,
    pub reserve_after: U256,
    pub supply_after: U256,
    pub output_amount: U256,
}

fn wide(x: U256) -> U512 {
    U512::from(x)
}

fn narrow(x: U512) -> U256 {
    assert!(x <= U512::from(U256::MAX), "U256 overflow");
    U256::from(x)
}

fn mul_div_down(a: U256, b: U256, d: U256) -> U256 {
    assert!(d != U256::ZERO, "division by zero");
    narrow(wide(a) * wide(b) / wide(d))
}

fn div_wad_up(a: U256, b: U256) -> U256 {
    assert!(b != U256::ZERO, "division by zero");
    let numerator = wide(a) * wide(U256::from(WAD_U128));
    let denominator = wide(b);
    let q = numerator / denominator;
    let r = numerator % denominator;
    narrow(if r == U512::ZERO { q } else { q + U512::ONE })
}

/// Exact Flap curve id 20 (CURVE_RH_BNB) fixed-point parameters.
pub fn curve20() -> Curve {
    Curve {
        r: U256::from(CURVE20_R_U128),
        h: U256::from(CURVE20_H_U128),
        k: U256::from(CURVE20_K_U128),
    }
}

/// Flap LibCurve::estimateSupply semantics: TOTAL_SUPPLY + h - divWadUp(k, r + reserve).
pub fn estimate_supply(curve: Curve, reserve: U256) -> U256 {
    let total_plus_h = U256::from(TOTAL_SUPPLY_U128) + curve.h;
    let virtual_token_reserve = div_wad_up(curve.k, curve.r + reserve);
    assert!(
        total_plus_h >= virtual_token_reserve,
        "invalid curve state: virtual reserve exceeds total+h"
    );
    total_plus_h - virtual_token_reserve
}

/// Quote a direct native-BNB bonding-curve BUY.
///
/// Fee rounding intentionally mirrors the proven Flap rule:
/// combined = floor(input * (protocol_bps + tax_bps) / 10000)
/// protocol = floor(input * protocol_bps / 10000)
/// tax      = combined - protocol
///
/// For G0.6a self-launch, reserve_before=0 and circulating_before=0.
pub fn quote_native_buy(
    curve_id: u8,
    reserve_before: U256,
    circulating_before: U256,
    input_amount: U256,
    protocol_fee_bps: u16,
    buy_tax_bps: u16,
    tax_on_bonding_curve: bool,
) -> BuyQuote {
    assert_eq!(
        curve_id, CURVE_RH_BNB_ID,
        "unsupported curve id for native-BNB quote"
    );
    assert!(input_amount > U256::ZERO, "BUY input must be positive");

    let effective_tax_bps = if tax_on_bonding_curve {
        buy_tax_bps
    } else {
        0
    };
    let combined_bps = u32::from(protocol_fee_bps) + u32::from(effective_tax_bps);
    assert!(
        combined_bps < BPS_DENOMINATOR as u32,
        "combined fee/tax must be <100%"
    );

    let denom = U256::from(BPS_DENOMINATOR);
    let protocol_fee = mul_div_down(
        input_amount,
        U256::from(protocol_fee_bps),
        denom,
    );
    let combined_fee = mul_div_down(
        input_amount,
        U256::from(combined_bps),
        denom,
    );
    assert!(combined_fee >= protocol_fee);
    let bonding_curve_tax = combined_fee - protocol_fee;
    let net_reserve_input = input_amount - combined_fee;
    let reserve_after = reserve_before + net_reserve_input;

    let supply_after = estimate_supply(curve20(), reserve_after);
    assert!(
        supply_after >= circulating_before,
        "calculated supply moved backwards"
    );
    let output_amount = supply_after - circulating_before;

    BuyQuote {
        input_amount,
        protocol_fee,
        bonding_curve_tax,
        combined_fee,
        net_reserve_input,
        reserve_after,
        supply_after,
        output_amount,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve20_starts_at_zero_supply() {
        assert_eq!(estimate_supply(curve20(), U256::ZERO), U256::ZERO);
    }

    #[test]
    fn g06a_001_bnb_initial_buy_vector() {
        let q = quote_native_buy(
            20,
            U256::ZERO,
            U256::ZERO,
            U256::from(1_000_000_000_000_000u64),
            100,
            100,
            true,
        );
        assert_eq!(q.protocol_fee, U256::from(10_000_000_000_000u64));
        assert_eq!(q.bonding_curve_tax, U256::from(10_000_000_000_000u64));
        assert_eq!(q.net_reserve_input, U256::from(980_000_000_000_000u64));
        assert_eq!(
            q.output_amount,
            U256::from(176_664_965_031_639_901_123_273u128)
        );
    }

    #[test]
    fn g06a_0001_bnb_initial_buy_vector() {
        let q = quote_native_buy(
            20,
            U256::ZERO,
            U256::ZERO,
            U256::from(100_000_000_000_000u64),
            100,
            100,
            true,
        );
        assert_eq!(
            q.output_amount,
            U256::from(17_669_034_223_232_267_628_301u128)
        );
    }

    #[test]
    #[should_panic(expected = "unsupported curve id")]
    fn reject_unbound_curve() {
        let _ = quote_native_buy(
            19,
            U256::ZERO,
            U256::ZERO,
            U256::from(1u64),
            100,
            100,
            true,
        );
    }
}
