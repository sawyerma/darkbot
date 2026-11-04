//! R12d: the OpenFour bonding curve (`module.curve.bonding`) as executed on
//! mainnet (`fixtures/fourmeme/openfour_parity_20261104.txt`, one witnessed
//! preset). A launch starts at the Classic virtual reserves
//! ([`virtual_reserves`]) and keeps their product `K = Q0 · T0` exactly. A
//! BUY of an exact token amount (`Core.buy`) costs the curve
//! `floor(K / (T − x)) − Q`, a SELL of `x` grosses `Q − floor(K / (T + x))`;
//! the protocol and the developer fee are each `floor(quote · bps / 10 000)`
//! of that curve quote, added to a BUY's payment and taken from a SELL's
//! proceeds. `Core.buyByBudget` evaluates a budget's inverse otherwise
//! (fewer tokens than the exact inverse, 1.2e-8 to 1.5e-7 relative in the
//! witnessed probes, its rule unknown): it is not priced, the BUY buys an
//! exact amount.

use alloy_primitives::{Address, U256};

use super::arith::{ArithError, add, mul, sub};
use super::fees::BPS_DENOMINATOR;
use super::four_classic::{LaunchTerms, virtual_reserves};
use super::policy::{BuyPolicy, ExitPolicy};
use super::slippage::{SlippageError, min_output};
use crate::chain_state::SnapshotId;
use crate::venues::fourmeme::abi::{
    AbiError, OPENFOUR_TRADE_LEN, encode_openfour_buy, encode_openfour_sell,
};
use crate::venues::fourmeme::profile::{ClassifiedOpenFourHolding, ClassifiedOpenFourLaunch};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenFourQuoteError {
    ZeroInput,
    /// A state input is zero or outside the rule (no remaining sale, a
    /// reserve at or below what the trade moves, fees at or above the whole).
    InvalidState,
    /// A BUY of more tokens than the sale has left: the curve fills partly,
    /// which is not quoted.
    ExceedsRemaining,
    /// The quote is zero tokens or zero wei.
    ZeroOutput,
    Arith(ArithError),
}

impl From<ArithError> for OpenFourQuoteError {
    fn from(e: ArithError) -> Self {
        Self::Arith(e)
    }
}

impl core::fmt::Display for OpenFourQuoteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ZeroInput => f.write_str("openfour: input is zero"),
            Self::InvalidState => f.write_str("openfour: curve state outside the rule"),
            Self::ExceedsRemaining => f.write_str("openfour: buys more than the sale has left"),
            Self::ZeroOutput => f.write_str("openfour: output is zero"),
            Self::Arith(e) => write!(f, "openfour: {e}"),
        }
    }
}

impl std::error::Error for OpenFourQuoteError {}

/// The curve of one OpenFour token: the invariant, its virtual reserves and
/// the tokens its sale has left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourCurve {
    /// `K = Q0 · T0` of the launch, kept exactly.
    pub k: U256,
    /// The virtual token reserve `T`.
    pub token_reserve: U256,
    /// The virtual quote reserve `Q`, in wei.
    pub quote_reserve: U256,
    /// `remainingForSale`.
    pub remaining: U256,
}

