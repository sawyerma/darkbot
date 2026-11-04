//! Fail-closed classification of a decoded Classic launch into a priced
//! profile (R12): the decoder ([`super::recognize`]) is factual; this is the
//! separate admission step. A launch is accepted only when every field the
//! BUY depends on has the value of witnessed launches, and it is refused
//! with the first deviating field otherwise.
//!
//! # Profile: a template word of the table
//!
//! The template word (`args` word 8, stored as `_tokenInfos.template`)
//! selects the token's mode: its last byte the quote asset (0 native BNB in
//! every witness; other values launched BSC-USD and other quotes), other
//! bytes fee modes (an AntiSniper word's `feeSetting` is not zero) and the
//! token's tax on both sides, which the venue's quote helper does not
//! show. Its meaning is not documented, so each word is priced only as
//! witnessed (`fixtures/fourmeme/classic_parity_20261104.txt`): native BNB,
//! `feeSetting` 0, its tax measured by executed BUYs and SELLs, the
//! launches' curve and creator purchase reproduced to the wei, their tokens
//! all CREATE2 under one frozen (deployer, template) pair, which therefore
//! names the token before execution. Every other word is refused.
//!
//! Bound with it: `totalSupply` and `maxOffers` of every witness, the
//! witnessed layout and the unknown `args` words zero, a creator value of
//! the word's witnessed ones and equal to the call's `msg.value` (larger
//! creator values of other words send 1.01 times it), and the venue's fee
//! terms. `maxRaising` is free: the launch curve reproduced across every
//! witnessed value (13.5 to 10 000 in quote units) and a BUY depends on
//! `K` and `T` only.
//!
//! The venue's fee terms are TokenManager2's, the same for every witnessed
//! token: the drift probe reads them before the launch exists
//! (`Helper3.getTokenInfo` of the deployment's reference token), and the
//! validated deployment carries them.
//!
//! R12c-3: a held token's state, read at one block
//! ([`ClassicTokenRead`]), is priced by the same table for its SELL
//! ([`classify_classic_holding`]): native, a priced word, the witnessed
//! supply, offers, `feeSetting`, `extraFee` and its own fee terms, within
//! its word's witnessed tax age, and only while it trades on its curve.

use alloy_primitives::{Address, B256, U256, b256};

use super::deployment::{OPENFOUR_MODULES, ValidatedFourDeployment};
use super::recognize::{ClassicLaunch, OpenFourLaunch};
use crate::economics::four_classic::{
    ClassicBuyQuote, ClassicCurve, ClassicFee, ClassicQuoteError, LaunchTerms, launch_curve,
    quote_buy,
};
use crate::economics::openfour::{
    OpenFourCurve, OpenFourFee, OpenFourQuoteError, creator_buy,
    launch_curve as openfour_launch_curve,
};

/// One priced template word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicTemplateProfile {
    pub template: B256,
    /// The index of the frozen (deployer, template) pair every witnessed
    /// token of the word is a CREATE2 clone under.
    pub pair: usize,
    /// The token's tax on either side, in basis points (executed BUYs and
    /// SELLs, the creator purchases).
    pub tax_bps: u16,
    /// The tax is witnessed from the launch block to this many blocks after
    /// it; a later state of the token is not priced.
    pub tax_witnessed_blocks: u64,
    /// The creator values (`args` word 10, wei) of its witnessed launches.
    pub creator_values: &'static [U256],
}

/// The priced template words (see the module doc; each line of the
/// fixture names its word).
pub const CLASSIC_PROFILES: &[ClassicTemplateProfile] = &[ClassicTemplateProfile {
    template: b256!("0665608cae7fb160a2bb9f169e1a0ccd6ff4aaac0000000000640a0a00002400"),
    pair: 0,
    tax_bps: 1_000,
    tax_witnessed_blocks: 461_245,
    creator_values: &[U256::from_limbs([1_780_000_000_000, 0, 0, 0])],
}];

