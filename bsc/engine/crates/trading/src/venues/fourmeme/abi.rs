//! Canonical ABI reading for the Four.meme calls and events: borrowed
//! words, no allocation, fail closed. A value is accepted only in the one
//! canonical encoding (minimal offsets in order, zero padding, nothing
//! after the last member).

use alloy_primitives::{Address, B256, U256};

/// One ABI word.
pub const WORD: usize = 32;

/// Why calldata or log data is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbiError {
    /// The calldata exceeds the caller's declared capacity.
    Oversize,
    /// The calldata does not start with the expected selector.
    WrongSelector,
    /// The input ends before an announced member does.
    Truncated,
    /// Bytes remain after the last member.
    TrailingBytes,
    /// Valid ABI, but not the unique canonical encoding.
    NonCanonical,
    /// A value type carries bits outside its declared width.
    ValueOutOfRange,
    /// A layout no frozen fixture witnessed (another argument length or
    /// head): unsupported, never guessed.
    UnwitnessedLayout,
}

impl core::fmt::Display for AbiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Oversize => "four abi: calldata exceeds the declared capacity",
            Self::WrongSelector => "four abi: unexpected selector",
            Self::Truncated => "four abi: input is truncated",
            Self::TrailingBytes => "four abi: trailing bytes",
            Self::NonCanonical => "four abi: non-canonical encoding",
            Self::ValueOutOfRange => "four abi: value outside its type",
            Self::UnwitnessedLayout => "four abi: layout not witnessed by the freeze",
        })
    }
}

impl std::error::Error for AbiError {}

/// A region read as ABI words.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Words<'a>(pub(crate) &'a [u8]);

impl<'a> Words<'a> {
    fn word(&self, index: usize) -> Result<&'a [u8], AbiError> {
        let start = index.checked_mul(WORD).ok_or(AbiError::Truncated)?;
        let end = start.checked_add(WORD).ok_or(AbiError::Truncated)?;
        self.0.get(start..end).ok_or(AbiError::Truncated)
    }

    pub(crate) fn u256(&self, index: usize) -> Result<U256, AbiError> {
        Ok(U256::from_be_slice(self.word(index)?))
    }

    pub(crate) fn b256(&self, index: usize) -> Result<B256, AbiError> {
        Ok(B256::from_slice(self.word(index)?))
    }

    pub(crate) fn address(&self, index: usize) -> Result<Address, AbiError> {
        address_word(self.word(index)?)
    }

    pub(crate) fn uint8(&self, index: usize) -> Result<u8, AbiError> {
        let (high, low) = self.word(index)?.split_at(WORD - 1);
        if high.iter().any(|b| *b != 0) {
            return Err(AbiError::ValueOutOfRange);
        }
        Ok(low[0])
    }

    pub(crate) fn bool(&self, index: usize) -> Result<bool, AbiError> {
        match self.uint8(index)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(AbiError::ValueOutOfRange),
        }
    }

    /// A word that must hold a byte offset or a length.
    pub(crate) fn usize(&self, index: usize) -> Result<usize, AbiError> {
        let (high, low) = self.word(index)?.split_at(WORD - 8);
        if high.iter().any(|b| *b != 0) {
            return Err(AbiError::ValueOutOfRange);
        }
        let mut value = [0u8; 8];
        value.copy_from_slice(low);
        usize::try_from(u64::from_be_bytes(value)).map_err(|_| AbiError::ValueOutOfRange)
    }
}

/// An address word: twelve zero bytes, then twenty.
pub(crate) fn address_word(word: &[u8]) -> Result<Address, AbiError> {
    if word.len() != WORD {
        return Err(AbiError::Truncated);
    }
    let (high, low) = word.split_at(WORD - 20);
    if high.iter().any(|b| *b != 0) {
        return Err(AbiError::ValueOutOfRange);
    }
    Ok(Address::from_slice(low))
}

/// `data` must be exactly `words` static words.
pub(crate) fn exact(data: &[u8], words: usize) -> Result<Words<'_>, AbiError> {
    match data.len().cmp(&(words * WORD)) {
        core::cmp::Ordering::Less => Err(AbiError::Truncated),
        core::cmp::Ordering::Greater => Err(AbiError::TrailingBytes),
        core::cmp::Ordering::Equal => Ok(Words(data)),
    }
}

/// The `N` dynamic members (`bytes` or `string`) of one tuple whose static
/// head has `head_words` words and whose dynamic members sit at `heads`,
/// in order. Canonical: the first tail starts right after the head, every
/// next one right after the previous padded one, padding is zero, and the
/// last one ends the tuple exactly.
pub(crate) fn tails<const N: usize>(
    tuple: &[u8],
    head_words: usize,
    heads: [usize; N],
) -> Result<[&[u8]; N], AbiError> {
    let w = Words(tuple);
    let head_len = head_words.checked_mul(WORD).ok_or(AbiError::Truncated)?;
    if tuple.len() < head_len {
        return Err(AbiError::Truncated);
    }
    let mut next = head_len;
    let mut out: [&[u8]; N] = [&[]; N];
    for (slot, head) in out.iter_mut().zip(heads) {
        if w.usize(head)? != next {
            return Err(AbiError::NonCanonical);
        }
        let (value, end) = dynamic(tuple, next)?;
        *slot = value;
        next = end;
    }
    if next != tuple.len() {
        return Err(AbiError::TrailingBytes);
    }
    Ok(out)
}

/// One `bytes` value whose length word starts at `offset` of `region`:
/// the value and the end of its zero-padded tail.
fn dynamic(region: &[u8], offset: usize) -> Result<(&[u8], usize), AbiError> {
    let start = offset.checked_add(WORD).ok_or(AbiError::Truncated)?;
    let len = Words(region.get(offset..start).ok_or(AbiError::Truncated)?).usize(0)?;
    let padded = len
        .checked_next_multiple_of(WORD)
        .ok_or(AbiError::Truncated)?;
    let end = start.checked_add(padded).ok_or(AbiError::Truncated)?;
    let tail = region.get(start..end).ok_or(AbiError::Truncated)?;
    let (value, padding) = tail.split_at(len);
    if padding.iter().any(|b| *b != 0) {
        return Err(AbiError::NonCanonical);
    }
    Ok((value, end))
}

/// The arguments of a call after its selector, checked against `selector`.
pub(crate) fn body<'a>(input: &'a [u8], selector: &[u8; 4]) -> Result<&'a [u8], AbiError> {
    input.strip_prefix(selector).ok_or(AbiError::WrongSelector)
}
