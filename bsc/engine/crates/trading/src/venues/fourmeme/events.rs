//! Canonical decode of the Four.meme events (Phase E contract §6).
//!
//! Decoded are the events with a witnessed live log in the receipts of the
//! freezes' fixtures: Classic `TokenCreate`, `TokenPurchase` and
//! `TokenSale`, OpenFour `TokenCreated`, `TradeExecuted` (BUY and SELL
//! form) and `PhaseTransition`. The other events of the published ABIs
//! (Classic `LiquidityAdded`, `TradeStop`; OpenFour `MigrateExecuted`) have
//! no witnessed log (freeze v2, 20 000 blocks): they are
//! [`FourLog::Unsupported`], so a graduation or a migration is not observed
//! from events until a witnessed fixture exists.
//!
//! Classic events index nothing (a log carries exactly its `topic0`);
//! OpenFour events index the parameters the published ABI marks `indexed`,
//! in order, as topics. The topic constants are Keccak-256 of the
//! signatures, recomputed with alloy in the tests and compared with the
//! freeze evidence.
//!
//! Decoding is fail-closed and copies nothing: a log of another contract is
//! [`FourLog::NotFour`], an unknown topic [`FourLog::Unrecognized`], a known
//! topic with any other layout [`FourLog::Malformed`]. [`decode_receipt_logs`]
//! refuses a failed receipt so that a revert never reads as a launch or a
//! trade.

use alloy_primitives::{Address, B256, U256, hex};

use super::abi::{AbiError, Words, address_word, exact, tails};
use super::deployment::{Family, FourDeployment};

/// A Four.meme event of a published ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourEventKind {
    TokenCreate,
    TokenPurchase,
    TokenSale,
    LiquidityAdded,
    TradeStop,
    TokenCreated,
    TradeExecuted,
    PhaseTransition,
    MigrateExecuted,
}

impl FourEventKind {
    pub const ALL: [FourEventKind; 9] = [
        Self::TokenCreate,
        Self::TokenPurchase,
        Self::TokenSale,
        Self::LiquidityAdded,
        Self::TradeStop,
        Self::TokenCreated,
        Self::TradeExecuted,
        Self::PhaseTransition,
        Self::MigrateExecuted,
    ];

    pub const fn family(self) -> Family {
        match self {
            Self::TokenCreate
            | Self::TokenPurchase
            | Self::TokenSale
            | Self::LiquidityAdded
            | Self::TradeStop => Family::Classic,
            _ => Family::OpenFour,
        }
    }

