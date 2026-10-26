//! Flap Portal call encoding and strict decoding.
//!
//! Witnessed ABI (Gate-0 `g06_flap_pending_decode`, G0.2/G0.3 fixtures):
//!
//! ```text
//! newTokenV6((string,string,string,uint8,bytes32,uint8,address,uint256,
//!             address,bytes,bytes32,bytes,uint8,uint8,uint16,uint16,uint64,
//!             uint64,uint16,uint16,uint16,uint16,uint256,address,address,
//!             uint8))                                   selector 0x8cb5772c
//! swapExactInput((address,address,uint256,uint256,bytes))
//!                                                       selector 0xef7ec2e7
//! ```
//!
//! The Gate-0 decoder accepted a launch only when re-encoding reproduced the
//! original bytes. This decoder enforces the same property directly: it
//! accepts exactly the canonical encoding (ordered tails, minimal offsets,
//! zero padding, clean value types, no trailing bytes). Every other input is
//! an error, so an accepted launch always re-encodes to identical bytes.
//!
//! `string` members are returned as raw bytes. The Portal does not require
//! UTF-8 and nothing on the BUY path depends on their content.

use alloy_primitives::{Address, B256, U256};

/// Selector of `newTokenV6`.
pub const NEW_TOKEN_V6_SELECTOR: [u8; 4] = [0x8c, 0xb5, 0x77, 0x2c];

/// Selector of `swapExactInput`.
pub const SWAP_EXACT_INPUT_SELECTOR: [u8; 4] = [0xef, 0x7e, 0xc2, 0xe7];

/// `tokenVersion` of the witnessed `TOKEN_TAXED_V3` mode.
pub const TOKEN_VERSION_TAXED_V3: u8 = 6;

/// Exact size of a native-input `swapExactInput` call with an empty permit.
pub const SWAP_EXACT_INPUT_NATIVE_LEN: usize = 4 + 7 * WORD;

/// Selector of ERC-20 `approve(address,uint256)`.
pub const APPROVE_SELECTOR: [u8; 4] = [0x09, 0x5e, 0xa7, 0xb3];

/// Exact size of an `approve` call.
pub const APPROVE_LEN: usize = 4 + 2 * WORD;

const WORD: usize = 32;
const LAUNCH_FIELDS: usize = 26;
const LAUNCH_HEAD: usize = LAUNCH_FIELDS * WORD;
const EXTENSION_HEAD: usize = 4 * WORD;

/// Reason calldata is not a supported canonical Flap call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiError {
    /// The calldata exceeds the caller's declared capacity.
    Oversize,
    /// The calldata does not start with the expected selector.
    WrongSelector,
    /// The calldata ends before an announced member does.
    Truncated,
    /// Bytes remain after the last member.
    TrailingBytes,
    /// Valid ABI, but not the unique canonical encoding.
    NonCanonical,
    /// A value type carries bits outside its declared width.
    ValueOutOfRange,
    /// The launch is not token version 6 (`TOKEN_TAXED_V3`).
    UnsupportedTokenVersion,
    /// The BUY input token is not native BNB.
    UnsupportedInputToken,
    /// A BUY must spend a positive amount.
    ZeroInputAmount,
    /// A BUY must carry a positive minimum output.
    ZeroMinOutput,
}

impl core::fmt::Display for AbiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Oversize => "flap abi: calldata exceeds the declared capacity",
            Self::WrongSelector => "flap abi: unexpected selector",
            Self::Truncated => "flap abi: calldata is truncated",
            Self::TrailingBytes => "flap abi: trailing bytes",
            Self::NonCanonical => "flap abi: non-canonical encoding",
            Self::ValueOutOfRange => "flap abi: value outside its declared width",
            Self::UnsupportedTokenVersion => "flap abi: unsupported token version",
            Self::UnsupportedInputToken => "flap abi: BUY input token is not native",
            Self::ZeroInputAmount => "flap abi: BUY input amount is zero",
            Self::ZeroMinOutput => "flap abi: BUY minimum output is zero",
        })
    }
}

impl std::error::Error for AbiError {}

/// Decoded `newTokenV6` parameters. Dynamic members borrow from the
/// calldata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaunchParamsV6<'a> {
    /// `name` (raw bytes).
    pub name: &'a [u8],
    /// `symbol` (raw bytes).
    pub symbol: &'a [u8],
    /// `uri` (raw bytes).
    pub uri: &'a [u8],
    /// `dexThresh`.
    pub dex_thresh: u8,
    /// CREATE2 salt of the token.
    pub salt: B256,
    /// `migratorType`.
    pub migrator_type: u8,
    /// Quote asset; the zero address is native BNB.
    pub quote_token: Address,
    /// Quote amount of the creator's launch-internal purchase.
    pub quote_amt: U256,
    /// `beneficiary`.
    pub beneficiary: Address,
    /// `permit`.
    pub permit: &'a [u8],
    /// `extensionId`.
    pub extension_id: B256,
    /// `extensionData`.
    pub extension_data: &'a [u8],
    /// `dexId`.
    pub dex_id: u8,
    /// `lpFeeProfile`.
    pub lp_fee_profile: u8,
    /// `buyTax` in basis points.
    pub buy_tax: u16,
    /// `sellTax` in basis points.
    pub sell_tax: u16,
    /// `taxDuration` in seconds.
    pub tax_duration: u64,
    /// `antiFarmerDuration` in seconds.
    pub anti_farmer_duration: u64,
    /// `mktBps`.
    pub mkt_bps: u16,
    /// `deflationBps`.
    pub deflation_bps: u16,
    /// `dividendBps`.
    pub dividend_bps: u16,
    /// `lpBps`.
    pub lp_bps: u16,
    /// `minimumShareBalance`.
    pub minimum_share_balance: U256,
    /// `dividendToken`.
    pub dividend_token: Address,
    /// `commissionReceiver`.
    pub commission_receiver: Address,
    /// `tokenVersion`; only [`TOKEN_VERSION_TAXED_V3`] is supported.
    pub token_version: u8,
}

