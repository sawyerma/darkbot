//! Native-BNB BUY on Flap curve id 20 (`CURVE_RH_BNB`): arithmetic and
//! runtime admission.
//!
//! # Arithmetic
//!
//! [`quote_native_buy`] is a port of Gate-0 `native_bnb_quote` with
//! identical operation order and typed errors instead of assertions. It is
//! a pure function of its inputs and not an admission: it does not know
//! whether a state or amount is qualified.
//!
//! # Admission
//!
//! [`admit_native_buy`] is the only producer of a sendable native BUY. It
//! requires a [`QualifiedNativeState`], which exists only for
//!
//! - a launch that the profile classifier accepted on a validated
//!   deployment (which excludes a creator purchase),
//! - resident venue terms equal to the witnessed ones,
//! - the state directly after the launch, before any trade of the token,
//! - a state snapshot of the same code generation that names the same
//!   launch transaction as its pending candidate.
//!
//! It takes the amount and slippage from a [`BuyPolicy`] of the snapshot's
//! policy generation and admits only the amount covered by the independent
//! EVM execution on mainnet state (G0.6a: 0.001 BNB, exact match). The 0.0001 BNB
//! acquisition was executed on the testnet deployment; it is a regression
//! vector of the arithmetic, not a qualified mainnet amount. Another amount
//! or state needs its own independent execution before it is added.
//!
//! # Input contract (master plan §5)
//!
//! | Input | Source | Witness | Class | Invalidated by | Refresh owner |
//! |---|---|---|---|---|---|
//! | profile, token, buy tax, launch identity | launch calldata | G0.6a EVM execution | launch-derived | launch replacement, reorg | C2 |
//! | pending candidate | launch transaction hash | — (identity, bound at ingress) | launch-derived | launch replacement, reorg | C2 |
//! | code generation | live code fingerprints | G0.6a preflight, crosscheck | startup-precomputed | Portal or implementation change | runner |
//! | venue terms | Portal configuration | G0.6a preflight | pre-resident | Portal or registry update | C2 |
//! | caller exemption | Portal configuration | G0.6a EVM execution (not exempt) | pre-resident | exemption or sender change | C2 |
//! | reserve 0, supply 0 | launch without creator purchase, no prior trade | G0.6a EVM execution | launch-derived | any trade of the token, reorg | C2 |
//! | input amount, slippage, gas | operating policy | G0.6a EVM execution (amount) | policy | policy generation change | runner |
//!
//! The state engine that supplies these inputs is phase C2 and does not
//! exist yet. Until then nothing in this crate can construct a
//! [`QualifiedNativeState`] from live data.

use alloy_primitives::{Address, U256};

use super::curve::{Curve, reserve, supply};
use super::{AdmissionError, AdmittedBuy, QuoteError};
use crate::chain_state::SnapshotId;
use crate::economics::arith::{add, sub};
use crate::economics::fees::split_fee;
use crate::economics::policy::BuyPolicy;
use crate::economics::slippage::min_output;
use crate::venues::flap::profile::{ClassifiedLaunch, LaunchProfile};

/// Curve id of `CURVE_RH_BNB`.
pub const CURVE_ID_RH_BNB: u8 = 20;

const CURVE20_R: u128 = 6_140_000_000_000_000_000;
const CURVE20_H: u128 = 107_036_752_000_000_000_000_000_000;
const CURVE20_K: u128 = 6_797_205_657_280_000_000_000_000_000;

/// Fixed-point parameters of curve id 20.
pub fn curve20() -> Curve {
    Curve {
        r: U256::from(CURVE20_R),
        h: U256::from(CURVE20_H),
        k: U256::from(CURVE20_K),
    }
}

/// Inputs of the native BUY arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeBuyInputs {
    /// Curve id of the token; must be [`CURVE_ID_RH_BNB`].
    pub curve_id: u8,
    /// Quote reserve of the token before this BUY, in wei.
    pub reserve_before: U256,
    /// Circulating token supply before this BUY.
    pub circulating_before: U256,
    /// Native amount to spend, in wei.
    pub input_amount: U256,
    /// Protocol fee in basis points.
    pub protocol_fee_bps: u16,
    /// Buy tax of the token in basis points.
    pub buy_tax_bps: u16,
    /// Whether the venue applies the buy tax on the bonding curve.
    pub tax_on_bonding_curve_enabled: bool,
    /// Whether the caller is exempt from fees.
    pub caller_fee_exempt: bool,
    /// The per-origin holding cap of the BUY's origin at this state.
    pub holding_cap: HoldingCap,
}

