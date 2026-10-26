//! Canonical decode of the Flap Portal's events (Phase D contract §7).
//!
//! Every signature below is in the Sourcify-verified ABI of the Portal
//! implementation `0x6a0e41d0…` (v5.27.0) and in both archived IPortal
//! sources (G0.1 Flap refreeze, `bsc/native/evidence/g01/flap_freeze_*`).
//! All parameters are non-indexed: a Portal log carries exactly its
//! `topic0`, and `data` is the canonical ABI encoding of the parameters.
//! The topic constants are Keccak-256 of the signatures, recomputed with
//! alloy in the tests and compared with the freeze evidence.
//!
//! Decoding is fail-closed and copies nothing: a log of another contract
//! is [`PortalLog::NotPortal`], an unknown topic is
//! [`PortalLog::Unrecognized`], a known topic with any other layout is
//! [`PortalLog::Malformed`]. Logs exist only in successful receipts;
//! [`decode_receipt_logs`] refuses a failed receipt so that a revert can
//! never read as a launch or a trade.

use alloy_primitives::{Address, B256, U256, hex};

use super::abi::AbiError;

const WORD: usize = 32;

/// A Portal event this decoder knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalEventKind {
    TokenCreated,
    TokenBought,
    TokenSold,
    LaunchedToDex,
    TokenVersionSet,
    TokenQuoteSet,
    TokenCurveSet,
    TokenCurveSetV2,
    TokenDexSupplyThreshSet,
    TokenMigratorSet,
    FlapTokenTaxSet,
    FlapTokenAsymmetricTaxSet,
    VanityTokenCreated,
    FlapTokenStaged,
}

impl PortalEventKind {
    pub const ALL: [PortalEventKind; 14] = [
        Self::TokenCreated,
        Self::TokenBought,
        Self::TokenSold,
        Self::LaunchedToDex,
        Self::TokenVersionSet,
        Self::TokenQuoteSet,
        Self::TokenCurveSet,
        Self::TokenCurveSetV2,
        Self::TokenDexSupplyThreshSet,
        Self::TokenMigratorSet,
        Self::FlapTokenTaxSet,
        Self::FlapTokenAsymmetricTaxSet,
        Self::VanityTokenCreated,
        Self::FlapTokenStaged,
    ];

    /// The canonical event signature (verified ABI).
    pub const fn signature(self) -> &'static str {
        match self {
            Self::TokenCreated => {
                "TokenCreated(uint256,address,uint256,address,string,string,string)"
            }
            Self::TokenBought => {
                "TokenBought(uint256,address,address,uint256,uint256,uint256,uint256)"
            }
            Self::TokenSold => "TokenSold(uint256,address,address,uint256,uint256,uint256,uint256)",
            Self::LaunchedToDex => "LaunchedToDEX(address,address,uint256,uint256)",
            Self::TokenVersionSet => "TokenVersionSet(address,uint8)",
            Self::TokenQuoteSet => "TokenQuoteSet(address,address)",
            Self::TokenCurveSet => "TokenCurveSet(address,address,uint256)",
            Self::TokenCurveSetV2 => "TokenCurveSetV2(address,uint256,uint256,uint256)",
            Self::TokenDexSupplyThreshSet => "TokenDexSupplyThreshSet(address,uint256)",
            Self::TokenMigratorSet => "TokenMigratorSet(address,uint8)",
            Self::FlapTokenTaxSet => "FlapTokenTaxSet(address,uint256)",
            Self::FlapTokenAsymmetricTaxSet => "FlapTokenAsymmetricTaxSet(address,uint256,uint256)",
            Self::VanityTokenCreated => "VanityTokenCreated(address,address,address)",
            Self::FlapTokenStaged => "FlapTokenStaged(uint256,address,address)",
        }
    }

    /// Keccak-256 of [`Self::signature`] (checked in the tests and against
    /// the freeze evidence).
    pub const fn topic0(self) -> [u8; 32] {
        match self {
            Self::TokenCreated => {
                hex!("504e7f360b2e5fe33cbaaae4c593bc55305328341bf79009e43e0e3b7f699603")
            }
            Self::TokenBought => {
                hex!("a800a2038683844fac66747f771bfdfae862eb28b16bcfa387afa9fbacce8ff7")
            }
            Self::TokenSold => {
                hex!("03a4693e592f5e75dc7c136acb39b146d2b4966c0e509c34f362dee02b3b861a")
            }
            Self::LaunchedToDex => {
                hex!("6e4f47630b8745b8cacbd44f42a8a33e7eea7cc08ef22fc7630f4f385784ff7d")
            }
            Self::TokenVersionSet => {
                hex!("37502bf23c59a12e1036e7580a8dc056803623dbcd0885abb882adfa069ac89e")
            }
            Self::TokenQuoteSet => {
                hex!("3ceb902d3c555c21c3415b6aa839104b18e4825b2f8324011ff979089a507a8c")
            }
            Self::TokenCurveSet => {
                hex!("da7793e72bf76b338906a98bfb58526d202873ecaf7a5894663053ee69069ce1")
            }
            Self::TokenCurveSetV2 => {
                hex!("71a10912a55f73d3cced0d1515c2b33c396c80342522bad0e295ccbede556f37")
            }
            Self::TokenDexSupplyThreshSet => {
                hex!("6f10bbe11587431707df676556a0551025a4f66551acdddc5793389e20f0d46e")
            }
            Self::TokenMigratorSet => {
                hex!("cf2372b9357f0d392563d2cedb11e2b3bf0c14d2b8b75eb8bad073bbea9b0ff9")
            }
            Self::FlapTokenTaxSet => {
                hex!("1546924f4680b1b2e093fd251c437c8781d81a21ed8ad1895e1f2c9b78db0cd0")
            }
            Self::FlapTokenAsymmetricTaxSet => {
                hex!("46cc246a238d1ca0951a15200994903e2d56cbb0389e63f09d66412a787aa3c0")
            }
            Self::VanityTokenCreated => {
                hex!("16f7f7185702757d3ae091c2aaad3cee3a5e0bb82ac2a51fbc9af44651b5080c")
            }
            Self::FlapTokenStaged => {
                hex!("2bf0a17cc6127084d945eb95a40df6c839234845722b025fdeef767b9464c02d")
            }
        }
    }

    pub fn from_topic0(topic0: &[u8; 32]) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.topic0() == *topic0)
    }
}

