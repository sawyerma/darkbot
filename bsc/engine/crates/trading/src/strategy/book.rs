//! The exact curve state of one Flap token from its launch (Phase H), and
//! (SC4) of one Four.meme token: its classified launch's curve, then the
//! state each of its trades reports (Classic `offers`/`funds`, OpenFour
//! `remainingForSale`/`totalRaised`): the sold tokens name the curve
//! exactly, the raised quote must be the curve's (else not `Tracking`).
//!
//! The launch receipt sets the token's terms (version, quote token, curve,
//! DEX threshold, taxes); every later Portal trade is replayed through the
//! production quote (`economics::flap::native_curve20`) from the empty
//! curve. A trade the quote does not reproduce to the unit (tokens, native
//! amount and protocol fee), a term outside the priced scope, a migration
//! or a gap makes the token not `Tracking`: it is never quoted again (no
//! entry, no exit quote), never guessed.

use alloy_primitives::U256;

use crate::economics::flap::curve::Curve;
use crate::economics::flap::native_curve20::{
    HoldingCap, NativeBuyInputs, NativeSellInputs, PricedToken, TokenTerms, Unpriced,
    WITNESSED_MAINNET_TERMS, curve20, quote_native_buy, quote_native_sell,
};
use crate::economics::four_classic::{
    ClassicCurve, ClassicFee, quote_buy as classic_buy, quote_sell as classic_sell, virtual_quote,
};
use crate::economics::openfour::{
    OpenFourCurve, OpenFourFee, max_buy_amount, quote_sell as openfour_sell,
};
use crate::venues::flap::abi::TOKEN_VERSION_TAXED_V3;

/// One term the Portal set for a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Term {
    Version(u8),
    QuoteToken([u8; 20]),
    Curve {
        r: U256,
        h: U256,
        k: U256,
    },
    DexThreshold(U256),
    Migrator(u8),
    Tax {
        buy: U256,
        sell: U256,
    },
    /// A BUY needs this NFT (`FlapNFTGated`).
    NftGate([u8; 20]),
    /// A per-origin holding cap (`FlapTokenMaxBuyPerOriginSet`).
    HoldingCap {
        bps: u16,
        max_amount: U256,
    },
}

/// One Portal curve trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trade {
    pub block_number: u64,
    pub tx_index: u64,
    pub log_index: u64,
    pub sell: bool,
    pub trader: [u8; 20],
    /// Tokens bought or sold.
    pub amount: U256,
    /// The native amount spent (BUY) or received (SELL).
    pub eth: U256,
    pub fee: U256,
}

/// Why a token is not (or no longer) quoted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Untracked {
    /// A term outside the priced scope (version, quote token, curve, tax).
    Unsupported(&'static str),
    /// A trade before every needed term was set.
    TermsIncomplete,
    /// A trade the quote does not reproduce to the unit.
    TradeMismatch,
    /// It left the curve (`LaunchedToDEX`).
    Migrated,
    /// Its Flap events of a block are incomplete (a refused log, a gap, a
    /// reorg).
    Gap,
    /// A real trade could not be replayed on a PAPER position's shadow
    /// curve (the shadow has fewer of the sold tokens, or the BUY is
    /// refused there).
    ShadowReplay,
    /// SC4: the venue's exact quote refuses the entry or the exit on this
    /// curve (sold out, below one unit).
    Quote,
}

/// SC4: a Four.meme token's curve: the one its classified launch starts on
/// (nothing sold), the state now, and its fee terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourCurve {
    Classic {
        start: ClassicCurve,
        now: ClassicCurve,
        fee: ClassicFee,
    },
    OpenFour {
        start: OpenFourCurve,
        now: OpenFourCurve,
        fee: OpenFourFee,
    },
}

/// SC4: the state a Four.meme trade leaves, as the venue reports it: the
/// tokens still for sale and the quote raised since the launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourPost {
    pub remaining: U256,
    pub raised: U256,
}

impl FourCurve {
    /// A Classic launch's curve (nothing sold) with its fee terms.
    pub fn classic(start: ClassicCurve, fee: ClassicFee) -> Option<Self> {
        start.sold.is_zero().then_some(Self::Classic {
            start,
            now: start,
            fee,
        })
    }

    /// An OpenFour launch's curve (nothing sold) with its fee terms.
    pub fn openfour(start: OpenFourCurve, fee: OpenFourFee) -> Self {
        Self::OpenFour {
            start,
            now: start,
            fee,
        }
    }

    /// A classified Classic launch's curve directly after it (its creator's
    /// BUY in it): the launch curve is that one with its sold tokens back.
    pub fn classic_at(now: ClassicCurve, fee: ClassicFee) -> Option<Self> {
        let start = ClassicCurve {
            k: now.k,
            t: now.t.checked_add(now.sold)?,
            offers: now.offers.checked_add(now.sold)?,
            sold: U256::ZERO,
        };
        Some(Self::Classic { start, now, fee })
    }