/// The Portal's per-origin holding cap for one origin at a state (v5.27.0
/// `maxBuyPerOrigin` and `buyQuotaOf`: `maxSupply · bps / 10000` less the
/// origin's balance; a token with `bps == 0` has none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldingCap {
    None,
    /// The tokens the origin may still acquire. A BUY above it fills only up
    /// to it and refunds the rest; the quote refuses it.
    Remaining(U256),
}

/// Result of a native BUY quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeBuyQuote {
    /// Protocol fee, in wei.
    pub protocol_fee: U256,
    /// Tax charged on the curve, in wei.
    pub bonding_curve_tax: U256,
    /// Protocol fee plus tax, in wei.
    pub combined_fee: U256,
    /// Amount added to the reserve, in wei.
    pub net_reserve_input: U256,
    /// Reserve after this BUY, in wei.
    pub reserve_after: U256,
    /// Circulating supply after this BUY.
    pub supply_after: U256,
    /// Tokens received; strictly positive.
    pub output_amount: U256,
}

/// Arithmetic of a direct native-BNB bonding-curve BUY. Not an admission.
pub fn quote_native_buy(inputs: &NativeBuyInputs) -> Result<NativeBuyQuote, QuoteError> {
    if inputs.curve_id != CURVE_ID_RH_BNB {
        return Err(QuoteError::UnsupportedCurve);
    }
    if inputs.input_amount.is_zero() {
        return Err(QuoteError::ZeroInput);
    }
    if !inputs.tax_on_bonding_curve_enabled {
        return Err(QuoteError::UnwitnessedTaxDisabled);
    }
    if inputs.caller_fee_exempt {
        return Err(QuoteError::UnwitnessedExemptCaller);
    }

    let fees = split_fee(
        inputs.input_amount,
        inputs.protocol_fee_bps,
        inputs.buy_tax_bps,
    )?;
    let net_reserve_input = sub(inputs.input_amount, fees.combined)?;
    let reserve_after = add(inputs.reserve_before, net_reserve_input)?;
    let supply_after = supply(curve20(), reserve_after)?;
    if supply_after < inputs.circulating_before {
        return Err(QuoteError::InvalidCurveState);
    }
    let output_amount = supply_after - inputs.circulating_before;
    if output_amount.is_zero() {
        return Err(QuoteError::ZeroOutput);
    }
    if let HoldingCap::Remaining(remaining) = inputs.holding_cap
        && output_amount > remaining
    {
        return Err(QuoteError::HoldingCapExceeded);
    }

    Ok(NativeBuyQuote {
        protocol_fee: fees.protocol,
        bonding_curve_tax: fees.tax,
        combined_fee: fees.combined,
        net_reserve_input,
        reserve_after,
        supply_after,
        output_amount,
    })
}

/// Inputs of the native SELL arithmetic (tokens to native BNB on the curve).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeSellInputs {
    /// Curve id of the token; must be [`CURVE_ID_RH_BNB`].
    pub curve_id: u8,
    /// Quote reserve of the token before this SELL, in wei.
    pub reserve_before: U256,
    /// Circulating token supply before this SELL.
    pub circulating_before: U256,
    /// Tokens sold.
    pub input_amount: U256,
    /// Protocol fee in basis points.
    pub protocol_fee_bps: u16,
    /// Sell tax of the token in basis points.
    pub sell_tax_bps: u16,
    /// Whether the venue applies the tax on the bonding curve.
    pub tax_on_bonding_curve_enabled: bool,
    /// Whether the caller is exempt from fees.
    pub caller_fee_exempt: bool,
}

/// Result of a native SELL quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeSellQuote {
    /// Native amount the curve releases, before fees, in wei.
    pub gross_output: U256,
    pub protocol_fee: U256,
    pub bonding_curve_tax: U256,
    pub combined_fee: U256,
    pub reserve_after: U256,
    pub supply_after: U256,
    /// Native amount received; strictly positive.
    pub output_amount: U256,
}

