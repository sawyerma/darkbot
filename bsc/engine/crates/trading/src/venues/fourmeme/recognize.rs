//! Hot-path recognition of a pending Four.meme call (Phase E contract §5):
//! destination, then the 4-byte selector, then the fixed lookup of the
//! family's selector table, then the verified decode. No I/O, no
//! allocation, no copy; oversized or malformed input fails closed.
//!
//! The table holds every launch, BUY and SELL function of the published
//! ABIs whose selector the freeze found in the executing dispatcher. Only
//! calls with a canonical fixture are decoded: `createToken` of both
//! families, Classic `buyTokenAMAP(address,uint256,uint256)` and OpenFour
//! `buyByBudget`. Every other known call is [`Recognized::Unsupported`].

use alloy_primitives::{Address, B256, U256, hex};

use super::abi::{AbiError, WORD, Words, body, tails};
use super::deployment::{Family, FourDeployment};

/// A launch, BUY or SELL function of a published Four.meme ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FourCall {
    ClassicCreateToken,
    ClassicBuyTokenAmap,
    ClassicBuyTokenAmapTo,
    ClassicBuyTokenAmapOrigin,
    ClassicBuyTokenAmapOriginTo,
    ClassicBuyToken,
    ClassicBuyTokenTo,
    ClassicBuyTokenOrigin,
    ClassicBuyTokenOriginTo,
    ClassicSellToken,
    ClassicSellTokenMinFunds,
    ClassicSellTokenOrigin,
    ClassicSellTokenOriginMinFunds,
    ClassicSellTokenOriginFee,
    ClassicSellTokenOriginFromTo,
    ClassicSellTokenOriginFromFee,
    OpenFourCreateToken,
    OpenFourBuy,
    OpenFourBuyByBudget,
    OpenFourSell,
}

/// What a call does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallRole {
    Launch,
    Buy,
    Sell,
}

impl FourCall {
    pub const ALL: [FourCall; 20] = [
        Self::ClassicCreateToken,
        Self::ClassicBuyTokenAmap,
        Self::ClassicBuyTokenAmapTo,
        Self::ClassicBuyTokenAmapOrigin,
        Self::ClassicBuyTokenAmapOriginTo,
        Self::ClassicBuyToken,
        Self::ClassicBuyTokenTo,
        Self::ClassicBuyTokenOrigin,
        Self::ClassicBuyTokenOriginTo,
        Self::ClassicSellToken,
        Self::ClassicSellTokenMinFunds,
        Self::ClassicSellTokenOrigin,
        Self::ClassicSellTokenOriginMinFunds,
        Self::ClassicSellTokenOriginFee,
        Self::ClassicSellTokenOriginFromTo,
        Self::ClassicSellTokenOriginFromFee,
        Self::OpenFourCreateToken,
        Self::OpenFourBuy,
        Self::OpenFourBuyByBudget,
        Self::OpenFourSell,
    ];

    pub const fn family(self) -> Family {
        match self {
            Self::OpenFourCreateToken
            | Self::OpenFourBuy
            | Self::OpenFourBuyByBudget
            | Self::OpenFourSell => Family::OpenFour,
            _ => Family::Classic,
        }
    }

    pub const fn role(self) -> CallRole {
        match self {
            Self::ClassicCreateToken | Self::OpenFourCreateToken => CallRole::Launch,
            Self::ClassicSellToken
            | Self::ClassicSellTokenMinFunds
            | Self::ClassicSellTokenOrigin
            | Self::ClassicSellTokenOriginMinFunds
            | Self::ClassicSellTokenOriginFee
            | Self::ClassicSellTokenOriginFromTo
            | Self::ClassicSellTokenOriginFromFee
            | Self::OpenFourSell => CallRole::Sell,
            _ => CallRole::Buy,
        }
    }