    /// A classified OpenFour launch's curve directly after it, of a sale of
    /// `sale` tokens: the launch curve is that one with its sold tokens
    /// back (its quote reserve `K / T` exactly, `K` its product).
    pub fn openfour_at(now: OpenFourCurve, sale: U256, fee: OpenFourFee) -> Option<Self> {
        let sold = sale.checked_sub(now.remaining)?;
        let token_reserve = now.token_reserve.checked_add(sold)?;
        let quote_reserve = now.k.checked_div(token_reserve)?;
        if quote_reserve.checked_mul(token_reserve)? != now.k {
            return None;
        }
        let start = OpenFourCurve {
            k: now.k,
            token_reserve,
            quote_reserve,
            remaining: sale,
        };
        Some(Self::OpenFour { start, now, fee })
    }

    /// The tokens the launch put up for sale.
    pub fn sale(&self) -> U256 {
        match self {
            Self::Classic { start, .. } => start.offers,
            Self::OpenFour { start, .. } => start.remaining,
        }
    }

    /// The tokens sold since the launch.
    pub fn sold(&self) -> U256 {
        match self {
            Self::Classic { now, .. } => now.sold,
            Self::OpenFour { start, now, .. } => start.remaining.saturating_sub(now.remaining),
        }
    }

    /// The curve once `sold` tokens of the sale are sold.
    fn with_sold(&self, sold: U256) -> Option<Self> {
        match *self {
            Self::Classic { start, fee, .. } => Some(Self::Classic {
                start,
                fee,
                now: ClassicCurve {
                    k: start.k,
                    t: start.t.checked_sub(sold)?,
                    offers: start.offers.checked_sub(sold)?,
                    sold,
                },
            }),
            Self::OpenFour { start, fee, .. } => {
                let token_reserve = start.token_reserve.checked_sub(sold)?;
                Some(Self::OpenFour {
                    start,
                    fee,
                    now: OpenFourCurve {
                        k: start.k,
                        token_reserve,
                        quote_reserve: start.k.checked_div(token_reserve)?,
                        remaining: start.remaining.checked_sub(sold)?,
                    },
                })
            }
        }
    }

    /// The quote raised since the launch at this state.
    fn raised(&self) -> Option<U256> {
        match self {
            Self::Classic { start, now, .. } => virtual_quote(now)
                .ok()?
                .checked_sub(virtual_quote(start).ok()?),
            Self::OpenFour { start, now, .. } => now.quote_reserve.checked_sub(start.quote_reserve),
        }
    }

    /// The state a trade left: the tokens still for sale name it; the
    /// raised quote it reports must be this curve's.
    pub fn after(&self, post: FourPost) -> Option<Self> {
        let next = self.with_sold(self.sale().checked_sub(post.remaining)?)?;
        (next.raised()? == post.raised).then_some(next)
    }

    /// The curve with `tokens` more sold: the real one with a position's
    /// tokens in it (both curves depend on the sold tokens alone).
    pub fn shifted(&self, tokens: U256) -> Option<Self> {
        self.with_sold(self.sold().checked_add(tokens)?)
    }

    /// The BUY of `input` wei: the tokens it yields and the wei it takes
    /// (Classic `buyTokenAMAP`, the rest refunded; OpenFour the largest
    /// exact-amount `Core.buy` within the input).
    pub fn buy(&self, input: U256) -> Option<(U256, U256)> {
        match self {
            Self::Classic { now, fee, .. } => classic_buy(now, fee, input)
                .ok()
                .map(|q| (q.amount, q.spent)),
            Self::OpenFour { now, fee, .. } => max_buy_amount(now, fee, input)
                .ok()
                .map(|q| (q.amount, q.pay)),
        }
    }

    /// The wei the SELL of `tokens` returns.
    pub fn sell(&self, tokens: U256) -> Option<U256> {
        match self {
            Self::Classic { now, fee, .. } => classic_sell(now, fee, tokens).ok().map(|q| q.funds),
            Self::OpenFour { now, fee, .. } => {
                openfour_sell(now, fee, tokens).ok().map(|q| q.received)
            }
        }
    }

