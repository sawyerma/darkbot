//! Four Classic (TokenManager2) native-BNB curve: exact BUY and SELL
//! arithmetic, the curve a launch starts with (R12), and the admission of a
//! BUY on the state directly after a classified launch.
//!
//! The implementations are not source-verified, so every rule here is an
//! execution fact, not a reading of code
//! (`fixtures/fourmeme/classic_parity_20261104.txt`, its header names the
//! calls and blocks): `buyTokenAMAP` and `sellToken` executed on mainnet
//! state (a probe through `eth_call`, the tokens and wei that moved), the
//! launch receipts' creator purchase, executed direct trades, and the
//! venue's quote helper (`Helper3.tryBuy`/`trySell`), which omits the
//! token's tax.
//!
//! The curve is a constant product of `K` over the virtual token reserve
//! `T` (both stored per token, `_tokenInfos`): the virtual quote reserve is
//! `y = K·1e18 / T`. `rate` is the venue's trading fee, `tax` the token's
//! (both in basis points of the curve amount).
//!
//! - BUY of `funds` wei: the budget `c = funds·1e4 / (1e4 + rate + tax)`,
//!   the tokens `floor_gwei(T − ceil(K·1e18 / (y + c)))`, the cost
//!   `K·1e18 / (T − tokens) − y`, the fee `cost·rate / 1e4`, the tax
//!   `cost·tax / 1e4`; the rest of `funds` is refunded. (The closed form
//!   `floor_gwei(T·c / (y + c))`, the helper's, is one gwei-unit more in 7
//!   of the 29 executed probes and equal in every helper row; the new
//!   reserve rounded down or up is the same in every witness, up is the
//!   smaller amount.)
//! - SELL of `tokens`: the curve pays `gross = y − K·1e18 / (T + tokens)`,
//!   less the fee `gross·rate / 1e4` and the tax `gross·tax / 1e4`.
//! - A launch of `totalSupply` S, `maxOffers` O and `maxRaising` R starts
//!   with `a = O·(S−O)·100 / (98·O − 100·(S−O))`, `T = O + a`,
//!   `K = floor(T/1e9) · floor(a·R / O / 1e9)`: the curve whose end price,
//!   after the O offered tokens, is 98 % of `R / (S−O)`. The creator's
//!   BUY inside the launch is a BUY of its `args` creator value (word 10).
//!
//! Every division floors but the BUY's new token reserve (rounded up).
//! The quotes are pure functions with typed errors: they accept any terms
//! and say nothing about whether a token, state or amount is qualified;
//! `venues::fourmeme::profile` gates what is priced.
//!
//! # Admission
//!
//! [`admit_classic_buy`] is the only producer of a sendable Classic BUY. It
//! needs a [`QualifiedClassicState`]: a launch the profile classified on a
//! validated deployment, the state directly after it (no trade of its token
//! before the BUY), the snapshot's code generation and pending candidate.
//! It admits only an amount the probes executed on such a state (0.001 and
//! 0.1 BNB), under the policy of the snapshot's generation, with a strictly
//! positive minimum (the slippage floor of the quote): TokenManager2
//! accepts the BUY at its tokens and reverts one above.

use alloy_primitives::{Address, U256};

use super::arith::{ArithError, add, mul, mul_div_down, mul_div_up, sub};
use super::fees::BPS_DENOMINATOR;
use super::policy::{BuyPolicy, ExitPolicy};
use super::slippage::{SlippageError, min_output};
use crate::chain_state::SnapshotId;
use crate::venues::fourmeme::abi::{
    AbiError, BUY_TOKEN_AMAP_LEN, SELL_TOKEN_MIN_FUNDS_LEN, encode_buy_token_amap,
    encode_sell_token_min_funds,
};
use crate::venues::fourmeme::profile::{ClassifiedClassicHolding, ClassifiedClassicLaunch};

/// Token amounts are whole gwei-units of the 18-decimal token (`1e9`).
const GWEI: u64 = 1_000_000_000;

/// The quote side's fixed point (`1e18`).
const ONE: u64 = 1_000_000_000_000_000_000;

/// The end price of the curve in percent of `maxRaising` over the reserved
/// tokens (the launch rule above).
const END_PRICE_PERCENT: u64 = 98;

