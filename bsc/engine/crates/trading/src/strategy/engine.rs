//! Strategy C on canonical Flap events (Phase H): the original backend's
//! policy (`LEGACY_BACKEND_FILTER_V1`), deterministic over the follower's
//! event order, PAPER only (it decides and accounts, it sends nothing).
//!
//! - Observe: from a Flap launch for `observation_ms` of block time; the
//!   features freeze at the cutoff; the decision comes with the first
//!   block whose time is past it.
//! - Entry: a PASS takes a slot (`max_open`; a busy slot discards the token
//!   for good) unless the creator already sold. The BUY of `input_wei` fills
//!   after `fill_delay_blocks` blocks, after all of that block's trades (an
//!   adverse assumption, not a measured inclusion); a creator sale before
//!   the fill cancels it at no cost (`LEGACY_FREE_CANCEL`) or lets it fill
//!   (`TX_LANDS`).
//! - Exit: after each block of an open position: CREATOR_SELL > TARGET >
//!   STOP (net liquidation value, every exit cost included, against the
//!   entry's whole cost) > TIMEOUT (`hold_ms` from the fill); the SELL fills
//!   like the BUY.
//! - Costs: `economics::flap::position` (exact quotes, the network fee as
//!   the class's adverse counterfactual at `gas_price_wei`). The PAPER BUY is
//!   filled on the real curve, then held on its own shadow curve: a copy of
//!   the real one with the PAPER BUY in it, on which every later real trade
//!   is replayed with the same input (a BUY's native amount, a SELL's token
//!   amount; `TokenBook::push_buy`/`push_sell`). The position is valued and
//!   sold there, so the curve stays exact with it; a real trade the shadow
//!   cannot replay leaves the position unresolved.
//! - Fail closed: a token whose curve is no longer tracked (`book`) is not
//!   entered; an open position on it is `Unresolved`, never valued. A refused
//!   Flap log, a gap or a reorg ends the tracking of every token.
//! - Plan 28.10.: a real LIVE position is `Held` (`Input::Hold`, journaled
//!   like every input): the tokens the receipt names and its whole real
//!   cost, valued on the real curve (which holds the real BUY) by the same
//!   exit rules; its exit triggers once. `Input::Unhold` ends it when its
//!   SELL is included.

use std::collections::BTreeMap;

use alloy_primitives::{U256, U512};