    /// The marginal price as a fraction, virtual quote over virtual tokens
    /// (`start`: the launch curve's).
    fn price_at(&self, start: bool) -> Option<(U256, U256)> {
        let (q, t) = match self {
            Self::Classic { start: s, now, .. } => {
                let c = if start { s } else { now };
                (virtual_quote(c).ok()?, c.t)
            }
            Self::OpenFour { start: s, now, .. } => {
                let c = if start { s } else { now };
                (c.quote_reserve, c.token_reserve)
            }
        };
        (!t.is_zero()).then_some((q, t))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Terms {
    pub version: Option<u8>,
    pub quote_token: Option<[u8; 20]>,
    pub curve: Option<Curve>,
    pub dex_threshold: Option<U256>,
    pub buy_tax_bps: Option<u16>,
    pub sell_tax_bps: Option<u16>,
}

/// The exact state of one token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenBook {
    pub terms: Terms,
    pub reserve: U256,
    pub circulating: U256,
    pub trades: u64,
    pub status: Result<(), Untracked>,
    /// SC4: a Four.meme token's curve (then the Flap terms, reserve and
    /// circulating supply are unused).
    pub four: Option<FourCurve>,
}

impl Default for TokenBook {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenBook {
    /// The empty curve of a token at its launch.
    pub fn new() -> Self {
        Self {
            terms: Terms::default(),
            reserve: U256::ZERO,
            circulating: U256::ZERO,
            trades: 0,
            status: Ok(()),
            four: None,
        }
    }

    /// SC4: a Four.meme token at its launch, on its classified curve; an
    /// unclassified one (`Err`: why) is never tracked.
    pub fn four(curve: Result<FourCurve, &'static str>) -> Self {
        let mut book = Self::new();
        match curve {
            Ok(c) => book.four = Some(c),
            Err(why) => book.untrack(Untracked::Unsupported(why)),
        }
        book
    }

    /// SC4: one Four.meme trade's reported state; the curve moves only when
    /// its raised quote is the curve's.
    pub fn four_trade(&mut self, post: FourPost) {
        if self.status.is_err() {
            return;
        }
        match self.four.and_then(|c| c.after(post)) {
            Some(c) => {
                self.four = Some(c);
                self.trades += 1;
            }
            None => self.untrack(Untracked::TradeMismatch),
        }
    }

    /// SC4: this Four.meme curve with a position's `tokens` in it (a PAPER
    /// position's shadow); not tracked when they do not fit.
    pub fn four_shifted(&self, tokens: U256) -> Self {
        let mut shadow = self.clone();
        shadow.four = self.four.and_then(|c| c.shifted(tokens));
        if shadow.four.is_none() {
            shadow.untrack(Untracked::ShadowReplay);
        }
        shadow
    }

    /// The tokens a token was launched with (the features' supply).
    pub fn total_supply(&self) -> U256 {
        match self.four {
            Some(_) => crate::venues::fourmeme::profile::WITNESSED_TOTAL_SUPPLY,
            None => U256::from(crate::economics::flap::curve::TOTAL_SUPPLY),
        }
    }

    /// The curve's progress as a fraction: Flap its circulating supply over
    /// its DEX threshold, Four the tokens sold over those for sale.
    pub fn progress(&self) -> Option<(U256, U256)> {
        match self.four {
            Some(c) => Some((c.sold(), c.sale())),
            None => Some((self.circulating, self.terms.dex_threshold?)),
        }
    }

    pub fn tracking(&self) -> bool {
        self.status.is_ok()
    }

    fn untrack(&mut self, why: Untracked) {
        if self.status.is_ok() {
            self.status = Err(why);
        }
    }

    pub fn term(&mut self, term: Term) {
        let t = &mut self.terms;
        let bps = |v: U256| u16::try_from(v).ok().filter(|b| *b < 10_000);
        match term {
            Term::Version(v) => t.version = Some(v),
            Term::QuoteToken(a) => t.quote_token = Some(a),
            Term::Curve { r, h, k } => t.curve = Some(Curve { r, h, k }),
            Term::DexThreshold(v) => t.dex_threshold = Some(v),
            // The migrator decides the DEX after graduation, not the curve.
            Term::Migrator(_) => {}
            Term::Tax { buy, sell } => match (bps(buy), bps(sell)) {
                (Some(b), Some(s)) => {
                    t.buy_tax_bps = Some(b);
                    t.sell_tax_bps = Some(s);
                }
                _ => self.untrack(Untracked::Unsupported("tax")),
            },
            // Who may buy is restricted: outside the priced scope (a PAPER
            // BUY models neither the NFT nor the origin's quota), for good.
            Term::NftGate(nft) if nft != [0; 20] => {
                self.untrack(Untracked::Unsupported("nft gate"))
            }
            Term::HoldingCap { bps, max_amount } if bps != 0 || !max_amount.is_zero() => {
                self.untrack(Untracked::Unsupported("holding cap"))
            }
            Term::NftGate(_) | Term::HoldingCap { .. } => {}
        }
        if let Err(why) = self.scope()
            && why != Untracked::TermsIncomplete
        {
            self.untrack(why);
        }
    }