impl LaunchParamsV6<'_> {
    /// Fails unless the launch is the witnessed `TOKEN_TAXED_V3` mode.
    pub fn require_taxed_v3(&self) -> Result<(), AbiError> {
        if self.token_version == TOKEN_VERSION_TAXED_V3 {
            Ok(())
        } else {
            Err(AbiError::UnsupportedTokenVersion)
        }
    }
}

/// The `(uint256 mode, uint256 price, uint256 deadline, bytes signature)`
/// tuple carried in `extensionData` of the witnessed priced extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtensionTuple<'a> {
    /// `mode`.
    pub mode: U256,
    /// `price`.
    pub price: U256,
    /// `deadline` (unix seconds).
    pub deadline: U256,
    /// `signature` (not verified here).
    pub signature: &'a [u8],
}

/// Decoded native-input `swapExactInput` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapExactInputNative {
    /// Token to acquire.
    pub output_token: Address,
    /// Native amount to spend; must equal the transaction value.
    pub input_amount: U256,
    /// Minimum acceptable output.
    pub min_output_amount: U256,
}

// ---------------------------------------------------------------------------
// Word readers. Callers pass exactly 32 bytes.
// ---------------------------------------------------------------------------

/// Reads an unsigned integer that must fit in the low `width` bytes
/// (`width <= 8`).
#[inline]
fn small_word(word: &[u8], width: usize) -> Result<u64, AbiError> {
    let (high, low) = word.split_at(WORD - width);
    if high.iter().any(|b| *b != 0) {
        return Err(AbiError::ValueOutOfRange);
    }
    let mut value: u64 = 0;
    for b in low {
        value = (value << 8) | u64::from(*b);
    }
    Ok(value)
}

#[inline]
fn u8_word(word: &[u8]) -> Result<u8, AbiError> {
    u8::try_from(small_word(word, 1)?).map_err(|_| AbiError::ValueOutOfRange)
}

#[inline]
fn u16_word(word: &[u8]) -> Result<u16, AbiError> {
    u16::try_from(small_word(word, 2)?).map_err(|_| AbiError::ValueOutOfRange)
}

#[inline]
fn usize_word(word: &[u8]) -> Result<usize, AbiError> {
    usize::try_from(small_word(word, 8)?).map_err(|_| AbiError::ValueOutOfRange)
}

#[inline]
fn address_word(word: &[u8]) -> Result<Address, AbiError> {
    let (high, low) = word.split_at(12);
    if high.iter().any(|b| *b != 0) {
        return Err(AbiError::ValueOutOfRange);
    }
    Ok(Address::from_slice(low))
}

/// Walks the dynamic members of one tuple in canonical order.
struct Tails<'a> {
    tuple: &'a [u8],
    next: usize,
}

impl<'a> Tails<'a> {
    /// Returns the member whose head word is `head_word`. The member must
    /// start exactly where the previous one ended and be zero padded.
    fn take(&mut self, head_word: &[u8]) -> Result<&'a [u8], AbiError> {
        let offset = usize_word(head_word)?;
        if offset != self.next {
            return Err(AbiError::NonCanonical);
        }
        let data_start = offset.checked_add(WORD).ok_or(AbiError::Truncated)?;
        let len_word = self
            .tuple
            .get(offset..data_start)
            .ok_or(AbiError::Truncated)?;
        let len = usize_word(len_word)?;
        let padded = len
            .checked_next_multiple_of(WORD)
            .ok_or(AbiError::Truncated)?;
        let end = data_start.checked_add(padded).ok_or(AbiError::Truncated)?;
        let region = self.tuple.get(data_start..end).ok_or(AbiError::Truncated)?;
        let (data, padding) = region.split_at(len);
        if padding.iter().any(|b| *b != 0) {
            return Err(AbiError::NonCanonical);
        }
        self.next = end;
        Ok(data)
    }

    /// Requires that the tails end exactly at the end of the tuple.
    fn finish(&self) -> Result<(), AbiError> {
        if self.next == self.tuple.len() {
            Ok(())
        } else {
            Err(AbiError::TrailingBytes)
        }
    }
}

