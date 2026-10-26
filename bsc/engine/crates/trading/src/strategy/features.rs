//! The observation window's features (Strategy C, the Solana backend's
//! `FeatureCalculator` / `TokenTracker`, ported to BSC Flap facts).
//!
//! From the token's launch (the end of its launch transaction) to the
//! cutoff, every Portal trade of another transaction counts; the creator's
//! purchase inside the launch transaction is `dev_initial_buy`. Prices are
//! the exact virtual-reserve ratio of the replayed curve
//! (`book::TokenBook::price`), never an event's rounded field. Each ratio is
//! rounded once, half to even, at the configured places (`fixed`).
//!
//! 16 of the original 17 features: `research_estimated_price_impact` was
//! always NULL in the original live PAPER runtime and could never pass a
//! filter, so it is not carried.

use alloy_primitives::{U256, U512};

use super::book::{TokenBook, Trade};
use super::fixed::{ratio, scale};
use crate::economics::flap::curve::TOTAL_SUPPLY;

/// The feature names, in contract order.
pub const NAMES: [&str; 16] = [
    "buy_count",
    "sell_count",
    "buy_volume_bnb",
    "sell_volume_bnb",
    "net_buy_flow_bnb",
    "unique_buyers",
    "unique_buyer_ratio",
    "largest_buy_bnb",
    "buyer_volume_concentration",
    "first5_buyer_concentration",
    "first_sell_latency_ms",
    "price_change",
    "max_drawdown",
    "dev_initial_buy",
    "curve_progress_pct",
    "provider_gap_flag",
];

/// The flag feature: a coverage gap inside the window. Not filterable; a
/// true flag makes the decision `DATA_INCOMPLETE`.
pub const GAP_FLAG: usize = 15;

/// Native amounts are exact wei: 18 places.
pub const BNB_PLACES: u32 = 18;

/// The places of feature `i` under `ratio_places`.
pub fn places(i: usize, ratio_places: u32) -> u32 {
    match NAMES[i] {
        "buy_volume_bnb" | "sell_volume_bnb" | "net_buy_flow_bnb" | "largest_buy_bnb" => BNB_PLACES,
        "unique_buyer_ratio"
        | "buyer_volume_concentration"
        | "first5_buyer_concentration"
        | "price_change"
        | "max_drawdown"
        | "dev_initial_buy"
        | "curve_progress_pct" => ratio_places,
        _ => 0,
    }
}

/// One feature value at its places, a flag, or NULL (missing basis).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    Num(i128),
    Flag(bool),
    Null,
}

/// One token's window so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub creator: [u8; 20],
    /// The launch transaction (block, index): its trades never count.
    pub launch: (u64, u64),
    pub t0_ms: u64,
    pub cutoff_ms: u64,
    buy_count: u64,
    sell_count: u64,
    buy_wei: U256,
    sell_wei: U256,
    /// Each buyer's volume, in order of the first buy.
    per_buyer: Vec<([u8; 20], U256)>,
    largest_buy: U256,
    first_sell_ms: Option<u64>,
    dev_tokens: U256,
    price_t0: Option<(U256, U256)>,
    price_t: Option<(U256, U256)>,
    peak: Option<(U256, U256)>,
    /// The deepest drawdown from the peak so far, at `ratio_places`.
    max_drawdown: i128,
    pub gap: bool,
    ratio_places: u32,
}

/// `a/b` versus `c/d` for positive fractions.
fn less(a: (U256, U256), c: (U256, U256)) -> bool {
    U512::from(a.0) * U512::from(c.1) < U512::from(c.0) * U512::from(a.1)
}

/// `a / b` of two positive fractions, rounded at `places`.
fn quotient(a: (U256, U256), b: (U256, U256), places: u32) -> Option<i128> {
    ratio(a.0.checked_mul(b.1)?, a.1.checked_mul(b.0)?, places)
}

impl Observation {
    pub fn new(
        creator: [u8; 20],
        launch: (u64, u64),
        t0_ms: u64,
        observation_ms: u64,
        ratio_places: u32,
    ) -> Self {
        Self {
            creator,
            launch,
            t0_ms,
            cutoff_ms: t0_ms.saturating_add(observation_ms),
            buy_count: 0,
            sell_count: 0,
            buy_wei: U256::ZERO,
            sell_wei: U256::ZERO,
            per_buyer: Vec::new(),
            largest_buy: U256::ZERO,
            first_sell_ms: None,
            dev_tokens: U256::ZERO,
            price_t0: None,
            price_t: None,
            peak: None,
            max_drawdown: 0,
            gap: false,
            ratio_places,
        }
    }

