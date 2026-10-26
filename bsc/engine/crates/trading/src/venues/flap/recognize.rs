//! Hot-path recognition of a pending Flap Portal call (Phase D contract
//! §5): destination, then the 4-byte selector, then the fixed lookup of
//! the selector table, then the verified decode. No I/O, no allocation,
//! no copy; oversized or malformed input fails closed.
//!
//! The table holds every launch and trade function of the Sourcify-verified
//! Portal ABI (v5.27.0, G0.1 Flap refreeze). Only calls with witnessed
//! fixtures are decoded: `newTokenV6` and `swapExactInput`. Every other
//! known call is reported as [`Recognized::Unsupported`], never guessed.
//! A call to any other contract (the launch routers and vanity launchers
//! of the live census) is [`Recognized::NotPortal`]: its calldata is not a
//! verified Portal ABI, so its launch is known only from the canonical
//! `TokenCreated` event.

use alloy_primitives::{Address, U256, hex};

use super::abi::{
    AbiError, LaunchParamsV6, NEW_TOKEN_V6_SELECTOR, SWAP_EXACT_INPUT_SELECTOR,
    TOKEN_VERSION_TAXED_V3, decode_new_token_v6,
};
use super::deployment::FlapDeployment;

const WORD: usize = 32;

/// A launch or trade function of the verified Portal ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalCall {
    NewTokenV2,
    NewTokenV3,
    NewTokenV4,
    NewTokenV5,
    NewTokenV6,
    NewTokenV7,
    StageNewTokenV5,
    CommitNewTokenV5,
    SwapExactInput,
    SwapExactInputV3,
    Buy,
    BuyOnCreation,
    Sell,
}

impl PortalCall {
    pub const ALL: [PortalCall; 13] = [
        Self::NewTokenV2,
        Self::NewTokenV3,
        Self::NewTokenV4,
        Self::NewTokenV5,
        Self::NewTokenV6,
        Self::NewTokenV7,
        Self::StageNewTokenV5,
        Self::CommitNewTokenV5,
        Self::SwapExactInput,
        Self::SwapExactInputV3,
        Self::Buy,
        Self::BuyOnCreation,
        Self::Sell,
    ];