/// The fee terms of a trade, in basis points of its curve quote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourFee {
    pub protocol_bps: u16,
    pub dev_bps: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourBuyQuote {
    pub amount: U256,
    /// The curve quote the tokens cost.
    pub cost: U256,
    pub protocol_fee: U256,
    pub dev_fee: U256,
    /// What the buyer pays: the cost and both fees.
    pub pay: U256,
    pub after: OpenFourCurve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourSellQuote {
    pub amount: U256,
    /// The curve quote the tokens fetch.
    pub gross: U256,
    pub protocol_fee: U256,
    pub dev_fee: U256,
    /// What the seller receives: the gross less both fees.
    pub received: U256,
    pub after: OpenFourCurve,
}

/// `floor(x · bps / 10 000)`.
fn part(x: U256, bps: u16) -> Result<U256, OpenFourQuoteError> {
    Ok(mul(x, U256::from(bps))? / U256::from(BPS_DENOMINATOR))
}

fn fees(fee: &OpenFourFee, quote: U256) -> Result<(U256, U256), OpenFourQuoteError> {
    if u32::from(fee.protocol_bps) + u32::from(fee.dev_bps) >= u32::from(BPS_DENOMINATOR) {
        return Err(OpenFourQuoteError::InvalidState);
    }
    Ok((part(quote, fee.protocol_bps)?, part(quote, fee.dev_bps)?))
}

/// What a trade of the curve quote `quote` is charged: both fees, each
/// floored on its own (the total of the fee router's preset-aware
/// `previewDistribution` without a tax or anti-sniper fee).
pub fn charged(fee: &OpenFourFee, quote: U256) -> Result<U256, OpenFourQuoteError> {
    let (protocol, dev) = fees(fee, quote)?;
    Ok(add(protocol, dev)?)
}

fn checked(curve: &OpenFourCurve) -> Result<(), OpenFourQuoteError> {
    if curve.k.is_zero()
        || curve.quote_reserve.is_zero()
        || curve.remaining.is_zero()
        || curve.token_reserve <= curve.remaining
    {
        return Err(OpenFourQuoteError::InvalidState);
    }
    Ok(())
}

/// The curve a launch with `terms` starts with.
pub fn launch_curve(terms: &LaunchTerms) -> Result<OpenFourCurve, OpenFourQuoteError> {
    let (token_reserve, quote_reserve) =
        virtual_reserves(terms).map_err(|_| OpenFourQuoteError::InvalidState)?;
    let curve = OpenFourCurve {
        k: mul(quote_reserve, token_reserve)?,
        token_reserve,
        quote_reserve,
        remaining: terms.max_offers,
    };
    checked(&curve)?;
    Ok(curve)
}

/// The BUY of exactly `amount` tokens (`Core.buy`).
pub fn quote_buy_amount(
    curve: &OpenFourCurve,
    fee: &OpenFourFee,
    amount: U256,
) -> Result<OpenFourBuyQuote, OpenFourQuoteError> {
    if amount.is_zero() {
        return Err(OpenFourQuoteError::ZeroInput);
    }
    checked(curve)?;
    if amount > curve.remaining {
        return Err(OpenFourQuoteError::ExceedsRemaining);
    }
    let token_reserve = sub(curve.token_reserve, amount)?;
    let cost = sub(curve.k / token_reserve, curve.quote_reserve)?;
    if cost.is_zero() {
        return Err(OpenFourQuoteError::ZeroOutput);
    }
    let (protocol_fee, dev_fee) = fees(fee, cost)?;
    Ok(OpenFourBuyQuote {
        amount,
        cost,
        protocol_fee,
        dev_fee,
        pay: add(add(cost, protocol_fee)?, dev_fee)?,
        after: OpenFourCurve {
            k: curve.k,
            token_reserve,
            quote_reserve: add(curve.quote_reserve, cost)?,
            remaining: curve.remaining - amount,
        },
    })
}

/// The largest exact BUY whose payment is at most `limit`: the most curve
/// quote `c` whose payment fits, then the most tokens costing at most `c`
/// (`T − floor(K / (Q + c + 1)) − 1`).
pub fn max_buy_amount(
    curve: &OpenFourCurve,
    fee: &OpenFourFee,
    limit: U256,
) -> Result<OpenFourBuyQuote, OpenFourQuoteError> {
    if limit.is_zero() {
        return Err(OpenFourQuoteError::ZeroInput);
    }
    checked(curve)?;
    let charged = u32::from(fee.protocol_bps) + u32::from(fee.dev_bps);
    let whole = U256::from(BPS_DENOMINATOR);
    let paying = |c: U256| -> Result<U256, OpenFourQuoteError> {
        let (p, d) = fees(fee, c)?;
        Ok(add(add(c, p)?, d)?)
    };
    // The fees floor at most one wei each: the bound is within two of the
    // proportional share.
    let mut c = mul(limit, whole)? / (whole + U256::from(charged));
    while paying(add(c, U256::ONE)?)? <= limit {
        c = add(c, U256::ONE)?;
    }
    let reserve = curve.k / add(add(curve.quote_reserve, c)?, U256::ONE)?;
    let amount = sub(sub(curve.token_reserve, reserve)?, U256::ONE)?;
    let quote = quote_buy_amount(curve, fee, amount)?;
    debug_assert!(quote.pay <= limit);
    Ok(quote)
}

/// The SELL of `amount` tokens (`Core.sell`).
pub fn quote_sell(
    curve: &OpenFourCurve,
    fee: &OpenFourFee,
    amount: U256,
) -> Result<OpenFourSellQuote, OpenFourQuoteError> {
    if amount.is_zero() {
        return Err(OpenFourQuoteError::ZeroInput);
    }
    checked(curve)?;
    let token_reserve = add(curve.token_reserve, amount)?;
    let quote_reserve = curve.k / token_reserve;
    let gross = sub(curve.quote_reserve, quote_reserve)?;
    let (protocol_fee, dev_fee) = fees(fee, gross)?;
    let received = sub(sub(gross, protocol_fee)?, dev_fee)?;
    if received.is_zero() {
        return Err(OpenFourQuoteError::ZeroOutput);
    }
    Ok(OpenFourSellQuote {
        amount,
        gross,
        protocol_fee,
        dev_fee,
        received,
        after: OpenFourCurve {
            k: curve.k,
            token_reserve,
            quote_reserve,
            remaining: add(curve.remaining, amount)?,
        },
    })
}

/// The creator's BUY inside `createToken` with `value`: the curve quote is
/// the value less the fees' share of it, the tokens the most it buys, and
/// they cost exactly that quote (witnessed for the priced creator values;
/// another outcome is refused).
pub fn creator_buy(
    curve: &OpenFourCurve,
    fee: &OpenFourFee,
    value: U256,
) -> Result<OpenFourBuyQuote, OpenFourQuoteError> {
    if value.is_zero() {
        return Err(OpenFourQuoteError::ZeroInput);
    }
    checked(curve)?;
    let charged = u16::try_from(u32::from(fee.protocol_bps) + u32::from(fee.dev_bps))
        .map_err(|_| OpenFourQuoteError::InvalidState)?;
    let quote = sub(value, part(value, charged)?)?;
    let reserve = curve.k / add(add(curve.quote_reserve, quote)?, U256::ONE)?;
    let amount = sub(sub(curve.token_reserve, reserve)?, U256::ONE)?;
    let bought = quote_buy_amount(curve, fee, amount)?;
    if bought.cost != quote {
        return Err(OpenFourQuoteError::InvalidState);
    }
    Ok(bought)
}

/// The BUY inputs admitted on a fresh launch of the bonding preset: those
/// whose exact BUY the probes executed on the state after the witnessed
/// launch (OFADMIT rows), in wei.
pub const QUALIFIED_OPENFOUR_INPUTS_WEI: [u64; 2] =
    [1_000_000_000_000_000, 100_000_000_000_000_000];

/// Why an OpenFour BUY or SELL is not admissible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenFourAdmissionError {
    GenerationMismatch,
    CandidateMismatch,
    PolicyGenerationMismatch,
    /// A trade of the token precedes the BUY.
    StateNotFresh,
    UnqualifiedAmount,
    /// The held token's state was read at another block than the snapshot's.
    StaleRead,
    Quote(OpenFourQuoteError),
    Slippage(SlippageError),
}