    /// One trade at block time `at_ms`, after `book` replayed it.
    pub fn trade(&mut self, trade: &Trade, at_ms: u64, book: &TokenBook) {
        if (trade.block_number, trade.tx_index) == self.launch {
            if !trade.sell && trade.trader == self.creator {
                self.dev_tokens = self.dev_tokens.saturating_add(trade.amount);
            }
            // The state after the whole launch transaction is t0.
            self.price_t0 = book.price();
            self.price_t = self.price_t0;
            self.peak = self.price_t0;
            return;
        }
        if at_ms > self.cutoff_ms {
            return;
        }
        if self.price_t0.is_none() {
            // A launch without a purchase: the empty curve is t0.
            self.price_t0 = book.price();
        }
        if trade.sell {
            self.sell_count += 1;
            self.sell_wei = self.sell_wei.saturating_add(trade.eth);
            self.first_sell_ms.get_or_insert(at_ms);
        } else {
            self.buy_count += 1;
            self.buy_wei = self.buy_wei.saturating_add(trade.eth);
            self.largest_buy = self.largest_buy.max(trade.eth);
            match self.per_buyer.iter_mut().find(|(b, _)| *b == trade.trader) {
                Some((_, v)) => *v = v.saturating_add(trade.eth),
                None => self.per_buyer.push((trade.trader, trade.eth)),
            }
        }
        if let Some(p) = book.price() {
            self.price_t = Some(p);
            let peak = match self.peak {
                Some(peak) if !less(peak, p) => peak,
                _ => p,
            };
            self.peak = Some(peak);
            if let (Some(q), Some(one)) = (
                quotient(p, peak, self.ratio_places),
                scale(self.ratio_places),
            ) {
                self.max_drawdown = self.max_drawdown.max(one - q);
            }
        }
    }