/// Why a Classic quote has no result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassicQuoteError {
    /// The input amount is zero.
    ZeroInput,
    /// A state input is zero or inconsistent (no offers, `T` not above the
    /// offers, fee and tax at or above the whole, a launch whose terms give
    /// no curve).
    InvalidState,
    /// The quote is zero tokens or zero wei.
    ZeroOutput,
    /// The BUY takes every remaining offer: TokenManager2 caps it and adds
    /// the liquidity (graduation), which is not quoted.
    ReachesMaxOffers,
    /// A SELL of more tokens than the curve sold.
    ExceedsSold,
    /// A minimum trading fee above zero: no witness says how it combines.
    UnwitnessedMinFee,
    Arith(ArithError),
}

impl From<ArithError> for ClassicQuoteError {
    fn from(e: ArithError) -> Self {
        Self::Arith(e)
    }
}

impl core::fmt::Display for ClassicQuoteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ZeroInput => f.write_str("four classic: input amount is zero"),
            Self::InvalidState => f.write_str("four classic: curve state outside the rule"),
            Self::ZeroOutput => f.write_str("four classic: output is zero"),
            Self::ReachesMaxOffers => f.write_str("four classic: takes every remaining offer"),
            Self::ExceedsSold => f.write_str("four classic: sells more than the curve sold"),
            Self::UnwitnessedMinFee => f.write_str("four classic: minimum trading fee unwitnessed"),
            Self::Arith(e) => write!(f, "four classic: {e}"),
        }
    }
}

impl std::error::Error for ClassicQuoteError {}

/// The terms of a Classic launch (its `createToken` arguments).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchTerms {
    pub total_supply: U256,
    pub max_offers: U256,
    pub max_raising: U256,
}

/// One token's curve (`_tokenInfos`: `K`, `T`, `offers`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicCurve {
    pub k: U256,
    /// The virtual token reserve.
    pub t: U256,
    /// Tokens still offered on the curve.
    pub offers: U256,
    /// Tokens the curve sold (`maxOffers − offers`).
    pub sold: U256,
}

/// The fee terms of a token: the venue's (`Helper3.getTokenInfo`) and the
/// token's tax (its template word's, measured by execution).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicFee {
    /// `tradingFeeRate`, in basis points.
    pub rate_bps: u16,
    /// `minTradingFee`, in wei.
    pub min_fee: U256,
    /// The token's tax on either side, in basis points.
    pub tax_bps: u16,
}

/// A BUY on the curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicBuyQuote {
    /// Tokens received; strictly positive, below the remaining offers.
    pub amount: U256,
    /// Wei the curve takes for them.
    pub cost: U256,
    /// Trading fee, in wei.
    pub fee: U256,
    /// The token's tax, in wei.
    pub tax: U256,
    /// Wei the BUY takes (`cost + fee + tax`); the rest of the funds is
    /// refunded.
    pub spent: U256,
    /// The curve after the BUY.
    pub after: ClassicCurve,
}

/// A SELL on the curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicSellQuote {
    /// Wei the curve pays before the fee and the tax.
    pub gross: U256,
    /// Trading fee, in wei.
    pub fee: U256,
    /// The token's tax, in wei.
    pub tax: U256,
    /// Wei the seller receives; strictly positive.
    pub funds: U256,
    /// The curve after the SELL.
    pub after: ClassicCurve,
}

/// `x` in whole gwei token units (every witnessed Classic trade's amount
/// is; SC4: a Strategy C SELL sells that much of what it holds).
pub fn floor_gwei(x: U256) -> U256 {
    let g = U256::from(GWEI);
    x / g * g
}

/// `floor(x · bps / 1e4)`.
fn part(x: U256, bps: u16) -> Result<U256, ClassicQuoteError> {
    Ok(mul_div_down(
        x,
        U256::from(bps),
        U256::from(BPS_DENOMINATOR),
    )?)
}

/// `K·1e18 / T`: the virtual quote reserve.
/// The curve's virtual quote reserve `floor(K · 1e18 / t)`: what it raised
/// since the launch is its difference to the launch curve's (SC4: the
/// `funds` a Classic trade reports).
pub fn virtual_quote(curve: &ClassicCurve) -> Result<U256, ClassicQuoteError> {
    Ok(mul_div_down(curve.k, U256::from(ONE), curve.t)?)
}