impl core::fmt::Display for OpenFourAdmissionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::GenerationMismatch => f.write_str("openfour admission: code generation mismatch"),
            Self::CandidateMismatch => {
                f.write_str("openfour admission: snapshot of another launch")
            }
            Self::PolicyGenerationMismatch => {
                f.write_str("openfour admission: policy generation mismatch")
            }
            Self::StateNotFresh => f.write_str("openfour admission: token state is not fresh"),
            Self::UnqualifiedAmount => f.write_str("openfour admission: amount not qualified"),
            Self::StaleRead => f.write_str("openfour admission: state read at another block"),
            Self::Quote(e) => write!(f, "openfour admission: {e}"),
            Self::Slippage(e) => write!(f, "openfour admission: {e}"),
        }
    }
}

impl std::error::Error for OpenFourAdmissionError {}

/// The one qualified state of an OpenFour launch: directly after it and its
/// creator BUY, before any other trade of its token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedOpenFourState {
    token: Address,
    curve: OpenFourCurve,
    fee: OpenFourFee,
    snapshot: SnapshotId,
}

impl QualifiedOpenFourState {
    /// Binds a classified launch to the state after it;
    /// `trades_since_launch` counts its token's trades ordered before the BUY.
    pub fn fresh_launch(
        classified: &ClassifiedOpenFourLaunch,
        trades_since_launch: u64,
        snapshot: SnapshotId,
    ) -> Result<Self, OpenFourAdmissionError> {
        if classified.codehash_generation() != snapshot.codehash_generation {
            return Err(OpenFourAdmissionError::GenerationMismatch);
        }
        if classified.candidate() != snapshot.pending_candidate {
            return Err(OpenFourAdmissionError::CandidateMismatch);
        }
        if trades_since_launch != 0 {
            return Err(OpenFourAdmissionError::StateNotFresh);
        }
        Ok(Self {
            token: classified.token(),
            curve: classified.curve(),
            fee: classified.fee(),
            snapshot,
        })
    }

    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }
}

