//! Canonical RLP (Recursive Length Prefix) primitives.
//!
//! Consolidates the twelve `rlp_bytes` / `rlp_list` / `rlp_u64` / `rlp_u256`
//! copies of the Gate-0 harness into one implementation with identical output
//! bytes. The Gate-0 semantics were proven against a canonical BSC mainnet
//! transaction hash (G0.3) and against executed testnet transactions (G0.7).
//!
//! # Encoding
//!
//! The `put_*` writers append to a caller-owned buffer and never allocate
//! beyond that buffer's growth. A caller that needs an exact-capacity buffer
//! uses the matching `*_encoded_len` function first; a list is written as
//! `put_list_header(payload_len)` followed by its items.
//!
//! Integers are encoded canonically: big-endian without leading zero bytes,
//! zero as the empty string (`0x80`).
//!
//! # Decoding
//!
//! The decoder is strict. It never panics on any input, borrows instead of
//! copying, and rejects every non-canonical form (non-minimal length prefix,
//! single byte below `0x80` carried with a prefix, integer with a leading
//! zero byte, trailing bytes).

/// Largest payload length that uses the single-byte (short) prefix form.
const SHORT_MAX: usize = 55;

/// Number of bytes in the minimal big-endian representation of `v`.
/// Zero has length 0.
#[inline]
fn be_len(v: u64) -> usize {
    8 - (v.leading_zeros() as usize) / 8
}

/// Length of the prefix that introduces a payload of `payload_len` bytes.
#[inline]
fn header_len(payload_len: usize) -> usize {
    if payload_len <= SHORT_MAX {
        1
    } else {
        1 + be_len(payload_len as u64)
    }
}

#[inline]
fn put_header(out: &mut Vec<u8>, short_base: u8, long_base: u8, payload_len: usize) {
    if payload_len <= SHORT_MAX {
        out.push(short_base + payload_len as u8);
    } else {
        let be = (payload_len as u64).to_be_bytes();
        let n = be_len(payload_len as u64);
        out.push(long_base + n as u8);
        out.extend_from_slice(&be[8 - n..]);
    }
}

#[inline]
fn strip_leading_zeros(be: &[u8]) -> &[u8] {
    let skip = be.iter().take_while(|b| **b == 0).count();
    &be[skip..]
}

// ---------------------------------------------------------------------------
// Encoded lengths
// ---------------------------------------------------------------------------

/// Encoded length of the byte string `bytes`.
#[inline]
pub fn bytes_encoded_len(bytes: &[u8]) -> usize {
    if bytes.len() == 1 && bytes[0] < 0x80 {
        1
    } else {
        header_len(bytes.len()) + bytes.len()
    }
}

/// Encoded length of the unsigned integer given as big-endian bytes
/// (leading zero bytes are not encoded).
#[inline]
pub fn uint_be_encoded_len(be: &[u8]) -> usize {
    bytes_encoded_len(strip_leading_zeros(be))
}

/// Encoded length of `v`.
#[inline]
pub fn u64_encoded_len(v: u64) -> usize {
    uint_be_encoded_len(&v.to_be_bytes())
}

/// Encoded length of a list whose concatenated encoded items occupy
/// `payload_len` bytes.
#[inline]
pub fn list_encoded_len(payload_len: usize) -> usize {
    header_len(payload_len) + payload_len
}

// ---------------------------------------------------------------------------
// Encoding into a caller-owned buffer
// ---------------------------------------------------------------------------

/// Appends the encoding of the byte string `bytes`.
#[inline]
pub fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    if bytes.len() == 1 && bytes[0] < 0x80 {
        out.push(bytes[0]);
        return;
    }
    put_header(out, 0x80, 0xb7, bytes.len());
    out.extend_from_slice(bytes);
}

/// Appends the canonical encoding of the unsigned integer given as
/// big-endian bytes. Leading zero bytes are stripped; zero becomes `0x80`.
#[inline]
pub fn put_uint_be(out: &mut Vec<u8>, be: &[u8]) {
    put_bytes(out, strip_leading_zeros(be));
}

/// Appends the canonical encoding of `v`.
#[inline]
pub fn put_u64(out: &mut Vec<u8>, v: u64) {
    put_uint_be(out, &v.to_be_bytes());
}

/// Appends the prefix of a list whose concatenated encoded items occupy
/// `payload_len` bytes. The caller appends exactly that many item bytes.
#[inline]
pub fn put_list_header(out: &mut Vec<u8>, payload_len: usize) {
    put_header(out, 0xc0, 0xf7, payload_len);
}