    /// Whether the terms set so far are inside the priced scope
    /// (`economics::snapshot`: Flap version 6, native quote, curve 20).
    fn scope(&self) -> Result<(), Untracked> {
        let t = &self.terms;
        if t.version.is_some_and(|v| v != TOKEN_VERSION_TAXED_V3) {
            return Err(Untracked::Unsupported("version"));
        }
        if t.quote_token.is_some_and(|a| a != [0; 20]) {
            return Err(Untracked::Unsupported("quote token"));
        }
        if t.curve.is_some_and(|c| c != curve20()) {
            return Err(Untracked::Unsupported("curve"));
        }
        match (
            t.version,
            t.quote_token,
            t.curve,
            t.dex_threshold,
            t.buy_tax_bps,
        ) {
            (Some(_), Some(_), Some(_), Some(_), Some(_)) => Ok(()),
            _ => Err(Untracked::TermsIncomplete),
        }
    }

    /// The validated quote terms (`PricedToken`): the launch's terms, no
    /// NFT gate (a `FlapNFTGated` or holding-cap log of the token, in the
    /// launch receipt or later, ends the tracking before any quote), on the
    /// curve while tracked.
    /// The validated terms while tracked (Phase I: what a PAPER intent is
    /// admitted with).
    pub fn priced(&self) -> Result<PricedToken, Untracked> {
        self.ready()
    }

    fn ready(&self) -> Result<PricedToken, Untracked> {
        self.status?;
        if self.four.is_some() {
            return Err(Untracked::Unsupported("four.meme curve"));
        }
        self.scope()?;
        let t = &self.terms;
        let (Some(version), Some(quote_token), Some(curve), Some(threshold), Some(buy), Some(sell)) = (
            t.version,
            t.quote_token,
            t.curve,
            t.dex_threshold,
            t.buy_tax_bps,
            t.sell_tax_bps,
        ) else {
            return Err(Untracked::TermsIncomplete);
        };
        PricedToken::new(&TokenTerms {
            tradable: true,
            version,
            quote_token,
            nft_gate: [0; 20],
            curve,
            dex_supply_threshold: threshold,
            buy_tax_bps: buy,
            sell_tax_bps: sell,
        })
        .map_err(|u| {
            Untracked::Unsupported(match u {
                Unpriced::NotTradable => "not tradable",
                Unpriced::Version(_) => "version",
                Unpriced::QuoteToken => "quote token",
                Unpriced::NftGate => "nft gate",
                Unpriced::Curve => "curve",
                Unpriced::Tax => "tax",
                Unpriced::Threshold => "threshold",
            })
        })
    }

    /// The BUY of `input` wei at this state (no holding cap: the profile's
    /// tokens set none, and a cut fill would show as a trade mismatch).
    pub fn buy_inputs(&self, input: U256) -> Result<NativeBuyInputs, Untracked> {
        Ok(NativeBuyInputs {
            token: self.ready()?,
            reserve_before: self.reserve,
            circulating_before: self.circulating,
            input_amount: input,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_buy_fee_bps,
            tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
            caller_fee_exempt: false,
            holding_cap: HoldingCap::None,
        })
    }

    /// The SELL of `tokens` at this state.
    pub fn sell_inputs(&self, tokens: U256) -> Result<NativeSellInputs, Untracked> {
        Ok(NativeSellInputs {
            token: self.ready()?,
            reserve_before: self.reserve,
            circulating_before: self.circulating,
            input_amount: tokens,
            protocol_fee_bps: WITNESSED_MAINNET_TERMS.protocol_sell_fee_bps,
            tax_on_bonding_curve_enabled: WITNESSED_MAINNET_TERMS.tax_on_bonding_curve_enabled,
            caller_fee_exempt: false,
        })
    }

    /// Replays one trade; the state moves only when the quote reproduces
    /// it to the unit.
    pub fn apply(&mut self, trade: &Trade) {
        if self.status.is_err() {
            return;
        }
        let next = if trade.sell {
            self.sell_inputs(trade.amount).map(|i| {
                quote_native_sell(&i)
                    .ok()
                    .filter(|q| q.output_amount == trade.eth && q.protocol_fee == trade.fee)
                    .map(|q| (q.reserve_after, q.supply_after))
            })
        } else {
            self.buy_inputs(trade.eth).map(|i| {
                quote_native_buy(&i)
                    .ok()
                    .filter(|q| q.output_amount == trade.amount && q.protocol_fee == trade.fee)
                    .map(|q| (q.reserve_after, q.supply_after))
            })
        };
        match next {
            Ok(Some((reserve, circulating))) => {
                self.reserve = reserve;
                self.circulating = circulating;
                self.trades += 1;
            }
            Ok(None) => self.untrack(Untracked::TradeMismatch),
            Err(why) => self.untrack(why),
        }
    }

    /// A BUY of `eth` on this curve, moving it (a PAPER position's own BUY,
    /// or a real BUY replayed on a shadow curve with the same native
    /// input): the tokens it yields.
    /// A failure stops the tracking of this curve.
    pub fn push_buy(&mut self, eth: U256) -> Result<U256, Untracked> {
        let q = self
            .buy_inputs(eth)
            .and_then(|i| quote_native_buy(&i).map_err(|_| Untracked::ShadowReplay))
            .inspect_err(|why| self.untrack(*why))?;
        self.reserve = q.reserve_after;
        self.circulating = q.supply_after;
        Ok(q.output_amount)
    }