fn checked(curve: &ClassicCurve) -> Result<(), ClassicQuoteError> {
    if curve.k.is_zero() || curve.offers.is_zero() || curve.t <= curve.offers {
        return Err(ClassicQuoteError::InvalidState);
    }
    Ok(())
}

/// The fee and the tax together, in basis points, below the whole.
fn charged_bps(fee: &ClassicFee) -> Result<u16, ClassicQuoteError> {
    if !fee.min_fee.is_zero() {
        return Err(ClassicQuoteError::UnwitnessedMinFee);
    }
    match fee.rate_bps.checked_add(fee.tax_bps) {
        Some(charged) if charged < BPS_DENOMINATOR => Ok(charged),
        _ => Err(ClassicQuoteError::InvalidState),
    }
}

/// The virtual reserves `(T, Q)` a launch with `terms` starts with: the
/// token reserve `maxOffers + a` and the quote reserve `a · maxRaising /
/// maxOffers`, `a` priced so the curve ends at 98 % of the raise per token
/// (R12d: OpenFour's bonding curve starts at the same reserves).
pub fn virtual_reserves(terms: &LaunchTerms) -> Result<(U256, U256), ClassicQuoteError> {
    let offers = terms.max_offers;
    let reserved = sub(terms.total_supply, offers).map_err(|_| ClassicQuoteError::InvalidState)?;
    if offers.is_zero() || reserved.is_zero() || terms.max_raising.is_zero() {
        return Err(ClassicQuoteError::InvalidState);
    }
    let hundred = U256::from(100u8);
    let scaled_offers = mul(U256::from(END_PRICE_PERCENT), offers)?;
    let scaled_reserved = mul(hundred, reserved)?;
    if scaled_offers <= scaled_reserved {
        return Err(ClassicQuoteError::InvalidState);
    }
    let a = mul_div_down(
        mul(offers, reserved)?,
        hundred,
        scaled_offers - scaled_reserved,
    )?;
    Ok((add(offers, a)?, mul_div_down(a, terms.max_raising, offers)?))
}

/// The curve a launch with `terms` starts with.
pub fn launch_curve(terms: &LaunchTerms) -> Result<ClassicCurve, ClassicQuoteError> {
    let (t, quote) = virtual_reserves(terms)?;
    let g = U256::from(GWEI);
    let k = mul(t / g, quote / g)?;
    let curve = ClassicCurve {
        k,
        t,
        offers: terms.max_offers,
        sold: U256::ZERO,
    };
    checked(&curve)?;
    Ok(curve)
}

/// The BUY `buyTokenAMAP(token, funds, minAmount)` with `msg.value = funds`
/// makes (and the creator's BUY inside `createToken` with its value):
/// `spent <= funds`, the rest refunded.
pub fn quote_buy(
    curve: &ClassicCurve,
    fee: &ClassicFee,
    funds: U256,
) -> Result<ClassicBuyQuote, ClassicQuoteError> {
    if funds.is_zero() {
        return Err(ClassicQuoteError::ZeroInput);
    }
    let charged = charged_bps(fee)?;
    checked(curve)?;
    let whole = U256::from(BPS_DENOMINATOR);
    let budget = mul_div_down(funds, whole, whole + U256::from(charged))?;
    let y = virtual_quote(curve)?;
    // The virtual token reserve after the BUY, rounded up.
    let reserve = mul_div_up(curve.k, U256::from(ONE), add(y, budget)?)?;
    let amount = if reserve < curve.t {
        floor_gwei(curve.t - reserve)
    } else {
        U256::ZERO
    };
    if amount.is_zero() {
        return Err(ClassicQuoteError::ZeroOutput);
    }
    if amount >= curve.offers {
        return Err(ClassicQuoteError::ReachesMaxOffers);
    }
    let t = curve.t - amount;
    let cost = sub(mul_div_down(curve.k, U256::from(ONE), t)?, y)?;
    if cost.is_zero() {
        return Err(ClassicQuoteError::ZeroOutput);
    }
    let (fee_wei, tax, spent) = split_purchase(fee, cost)?;
    if spent > funds {
        return Err(ClassicQuoteError::InvalidState);
    }
    Ok(ClassicBuyQuote {
        amount,
        cost,
        fee: fee_wei,
        tax,
        spent,
        after: ClassicCurve {
            k: curve.k,
            t,
            offers: curve.offers - amount,
            sold: add(curve.sold, amount)?,
        },
    })
}

