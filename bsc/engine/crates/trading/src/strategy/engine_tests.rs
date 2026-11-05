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

/// Plan 28.10.: a real LIVE position held on a launch the filter would
/// reject: never decided away, valued on the real curve (its own BUY in
/// it) by the same rules; the target triggers its exit once; its tokens
/// and the real curve are what the exit sells on; `Unhold` ends it.
#[test]
fn a_held_real_position_exits_by_the_same_rules_once() {
    let mut c = Chain::new(config("buy_count:100:"));
    c.launch(TOKEN, 5_000_000_000_000_000);
    // The real BUY (ours) is one of the curve's trades.
    let input = 10_000_000_000_000_000u64;
    let before = c.book(TOKEN).clone();
    let q = quote_native_buy(&before.buy_inputs(U256::from(input)).unwrap()).unwrap();
    c.buy(TOKEN, [0xEE; 20], input);
    c.feed(Input::Hold {
        token: TOKEN,
        tokens: q.output_amount,
        spent_wei: U256::from(input + 30_000_000_000_000),
        filled_ms: 0,
    });
    assert_eq!(c.engine.held_positions(), 1);
    assert_eq!(c.engine.position_tokens(&TOKEN), Some(q.output_amount));
    for _ in 0..5 {
        c.next_block();
    }
    // Held: no decision, no trigger on a flat curve.
    assert!(c.kinds().is_empty(), "{:?}", c.kinds());
    for i in 10..30u8 {
        c.buy(TOKEN, [i; 20], 200_000_000_000_000_000);
    }
    c.next_block();
    c.next_block();
    c.next_block();
    assert_eq!(c.kinds(), ["trigger 105 Target"]);
    let Some(StrategyCurve::Flap { reserve, .. }) = c.engine.curve_of(&TOKEN) else {
        panic!("a Flap curve")
    };
    assert_eq!(reserve, c.book(TOKEN).reserve, "the real curve");
    c.feed(Input::Unhold { token: TOKEN });
    assert_eq!((c.engine.held_positions(), c.engine.followed()), (0, 0));
}

/// The terms the test chain's tokens carry, as `getTokenV8Safe` would read
/// them.
fn read_terms(book: &TokenBook) -> crate::economics::flap::native_curve20::TokenTerms {
    crate::economics::flap::native_curve20::TokenTerms {
        tradable: true,
        version: book.terms.version.unwrap(),
        quote_token: book.terms.quote_token.unwrap(),
        nft_gate: [0; 20],
        curve: book.terms.curve.unwrap(),
        dex_supply_threshold: book.terms.dex_threshold.unwrap(),
        buy_tax_bps: book.terms.buy_tax_bps.unwrap(),
        sell_tax_bps: book.terms.sell_tax_bps.unwrap(),
    }
}