    /// The canonical function signature (verified ABI).
    pub const fn signature(self) -> &'static str {
        match self {
            Self::NewTokenV2 => {
                "newTokenV2((string,string,string,uint8,bytes32,uint16,uint8,address,uint256,address,bytes))"
            }
            Self::NewTokenV3 => {
                "newTokenV3((string,string,string,uint8,bytes32,uint16,uint8,address,uint256,address,bytes,bytes32,bytes))"
            }
            Self::NewTokenV4 => {
                "newTokenV4((string,string,string,uint8,bytes32,uint16,uint8,address,uint256,address,bytes,bytes32,bytes,uint8,uint8))"
            }
            Self::NewTokenV5 => {
                "newTokenV5((string,string,string,uint8,bytes32,uint16,uint8,address,uint256,address,bytes,bytes32,bytes,uint8,uint8,uint64,uint64,uint16,uint16,uint16,uint16,uint256))"
            }
            Self::NewTokenV6 => {
                "newTokenV6((string,string,string,uint8,bytes32,uint8,address,uint256,address,bytes,bytes32,bytes,uint8,uint8,uint16,uint16,uint64,uint64,uint16,uint16,uint16,uint16,uint256,address,address,uint8))"
            }
            Self::NewTokenV7 => {
                "newTokenV7((string,string,string,uint8,bytes32,uint8,address,uint256,bytes,bytes32,bytes,uint8,uint16,uint16,uint64,uint64,address,uint8,(uint8,uint16,address,address,uint256)[4]))"
            }
            Self::StageNewTokenV5 => "stageNewTokenV5((uint8,bytes32,bool,uint8,address,uint8))",
            Self::CommitNewTokenV5 => {
                "commitNewTokenV5((bytes32,uint16,string,string,string,uint256,address,bytes,uint64,uint64,uint16,uint16,uint16,uint16,uint256))"
            }
            Self::SwapExactInput => "swapExactInput((address,address,uint256,uint256,bytes))",
            Self::SwapExactInputV3 => {
                "swapExactInputV3((address,address,uint256,uint256,bytes,bytes))"
            }
            Self::Buy => "buy(address,address,uint256)",
            Self::BuyOnCreation => "buyOnCreation(address,address,uint256)",
            Self::Sell => "sell(address,uint256,uint256)",
        }
    }

    /// The first four bytes of Keccak-256 of [`Self::signature`] (checked
    /// with alloy in the tests and against the freeze evidence).
    pub const fn selector(self) -> [u8; 4] {
        match self {
            Self::NewTokenV2 => hex!("0ba6324e"),
            Self::NewTokenV3 => hex!("7e15676e"),
            Self::NewTokenV4 => hex!("3ba6f26a"),
            Self::NewTokenV5 => hex!("2e2fdbd9"),
            Self::NewTokenV6 => NEW_TOKEN_V6_SELECTOR,
            Self::NewTokenV7 => hex!("87ef5b30"),
            Self::StageNewTokenV5 => hex!("9d55bde4"),
            Self::CommitNewTokenV5 => hex!("5d29f9f2"),
            Self::SwapExactInput => SWAP_EXACT_INPUT_SELECTOR,
            Self::SwapExactInputV3 => hex!("4f7edf58"),
            Self::Buy => hex!("153e66e6"),
            Self::BuyOnCreation => hex!("fcb5c9e3"),
            Self::Sell => hex!("6a272462"),
        }
    }

    pub fn from_selector(selector: [u8; 4]) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.selector() == selector)
    }

    /// A launch (token creation) function.
    pub const fn is_launch(self) -> bool {
        matches!(
            self,
            Self::NewTokenV2
                | Self::NewTokenV3
                | Self::NewTokenV4
                | Self::NewTokenV5
                | Self::NewTokenV6
                | Self::NewTokenV7
                | Self::StageNewTokenV5
                | Self::CommitNewTokenV5
        )
    }
}

/// A decoded `swapExactInput` call. Native BNB is the zero address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapExactInput<'a> {
    pub input_token: Address,
    pub output_token: Address,
    pub input_amount: U256,
    pub min_output_amount: U256,
    /// `permitData` (not verified here).
    pub permit: &'a [u8],
}

impl SwapExactInput<'_> {
    /// Native in, a token out: a BUY of `output_token`.
    pub fn is_buy(&self) -> bool {
        self.input_token == Address::ZERO && self.output_token != Address::ZERO
    }

    /// A token in, native out: a SELL of `input_token`.
    pub fn is_sell(&self) -> bool {
        self.output_token == Address::ZERO && self.input_token != Address::ZERO
    }
}

/// What a pending call is to this decoder.
// Returned by value on the hot path: boxing the launch would allocate
// (Phase D contract §5 forbids allocation there); the size is fixed.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recognized<'a> {
    /// Not addressed to the Portal (a contract creation, a router, a
    /// launcher or anything else).
    NotPortal,
    /// To the Portal, with fewer than four bytes or a selector outside the
    /// table.
    UnknownSelector([u8; 4]),
    /// A canonical `newTokenV6` launch; its token version is as found.
    LaunchV6(LaunchParamsV6<'a>),
    /// A canonical `swapExactInput`.
    Swap(SwapExactInput<'a>),
    /// A known call this decoder does not decode (no witnessed fixture).
    Unsupported(PortalCall),
    /// A decoded call whose calldata is oversized or not canonical.
    Malformed(PortalCall, AbiError),
}

/// Recognizes the pending call to `to` with `input`. `portal` is the
/// validated deployment's Portal; `max_calldata_len` the caller's bound.
pub fn recognize<'a>(
    portal: Address,
    to: Option<Address>,
    input: &'a [u8],
    max_calldata_len: usize,
) -> Recognized<'a> {
    if to != Some(portal) {
        return Recognized::NotPortal;
    }
    let Some(selector) = input.get(..4).and_then(|s| <[u8; 4]>::try_from(s).ok()) else {
        let mut short = [0u8; 4];
        short[..input.len()].copy_from_slice(input);
        return Recognized::UnknownSelector(short);
    };
    let Some(call) = PortalCall::from_selector(selector) else {
        return Recognized::UnknownSelector(selector);
    };
    if input.len() > max_calldata_len {
        return Recognized::Malformed(call, AbiError::Oversize);
    }
    match call {
        PortalCall::NewTokenV6 => match decode_new_token_v6(input, max_calldata_len) {
            Ok(params) => Recognized::LaunchV6(params),
            Err(e) => Recognized::Malformed(call, e),
        },
        PortalCall::SwapExactInput => match decode_swap_exact_input(input) {
            Ok(swap) => Recognized::Swap(swap),
            Err(e) => Recognized::Malformed(call, e),
        },
        other => Recognized::Unsupported(other),
    }
}

