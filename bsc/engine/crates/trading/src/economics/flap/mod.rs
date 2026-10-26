//! Scoped Flap BUY quotes and their runtime admission.
//!
//! Two layers, kept apart on purpose:
//!
//! - **Arithmetic.** [`native_curve20::quote_native_buy`] and the DOGE
//!   computation are exact integer functions of their inputs. They accept
//!   any well-formed input and say nothing about whether a state or amount
//!   is qualified.
//! - **Admission.** A BUY becomes sendable only as an [`AdmittedBuy`]. Its
//!   constructor is private to this module tree, so no other part of the
//!   crate can build one from raw numbers. It is produced solely by an
//!   admission function, from a launch that passed the profile classifier
//!   on a validated deployment, in a state and for an amount that are
//!   covered by witnessed evidence, under the operating policy of the same
//!   generation, with a strictly positive minimum output that never
//!   exceeds the expected output.
//!
//! Qualified today (master plan §3.2): the native-BNB curve-20 profile on a
//! fresh launch for the EVM-crosschecked amount. The DOGE profile is
//! implemented as arithmetic for its single witnessed state and is disabled
//! at runtime.

pub mod curve;
pub mod doge_v3;
pub mod native_curve20;

use alloy_primitives::{Address, U256};

use super::arith::ArithError;
use super::fees::FeeError;
use super::slippage::SlippageError;
use crate::chain_state::SnapshotId;
use crate::venues::flap::abi::{
    AbiError, SWAP_EXACT_INPUT_NATIVE_LEN, encode_swap_exact_input_native,
};

/// Reason a quote is not available for the given inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteError {
    /// The curve id is not the witnessed one.
    UnsupportedCurve,
    /// The BUY input amount is zero.
    ZeroInput,
    /// The tax-disabled branch has no witness and is not quoted.
    UnwitnessedTaxDisabled,
    /// The fee-exempt-caller branch has no witness and is not quoted.
    UnwitnessedExemptCaller,
    /// A state input is zero, too wide or otherwise outside the profile.
    InvalidState,
    /// The swap would leave the constant-liquidity segment that is known.
    OutsideQualifiedSegment,
    /// The curve state is inconsistent (virtual reserve above total supply
    /// plus `h`, or supply below the circulating amount).
    InvalidCurveState,
    /// The quote is zero.
    ZeroOutput,
    /// The BUY's supply after it reaches the token's DEX threshold: the
    /// token would leave the curve (migration), which is not quoted.
    ReachesDexThreshold,
    /// The BUY's output is above the origin's per-origin holding allowance:
    /// the Portal fills only up to it and refunds the rest, a partial fill
    /// that is not quoted.
    HoldingCapExceeded,
    /// Fee split failure.
    Fee(FeeError),
    /// Arithmetic failure.
    Arith(ArithError),
}

impl From<ArithError> for QuoteError {
    fn from(e: ArithError) -> Self {
        Self::Arith(e)
    }
}

impl From<FeeError> for QuoteError {
    fn from(e: FeeError) -> Self {
        Self::Fee(e)
    }
}

impl core::fmt::Display for QuoteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedCurve => f.write_str("quote: unsupported curve id"),
            Self::ZeroInput => f.write_str("quote: input amount is zero"),
            Self::UnwitnessedTaxDisabled => f.write_str("quote: tax-disabled branch unwitnessed"),
            Self::UnwitnessedExemptCaller => f.write_str("quote: exempt-caller branch unwitnessed"),
            Self::InvalidState => f.write_str("quote: state input outside the profile"),
            Self::OutsideQualifiedSegment => f.write_str("quote: swap leaves the known segment"),
            Self::InvalidCurveState => f.write_str("quote: inconsistent curve state"),
            Self::ZeroOutput => f.write_str("quote: output is zero"),
            Self::HoldingCapExceeded => f.write_str("quote: above the per-origin holding cap"),
            Self::ReachesDexThreshold => f.write_str("quote: reaches the DEX threshold"),
            Self::Fee(e) => write!(f, "quote: {e}"),
            Self::Arith(e) => write!(f, "quote: {e}"),
        }
    }
}

impl std::error::Error for QuoteError {}

/// Why a BUY is not admissible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionError {
    /// The launch was classified against another code generation than the
    /// state snapshot.
    GenerationMismatch,
    /// The state snapshot belongs to another launch transaction than the
    /// classified one.
    CandidateMismatch,
    /// The operating policy has another generation than the state snapshot.
    PolicyGenerationMismatch,
    /// The resident venue terms differ from the witnessed ones.
    VenueTerms,
    /// A trade of the token precedes this BUY; only the state directly
    /// after the launch is qualified.
    StateNotFresh,
    /// The input amount is not one covered by witnessed evidence.
    UnqualifiedAmount,
    /// The quote failed.
    Quote(QuoteError),
    /// No admissible minimum output exists.
    Slippage(SlippageError),
}

impl From<QuoteError> for AdmissionError {
    fn from(e: QuoteError) -> Self {
        Self::Quote(e)
    }
}

impl From<SlippageError> for AdmissionError {
    fn from(e: SlippageError) -> Self {
        Self::Slippage(e)
    }
}

impl core::fmt::Display for AdmissionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::GenerationMismatch => f.write_str("admission: code generation mismatch"),
            Self::CandidateMismatch => f.write_str("admission: snapshot of another launch"),
            Self::PolicyGenerationMismatch => f.write_str("admission: policy generation mismatch"),
            Self::VenueTerms => f.write_str("admission: venue terms differ from the witness"),
            Self::StateNotFresh => f.write_str("admission: token state is not fresh"),
            Self::UnqualifiedAmount => f.write_str("admission: input amount is not qualified"),
            Self::Quote(e) => write!(f, "admission: {e}"),
            Self::Slippage(e) => write!(f, "admission: {e}"),
        }
    }
}

impl std::error::Error for AdmissionError {}

/// A BUY that passed admission: token, amounts and the state it is valid
/// for. Its constructor is private to this module tree; only the admission
/// functions here can create one.
///
/// Invariants: `input_amount > 0`, `0 < min_output_amount <= expected_output`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmittedBuy {
    token: Address,
    input_amount: U256,
    expected_output: U256,
    min_output_amount: U256,
    slippage_bps: u16,
    snapshot: SnapshotId,
}

impl AdmittedBuy {
    fn new(
        token: Address,
        input_amount: U256,
        expected_output: U256,
        min_output_amount: U256,
        slippage_bps: u16,
        snapshot: SnapshotId,
    ) -> Self {
        Self {
            token,
            input_amount,
            expected_output,
            min_output_amount,
            slippage_bps,
            snapshot,
        }
    }

    /// Token to acquire.
    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// Native amount to spend, in wei.
    #[inline]
    pub fn input_amount(&self) -> U256 {
        self.input_amount
    }

    /// Output the quote expects.
    #[inline]
    pub fn expected_output(&self) -> U256 {
        self.expected_output
    }

    /// Minimum output bound into the calldata.
    #[inline]
    pub fn min_output_amount(&self) -> U256 {
        self.min_output_amount
    }

    /// Slippage policy the minimum was derived with.
    #[inline]
    pub fn slippage_bps(&self) -> u16 {
        self.slippage_bps
    }

    /// State snapshot the admission is valid for. A different current
    /// snapshot invalidates this value and everything signed from it.
    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }

    /// Calldata of the direct native-input BUY.
    pub fn calldata(&self) -> Result<[u8; SWAP_EXACT_INPUT_NATIVE_LEN], AbiError> {
        encode_swap_exact_input_native(self.token, self.input_amount, self.min_output_amount)
    }
}