    /// A SELL of `tokens` on this curve, moving it (a real SELL replayed on
    /// a shadow curve with the same token amount): the native amount.
    pub fn push_sell(&mut self, tokens: U256) -> Result<U256, Untracked> {
        let q = self
            .sell_inputs(tokens)
            .and_then(|i| quote_native_sell(&i).map_err(|_| Untracked::ShadowReplay))
            .inspect_err(|why| self.untrack(*why))?;
        self.reserve = q.reserve_after;
        self.circulating = q.supply_after;
        Ok(q.output_amount)
    }

    pub fn migrated(&mut self) {
        self.untrack(Untracked::Migrated);
    }

    /// A token's curve as the Portal holds it at a block (`getTokenV8Safe`,
    /// `nftGate`): its terms set and validated as the launch receipt's
    /// would be, off the curve (`tradable` false) migrated, its reserve and
    /// circulating supply the read ones. For a real position the engine no
    /// longer follows (a restart, a gap, a reorg ended its tracking);
    /// tracked from the next block on.
    pub fn seeded(terms: &TokenTerms, reserve: U256, circulating: U256) -> Self {
        let mut book = Self::new();
        let Curve { r, h, k } = terms.curve;
        for term in [
            Term::Version(terms.version),
            Term::QuoteToken(terms.quote_token),
            Term::Curve { r, h, k },
            Term::DexThreshold(terms.dex_supply_threshold),
            Term::Tax {
                buy: U256::from(terms.buy_tax_bps),
                sell: U256::from(terms.sell_tax_bps),
            },
            Term::NftGate(terms.nft_gate),
        ] {
            book.term(term);
        }
        if !terms.tradable {
            book.migrated();
        }
        book.reserve = reserve;
        book.circulating = circulating;
        book
    }

    pub fn gap(&mut self) {
        self.untrack(Untracked::Gap);
    }

    /// The price of the empty curve (the launch without a purchase, t0 of
    /// its observation), as [`TokenBook::price`] at no reserve and no
    /// circulating supply.
    pub fn empty_price(&self) -> Option<(U256, U256)> {
        if let Some(c) = self.four {
            return c.price_at(true);
        }
        let curve = self.terms.curve?;
        let den = U256::from(crate::economics::flap::curve::TOTAL_SUPPLY).checked_add(curve.h)?;
        (!den.is_zero()).then_some((curve.r, den))
    }