/// 01.11. (R1): a held real position whose tracking a gap ended is
/// unresolved and no longer held; held again on its curve as read
/// (`Reheld`), it is valued on that curve and every later trade, and its
/// exit triggers by the same rules. A token this engine never followed (a
/// restart) is held the same; one off the curve (migrated) is held but
/// never valued: unresolved at the next block, not exited.
#[test]
fn a_position_held_again_on_its_read_curve_exits_by_the_same_rules() {
    let mut c = Chain::new(config("buy_count:100:"));
    c.launch(TOKEN, 5_000_000_000_000_000);
    let input = 10_000_000_000_000_000u64;
    let q = quote_native_buy(&c.book(TOKEN).buy_inputs(U256::from(input)).unwrap()).unwrap();
    c.buy(TOKEN, [0xEE; 20], input);
    let spent = U256::from(input + 30_000_000_000_000);
    c.feed(Input::Hold {
        token: TOKEN,
        tokens: q.output_amount,
        spent_wei: spent,
        filled_ms: 0,
    });
    assert!(c.engine.holds(&TOKEN));
    c.feed(Input::Incomplete);
    c.next_block();
    assert_eq!(c.kinds(), ["unresolved 100 Gap"], "{:?}", c.kinds());
    assert!(!c.engine.holds(&TOKEN));
    let held = |c: &mut Chain, token: [u8; 20], creator: [u8; 20]| {
        let book = c.book(token).clone();
        c.feed(Input::Reheld(Reheld {
            token,
            creator,
            terms: read_terms(&book),
            reserve: book.reserve,
            circulating: book.circulating,
            tokens: q.output_amount,
            spent_wei: spent,
            filled_ms: 0,
        }));
    };
    held(&mut c, TOKEN, CREATOR);
    assert!(c.engine.holds(&TOKEN));
    assert_eq!(c.engine.position_tokens(&TOKEN), Some(q.output_amount));
    c.out.clear();
    for i in 10..30u8 {
        c.buy(TOKEN, [i; 20], 200_000_000_000_000_000);
    }
    c.next_block();
    c.next_block();
    assert_eq!(c.kinds(), ["trigger 101 Target"]);
    let Some(StrategyCurve::Flap { reserve, .. }) = c.engine.curve_of(&TOKEN) else {
        panic!("a Flap curve")
    };
    assert_eq!(reserve, c.book(TOKEN).reserve, "the real curve");
    // A token this engine never followed: held on its read curve alone.
    let other = [0xAB; 20];
    let mut fresh = Chain::new(config("buy_count:100:"));
    fresh.launch(other, 5_000_000_000_000_000);
    let mut unknown = Chain::new(config("buy_count:100:"));
    unknown.books = fresh.books.clone();
    held(&mut unknown, other, [0; 20]);
    assert!(unknown.engine.holds(&other));
    assert_eq!(unknown.engine.followed(), 1);
    for i in 10..30u8 {
        unknown.buy(other, [i; 20], 200_000_000_000_000_000);
    }
    unknown.next_block();
    unknown.next_block();
    assert_eq!(unknown.kinds(), ["trigger 100 Target"]);
    // Off the curve: held, never valued, unresolved at the next block.
    let mut migrated = Chain::new(config("buy_count:100:"));
    migrated.books = fresh.books.clone();
    let book = migrated.book(other).clone();
    migrated.feed(Input::Reheld(Reheld {
        token: other,
        creator: [0; 20],
        terms: crate::economics::flap::native_curve20::TokenTerms {
            tradable: false,
            ..read_terms(&book)
        },
        reserve: book.reserve,
        circulating: book.circulating,
        tokens: q.output_amount,
        spent_wei: spent,
        filled_ms: 0,
    }));
    assert!(migrated.engine.curve_of(&other).is_none());
    migrated.next_block();
    assert_eq!(migrated.kinds(), ["unresolved 100 Migrated"]);
}

/// 01.11. (R1 review): a held position a gap untracked keeps its creator
/// when held again without one: the creator's sale still triggers its exit.
#[test]
fn a_position_held_again_after_a_gap_keeps_its_creator_rule() {
    let mut c = Chain::new(config("buy_count:100:"));
    c.launch(TOKEN, 5_000_000_000_000_000);
    let input = 10_000_000_000_000_000u64;
    let q = quote_native_buy(&c.book(TOKEN).buy_inputs(U256::from(input)).unwrap()).unwrap();
    c.buy(TOKEN, [0xEE; 20], input);
    let spent = U256::from(input + 30_000_000_000_000);
    c.feed(Input::Hold {
        token: TOKEN,
        tokens: q.output_amount,
        spent_wei: spent,
        filled_ms: 0,
    });
    c.feed(Input::Incomplete);
    c.next_block();
    assert!(!c.engine.holds(&TOKEN));
    let book = c.book(TOKEN).clone();
    c.feed(Input::Reheld(Reheld {
        token: TOKEN,
        creator: [0; 20],
        terms: read_terms(&book),
        reserve: book.reserve,
        circulating: book.circulating,
        tokens: q.output_amount,
        spent_wei: spent,
        filled_ms: 0,
    }));
    assert!(c.engine.holds(&TOKEN));
    c.out.clear();
    c.sell(TOKEN, CREATOR, 1_000);
    c.next_block();
    c.next_block();
    assert_eq!(c.kinds(), ["trigger 101 CreatorSell"], "{:?}", c.kinds());
}