/// One BUY or SELL on the bonding curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurveTrade {
    pub ts: U256,
    pub token: Address,
    /// Buyer or seller.
    pub trader: Address,
    /// Tokens bought or sold.
    pub amount: U256,
    /// Native amount spent or received.
    pub eth: U256,
    pub fee: U256,
    pub post_price: U256,
}

/// A decoded Portal event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalEvent<'a> {
    TokenCreated {
        ts: U256,
        creator: Address,
        nonce: U256,
        token: Address,
        name: &'a [u8],
        symbol: &'a [u8],
        meta: &'a [u8],
    },
    TokenBought(CurveTrade),
    TokenSold(CurveTrade),
    /// Migration (graduation) of the token to a DEX pool.
    LaunchedToDex {
        token: Address,
        pool: Address,
        amount: U256,
        eth: U256,
    },
    TokenVersionSet {
        token: Address,
        version: u8,
    },
    TokenQuoteSet {
        token: Address,
        quote_token: Address,
    },
    TokenCurveSet {
        token: Address,
        curve: Address,
        parameter: U256,
    },
    TokenCurveSetV2 {
        token: Address,
        r: U256,
        h: U256,
        k: U256,
    },
    TokenDexSupplyThreshSet {
        token: Address,
        dex_supply_thresh: U256,
    },
    TokenMigratorSet {
        token: Address,
        migrator_type: u8,
    },
    FlapTokenTaxSet {
        token: Address,
        tax: U256,
    },
    FlapTokenAsymmetricTaxSet {
        token: Address,
        buy_tax: U256,
        sell_tax: U256,
    },
    VanityTokenCreated {
        token: Address,
        creator: Address,
        beneficiary: Address,
    },
    FlapTokenStaged {
        ts: U256,
        creator: Address,
        token: Address,
    },
}

impl PortalEvent<'_> {
    pub fn kind(&self) -> PortalEventKind {
        match self {
            Self::TokenCreated { .. } => PortalEventKind::TokenCreated,
            Self::TokenBought(_) => PortalEventKind::TokenBought,
            Self::TokenSold(_) => PortalEventKind::TokenSold,
            Self::LaunchedToDex { .. } => PortalEventKind::LaunchedToDex,
            Self::TokenVersionSet { .. } => PortalEventKind::TokenVersionSet,
            Self::TokenQuoteSet { .. } => PortalEventKind::TokenQuoteSet,
            Self::TokenCurveSet { .. } => PortalEventKind::TokenCurveSet,
            Self::TokenCurveSetV2 { .. } => PortalEventKind::TokenCurveSetV2,
            Self::TokenDexSupplyThreshSet { .. } => PortalEventKind::TokenDexSupplyThreshSet,
            Self::TokenMigratorSet { .. } => PortalEventKind::TokenMigratorSet,
            Self::FlapTokenTaxSet { .. } => PortalEventKind::FlapTokenTaxSet,
            Self::FlapTokenAsymmetricTaxSet { .. } => PortalEventKind::FlapTokenAsymmetricTaxSet,
            Self::VanityTokenCreated { .. } => PortalEventKind::VanityTokenCreated,
            Self::FlapTokenStaged { .. } => PortalEventKind::FlapTokenStaged,
        }
    }
}

