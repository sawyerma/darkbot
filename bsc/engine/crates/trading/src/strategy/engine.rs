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

use std::collections::BTreeMap;

use alloy_primitives::{U256, U512};

use super::book::{Term, TokenBook, Trade, Untracked};
use super::features::{Observation, Value};
use super::filters::{Rule, Verdict, evaluate};
use super::fixed::scale;
use crate::economics::flap::QuoteError;
use crate::economics::flap::native_curve20::quote_native_sell;
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
    Exiting {
        entry: Entry,
        shadow: Box<TokenBook>,
        fill_block: u64,
        reason: ExitReason,
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
                (Verdict::Reject(f), _) => Decision::Reject(f),
                (Verdict::Pass, Err(why)) => Decision::NotTracked(why),
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
                Stage::Entering {
                    fill_block,
                    decided_block,
                } if fill_block == number => {
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
                    if t.creator_sold.is_some_and(|b| b >= fill_block) {
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
                                fill_block: number.saturating_add(self.cfg.fill_delay_blocks),
                                reason,
                            };
                        }
                    }
                }
                Stage::Exiting {
                    entry: e,
                    shadow,
                    fill_block,
                    reason,
                } if fill_block == number => {
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
                Stage::Exiting { .. } => {}
            }
        }
    }
}
