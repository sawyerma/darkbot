//! Strategy C scenarios on synthetic but exactly replayed Flap curves: each
//! trade is produced by the production quote at the book's state, so the
//! engine sees what the follower would report.

use alloy_primitives::U256;

use super::book::{Term, TokenBook, Trade};
use super::engine::*;
use super::filters::parse_rules;
use crate::economics::flap::native_curve20::{curve20, quote_native_buy, quote_native_sell};
use crate::economics::flap::position::Outcome;
use crate::economics::network::{Action, GasEvidence, PathClass, Venue};

const BLOCK_MS: u64 = 750;
const TOKEN: [u8; 20] = [0xAA; 20];
const CREATOR: [u8; 20] = [0xCC; 20];

fn gas() -> GasEvidence {
    let mut e = GasEvidence::default();
    for (action, used) in [
        (Action::Buy, 284_720),
        (Action::Approve, 49_248),
        (Action::Sell, 356_630),
    ] {
        e.add(
            PathClass {
                venue: Venue::Flap,
                action,
            },
            used,
        )
        .unwrap();
    }
    e
}

fn config(rules: &str) -> Config {
    Config {
        observation_ms: 3_000,
        ratio_places: 4,
        first_buyer_window: 5,
        rules: parse_rules(rules, 4).unwrap(),
        // 1.5x and 0.5x.
        target_net_multiple: 15_000,
        stop_net_multiple: 5_000,
        hold_ms: 60_000,
        max_open: 2,
        input_wei: U256::from(10_000_000_000_000_000u64),
        fill_delay_blocks: 1,
        creator_sold_before_fill: CreatorSoldBeforeFill::FreeCancel,
        gas_price_wei: U256::from(100_000_000u64),
    }
}

/// Drives an engine with a shadow book per token that produces each
/// trade exactly as the Portal would report it.
struct Chain {
    engine: Engine,
    books: Vec<([u8; 20], TokenBook)>,
    block: u64,
    tx: u64,
    out: Vec<Output>,
}

impl Chain {
    fn new(cfg: Config) -> Self {
        let mut c = Self {
            engine: Engine::new(cfg, gas()),
            books: Vec::new(),
            block: 100,
            tx: 0,
            out: Vec::new(),
        };
        c.feed(Input::Block {
            number: 100,
            time_ms: 0,
        });
        c
    }

    fn feed(&mut self, input: Input) {
        self.out.extend(self.engine.step(input));
    }

    fn next_block(&mut self) {
        self.block += 1;
        self.tx = 0;
        self.feed(Input::Block {
            number: self.block,
            time_ms: (self.block - 100) * BLOCK_MS,
        });
    }

    fn book(&mut self, token: [u8; 20]) -> &mut TokenBook {
        let i = self.books.iter().position(|(t, _)| *t == token).unwrap();
        &mut self.books[i].1
    }

    fn launch(&mut self, token: [u8; 20], dev_wei: u64) {
        self.tx += 1;
        let tx_index = self.tx;
        self.feed(Input::Launch {
            token,
            creator: CREATOR,
            tx_index,
        });
        let c = curve20();
        let mut book = TokenBook::new();
        for term in [
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
            book.term(term);
            self.feed(Input::Term { token, term });
        }
        self.books.push((token, book));
        self.tx -= 1;
        self.buy(token, CREATOR, dev_wei);
    }

    fn buy(&mut self, token: [u8; 20], trader: [u8; 20], wei: u64) {
        self.tx += 1;
        let (block, tx) = (self.block, self.tx);
        let book = self.book(token);
        let eth = U256::from(wei);
        let q = quote_native_buy(&book.buy_inputs(eth).unwrap()).unwrap();
        let trade = Trade {
            block_number: block,
            tx_index: tx,
            log_index: 0,
            sell: false,
            trader,
            amount: q.output_amount,
            eth,
            fee: q.protocol_fee,
        };
        book.apply(&trade);
        self.feed(Input::Trade { token, trade });
    }