/// The SELL's fees on `gross`: `floor(gross * protocol_bps / 10000)` and
/// `floor(gross * tax_bps / 10000)`, each on its own.
pub fn split_sell_fee(
    gross: U256,
    protocol_bps: u16,
    tax_bps: u16,
) -> Result<crate::economics::fees::FeeSplit, QuoteError> {
    use crate::economics::arith::mul_div_down;
    use crate::economics::fees::{BPS_DENOMINATOR, FeeError, FeeSplit};
    if u32::from(protocol_bps) + u32::from(tax_bps) >= u32::from(BPS_DENOMINATOR) {
        return Err(QuoteError::Fee(FeeError::RateTooHigh));
    }
    let d = U256::from(BPS_DENOMINATOR);
    let protocol = mul_div_down(gross, U256::from(protocol_bps), d)?;
    let tax = mul_div_down(gross, U256::from(tax_bps), d)?;
    Ok(FeeSplit {
        combined: add(protocol, tax)?,
        protocol,
        tax,
    })
}

/// Arithmetic of a direct bonding-curve SELL to native BNB. Not an
/// admission. The reserve after the SELL is `LibCurve::estimateReserve`
/// (rounded up); the protocol fee and the sell tax come off the released
/// amount, each floored on its own (unlike the BUY, whose tax is the
/// remainder of one combined floor). Witnessed exactly against
/// `Portal.quoteExactInput` (`fixtures/flap/quote_parity_*.txt`).
pub fn quote_native_sell(inputs: &NativeSellInputs) -> Result<NativeSellQuote, QuoteError> {
    if inputs.curve_id != CURVE_ID_RH_BNB {
        return Err(QuoteError::UnsupportedCurve);
    }
    if inputs.input_amount.is_zero() {
        return Err(QuoteError::ZeroInput);
    }
    if !inputs.tax_on_bonding_curve_enabled {
        return Err(QuoteError::UnwitnessedTaxDisabled);
    }
    if inputs.caller_fee_exempt {
        return Err(QuoteError::UnwitnessedExemptCaller);
    }
    if inputs.input_amount > inputs.circulating_before {
        return Err(QuoteError::InvalidCurveState);
    }
    let supply_after = inputs.circulating_before - inputs.input_amount;
    let reserve_after = reserve(curve20(), supply_after)?;
    if reserve_after > inputs.reserve_before {
        return Err(QuoteError::InvalidCurveState);
    }
    let gross_output = inputs.reserve_before - reserve_after;
    let fees = split_sell_fee(gross_output, inputs.protocol_fee_bps, inputs.sell_tax_bps)?;
    let output_amount = sub(gross_output, fees.combined)?;
    if output_amount.is_zero() {
        return Err(QuoteError::ZeroOutput);
    }
    Ok(NativeSellQuote {
        gross_output,
        protocol_fee: fees.protocol,
        bonding_curve_tax: fees.tax,
        combined_fee: fees.combined,
        reserve_after,
        supply_after,
        output_amount,
    })
}

/// Resident venue terms that the native BUY depends on, as read from the
/// Portal outside the entry path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VenueBuyTerms {
    /// Whether native-BNB quoting is enabled.
    pub native_quote_enabled: bool,
    /// Curve id used for native-BNB tokens.
    pub default_curve_id: u8,
    /// Protocol fee on a BUY, in basis points.
    pub protocol_buy_fee_bps: u16,
    /// Whether the buy tax applies on the bonding curve.
    pub tax_on_bonding_curve_enabled: bool,
    /// Whether the intended caller is exempt from fees.
    pub caller_fee_exempt: bool,
}

/// Venue terms of the witnessed mainnet executions (G0.6a read-only
/// preflight: native quoting enabled, curve id 20, 100 bps BUY fee, tax on
/// the curve enabled; G0.6a EVM execution: caller not exempt).
pub const WITNESSED_MAINNET_TERMS: VenueBuyTerms = VenueBuyTerms {
    native_quote_enabled: true,
    default_curve_id: CURVE_ID_RH_BNB,
    protocol_buy_fee_bps: 100,
    tax_on_bonding_curve_enabled: true,
    caller_fee_exempt: false,
};

/// The holding cap of the qualified state, directly after a `newTokenV6`
/// launch without extension, for an origin that holds none of the token:
/// none. A cap comes only from the admin setters (`setTokenMaxBuyBps`,
/// `setTokenNFTGate`, each announced by its event) or from the NFT-gate
/// extension of `newTokenV7`, which the profile refuses (`g02_flap_parity`
/// records each sampled launch receipt's cap and gate). Any other state's
/// cap is an input the quote enforces.
pub const FRESH_LAUNCH_HOLDING_CAP: HoldingCap = HoldingCap::None;

