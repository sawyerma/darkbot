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
//! - Four.meme Classic TokenManager2 `TokenCreate` and OpenFour Core
//!   `TokenCreated` (OpenFour indexes presetId, creator and token): the one
//!   Four.meme decoder of Phase E (`trading::venues::fourmeme`, G0.1 Four
//!   freeze). Their buy events are not counted here: whether every
//!   Four.meme buy is emitted as `TokenPurchase`/`TradeExecuted` is not
//!   proven.
//!
//! Every topic is recomputed from its signature in the tests.

use alloy_primitives::{Address, hex};
use darkbot_bsc_trading::venues::flap::events::{
    PortalEvent, PortalEventKind, PortalLog, decode_portal_log,
};
use darkbot_bsc_trading::venues::fourmeme::deployment::bsc_mainnet_g01_129842711;
use darkbot_bsc_trading::venues::fourmeme::events::{
    FourEvent, FourEventKind, FourLog, decode_four_log,
};
use darkbot_bsc_trading::venues::fourmeme::recognize::PendingToken;

use super::jsonrpc::{BlockHead, Log, Receipt};
use super::{CanonicalEvent, CanonicalLaunch, CanonicalTerm, CanonicalTrade, FlapTerm};
use crate::ingress::{four_launch_token, is_flap_launch};
use crate::wss::{BSC_MAINNET_CHAIN_ID, ParsedTx, RejectReason, VerifiedTx, verify_parsed};

/// The venue of a launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Venue {
    /// Flap Portal on BSC.
    Flap,
    /// Four.meme Classic TokenManager2 on BSC.
    Four,
    /// Four.meme OpenFour Core on BSC.
    OpenFour,
}

/// What the production Four decoder makes of a Four.meme launch's verified
/// signed bytes before execution (Phase E); `NotApplicable` for Flap. The
/// runtime status counts every verdict, the journal's `C` record keeps it
/// (its last byte) and the census counts it per verdict; the first full
/// pending sighting of a Four launch is journaled with its signed bytes, so
/// a replay can decode it again (Phase E review L2, Phase F §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourToken {
    NotApplicable,
    /// Not a decoded launch call (another contract, a router, refused or
    /// unverifiable bytes).
    NotRecognized,
    /// A decoded launch without a proven derivation (an OpenFour preset
    /// outside the frozen table).
    Unknown,
    /// The token derived before execution is the canonical one.
    Matched,
    /// A derived token that is not the canonical one (another mode or
    /// template than the frozen ones).
    Mismatch,
    /// Paket 8 review M1: a Four Classic launch whose canonical token is
    /// one of the frozen pairs' candidates. Before execution the calldata
    /// names neither pair, so no token was derivable: never `Matched`,
    /// never actionable or C1 scope.
    Candidate,
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
    /// The venue's buy event, where its layout and completeness are proven.
    pub buy: Option<BuyEvent>,
}

/// The event of a venue that reports one buy of a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuyEvent {
    pub signature: &'static str,
    pub topic0: [u8; 32],
}

/// The Flap Portal (BSC).
pub const FLAP_PORTAL: [u8; 20] = hex!("e2ce6ab80874fa9fa2aae65d277dd6b8e65c9de0");
/// The Four.meme Classic TokenManager2 (BSC).
pub const FOUR_TOKEN_MANAGER2: [u8; 20] = hex!("5c952063c7fc8610ffdb798152d69f0b9550762b");
/// The Four.meme OpenFour Core (BSC).
pub const OPENFOUR_CORE: [u8; 20] = hex!("ebe7b6c1089d9f72ad07f34e36d898e44e5e27f3");