    /// The canonical event signature (published ABI).
    pub const fn signature(self) -> &'static str {
        match self {
            Self::TokenCreate => {
                "TokenCreate(address,address,uint256,string,string,uint256,uint256,uint256)"
            }
            Self::TokenPurchase => {
                "TokenPurchase(address,address,uint256,uint256,uint256,uint256,uint256,uint256)"
            }
            Self::TokenSale => {
                "TokenSale(address,address,uint256,uint256,uint256,uint256,uint256,uint256)"
            }
            Self::LiquidityAdded => "LiquidityAdded(address,uint256,address,uint256)",
            Self::TradeStop => "TradeStop(address)",
            Self::TokenCreated => {
                "TokenCreated(uint256,uint256,address,address,string,string,uint256,uint256,uint256,uint256,address,address,address,address,address,address,address,string,uint256,bytes)"
            }
            Self::TradeExecuted => {
                "TradeExecuted(address,address,uint256,bool,address,address,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256,uint256)"
            }
            Self::PhaseTransition => "PhaseTransition(address,uint8,uint8,address)",
            Self::MigrateExecuted => {
                "MigrateExecuted(uint256,address,address,address,address,bytes8,uint256,uint256,bytes32,uint8,bytes)"
            }
        }
    }

    /// Keccak-256 of [`Self::signature`].
    pub const fn topic0(self) -> [u8; 32] {
        match self {
            Self::TokenCreate => {
                hex!("396d5e902b675b032348d3d2e9517ee8f0c4a926603fbc075d3d282ff00cad20")
            }
            Self::TokenPurchase => {
                hex!("7db52723a3b2cdd6164364b3b766e65e540d7be48ffa89582956d8eaebe62942")
            }
            Self::TokenSale => {
                hex!("0a5575b3648bae2210cee56bf33254cc1ddfbc7bf637c0af2ac18b14fb1bae19")
            }
            Self::LiquidityAdded => {
                hex!("c18aa71171b358b706fe3dd345299685ba21a5316c66ffa9e319268b033c44b0")
            }
            Self::TradeStop => {
                hex!("8f9ab4bd7eff0d085f91575d50cd83f97aa5258e24ded7630d4fd6739e857132")
            }
            Self::TokenCreated => {
                hex!("7dae4dcd1b20eb0445e7cc1d02b3e2e7f32fa63081a0e764732c269f2b8f228f")
            }
            Self::TradeExecuted => {
                hex!("8728ba77a8dc0cbf185f23fe65b61fc658a37975e027062c3114394222a10f3f")
            }
            Self::PhaseTransition => {
                hex!("ba80aa59bb279da81e47a24a4afc9d5f15204d3b1fed9b7e1d2680c5b54b22f5")
            }
            Self::MigrateExecuted => {
                hex!("147a4684ffc452d571b9faa729585d977e7c7caf3f0eb09bcc430ca605e0f8ff")
            }
        }
    }

    /// Topics of a log of this event: `topic0` and one per indexed
    /// parameter.
    pub const fn topics(self) -> usize {
        match self {
            Self::TokenCreated | Self::TradeExecuted | Self::MigrateExecuted => 4,
            Self::PhaseTransition => 2,
            _ => 1,
        }
    }

    /// Whether a live log of this event is in the fixtures' receipts (its
    /// layout is then witnessed, not only declared).
    pub const fn witnessed(self) -> bool {
        matches!(
            self,
            Self::TokenCreate
                | Self::TokenPurchase
                | Self::TokenSale
                | Self::TokenCreated
                | Self::TradeExecuted
                | Self::PhaseTransition
        )
    }

    pub fn from_topic0(family: Family, topic0: &[u8; 32]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| k.family() == family && k.topic0() == *topic0)
    }
}

/// A Classic `TokenCreate`: the published ABI's names. Creator, token,
/// request id, name and symbol are checked against the launch; the other
/// values are facts of the log with UNKNOWN economic meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicCreate<'a> {
    pub creator: Address,
    pub token: Address,
    pub request_id: U256,
    pub name: &'a [u8],
    pub symbol: &'a [u8],
    pub total_supply: U256,
    pub launch_time: U256,
    pub launch_fee: U256,
}

/// A Classic `TokenPurchase` or `TokenSale` (one layout; published ABI
/// names; token and account checked against the BUY or SELL, amounts
/// UNKNOWN until Phase G).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicTrade {
    pub token: Address,
    pub account: Address,
    pub price: U256,
    pub amount: U256,
    pub cost: U256,
    pub fee: U256,
    pub offers: U256,
    pub funds: U256,
}

/// An OpenFour `TokenCreated` (published ABI names; ids, creator, token,
/// name, symbol, supply/sale/raise, quote asset and meta URI checked
/// against the launch, the module addresses and the rest are facts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourCreated<'a> {
    pub request_id: U256,
    pub preset_id: U256,
    pub creator: Address,
    pub token: Address,
    pub name: &'a [u8],
    pub symbol: &'a [u8],
    pub max_supply: U256,
    pub sale_amount: U256,
    pub raise_amount: U256,
    pub initial_price: U256,
    pub quote_asset: Address,
    pub vault: Address,
    pub curve_module: Address,
    pub trade_module: Address,
    pub migrate_module: Address,
    pub custom_data: Address,
    pub token_module: Address,
    pub token_meta_uri: &'a [u8],
    pub flags: U256,
    pub encoded_tags: &'a [u8],
}