    /// The canonical function signature (published ABI).
    pub const fn signature(self) -> &'static str {
        match self {
            Self::ClassicCreateToken | Self::OpenFourCreateToken => "createToken(bytes,bytes)",
            Self::ClassicBuyTokenAmap => "buyTokenAMAP(address,uint256,uint256)",
            Self::ClassicBuyTokenAmapTo => "buyTokenAMAP(address,address,uint256,uint256)",
            Self::ClassicBuyTokenAmapOrigin => "buyTokenAMAP(uint256,address,uint256,uint256)",
            Self::ClassicBuyTokenAmapOriginTo => {
                "buyTokenAMAP(uint256,address,address,uint256,uint256)"
            }
            Self::ClassicBuyToken => "buyToken(address,uint256,uint256)",
            Self::ClassicBuyTokenTo => "buyToken(address,address,uint256,uint256)",
            Self::ClassicBuyTokenOrigin => "buyToken(uint256,address,uint256,uint256)",
            Self::ClassicBuyTokenOriginTo => "buyToken(uint256,address,address,uint256,uint256)",
            Self::ClassicSellToken => "sellToken(address,uint256)",
            Self::ClassicSellTokenMinFunds => "sellToken(address,uint256,uint256)",
            Self::ClassicSellTokenOrigin => "sellToken(uint256,address,uint256)",
            Self::ClassicSellTokenOriginMinFunds => "sellToken(uint256,address,uint256,uint256)",
            Self::ClassicSellTokenOriginFee => {
                "sellToken(uint256,address,uint256,uint256,uint256,address)"
            }
            Self::ClassicSellTokenOriginFromTo => {
                "sellToken(uint256,address,address,address,uint256,uint256)"
            }
            Self::ClassicSellTokenOriginFromFee => {
                "sellToken(uint256,address,address,uint256,uint256,uint256,address)"
            }
            Self::OpenFourBuy => "buy(address,uint256,uint256,uint256,bytes)",
            Self::OpenFourBuyByBudget => "buyByBudget(address,uint256,uint256,uint256,bytes)",
            Self::OpenFourSell => "sell(address,uint256,uint256,uint256,bytes)",
        }
    }

    /// The first four bytes of Keccak-256 of [`Self::signature`] (checked
    /// with alloy in the tests and against the freeze evidence).
    pub const fn selector(self) -> [u8; 4] {
        match self {
            Self::ClassicCreateToken | Self::OpenFourCreateToken => hex!("519ebb10"),
            Self::ClassicBuyTokenAmap => hex!("87f27655"),
            Self::ClassicBuyTokenAmapTo => hex!("7f79f6df"),
            Self::ClassicBuyTokenAmapOrigin => hex!("edf9e251"),
            Self::ClassicBuyTokenAmapOriginTo => hex!("5e56c39a"),
            Self::ClassicBuyToken => hex!("e671499b"),
            Self::ClassicBuyTokenTo => hex!("7d17ff3d"),
            Self::ClassicBuyTokenOrigin => hex!("43fc3523"),
            Self::ClassicBuyTokenOriginTo => hex!("90702094"),
            Self::ClassicSellToken => hex!("f464e7db"),
            Self::ClassicSellTokenMinFunds => hex!("3e11741f"),
            Self::ClassicSellTokenOrigin => hex!("15a68a10"),
            Self::ClassicSellTokenOriginMinFunds => hex!("0da74935"),
            Self::ClassicSellTokenOriginFee => hex!("06e7b98f"),
            Self::ClassicSellTokenOriginFromTo => hex!("930cc050"),
            Self::ClassicSellTokenOriginFromFee => hex!("e63aaf36"),
            Self::OpenFourBuy => hex!("247d525e"),
            Self::OpenFourBuyByBudget => hex!("995ee6da"),
            Self::OpenFourSell => hex!("8bfc7764"),
        }
    }

    /// The call of `family` with `selector`.
    pub fn from_selector(family: Family, selector: [u8; 4]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.family() == family && c.selector() == selector)
    }
}

/// A decoded Classic `createToken(bytes args, bytes signature)`. `args` is
/// one ABI tuple (its word 0 is the tuple offset 0x20); the freeze proves
/// its head fields 0 (`requestId`, equal to the event's) and 1 (the CREATE2
/// salt). Every other field of `args` is UNKNOWN and kept raw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicLaunch<'a> {
    pub request_id: U256,
    pub salt: B256,
    pub args: &'a [u8],
    pub signature: &'a [u8],
}

/// OpenFour `TokenInitParams` (published `OpenFourTools` ABI), checked
/// against the `TokenCreated` event (name, symbol, tokenUri =
/// tokenMetaUri, maxSupply, saleAmount, raiseAmount, quoteAsset) and the
/// derivation (`tokenSalt`). The `*_params` members are opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenInitParams<'a> {
    pub name: &'a [u8],
    pub symbol: &'a [u8],
    pub token_uri: &'a [u8],
    pub max_supply: U256,
    pub sale_amount: U256,
    pub raise_amount: U256,
    pub quote_asset: Address,
    pub token_salt: B256,
    pub token_params: &'a [u8],
    pub vault_params: &'a [u8],
    pub curve_params: &'a [u8],
    pub trade_params: &'a [u8],
    pub migrate_params: &'a [u8],
    pub custom_data_params: &'a [u8],
}