/// The venues, in no particular order; a log matches at most one.
pub const LAUNCH_EVENTS: [LaunchEvent; 3] = [
    LaunchEvent {
        venue: Venue::Flap,
        contract: FLAP_PORTAL,
        // The Flap entries come from the one decoder's table.
        signature: PortalEventKind::TokenCreated.signature(),
        topic0: PortalEventKind::TokenCreated.topic0(),
        buy: Some(BuyEvent {
            signature: PortalEventKind::TokenBought.signature(),
            topic0: PortalEventKind::TokenBought.topic0(),
        }),
    },
    // The Four.meme entries come from the one Four decoder's table.
    LaunchEvent {
        venue: Venue::Four,
        contract: FOUR_TOKEN_MANAGER2,
        signature: FourEventKind::TokenCreate.signature(),
        topic0: FourEventKind::TokenCreate.topic0(),
        buy: None,
    },
    LaunchEvent {
        venue: Venue::OpenFour,
        contract: OPENFOUR_CORE,
        signature: FourEventKind::TokenCreated.signature(),
        topic0: FourEventKind::TokenCreated.topic0(),
        buy: None,
    },
];

/// A launch as its block's receipts show it; [`CanonicalLaunch`] once its
/// transaction is verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReceiptLaunch {
    pub(super) venue: Venue,
    pub(super) block_number: u64,
    pub(super) block_hash: [u8; 32],
    pub(super) tx_index: u64,
    pub(super) tx_hash: [u8; 32],
    pub(super) token: Option<[u8; 20]>,
    pub(super) creator: Option<[u8; 20]>,
    pub(super) external_buys_after: Option<u32>,
    pub(super) first_external_buy_index: Option<u64>,
}

/// The WSS verifier's result for one transaction object of a block (for
/// BSC Mainnet), or why the object could not be converted.
pub(super) fn verify_object(
    object: &Result<ParsedTx, RejectReason>,
) -> Result<VerifiedTx, RejectReason> {
    match object {
        Ok(parsed) => verify_parsed(parsed, BSC_MAINNET_CHAIN_ID),
        Err(reason) => Err(*reason),
    }
}

impl ReceiptLaunch {
    /// The launch with the facts of its verified transaction: sender,
    /// nonce, and the scope decision of the pending path on the signed
    /// bytes (Flap: `ingress::is_flap_launch`, the C1 scope; Four.meme:
    /// `ingress::four_launch_token` names this launch's token before
    /// execution); or, for a transaction the verifier refuses, that reason.
    pub(super) fn with_transaction(
        self,
        tx: Result<&VerifiedTx, RejectReason>,
        max_launch_calldata: usize,
    ) -> CanonicalLaunch {
        let four = bsc_mainnet_g01_129842711();
        let four_token = match (self.venue, tx) {
            (Venue::Flap, _) => FourToken::NotApplicable,
            (_, Err(_)) => FourToken::NotRecognized,
            (_, Ok(t)) => match four_launch_token(&four, &t.raw, max_launch_calldata) {
                None => FourToken::NotRecognized,
                Some(PendingToken::Unknown) => FourToken::Unknown,
                Some(PendingToken::Derived(token)) => {
                    if Some(token.into_array()) == self.token {
                        FourToken::Matched
                    } else {
                        FourToken::Mismatch
                    }
                }
                // Paket 8 review H1/M1: a Classic launch's token is one of
                // the frozen pairs' candidates (none of them: another pair);
                // a candidate, not a token derived before execution.
                Some(PendingToken::Candidates(tokens)) => {
                    if tokens.iter().any(|t| Some(t.into_array()) == self.token) {
                        FourToken::Candidate
                    } else {
                        FourToken::Mismatch
                    }
                }
            },
        };
        CanonicalLaunch {
            venue: self.venue,
            block_number: self.block_number,
            block_hash: self.block_hash,
            tx_index: self.tx_index,
            tx_hash: self.tx_hash,
            token: self.token,
            creator: self.creator,
            external_buys_after: self.external_buys_after,
            first_external_buy_index: self.first_external_buy_index,
            sender: tx.ok().map(|t| t.from),
            nonce: tx.ok().map(|t| t.nonce),
            c1_scope: match self.venue {
                Venue::Flap => tx.is_ok_and(|t| is_flap_launch(&t.raw, max_launch_calldata)),
                Venue::Four | Venue::OpenFour => four_token == FourToken::Matched,
            },
            venue_target: tx.is_ok_and(|t| crate::ingress::venue_target(&four, &t.raw)),
            four_token,
            unverifiable: tx.err(),
        }
    }
}

