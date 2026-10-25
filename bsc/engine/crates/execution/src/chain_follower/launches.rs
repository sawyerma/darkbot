//! Launch detection from one block's verified receipts: the one table of
//! the venues, each identified by contract address AND event topic.
//!
//! Sources of the event layouts (all parameters non-indexed, so `topics`
//! is exactly `[topic0]` and the parameters are ABI words in `data`):
//! - Flap Portal `TokenCreated(uint256 ts, address creator, uint256 nonce,
//!   address token, string name, string symbol, string meta)` and
//!   `TokenBought(uint256 ts, address token, address buyer, uint256 amount,
//!   uint256 eth, uint256 fee, uint256 postPrice)`: the archived Portal
//!   interface
//!   (`prestudy/bsc/evidence/tests/legacy_gate0/gate0-evidence/2026-09-27-g01/sources/flap_launch.md`),
//!   the frozen IPortal ABI of
//!   `qualification/src/bin/g08_mainnet_verify.rs::portal_buys` and the
//!   gate-0 keccak comparison (`.../2026-09-27-g01/keccak_comparison.json`).
//! - Four.meme TokenManager2 `TokenCreate(address creator, address token,
//!   uint256 requestId, string name, string symbol, uint256 totalSupply,
//!   uint256 launchTime, uint256 launchFee)`: the archived ABI
//!   `.../2026-09-27-g01/sources/TokenManager2.lite.json`. Its buy event is
//!   not decoded here: whether every Four.meme buy is emitted as
//!   `TokenPurchase` is not proven (`bsc/tools/br0`, acceptance item 3).
//!
//! Every topic is recomputed from its signature in the tests.

use alloy_primitives::hex;

use super::CanonicalLaunch;
use super::jsonrpc::{BlockHead, Log, Receipt};

/// The venue of a launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Venue {
    /// Flap Portal on BSC.
    Flap,
    /// Four.meme TokenManager2 on BSC.
    Four,
}

/// The event of a venue that announces a new token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchEvent {
    pub venue: Venue,
    /// The emitting contract.
    pub contract: [u8; 20],
    /// The canonical event signature.
    pub signature: &'static str,
    /// Keccak-256 of `signature`.
    pub topic0: [u8; 32],
    /// The static head of `data`, in 32-byte words.
    pub head_words: usize,
    /// The word holding the creator address.
    pub creator_word: usize,
    /// The word holding the new token's address.
    pub token_word: usize,
    /// The venue's buy event, where its layout is proven.
    pub buy: Option<BuyEvent>,
}

/// The event of a venue that reports one buy of a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyEvent {
    pub signature: &'static str,
    pub topic0: [u8; 32],
    /// The exact length of `data`, in 32-byte words (no dynamic part).
    pub words: usize,
    /// The word holding the bought token's address.
    pub token_word: usize,
}

/// The Flap Portal (BSC).
pub const FLAP_PORTAL: [u8; 20] = hex!("e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0");
/// The Four.meme TokenManager2 (BSC).
pub const FOUR_TOKEN_MANAGER2: [u8; 20] = hex!("5c952063c7fc8610ffdb798152d69f0b9550762b");

/// The venues, in no particular order; a log matches at most one.
pub const LAUNCH_EVENTS: [LaunchEvent; 2] = [
    LaunchEvent {
        venue: Venue::Flap,
        contract: FLAP_PORTAL,
        signature: "TokenCreated(uint256,address,uint256,address,string,string,string)",
        topic0: hex!("504e7f360b2e5fe33cbaaae4c593bc55305328341bf79009e43e0e3b7f699603"),
        head_words: 7,
        creator_word: 1,
        token_word: 3,
        buy: Some(BuyEvent {
            signature: "TokenBought(uint256,address,address,uint256,uint256,uint256,uint256)",
            topic0: hex!("a800a2038683844fac66747f771bfdfae862eb28b16bcfa387afa9fbacce8ff7"),
            words: 7,
            token_word: 1,
        }),
    },
    LaunchEvent {
        venue: Venue::Four,
        contract: FOUR_TOKEN_MANAGER2,
        signature: "TokenCreate(address,address,uint256,string,string,uint256,uint256,uint256)",
        topic0: hex!("396d5e902b675b032348d3d2e9517ee8f0c4a926603fbc075d3d282ff00cad20"),
        head_words: 8,
        creator_word: 0,
        token_word: 1,
        buy: None,
    },
];

const WORD: usize = 32;

/// The launches of one block whose receipts were verified against `block`,
/// in receipt and log order. Failed receipts are skipped.
pub(super) fn detect(block: &BlockHead, receipts: &[Receipt]) -> Vec<CanonicalLaunch> {
    let mut launches = Vec::new();
    for receipt in receipts.iter().filter(|r| r.succeeded) {
        for log in &receipt.logs {
            let Some(event) = LAUNCH_EVENTS
                .iter()
                .find(|e| is_event(log, e.contract, e.topic0))
            else {
                continue;
            };
            let words =
                (log.topics.len() == 1 && log.data.len() >= event.head_words * WORD).then(|| {
                    (
                        address_word(&log.data, event.token_word),
                        address_word(&log.data, event.creator_word),
                    )
                });
            let (token, creator) = words.unwrap_or((None, None));
            let (external_buys_after, first_external_buy_index) = match (event.buy, token) {
                (Some(buy), Some(token)) => {
                    match buys_after(receipts, receipt.tx_index, event.contract, &buy, token) {
                        Some((count, first)) => (Some(count), first),
                        None => (None, None),
                    }
                }
                _ => (None, None),
            };
            launches.push(CanonicalLaunch {
                venue: event.venue,
                block_number: block.header.number,
                block_hash: block.header.hash,
                tx_index: receipt.tx_index,
                tx_hash: receipt.tx_hash,
                token,
                creator,
                external_buys_after,
                first_external_buy_index,
            });
        }
    }
    launches
}

/// The buy events of `token` in successful receipts after `launch_index`:
/// their count and the first one's transaction index. `None` when any buy
/// event of the venue there is malformed (the count would be a guess).
fn buys_after(
    receipts: &[Receipt],
    launch_index: u64,
    contract: [u8; 20],
    buy: &BuyEvent,
    token: [u8; 20],
) -> Option<(u32, Option<u64>)> {
    let mut count = 0u32;
    let mut first = None;
    for receipt in receipts
        .iter()
        .filter(|r| r.succeeded && r.tx_index > launch_index)
    {
        for log in receipt
            .logs
            .iter()
            .filter(|l| is_event(l, contract, buy.topic0))
        {
            if log.topics.len() != 1 || log.data.len() != buy.words * WORD {
                return None;
            }
            if address_word(&log.data, buy.token_word)? == token {
                count = count.checked_add(1)?;
                first.get_or_insert(receipt.tx_index);
            }
        }
    }
    Some((count, first))
}

fn is_event(log: &Log, contract: [u8; 20], topic0: [u8; 32]) -> bool {
    log.address == contract && log.topics.first() == Some(&topic0)
}

/// Word `index` of `data` as an address: twelve zero bytes, then twenty.
fn address_word(data: &[u8], index: usize) -> Option<[u8; 20]> {
    let word = data.get(index * WORD..(index + 1) * WORD)?;
    let (pad, address) = word.split_at(12);
    if pad.iter().any(|b| *b != 0) {
        return None;
    }
    address.try_into().ok()
}