    /// The marginal price as a fraction: virtual quote reserve over virtual
    /// tokens, `(r + reserve) / (TOTAL + h - circulating)`, both in wei.
    pub fn price(&self) -> Option<(U256, U256)> {
        if let Some(c) = self.four {
            return c.price_at(false);
        }
        let curve = self.terms.curve?;
        let total_plus_h =
            U256::from(crate::economics::flap::curve::TOTAL_SUPPLY).checked_add(curve.h)?;
        let den = total_plus_h.checked_sub(self.circulating)?;
        let num = curve.r.checked_add(self.reserve)?;
        (!den.is_zero()).then_some((num, den))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SC4: the Four.meme curve against the chain.
    mod four {
        use super::*;
        use crate::economics::four_classic::LaunchTerms;
        use crate::economics::openfour::{launch_curve, quote_buy_amount};

        const CLASSIC: &str = include_str!("../../fixtures/fourmeme/classic_state_20261104.txt");
        const OPENFOUR: &str = include_str!("../../fixtures/fourmeme/openfour_parity_20261104.txt");
        const FEE: OpenFourFee = OpenFourFee {
            protocol_bps: 50,
            dev_bps: 50,
        };

        fn rows(
            fixture: &'static str,
            kind: &'static str,
        ) -> impl Iterator<Item = Vec<&'static str>> {
            fixture
                .lines()
                .map(|l| l.split(' ').collect::<Vec<_>>())
                .filter(move |f| f[0] == kind)
        }

        fn u(s: &str) -> U256 {
            U256::from_str_radix(s, 10).unwrap()
        }

        /// The witnessed OpenFour launch's curve (nothing sold).
        fn openfour_start() -> FourCurve {
            let f = rows(OPENFOUR, "OFLAUNCH").next().unwrap();
            let start = launch_curve(&LaunchTerms {
                total_supply: u(f[8]),
                max_offers: u(f[9]),
                max_raising: u(f[10]),
            })
            .unwrap();
            FourCurve::openfour(start, FEE)
        }

        /// Every witnessed Classic trade (BUYs and SELLs, first and later
        /// ones): the curve its reported offers name raised exactly its
        /// reported funds, its virtual token reserve TokenManager2's at the
        /// block; one wei more raised is a mismatch that ends the tracking.
        #[test]
        fn classic_trades_name_the_curve_they_leave() {
            let sale = U256::from(800_000_000u64) * U256::from(10u64).pow(U256::from(18u8));
            let fee = ClassicFee {
                rate_bps: 100,
                min_fee: U256::ZERO,
                tax_bps: 0,
            };
            let mut n = 0;
            for f in rows(CLASSIC, "CLSTATE") {
                let (offers, funds, k, t) = (u(f[9]), u(f[10]), u(f[11]), u(f[12]));
                assert_eq!((f[13], f[14]), (f[9], "OK"), "{f:?}");
                let sold = sale - offers;
                let start = ClassicCurve {
                    k,
                    t: t + sold,
                    offers: sale,
                    sold: U256::ZERO,
                };
                let c = FourCurve::classic(start, fee).unwrap();
                let post = FourPost {
                    remaining: offers,
                    raised: funds,
                };
                let Some(FourCurve::Classic { now, .. }) = c.after(post) else {
                    panic!("{f:?}");
                };
                assert_eq!((now.t, now.offers, now.sold), (t, offers, sold), "{f:?}");
                let wrong = FourPost {
                    raised: funds + U256::ONE,
                    ..post
                };
                assert_eq!(c.after(wrong), None, "{f:?}");
                let mut book = TokenBook::four(Ok(c));
                book.four_trade(post);
                assert!(book.tracking() && book.trades == 1);
                book.four_trade(wrong);
                assert_eq!(book.status, Err(Untracked::TradeMismatch));
                n += 1;
            }
            assert_eq!(n, 8);
        }

        /// The OpenFour snapshots: the remaining sale names the curve
        /// `getCurveLiquiditySnapshot` read (after the creator's BUY and
        /// after later trades), its raised quote the total raised.
        #[test]
        fn openfour_states_name_the_curve_they_leave() {
            let c = openfour_start();
            let mut n = 0;
            for f in rows(OPENFOUR, "OFSNAP") {
                let post = FourPost {
                    remaining: u(f[7]),
                    raised: u(f[8]),
                };
                let Some(FourCurve::OpenFour { now, .. }) = c.after(post) else {
                    panic!("{f:?}");
                };
                assert_eq!(
                    (now.k, now.token_reserve, now.quote_reserve, now.remaining),
                    (u(f[3]) * u(f[4]), u(f[6]), u(f[5]), u(f[7])),
                    "{f:?}"
                );
                let wrong = FourPost {
                    raised: post.raised + U256::ONE,
                    ..post
                };
                assert_eq!(c.after(wrong), None);
                n += 1;
            }
            assert_eq!(n, 2);
        }

        /// A position's shadow is the real curve with its tokens sold: the
        /// curve an executed exact BUY of them left, its SELL the executed
        /// one's proceeds (OFSELL); too many tokens do not fit.
        #[test]
        fn a_shadow_is_the_real_curve_with_the_positions_tokens() {
            let c = openfour_start();
            let mut n = 0;
            for f in rows(OPENFOUR, "OFSELL") {
                let snap = rows(OPENFOUR, "OFSNAP").find(|s| s[1] == f[1]).unwrap();
                let real = c
                    .after(FourPost {
                        remaining: u(snap[7]),
                        raised: u(snap[8]),
                    })
                    .unwrap();
                let FourCurve::OpenFour { now, .. } = real else {
                    panic!();
                };
                let amount = u(f[3]);
                let shadow = real.shifted(amount).unwrap();
                let FourCurve::OpenFour { now: after, .. } = shadow else {
                    panic!();
                };
                assert_eq!(after, quote_buy_amount(&now, &FEE, amount).unwrap().after);
                assert_eq!(shadow.sell(amount), Some(u(f[6])), "{f:?}");
                n += 1;
            }
            assert_eq!(n, 4);
            let book = TokenBook::four(Ok(c));
            let sale = c.sale();
            assert_eq!(
                book.four_shifted(sale + U256::ONE).status,
                Err(Untracked::ShadowReplay)
            );
            // Classic: the BUY's curve is the shifted one.
            let f = rows(CLASSIC, "CLSTATE").next().unwrap();
            let start = ClassicCurve {
                k: u(f[11]),
                t: u(f[12])
                    + (U256::from(800_000_000u64) * U256::from(10u64).pow(U256::from(18u8))
                        - u(f[9])),
                offers: U256::from(800_000_000u64) * U256::from(10u64).pow(U256::from(18u8)),
                sold: U256::ZERO,
            };
            let fee = ClassicFee {
                rate_bps: 100,
                min_fee: U256::ZERO,
                tax_bps: 1_000,
            };
            let classic = FourCurve::classic(start, fee).unwrap();
            let input = U256::from(10u64).pow(U256::from(16u8));
            let (tokens, spent) = classic.buy(input).unwrap();
            let q = classic_buy(&start, &fee, input).unwrap();
            assert_eq!((tokens, spent), (q.amount, q.spent));
            let FourCurve::Classic { now, .. } = classic.shifted(tokens).unwrap() else {
                panic!();
            };
            assert_eq!(now, q.after);
            assert!(
                FourCurve::classic(q.after, fee).is_none(),
                "a launch curve sold nothing"
            );
        }

        /// A classified launch's curve directly after it (its creator's BUY
        /// in it) names its launch curve: that one with its sold tokens back.
        #[test]
        fn a_launchs_curve_after_its_creators_buy_names_its_launch_curve() {
            let c = openfour_start();
            let snap = rows(OPENFOUR, "OFSNAP").next().unwrap();
            let after = c
                .after(FourPost {
                    remaining: u(snap[7]),
                    raised: u(snap[8]),
                })
                .unwrap();
            let FourCurve::OpenFour { now, .. } = after else {
                panic!()
            };
            assert_eq!(FourCurve::openfour_at(now, c.sale(), FEE), Some(after));
            assert_eq!(
                FourCurve::openfour_at(now, now.remaining - U256::ONE, FEE),
                None
            );
            let f = rows(CLASSIC, "CLSTATE").next().unwrap();
            let sale = U256::from(800_000_000u64) * U256::from(10u64).pow(U256::from(18u8));
            let launch = ClassicCurve {
                k: u(f[11]),
                t: u(f[12]) + (sale - u(f[9])),
                offers: sale,
                sold: U256::ZERO,
            };
            let fee = ClassicFee {
                rate_bps: 100,
                min_fee: U256::ZERO,
                tax_bps: 1_000,
            };
            let bought =
                classic_buy(&launch, &fee, U256::from(10u64).pow(U256::from(17u8))).unwrap();
            assert_eq!(
                FourCurve::classic_at(bought.after, fee),
                Some(FourCurve::Classic {
                    start: launch,
                    now: bought.after,
                    fee
                })
            );
        }

        /// The Four.meme book's features: the price is the virtual quote
        /// over the virtual tokens, the progress the sold over the sale, the
        /// supply the witnessed one; an unclassified launch is untracked
        /// with its reason and never Flap-priced.
        #[test]
        fn a_four_books_price_progress_and_scope() {
            let c = openfour_start();
            let snap = rows(OPENFOUR, "OFSNAP").next().unwrap();
            let mut book = TokenBook::four(Ok(c));
            assert_eq!(book.empty_price(), book.price());
            book.four_trade(FourPost {
                remaining: u(snap[7]),
                raised: u(snap[8]),
            });
            assert_eq!(book.price(), Some((u(snap[5]), u(snap[6]))));
            assert_eq!(book.empty_price(), Some((u(snap[3]), u(snap[4]))));
            assert_eq!(book.progress(), Some((c.sale() - u(snap[7]), c.sale())));
            assert_eq!(
                book.total_supply(),
                crate::venues::fourmeme::profile::WITNESSED_TOTAL_SUPPLY
            );
            assert_eq!(
                book.priced(),
                Err(Untracked::Unsupported("four.meme curve"))
            );
            let refused = TokenBook::four(Err("preset"));
            assert_eq!(refused.status, Err(Untracked::Unsupported("preset")));
        }
    }