    /// Sells `ppm` of the circulating supply.
    fn sell(&mut self, token: [u8; 20], trader: [u8; 20], ppm: u64) {
        self.tx += 1;
        let (block, tx) = (self.block, self.tx);
        let book = self.book(token);
        let amount = book.circulating * U256::from(ppm) / U256::from(1_000_000u64);
        let q = quote_native_sell(&book.sell_inputs(amount).unwrap()).unwrap();
        let trade = Trade {
            block_number: block,
            tx_index: tx,
            log_index: 0,
            sell: true,
            trader,
            amount,
            eth: q.output_amount,
            fee: q.protocol_fee,
        };
        book.apply(&trade);
        self.feed(Input::Trade { token, trade });
    }

    /// The launch, five buyers in the window, then blocks until the
    /// decision (block 105, the first past the 3 s cutoff).
    fn passing_launch(&mut self, token: [u8; 20]) {
        self.launch(token, 5_000_000_000_000_000);
        for i in 1..=4u8 {
            self.next_block();
            self.buy(token, [i; 20], 20_000_000_000_000_000);
        }
        self.next_block(); // 105: decided, fills at the end of 106
    }

    fn kinds(&self) -> Vec<String> {
        self.out
            .iter()
            .map(|o| match o {
                Output::Decided {
                    decision, block, ..
                } => format!("decided {block} {decision:?}"),
                Output::Entered { block, .. } => format!("entered {block}"),
                Output::EntryCancelled { block, cancel, .. } => {
                    format!("cancelled {block} {cancel:?}")
                }
                Output::ExitTriggered { block, reason, .. } => {
                    format!("trigger {block} {reason:?}")
                }
                Output::Exited {
                    block,
                    reason,
                    outcome,
                    ..
                } => format!(
                    "exited {block} {reason:?} {}",
                    match outcome {
                        Outcome::Gain(_) => "gain",
                        Outcome::Loss(_) => "loss",
                    }
                ),
                Output::Unresolved { block, reason, .. } => {
                    format!("unresolved {block} {reason:?}")
                }
            })
            .collect()
    }
}

/// PASS, the BUY one block later, a rally to the target, the SELL one
/// block after the trigger: a gain net of every cost.
#[test]
fn a_passing_launch_enters_and_exits_at_the_target() {
    let mut c = Chain::new(config("buy_count:3:,unique_buyers:3:"));
    c.passing_launch(TOKEN);
    c.next_block(); // 106: the fill at its end
    c.next_block(); // 107
    for i in 10..30u8 {
        c.buy(TOKEN, [i; 20], 200_000_000_000_000_000);
    }
    c.next_block(); // 108: target at the end of 107
    c.next_block(); // 109: exit filled at the end of 108
    assert_eq!(
        c.kinds(),
        [
            "decided 105 Pass",
            "entered 106",
            "trigger 107 Target",
            "exited 108 Target gain"
        ]
    );
    let Some(Output::Entered { entry, .. }) = c.out.get(1) else {
        panic!()
    };
    assert_eq!(entry.input_wei, U256::from(10_000_000_000_000_000u64));
    assert!(entry.spent_wei > entry.input_wei);
    assert_eq!(c.engine.followed(), 0);
}

/// A filter that fails rejects the token for good.
#[test]
fn a_failing_filter_rejects() {
    let mut c = Chain::new(config("buy_count:10:"));
    c.passing_launch(TOKEN);
    assert_eq!(c.kinds(), ["decided 105 Reject(0)"]);
    assert_eq!(c.engine.followed(), 0);
}

/// The creator selling between the decision and the fill cancels at no
/// cost, unless the BUY is taken to land.
#[test]
fn a_creator_sale_before_the_fill_cancels_or_lands() {
    for (policy, expected) in [
        (
            CreatorSoldBeforeFill::FreeCancel,
            "cancelled 106 CreatorSold",
        ),
        (CreatorSoldBeforeFill::TxLands, "entered 106"),
    ] {
        let mut cfg = config("");
        cfg.creator_sold_before_fill = policy;
        let mut c = Chain::new(cfg);
        c.passing_launch(TOKEN);
        c.sell(TOKEN, CREATOR, 100_000);
        c.next_block(); // 106
        c.next_block(); // end of 106
        assert_eq!(
            c.kinds()[..2],
            ["decided 105 Pass".to_string(), expected.to_string()]
        );
    }
}