/// What one log is to this decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalLog<'a> {
    /// Emitted by another contract.
    NotPortal,
    Event(PortalEvent<'a>),
    /// A Portal topic this decoder does not know (counted, never guessed).
    Unrecognized(B256),
    /// A known topic with another layout: refused.
    Malformed(PortalEventKind, AbiError),
}

/// Why a receipt's logs were not decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiptError {
    /// The transaction reverted: it has no effects to decode.
    Reverted,
}

/// The fixed words of one log's data, checked to be exactly `words` long
/// when `exact` (static layouts) or at least that long (dynamic tail).
struct Words<'a>(&'a [u8]);

impl<'a> Words<'a> {
    fn word(&self, index: usize) -> &'a [u8] {
        &self.0[index * WORD..(index + 1) * WORD]
    }

    fn u256(&self, index: usize) -> U256 {
        U256::from_be_slice(self.word(index))
    }

    fn address(&self, index: usize) -> Result<Address, AbiError> {
        let (high, low) = self.word(index).split_at(12);
        if high.iter().any(|b| *b != 0) {
            return Err(AbiError::ValueOutOfRange);
        }
        Ok(Address::from_slice(low))
    }

    fn u8(&self, index: usize) -> Result<u8, AbiError> {
        let (high, low) = self.word(index).split_at(WORD - 1);
        if high.iter().any(|b| *b != 0) {
            return Err(AbiError::ValueOutOfRange);
        }
        Ok(low[0])
    }

    fn offset(&self, index: usize) -> Result<usize, AbiError> {
        let (high, low) = self.word(index).split_at(WORD - 8);
        if high.iter().any(|b| *b != 0) {
            return Err(AbiError::ValueOutOfRange);
        }
        usize::try_from(u64::from_be_bytes(
            low.try_into().map_err(|_| AbiError::Truncated)?,
        ))
        .map_err(|_| AbiError::ValueOutOfRange)
    }
}

/// `data` must be exactly `words` static words.
fn exact(data: &[u8], words: usize) -> Result<Words<'_>, AbiError> {
    match data.len().cmp(&(words * WORD)) {
        core::cmp::Ordering::Less => Err(AbiError::Truncated),
        core::cmp::Ordering::Greater => Err(AbiError::TrailingBytes),
        core::cmp::Ordering::Equal => Ok(Words(data)),
    }
}

/// The three strings of `TokenCreated`, canonical: each tail starts where
/// the previous ended, zero padded, nothing after the last.
fn strings(data: &[u8], heads: [usize; 3], head_len: usize) -> Result<[&[u8]; 3], AbiError> {
    let w = Words(data);
    let mut next = head_len;
    let mut out: [&[u8]; 3] = [&[]; 3];
    for (slot, head) in out.iter_mut().zip(heads) {
        let offset = w.offset(head)?;
        if offset != next {
            return Err(AbiError::NonCanonical);
        }
        let start = offset.checked_add(WORD).ok_or(AbiError::Truncated)?;
        let len_word = data.get(offset..start).ok_or(AbiError::Truncated)?;
        let len = Words(len_word).offset(0)?;
        let padded = len
            .checked_next_multiple_of(WORD)
            .ok_or(AbiError::Truncated)?;
        let end = start.checked_add(padded).ok_or(AbiError::Truncated)?;
        let region = data.get(start..end).ok_or(AbiError::Truncated)?;
        let (value, padding) = region.split_at(len);
        if padding.iter().any(|b| *b != 0) {
            return Err(AbiError::NonCanonical);
        }
        *slot = value;
        next = end;
    }
    if next != data.len() {
        return Err(AbiError::TrailingBytes);
    }
    Ok(out)
}