    fn launched(buy_tax: u64, sell_tax: u64) -> TokenBook {
        let c = curve20();
        let mut b = TokenBook::new();
        for t in [
            Term::Version(TOKEN_VERSION_TAXED_V3),
            Term::QuoteToken([0; 20]),
            Term::Curve {
                r: c.r,
                h: c.h,
                k: c.k,
            },
            Term::DexThreshold(U256::from(8u64) * U256::from(10u64).pow(U256::from(26u64))),
            Term::Migrator(1),
            Term::Tax {
                buy: U256::from(buy_tax),
                sell: U256::from(sell_tax),
            },
        ] {
            b.term(t);
        }
        b
    }

    fn trade(sell: bool, amount: U256, eth: U256, fee: U256) -> Trade {
        Trade {
            block_number: 1,
            tx_index: 0,
            log_index: 0,
            sell,
            trader: [1; 20],
            amount,
            eth,
            fee,
        }
    }

    /// A BUY and a SELL replayed from the empty curve move the state
    /// exactly as the quote says; a trade off by one unit stops tracking.
    #[test]
    fn trades_replay_exactly_or_stop_tracking() {
        let mut b = launched(100, 100);
        let eth = U256::from(1_000_000_000_000_000u64);
        let q = quote_native_buy(&b.buy_inputs(eth).unwrap()).unwrap();
        b.apply(&trade(false, q.output_amount, eth, q.protocol_fee));
        assert_eq!(
            (b.reserve, b.circulating, b.trades),
            (q.reserve_after, q.supply_after, 1)
        );
        let half = q.output_amount / U256::from(2u8);
        let s = quote_native_sell(&b.sell_inputs(half).unwrap()).unwrap();
        b.apply(&trade(true, half, s.output_amount, s.protocol_fee));
        assert!(b.tracking() && b.trades == 2);
        assert_eq!(b.circulating, q.supply_after - half);
        let mut off = b.clone();
        let next = quote_native_buy(&off.buy_inputs(eth).unwrap()).unwrap();
        off.apply(&trade(
            false,
            next.output_amount + U256::from(1u8),
            eth,
            next.protocol_fee,
        ));
        assert_eq!(off.status, Err(Untracked::TradeMismatch));
        // Never quoted again.
        assert_eq!(off.buy_inputs(eth), Err(Untracked::TradeMismatch));
    }