/// After the fill: the creator's sale wins over every other trigger.
#[test]
fn a_creator_sale_after_the_fill_exits_first() {
    let mut c = Chain::new(config(""));
    c.passing_launch(TOKEN);
    c.next_block();
    c.next_block(); // 107
    c.sell(TOKEN, CREATOR, 10_000);
    c.next_block();
    c.next_block();
    assert_eq!(
        c.kinds()[1..],
        [
            "entered 106",
            "trigger 107 CreatorSell",
            "exited 108 CreatorSell loss"
        ]
    );
}

/// Without a move the position times out; a crash hits the stop.
#[test]
fn timeout_and_stop() {
    let mut cfg = config("");
    cfg.hold_ms = 1_500;
    let mut c = Chain::new(cfg);
    c.passing_launch(TOKEN);
    // Filled at the end of 106 (4.5 s); 1.5 s later is block 108; its SELL
    // fills at the end of 109, seen when 110 arrives.
    for _ in 0..5 {
        c.next_block();
    }
    assert_eq!(
        c.kinds()[1..],
        [
            "entered 106",
            "trigger 108 Timeout",
            "exited 109 Timeout loss"
        ]
    );
    // The stop: the early curve is nearly flat (6.14 BNB of virtual
    // reserve), so the entry comes high on it (about 10 BNB raised in the
    // window) and the holders then sell nearly everything.
    let mut c = Chain::new(config(""));
    c.launch(TOKEN, 5_000_000_000_000_000);
    for i in 1..=4u8 {
        c.next_block();
        for j in 0..5u8 {
            c.buy(TOKEN, [i * 10 + j; 20], 500_000_000_000_000_000);
        }
    }
    c.next_block(); // 105: decided
    c.next_block(); // 106: filled at its end
    c.next_block(); // 107
    for i in 1..=4u8 {
        for j in 0..5u8 {
            c.sell(TOKEN, [i * 10 + j; 20], 200_000);
        }
    }
    c.next_block();
    c.next_block();
    assert_eq!(
        c.kinds()[1..],
        ["entered 106", "trigger 107 Stop", "exited 108 Stop loss"]
    );
}

/// A gap before the decision is DATA_INCOMPLETE; one on an open position
/// leaves it unresolved, never valued.
#[test]
fn gaps_fail_closed() {
    let mut c = Chain::new(config(""));
    c.launch(TOKEN, 5_000_000_000_000_000);
    c.next_block();
    c.feed(Input::Incomplete);
    for _ in 0..4 {
        c.next_block();
    }
    assert_eq!(c.kinds(), ["decided 105 DataIncomplete"]);
    let mut c = Chain::new(config(""));
    c.passing_launch(TOKEN);
    c.next_block();
    c.next_block(); // 107
    c.feed(Input::Incomplete);
    c.next_block();
    assert_eq!(c.kinds()[1..], ["entered 106", "unresolved 107 Gap"]);
}

/// A launch outside the priced scope (another quote token) is NOT_TRACKED
/// with its reason, never DATA_INCOMPLETE and never a filter verdict.
#[test]
fn an_unsupported_launch_is_not_tracked() {
    use super::book::Untracked;
    let mut c = Chain::new(config("buy_count:10:"));
    c.feed(Input::Launch {
        token: TOKEN,
        creator: CREATOR,
        tx_index: 1,
    });
    c.feed(Input::Term {
        token: TOKEN,
        term: Term::QuoteToken([9; 20]),
    });
    for _ in 0..5 {
        c.next_block();
    }
    assert!(matches!(
        c.out.as_slice(),
        [Output::Decided {
            block: 105,
            decision: Decision::NotTracked(Untracked::Unsupported("quote token")),
            ..
        }]
    ));
}