/// The token a pending V6 launch will create, where its derivation is
/// proven: `TOKEN_TAXED_V3` (CREATE2 from the Portal with the launch salt
/// and the tax-token clone's init code; reproduced on every located launch
/// of the G0.1 refreeze). Any other version: `None`, never a guess.
pub fn predicted_token(
    deployment: &FlapDeployment,
    launch: &LaunchParamsV6<'_>,
) -> Option<Address> {
    (launch.token_version == TOKEN_VERSION_TAXED_V3).then(|| deployment.taxed_v3_token(launch.salt))
}

fn usize_word(word: &[u8]) -> Result<usize, AbiError> {
    let (high, low) = word.split_at(WORD - 8);
    if high.iter().any(|b| *b != 0) {
        return Err(AbiError::ValueOutOfRange);
    }
    let mut value = [0u8; 8];
    value.copy_from_slice(low);
    usize::try_from(u64::from_be_bytes(value)).map_err(|_| AbiError::ValueOutOfRange)
}

fn address_word(word: &[u8]) -> Result<Address, AbiError> {
    let (high, low) = word.split_at(12);
    if high.iter().any(|b| *b != 0) {
        return Err(AbiError::ValueOutOfRange);
    }
    Ok(Address::from_slice(low))
}

/// Decodes canonical `swapExactInput((address,address,uint256,uint256,bytes))`
/// calldata: tuple at offset 32, `permitData` right after the static head,
/// zero padded, nothing after it.
pub fn decode_swap_exact_input(calldata: &[u8]) -> Result<SwapExactInput<'_>, AbiError> {
    let body = calldata
        .strip_prefix(&SWAP_EXACT_INPUT_SELECTOR)
        .ok_or(AbiError::WrongSelector)?;
    // Tuple offset, five head words, the permit length.
    if body.len() < 7 * WORD {
        return Err(AbiError::Truncated);
    }
    let word = |index: usize| &body[index * WORD..(index + 1) * WORD];
    if usize_word(word(0))? != WORD || usize_word(word(5))? != 5 * WORD {
        return Err(AbiError::NonCanonical);
    }
    let len = usize_word(word(6))?;
    let padded = len
        .checked_next_multiple_of(WORD)
        .ok_or(AbiError::Truncated)?;
    let end = (7 * WORD).checked_add(padded).ok_or(AbiError::Truncated)?;
    let region = body.get(7 * WORD..end).ok_or(AbiError::Truncated)?;
    if body.len() != end {
        return Err(AbiError::TrailingBytes);
    }
    let (permit, padding) = region.split_at(len);
    if padding.iter().any(|b| *b != 0) {
        return Err(AbiError::NonCanonical);
    }
    Ok(SwapExactInput {
        input_token: address_word(word(1))?,
        output_token: address_word(word(2))?,
        input_amount: U256::from_be_slice(word(3)),
        min_output_amount: U256::from_be_slice(word(4)),
        permit,
    })
}