fn decode_kind(kind: PortalEventKind, data: &[u8]) -> Result<PortalEvent<'_>, AbiError> {
    use PortalEventKind as K;
    let trade = |w: &Words<'_>| -> Result<CurveTrade, AbiError> {
        Ok(CurveTrade {
            ts: w.u256(0),
            token: w.address(1)?,
            trader: w.address(2)?,
            amount: w.u256(3),
            eth: w.u256(4),
            fee: w.u256(5),
            post_price: w.u256(6),
        })
    };
    Ok(match kind {
        K::TokenCreated => {
            const HEAD: usize = 7;
            if data.len() < HEAD * WORD {
                return Err(AbiError::Truncated);
            }
            let w = Words(data);
            let [name, symbol, meta] = strings(data, [4, 5, 6], HEAD * WORD)?;
            PortalEvent::TokenCreated {
                ts: w.u256(0),
                creator: w.address(1)?,
                nonce: w.u256(2),
                token: w.address(3)?,
                name,
                symbol,
                meta,
            }
        }
        K::TokenBought => PortalEvent::TokenBought(trade(&exact(data, 7)?)?),
        K::TokenSold => PortalEvent::TokenSold(trade(&exact(data, 7)?)?),
        K::LaunchedToDex => {
            let w = exact(data, 4)?;
            PortalEvent::LaunchedToDex {
                token: w.address(0)?,
                pool: w.address(1)?,
                amount: w.u256(2),
                eth: w.u256(3),
            }
        }
        K::TokenVersionSet => {
            let w = exact(data, 2)?;
            PortalEvent::TokenVersionSet {
                token: w.address(0)?,
                version: w.u8(1)?,
            }
        }
        K::TokenQuoteSet => {
            let w = exact(data, 2)?;
            PortalEvent::TokenQuoteSet {
                token: w.address(0)?,
                quote_token: w.address(1)?,
            }
        }
        K::TokenCurveSet => {
            let w = exact(data, 3)?;
            PortalEvent::TokenCurveSet {
                token: w.address(0)?,
                curve: w.address(1)?,
                parameter: w.u256(2),
            }
        }
        K::TokenCurveSetV2 => {
            let w = exact(data, 4)?;
            PortalEvent::TokenCurveSetV2 {
                token: w.address(0)?,
                r: w.u256(1),
                h: w.u256(2),
                k: w.u256(3),
            }
        }
        K::TokenDexSupplyThreshSet => {
            let w = exact(data, 2)?;
            PortalEvent::TokenDexSupplyThreshSet {
                token: w.address(0)?,
                dex_supply_thresh: w.u256(1),
            }
        }
        K::TokenMigratorSet => {
            let w = exact(data, 2)?;
            PortalEvent::TokenMigratorSet {
                token: w.address(0)?,
                migrator_type: w.u8(1)?,
            }
        }
        K::FlapTokenTaxSet => {
            let w = exact(data, 2)?;
            PortalEvent::FlapTokenTaxSet {
                token: w.address(0)?,
                tax: w.u256(1),
            }
        }
        K::FlapTokenAsymmetricTaxSet => {
            let w = exact(data, 3)?;
            PortalEvent::FlapTokenAsymmetricTaxSet {
                token: w.address(0)?,
                buy_tax: w.u256(1),
                sell_tax: w.u256(2),
            }
        }
        K::VanityTokenCreated => {
            let w = exact(data, 3)?;
            PortalEvent::VanityTokenCreated {
                token: w.address(0)?,
                creator: w.address(1)?,
                beneficiary: w.address(2)?,
            }
        }
        K::FlapTokenStaged => {
            let w = exact(data, 3)?;
            PortalEvent::FlapTokenStaged {
                ts: w.u256(0),
                creator: w.address(1)?,
                token: w.address(2)?,
            }
        }
    })
}

/// Decodes one log of a successful receipt. `portal` is the validated
/// deployment's Portal address.
pub fn decode_portal_log<'a>(
    portal: Address,
    emitter: Address,
    topics: &[[u8; 32]],
    data: &'a [u8],
) -> PortalLog<'a> {
    if emitter != portal {
        return PortalLog::NotPortal;
    }
    let Some(topic0) = topics.first() else {
        return PortalLog::Unrecognized(B256::ZERO);
    };
    let Some(kind) = PortalEventKind::from_topic0(topic0) else {
        return PortalLog::Unrecognized(B256::from(*topic0));
    };
    if topics.len() != 1 {
        // No Portal event has an indexed parameter.
        return PortalLog::Malformed(kind, AbiError::NonCanonical);
    }
    match decode_kind(kind, data) {
        Ok(event) => PortalLog::Event(event),
        Err(e) => PortalLog::Malformed(kind, e),
    }
}

/// The Portal logs of one receipt, decoded in log order; a reverted
/// receipt is refused (it has no effects).
pub fn decode_receipt_logs<'a, L>(
    portal: Address,
    succeeded: bool,
    logs: L,
) -> Result<impl Iterator<Item = PortalLog<'a>>, ReceiptError>
where
    L: IntoIterator<Item = (Address, &'a [[u8; 32]], &'a [u8])>,
{
    if !succeeded {
        return Err(ReceiptError::Reverted);
    }
    Ok(logs
        .into_iter()
        .map(move |(emitter, topics, data)| decode_portal_log(portal, emitter, topics, data)))
}