    /// The 16 features at the cutoff; `curve` is the token's book there.
    pub fn features(&self, book: &TokenBook, first_buyer_window: usize) -> [Value; 16] {
        let p = self.ratio_places;
        let num = |v: Option<i128>| v.map_or(Value::Null, Value::Num);
        let wei = |v: U256| num(i128::try_from(v).ok());
        let one = scale(p);
        let unique = self.per_buyer.len() as u64;
        let hhi = (!self.buy_wei.is_zero())
            .then(|| {
                let squares = self
                    .per_buyer
                    .iter()
                    .try_fold(U256::ZERO, |a, (_, v)| a.checked_add(v.checked_mul(*v)?))?;
                ratio(squares, self.buy_wei.checked_mul(self.buy_wei)?, p)
            })
            .flatten();
        let first_n: U256 = self
            .per_buyer
            .iter()
            .take(first_buyer_window)
            .fold(U256::ZERO, |a, (_, v)| a.saturating_add(*v));
        let progress = book
            .terms
            .dex_threshold
            .and_then(|t| ratio(book.circulating.checked_mul(U256::from(100u8))?, t, p));
        [
            Value::Num(i128::from(self.buy_count)),
            Value::Num(i128::from(self.sell_count)),
            wei(self.buy_wei),
            wei(self.sell_wei),
            num(i128::try_from(self.buy_wei)
                .ok()
                .zip(i128::try_from(self.sell_wei).ok())
                .map(|(b, s)| b - s)),
            Value::Num(i128::from(unique)),
            num((self.buy_count > 0)
                .then(|| ratio(U256::from(unique), U256::from(self.buy_count), p))
                .flatten()),
            if self.buy_count > 0 {
                wei(self.largest_buy)
            } else {
                Value::Null
            },
            num(hhi),
            num((!self.buy_wei.is_zero())
                .then(|| ratio(first_n, self.buy_wei, p))
                .flatten()),
            num(self
                .first_sell_ms
                .map(|t| i128::from(t.saturating_sub(self.t0_ms)))),
            num(match (self.price_t, self.price_t0, one) {
                (Some(t), Some(t0), Some(one)) => quotient(t, t0, p).map(|q| q - one),
                _ => None,
            }),
            if self.price_t0.is_some() {
                Value::Num(self.max_drawdown)
            } else {
                Value::Null
            },
            num(ratio(self.dev_tokens, U256::from(TOTAL_SUPPLY), p)),
            num(progress),
            // A coverage gap only: a token the book stopped tracking (a
            // term outside the scope, a mismatch) is NOT_TRACKED with its
            // reason, not missing data.
            Value::Flag(self.gap),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::super::book::{Term, TokenBook};
    use super::*;
    use crate::economics::flap::native_curve20::{curve20, quote_native_buy, quote_native_sell};

    fn book() -> TokenBook {
        let c = curve20();
        let mut b = TokenBook::new();
        for t in [
            Term::Version(6),
            Term::QuoteToken([0; 20]),
            Term::Curve {
                r: c.r,
                h: c.h,
                k: c.k,
            },
            Term::DexThreshold(U256::from(8u64) * U256::from(10u64).pow(U256::from(26u64))),
            Term::Tax {
                buy: U256::from(100u8),
                sell: U256::from(100u8),
            },
        ] {
            b.term(t);
        }
        b
    }

    fn buy(b: &mut TokenBook, block: u64, tx: u64, trader: u8, wei: u64) -> Trade {
        let eth = U256::from(wei);
        let q = quote_native_buy(&b.buy_inputs(eth).unwrap()).unwrap();
        let t = Trade {
            block_number: block,
            tx_index: tx,
            log_index: 0,
            sell: false,
            trader: [trader; 20],
            amount: q.output_amount,
            eth,
            fee: q.protocol_fee,
        };
        b.apply(&t);
        t
    }

    /// A launch with a creator purchase, three buys of two buyers and a
    /// sell: every count, volume and ratio, the creator's share, the sell's
    /// latency, a rise then a drawdown, and the curve progress.
    #[test]
    fn the_window_features_follow_the_trades() {
        let mut b = book();
        let mut o = Observation::new([7; 20], (10, 1), 1_000, 3_000, 4);
        let dev = buy(&mut b, 10, 1, 7, 10_000_000_000_000_000);
        o.trade(&dev, 1_000, &b);
        let a1 = buy(&mut b, 11, 0, 1, 20_000_000_000_000_000);
        o.trade(&a1, 1_750, &b);
        let a2 = buy(&mut b, 12, 0, 2, 60_000_000_000_000_000);
        o.trade(&a2, 2_500, &b);
        let a3 = buy(&mut b, 12, 1, 1, 20_000_000_000_000_000);
        o.trade(&a3, 2_500, &b);
        let half = a2.amount / U256::from(2u8);
        let s = quote_native_sell(&b.sell_inputs(half).unwrap()).unwrap();
        let sell = Trade {
            block_number: 13,
            tx_index: 0,
            log_index: 0,
            sell: true,
            trader: [2; 20],
            amount: half,
            eth: s.output_amount,
            fee: s.protocol_fee,
        };
        b.apply(&sell);
        o.trade(&sell, 3_250, &b);
        // After the cutoff: not counted.
        let late = buy(&mut b, 14, 0, 3, 1_000_000_000_000_000);
        o.trade(&late, 4_001, &b);
        let f = o.features(&b, 5);
        let n = |i: usize| match f[i] {
            Value::Num(v) => v,
            other => panic!("{} {other:?}", NAMES[i]),
        };
        assert_eq!((n(0), n(1), n(5)), (3, 1, 2));
        assert_eq!(n(2), 100_000_000_000_000_000);
        assert_eq!(
            n(4),
            100_000_000_000_000_000 - i128::try_from(s.output_amount).unwrap()
        );
        assert_eq!(n(6), 6_667);
        assert_eq!(n(7), 60_000_000_000_000_000);
        // (0.4² + 0.6²) = 0.52; the first five buyers hold everything.
        assert_eq!((n(8), n(9)), (5_200, 10_000));
        assert_eq!(n(10), 2_250);
        assert!(n(11) > 0 && n(12) > 0);
        assert!(n(13) > 0 && n(14) > 0);
        assert_eq!(f[GAP_FLAG], Value::Flag(false));
        // A gap in the window is the flag; a token no longer tracked is not.
        let mut untracked = b.clone();
        untracked.term(Term::QuoteToken([9; 20]));
        assert_eq!(o.features(&untracked, 5)[GAP_FLAG], Value::Flag(false));
        o.gap = true;
        assert_eq!(o.features(&b, 5)[GAP_FLAG], Value::Flag(true));
    }

    /// Without a buy the ratios that need one are NULL, never zero.
    #[test]
    fn missing_bases_are_null() {
        let b = book();
        let o = Observation::new([7; 20], (10, 1), 1_000, 3_000, 4);
        let f = o.features(&b, 5);
        for i in [6, 7, 8, 9, 10, 11, 12] {
            assert_eq!(f[i], Value::Null, "{}", NAMES[i]);
        }
        assert_eq!(f[0], Value::Num(0));
    }
}