/// An OpenFour BUY that passed admission: only [`admit_openfour_buy`]
/// creates one. Exactly `amount` tokens, `pay` on the admitted state,
/// `max_pay` (the input, the transaction's value) the most it may cost:
/// `0 < amount`, `pay * (1 + slippage) <= max_pay`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmittedOpenFourBuy {
    token: Address,
    amount: U256,
    pay: U256,
    max_pay: U256,
    slippage_bps: u16,
    snapshot: SnapshotId,
}

impl AdmittedOpenFourBuy {
    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// The exact token amount bought.
    #[inline]
    pub fn amount(&self) -> U256 {
        self.amount
    }

    /// What the amount costs on the admitted state (cost and fees).
    #[inline]
    pub fn pay(&self) -> U256 {
        self.pay
    }

    /// `maxPayAmount` and the transaction's value: the policy's input.
    #[inline]
    pub fn max_pay(&self) -> U256 {
        self.max_pay
    }

    #[inline]
    pub fn slippage_bps(&self) -> u16 {
        self.slippage_bps
    }

    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }

    /// The calldata of `Core.buy(token, amount, maxPay, 0, "")`.
    pub fn calldata(&self) -> Result<[u8; OPENFOUR_TRADE_LEN], AbiError> {
        encode_openfour_buy(self.token, self.amount, self.max_pay)
    }
}

