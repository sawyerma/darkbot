//! Differential and robustness checks of `darkbot_wire::rlp` against the
//! independent `alloy-rlp` implementation.
//!
//! The decoder is meant to see untrusted bytes. These tests feed it
//! structured and random inputs and require that it never panics and that
//! it makes the same accept/reject decision and the same payload split as
//! `alloy-rlp`. They complement, and do not replace, coverage-guided fuzzing.

use alloy_rlp::{Decodable, Header};
use darkbot_wire::rlp;

/// Deterministic xorshift64* generator: no dependency, reproducible inputs.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// Uniform-enough value in `0..bound`. `bound` must be positive.
    fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next_u64().to_le_bytes()[0]).collect()
    }
}

fn check_string(input: &[u8]) {
    let mut their_rest = input;
    let theirs = Header::decode_bytes(&mut their_rest, false);
    match (rlp::take_bytes(input), theirs) {
        (Ok((payload, rest)), Ok(expected)) => {
            assert_eq!(payload, expected, "string payload differs: {input:02x?}");
            assert_eq!(rest, their_rest, "string rest differs: {input:02x?}");
        }
        (Err(_), Err(_)) => {}
        (ours, theirs) => {
            panic!("string disagreement on {input:02x?}: ours={ours:?} theirs={theirs:?}")
        }
    }
}

fn check_list(input: &[u8]) {
    let mut their_rest = input;
    let theirs = Header::decode_bytes(&mut their_rest, true);
    match (rlp::list_payload_exact(input), theirs) {
        (Ok(payload), Ok(expected)) => {
            assert!(their_rest.is_empty(), "list rest differs: {input:02x?}");
            assert_eq!(payload, expected, "list payload differs: {input:02x?}");
        }
        (Err(rlp::DecodeError::TrailingBytes), Ok(_)) => {
            assert!(
                !their_rest.is_empty(),
                "spurious trailing bytes: {input:02x?}"
            );
        }
        (Err(_), Err(_)) => {}
        (ours, theirs) => {
            panic!("list disagreement on {input:02x?}: ours={ours:?} theirs={theirs:?}")
        }
    }
}

fn check_u64(input: &[u8]) {
    let mut their_rest = input;
    let theirs = <u64 as Decodable>::decode(&mut their_rest);
    let ours = rlp::take_bytes(input)
        .and_then(|(payload, rest)| rlp::uint_u64(payload).map(|value| (value, rest)));
    match (ours, theirs) {
        (Ok((value, rest)), Ok(expected)) => {
            assert_eq!(value, expected, "integer differs: {input:02x?}");
            assert_eq!(rest, their_rest, "integer rest differs: {input:02x?}");
        }
        (Err(_), Err(_)) => {}
        (ours, theirs) => {
            panic!("integer disagreement on {input:02x?}: ours={ours:?} theirs={theirs:?}")
        }
    }
}

fn check_all(input: &[u8]) {
    check_string(input);
    check_list(input);
    check_u64(input);
}

#[test]
fn decoder_agrees_with_alloy_rlp_and_never_panics() {
    let mut rng = Rng(0x00c0_ffee_dead_beef);
    let mut cases = 0usize;

    // 1. Every prefix byte, each with short random tails.
    for first in 0..=255u8 {
        for _ in 0..24 {
            let tail_len = rng.below(72);
            let mut input = vec![first];
            input.extend_from_slice(&rng.bytes(tail_len));
            check_all(&input);
            cases += 1;
        }
    }

    // 2. Long-form string and list prefixes with chosen length fields and
    //    one byte less than, exactly, and one byte more than announced.
    let announced_lengths: [u64; 10] = [0, 1, 55, 56, 57, 255, 256, 1000, 65_535, 65_536];
    for base in [0xb7u8, 0xf7] {
        for len_of_len in 1..=8u8 {
            for announced in announced_lengths {
                let be = announced.to_be_bytes();
                let field = &be[8 - usize::from(len_of_len)..];
                let encoded_len = field.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
                let exact = usize::try_from(encoded_len).unwrap().min(70_000);
                for available in [exact.saturating_sub(1), exact, exact + 1] {
                    let mut input = vec![base + len_of_len];
                    input.extend_from_slice(field);
                    input.extend_from_slice(&rng.bytes(available));
                    check_all(&input);
                    cases += 1;
                }
            }
        }
    }

    // 3. Valid encodings, then truncated, extended, corrupted and nested.
    for _ in 0..3000 {
        let len = match rng.below(4) {
            0 => rng.below(3),
            1 => rng.below(60),
            2 => 50 + rng.below(12),
            _ => rng.below(1200),
        };
        let data = rng.bytes(len);
        let encoded = rlp::encode_bytes(&data);
        check_all(&encoded);

        let cut = rng.below(encoded.len() + 1);
        check_all(&encoded[..cut]);

        let extra = 1 + rng.below(4);
        let mut extended = encoded.clone();
        extended.extend_from_slice(&rng.bytes(extra));
        check_all(&extended);

        let position = rng.below(encoded.len().min(10));
        let bit = rng.below(8);
        let mut corrupted = encoded.clone();
        corrupted[position] ^= 1u8 << bit;
        check_all(&corrupted);

        let nested = rlp::encode_list(&[encoded.as_slice()]);
        check_all(&nested);

        cases += 5;
    }

    // 4. Unstructured random buffers.
    for _ in 0..6000 {
        let len = rng.below(48);
        let input = rng.bytes(len);
        check_all(&input);
        cases += 1;
    }

    assert!(cases >= 20_000, "only {cases} inputs were checked");
}

#[test]
fn encoders_agree_with_alloy_rlp() {
    let mut rng = Rng(0x1234_5678_9abc_def1);

    for len in (0usize..=300).chain([1023, 1024, 65_535, 65_536]) {
        let data = rng.bytes(len);
        assert_eq!(
            rlp::encode_bytes(&data),
            alloy_rlp::encode(data.as_slice()),
            "len={len}"
        );
    }
    for single in 0..=255u8 {
        let one = [single];
        assert_eq!(rlp::encode_bytes(&one), alloy_rlp::encode(one.as_slice()));
    }

    let mut values = vec![0u64, 1, 0x7f, 0x80, 0xff, 0x100, u64::MAX];
    for shift in 0..64u32 {
        values.push(rng.next_u64() >> shift);
    }
    for value in values {
        let expected = alloy_rlp::encode(value);
        assert_eq!(rlp::encode_u64(value), expected, "value={value}");
        assert_eq!(
            rlp::encode_uint_be(&value.to_be_bytes()),
            expected,
            "value={value}"
        );
    }
}