    /// Terms outside the priced scope, a trade before the terms, a
    /// migration and a gap each stop tracking.
    #[test]
    fn unsupported_terms_migration_and_gaps_stop_tracking() {
        let mut v7 = TokenBook::new();
        v7.term(Term::Version(7));
        assert_eq!(v7.status, Err(Untracked::Unsupported("version")));
        let mut doge = TokenBook::new();
        doge.term(Term::QuoteToken([9; 20]));
        assert_eq!(doge.status, Err(Untracked::Unsupported("quote token")));
        let mut early = TokenBook::new();
        early.term(Term::Version(TOKEN_VERSION_TAXED_V3));
        early.apply(&trade(false, U256::from(1u8), U256::from(1u8), U256::ZERO));
        assert_eq!(early.status, Err(Untracked::TermsIncomplete));
        let mut tax = TokenBook::new();
        tax.term(Term::Tax {
            buy: U256::from(10_000u64),
            sell: U256::ZERO,
        });
        assert_eq!(tax.status, Err(Untracked::Unsupported("tax")));
        // Who may buy is restricted: an NFT gate or a per-origin cap; a
        // cleared one (zero) changes nothing.
        let mut gated = launched(0, 0);
        gated.term(Term::NftGate([0; 20]));
        gated.term(Term::HoldingCap {
            bps: 0,
            max_amount: U256::ZERO,
        });
        assert!(gated.tracking());
        gated.term(Term::NftGate([7; 20]));
        assert_eq!(gated.status, Err(Untracked::Unsupported("nft gate")));
        assert!(gated.buy_inputs(U256::from(1u8)).is_err());
        let mut capped = launched(0, 0);
        capped.term(Term::HoldingCap {
            bps: 200,
            max_amount: U256::from(20_000_000u64) * U256::from(10u64).pow(U256::from(18u8)),
        });
        assert_eq!(capped.status, Err(Untracked::Unsupported("holding cap")));
        let mut m = launched(0, 0);
        m.migrated();
        assert_eq!(m.status, Err(Untracked::Migrated));
        let mut g = launched(0, 0);
        g.gap();
        assert_eq!(g.status, Err(Untracked::Gap));
        // The first reason stays.
        g.migrated();
        assert_eq!(g.status, Err(Untracked::Gap));
    }

    /// A shadow curve moves like the quote says, and a replay it must
    /// refuse (here a BUY reaching the DEX threshold) ends its tracking.
    #[test]
    fn a_shadow_replays_or_stops_tracking() {
        let mut b = launched(100, 100);
        let eth = U256::from(1_000_000_000_000_000u64);
        let q = quote_native_buy(&b.buy_inputs(eth).unwrap()).unwrap();
        assert_eq!(b.push_buy(eth), Ok(q.output_amount));
        assert_eq!(
            (b.reserve, b.circulating),
            (q.reserve_after, q.supply_after)
        );
        let s = quote_native_sell(&b.sell_inputs(q.output_amount).unwrap()).unwrap();
        assert_eq!(b.push_sell(q.output_amount), Ok(s.output_amount));
        let mut far = b.clone();
        assert_eq!(
            far.push_buy(U256::from(1_000_000_000_000_000_000_000u128)),
            Err(Untracked::ShadowReplay)
        );
        assert!(!far.tracking());
    }

    /// The price at the empty curve is r / (TOTAL + h) and rises with a BUY.
    #[test]
    fn the_price_is_the_virtual_reserve_ratio() {
        let mut b = launched(0, 0);
        let c = curve20();
        let total_plus_h = U256::from(crate::economics::flap::curve::TOTAL_SUPPLY) + c.h;
        assert_eq!(b.price(), Some((c.r, total_plus_h)));
        let eth = U256::from(1_000_000_000_000_000_000u128);
        let q = quote_native_buy(&b.buy_inputs(eth).unwrap()).unwrap();
        b.apply(&trade(false, q.output_amount, eth, q.protocol_fee));
        let (n1, d1) = b.price().unwrap();
        assert!(n1 * total_plus_h > c.r * d1);
    }
}