/// An OpenFour `TradeExecuted`, BUY or SELL form by `isBuy` (published
/// ABI names; token and trader checked against the BUY or SELL, amounts and
/// fees UNKNOWN until Phase G).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourTrade {
    pub token: Address,
    pub trader: Address,
    pub preset_id: U256,
    pub quote_asset: Address,
    pub vault: Address,
    pub requested_token_amount: U256,
    pub token_amount: U256,
    pub curve_quote_amount: U256,
    pub trader_quote_amount: U256,
    pub last_price: U256,
    pub slippage_limit: U256,
    pub protocol_fee: U256,
    pub tax_fee: U256,
    pub dev_fee: U256,
    pub anti_sniper_fee: U256,
    pub total_raised: U256,
    pub remaining_for_sale: U256,
}

/// A decoded Four.meme event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourEvent<'a> {
    TokenCreate(ClassicCreate<'a>),
    TokenPurchase(ClassicTrade),
    TokenSale(ClassicTrade),
    TokenCreated(OpenFourCreated<'a>),
    TradeExecutedBuy(OpenFourTrade),
    TradeExecutedSell(OpenFourTrade),
    /// An OpenFour phase change of `token`; the meaning of the phase
    /// numbers is UNKNOWN (no mode is inferred from them).
    PhaseTransition {
        token: Address,
        from: u8,
        to: u8,
        operator: Address,
    },
}

impl FourEvent<'_> {
    pub fn kind(&self) -> FourEventKind {
        match self {
            Self::TokenCreate(_) => FourEventKind::TokenCreate,
            Self::TokenPurchase(_) => FourEventKind::TokenPurchase,
            Self::TokenSale(_) => FourEventKind::TokenSale,
            Self::TokenCreated(_) => FourEventKind::TokenCreated,
            Self::TradeExecutedBuy(_) | Self::TradeExecutedSell(_) => FourEventKind::TradeExecuted,
            Self::PhaseTransition { .. } => FourEventKind::PhaseTransition,
        }
    }

    /// The token a launch event creates.
    pub fn created_token(&self) -> Option<Address> {
        match self {
            Self::TokenCreate(c) => Some(c.token),
            Self::TokenCreated(c) => Some(c.token),
            _ => None,
        }
    }
}

/// What one log is to this decoder.
// Returned by value per log: boxing the event would allocate (contract §5
// forbids allocation in the decoder); the size is fixed.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourLog<'a> {
    /// Emitted by another contract.
    NotFour,
    Event(FourEvent<'a>),
    /// A topic of a Four contract this decoder does not know (counted,
    /// never guessed).
    Unrecognized(Family, B256),
    /// A known topic with another layout: refused.
    Malformed(FourEventKind, AbiError),
    /// A topic of a published ABI without a witnessed live log:
    /// recognized, not decoded.
    Unsupported(FourEventKind),
}

/// Why a receipt's logs were not decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptError {
    /// The transaction reverted: it has no effects to decode.
    Reverted,
}