/// `totalSupply` of every witnessed launch (1e9 tokens of 18 decimals).
pub const WITNESSED_TOTAL_SUPPLY: U256 = U256::from_limbs([0x9fd0803ce8000000, 0x33b2e3c, 0, 0]);
/// `maxOffers` of every witnessed launch (8e8 tokens).
pub const WITNESSED_MAX_OFFERS: U256 = U256::from_limbs([0xe640669720000000, 0x295be96, 0, 0]);

/// The venue's fee terms (`Helper3.getTokenInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VenueFee {
    /// `tradingFeeRate`, in basis points.
    pub rate_bps: u16,
    /// `minTradingFee`, in wei.
    pub min_fee: U256,
}

/// The venue's fee terms of every witnessed token (`tradingFeeRate` 100,
/// `minTradingFee` 0).
pub const WITNESSED_VENUE_FEE: VenueFee = VenueFee {
    rate_bps: 100,
    min_fee: U256::ZERO,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedClassicLaunch {
    /// The deployment's Classic economics are not witnessed by execution.
    EconomicsNotWitnessed,
    /// `args` is not the witnessed layout.
    Layout,
    /// A head word of `args` whose meaning no witness proves is not zero.
    UnknownArgument,
    TotalSupply,
    MaxOffers,
    /// The template word is not in [`CLASSIC_PROFILES`].
    Template,
    /// The creator value of `args` differs from the call's `msg.value`.
    CreatorValue,
    /// A creator value the word was not witnessed with.
    UnwitnessedCreatorValue,
    /// The venue's fee terms differ from [`WITNESSED_VENUE_FEE`].
    FeeTerms,
    /// The launch's curve or creator purchase has no quote.
    Curve(ClassicQuoteError),
}

impl core::fmt::Display for UnsupportedClassicLaunch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EconomicsNotWitnessed => {
                f.write_str("classic launch: the deployment's economics are not witnessed")
            }
            Self::Layout => f.write_str("classic launch: args layout not witnessed"),
            Self::UnknownArgument => f.write_str("classic launch: an unwitnessed argument is set"),
            Self::TotalSupply => f.write_str("classic launch: totalSupply"),
            Self::MaxOffers => f.write_str("classic launch: maxOffers"),
            Self::Template => f.write_str("classic launch: template word not priced"),
            Self::CreatorValue => f.write_str("classic launch: creator value is not msg.value"),
            Self::UnwitnessedCreatorValue => {
                f.write_str("classic launch: creator value not witnessed for the word")
            }
            Self::FeeTerms => f.write_str("classic launch: fee terms not witnessed"),
            Self::Curve(e) => write!(f, "classic launch: {e}"),
        }
    }
}

impl std::error::Error for UnsupportedClassicLaunch {}

/// A launch of a priced profile: its token and the curve directly after it
/// (after the creator's BUY, before any other trade).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassifiedClassicLaunch {
    candidate: B256,
    codehash_generation: u64,
    token: Address,
    template: B256,
    curve: ClassicCurve,
    fee: ClassicFee,
    creator_purchase: Option<ClassicBuyQuote>,
}

impl ClassifiedClassicLaunch {
    /// The hash of the launch transaction the calldata came from.
    #[inline]
    pub fn candidate(&self) -> B256 {
        self.candidate
    }

    /// The code generation of the deployment it was classified under.
    #[inline]
    pub fn codehash_generation(&self) -> u64 {
        self.codehash_generation
    }

    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    #[inline]
    pub fn template(&self) -> B256 {
        self.template
    }

    /// The curve directly after the launch.
    #[inline]
    pub fn curve(&self) -> ClassicCurve {
        self.curve
    }

    /// The terms the token trades under: the venue's fee, the word's tax.
    #[inline]
    pub fn fee(&self) -> ClassicFee {
        self.fee
    }

    /// The creator's BUY inside the launch, if its value is not zero.
    #[inline]
    pub fn creator_purchase(&self) -> Option<ClassicBuyQuote> {
        self.creator_purchase
    }
}