// ---------------------------------------------------------------------------
// Allocation-returning forms
//
// Same encoders as above, for callers outside the latency-critical path
// (tools, tests, qualification harnesses).
// ---------------------------------------------------------------------------

/// Returns the encoding of the byte string `bytes`.
pub fn encode_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes_encoded_len(bytes));
    put_bytes(&mut out, bytes);
    out
}

/// Returns the canonical encoding of the big-endian unsigned integer `be`.
pub fn encode_uint_be(be: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(uint_be_encoded_len(be));
    put_uint_be(&mut out, be);
    out
}

/// Returns the canonical encoding of `v`.
pub fn encode_u64(v: u64) -> Vec<u8> {
    encode_uint_be(&v.to_be_bytes())
}

/// Returns the encoding of a list of already encoded items.
pub fn encode_list<T: AsRef<[u8]>>(encoded_items: &[T]) -> Vec<u8> {
    let payload_len: usize = encoded_items.iter().map(|item| item.as_ref().len()).sum();
    let mut out = Vec::with_capacity(list_encoded_len(payload_len));
    put_list_header(&mut out, payload_len);
    for item in encoded_items {
        out.extend_from_slice(item.as_ref());
    }
    out
}

// ---------------------------------------------------------------------------
// Strict decoding
// ---------------------------------------------------------------------------

/// Reason a byte sequence is not the expected canonical RLP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The input ends before the announced item does.
    UnexpectedEnd,
    /// A byte string was required but the item is a list.
    ExpectedString,
    /// A list was required but the item is a byte string.
    ExpectedList,
    /// The item is valid RLP but not in its unique canonical form.
    NonCanonical,
    /// A length or integer does not fit the requested width.
    LengthOverflow,
    /// Bytes remain after the last expected item.
    TrailingBytes,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::UnexpectedEnd => "rlp: input ends before the item does",
            Self::ExpectedString => "rlp: expected a byte string, found a list",
            Self::ExpectedList => "rlp: expected a list, found a byte string",
            Self::NonCanonical => "rlp: non-canonical encoding",
            Self::LengthOverflow => "rlp: length or integer exceeds the supported width",
            Self::TrailingBytes => "rlp: trailing bytes after the last item",
        })
    }
}

impl std::error::Error for DecodeError {}

/// Reads a long-form length. `len_of_len` is 1..=8 by construction of the
/// prefix byte.
fn read_long_len(input: &[u8], len_of_len: usize) -> Result<usize, DecodeError> {
    let len_bytes = input
        .get(1..1 + len_of_len)
        .ok_or(DecodeError::UnexpectedEnd)?;
    if len_bytes[0] == 0 {
        return Err(DecodeError::NonCanonical);
    }
    let mut len: u64 = 0;
    for b in len_bytes {
        len = (len << 8) | u64::from(*b);
    }
    if len <= SHORT_MAX as u64 {
        return Err(DecodeError::NonCanonical);
    }
    usize::try_from(len).map_err(|_| DecodeError::LengthOverflow)
}

fn split_payload(input: &[u8], header: usize, len: usize) -> Result<(&[u8], &[u8]), DecodeError> {
    let end = header.checked_add(len).ok_or(DecodeError::LengthOverflow)?;
    if end > input.len() {
        return Err(DecodeError::UnexpectedEnd);
    }
    Ok((&input[header..end], &input[end..]))
}

/// Splits one byte-string item off the front of `input`.
///
/// Returns `(payload, rest)`; both borrow from `input`.
pub fn take_bytes(input: &[u8]) -> Result<(&[u8], &[u8]), DecodeError> {
    let first = *input.first().ok_or(DecodeError::UnexpectedEnd)?;
    match first {
        0x00..=0x7f => Ok((&input[..1], &input[1..])),
        0x80..=0xb7 => {
            let len = usize::from(first - 0x80);
            let (payload, rest) = split_payload(input, 1, len)?;
            if len == 1 && payload[0] < 0x80 {
                return Err(DecodeError::NonCanonical);
            }
            Ok((payload, rest))
        }
        0xb8..=0xbf => {
            let len_of_len = usize::from(first - 0xb7);
            let len = read_long_len(input, len_of_len)?;
            split_payload(input, 1 + len_of_len, len)
        }
        0xc0..=0xff => Err(DecodeError::ExpectedString),
    }
}