/// A BUY's curve `cost` with what TokenManager2 charges on it: the trading
/// fee, the token's tax and the buyer's spend.
fn split_purchase(fee: &ClassicFee, cost: U256) -> Result<(U256, U256, U256), ClassicQuoteError> {
    let fee_wei = part(cost, fee.rate_bps)?;
    let tax = part(cost, fee.tax_bps)?;
    Ok((fee_wei, tax, add(add(cost, fee_wei)?, tax)?))
}

/// SC4 d: what a BUY whose curve cost `cost` (a TokenPurchase's `cost`)
/// spent under `fee`, as [`quote_buy`] charges it.
pub fn purchase_spent(fee: &ClassicFee, cost: U256) -> Result<U256, ClassicQuoteError> {
    charged_bps(fee)?;
    Ok(split_purchase(fee, cost)?.2)
}

/// A SELL's `gross` (the curve's wei) split as TokenManager2 pays it: the
/// trading fee, the token's tax and the seller's funds.
fn split_sale(fee: &ClassicFee, gross: U256) -> Result<(U256, U256, U256), ClassicQuoteError> {
    let fee_wei = part(gross, fee.rate_bps)?;
    let tax = part(gross, fee.tax_bps)?;
    Ok((fee_wei, tax, sub(gross, add(fee_wei, tax)?)?))
}

/// SC4 d: the funds a SELL whose curve paid `gross` (a TokenSale's `cost`)
/// pays the seller under `fee`, as [`quote_sell`] splits it.
pub fn sale_funds(fee: &ClassicFee, gross: U256) -> Result<U256, ClassicQuoteError> {
    charged_bps(fee)?;
    Ok(split_sale(fee, gross)?.2)
}

/// The SELL of `amount` tokens (`sellToken(token, amount)`): at most what
/// the curve sold is taken back.
pub fn quote_sell(
    curve: &ClassicCurve,
    fee: &ClassicFee,
    amount: U256,
) -> Result<ClassicSellQuote, ClassicQuoteError> {
    if amount.is_zero() {
        return Err(ClassicQuoteError::ZeroInput);
    }
    charged_bps(fee)?;
    checked(curve)?;
    if amount > curve.sold {
        return Err(ClassicQuoteError::ExceedsSold);
    }
    let y = virtual_quote(curve)?;
    let t = add(curve.t, amount)?;
    let gross = sub(y, mul_div_down(curve.k, U256::from(ONE), t)?)?;
    let (fee_wei, tax, funds) = split_sale(fee, gross)?;
    if funds.is_zero() {
        return Err(ClassicQuoteError::ZeroOutput);
    }
    Ok(ClassicSellQuote {
        gross,
        fee: fee_wei,
        tax,
        funds,
        after: ClassicCurve {
            k: curve.k,
            t,
            offers: add(curve.offers, amount)?,
            sold: curve.sold - amount,
        },
    })
}

/// The BUY amounts admitted: the funds the probes executed on the state
/// directly after witnessed launches and (SC4: Strategy C's tracked states)
/// on later states of the priced profile (the PROBE rows; SC4 0.01 BNB on
/// both), in wei.
pub const QUALIFIED_CLASSIC_INPUTS_WEI: [u64; 3] = [
    1_000_000_000_000_000,
    10_000_000_000_000_000,
    100_000_000_000_000_000,
];

/// Why a Classic BUY is not admissible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassicAdmissionError {
    /// Classified under another code generation than the snapshot's.
    GenerationMismatch,
    /// The snapshot assumes another launch transaction.
    CandidateMismatch,
    /// The policy has another generation than the snapshot.
    PolicyGenerationMismatch,
    /// A trade of the token precedes the BUY.
    StateNotFresh,
    /// An amount no probe executed on a fresh launch.
    UnqualifiedAmount,
    /// R12c-3: the held token's state was read at another block than the
    /// snapshot's.
    StaleRead,
    /// R12c-3: a SELL of a token amount that is not whole gwei units (every
    /// witnessed trade's is).
    UnalignedAmount,
    /// SC4: a tracked curve's fee terms are not the validated deployment's.
    TermsMismatch,
    /// SC4 c (B2): a tracked state further along its curve than any state
    /// a BUY was executed from ([`QUALIFIED_CLASSIC_MIN_OFFERS`]).
    Unwitnessed,
    Quote(ClassicQuoteError),
    Slippage(SlippageError),
}