/// Classifies `launch`, sent with `value` in transaction `candidate`, on
/// the validated `deployment` under the fee terms it observed.
pub fn classify_classic_launch(
    validated: &ValidatedFourDeployment,
    launch: &ClassicLaunch<'_>,
    value: U256,
    candidate: B256,
) -> Result<ClassifiedClassicLaunch, UnsupportedClassicLaunch> {
    let venue = validated.venue_fee();
    if !validated.classic_economics_witnessed() {
        return Err(UnsupportedClassicLaunch::EconomicsNotWitnessed);
    }
    let args = launch
        .launch_args()
        .map_err(|_| UnsupportedClassicLaunch::Layout)?;
    if args.unknown.iter().any(|w| *w != B256::ZERO) {
        return Err(UnsupportedClassicLaunch::UnknownArgument);
    }
    if args.total_supply != WITNESSED_TOTAL_SUPPLY {
        return Err(UnsupportedClassicLaunch::TotalSupply);
    }
    if args.max_offers != WITNESSED_MAX_OFFERS {
        return Err(UnsupportedClassicLaunch::MaxOffers);
    }
    let profile = CLASSIC_PROFILES
        .iter()
        .find(|p| p.template == args.template)
        .ok_or(UnsupportedClassicLaunch::Template)?;
    if args.creator_value != value {
        return Err(UnsupportedClassicLaunch::CreatorValue);
    }
    if !profile.creator_values.contains(&args.creator_value) {
        return Err(UnsupportedClassicLaunch::UnwitnessedCreatorValue);
    }
    if venue != WITNESSED_VENUE_FEE {
        return Err(UnsupportedClassicLaunch::FeeTerms);
    }
    let fee = ClassicFee {
        rate_bps: venue.rate_bps,
        min_fee: venue.min_fee,
        tax_bps: profile.tax_bps,
    };
    let launched = launch_curve(&LaunchTerms {
        total_supply: args.total_supply,
        max_offers: args.max_offers,
        max_raising: args.max_raising,
    })
    .map_err(UnsupportedClassicLaunch::Curve)?;
    let creator_purchase = if args.creator_value.is_zero() {
        None
    } else {
        Some(
            quote_buy(&launched, &fee, args.creator_value)
                .map_err(UnsupportedClassicLaunch::Curve)?,
        )
    };
    Ok(ClassifiedClassicLaunch {
        candidate,
        codehash_generation: validated.codehash_generation(),
        token: validated.deployment().classic_tokens(launch.salt)[profile.pair],
        template: args.template,
        curve: creator_purchase.map_or(launched, |p| p.after),
        fee,
        creator_purchase,
    })
}

/// A held Classic token as read at one canonical block (R12c-3): its
/// `TokenManager2._tokenInfos` and `_tokenInfoEx1s` words and its
/// `Helper3.getTokenInfo` fee terms, all at `block_hash`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicTokenRead {
    pub token: Address,
    pub block_number: u64,
    pub block_hash: B256,
    pub quote: Address,
    pub template: B256,
    pub total_supply: U256,
    pub max_offers: U256,
    pub offers: U256,
    pub k: U256,
    pub t: U256,
    /// `_tokenInfos.status`: zero while the token trades on its curve.
    pub status: U256,
    /// `_tokenInfoEx1s.feeSetting`.
    pub fee_setting: U256,
    /// `_tokenInfoEx1s.extraFee` (zero on every witnessed token).
    pub extra_fee: U256,
    /// `_tokenInfoEx1s.blockNumber`: the token's launch block.
    pub launch_block: u64,
    /// `Helper3.getTokenInfo(token)`: its fee terms and whether its
    /// liquidity was added (it left the curve).
    pub venue_fee: VenueFee,
    pub liquidity_added: bool,
}

/// Why a held Classic token's state is not priced (the first deviating
/// field).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedClassicHolding {
    /// The deployment's Classic economics are not witnessed by execution.
    EconomicsNotWitnessed,
    /// Not quoted in native BNB.
    Quote,
    /// The template word is not in [`CLASSIC_PROFILES`].
    Template,
    TotalSupply,
    MaxOffers,
    /// A `feeSetting` other than the witnessed zero.
    FeeSetting,
    /// An `extraFee` other than the witnessed zero.
    ExtraFee,
    /// It left its curve (a status or its liquidity added).
    OffCurve,
    /// Read before its launch, or its SELL may land later after it than its
    /// word's tax is witnessed.
    TaxUnwitnessed,
    /// Its fee terms (or the deployment's) differ from [`WITNESSED_VENUE_FEE`].
    FeeTerms,
    /// More offers than `maxOffers`.
    Curve,
}