// ---------------------------------------------------------------------------
// newTokenV6
// ---------------------------------------------------------------------------

/// Decodes canonical `newTokenV6` calldata without copying.
///
/// `max_calldata_len` is the caller's declared capacity; longer calldata is
/// rejected before any parsing. The token version is returned as found; use
/// [`LaunchParamsV6::require_taxed_v3`] (or the address derivation, which
/// calls it) before acting on a launch.
pub fn decode_new_token_v6(
    calldata: &[u8],
    max_calldata_len: usize,
) -> Result<LaunchParamsV6<'_>, AbiError> {
    if calldata.len() > max_calldata_len {
        return Err(AbiError::Oversize);
    }
    let body = calldata
        .strip_prefix(&NEW_TOKEN_V6_SELECTOR)
        .ok_or(AbiError::WrongSelector)?;
    if body.len() < WORD + LAUNCH_HEAD {
        return Err(AbiError::Truncated);
    }
    let (tuple_offset, tuple) = body.split_at(WORD);
    if usize_word(tuple_offset)? != WORD {
        return Err(AbiError::NonCanonical);
    }
    let word = |index: usize| &tuple[index * WORD..(index + 1) * WORD];

    let mut tails = Tails {
        tuple,
        next: LAUNCH_HEAD,
    };
    let name = tails.take(word(0))?;
    let symbol = tails.take(word(1))?;
    let uri = tails.take(word(2))?;
    let permit = tails.take(word(9))?;
    let extension_data = tails.take(word(11))?;
    tails.finish()?;

    Ok(LaunchParamsV6 {
        name,
        symbol,
        uri,
        dex_thresh: u8_word(word(3))?,
        salt: B256::from_slice(word(4)),
        migrator_type: u8_word(word(5))?,
        quote_token: address_word(word(6))?,
        quote_amt: U256::from_be_slice(word(7)),
        beneficiary: address_word(word(8))?,
        permit,
        extension_id: B256::from_slice(word(10)),
        extension_data,
        dex_id: u8_word(word(12))?,
        lp_fee_profile: u8_word(word(13))?,
        buy_tax: u16_word(word(14))?,
        sell_tax: u16_word(word(15))?,
        tax_duration: small_word(word(16), 8)?,
        anti_farmer_duration: small_word(word(17), 8)?,
        mkt_bps: u16_word(word(18))?,
        deflation_bps: u16_word(word(19))?,
        dividend_bps: u16_word(word(20))?,
        lp_bps: u16_word(word(21))?,
        minimum_share_balance: U256::from_be_slice(word(22)),
        dividend_token: address_word(word(23))?,
        commission_receiver: address_word(word(24))?,
        token_version: u8_word(word(25))?,
    })
}

#[inline]
fn padded_len(len: usize) -> usize {
    len.next_multiple_of(WORD)
}

#[inline]
fn put_u64_word(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&[0u8; WORD - 8]);
    out.extend_from_slice(&value.to_be_bytes());
}

#[inline]
fn put_address_word(out: &mut Vec<u8>, address: Address) {
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(address.as_slice());
}

#[inline]
fn put_u256_word(out: &mut Vec<u8>, value: U256) {
    out.extend_from_slice(&value.to_be_bytes::<32>());
}

#[inline]
fn put_tail(out: &mut Vec<u8>, data: &[u8]) {
    put_u64_word(out, data.len() as u64);
    out.extend_from_slice(data);
    out.resize(out.len() + (padded_len(data.len()) - data.len()), 0);
}

/// Exact size of the canonical `newTokenV6` calldata for `params`.
pub fn new_token_v6_encoded_len(params: &LaunchParamsV6<'_>) -> usize {
    let tails = [
        params.name,
        params.symbol,
        params.uri,
        params.permit,
        params.extension_data,
    ];
    let tail_len: usize = tails.iter().map(|t| WORD + padded_len(t.len())).sum();
    4 + WORD + LAUNCH_HEAD + tail_len
}

