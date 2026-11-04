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
//! token: the caller reads them before the launch exists, from an existing
//! token at its snapshot's block (`Helper3.getTokenInfo`).

use alloy_primitives::{Address, B256, U256, b256};

use super::deployment::FourDeployment;
use super::recognize::ClassicLaunch;
use crate::economics::four_classic::{
    ClassicBuyQuote, ClassicCurve, ClassicFee, ClassicQuoteError, LaunchTerms, launch_curve,
    quote_buy,
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

/// Classifies `launch`, sent with `value` in transaction `candidate`, under
/// the venue's fee terms `venue`.
pub fn classify_classic_launch(
    deployment: &FourDeployment,
    launch: &ClassicLaunch<'_>,
    value: U256,
    venue: &VenueFee,
    candidate: B256,
) -> Result<ClassifiedClassicLaunch, UnsupportedClassicLaunch> {
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
    if *venue != WITNESSED_VENUE_FEE {
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
        token: deployment.classic_tokens(launch.salt)[profile.pair],
        template: args.template,
        curve: creator_purchase.map_or(launched, |p| p.after),
        fee,
        creator_purchase,
    })
}