impl core::fmt::Display for UnsupportedClassicHolding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::EconomicsNotWitnessed => {
                "classic holding: the deployment's economics are not witnessed"
            }
            Self::Quote => "classic holding: not quoted in native BNB",
            Self::Template => "classic holding: template word not priced",
            Self::TotalSupply => "classic holding: totalSupply",
            Self::MaxOffers => "classic holding: maxOffers",
            Self::FeeSetting => "classic holding: feeSetting not witnessed",
            Self::ExtraFee => "classic holding: extraFee not witnessed",
            Self::OffCurve => "classic holding: the token left its curve",
            Self::TaxUnwitnessed => "classic holding: tax not witnessed at this age",
            Self::FeeTerms => "classic holding: fee terms not witnessed",
            Self::Curve => "classic holding: offers above maxOffers",
        })
    }
}

impl std::error::Error for UnsupportedClassicHolding {}

/// A held token whose state at one block is priced: its curve and terms,
/// bound to the deployment's code generation and the block read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassifiedClassicHolding {
    codehash_generation: u64,
    token: Address,
    block_hash: B256,
    curve: ClassicCurve,
    fee: ClassicFee,
}

impl ClassifiedClassicHolding {
    #[inline]
    pub fn codehash_generation(&self) -> u64 {
        self.codehash_generation
    }

    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// The block the state was read at.
    #[inline]
    pub fn block_hash(&self) -> B256 {
        self.block_hash
    }

    #[inline]
    pub fn curve(&self) -> ClassicCurve {
        self.curve
    }

    #[inline]
    pub fn fee(&self) -> ClassicFee {
        self.fee
    }
}

/// Classifies a held token's state `read` on the validated `deployment`
/// for a SELL that lands at most `valid_blocks` after the read: priced only
/// as the launches are (native, a priced word, the witnessed supply,
/// offers, `feeSetting` and fee terms, its tax witnessed up to the last
/// block it may land in) and while it trades on its curve.
pub fn classify_classic_holding(
    validated: &ValidatedFourDeployment,
    read: &ClassicTokenRead,
    valid_blocks: u64,
) -> Result<ClassifiedClassicHolding, UnsupportedClassicHolding> {
    if !validated.classic_economics_witnessed() {
        return Err(UnsupportedClassicHolding::EconomicsNotWitnessed);
    }
    if read.quote != Address::ZERO {
        return Err(UnsupportedClassicHolding::Quote);
    }
    let profile = CLASSIC_PROFILES
        .iter()
        .find(|p| p.template == read.template)
        .ok_or(UnsupportedClassicHolding::Template)?;
    if read.total_supply != WITNESSED_TOTAL_SUPPLY {
        return Err(UnsupportedClassicHolding::TotalSupply);
    }
    if read.max_offers != WITNESSED_MAX_OFFERS {
        return Err(UnsupportedClassicHolding::MaxOffers);
    }
    if !read.fee_setting.is_zero() {
        return Err(UnsupportedClassicHolding::FeeSetting);
    }
    if !read.extra_fee.is_zero() {
        return Err(UnsupportedClassicHolding::ExtraFee);
    }
    if !read.status.is_zero() || read.liquidity_added {
        return Err(UnsupportedClassicHolding::OffCurve);
    }
    let age = read
        .block_number
        .checked_sub(read.launch_block)
        .and_then(|a| a.checked_add(valid_blocks));
    if age.is_none_or(|a| a > profile.tax_witnessed_blocks) {
        return Err(UnsupportedClassicHolding::TaxUnwitnessed);
    }
    if read.venue_fee != WITNESSED_VENUE_FEE || validated.venue_fee() != WITNESSED_VENUE_FEE {
        return Err(UnsupportedClassicHolding::FeeTerms);
    }
    let sold = read
        .max_offers
        .checked_sub(read.offers)
        .ok_or(UnsupportedClassicHolding::Curve)?;
    Ok(ClassifiedClassicHolding {
        codehash_generation: validated.codehash_generation(),
        token: read.token,
        block_hash: read.block_hash,
        curve: ClassicCurve {
            k: read.k,
            t: read.t,
            offers: read.offers,
            sold,
        },
        fee: ClassicFee {
            rate_bps: read.venue_fee.rate_bps,
            min_fee: read.venue_fee.min_fee,
            tax_bps: profile.tax_bps,
        },
    })
}