/// The reverted launch calls of one block: failed receipts whose
/// transaction object verifies and whose signed bytes are a launch to the
/// production decoders (`ingress::launch_venue`, the pending path's own
/// decision); an unverifiable failed transaction is not guessed.
pub(super) fn reverted(
    receipts: &[Receipt],
    objects: &[Result<ParsedTx, RejectReason>],
    max_launch_calldata: usize,
) -> Vec<(Venue, u64, [u8; 32])> {
    let four = bsc_mainnet_g01_129842711();
    receipts
        .iter()
        .filter(|r| !r.succeeded)
        .filter_map(|r| {
            let object = objects.get(usize::try_from(r.tx_index).ok()?)?;
            let tx = verify_object(object).ok()?;
            let venue = crate::ingress::launch_venue(&four, &tx.raw, max_launch_calldata)?;
            Some((venue, r.tx_index, r.tx_hash))
        })
        .collect()
}

/// The launches of one block whose receipts were verified against `block`,
/// in receipt and log order. Failed receipts are skipped (see [`reverted`]).
pub(super) fn detect(block: &BlockHead, receipts: &[Receipt]) -> Vec<ReceiptLaunch> {
    let mut launches = Vec::new();
    for receipt in receipts.iter().filter(|r| r.succeeded) {
        for log in &receipt.logs {
            let Some(event) = LAUNCH_EVENTS
                .iter()
                .find(|e| is_event(log, e.contract, e.topic0))
            else {
                continue;
            };
            let (token, creator) = match event.venue {
                // Phase D: the one Flap decoder (strict canonical layout).
                Venue::Flap => match flap_log(log) {
                    PortalLog::Event(PortalEvent::TokenCreated { token, creator, .. }) => {
                        (Some(token.into_array()), Some(creator.into_array()))
                    }
                    _ => (None, None),
                },
                // Phase E: the one Four.meme decoder (strict canonical layout).
                Venue::Four | Venue::OpenFour => match four_log(log) {
                    FourLog::Event(FourEvent::TokenCreate(c)) => {
                        (Some(c.token.into_array()), Some(c.creator.into_array()))
                    }
                    FourLog::Event(FourEvent::TokenCreated(c)) => {
                        (Some(c.token.into_array()), Some(c.creator.into_array()))
                    }
                    _ => (None, None),
                },
            };
            let (external_buys_after, first_external_buy_index) = match (event.buy, token) {
                (Some(buy), Some(token)) => {
                    match buys_after(receipts, receipt.tx_index, &buy, token) {
                        Some((count, first)) => (Some(count), first),
                        None => (None, None),
                    }
                }
                _ => (None, None),
            };
            launches.push(ReceiptLaunch {
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

/// The Flap Portal buy events of `token` in successful receipts after
/// `launch_index` (only Flap has a proven buy event): their count and the
/// first one's transaction index. `None` when any buy event there is
/// malformed (the count would be a guess).
fn buys_after(
    receipts: &[Receipt],
    launch_index: u64,
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
            .filter(|l| is_event(l, FLAP_PORTAL, buy.topic0))
        {
            // Phase D: the one Flap decoder; a malformed buy voids the count.
            let bought = match flap_log(log) {
                PortalLog::Event(PortalEvent::TokenBought(trade)) => trade.token.into_array(),
                _ => return None,
            };
            if bought == token {
                count = count.checked_add(1)?;
                first.get_or_insert(receipt.tx_index);
            }
        }
    }
    Some((count, first))
}

/// Two events of the live Portal outside the verified v5.27.0 ABI (declared
/// signatures, as `g02_flap_parity` reads them; topics recomputed in the
/// tests) that restrict who may buy a token: an NFT gate and a per-origin
/// holding cap. Both put the token outside Strategy C's priced scope; a
/// log of either topic with another layout is refused.
pub(super) const FLAP_NFT_GATED: [u8; 32] =
    hex!("2179156b3d2ade0f0de96eff28a7c29d924002784e113e69108d91a02cbf9098");
pub(super) const FLAP_MAX_BUY_PER_ORIGIN_SET: [u8; 32] =
    hex!("afcebb99a3d0047f96f0d1cc8872781aaeef86f4f5e7e1c6a33e5907d6a3ed82");

/// The restriction term of one log of the two topics above, `None` when its
/// layout differs: `FlapNFTGated` has its token and NFT as data words,
/// `FlapTokenMaxBuyPerOriginSet` its token indexed and bps and amount as
/// data words.
fn restriction(log: &Log) -> Option<([u8; 20], FlapTerm)> {
    let address = |w: &[u8]| -> Option<[u8; 20]> {
        w[..12]
            .iter()
            .all(|b| *b == 0)
            .then(|| w[12..32].try_into().ok())?
    };
    match log.topics.first()? {
        t if *t == FLAP_NFT_GATED && log.topics.len() == 1 && log.data.len() == 64 => Some((
            address(&log.data[..32])?,
            FlapTerm::NftGate(address(&log.data[32..])?),
        )),
        t if *t == FLAP_MAX_BUY_PER_ORIGIN_SET
            && log.topics.len() == 2
            && log.data.len() == 64
            && log.data[..30].iter().all(|b| *b == 0) =>
        {
            Some((
                address(&log.topics[1])?,
                FlapTerm::HoldingCap {
                    bps: u16::from_be_bytes([log.data[30], log.data[31]]),
                    max_amount: log.data[32..].try_into().ok()?,
                },
            ))
        }
        _ => None,
    }
}

/// The Portal trades, token terms and migrations of the block's successful
/// receipts, in receipt and log order (Phase H); a log of these kinds the
/// decoder refuses is reported, not skipped silently.
pub(super) fn flap_events(block: &BlockHead, receipts: &[Receipt]) -> Vec<CanonicalEvent> {
    use PortalEventKind as K;
    let kinds = [
        K::TokenBought,
        K::TokenSold,
        K::TokenVersionSet,
        K::TokenQuoteSet,
        K::TokenCurveSetV2,
        K::TokenDexSupplyThreshSet,
        K::TokenMigratorSet,
        K::FlapTokenTaxSet,
        K::FlapTokenAsymmetricTaxSet,
    ];
    let (number, hash) = (block.header.number, block.header.hash);
    let mut out = Vec::new();
    for receipt in receipts.iter().filter(|r| r.succeeded) {
        for log in receipt.logs.iter().filter(|l| l.address == FLAP_PORTAL) {
            let refused = CanonicalEvent::FlapTradeUndecodable {
                block_number: number,
                block_hash: hash,
                tx_index: receipt.tx_index,
                log_index: log.log_index,
            };
            if log
                .topics
                .first()
                .is_some_and(|t| *t == FLAP_NFT_GATED || *t == FLAP_MAX_BUY_PER_ORIGIN_SET)
            {
                out.push(match restriction(log) {
                    Some((token, term)) => CanonicalEvent::FlapTerm(CanonicalTerm {
                        block_number: number,
                        block_hash: hash,
                        tx_index: receipt.tx_index,
                        log_index: log.log_index,
                        token,
                        term,
                    }),
                    None => refused,
                });
                continue;
            }
            let Some(kind) = log.topics.first().and_then(PortalEventKind::from_topic0) else {
                continue;
            };
            if kind == K::LaunchedToDex {
                // Unwitnessed, so not decoded by the Phase D decoder: the
                // verified ABI's first data word is the token (no Portal
                // event has an indexed parameter), checked to be an address.
                let token = (log.topics.len() == 1 && log.data.len() == 4 * 32)
                    .then(|| &log.data[..32])
                    .filter(|w| w[..12].iter().all(|b| *b == 0))
                    .and_then(|w| <[u8; 20]>::try_from(&w[12..]).ok());
                out.push(match token {
                    Some(token) => CanonicalEvent::FlapMigrated {
                        block_number: number,
                        block_hash: hash,
                        tx_index: receipt.tx_index,
                        log_index: log.log_index,
                        token,
                    },
                    None => refused,
                });
                continue;
            }
            if !kinds.contains(&kind) {
                continue;
            }
            let term = |token: Address, term| {
                CanonicalEvent::FlapTerm(CanonicalTerm {
                    block_number: number,
                    block_hash: hash,
                    tx_index: receipt.tx_index,
                    log_index: log.log_index,
                    token: token.into_array(),
                    term,
                })
            };
            out.push(match flap_log(log) {
                PortalLog::Event(PortalEvent::TokenBought(t) | PortalEvent::TokenSold(t)) => {
                    CanonicalEvent::FlapTrade(CanonicalTrade {
                        block_number: number,
                        block_hash: hash,
                        tx_index: receipt.tx_index,
                        tx_hash: receipt.tx_hash,
                        log_index: log.log_index,
                        origin: receipt.from,
                        sell: kind == K::TokenSold,
                        token: t.token.into_array(),
                        trader: t.trader.into_array(),
                        amount: t.amount.to_be_bytes(),
                        eth: t.eth.to_be_bytes(),
                        fee: t.fee.to_be_bytes(),
                        post_price: t.post_price.to_be_bytes(),
                    })
                }
                PortalLog::Event(PortalEvent::TokenVersionSet { token, version }) => {
                    term(token, FlapTerm::Version(version))
                }
                PortalLog::Event(PortalEvent::TokenQuoteSet { token, quote_token }) => {
                    term(token, FlapTerm::QuoteToken(quote_token.into_array()))
                }
                PortalLog::Event(PortalEvent::TokenCurveSetV2 { token, r, h, k }) => term(
                    token,
                    FlapTerm::Curve {
                        r: r.to_be_bytes(),
                        h: h.to_be_bytes(),
                        k: k.to_be_bytes(),
                    },
                ),
                PortalLog::Event(PortalEvent::TokenDexSupplyThreshSet {
                    token,
                    dex_supply_thresh,
                }) => term(
                    token,
                    FlapTerm::DexThreshold(dex_supply_thresh.to_be_bytes()),
                ),
                PortalLog::Event(PortalEvent::TokenMigratorSet {
                    token,
                    migrator_type,
                }) => term(token, FlapTerm::Migrator(migrator_type)),
                PortalLog::Event(PortalEvent::FlapTokenTaxSet { token, tax }) => term(
                    token,
                    FlapTerm::Tax {
                        buy: tax.to_be_bytes(),
                        sell: tax.to_be_bytes(),
                    },
                ),
                PortalLog::Event(PortalEvent::FlapTokenAsymmetricTaxSet {
                    token,
                    buy_tax,
                    sell_tax,
                }) => term(
                    token,
                    FlapTerm::Tax {
                        buy: buy_tax.to_be_bytes(),
                        sell: sell_tax.to_be_bytes(),
                    },
                ),
                _ => refused,
            });
        }
    }
    out
}

/// One log through the production Flap decoder (`trading::venues::flap`).
fn flap_log(log: &Log) -> PortalLog<'_> {
    decode_portal_log(
        Address::from(FLAP_PORTAL),
        Address::from(log.address),
        &log.topics,
        &log.data,
    )
}

/// One log through the production Four.meme decoder
/// (`trading::venues::fourmeme`, the frozen deployment).
fn four_log(log: &Log) -> FourLog<'_> {
    decode_four_log(
        &bsc_mainnet_g01_129842711(),
        Address::from(log.address),
        &log.topics,
        &log.data,
    )
}

fn is_event(log: &Log, contract: [u8; 20], topic0: [u8; 32]) -> bool {
    log.address == contract && log.topics.first() == Some(&topic0)
}
