//! The exact curve state of one Flap token from its launch (Phase H).
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
    fn ready(&self) -> Result<PricedToken, Untracked> {
        self.status?;
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

    pub fn gap(&mut self) {
        self.untrack(Untracked::Gap);
    }

    /// The price of the empty curve (the launch without a purchase, t0 of
    /// its observation), as [`TokenBook::price`] at no reserve and no
    /// circulating supply.
    pub fn empty_price(&self) -> Option<(U256, U256)> {
        let curve = self.terms.curve?;
        let den = U256::from(crate::economics::flap::curve::TOTAL_SUPPLY).checked_add(curve.h)?;
        (!den.is_zero()).then_some((curve.r, den))
    }

    /// The marginal price as a fraction: virtual quote reserve over virtual
    /// tokens, `(r + reserve) / (TOTAL + h - circulating)`, both in wei.
    pub fn price(&self) -> Option<(U256, U256)> {
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