/// R12d: the OpenFour launch terms the bonding preset was executed with
/// (`fixtures/fourmeme/openfour_parity_20261104.txt`): WBNB quote paid
/// natively, 1e27 supply, 8e26 for sale, an 18 BNB raise, the creator values
/// of its launches (its fee terms are the validated pin's, probed at the
/// fee router).
pub const OPENFOUR_WITNESSED_QUOTE: Address =
    alloy_primitives::address!("bb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c");
pub const OPENFOUR_WITNESSED_RAISE: U256 = U256::from_limbs([0xf9cc_d8a1_c508_0000, 0, 0, 0]);
pub const OPENFOUR_CREATOR_VALUES: &[U256] =
    &[U256::from_limbs([300_000_000_000_000_000, 0, 0, 0])];
/// The `createArgs` head word that carries the creator's BUY value (equal to
/// the call's value in the witness); the others must be as witnessed: the
/// first free (a deadline), the rest zero.
const OPENFOUR_CREATOR_WORD: usize = 2;

/// Why an OpenFour launch is not priced (the first deviating field).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedOpenFourLaunch {
    /// The deployment pins no bonding preset (or not this one).
    EconomicsNotWitnessed,
    Preset,
    /// No frozen token implementation names its token.
    NoToken,
    Quote,
    /// Supply, sale or raise other than the witnessed.
    Terms,
    /// A module or custom data parameter is set (the witness had none).
    ModuleParams,
    /// An unnamed `createArgs` word the witness did not set.
    UnknownArgument,
    /// The creator word is not the call's value.
    CreatorValue,
    UnwitnessedCreatorValue,
    Curve(OpenFourQuoteError),
}

impl core::fmt::Display for UnsupportedOpenFourLaunch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EconomicsNotWitnessed => {
                f.write_str("openfour launch: no executed bonding preset is pinned")
            }
            Self::Preset => f.write_str("openfour launch: preset not executed"),
            Self::NoToken => f.write_str("openfour launch: no frozen token derivation"),
            Self::Quote => f.write_str("openfour launch: quote asset not witnessed"),
            Self::Terms => f.write_str("openfour launch: supply, sale or raise not witnessed"),
            Self::ModuleParams => f.write_str("openfour launch: module parameters set"),
            Self::UnknownArgument => f.write_str("openfour launch: an unwitnessed argument is set"),
            Self::CreatorValue => {
                f.write_str("openfour launch: creator value is not the call's value")
            }
            Self::UnwitnessedCreatorValue => {
                f.write_str("openfour launch: creator value not witnessed")
            }
            Self::Curve(e) => write!(f, "openfour launch: {e}"),
        }
    }
}

impl std::error::Error for UnsupportedOpenFourLaunch {}

/// An OpenFour launch priced on the bonding preset: its token (derived
/// before execution), the curve after its creator BUY and the fee terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassifiedOpenFourLaunch {
    candidate: B256,
    codehash_generation: u64,
    token: Address,
    curve: OpenFourCurve,
    fee: OpenFourFee,
}

impl ClassifiedOpenFourLaunch {
    #[inline]
    pub fn candidate(&self) -> B256 {
        self.candidate
    }

    #[inline]
    pub fn codehash_generation(&self) -> u64 {
        self.codehash_generation
    }

    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    /// The curve after the launch and its creator BUY.
    #[inline]
    pub fn curve(&self) -> OpenFourCurve {
        self.curve
    }

    #[inline]
    pub fn fee(&self) -> OpenFourFee {
        self.fee
    }
}

fn openfour_terms(max_supply: U256, sale: U256, raise: U256) -> Option<LaunchTerms> {
    (max_supply == WITNESSED_TOTAL_SUPPLY
        && sale == WITNESSED_MAX_OFFERS
        && raise == OPENFOUR_WITNESSED_RAISE)
        .then_some(LaunchTerms {
            total_supply: max_supply,
            max_offers: sale,
            max_raising: raise,
        })
}