/// The input amount covered by the independent EVM execution on mainnet
/// state: 0.001 BNB, in wei.
pub const QUALIFIED_MAINNET_INPUT_WEI: u64 = 1_000_000_000_000_000;

/// The one qualified state of the native profile: directly after a
/// classified launch, before any trade of the token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedNativeState {
    token: Address,
    buy_tax_bps: u16,
    snapshot: SnapshotId,
}

impl QualifiedNativeState {
    /// Binds a classified launch to the fresh post-launch state.
    ///
    /// `trades_since_launch` is the number of trades of the token ordered
    /// before the intended BUY in `snapshot`; anything but zero is not
    /// qualified.
    pub fn fresh_launch(
        classified: &ClassifiedLaunch,
        terms: &VenueBuyTerms,
        trades_since_launch: u64,
        snapshot: SnapshotId,
    ) -> Result<Self, AdmissionError> {
        match classified.profile() {
            LaunchProfile::NativeCurve20NoCreatorPurchase => {}
        }
        if classified.codehash_generation() != snapshot.codehash_generation {
            return Err(AdmissionError::GenerationMismatch);
        }
        if classified.candidate() != snapshot.pending_candidate {
            return Err(AdmissionError::CandidateMismatch);
        }
        if *terms != WITNESSED_MAINNET_TERMS {
            return Err(AdmissionError::VenueTerms);
        }
        if trades_since_launch != 0 {
            return Err(AdmissionError::StateNotFresh);
        }
        Ok(Self {
            token: classified.token(),
            buy_tax_bps: classified.buy_tax_bps(),
            snapshot,
        })
    }

    /// Token of the state.
    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// Snapshot the state belongs to.
    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }
}

/// Admits a native BUY in a qualified state under the current policy.
///
/// The policy must be of the snapshot's policy generation. The resulting
/// minimum output is strictly positive and never above the expected output.
pub fn admit_native_buy(
    state: &QualifiedNativeState,
    policy: &BuyPolicy,
) -> Result<AdmittedBuy, AdmissionError> {
    if policy.generation() != state.snapshot.policy_generation {
        return Err(AdmissionError::PolicyGenerationMismatch);
    }
    let input_amount = policy.input_amount();
    if input_amount != U256::from(QUALIFIED_MAINNET_INPUT_WEI) {
        return Err(AdmissionError::UnqualifiedAmount);
    }
    let quote = quote_native_buy(&NativeBuyInputs {
        curve_id: WITNESSED_MAINNET_TERMS.default_curve_id,
        reserve_before: U256::ZERO,
        circulating_before: U256::ZERO,
        input_amount,
        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
        buy_tax_bps: state.buy_tax_bps,
        tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
        caller_fee_exempt: WITNESSED_MAINNET_TERMS.caller_fee_exempt,
        holding_cap: FRESH_LAUNCH_HOLDING_CAP,
    })?;
    let min_output_amount = min_output(quote.output_amount, policy.slippage_bps())?;
    Ok(AdmittedBuy::new(
        state.token,
        input_amount,
        quote.output_amount,
        min_output_amount,
        policy.slippage_bps(),
        state.snapshot,
    ))
}

#[cfg(test)]
mod parity_tests {
    //! Phase G (Economics gate §6 level 2): the production BUY and SELL
    //! arithmetic against `Portal.quoteExactInput` at pinned states of live
    //! curve-20 tokens (`fixtures/flap/quote_parity_g02.txt`, g02_flap_parity).
    use super::*;

    const PARITY: &str = include_str!("../../../fixtures/flap/quote_parity_g02.txt");