impl core::fmt::Display for ClassicAdmissionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::GenerationMismatch => f.write_str("classic admission: code generation mismatch"),
            Self::CandidateMismatch => f.write_str("classic admission: snapshot of another launch"),
            Self::PolicyGenerationMismatch => {
                f.write_str("classic admission: policy generation mismatch")
            }
            Self::StateNotFresh => f.write_str("classic admission: token state is not fresh"),
            Self::UnqualifiedAmount => f.write_str("classic admission: amount not qualified"),
            Self::StaleRead => f.write_str("classic admission: state read at another block"),
            Self::UnalignedAmount => {
                f.write_str("classic admission: amount not in whole gwei units")
            }
            Self::TermsMismatch => {
                f.write_str("classic admission: fee terms not the validated deployment's")
            }
            Self::Unwitnessed => {
                f.write_str("classic admission: state deeper on its curve than any executed BUY's")
            }
            Self::Quote(e) => write!(f, "classic admission: {e}"),
            Self::Slippage(e) => write!(f, "classic admission: {e}"),
        }
    }
}

impl std::error::Error for ClassicAdmissionError {}

/// The one qualified state of a Classic launch: directly after it, before
/// any other trade of its token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedClassicState {
    token: Address,
    curve: ClassicCurve,
    fee: ClassicFee,
    snapshot: SnapshotId,
    /// SC4 c: a tracked (moved) state, not a launch's: its BUY only of
    /// [`QUALIFIED_CLASSIC_TRACKED_INPUTS_WEI`].
    tracked: bool,
}

impl QualifiedClassicState {
    /// Binds a classified launch to the state after it; `trades_since_launch`
    /// counts the trades of its token ordered before the BUY in `snapshot`.
    pub fn fresh_launch(
        classified: &ClassifiedClassicLaunch,
        trades_since_launch: u64,
        snapshot: SnapshotId,
    ) -> Result<Self, ClassicAdmissionError> {
        if classified.codehash_generation() != snapshot.codehash_generation {
            return Err(ClassicAdmissionError::GenerationMismatch);
        }
        if classified.candidate() != snapshot.pending_candidate {
            return Err(ClassicAdmissionError::CandidateMismatch);
        }
        if trades_since_launch != 0 {
            return Err(ClassicAdmissionError::StateNotFresh);
        }
        Ok(Self {
            token: classified.token(),
            curve: classified.curve(),
            fee: classified.fee(),
            snapshot,
            tracked: false,
        })
    }