/// Classifies an OpenFour `launch` sent by `creator` with `value` in
/// transaction `candidate` on the validated `deployment`: priced only on the
/// executed bonding preset with the witnessed terms, no module parameter,
/// and a witnessed creator value.
pub fn classify_openfour_launch(
    validated: &ValidatedFourDeployment,
    launch: &OpenFourLaunch<'_>,
    creator: Address,
    value: U256,
    candidate: B256,
) -> Result<ClassifiedOpenFourLaunch, UnsupportedOpenFourLaunch> {
    let bonding = validated
        .openfour_bonding()
        .ok_or(UnsupportedOpenFourLaunch::EconomicsNotWitnessed)?;
    if launch.preset_id != bonding.preset_id {
        return Err(UnsupportedOpenFourLaunch::Preset);
    }
    let init = &launch.init;
    let token = validated
        .deployment()
        .openfour_token(
            launch.preset_id,
            init.token_salt,
            launch.request_id,
            Some(creator),
        )
        .ok_or(UnsupportedOpenFourLaunch::NoToken)?;
    if init.quote_asset != OPENFOUR_WITNESSED_QUOTE {
        return Err(UnsupportedOpenFourLaunch::Quote);
    }
    let terms = openfour_terms(init.max_supply, init.sale_amount, init.raise_amount)
        .ok_or(UnsupportedOpenFourLaunch::Terms)?;
    if [
        init.token_params,
        init.vault_params,
        init.curve_params,
        init.trade_params,
        init.migrate_params,
        init.custom_data_params,
    ]
    .iter()
    .any(|p| !p.is_empty())
    {
        return Err(UnsupportedOpenFourLaunch::ModuleParams);
    }
    if launch
        .unnamed
        .iter()
        .enumerate()
        .any(|(i, w)| i != 0 && i != OPENFOUR_CREATOR_WORD && *w != B256::ZERO)
    {
        return Err(UnsupportedOpenFourLaunch::UnknownArgument);
    }
    if U256::from_be_bytes(launch.unnamed[OPENFOUR_CREATOR_WORD].0) != value {
        return Err(UnsupportedOpenFourLaunch::CreatorValue);
    }
    if !OPENFOUR_CREATOR_VALUES.contains(&value) {
        return Err(UnsupportedOpenFourLaunch::UnwitnessedCreatorValue);
    }
    // The fee terms the drift probe validated for the preset.
    let fee = bonding.fee;
    let start = openfour_launch_curve(&terms).map_err(UnsupportedOpenFourLaunch::Curve)?;
    let bought = creator_buy(&start, &fee, value).map_err(UnsupportedOpenFourLaunch::Curve)?;
    Ok(ClassifiedOpenFourLaunch {
        candidate,
        codehash_generation: validated.codehash_generation(),
        token,
        curve: bought.after,
        fee,
    })
}

/// R12d: a held OpenFour token as read at one canonical block:
/// `Core.tokens(token)`, its vault's `phase()`, each module proxy's beacon
/// and `Tools.getCurveLiquiditySnapshot(token)`, all at `block_hash`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourTokenRead {
    pub token: Address,
    pub block_number: u64,
    pub block_hash: B256,
    pub preset_id: U256,
    pub max_supply: U256,
    pub sale_amount: U256,
    pub raise_amount: U256,
    pub quote_asset: Address,
    pub exists: bool,
    pub paused: bool,
    pub anti_sniper_enabled: bool,
    pub custom_data: Address,
    /// The vault's phase (`Trading` is 1).
    pub phase: u8,
    /// The beacons of its token module, vault, curve, trade and migrate
    /// module proxies.
    pub module_beacons: [Address; OPENFOUR_MODULES.len()],
    pub initial_quote: U256,
    pub initial_token: U256,
    pub current_quote: U256,
    pub current_token: U256,
    pub remaining: U256,
}

/// The vault phase of an internal market that trades.
pub const OPENFOUR_PHASE_TRADING: u8 = 1;