/// A decoded OpenFour `createToken(bytes createArgs, bytes signature)`.
/// `createArgs` is one ABI tuple: `requestId` and `presetId` (both equal to
/// the event's), five unnamed static words (kept raw), the offset of
/// [`TokenInitParams`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourLaunch<'a> {
    pub request_id: U256,
    pub preset_id: U256,
    pub unnamed: [B256; OPENFOUR_UNNAMED_WORDS],
    pub init: TokenInitParams<'a>,
    pub signature: &'a [u8],
}

/// Unnamed static words of the `createArgs` head (after the two ids).
pub const OPENFOUR_UNNAMED_WORDS: usize = 5;
/// The `createArgs` head: two ids, the unnamed words, the init offset.
const OPENFOUR_ARGS_HEAD_WORDS: usize = 2 + OPENFOUR_UNNAMED_WORDS + 1;
/// `TokenInitParams` head words: three strings, four static values, the
/// salt, six `bytes`.
const INIT_HEAD_WORDS: usize = 14;

/// A decoded Classic `buyTokenAMAP(address token, uint256 funds, uint256
/// minAmount)`. Token and buyer positions are checked against the event;
/// the amount semantics are UNKNOWN (Phase G).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassicBuyAmap {
    pub token: Address,
    pub funds: U256,
    pub min_amount: U256,
}

/// A decoded OpenFour `buyByBudget(address token, uint256 maxPayAmount,
/// uint256 minAmountOut, uint256 options, bytes proof)`. Token checked
/// against the event; amount semantics UNKNOWN (Phase G).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFourBuyByBudget<'a> {
    pub token: Address,
    pub max_pay_amount: U256,
    pub min_amount_out: U256,
    pub options: U256,
    pub proof: &'a [u8],
}

/// What a pending call is to this decoder.
// Returned by value on the hot path: boxing would allocate.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recognized<'a> {
    /// Not addressed to TokenManager2 or OpenFour Core.
    NotFour,
    /// To a Four contract, with fewer than four bytes or a selector outside
    /// the family's table.
    UnknownSelector(Family, [u8; 4]),
    ClassicLaunch(ClassicLaunch<'a>),
    OpenFourLaunch(OpenFourLaunch<'a>),
    ClassicBuy(ClassicBuyAmap),
    OpenFourBuy(OpenFourBuyByBudget<'a>),
    /// A known call this decoder does not decode (no canonical fixture).
    Unsupported(FourCall),
    /// A decoded call whose input is oversized, not canonical or of an
    /// unwitnessed layout.
    Malformed(FourCall, AbiError),
}

/// The token a pending launch creates, as far as the calldata proves it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingToken {
    /// CREATE2 from frozen inputs the calldata selects: an OpenFour preset
    /// of the frozen table and its `tokenSalt`.
    Derived(Address),
    /// CREATE2 from the frozen Classic deployer and template and the salt
    /// of the calldata. Every frozen launch used that template, but the
    /// calldata does not name it: a candidate until the canonical
    /// `TokenCreate` names the same token.
    Unconfirmed(Address),
    /// No proven derivation (another preset, not a decoded launch).
    Unknown,
}

/// Recognizes the pending call to `to` with `input` against the frozen
/// `deployment`; `max_calldata_len` is the caller's bound.
pub fn recognize<'a>(
    deployment: &FourDeployment,
    to: Option<Address>,
    input: &'a [u8],
    max_calldata_len: usize,
) -> Recognized<'a> {
    let Some(family) = to.and_then(|to| deployment.family_of(to)) else {
        return Recognized::NotFour;
    };
    let Some(selector) = input.get(..4).and_then(|s| <[u8; 4]>::try_from(s).ok()) else {
        let mut short = [0u8; 4];
        short[..input.len()].copy_from_slice(input);
        return Recognized::UnknownSelector(family, short);
    };
    let Some(call) = FourCall::from_selector(family, selector) else {
        return Recognized::UnknownSelector(family, selector);
    };
    if input.len() > max_calldata_len {
        return Recognized::Malformed(call, AbiError::Oversize);
    }
    let decoded = match call {
        FourCall::ClassicCreateToken => {
            decode_classic_create(input, deployment.classic_args_len).map(Recognized::ClassicLaunch)
        }
        FourCall::OpenFourCreateToken => {
            decode_openfour_create(input).map(Recognized::OpenFourLaunch)
        }
        FourCall::ClassicBuyTokenAmap => decode_classic_buy_amap(input).map(Recognized::ClassicBuy),
        FourCall::OpenFourBuyByBudget => {
            decode_openfour_buy_by_budget(input).map(Recognized::OpenFourBuy)
        }
        other => return Recognized::Unsupported(other),
    };
    decoded.unwrap_or_else(|e| Recognized::Malformed(call, e))
}