    /// SC4: binds a token's curve as Strategy C tracks it exactly (the state
    /// its reported trades name, its launch's classified fee terms) to
    /// `snapshot`, on the `validated` deployment the BUY is armed on: its
    /// code generation the snapshot's, its venue fee the curve's.
    pub fn tracked(
        validated: &crate::venues::fourmeme::deployment::ValidatedFourDeployment,
        token: Address,
        curve: ClassicCurve,
        fee: ClassicFee,
        snapshot: SnapshotId,
    ) -> Result<Self, ClassicAdmissionError> {
        tracked_terms(validated, &fee, &snapshot)?;
        if curve.offers < QUALIFIED_CLASSIC_MIN_OFFERS {
            return Err(ClassicAdmissionError::Unwitnessed);
        }
        Ok(Self {
            token,
            curve,
            fee,
            snapshot,
            tracked: true,
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

/// SC4 c (B2, review M1): the inputs a BUY was executed with on states
/// third-party trades moved (fixture PROBE rows of moved curves): only the
/// Strategy C input; a tracked BUY of another input is not admitted (0.001
/// and 0.1 BNB were executed on launch states only).
pub const QUALIFIED_CLASSIC_TRACKED_INPUTS_WEI: [u64; 1] = [10_000_000_000_000_000];

/// SC4 c (B2): the offers still for sale of the deepest Classic state a
/// BUY of the priced word was executed from (fixture PROBE rows on curves
/// third-party BUYs and SELLs moved; the newest 6000 tokens held none
/// deeper and none near sell-out): a tracked BUY further along its curve is
/// not admitted. Exits are not bounded by it (their `minFunds` refuses a
/// SELL the quote does not name).
pub const QUALIFIED_CLASSIC_MIN_OFFERS: U256 =
    U256::from_limbs([5_532_986_722_337_444_864, 42_068_846, 0, 0]);

/// SC4: a tracked curve's generation and venue fee against the validated
/// deployment.
fn tracked_terms(
    validated: &crate::venues::fourmeme::deployment::ValidatedFourDeployment,
    fee: &ClassicFee,
    snapshot: &SnapshotId,
) -> Result<(), ClassicAdmissionError> {
    if validated.codehash_generation() != snapshot.codehash_generation {
        return Err(ClassicAdmissionError::GenerationMismatch);
    }
    let venue = validated.venue_fee();
    if (venue.rate_bps, venue.min_fee) != (fee.rate_bps, fee.min_fee) {
        return Err(ClassicAdmissionError::TermsMismatch);
    }
    // Review L3: its tax a priced word's (the launch's classification).
    if !crate::venues::fourmeme::profile::CLASSIC_PROFILES
        .iter()
        .any(|p| p.tax_bps == fee.tax_bps)
    {
        return Err(ClassicAdmissionError::TermsMismatch);
    }
    Ok(())
}

/// A Classic BUY that passed admission: only [`admit_classic_buy`] creates
/// one. Invariants: `funds > 0`, `0 < min_tokens <= expected_tokens`,
/// `spent <= funds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmittedClassicBuy {
    token: Address,
    funds: U256,
    expected_tokens: U256,
    min_tokens: U256,
    spent: U256,
    slippage_bps: u16,
    snapshot: SnapshotId,
}

impl AdmittedClassicBuy {
    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// The BUY's `funds` and `msg.value`, in wei.
    #[inline]
    pub fn funds(&self) -> U256 {
        self.funds
    }

    #[inline]
    pub fn expected_tokens(&self) -> U256 {
        self.expected_tokens
    }

    /// The `minAmount` bound into the calldata.
    #[inline]
    pub fn min_tokens(&self) -> U256 {
        self.min_tokens
    }

    /// Wei the quote takes (cost, fee and tax); the rest is refunded.
    #[inline]
    pub fn spent(&self) -> U256 {
        self.spent
    }

    #[inline]
    pub fn slippage_bps(&self) -> u16 {
        self.slippage_bps
    }

    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }

    /// The calldata of the direct `buyTokenAMAP(token, funds, minAmount)`.
    pub fn calldata(&self) -> Result<[u8; BUY_TOKEN_AMAP_LEN], AbiError> {
        encode_buy_token_amap(self.token, self.funds, self.min_tokens)
    }
}

/// Admits a Classic BUY in a qualified state under `policy`.
pub fn admit_classic_buy(
    state: &QualifiedClassicState,
    policy: &BuyPolicy,
) -> Result<AdmittedClassicBuy, ClassicAdmissionError> {
    if policy.generation() != state.snapshot.policy_generation {
        return Err(ClassicAdmissionError::PolicyGenerationMismatch);
    }
    let funds = policy.input_amount();
    let inputs: &[u64] = match state.tracked {
        true => &QUALIFIED_CLASSIC_TRACKED_INPUTS_WEI,
        false => &QUALIFIED_CLASSIC_INPUTS_WEI,
    };
    if !inputs.iter().any(|w| U256::from(*w) == funds) {
        return Err(ClassicAdmissionError::UnqualifiedAmount);
    }
    let quote = quote_buy(&state.curve, &state.fee, funds).map_err(ClassicAdmissionError::Quote)?;
    let min_tokens =
        min_output(quote.amount, policy.slippage_bps()).map_err(ClassicAdmissionError::Slippage)?;
    Ok(AdmittedClassicBuy {
        token: state.token,
        funds,
        expected_tokens: quote.amount,
        min_tokens,
        spent: quote.spent,
        slippage_bps: policy.slippage_bps(),
        snapshot: state.snapshot,
    })
}

/// R12c-3: a held token's state at the snapshot's canonical block, the
/// one state its SELL is admitted on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualifiedClassicHolding {
    token: Address,
    curve: ClassicCurve,
    fee: ClassicFee,
    snapshot: SnapshotId,
}

impl QualifiedClassicHolding {
    /// Binds a classified holding to `snapshot` (a canonical snapshot of
    /// the block it was read at, under its code generation).
    pub fn at(
        classified: &ClassifiedClassicHolding,
        snapshot: SnapshotId,
    ) -> Result<Self, ClassicAdmissionError> {
        if classified.codehash_generation() != snapshot.codehash_generation {
            return Err(ClassicAdmissionError::GenerationMismatch);
        }
        if classified.block_hash() != snapshot.canonical_block_hash {
            return Err(ClassicAdmissionError::StaleRead);
        }
        Ok(Self {
            token: classified.token(),
            curve: classified.curve(),
            fee: classified.fee(),
            snapshot,
        })
    }