use super::book::{Term, TokenBook, Trade, Untracked};
use super::features::{Observation, Value};
use super::filters::{Rule, Verdict, evaluate};
use super::fixed::scale;
use crate::economics::flap::QuoteError;
use crate::economics::flap::native_curve20::{PricedToken, quote_native_sell};
use crate::economics::flap::position::{Entry, Exit, Outcome, PositionError, entry, exit, outcome};
use crate::economics::network::{CostError, GasEvidence};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatorSoldBeforeFill {
    /// The entry is dropped at no cost.
    FreeCancel,
    /// The BUY lands and is valued like any other.
    TxLands,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub observation_ms: u64,
    pub ratio_places: u32,
    pub first_buyer_window: usize,
    pub rules: Vec<Rule>,
    /// Net multiples at `ratio_places`.
    pub target_net_multiple: i128,
    pub stop_net_multiple: i128,
    pub hold_ms: u64,
    pub max_open: usize,
    pub input_wei: U256,
    pub fill_delay_blocks: u64,
    pub creator_sold_before_fill: CreatorSoldBeforeFill,
    pub gas_price_wei: U256,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// A canonical block: everything before it belongs to the previous one.
    Block {
        number: u64,
        time_ms: u64,
    },
    Launch {
        token: [u8; 20],
        creator: [u8; 20],
        tx_index: u64,
    },
    Term {
        token: [u8; 20],
        term: Term,
    },
    Trade {
        token: [u8; 20],
        trade: Trade,
    },
    Migrated {
        token: [u8; 20],
    },
    /// A refused Flap log, a gap or a reorg: no curve stays tracked.
    Incomplete,
    /// Plan 28.10.: a real LIVE position on `token`, its BUY included at
    /// `filled_ms`: `tokens` received for `spent_wei` (input and gas).
    Hold {
        token: [u8; 20],
        tokens: U256,
        spent_wei: U256,
        filled_ms: u64,
    },
    /// The real position's SELL was included: it ends.
    Unhold {
        token: [u8; 20],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Pass,
    Reject(usize),
    DataIncomplete,
    NotTracked(Untracked),
    SlotLimit,
    CreatorSold,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cancel {
    CreatorSold,
    NotTracked(Untracked),
    Quote(QuoteError),
    Cost(CostError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExitReason {
    CreatorSell,
    Target,
    Stop,
    Timeout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    Decided {
        token: [u8; 20],
        block: u64,
        decision: Decision,
        features: Box<[Value; 16]>,
    },
    Entered {
        token: [u8; 20],
        block: u64,
        entry: Entry,
    },
    EntryCancelled {
        token: [u8; 20],
        block: u64,
        cancel: Cancel,
    },
    ExitTriggered {
        token: [u8; 20],
        block: u64,
        reason: ExitReason,
    },
    Exited {
        token: [u8; 20],
        block: u64,
        reason: ExitReason,
        exit: Exit,
        outcome: Outcome,
    },
    /// An open position whose curve is no longer tracked: not valued.
    Unresolved {
        token: [u8; 20],
        block: u64,
        reason: Untracked,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Stage {
    Observing(Box<Observation>),
    /// Waiting for the fill at the end of `fill_block`.
    Entering {
        fill_block: u64,
        decided_block: u64,
    },
    /// `shadow`: the real curve with the PAPER BUY in it, every later real
    /// trade replayed on it.
    Open {
        entry: Entry,
        shadow: Box<TokenBook>,
        fill_ms: u64,
        fill_block: u64,
    },
    /// `exit_block`: the SELL fills at its end, after its trades;
    /// `fill_block`: the entry's, after which real trades are replayed on
    /// the shadow (the exit block's among them).
    Exiting {
        entry: Entry,
        shadow: Box<TokenBook>,
        fill_block: u64,
        exit_block: u64,
        reason: ExitReason,
    },
    /// Plan 28.10.: a real LIVE position, valued on the real book;
    /// `triggered` once its exit was called for.
    Held {
        entry: Entry,
        filled_ms: u64,
        triggered: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    token: [u8; 20],
    creator: [u8; 20],
    book: TokenBook,
    /// The block of the creator's first sale after the launch.
    creator_sold: Option<u64>,
    stage: Stage,
}

/// The engine: deterministic over its inputs.
#[derive(Clone, Debug)]
pub struct Engine {
    cfg: Config,
    gas: GasEvidence,
    /// By launch order (block, transaction).
    tokens: BTreeMap<(u64, u64), Token>,
    block: Option<(u64, u64)>,
}

impl Engine {
    pub fn new(cfg: Config, gas: GasEvidence) -> Self {
        Self {
            cfg,
            gas,
            tokens: BTreeMap::new(),
            block: None,
        }
    }

    /// The tokens followed (observing, entering, open or exiting).
    pub fn followed(&self) -> usize {
        self.tokens.len()
    }

    /// PAPER positions entered and not yet closed (open or exiting).
    pub fn open_positions(&self) -> usize {
        self.tokens
            .values()
            .filter(|t| matches!(t.stage, Stage::Open { .. } | Stage::Exiting { .. }))
            .count()
    }

    /// The exact curve a PAPER intent of `token` is admitted on (Phase I):
    /// its validated terms, reserve and circulating supply, on the
    /// position's shadow curve once one is open. `None` when the token is
    /// not followed or no longer tracked.
    pub fn curve_of(&self, token: &[u8; 20]) -> Option<(PricedToken, U256, U256)> {
        let t = self.tokens.values().find(|t| t.token == *token)?;
        let book = match &t.stage {
            Stage::Open { shadow, .. } | Stage::Exiting { shadow, .. } => shadow.as_ref(),
            Stage::Observing(_) | Stage::Entering { .. } | Stage::Held { .. } => &t.book,
        };
        Some((book.priced().ok()?, book.reserve, book.circulating))
    }

    /// The tokens an open position holds (what its exit sells).
    pub fn position_tokens(&self, token: &[u8; 20]) -> Option<U256> {
        match &self.tokens.values().find(|t| t.token == *token)?.stage {
            Stage::Open { entry, .. }
            | Stage::Exiting { entry, .. }
            | Stage::Held { entry, .. } => Some(entry.tokens),
            Stage::Observing(_) | Stage::Entering { .. } => None,
        }
    }

    /// Real LIVE positions held (plan 28.10.).
    pub fn held_positions(&self) -> usize {
        self.tokens
            .values()
            .filter(|t| matches!(t.stage, Stage::Held { .. }))
            .count()
    }

    fn token_mut(&mut self, token: &[u8; 20]) -> Option<&mut Token> {
        self.tokens.values_mut().find(|t| t.token == *token)
    }

    fn slots_taken(&self) -> usize {
        self.tokens
            .values()
            .filter(|t| !matches!(t.stage, Stage::Observing(_)))
            .count()
    }

    pub fn step(&mut self, input: Input) -> Vec<Output> {
        let mut out = Vec::new();
        match input {
            Input::Block { number, time_ms } => {
                if let Some((prev, prev_ms)) = self.block {
                    self.end_of_block(prev, prev_ms, &mut out);
                }
                self.block = Some((number, time_ms));
                self.decide(number, time_ms, &mut out);
            }
            Input::Launch {
                token,
                creator,
                tx_index,
            } => {
                if let Some((number, time_ms)) = self.block {
                    self.tokens.entry((number, tx_index)).or_insert(Token {
                        token,
                        creator,
                        book: TokenBook::new(),
                        creator_sold: None,
                        stage: Stage::Observing(Box::new(Observation::new(
                            creator,
                            (number, tx_index),
                            time_ms,
                            self.cfg.observation_ms,
                            self.cfg.ratio_places,
                        ))),
                    });
                }
            }
            Input::Term { token, term } => {
                if let Some(t) = self.token_mut(&token) {
                    t.book.term(term);
                }
            }
            Input::Trade { token, trade } => {
                let at_ms = self.block.map_or(0, |(_, ms)| ms);
                if let Some(t) = self.token_mut(&token) {
                    t.book.apply(&trade);
                    let launch_tx = match &t.stage {
                        Stage::Observing(o) => o.launch,
                        _ => (u64::MAX, u64::MAX),
                    };
                    if trade.sell
                        && trade.trader == t.creator
                        && (trade.block_number, trade.tx_index) != launch_tx
                    {
                        t.creator_sold.get_or_insert(trade.block_number);
                    }
                    match &mut t.stage {
                        Stage::Observing(o) => o.trade(&trade, at_ms, &t.book),
                        Stage::Open {
                            shadow, fill_block, ..
                        }
                        | Stage::Exiting {
                            shadow, fill_block, ..
                        } if trade.block_number > *fill_block => {
                            // A failure leaves the shadow untracked.
                            let _ = if trade.sell {
                                shadow.push_sell(trade.amount)
                            } else {
                                shadow.push_buy(trade.eth)
                            };
                        }
                        _ => {}
                    }
                }
            }
            Input::Migrated { token } => {
                if let Some(t) = self.token_mut(&token) {
                    t.book.migrated();
                }
            }
            Input::Incomplete => {
                for t in self.tokens.values_mut() {
                    t.book.gap();
                    if let Stage::Observing(o) = &mut t.stage {
                        o.gap = true;
                    }
                }
            }
            Input::Hold {
                token,
                tokens,
                spent_wei,
                filled_ms,
            } => {
                if let Some(t) = self.token_mut(&token) {
                    t.stage = Stage::Held {
                        entry: Entry {
                            tokens,
                            input_wei: spent_wei,
                            network_fee_wei: U256::ZERO,
                            spent_wei,
                        },
                        filled_ms,
                        triggered: false,
                    };
                }
            }
            Input::Unhold { token } => {
                self.tokens.retain(|_, t| t.token != token);
            }
        }
        out
    }

    /// The decisions of every window whose cutoff `time_ms` passed.
    fn decide(&mut self, number: u64, time_ms: u64, out: &mut Vec<Output>) {
        let keys: Vec<(u64, u64)> = self.tokens.keys().copied().collect();
        for key in keys {
            let Some(t) = self.tokens.get(&key) else {
                continue;
            };
            let Stage::Observing(o) = &t.stage else {
                continue;
            };
            if time_ms <= o.cutoff_ms {
                continue;
            }
            let features = o.features(&t.book, self.cfg.first_buyer_window);
            let decision = match (evaluate(&self.cfg.rules, &features), t.book.status) {
                (Verdict::DataIncomplete, _) => Decision::DataIncomplete,
                // Outside the scope or not exact: its features are no
                // basis for a filter verdict.
                (_, Err(why)) => Decision::NotTracked(why),
                (Verdict::Reject(f), Ok(())) => Decision::Reject(f),
                (Verdict::Pass, Ok(())) if t.creator_sold.is_some() => Decision::CreatorSold,
                (Verdict::Pass, Ok(())) if self.slots_taken() >= self.cfg.max_open => {
                    Decision::SlotLimit
                }
                (Verdict::Pass, Ok(())) => Decision::Pass,
            };
            let token = t.token;
            out.push(Output::Decided {
                token,
                block: number,
                decision,
                features: Box::new(features),
            });
            if decision == Decision::Pass {
                if let Some(t) = self.tokens.get_mut(&key) {
                    t.stage = Stage::Entering {
                        fill_block: number.saturating_add(self.cfg.fill_delay_blocks),
                        decided_block: number,
                    };
                }
            } else {
                self.tokens.remove(&key);
            }
        }
    }

    /// The net liquidation value of the position (the SELL's proceeds less
    /// every exit network fee) against its whole entry cost: whether it
    /// reached the target multiple, the stop multiple (`value <= stop *
    /// cost`, so a stop of 0 still fires once the value is not positive),
    /// or neither; `None` when the position cannot be quoted.
    fn valuation(&self, shadow: &TokenBook, e: &Entry) -> Option<(bool, bool)> {
        let sell = shadow.sell_inputs(e.tokens).ok()?;
        let x = exit(e, &sell, false, &self.gas, self.cfg.gas_price_wei).ok()?;
        let one = U512::from(u128::try_from(scale(self.cfg.ratio_places)?).ok()?);
        let spent = U512::from(e.spent_wei);
        let multiple = |m: i128| Some(U512::from(u128::try_from(m).ok()?) * spent);
        let (target, stop) = (
            multiple(self.cfg.target_net_multiple)?,
            multiple(self.cfg.stop_net_multiple)?,
        );
        Some(if x.proceeds_wei >= x.network_fee_wei {
            let value = U512::from(x.proceeds_wei - x.network_fee_wei) * one;
            (value >= target, value <= stop)
        } else {
            // A negative value: never the target, always the stop.
            (false, true)
        })
    }

    fn end_of_block(&mut self, number: u64, time_ms: u64, out: &mut Vec<Output>) {
        let keys: Vec<(u64, u64)> = self.tokens.keys().copied().collect();
        for key in keys {
            let Some(t) = self.tokens.get(&key).cloned() else {
                continue;
            };
            match t.stage {
                Stage::Observing(_) => {}
                // At the fill block, or the first block after it when its
                // height was skipped (a gap leaves the book untracked, so
                // that fill cancels).
                Stage::Entering {
                    fill_block,
                    decided_block,
                } if fill_block <= number => {
                    let sold_after_decision = t.creator_sold.is_some_and(|b| b >= decided_block);
                    let result = if sold_after_decision
                        && self.cfg.creator_sold_before_fill == CreatorSoldBeforeFill::FreeCancel
                    {
                        Err(Cancel::CreatorSold)
                    } else {
                        t.book
                            .buy_inputs(self.cfg.input_wei)
                            .map_err(Cancel::NotTracked)
                            .and_then(|buy| {
                                let e = entry(&buy, &self.gas, self.cfg.gas_price_wei).map_err(
                                    |e| match e {
                                        PositionError::Quote(q) => Cancel::Quote(q),
                                        PositionError::Cost(c) => Cancel::Cost(c),
                                        _ => Cancel::Cost(CostError::Overflow),
                                    },
                                )?;
                                let mut shadow = t.book.clone();
                                let tokens = shadow
                                    .push_buy(self.cfg.input_wei)
                                    .map_err(Cancel::NotTracked)?;
                                debug_assert_eq!(tokens, e.tokens);
                                Ok((e, Box::new(shadow)))
                            })
                    };
                    match result {
                        Ok((e, shadow)) => {
                            out.push(Output::Entered {
                                token: t.token,
                                block: number,
                                entry: e,
                            });
                            if let Some(t) = self.tokens.get_mut(&key) {
                                t.stage = Stage::Open {
                                    entry: e,
                                    shadow,
                                    fill_ms: time_ms,
                                    fill_block: number,
                                };
                            }
                        }
                        Err(cancel) => {
                            out.push(Output::EntryCancelled {
                                token: t.token,
                                block: number,
                                cancel,
                            });
                            self.tokens.remove(&key);
                        }
                    }
                }
                Stage::Entering { .. } => {}
                Stage::Open {
                    entry: e,
                    shadow,
                    fill_ms,
                    fill_block,
                } => {
                    if let Err(why) = t.book.status.and(shadow.status) {
                        out.push(Output::Unresolved {
                            token: t.token,
                            block: number,
                            reason: why,
                        });
                        self.tokens.remove(&key);
                        continue;
                    }
                    let mut reasons = Vec::new();
                    // Any sale of the creator latches once the position is
                    // open (one before the decision rejected it; one before
                    // the fill under TX_LANDS counts here).
                    if t.creator_sold.is_some() {
                        reasons.push(ExitReason::CreatorSell);
                    }
                    if let Some((target, stop)) = self.valuation(&shadow, &e) {
                        if target {
                            reasons.push(ExitReason::Target);
                        } else if stop {
                            reasons.push(ExitReason::Stop);
                        }
                    }
                    if time_ms >= fill_ms.saturating_add(self.cfg.hold_ms) {
                        reasons.push(ExitReason::Timeout);
                    }
                    if let Some(reason) = reasons.into_iter().min() {
                        out.push(Output::ExitTriggered {
                            token: t.token,
                            block: number,
                            reason,
                        });
                        if let Some(t) = self.tokens.get_mut(&key) {
                            t.stage = Stage::Exiting {
                                entry: e,
                                shadow,
                                fill_block,
                                exit_block: number.saturating_add(self.cfg.fill_delay_blocks),
                                reason,
                            };
                        }
                    }
                }
                Stage::Held {
                    entry: e,
                    filled_ms,
                    triggered: false,
                } => {
                    if let Err(why) = t.book.status {
                        out.push(Output::Unresolved {
                            token: t.token,
                            block: number,
                            reason: why,
                        });
                        self.tokens.remove(&key);
                        continue;
                    }
                    let mut reasons = Vec::new();
                    if t.creator_sold.is_some() {
                        reasons.push(ExitReason::CreatorSell);
                    }
                    // The real curve holds the real BUY: it is the one the
                    // position is valued and sold on.
                    if let Some((target, stop)) = self.valuation(&t.book, &e) {
                        if target {
                            reasons.push(ExitReason::Target);
                        } else if stop {
                            reasons.push(ExitReason::Stop);
                        }
                    }
                    if time_ms >= filled_ms.saturating_add(self.cfg.hold_ms) {
                        reasons.push(ExitReason::Timeout);
                    }
                    if let Some(reason) = reasons.into_iter().min() {
                        out.push(Output::ExitTriggered {
                            token: t.token,
                            block: number,
                            reason,
                        });
                        if let Some(t) = self.tokens.get_mut(&key) {
                            t.stage = Stage::Held {
                                entry: e,
                                filled_ms,
                                triggered: true,
                            };
                        }
                    }
                }
                Stage::Exiting {
                    entry: e,
                    shadow,
                    exit_block,
                    reason,
                    ..
                } if exit_block <= number => {
                    let valued = t
                        .book
                        .tracking()
                        .then(|| shadow.sell_inputs(e.tokens).ok())
                        .flatten()
                        .and_then(|sell| {
                            quote_native_sell(&sell).ok()?;
                            let x =
                                exit(&e, &sell, false, &self.gas, self.cfg.gas_price_wei).ok()?;
                            Some((x, outcome(&e, &x).ok()?))
                        });
                    match valued {
                        Some((x, o)) => out.push(Output::Exited {
                            token: t.token,
                            block: number,
                            reason,
                            exit: x,
                            outcome: o,
                        }),
                        None => out.push(Output::Unresolved {
                            token: t.token,
                            block: number,
                            reason: t
                                .book
                                .status
                                .and(shadow.status)
                                .err()
                                .unwrap_or(Untracked::ShadowReplay),
                        }),
                    }
                    self.tokens.remove(&key);
                }
                Stage::Exiting { .. }
                | Stage::Held {
                    triggered: true, ..
                } => {}
            }
        }
    }
}