/// The token of a recognized launch, where its derivation is proven.
pub fn pending_token(deployment: &FourDeployment, recognized: &Recognized<'_>) -> PendingToken {
    match recognized {
        Recognized::ClassicLaunch(l) => PendingToken::Unconfirmed(deployment.classic_token(l.salt)),
        Recognized::OpenFourLaunch(l) => deployment
            .openfour_token(l.preset_id, l.init.token_salt)
            .map_or(PendingToken::Unknown, PendingToken::Derived),
        _ => PendingToken::Unknown,
    }
}

/// The two `bytes` arguments of `createToken(bytes,bytes)`, canonical.
fn create_args(input: &[u8]) -> Result<[&[u8]; 2], AbiError> {
    tails(
        body(input, &FourCall::ClassicCreateToken.selector())?,
        2,
        [0, 1],
    )
}

/// The ABI tuple inside a `bytes` argument: its word 0 must be the
/// tuple offset 0x20.
fn tuple(argument: &[u8]) -> Result<&[u8], AbiError> {
    if Words(argument).usize(0)? != WORD {
        return Err(AbiError::NonCanonical);
    }
    Ok(&argument[WORD..])
}

pub fn decode_classic_create(input: &[u8], args_len: usize) -> Result<ClassicLaunch<'_>, AbiError> {
    let [args, signature] = create_args(input)?;
    let head = Words(tuple(args)?);
    if args.len() != args_len {
        return Err(AbiError::UnwitnessedLayout);
    }
    Ok(ClassicLaunch {
        request_id: head.u256(0)?,
        salt: head.b256(1)?,
        args,
        signature,
    })
}

pub fn decode_openfour_create(input: &[u8]) -> Result<OpenFourLaunch<'_>, AbiError> {
    let [args, signature] = create_args(input)?;
    let tuple = tuple(args)?;
    let head = Words(tuple);
    if head.usize(OPENFOUR_ARGS_HEAD_WORDS - 1)? != OPENFOUR_ARGS_HEAD_WORDS * WORD {
        return Err(AbiError::UnwitnessedLayout);
    }
    let mut unnamed = [B256::ZERO; OPENFOUR_UNNAMED_WORDS];
    for (i, w) in unnamed.iter_mut().enumerate() {
        *w = head.b256(2 + i)?;
    }
    // TokenInitParams is the last member of createArgs and ends it.
    let init = &tuple[OPENFOUR_ARGS_HEAD_WORDS * WORD..];
    let [
        name,
        symbol,
        token_uri,
        token_params,
        vault_params,
        curve_params,
        trade_params,
        migrate_params,
        custom_data_params,
    ] = tails(init, INIT_HEAD_WORDS, [0, 1, 2, 8, 9, 10, 11, 12, 13])?;
    let i = Words(init);
    Ok(OpenFourLaunch {
        request_id: head.u256(0)?,
        preset_id: head.u256(1)?,
        unnamed,
        init: TokenInitParams {
            name,
            symbol,
            token_uri,
            max_supply: i.u256(3)?,
            sale_amount: i.u256(4)?,
            raise_amount: i.u256(5)?,
            quote_asset: i.address(6)?,
            token_salt: i.b256(7)?,
            token_params,
            vault_params,
            curve_params,
            trade_params,
            migrate_params,
            custom_data_params,
        },
        signature,
    })
}

pub fn decode_classic_buy_amap(input: &[u8]) -> Result<ClassicBuyAmap, AbiError> {
    let w = super::abi::exact(body(input, &FourCall::ClassicBuyTokenAmap.selector())?, 3)?;
    Ok(ClassicBuyAmap {
        token: w.address(0)?,
        funds: w.u256(1)?,
        min_amount: w.u256(2)?,
    })
}

pub fn decode_openfour_buy_by_budget(input: &[u8]) -> Result<OpenFourBuyByBudget<'_>, AbiError> {
    let args = body(input, &FourCall::OpenFourBuyByBudget.selector())?;
    let [proof] = tails(args, 5, [4])?;
    let w = Words(args);
    Ok(OpenFourBuyByBudget {
        token: w.address(0)?,
        max_pay_amount: w.u256(1)?,
        min_amount_out: w.u256(2)?,
        options: w.u256(3)?,
        proof,
    })
}