    /// SC4: a PAPER position's token on the curve Strategy C tracks
    /// exactly, bound to `snapshot` on the `validated` deployment (as
    /// [`QualifiedClassicState::tracked`]).
    pub fn tracked(
        validated: &crate::venues::fourmeme::deployment::ValidatedFourDeployment,
        token: Address,
        curve: ClassicCurve,
        fee: ClassicFee,
        snapshot: SnapshotId,
    ) -> Result<Self, ClassicAdmissionError> {
        tracked_terms(validated, &fee, &snapshot)?;
        Ok(Self {
            token,
            curve,
            fee,
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

/// A Classic SELL that passed admission: only [`admit_classic_sell`]
/// creates one. TokenManager2 compares `minFunds` with the SELL's gross
/// (the curve's wei before the trading fee and the token's tax: executed,
/// the gross passes and one wei above it reverts), so the minimum is the
/// slippage floor of the expected gross. Invariants: `amount > 0`,
/// `0 < min_funds <= expected_gross`, `expected_funds < expected_gross`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdmittedClassicSell {
    token: Address,
    amount: U256,
    expected_gross: U256,
    expected_funds: U256,
    min_funds: U256,
    slippage_bps: u16,
    snapshot: SnapshotId,
}

impl AdmittedClassicSell {
    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// Tokens to sell (the approval covers exactly these).
    #[inline]
    pub fn amount(&self) -> U256 {
        self.amount
    }

    /// Wei the quote expects to the seller (after the fee and the tax).
    #[inline]
    pub fn expected_funds(&self) -> U256 {
        self.expected_funds
    }

    /// Wei the quote expects from the curve before the fee and the tax
    /// (what TokenManager2 compares `minFunds` with).
    #[inline]
    pub fn expected_gross(&self) -> U256 {
        self.expected_gross
    }

    /// The `minFunds` bound into the calldata: the slippage floor of the
    /// expected gross.
    #[inline]
    pub fn min_funds(&self) -> U256 {
        self.min_funds
    }

    #[inline]
    pub fn slippage_bps(&self) -> u16 {
        self.slippage_bps
    }

    #[inline]
    pub fn snapshot(&self) -> SnapshotId {
        self.snapshot
    }

    /// The calldata of the direct `sellToken(token, amount, minFunds)`.
    pub fn calldata(&self) -> Result<[u8; SELL_TOKEN_MIN_FUNDS_LEN], AbiError> {
        encode_sell_token_min_funds(self.token, self.amount, self.min_funds)
    }
}

/// Admits a SELL of `amount` tokens of a held token in its qualified state
/// under the exit `policy`.
pub fn admit_classic_sell(
    state: &QualifiedClassicHolding,
    amount: U256,
    policy: &ExitPolicy,
) -> Result<AdmittedClassicSell, ClassicAdmissionError> {
    if policy.generation() != state.snapshot.policy_generation {
        return Err(ClassicAdmissionError::PolicyGenerationMismatch);
    }
    if amount % U256::from(GWEI) != U256::ZERO {
        return Err(ClassicAdmissionError::UnalignedAmount);
    }
    let quote =
        quote_sell(&state.curve, &state.fee, amount).map_err(ClassicAdmissionError::Quote)?;
    let min_funds =
        min_output(quote.gross, policy.slippage_bps()).map_err(ClassicAdmissionError::Slippage)?;
    Ok(AdmittedClassicSell {
        token: state.token,
        amount,
        expected_gross: quote.gross,
        expected_funds: quote.funds,
        min_funds,
        slippage_bps: policy.slippage_bps(),
        snapshot: state.snapshot,
    })
}

#[cfg(test)]
mod tests;