fn decode_kind<'a>(
    kind: FourEventKind,
    topics: &[[u8; 32]],
    data: &'a [u8],
) -> Result<FourLog<'a>, AbiError> {
    use FourEventKind as K;
    let indexed = |i: usize| address_word(&topics[i]);
    let event = match kind {
        K::TokenCreate => {
            let [name, symbol] = tails(data, 8, [3, 4])?;
            let w = Words(data);
            FourEvent::TokenCreate(ClassicCreate {
                creator: w.address(0)?,
                token: w.address(1)?,
                request_id: w.u256(2)?,
                name,
                symbol,
                total_supply: w.u256(5)?,
                launch_time: w.u256(6)?,
                launch_fee: w.u256(7)?,
            })
        }
        K::TokenPurchase | K::TokenSale => {
            let w = exact(data, 8)?;
            let trade = ClassicTrade {
                token: w.address(0)?,
                account: w.address(1)?,
                price: w.u256(2)?,
                amount: w.u256(3)?,
                cost: w.u256(4)?,
                fee: w.u256(5)?,
                offers: w.u256(6)?,
                funds: w.u256(7)?,
            };
            if kind == K::TokenSale {
                FourEvent::TokenSale(trade)
            } else {
                FourEvent::TokenPurchase(trade)
            }
        }
        K::TokenCreated => {
            let [name, symbol, token_meta_uri, encoded_tags] = tails(data, 17, [1, 2, 14, 16])?;
            let w = Words(data);
            FourEvent::TokenCreated(OpenFourCreated {
                request_id: w.u256(0)?,
                preset_id: U256::from_be_bytes(topics[1]),
                creator: indexed(2)?,
                token: indexed(3)?,
                name,
                symbol,
                max_supply: w.u256(3)?,
                sale_amount: w.u256(4)?,
                raise_amount: w.u256(5)?,
                initial_price: w.u256(6)?,
                quote_asset: w.address(7)?,
                vault: w.address(8)?,
                curve_module: w.address(9)?,
                trade_module: w.address(10)?,
                migrate_module: w.address(11)?,
                custom_data: w.address(12)?,
                token_module: w.address(13)?,
                token_meta_uri,
                flags: w.u256(15)?,
                encoded_tags,
            })
        }
        K::TradeExecuted => {
            let w = exact(data, 15)?;
            let trade = OpenFourTrade {
                token: indexed(1)?,
                trader: indexed(2)?,
                preset_id: U256::from_be_bytes(topics[3]),
                quote_asset: w.address(1)?,
                vault: w.address(2)?,
                requested_token_amount: w.u256(3)?,
                token_amount: w.u256(4)?,
                curve_quote_amount: w.u256(5)?,
                trader_quote_amount: w.u256(6)?,
                last_price: w.u256(7)?,
                slippage_limit: w.u256(8)?,
                protocol_fee: w.u256(9)?,
                tax_fee: w.u256(10)?,
                dev_fee: w.u256(11)?,
                anti_sniper_fee: w.u256(12)?,
                total_raised: w.u256(13)?,
                remaining_for_sale: w.u256(14)?,
            };
            if w.bool(0)? {
                FourEvent::TradeExecutedBuy(trade)
            } else {
                FourEvent::TradeExecutedSell(trade)
            }
        }
        K::PhaseTransition => {
            let w = exact(data, 3)?;
            FourEvent::PhaseTransition {
                token: indexed(1)?,
                from: w.uint8(0)?,
                to: w.uint8(1)?,
                operator: w.address(2)?,
            }
        }
        // Recognized by topic only; `decode_four_log` reports them
        // unsupported before any decode.
        K::LiquidityAdded | K::TradeStop | K::MigrateExecuted => {
            return Ok(FourLog::Unsupported(kind));
        }
    };
    Ok(FourLog::Event(event))
}

/// Decodes one log of a successful receipt against the frozen
/// `deployment`.
pub fn decode_four_log<'a>(
    deployment: &FourDeployment,
    emitter: Address,
    topics: &[[u8; 32]],
    data: &'a [u8],
) -> FourLog<'a> {
    let Some(family) = deployment.family_of(emitter) else {
        return FourLog::NotFour;
    };
    let Some(topic0) = topics.first() else {
        return FourLog::Unrecognized(family, B256::ZERO);
    };
    let Some(kind) = FourEventKind::from_topic0(family, topic0) else {
        return FourLog::Unrecognized(family, B256::from(*topic0));
    };
    if !kind.witnessed() {
        return FourLog::Unsupported(kind);
    }
    if topics.len() != kind.topics() {
        return FourLog::Malformed(kind, AbiError::NonCanonical);
    }
    decode_kind(kind, topics, data).unwrap_or_else(|e| FourLog::Malformed(kind, e))
}

/// The Four.meme logs of one receipt, decoded in log order; a reverted
/// receipt is refused (it has no effects).
pub fn decode_receipt_logs<'a, 'd, L>(
    deployment: &'d FourDeployment,
    succeeded: bool,
    logs: L,
) -> Result<impl Iterator<Item = FourLog<'a>> + 'd, ReceiptError>
where
    L: IntoIterator<Item = (Address, &'a [[u8; 32]], &'a [u8])>,
    L::IntoIter: 'd,
    'a: 'd,
{
    if !succeeded {
        return Err(ReceiptError::Reverted);
    }
    Ok(logs
        .into_iter()
        .map(move |(emitter, topics, data)| decode_four_log(deployment, emitter, topics, data)))
}