/// Why a held OpenFour token's state is not priced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedOpenFourHolding {
    EconomicsNotWitnessed,
    Preset,
    Quote,
    Terms,
    /// Not existing, paused, with anti-sniper fees or a custom data module.
    Mode,
    /// Its vault does not trade (created, migrating, sold out, migrated).
    OffCurve,
    /// A module proxy follows another beacon than the pinned preset's.
    Modules,
    /// The snapshot's initial liquidity is not the terms' curve.
    Curve,
}

impl core::fmt::Display for UnsupportedOpenFourHolding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::EconomicsNotWitnessed => "openfour holding: no executed bonding preset is pinned",
            Self::Preset => "openfour holding: preset not executed",
            Self::Quote => "openfour holding: quote asset not witnessed",
            Self::Terms => "openfour holding: supply, sale or raise not witnessed",
            Self::Mode => "openfour holding: paused, anti-sniper, custom data or unknown token",
            Self::OffCurve => "openfour holding: the vault does not trade",
            Self::Modules => "openfour holding: a module follows another beacon",
            Self::Curve => "openfour holding: initial liquidity is not the terms' curve",
        })
    }
}

impl std::error::Error for UnsupportedOpenFourHolding {}

/// A held OpenFour token whose state at one block is priced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassifiedOpenFourHolding {
    codehash_generation: u64,
    token: Address,
    block_hash: B256,
    curve: OpenFourCurve,
    fee: OpenFourFee,
}

impl ClassifiedOpenFourHolding {
    #[inline]
    pub fn codehash_generation(&self) -> u64 {
        self.codehash_generation
    }

    #[inline]
    pub fn token(&self) -> Address {
        self.token
    }

    #[inline]
    pub fn block_hash(&self) -> B256 {
        self.block_hash
    }

    #[inline]
    pub fn curve(&self) -> OpenFourCurve {
        self.curve
    }

    #[inline]
    pub fn fee(&self) -> OpenFourFee {
        self.fee
    }
}

/// Classifies a held OpenFour token's state `read` on the validated
/// `deployment`: priced only on the executed bonding preset's terms and
/// modules, while its vault trades.
pub fn classify_openfour_holding(
    validated: &ValidatedFourDeployment,
    read: &OpenFourTokenRead,
) -> Result<ClassifiedOpenFourHolding, UnsupportedOpenFourHolding> {
    let bonding = validated
        .openfour_bonding()
        .ok_or(UnsupportedOpenFourHolding::EconomicsNotWitnessed)?;
    if read.preset_id != bonding.preset_id {
        return Err(UnsupportedOpenFourHolding::Preset);
    }
    if read.quote_asset != OPENFOUR_WITNESSED_QUOTE {
        return Err(UnsupportedOpenFourHolding::Quote);
    }
    let terms = openfour_terms(read.max_supply, read.sale_amount, read.raise_amount)
        .ok_or(UnsupportedOpenFourHolding::Terms)?;
    if !read.exists || read.paused || read.anti_sniper_enabled || read.custom_data != Address::ZERO
    {
        return Err(UnsupportedOpenFourHolding::Mode);
    }
    if read.phase != OPENFOUR_PHASE_TRADING {
        return Err(UnsupportedOpenFourHolding::OffCurve);
    }
    if bonding
        .modules
        .iter()
        .zip(read.module_beacons)
        .any(|(m, beacon)| m.beacon != beacon)
    {
        return Err(UnsupportedOpenFourHolding::Modules);
    }
    let start = openfour_launch_curve(&terms).map_err(|_| UnsupportedOpenFourHolding::Curve)?;
    if (start.quote_reserve, start.token_reserve) != (read.initial_quote, read.initial_token) {
        return Err(UnsupportedOpenFourHolding::Curve);
    }
    Ok(ClassifiedOpenFourHolding {
        codehash_generation: validated.codehash_generation(),
        token: read.token,
        block_hash: read.block_hash,
        curve: OpenFourCurve {
            k: start.k,
            token_reserve: read.current_token,
            quote_reserve: read.current_quote,
            remaining: read.remaining,
        },
        fee: bonding.fee,
    })
}