/// Writes the canonical `newTokenV6` calldata for `params` into `out`,
/// replacing its contents. Used for controlled self-launches; a foreign
/// launch is forwarded as its original signed bytes and never re-encoded.
pub fn encode_new_token_v6(params: &LaunchParamsV6<'_>, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(new_token_v6_encoded_len(params));
    out.extend_from_slice(&NEW_TOKEN_V6_SELECTOR);
    put_u64_word(out, WORD as u64);

    // Head: value members in place, dynamic members as running offsets.
    let mut next = LAUNCH_HEAD;
    let mut offset_of = |data: &[u8]| {
        let offset = next;
        next += WORD + padded_len(data.len());
        offset as u64
    };
    put_u64_word(out, offset_of(params.name));
    put_u64_word(out, offset_of(params.symbol));
    put_u64_word(out, offset_of(params.uri));
    put_u64_word(out, u64::from(params.dex_thresh));
    out.extend_from_slice(params.salt.as_slice());
    put_u64_word(out, u64::from(params.migrator_type));
    put_address_word(out, params.quote_token);
    put_u256_word(out, params.quote_amt);
    put_address_word(out, params.beneficiary);
    put_u64_word(out, offset_of(params.permit));
    out.extend_from_slice(params.extension_id.as_slice());
    put_u64_word(out, offset_of(params.extension_data));
    put_u64_word(out, u64::from(params.dex_id));
    put_u64_word(out, u64::from(params.lp_fee_profile));
    put_u64_word(out, u64::from(params.buy_tax));
    put_u64_word(out, u64::from(params.sell_tax));
    put_u64_word(out, params.tax_duration);
    put_u64_word(out, params.anti_farmer_duration);
    put_u64_word(out, u64::from(params.mkt_bps));
    put_u64_word(out, u64::from(params.deflation_bps));
    put_u64_word(out, u64::from(params.dividend_bps));
    put_u64_word(out, u64::from(params.lp_bps));
    put_u256_word(out, params.minimum_share_balance);
    put_address_word(out, params.dividend_token);
    put_address_word(out, params.commission_receiver);
    put_u64_word(out, u64::from(params.token_version));

    // Tails, in member order.
    put_tail(out, params.name);
    put_tail(out, params.symbol);
    put_tail(out, params.uri);
    put_tail(out, params.permit);
    put_tail(out, params.extension_data);
}

/// Decodes the canonical `(uint256,uint256,uint256,bytes)` extension tuple.
pub fn decode_extension_tuple(extension_data: &[u8]) -> Result<ExtensionTuple<'_>, AbiError> {
    if extension_data.len() < EXTENSION_HEAD {
        return Err(AbiError::Truncated);
    }
    let word = |index: usize| &extension_data[index * WORD..(index + 1) * WORD];
    let mut tails = Tails {
        tuple: extension_data,
        next: EXTENSION_HEAD,
    };
    let signature = tails.take(word(3))?;
    tails.finish()?;
    Ok(ExtensionTuple {
        mode: U256::from_be_slice(word(0)),
        price: U256::from_be_slice(word(1)),
        deadline: U256::from_be_slice(word(2)),
        signature,
    })
}

// ---------------------------------------------------------------------------
// swapExactInput, native input, empty permit
// ---------------------------------------------------------------------------

/// Returns the calldata of a direct native-input BUY of `output_token`.
///
/// Fixed size, no allocation. The acquired tokens go to the caller of the
/// Portal: this ABI has no recipient argument. Zero amounts are rejected; a
/// production BUY always carries a computed positive minimum output.
pub fn encode_swap_exact_input_native(
    output_token: Address,
    input_amount: U256,
    min_output_amount: U256,
) -> Result<[u8; SWAP_EXACT_INPUT_NATIVE_LEN], AbiError> {
    if input_amount.is_zero() {
        return Err(AbiError::ZeroInputAmount);
    }
    if min_output_amount.is_zero() {
        return Err(AbiError::ZeroMinOutput);
    }
    let mut out = [0u8; SWAP_EXACT_INPUT_NATIVE_LEN];
    out[..4].copy_from_slice(&SWAP_EXACT_INPUT_SELECTOR);
    // Word 0: offset of the tuple. Word 1: inputToken = native (zero).
    out[4 + WORD - 1] = 0x20;
    out[4 + 2 * WORD + 12..4 + 3 * WORD].copy_from_slice(output_token.as_slice());
    out[4 + 3 * WORD..4 + 4 * WORD].copy_from_slice(&input_amount.to_be_bytes::<32>());
    out[4 + 4 * WORD..4 + 5 * WORD].copy_from_slice(&min_output_amount.to_be_bytes::<32>());
    // Word 5: offset of `permit` inside the tuple. Word 6: its length, zero.
    out[4 + 6 * WORD - 1] = 0xa0;
    Ok(out)
}

/// Returns the calldata of a direct native-output SELL of `input_token`:
/// the same `swapExactInput` layout as the BUY with the tokens swapped
/// (witnessed, `fixtures/flap/exit_calls_witnessed.txt`). The Portal takes
/// the tokens from the caller, so an approval of at least `input_amount`
/// precedes it; the native output goes to the caller. Zero amounts are
/// rejected.
pub fn encode_swap_exact_input_sell(
    input_token: Address,
    input_amount: U256,
    min_output_amount: U256,
) -> Result<[u8; SWAP_EXACT_INPUT_NATIVE_LEN], AbiError> {
    if input_token == Address::ZERO {
        return Err(AbiError::UnsupportedInputToken);
    }
    if input_amount.is_zero() {
        return Err(AbiError::ZeroInputAmount);
    }
    if min_output_amount.is_zero() {
        return Err(AbiError::ZeroMinOutput);
    }
    let mut out = [0u8; SWAP_EXACT_INPUT_NATIVE_LEN];
    out[..4].copy_from_slice(&SWAP_EXACT_INPUT_SELECTOR);
    // Word 0: offset of the tuple. Word 1: inputToken. Word 2: outputToken
    // = native (zero).
    out[4 + WORD - 1] = 0x20;
    out[4 + WORD + 12..4 + 2 * WORD].copy_from_slice(input_token.as_slice());
    out[4 + 3 * WORD..4 + 4 * WORD].copy_from_slice(&input_amount.to_be_bytes::<32>());
    out[4 + 4 * WORD..4 + 5 * WORD].copy_from_slice(&min_output_amount.to_be_bytes::<32>());
    out[4 + 6 * WORD - 1] = 0xa0;
    Ok(out)
}

