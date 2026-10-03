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

use super::curve::{Curve, supply};
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
mod tests {
    use super::*;
    use crate::economics::fees::FeeError;
    use crate::economics::policy::PolicyError;
    use crate::tx::legacy::testdata::unhex;
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