/// R1 review: a held position whose curve a real trade no longer
/// reproduces is not held for its exit any more (the runtime reads its
/// curve again), even before the block ends.
#[test]
fn a_held_position_on_an_untracked_curve_is_not_held() {
    let mut c = Chain::new(config("buy_count:100:"));
    c.launch(TOKEN, 5_000_000_000_000_000);
    c.feed(Input::Hold {
        token: TOKEN,
        tokens: U256::from(1_000u64),
        spent_wei: U256::from(10_000u64),
        filled_ms: 0,
    });
    assert!(c.engine.holds(&TOKEN));
    // A BUY whose amounts the quote does not reproduce: TradeMismatch.
    c.feed(Input::Trade {
        token: TOKEN,
        trade: Trade {
            block_number: 100,
            tx_index: 9,
            log_index: 0,
            sell: false,
            trader: [0x51; 20],
            amount: U256::from(1u8),
            eth: U256::from(1_000_000_000_000_000u64),
            fee: U256::ZERO,
        },
    });
    assert!(
        !c.engine.holds(&TOKEN),
        "not held on a curve it no longer tracks"
    );
}

/// R1 re-check (LOW 4): the creator's sale while a gap-untracked position
/// waits for its curve read is not lost: held again, its exit triggers by
/// the creator rule.
#[test]
fn a_creator_sale_while_a_position_waits_to_be_held_again_still_counts() {
    let mut c = Chain::new(config("buy_count:100:"));
    c.launch(TOKEN, 5_000_000_000_000_000);
    let input = 10_000_000_000_000_000u64;
    let q = quote_native_buy(&c.book(TOKEN).buy_inputs(U256::from(input)).unwrap()).unwrap();
    c.buy(TOKEN, [0xEE; 20], input);
    let spent = U256::from(input + 30_000_000_000_000);
    c.feed(Input::Hold {
        token: TOKEN,
        tokens: q.output_amount,
        spent_wei: spent,
        filled_ms: 0,
    });
    c.feed(Input::Incomplete);
    c.next_block();
    assert!(!c.engine.holds(&TOKEN));
    // Not followed now: the creator sells.
    c.sell(TOKEN, CREATOR, 1_000);
    c.next_block();
    let book = c.book(TOKEN).clone();
    c.feed(Input::Reheld(Reheld {
        token: TOKEN,
        creator: [0; 20],
        terms: read_terms(&book),
        reserve: book.reserve,
        circulating: book.circulating,
        tokens: q.output_amount,
        spent_wei: spent,
        filled_ms: 0,
    }));
    c.out.clear();
    c.next_block();
    assert_eq!(c.kinds(), ["trigger 102 CreatorSell"], "{:?}", c.kinds());
}

/// R1 re-check (LOW 3): a held position counts as decided only on a curve
/// the engine prices: one whose terms are incomplete is not held.
#[test]
fn a_held_position_without_its_terms_is_not_held() {
    let mut c = Chain::new(config("buy_count:100:"));
    // Launched, but no terms reached the engine.
    c.feed(Input::Launch {
        token: TOKEN,
        creator: CREATOR,
        tx_index: 1,
    });
    c.feed(Input::Hold {
        token: TOKEN,
        tokens: U256::from(1_000u64),
        spent_wei: U256::from(10_000u64),
        filled_ms: 0,
    });
    assert!(!c.engine.holds(&TOKEN), "no priced curve, no decided exit");
}

/// SC4: an OpenFour token on its exact curve, driven as the follower
/// reports it (each trade with the state it left).
mod four {
    use super::*;
    use crate::economics::four_classic::LaunchTerms;
    use crate::economics::openfour::{OpenFourCurve, OpenFourFee, launch_curve};
    use crate::strategy::book::{FourCurve, FourPost};

    const FOUR_TOKEN: [u8; 20] = [0xF4; 20];
    const FEE: OpenFourFee = OpenFourFee {
        protocol_bps: 50,
        dev_bps: 50,
    };

    fn gas() -> GasEvidence {
        let mut e = super::gas();
        // The frozen OpenFour receipts' largest gas used per class.
        for (action, used) in [
            (Action::Buy, 1_694_570),
            (Action::Approve, 48_788),
            (Action::Sell, 570_922),
        ] {
            e.add(
                PathClass {
                    venue: Venue::OpenFour,
                    action,
                },
                used,
            )
            .unwrap();
        }
        e
    }