/// Returns the calldata of ERC-20 `approve(spender, amount)` (witnessed for
/// the Portal as spender). A zero amount is rejected (it revokes).
pub fn encode_approve(spender: Address, amount: U256) -> Result<[u8; APPROVE_LEN], AbiError> {
    if amount.is_zero() {
        return Err(AbiError::ZeroInputAmount);
    }
    let mut out = [0u8; APPROVE_LEN];
    out[..4].copy_from_slice(&APPROVE_SELECTOR);
    out[4 + 12..4 + WORD].copy_from_slice(spender.as_slice());
    out[4 + WORD..].copy_from_slice(&amount.to_be_bytes::<32>());
    Ok(out)
}

/// Decodes a canonical `approve(address,uint256)` call.
pub fn decode_approve(calldata: &[u8]) -> Result<(Address, U256), AbiError> {
    let body = calldata
        .strip_prefix(&APPROVE_SELECTOR)
        .ok_or(AbiError::WrongSelector)?;
    match body.len().cmp(&(2 * WORD)) {
        core::cmp::Ordering::Less => return Err(AbiError::Truncated),
        core::cmp::Ordering::Greater => return Err(AbiError::TrailingBytes),
        core::cmp::Ordering::Equal => {}
    }
    if body[..12].iter().any(|b| *b != 0) {
        return Err(AbiError::ValueOutOfRange);
    }
    Ok((
        Address::from_slice(&body[12..WORD]),
        U256::from_be_slice(&body[WORD..]),
    ))
}

/// Decodes a canonical native-input `swapExactInput` call with an empty
/// permit. Amounts are returned as found, including zero. The one
/// `swapExactInput` decoder is [`super::recognize::decode_swap_exact_input`];
/// this is its native-BUY restriction.
pub fn decode_swap_exact_input_native(calldata: &[u8]) -> Result<SwapExactInputNative, AbiError> {
    let swap = super::recognize::decode_swap_exact_input(calldata)?;
    if !swap.permit.is_empty() {
        return Err(AbiError::NonCanonical);
    }
    if swap.input_token != Address::ZERO {
        return Err(AbiError::UnsupportedInputToken);
    }
    Ok(SwapExactInputNative {
        output_token: swap.output_token,
        input_amount: swap.input_amount,
        min_output_amount: swap.min_output_amount,
    })
}

#[cfg(test)]
mod exit_tests {
    //! Phase I: the exit calls against the witnessed SELL and approval.
    use super::*;
    use crate::venues::flap::recognize::decode_swap_exact_input;

    const WITNESSED: &str = include_str!("../../../fixtures/flap/exit_calls_witnessed.txt");

    fn call(kind: &str) -> (Address, Vec<u8>) {
        let f: Vec<&str> = WITNESSED
            .lines()
            .find(|l| l.starts_with(kind))
            .unwrap()
            .split_whitespace()
            .collect();
        (
            f[3].parse().unwrap(),
            alloy_primitives::hex::decode(f[4]).unwrap(),
        )
    }

    /// The SELL encoder reproduces the witnessed SELL byte for byte from
    /// its decoded members; the one decoder reads it as a SELL.
    #[test]
    fn the_sell_is_the_witnessed_call() {
        let (portal, bytes) = call("SELL");
        assert_eq!(
            portal,
            super::super::deployment::bsc_mainnet_v5_27_0().portal
        );
        let swap = decode_swap_exact_input(&bytes).unwrap();
        assert!(swap.is_sell() && swap.permit.is_empty());
        let ours = encode_swap_exact_input_sell(
            swap.input_token,
            swap.input_amount,
            swap.min_output_amount,
        )
        .unwrap();
        assert_eq!(ours[..], bytes[..]);
        for (token, amount, min) in [
            (Address::ZERO, U256::from(1u8), U256::from(1u8)),
            (swap.input_token, U256::ZERO, U256::from(1u8)),
            (swap.input_token, U256::from(1u8), U256::ZERO),
        ] {
            assert!(encode_swap_exact_input_sell(token, amount, min).is_err());
        }
    }