/// Admits an OpenFour BUY in a qualified state under `policy`: the policy's
/// input is `maxPay`; the amount the largest whose payment, raised by the
/// slippage, fits it.
pub fn admit_openfour_buy(
    state: &QualifiedOpenFourState,
    policy: &BuyPolicy,
) -> Result<AdmittedOpenFourBuy, OpenFourAdmissionError> {
    if policy.generation() != state.snapshot.policy_generation {
        return Err(OpenFourAdmissionError::PolicyGenerationMismatch);
    }
    let max_pay = policy.input_amount();
    if !QUALIFIED_OPENFOUR_INPUTS_WEI
        .iter()
        .any(|w| U256::from(*w) == max_pay)
    {
        return Err(OpenFourAdmissionError::UnqualifiedAmount);
    }
    let whole = U256::from(BPS_DENOMINATOR);
    let limit = mul(max_pay, whole).map_err(|e| OpenFourAdmissionError::Quote(e.into()))?
        / (whole + U256::from(policy.slippage_bps()));
    let quote =
        max_buy_amount(&state.curve, &state.fee, limit).map_err(OpenFourAdmissionError::Quote)?;
    Ok(AdmittedOpenFourBuy {
        token: state.token,
        amount: quote.amount,
        pay: quote.pay,
        max_pay,
        slippage_bps: policy.slippage_bps(),
        snapshot: state.snapshot,
    })
}

/// A held OpenFour token's state at the snapshot's canonical block, the one
/// state its SELL is admitted on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedOpenFourHolding {
    token: Address,
    curve: OpenFourCurve,
    fee: OpenFourFee,
    snapshot: SnapshotId,
}

impl QualifiedOpenFourHolding {
    pub fn at(
        classified: &ClassifiedOpenFourHolding,
        snapshot: SnapshotId,
    ) -> Result<Self, OpenFourAdmissionError> {
        if classified.codehash_generation() != snapshot.codehash_generation {
            return Err(OpenFourAdmissionError::GenerationMismatch);
        }
        if classified.block_hash() != snapshot.canonical_block_hash {
            return Err(OpenFourAdmissionError::StaleRead);
        }
        Ok(Self {
            token: classified.token(),
            curve: classified.curve(),
            fee: classified.fee(),
            snapshot,
        })
    }

    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }
}

/// An OpenFour SELL that passed admission: only [`admit_openfour_sell`]
/// creates one. `Core.sell` compares `minReceiveAmount` with the seller's
/// NET proceeds (executed), so the minimum is the slippage floor of the
/// expected net. Invariants: `amount > 0`, `0 < min_receive <=
/// expected_received`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmittedOpenFourSell {
    token: Address,
    amount: U256,
    expected_received: U256,
    min_receive: U256,
    slippage_bps: u16,
    snapshot: SnapshotId,
}

impl AdmittedOpenFourSell {
    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    #[inline]
    pub fn amount(&self) -> U256 {
        self.amount
    }

    #[inline]
    pub fn expected_received(&self) -> U256 {
        self.expected_received
    }

    #[inline]
    pub fn min_receive(&self) -> U256 {
        self.min_receive
    }

    #[inline]
    pub fn slippage_bps(&self) -> u16 {
        self.slippage_bps
    }

    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }

    /// The calldata of `Core.sell(token, amount, minReceive, 0, "")`.
    pub fn calldata(&self) -> Result<[u8; OPENFOUR_TRADE_LEN], AbiError> {
        encode_openfour_sell(self.token, self.amount, self.min_receive)
    }
}

/// Admits a SELL of `amount` tokens of a held OpenFour token in its
/// qualified state under the exit `policy`.
pub fn admit_openfour_sell(
    state: &QualifiedOpenFourHolding,
    amount: U256,
    policy: &ExitPolicy,
) -> Result<AdmittedOpenFourSell, OpenFourAdmissionError> {
    if policy.generation() != state.snapshot.policy_generation {
        return Err(OpenFourAdmissionError::PolicyGenerationMismatch);
    }
    let quote =
        quote_sell(&state.curve, &state.fee, amount).map_err(OpenFourAdmissionError::Quote)?;
    let min_receive = min_output(quote.received, policy.slippage_bps())
        .map_err(OpenFourAdmissionError::Slippage)?;
    Ok(AdmittedOpenFourSell {
        token: state.token,
        amount,
        expected_received: quote.received,
        min_receive,
        slippage_bps: policy.slippage_bps(),
        snapshot: state.snapshot,
    })
}

#[cfg(test)]
pub(crate) mod tests;