/// With one slot the second passing launch of the same window is discarded.
#[test]
fn a_busy_slot_discards_for_good() {
    let mut cfg = config("");
    cfg.max_open = 1;
    let mut c = Chain::new(cfg);
    c.launch(TOKEN, 5_000_000_000_000_000);
    c.launch([0xBB; 20], 5_000_000_000_000_000);
    for _ in 0..5 {
        c.next_block();
    }
    assert_eq!(c.kinds(), ["decided 105 Pass", "decided 105 SlotLimit"]);
}

/// H gate HIGH 1: the exit fills after the real trades of its block; a dump
/// in that block lowers the proceeds (before: the shadow ignored every
/// trade while exiting).
#[test]
fn the_exit_fills_after_the_trades_of_its_block() {
    let run = |dump: bool| {
        let mut cfg = config("");
        cfg.hold_ms = 1_500;
        let mut c = Chain::new(cfg);
        c.launch(TOKEN, 5_000_000_000_000_000);
        for i in 1..=4u8 {
            c.next_block();
            for j in 0..5u8 {
                c.buy(TOKEN, [i * 10 + j; 20], 500_000_000_000_000_000);
            }
        }
        // 105: decided; 106: filled at its end; the timeout triggers at
        // the end of 108 (seen when 109 arrives).
        for _ in 0..5 {
            c.next_block();
        }
        // 109: the exit block; the holders dump in it.
        if dump {
            for i in 1..=4u8 {
                c.sell(TOKEN, [i * 10; 20], 150_000);
            }
        }
        c.next_block(); // 110: the exit filled at the end of 109
        let exited = c.out.iter().find_map(|o| match o {
            Output::Exited { exit, block, .. } => Some((*block, exit.proceeds_wei)),
            _ => None,
        });
        (c.kinds(), exited.unwrap())
    };
    let (kinds, (block, calm)) = run(false);
    assert_eq!(
        kinds[1..],
        [
            "entered 106",
            "trigger 108 Timeout",
            "exited 109 Timeout loss"
        ]
    );
    assert_eq!(block, 109);
    let (_, (_, dumped)) = run(true);
    assert!(dumped < calm, "{dumped} < {calm}");
}

/// H gate MEDIUM 4: a fill block whose height was skipped (a gap) cancels
/// the entry at the next block; the slot is free again (before: the token
/// stayed Entering for good and held its slot).
#[test]
fn a_skipped_fill_block_cancels_and_frees_the_slot() {
    use super::book::Untracked;
    let mut cfg = config("");
    cfg.max_open = 1;
    let mut c = Chain::new(cfg);
    c.passing_launch(TOKEN); // 105: decided, fills at the end of 106
    c.feed(Input::Incomplete);
    c.block = 108; // 106 and 107 skipped
    c.next_block(); // 109: the end of 108
    c.next_block();
    assert!(
        c.out.iter().any(|o| matches!(
            o,
            Output::EntryCancelled {
                block: 109,
                cancel: Cancel::NotTracked(Untracked::Gap),
                ..
            }
        )),
        "{:?}",
        c.kinds()
    );
    assert_eq!(c.engine.followed(), 0);
}

/// H gate MEDIUM 5: under TX_LANDS a creator sale between the decision and
/// the fill lets the BUY land, then latches CREATOR_SELL at once (the
/// original's latch for any creator sale once filled).
#[test]
fn tx_lands_then_exits_on_the_creators_earlier_sale() {
    let mut cfg = config("");
    cfg.creator_sold_before_fill = CreatorSoldBeforeFill::TxLands;
    let mut c = Chain::new(cfg);
    c.passing_launch(TOKEN);
    c.sell(TOKEN, CREATOR, 100_000);
    c.next_block(); // 106
    c.next_block(); // the fill at the end of 106; 107
    c.next_block(); // the end of 107
    assert_eq!(
        c.kinds()[..3],
        [
            "decided 105 Pass".to_string(),
            "entered 106".to_string(),
            "trigger 107 CreatorSell".to_string()
        ]
    );
}