    /// The approval encoder reproduces the witnessed approve of the Portal.
    #[test]
    fn the_approval_is_the_witnessed_call() {
        let (_token, bytes) = call("APPROVE");
        let (spender, amount) = decode_approve(&bytes).unwrap();
        assert_eq!(
            spender,
            super::super::deployment::bsc_mainnet_v5_27_0().portal
        );
        assert_eq!(encode_approve(spender, amount).unwrap()[..], bytes[..]);
        assert!(encode_approve(spender, U256::ZERO).is_err());
        assert_eq!(decode_approve(&bytes[..67]), Err(AbiError::Truncated));
        let mut dirty = bytes.clone();
        dirty[5] = 1;
        assert_eq!(decode_approve(&dirty), Err(AbiError::ValueOutOfRange));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transactions::legacy::testdata::{GOLDEN_CALLDATA, unhex};
    use alloy_primitives::keccak256;

    /// Canonical launch of the G0.2/G0.3 fixture (block 124415357, tx 0).
    const DOGE_LAUNCH: &str =
        include_str!("../../../fixtures/flap/launch_v6_doge_block_124415357.hex");
    /// Pending V6 launch from the G0.4 coverage capture: native quote with a
    /// creator purchase, buy tax 0.
    const NATIVE_CREATOR_BUY: &str =
        include_str!("../../../fixtures/flap/launch_v6_native_creator_buy_pending.hex");
    /// Pending V6 launch from the G0.4 coverage capture: native quote, 200
    /// bps taxes split between marketing and deflation.
    const NATIVE_SPLIT_TAX: &str =
        include_str!("../../../fixtures/flap/launch_v6_native_split_tax_pending.hex");

    const DOGE: &str = "ba2ae424d960c26247dd6c32edc70b295c744c43";
    const TOKEN: &str = "abe1fe16cf6bc48c9524713054efe6e954b17777";

    /// Offset of head word `index` of the launch tuple inside the calldata.
    const fn head(index: usize) -> usize {
        4 + WORD + index * WORD
    }

    fn fixture(hex: &str) -> Vec<u8> {
        unhex(hex.trim())
    }

    fn address(hex: &str) -> Address {
        Address::from_slice(&unhex(hex))
    }

    #[test]
    fn historical_doge_launch_decodes_field_exact() {
        let calldata = fixture(DOGE_LAUNCH);
        assert_eq!(calldata.len(), 1444);
        // `launch_calldata_keccak` of g03_rust_inputs.json.
        assert_eq!(
            keccak256(&calldata).as_slice(),
            unhex("4695a68e24ed5360acb35cc9e9fff9d82c1fcc0fb874a748653dfac4c53e85da").as_slice()
        );

        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        assert_eq!(launch.name, b"Koyuki".as_slice());
        assert_eq!(launch.symbol, unhex("e38193e38286e3818d").as_slice());
        assert_eq!(launch.uri.len(), 80);
        assert!(launch.uri.starts_with(b"https://ipfs.io/ipfs/"));
        assert_eq!(launch.dex_thresh, 1);
        assert_eq!(
            launch.salt.as_slice(),
            unhex("0c5ae0f6ffd0a19acfb501ad8eb95542ce1ee50305987db5b6d388537a128e2c").as_slice()
        );
        assert_eq!(launch.migrator_type, 1);
        assert_eq!(launch.quote_token, address(DOGE));
        assert_eq!(launch.quote_amt, U256::from(80_574_062_404u64));
        assert_eq!(
            launch.beneficiary,
            address("9d6716dea8614448bd342f5c5c84a8def9079dc4")
        );
        assert!(launch.permit.is_empty());
        assert_eq!(
            launch.extension_id.as_slice(),
            unhex("3526f6a0e790306d64c896eeda5ec66052ea17889e20632ff0cf512923153dad").as_slice()
        );
        assert_eq!(launch.extension_data.len(), 256);
        assert_eq!(launch.dex_id, 0);
        assert_eq!(launch.lp_fee_profile, 0);
        assert_eq!(launch.buy_tax, 100);
        assert_eq!(launch.sell_tax, 100);
        assert_eq!(launch.tax_duration, 3_153_600_000);
        assert_eq!(launch.anti_farmer_duration, 31_536_000);
        assert_eq!(launch.mkt_bps, 0);
        assert_eq!(launch.deflation_bps, 0);
        assert_eq!(launch.dividend_bps, 10_000);
        assert_eq!(launch.lp_bps, 0);
        assert_eq!(
            launch.minimum_share_balance,
            U256::from(10_000_000_000_000_000_000_000u128)
        );
        assert_eq!(launch.dividend_token, address(DOGE));
        assert_eq!(launch.commission_receiver, Address::ZERO);
        assert_eq!(launch.token_version, TOKEN_VERSION_TAXED_V3);
        assert_eq!(launch.require_taxed_v3(), Ok(()));
    }

    #[test]
    fn captured_native_launches_decode_field_exact() {
        let calldata = fixture(NATIVE_CREATOR_BUY);
        // `calldata_keccak256` of fixtures/flap/MANIFEST.json.
        assert_eq!(
            keccak256(&calldata).as_slice(),
            unhex("7bec0a64177f951ad4290355bc4c55c9e23a16097c1368abf58ec978560b4493").as_slice()
        );
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        assert_eq!(launch.quote_token, Address::ZERO);
        assert_eq!(launch.quote_amt, U256::from(10_000_000_000_000_000u64));
        assert_eq!(launch.buy_tax, 0);
        assert_eq!(launch.sell_tax, 100);
        assert_eq!(launch.extension_id, B256::ZERO);
        assert!(launch.extension_data.is_empty());
        assert_eq!(launch.token_version, 6);

        let calldata = fixture(NATIVE_SPLIT_TAX);
        assert_eq!(
            keccak256(&calldata).as_slice(),
            unhex("142128a92efab2c9cf27875e0b48240a6059f527e077d5fbffe9038cf2413417").as_slice()
        );
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        assert_eq!(launch.name, b"CZ SOON".as_slice());
        assert_eq!(launch.symbol, b"SOON".as_slice());
        assert_eq!(launch.uri.len(), 59);
        assert_eq!(launch.quote_amt, U256::ZERO);
        assert_eq!(launch.buy_tax, 200);
        assert_eq!(launch.sell_tax, 200);
        assert_eq!(launch.anti_farmer_duration, 86_400);
        assert_eq!(launch.mkt_bps, 5_000);
        assert_eq!(launch.deflation_bps, 5_000);
        assert_eq!(launch.dividend_bps, 0);
        assert_eq!(launch.minimum_share_balance, U256::ZERO);
        assert_eq!(launch.token_version, 6);
    }

    #[test]
    fn accepted_launches_reencode_to_identical_bytes() {
        let mut out = vec![0xeeu8; 7];
        for hex in [DOGE_LAUNCH, NATIVE_CREATOR_BUY, NATIVE_SPLIT_TAX] {
            let calldata = fixture(hex);
            let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
            assert_eq!(new_token_v6_encoded_len(&launch), calldata.len());
            encode_new_token_v6(&launch, &mut out);
            assert_eq!(out, calldata);
        }
    }

    #[test]
    fn oversize_wrong_selector_and_truncation_are_rejected() {
        let calldata = fixture(DOGE_LAUNCH);
        let max = calldata.len();

        assert_eq!(
            decode_new_token_v6(&calldata, max - 1),
            Err(AbiError::Oversize)
        );

        let mut other = calldata.clone();
        other[0] ^= 0x01;
        assert_eq!(
            decode_new_token_v6(&other, max),
            Err(AbiError::WrongSelector)
        );
        assert_eq!(
            decode_new_token_v6(&calldata[..3], max),
            Err(AbiError::WrongSelector)
        );
        assert_eq!(
            decode_new_token_v6(&unhex(GOLDEN_CALLDATA), max),
            Err(AbiError::WrongSelector)
        );

        // One byte short of the full head, and one byte short of the end.
        assert_eq!(
            decode_new_token_v6(&calldata[..head(LAUNCH_FIELDS) - 1], max),
            Err(AbiError::Truncated)
        );
        assert_eq!(
            decode_new_token_v6(&calldata[..max - 1], max),
            Err(AbiError::Truncated)
        );

        let mut longer = calldata.clone();
        longer.extend_from_slice(&[0u8; WORD]);
        assert_eq!(
            decode_new_token_v6(&longer, longer.len()),
            Err(AbiError::TrailingBytes)
        );
    }

    #[test]
    fn non_canonical_layouts_are_rejected() {
        let calldata = fixture(DOGE_LAUNCH);
        let max = calldata.len();

        // Tuple offset 0x40 instead of 0x20.
        let mut other = calldata.clone();
        other[4 + WORD - 1] = 0x40;
        assert_eq!(
            decode_new_token_v6(&other, max),
            Err(AbiError::NonCanonical)
        );

        // `name` offset moved by one word.
        let mut other = calldata.clone();
        other[head(0) + WORD - 1] = other[head(0) + WORD - 1].wrapping_add(0x20);
        assert_eq!(
            decode_new_token_v6(&other, max),
            Err(AbiError::NonCanonical)
        );

        // Non-zero padding after the six bytes of `name`.
        let mut other = calldata.clone();
        other[head(LAUNCH_FIELDS) + WORD + 6] = 0x01;
        assert_eq!(
            decode_new_token_v6(&other, max),
            Err(AbiError::NonCanonical)
        );

        // `name` length far beyond the calldata.
        let mut other = calldata.clone();
        other[head(LAUNCH_FIELDS) + WORD - 2] = 0xff;
        other[head(LAUNCH_FIELDS) + WORD - 1] = 0xff;
        assert_eq!(decode_new_token_v6(&other, max), Err(AbiError::Truncated));
    }

    #[test]
    fn value_types_with_dirty_high_bits_are_rejected() {
        let calldata = fixture(DOGE_LAUNCH);
        let max = calldata.len();
        // (head word, byte inside the word): uint8 dexThresh, address
        // quoteToken, uint16 buyTax, uint64 taxDuration, uint8 tokenVersion.
        for (index, byte) in [(3usize, 0usize), (6, 0), (14, 29), (16, 23), (25, 30)] {
            let mut other = calldata.clone();
            other[head(index) + byte] = 0x01;
            assert_eq!(
                decode_new_token_v6(&other, max),
                Err(AbiError::ValueOutOfRange),
                "word {index} byte {byte}"
            );
        }
    }

    #[test]
    fn unknown_token_version_decodes_but_is_not_supported() {
        let mut calldata = fixture(DOGE_LAUNCH);
        calldata[head(25) + WORD - 1] = 7;
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        assert_eq!(launch.token_version, 7);
        assert_eq!(
            launch.require_taxed_v3(),
            Err(AbiError::UnsupportedTokenVersion)
        );
    }

    #[test]
    fn extension_tuple_of_the_doge_launch() {
        let calldata = fixture(DOGE_LAUNCH);
        let launch = decode_new_token_v6(&calldata, calldata.len()).unwrap();
        let data = launch.extension_data;
        let extension = decode_extension_tuple(data).unwrap();
        assert_eq!(extension.mode, U256::ZERO);
        assert_eq!(extension.price, U256::from(96_028_517_023_029_920u64));
        assert_eq!(extension.deadline, U256::from(1_790_549_433u64));
        assert_eq!(extension.signature.len(), 65);

        let empty: &[u8] = &[];
        assert_eq!(decode_extension_tuple(empty), Err(AbiError::Truncated));
        assert_eq!(
            decode_extension_tuple(&data[..data.len() - 1]),
            Err(AbiError::Truncated)
        );
        let mut longer = data.to_vec();
        longer.extend_from_slice(&[0u8; WORD]);
        assert_eq!(
            decode_extension_tuple(&longer),
            Err(AbiError::TrailingBytes)
        );
        let mut other = data.to_vec();
        other[4 * WORD - 1] = 0xa0;
        assert_eq!(decode_extension_tuple(&other), Err(AbiError::NonCanonical));
    }

    #[test]
    fn buy_calldata_matches_the_canonical_mainnet_transaction() {
        // Calldata of mainnet transaction 0x8aff738f…570f6b (G0.3 step 4).
        let golden = unhex(GOLDEN_CALLDATA);
        let input = U256::from(150_000_000_000_000_000u64);
        let minimum = U256::from(6_000_000_000_000_000_000_000_000u128);
        let buy = encode_swap_exact_input_native(address(TOKEN), input, minimum).unwrap();
        assert_eq!(buy.len(), 228);
        assert_eq!(buy.as_slice(), golden.as_slice());

        assert_eq!(
            decode_swap_exact_input_native(&golden),
            Ok(SwapExactInputNative {
                output_token: address(TOKEN),
                input_amount: input,
                min_output_amount: minimum,
            })
        );
    }

    #[test]
    fn buy_encoder_rejects_zero_amounts() {
        let one = U256::from(1u64);
        assert_eq!(
            encode_swap_exact_input_native(address(TOKEN), one, U256::ZERO),
            Err(AbiError::ZeroMinOutput)
        );
        assert_eq!(
            encode_swap_exact_input_native(address(TOKEN), U256::ZERO, one),
            Err(AbiError::ZeroInputAmount)
        );
    }

    #[test]
    fn buy_decoder_rejects_everything_but_the_canonical_native_form() {
        let golden = unhex(GOLDEN_CALLDATA);
        let word = |index: usize| 4 + index * WORD;

        assert_eq!(
            decode_swap_exact_input_native(&golden[..227]),
            Err(AbiError::Truncated)
        );
        assert_eq!(
            decode_swap_exact_input_native(&golden[..3]),
            Err(AbiError::WrongSelector)
        );
        let mut longer = golden.clone();
        longer.push(0);
        assert_eq!(
            decode_swap_exact_input_native(&longer),
            Err(AbiError::TrailingBytes)
        );

        let mut other = golden.clone();
        other[..4].copy_from_slice(&NEW_TOKEN_V6_SELECTOR);
        assert_eq!(
            decode_swap_exact_input_native(&other),
            Err(AbiError::WrongSelector)
        );

        // Tuple offset, permit offset: not the canonical layout.
        for (index, value) in [(0usize, 0x40u8), (5, 0xc0)] {
            let mut other = golden.clone();
            other[word(index) + WORD - 1] = value;
            assert_eq!(
                decode_swap_exact_input_native(&other),
                Err(AbiError::NonCanonical),
                "word {index}"
            );
        }
        // A permit length announcing bytes that are not there.
        let mut other = golden.clone();
        other[word(6) + WORD - 1] = 0x01;
        assert_eq!(
            decode_swap_exact_input_native(&other),
            Err(AbiError::Truncated)
        );
        // A present, canonical permit: not the native BUY form.
        let mut permit = golden.clone();
        permit[word(6) + WORD - 1] = 0x01;
        permit.push(0xab);
        permit.extend_from_slice(&[0u8; WORD - 1]);
        assert_eq!(
            decode_swap_exact_input_native(&permit),
            Err(AbiError::NonCanonical)
        );

        // Token input instead of native input.
        let mut other = golden.clone();
        other[word(1) + WORD - 1] = 0x01;
        assert_eq!(
            decode_swap_exact_input_native(&other),
            Err(AbiError::UnsupportedInputToken)
        );

        // Dirty high bytes in the output token word.
        let mut other = golden.clone();
        other[word(2)] = 0x01;
        assert_eq!(
            decode_swap_exact_input_native(&other),
            Err(AbiError::ValueOutOfRange)
        );
    }
}