    /// Every sample: the local quote is the Portal's, to the unit. The one
    /// exception is a BUY the token's per-origin holding cap (2 % of the
    /// supply) cuts: the Portal fills up to the cap and refunds the rest, a
    /// partial fill the local quote does not model (it must refuse).
    #[test]
    fn buy_and_sell_equal_the_portal_quote_at_the_same_state() {
        let (mut buys, mut sells, mut capped) = (0, 0, 0);
        for line in PARITY
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        {
            let f: Vec<&str> = line.split_whitespace().collect();
            let n = |i: usize| U256::from_str_radix(f[i], 10).unwrap();
            let (reserve, supply, amount) = (n(3), n(4), n(8));
            let (buy_tax, sell_tax): (u16, u16) = (f[5].parse().unwrap(), f[6].parse().unwrap());
            let chain = n(9);
            match f[7] {
                "BUY" => {
                    let local = quote_native_buy(&NativeBuyInputs {
                        curve_id: CURVE_ID_RH_BNB,
                        reserve_before: reserve,
                        circulating_before: supply,
                        input_amount: amount,
                        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                        buy_tax_bps: buy_tax,
                        tax_on_bonding_curve_enabled: true,
                        caller_fee_exempt: false,
                        holding_cap: HoldingCap::None,
                    })
                    .unwrap()
                    .output_amount;
                    if local == chain {
                        buys += 1;
                    } else {
                        // 2 % of the 1e9-token supply: the call's origin
                        // holds none, so its allowance is the whole cap,
                        // and the quote refuses what the Portal cuts.
                        let cap =
                            U256::from(20_000_000u64) * U256::from(10u64).pow(U256::from(18u64));
                        assert_eq!(chain, cap, "{line}");
                        assert!(local > chain, "{line}");
                        let refused = quote_native_buy(&NativeBuyInputs {
                            curve_id: CURVE_ID_RH_BNB,
                            reserve_before: reserve,
                            circulating_before: supply,
                            input_amount: amount,
                            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                            buy_tax_bps: buy_tax,
                            tax_on_bonding_curve_enabled: true,
                            caller_fee_exempt: false,
                            holding_cap: HoldingCap::Remaining(cap),
                        });
                        assert_eq!(refused, Err(QuoteError::HoldingCapExceeded), "{line}");
                        capped += 1;
                    }
                }
                "SELL" => {
                    let local = quote_native_sell(&NativeSellInputs {
                        curve_id: CURVE_ID_RH_BNB,
                        reserve_before: reserve,
                        circulating_before: supply,
                        input_amount: amount,
                        protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
                        sell_tax_bps: sell_tax,
                        tax_on_bonding_curve_enabled: true,
                        caller_fee_exempt: false,
                    })
                    .unwrap()
                    .output_amount;
                    assert_eq!(local, chain, "{line}");
                    sells += 1;
                }
                other => panic!("side {other}"),
            }
        }
        assert_eq!((buys, sells, capped), (39, 35, 1));
    }