/// Requires `input` to be exactly one list and returns its payload (the
/// concatenated encoded items).
pub fn list_payload_exact(input: &[u8]) -> Result<&[u8], DecodeError> {
    let first = *input.first().ok_or(DecodeError::UnexpectedEnd)?;
    let (payload, rest) = match first {
        0x00..=0xbf => return Err(DecodeError::ExpectedList),
        0xc0..=0xf7 => split_payload(input, 1, usize::from(first - 0xc0))?,
        0xf8..=0xff => {
            let len_of_len = usize::from(first - 0xf7);
            let len = read_long_len(input, len_of_len)?;
            split_payload(input, 1 + len_of_len, len)?
        }
    };
    if !rest.is_empty() {
        return Err(DecodeError::TrailingBytes);
    }
    Ok(payload)
}

/// Checks that `be` is a canonical unsigned integer of at most `max_len`
/// bytes (no leading zero byte; zero is the empty slice) and returns it.
pub fn canonical_uint(be: &[u8], max_len: usize) -> Result<&[u8], DecodeError> {
    if be.len() > max_len {
        return Err(DecodeError::LengthOverflow);
    }
    if be.first() == Some(&0) {
        return Err(DecodeError::NonCanonical);
    }
    Ok(be)
}

/// Interprets a decoded byte string as a canonical `u64`.
pub fn uint_u64(be: &[u8]) -> Result<u64, DecodeError> {
    let be = canonical_uint(be, 8)?;
    let mut v: u64 = 0;
    for b in be {
        v = (v << 8) | u64::from(*b);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unhex(s: &str) -> Vec<u8> {
        assert!(s.len().is_multiple_of(2), "odd hex length");
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Straightforward reference encoder, written independently of the
    /// production writers, used only to cross-check them.
    fn reference_bytes(b: &[u8]) -> Vec<u8> {
        if b.len() == 1 && b[0] < 0x80 {
            return b.to_vec();
        }
        let mut out = Vec::new();
        if b.len() <= 55 {
            out.push(0x80 + b.len() as u8);
        } else {
            let be: Vec<u8> = b
                .len()
                .to_be_bytes()
                .iter()
                .copied()
                .skip_while(|x| *x == 0)
                .collect();
            out.push(0xb7 + be.len() as u8);
            out.extend_from_slice(&be);
        }
        out.extend_from_slice(b);
        out
    }

    #[test]
    fn ethereum_reference_vectors() {
        // Byte strings.
        assert_eq!(encode_bytes(b""), unhex("80"));
        assert_eq!(encode_bytes(b"dog"), unhex("83646f67"));
        assert_eq!(encode_bytes(&[0x00]), unhex("00"));
        assert_eq!(encode_bytes(&[0x7f]), unhex("7f"));
        assert_eq!(encode_bytes(&[0x80]), unhex("8180"));
        // Integers.
        assert_eq!(encode_u64(0), unhex("80"));
        assert_eq!(encode_u64(15), unhex("0f"));
        assert_eq!(encode_u64(1024), unhex("820400"));
        assert_eq!(encode_u64(u64::MAX), unhex("88ffffffffffffffff"));
        // Lists.
        assert_eq!(encode_list::<Vec<u8>>(&[]), unhex("c0"));
        assert_eq!(
            encode_list(&[encode_bytes(b"cat"), encode_bytes(b"dog")]),
            unhex("c88363617483646f67")
        );
    }

    #[test]
    fn short_long_boundary_at_55_and_56_bytes() {
        let s55 = [0xaau8; 55];
        let e55 = encode_bytes(&s55);
        assert_eq!(e55[0], 0xb7);
        assert_eq!(e55.len(), 56);

        let s56 = [0xaau8; 56];
        let e56 = encode_bytes(&s56);
        assert_eq!(&e56[..2], &[0xb8, 56]);
        assert_eq!(e56.len(), 58);

        let s1024 = vec![0xaau8; 1024];
        let e1024 = encode_bytes(&s1024);
        assert_eq!(&e1024[..3], &[0xb9, 0x04, 0x00]);

        let mut h = Vec::new();
        put_list_header(&mut h, 55);
        assert_eq!(h, [0xf7]);
        h.clear();
        put_list_header(&mut h, 56);
        assert_eq!(h, [0xf8, 56]);
        h.clear();
        put_list_header(&mut h, 340);
        assert_eq!(h, [0xf9, 0x01, 0x54]);
    }

    #[test]
    fn writers_match_reference_and_declared_lengths() {
        for len in (0usize..=300).chain([1023, 1024, 65_535, 65_536, 70_000]) {
            for first in [0x00u8, 0x7f, 0x80, 0xff] {
                let mut data = vec![0x5au8; len];
                if len > 0 {
                    data[0] = first;
                }
                let encoded = encode_bytes(&data);
                assert_eq!(
                    encoded,
                    reference_bytes(&data),
                    "len={len} first={first:#x}"
                );
                assert_eq!(encoded.len(), bytes_encoded_len(&data));

                let mut with_tail = encoded.clone();
                with_tail.push(0xaa);
                let (payload, rest) = take_bytes(&with_tail).unwrap();
                assert_eq!(payload, data.as_slice());
                assert_eq!(rest, [0xaau8].as_slice());
            }
        }
    }

    #[test]
    fn integers_are_canonical_and_round_trip() {
        let mut values = vec![
            0u64,
            1,
            0x7f,
            0x80,
            0xff,
            0x100,
            0xffff,
            u64::from(u32::MAX),
            u64::from(u32::MAX) + 1,
            1 << 56,
            u64::MAX,
        ];
        // Deterministic spread over all byte widths.
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        for i in 0..2000u32 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            values.push(x >> (i % 64));
        }
        for v in values {
            let encoded = encode_u64(v);
            assert_eq!(encoded.len(), u64_encoded_len(v));
            let (payload, rest) = take_bytes(&encoded).unwrap();
            assert!(rest.is_empty());
            assert_ne!(payload.first(), Some(&0), "leading zero for {v}");
            assert_eq!(uint_u64(payload).unwrap(), v);
        }
    }

    #[test]
    fn uint_be_strips_leading_zeros_like_gate0_rlp_u256() {
        let mut word = [0u8; 32];
        assert_eq!(encode_uint_be(&word), unhex("80"));
        word[31] = 0x01;
        assert_eq!(encode_uint_be(&word), unhex("01"));
        word[31] = 0x80;
        assert_eq!(encode_uint_be(&word), unhex("8180"));
        word[0] = 0xff;
        let full = encode_uint_be(&word);
        assert_eq!(full[0], 0xa0);
        assert_eq!(full.len(), 33);
        assert_eq!(uint_be_encoded_len(&word), 33);
    }

    #[test]
    fn decoder_rejects_non_canonical_and_truncated_strings() {
        use DecodeError::*;
        // In order: empty input; single byte below 0x80 carried with a prefix;
        // long form with length 0; long form for a 55-byte payload; leading
        // zero in the length; truncated payload; truncated length bytes;
        // announced length far beyond the input; a list.
        let long_55 = format!("b837{}", "00".repeat(55));
        let cases: [(&str, DecodeError); 9] = [
            ("", UnexpectedEnd),
            ("8100", NonCanonical),
            ("b800", NonCanonical),
            (long_55.as_str(), NonCanonical),
            ("b9000100", NonCanonical),
            ("83aabb", UnexpectedEnd),
            ("b9", UnexpectedEnd),
            ("bf0101010101010101", UnexpectedEnd),
            ("c0", ExpectedString),
        ];
        for (hex, want) in cases {
            assert_eq!(take_bytes(&unhex(hex)), Err(want), "input {hex}");
        }
    }

    #[test]
    fn list_decoder_is_exact() {
        use DecodeError::*;
        let empty: &[u8] = &[];
        let one_zero: &[u8] = &[0x00];
        assert_eq!(list_payload_exact(&unhex("c100")), Ok(one_zero));
        assert_eq!(list_payload_exact(&unhex("c0")), Ok(empty));
        assert_eq!(list_payload_exact(&unhex("")), Err(UnexpectedEnd));
        assert_eq!(list_payload_exact(&unhex("80")), Err(ExpectedList));
        assert_eq!(list_payload_exact(&unhex("c2")), Err(UnexpectedEnd));
        assert_eq!(list_payload_exact(&unhex("c0aa")), Err(TrailingBytes));
        assert_eq!(list_payload_exact(&unhex("f80100")), Err(NonCanonical));
        assert_eq!(list_payload_exact(&unhex("f800")), Err(NonCanonical));
    }

    #[test]
    fn integer_decoder_rejects_leading_zero_and_overflow() {
        let empty: &[u8] = &[];
        assert_eq!(uint_u64(empty), Ok(0u64));
        assert_eq!(uint_u64(&[0x01u8, 0x00]), Ok(256u64));
        assert_eq!(uint_u64(&[0x00u8]), Err(DecodeError::NonCanonical));
        assert_eq!(uint_u64(&[0x00u8, 0x01]), Err(DecodeError::NonCanonical));
        assert_eq!(uint_u64(&[1u8; 9]), Err(DecodeError::LengthOverflow));
        let word: &[u8] = &[1u8; 32];
        assert_eq!(canonical_uint(word, 32), Ok(word));
        assert_eq!(
            canonical_uint(&[1u8; 33], 32),
            Err(DecodeError::LengthOverflow)
        );
    }
}