    fn start() -> OpenFourCurve {
        let e18 = U256::from(10u64).pow(U256::from(18u8));
        launch_curve(&LaunchTerms {
            total_supply: U256::from(1_000_000_000u64) * e18,
            max_offers: U256::from(800_000_000u64) * e18,
            max_raising: U256::from(18u64) * e18,
        })
        .unwrap()
    }

    /// The chain's OpenFour curve and the engine fed with its trades.
    struct FourChain {
        engine: Engine,
        real: FourCurve,
        block: u64,
        tx: u64,
        out: Vec<Output>,
    }

    impl FourChain {
        fn new(cfg: Config) -> Self {
            let mut c = Self {
                engine: Engine::new(cfg, gas()),
                real: FourCurve::openfour(start(), FEE),
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

        /// The state the chain's curve reports.
        fn post(&self) -> FourPost {
            let FourCurve::OpenFour { start, now, .. } = self.real else {
                panic!()
            };
            FourPost {
                remaining: now.remaining,
                raised: now.quote_reserve - start.quote_reserve,
            }
        }

        /// A BUY of `wei` by `trader` (`launch`: inside the launch tx).
        fn buy(&mut self, trader: [u8; 20], wei: u64, launch: bool) {
            if !launch {
                self.tx += 1;
            }
            let (tokens, paid) = self.real.buy(U256::from(wei)).unwrap();
            self.real = self.real.shifted(tokens).unwrap();
            let trade = Trade {
                block_number: self.block,
                tx_index: self.tx,
                log_index: 0,
                sell: false,
                trader,
                amount: tokens,
                eth: paid,
                fee: U256::ZERO,
            };
            let post = self.post();
            self.feed(Input::FourTrade {
                token: FOUR_TOKEN,
                trade,
                post,
            });
        }

        /// The launch with its creator's BUY, four buyers, the decision
        /// block (105).
        fn passing_launch(&mut self) {
            self.tx += 1;
            self.feed(Input::FourLaunch {
                token: FOUR_TOKEN,
                creator: CREATOR,
                tx_index: self.tx,
                curve: Ok(self.real),
            });
            self.buy(CREATOR, 5_000_000_000_000_000, true);
            for i in 1..=4u8 {
                self.next_block();
                self.buy([i; 20], 20_000_000_000_000_000, false);
            }
            self.next_block();
        }

        fn kinds(&self) -> Vec<String> {
            self.out
                .iter()
                .map(|o| match o {
                    Output::Decided {
                        decision, block, ..
                    } => format!("decided {block} {decision:?}"),
                    Output::Entered { block, .. } => format!("entered {block}"),
                    Output::ExitTriggered { block, reason, .. } => {
                        format!("trigger {block} {reason:?}")
                    }
                    Output::Exited { block, reason, .. } => format!("exited {block} {reason:?}"),
                    Output::Unresolved { block, reason, .. } => {
                        format!("unresolved {block} {reason:?}")
                    }
                    other => format!("{other:?}"),
                })
                .collect()
        }
    }

    /// PASS, the exact-amount BUY at the fill (the largest within the
    /// input, its payment the input it takes, the OpenFour BUY's gas on
    /// top), a rally to the target on the shadow (the real curve with the
    /// position's tokens), the SELL's proceeds that shadow's exact quote.
    #[test]
    fn an_openfour_launch_enters_and_exits_at_the_target_on_its_exact_curve() {
        let mut c = FourChain::new(config("buy_count:3:,unique_buyers:3:"));
        c.passing_launch();
        c.next_block(); // 106: the fill at its end
        let at_fill = c.real;
        c.next_block(); // 107
        for i in 10..30u8 {
            c.buy([i; 20], 200_000_000_000_000_000, false);
        }
        let at_exit = c.real;
        c.next_block(); // 108: target at the end of 107
        c.next_block(); // 109: exit filled at the end of 108
        assert_eq!(
            c.kinds(),
            [
                "decided 105 Pass",
                "entered 106",
                "trigger 107 Target",
                "exited 108 Target"
            ]
        );
        let Some(Output::Entered { entry, .. }) = c.out.get(1) else {
            panic!()
        };
        let input = U256::from(10_000_000_000_000_000u64);
        let (tokens, paid) = at_fill.buy(input).unwrap();
        assert_eq!((entry.tokens, entry.input_wei), (tokens, paid));
        assert!(paid <= input);
        assert_eq!(
            entry.spent_wei,
            paid + U256::from(1_694_570u64) * U256::from(100_000_000u64)
        );
        let Some(Output::Exited { exit, outcome, .. }) = c.out.get(3) else {
            panic!()
        };
        let shadow = at_exit.shifted(tokens).unwrap();
        assert_eq!(exit.proceeds_wei, shadow.sell(tokens).unwrap());
        assert_eq!(
            exit.network_fee_wei,
            U256::from(570_922u64 + 48_788) * U256::from(100_000_000u64)
        );
        assert!(matches!(outcome, Outcome::Gain(_)));
        assert_eq!(c.engine.followed(), 0);
    }

    /// A Classic token: the fill takes the wei its BUY takes (the gwei-
    /// floored tokens' cost, fee and tax), the rest of the input refunded,
    /// the Classic BUY's gas on top.
    #[test]
    fn a_classic_fill_takes_what_its_buy_takes() {
        use crate::economics::four_classic::{ClassicCurve, ClassicFee, quote_buy, virtual_quote};
        let e18 = U256::from(10u64).pow(U256::from(18u8));
        let sale = U256::from(800_000_000u64) * e18;
        let start = ClassicCurve {
            k: U256::from_str_radix("5296303246049315066534058923", 10).unwrap(),
            t: U256::from(1_073_972_602u64) * e18,
            offers: sale,
            sold: U256::ZERO,
        };
        let fee = ClassicFee {
            rate_bps: 100,
            min_fee: U256::ZERO,
            tax_bps: 1_000,
        };
        let mut gas = gas();
        gas.add(
            PathClass {
                venue: Venue::FourClassic,
                action: Action::Buy,
            },
            1_000_000,
        )
        .unwrap();
        let mut engine = Engine::new(config(""), gas);
        let mut out = Vec::new();
        out.extend(engine.step(Input::Block {
            number: 100,
            time_ms: 0,
        }));
        out.extend(engine.step(Input::FourLaunch {
            token: FOUR_TOKEN,
            creator: CREATOR,
            tx_index: 1,
            curve: Ok(FourCurve::classic(start, fee).unwrap()),
        }));
        // Decided at 105 (past the 3 s cutoff), filled at the end of 106.
        for n in 101..=107u64 {
            out.extend(engine.step(Input::Block {
                number: n,
                time_ms: (n - 100) * BLOCK_MS,
            }));
        }
        let entry = out
            .iter()
            .find_map(|o| match o {
                Output::Entered { entry, .. } => Some(*entry),
                _ => None,
            })
            .expect("the entry");
        let input = U256::from(10_000_000_000_000_000u64);
        let q = quote_buy(&start, &fee, input).unwrap();
        assert!(q.spent < input, "a refund");
        assert_eq!((entry.tokens, entry.input_wei), (q.amount, q.spent));
        assert_eq!(
            entry.spent_wei,
            q.spent + U256::from(1_000_000u64) * U256::from(100_000_000u64)
        );
        assert!(virtual_quote(&q.after).unwrap() > virtual_quote(&start).unwrap());
    }

    /// Each venue has its own slots: an open Flap position at `max_open` 1
    /// leaves a Four.meme PASS its entry (and a refused Flap log leaves the
    /// Four.meme curve tracked).
    #[test]
    fn each_venue_has_its_own_slots() {
        let mut cfg = config("buy_count:3:,unique_buyers:3:");
        cfg.max_open = 1;
        let mut flap = Chain::new(cfg.clone());
        // One engine with the gas of both venues.
        flap.engine = Engine::new(cfg.clone(), gas());
        flap.feed(Input::Block {
            number: 100,
            time_ms: 0,
        });
        flap.passing_launch(TOKEN);
        flap.next_block(); // 106: the Flap fill
        flap.next_block(); // 107
        assert_eq!(flap.kinds(), ["decided 105 Pass", "entered 106"]);
        // A Four.meme launch now, on the same engine.
        let mut c = FourChain::new(cfg);
        c.engine = flap.engine.clone();
        c.block = flap.block;
        c.passing_launch();
        c.feed(Input::FlapIncomplete);
        c.next_block();
        c.next_block();
        let kinds = c.kinds();
        assert!(
            kinds.contains(&format!("decided {} Pass", 107 + 5)),
            "{kinds:?}"
        );
        assert!(kinds.contains(&format!("entered {}", 107 + 6)), "{kinds:?}");
    }

    /// SC4 c: a followed Four.meme token's intent curve is its exact Four
    /// curve while tracked, none once its tracking ended (a refused log).
    #[test]
    fn a_four_tokens_curve_is_given_only_while_tracked() {
        let mut c = FourChain::new(config("buy_count:3:,unique_buyers:3:"));
        c.passing_launch();
        assert_eq!(
            c.engine.curve_of(&FOUR_TOKEN),
            Some(StrategyCurve::Four(c.real))
        );
        c.feed(Input::FourIncomplete);
        assert_eq!(c.engine.curve_of(&FOUR_TOKEN), None);
    }

    /// A refused Four.meme log ends the Four.meme curves' tracking only; the
    /// vault leaving its trading phase leaves an open position unresolved;
    /// an unclassified launch is not tracked, with its reason.
    #[test]
    fn four_gaps_and_phases_concern_four_curves_only() {
        // A Flap launch decides as before past a refused Four.meme log.
        let mut flap = Chain::new(config("buy_count:3:,unique_buyers:3:"));
        flap.launch(TOKEN, 5_000_000_000_000_000);
        flap.feed(Input::FourIncomplete);
        for i in 1..=4u8 {
            flap.next_block();
            flap.buy(TOKEN, [i; 20], 20_000_000_000_000_000);
        }
        flap.next_block();
        assert_eq!(flap.kinds(), ["decided 105 Pass"]);
        // A Four.meme position open: the refused log leaves it unresolved.
        let mut c = FourChain::new(config("buy_count:3:,unique_buyers:3:"));
        c.passing_launch();
        c.next_block(); // 106: filled
        c.next_block();
        c.feed(Input::FourIncomplete);
        c.next_block();
        assert_eq!(
            c.kinds()[1..],
            ["entered 106".to_string(), "unresolved 107 Gap".to_string()]
        );
        let mut c = FourChain::new(config("buy_count:3:,unique_buyers:3:"));
        c.passing_launch();
        c.next_block();
        c.next_block();
        c.feed(Input::FourOffCurve { token: FOUR_TOKEN });
        c.next_block();
        assert_eq!(c.kinds().last().unwrap(), "unresolved 107 Migrated");
        let mut c = FourChain::new(config("buy_count:0:"));
        c.feed(Input::FourLaunch {
            token: FOUR_TOKEN,
            creator: CREATOR,
            tx_index: 1,
            curve: Err("preset"),
        });
        for _ in 0..5 {
            c.next_block();
        }
        assert_eq!(
            c.kinds(),
            ["decided 105 NotTracked(Unsupported(\"preset\"))"]
        );
    }
}

/// A6: the exit multiples' one check, at the configured places:
/// `0 <= stop < 1x < target`; the bounds themselves are refused.
#[test]
fn the_exit_multiples_need_stop_below_one_below_target() {
    let one = super::fixed::scale(config("").ratio_places).unwrap();
    let with = |target: i128, stop: i128| Config {
        target_net_multiple: target,
        stop_net_multiple: stop,
        ..config("")
    };
    assert_eq!(with(one + 1, one - 1).validate(), Ok(()));
    assert_eq!(with(one + 1, 0).validate(), Ok(()));
    assert_eq!(
        with(one + 1, one).validate(),
        Err(ConfigError::StopNotBelowOne)
    );
    assert_eq!(
        with(one + 1, -1).validate(),
        Err(ConfigError::StopNotBelowOne)
    );
    assert_eq!(with(one, 0).validate(), Err(ConfigError::TargetNotAboveOne));
    assert_eq!(
        Config {
            ratio_places: 39,
            ..with(one + 1, 0)
        }
        .validate(),
        Err(ConfigError::Places)
    );
}