    /// The SELL fees are floored each on its own; the BUY's tax is the
    /// remainder of the combined floor: on one amount they can differ by one.
    #[test]
    fn sell_and_buy_fee_rounding_differ_where_witnessed() {
        let gross = U256::from(1_318_019_119u64);
        let sell = split_sell_fee(gross, 100, 900).unwrap();
        let buy = crate::economics::fees::split_fee(gross, 100, 900).unwrap();
        assert_eq!(sell.protocol, buy.protocol);
        assert!(buy.combined >= sell.combined && buy.combined - sell.combined <= U256::from(1u8));
        assert!(split_sell_fee(gross, 5_000, 5_000).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economics::fees::FeeError;
    use crate::economics::policy::PolicyError;
    use crate::transactions::legacy::testdata::unhex;
    use crate::venues::flap::deployment::testkit::validated_mainnet;
    use crate::venues::flap::profile::classify_launch;
    use crate::venues::flap::profile::testkit::{
        WITNESSED_TOKEN, candidate_a, candidate_b, witnessed_native_launch,
    };
    use alloy_primitives::B256;

    const POLICY_GENERATION: u64 = 3;

    fn snapshot(codehash_generation: u64) -> SnapshotId {
        SnapshotId {
            canonical_block_hash: B256::from_slice(&[0xb1u8; 32]),
            ordered_state_cursor: 41,
            pending_candidate: candidate_a(),
            policy_generation: POLICY_GENERATION,
            codehash_generation,
        }
    }

    /// The witnessed launch, classified on the validated mainnet deployment
    /// of code generation 7 as launch transaction A.
    fn classified() -> ClassifiedLaunch {
        classify_launch(
            &validated_mainnet(7),
            candidate_a(),
            &witnessed_native_launch(),
        )
        .unwrap()
    }

    fn qualified() -> QualifiedNativeState {
        QualifiedNativeState::fresh_launch(&classified(), &WITNESSED_MAINNET_TERMS, 0, snapshot(7))
            .unwrap()
    }

    fn qualified_amount() -> U256 {
        U256::from(QUALIFIED_MAINNET_INPUT_WEI)
    }

    /// Arithmetic fixture using the already witnessed venue/profile values.
    fn witnessed(input_amount: u64) -> NativeBuyInputs {
        let launch = witnessed_native_launch();
        NativeBuyInputs {
            curve_id: WITNESSED_MAINNET_TERMS.default_curve_id,
            reserve_before: U256::ZERO,
            circulating_before: U256::ZERO,
            input_amount: U256::from(input_amount),
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            buy_tax_bps: launch.buy_tax,
            tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
            caller_fee_exempt: WITNESSED_MAINNET_TERMS.caller_fee_exempt,
            holding_cap: HoldingCap::None,
        }
    }

    /// Policy of the snapshot's generation.
    fn policy(input_amount: U256, slippage_bps: u16) -> BuyPolicy {
        BuyPolicy::new(
            POLICY_GENERATION,
            input_amount,
            slippage_bps,
            310_000_000,
            1_500_000,
        )
        .unwrap()
    }

    #[test]
    fn curve20_starts_at_zero_supply() {
        assert_eq!(supply(curve20(), U256::ZERO), Ok(U256::ZERO));
    }

    #[test]
    fn g06a_001_bnb_initial_buy_vector() {
        let quote = quote_native_buy(&witnessed(1_000_000_000_000_000)).unwrap();
        assert_eq!(quote.protocol_fee, U256::from(10_000_000_000_000u64));
        assert_eq!(quote.bonding_curve_tax, U256::from(10_000_000_000_000u64));
        assert_eq!(quote.combined_fee, U256::from(20_000_000_000_000u64));
        assert_eq!(quote.net_reserve_input, U256::from(980_000_000_000_000u64));
        assert_eq!(quote.reserve_after, U256::from(980_000_000_000_000u64));
        let expected = U256::from(176_664_965_031_639_901_123_273u128);
        assert_eq!(quote.output_amount, expected);
        assert_eq!(quote.supply_after, expected);
        // G0.6a EVM crosscheck `rust_min_output_100bps`.
        assert_eq!(
            min_output(quote.output_amount, 100),
            Ok(U256::from(174_898_315_381_323_502_112_040u128))
        );
    }

    /// A BUY above the origin's holding allowance is refused, never quoted
    /// as the partial fill the Portal would make; at or below it, it is
    /// quoted unchanged.
    #[test]
    fn a_buy_above_the_holding_allowance_is_refused() {
        let free = quote_native_buy(&witnessed(1_000_000_000_000_000))
            .unwrap()
            .output_amount;
        let capped = |remaining: U256| {
            let mut inputs = witnessed(1_000_000_000_000_000);
            inputs.holding_cap = HoldingCap::Remaining(remaining);
            quote_native_buy(&inputs).map(|q| q.output_amount)
        };
        assert_eq!(capped(free), Ok(free));
        assert_eq!(
            capped(free - U256::from(1u8)),
            Err(QuoteError::HoldingCapExceeded)
        );
        assert_eq!(capped(U256::ZERO), Err(QuoteError::HoldingCapExceeded));
    }

    #[test]
    fn g07_0001_bnb_testnet_acquisition_vector() {
        let quote = quote_native_buy(&witnessed(100_000_000_000_000)).unwrap();
        assert_eq!(quote.net_reserve_input, U256::from(98_000_000_000_000u64));
        assert_eq!(
            quote.output_amount,
            U256::from(17_669_034_223_232_267_628_301u128)
        );
    }

    #[test]
    fn second_buy_continues_from_the_first_buys_state() {
        // Two consecutive BUYs must yield the same total as the curve at the
        // summed reserve: the quote is a pure function of the state.
        let first = quote_native_buy(&witnessed(100_000_000_000_000)).unwrap();
        let mut inputs = witnessed(100_000_000_000_000);
        inputs.reserve_before = first.reserve_after;
        inputs.circulating_before = first.supply_after;
        let second = quote_native_buy(&inputs).unwrap();
        assert_eq!(
            second.supply_after,
            supply(curve20(), U256::from(196_000_000_000_000u64)).unwrap()
        );
        assert_eq!(
            second.output_amount,
            second.supply_after - first.supply_after
        );
        assert!(second.output_amount < first.output_amount);
    }

    #[test]
    fn unsupported_and_unwitnessed_branches_are_rejected() {
        let mut inputs = witnessed(1);
        inputs.curve_id = 19;
        assert_eq!(quote_native_buy(&inputs), Err(QuoteError::UnsupportedCurve));

        let mut inputs = witnessed(1_000_000_000_000_000);
        inputs.input_amount = U256::ZERO;
        assert_eq!(quote_native_buy(&inputs), Err(QuoteError::ZeroInput));

        let mut inputs = witnessed(1_000_000_000_000_000);
        inputs.tax_on_bonding_curve_enabled = false;
        assert_eq!(
            quote_native_buy(&inputs),
            Err(QuoteError::UnwitnessedTaxDisabled)
        );

        let mut inputs = witnessed(1_000_000_000_000_000);
        inputs.caller_fee_exempt = true;
        assert_eq!(
            quote_native_buy(&inputs),
            Err(QuoteError::UnwitnessedExemptCaller)
        );

        let mut inputs = witnessed(1_000_000_000_000_000);
        inputs.protocol_fee_bps = 5_000;
        inputs.buy_tax_bps = 5_000;
        assert_eq!(
            quote_native_buy(&inputs),
            Err(QuoteError::Fee(FeeError::RateTooHigh))
        );
    }

    #[test]
    fn inconsistent_or_extreme_state_is_an_error_not_a_panic() {
        // Circulating supply above what the curve yields.
        let mut inputs = witnessed(1_000_000_000_000_000);
        inputs.circulating_before = U256::MAX;
        assert_eq!(
            quote_native_buy(&inputs),
            Err(QuoteError::InvalidCurveState)
        );

        // Reserve addition overflows 256 bits.
        let mut inputs = witnessed(1_000_000_000_000_000);
        inputs.reserve_before = U256::MAX;
        assert!(quote_native_buy(&inputs).is_err());

        // An input too small to move the curve by one token unit.
        let mut inputs = witnessed(1);
        inputs.reserve_before = U256::from(u128::MAX);
        inputs.circulating_before = supply(curve20(), U256::from(u128::MAX)).unwrap();
        assert_eq!(quote_native_buy(&inputs), Err(QuoteError::ZeroOutput));
    }

    #[test]
    fn admitted_buy_reproduces_the_evm_crosscheck() {
        let admitted = admit_native_buy(&qualified(), &policy(qualified_amount(), 100)).unwrap();
        assert_eq!(
            admitted.token().as_slice(),
            unhex(WITNESSED_TOKEN).as_slice()
        );
        assert_eq!(admitted.input_amount(), qualified_amount());
        // g06a_economics_evm_crosscheck: `rust_expected_output` equals
        // `evm_simulated_received_amount`; `rust_min_output_100bps`.
        assert_eq!(
            admitted.expected_output(),
            U256::from(176_664_965_031_639_901_123_273u128)
        );
        assert_eq!(
            admitted.min_output_amount(),
            U256::from(174_898_315_381_323_502_112_040u128)
        );
        assert_eq!(admitted.slippage_bps(), 100);
        assert_eq!(admitted.snapshot(), snapshot(7));

        let calldata = admitted.calldata().unwrap();
        assert_eq!(calldata.len(), 228);
        assert_eq!(
            &calldata[4 + 2 * 32 + 12..4 + 3 * 32],
            unhex(WITNESSED_TOKEN).as_slice()
        );
    }

    #[test]
    fn only_the_fresh_state_of_matching_generation_and_terms_is_qualified() {
        let launch = classified();
        let terms = WITNESSED_MAINNET_TERMS;

        // Any earlier trade of the token.
        assert_eq!(
            QualifiedNativeState::fresh_launch(&launch, &terms, 1, snapshot(7)),
            Err(AdmissionError::StateNotFresh)
        );
        // Snapshot of another code generation.
        assert_eq!(
            QualifiedNativeState::fresh_launch(&launch, &terms, 0, snapshot(8)),
            Err(AdmissionError::GenerationMismatch)
        );

        // Every deviation of the resident venue terms.
        let mut other = terms;
        other.native_quote_enabled = false;
        assert_eq!(
            QualifiedNativeState::fresh_launch(&launch, &other, 0, snapshot(7)),
            Err(AdmissionError::VenueTerms)
        );
        let mut other = terms;
        other.default_curve_id = 19;
        assert_eq!(
            QualifiedNativeState::fresh_launch(&launch, &other, 0, snapshot(7)),
            Err(AdmissionError::VenueTerms)
        );
        let mut other = terms;
        other.protocol_buy_fee_bps = 50;
        assert_eq!(
            QualifiedNativeState::fresh_launch(&launch, &other, 0, snapshot(7)),
            Err(AdmissionError::VenueTerms)
        );
        let mut other = terms;
        other.tax_on_bonding_curve_enabled = false;
        assert_eq!(
            QualifiedNativeState::fresh_launch(&launch, &other, 0, snapshot(7)),
            Err(AdmissionError::VenueTerms)
        );
        let mut other = terms;
        other.caller_fee_exempt = true;
        assert_eq!(
            QualifiedNativeState::fresh_launch(&launch, &other, 0, snapshot(7)),
            Err(AdmissionError::VenueTerms)
        );
    }

    #[test]
    fn launch_a_cannot_be_combined_with_the_snapshot_of_launch_b() {
        let terms = WITNESSED_MAINNET_TERMS;
        let launch_a = classified();
        assert_eq!(launch_a.candidate(), candidate_a());

        let mut snapshot_b = snapshot(7);
        snapshot_b.pending_candidate = candidate_b();
        assert_eq!(
            QualifiedNativeState::fresh_launch(&launch_a, &terms, 0, snapshot_b),
            Err(AdmissionError::CandidateMismatch)
        );

        // The same calldata carried by transaction B is launch B: it fits
        // snapshot B and not snapshot A.
        let launch_b = classify_launch(
            &validated_mainnet(7),
            candidate_b(),
            &witnessed_native_launch(),
        )
        .unwrap();
        assert!(QualifiedNativeState::fresh_launch(&launch_b, &terms, 0, snapshot_b).is_ok());
        assert_eq!(
            QualifiedNativeState::fresh_launch(&launch_b, &terms, 0, snapshot(7)),
            Err(AdmissionError::CandidateMismatch)
        );
    }

    #[test]
    fn policy_of_another_generation_is_not_admitted() {
        let state = qualified();
        let stale = BuyPolicy::new(
            POLICY_GENERATION - 1,
            qualified_amount(),
            100,
            310_000_000,
            1_500_000,
        )
        .unwrap();
        assert_eq!(
            admit_native_buy(&state, &stale),
            Err(AdmissionError::PolicyGenerationMismatch)
        );
        let newer = BuyPolicy::new(
            POLICY_GENERATION + 1,
            qualified_amount(),
            100,
            310_000_000,
            1_500_000,
        )
        .unwrap();
        assert_eq!(
            admit_native_buy(&state, &newer),
            Err(AdmissionError::PolicyGenerationMismatch)
        );
    }

    #[test]
    fn only_the_crosschecked_amount_is_admitted() {
        let state = qualified();
        // The testnet-witnessed amount, a neighbour and a large amount.
        for amount in [
            U256::from(100_000_000_000_000u64),
            qualified_amount() - U256::from(1u64),
            qualified_amount() + U256::from(1u64),
            U256::MAX,
        ] {
            assert_eq!(
                admit_native_buy(&state, &policy(amount, 100)),
                Err(AdmissionError::UnqualifiedAmount)
            );
        }
    }

    /// Rust-side regression of the EVM boundary cases of G0.3: a minimum of
    /// exactly the expected output executes, one unit more reverts. The
    /// policy path can reach the first and never the second.
    #[test]
    fn admitted_minimum_is_positive_and_never_exceeds_the_expected_output() {
        let state = qualified();
        for bps in [0u16, 1, 50, 100, 500, 5_000, 9_999] {
            let admitted = admit_native_buy(&state, &policy(qualified_amount(), bps)).unwrap();
            assert!(admitted.min_output_amount() > U256::ZERO, "bps {bps}");
            assert!(
                admitted.min_output_amount() <= admitted.expected_output(),
                "bps {bps}"
            );
        }
        let exact = admit_native_buy(&state, &policy(qualified_amount(), 0)).unwrap();
        assert_eq!(exact.min_output_amount(), exact.expected_output());

        // 10,000 bps and more never reach admission: the policy rejects them.
        assert_eq!(
            BuyPolicy::new(POLICY_GENERATION, qualified_amount(), 10_000, 1, 1),
            Err(PolicyError::InvalidSlippageBps)
        );
    }
}
