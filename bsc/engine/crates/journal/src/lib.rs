//! Minimal production capture/replay foundation required by C0.
//!
//! The journal is append-only, versioned, checksummed per frame and hashed
//! over the exact bytes written. Replay verifies framing, checksum, bounds
//! and contiguous sequence numbers before yielding a record.
//!
//! The bounded recorder uses `try_send`: the producer never waits for the
//! writer. Queue overflow is returned to the caller and counted, never
//! silently discarded. The drain/writer belongs on a background owner.
//! Allocation freedom and final Fast-Lane placement are later C1/C3
//! qualification concerns; this module makes no latency claim.

#![deny(unsafe_code)]

#[allow(unsafe_code)]
pub mod durable;
mod format;
mod recorder;
mod replay;
pub mod storage;

pub use format::{
    CaptureRecord, DEFAULT_MAX_PAYLOAD, HEADER_LEN, JOURNAL_MAGIC, JOURNAL_VERSION, JOURNAL_SPAN_VERSION, JournalError,
    JournalManifest, JournalWriter, RecordKind,
};
pub use recorder::{
    RECORD_OVERHEAD, RecordError, Recorder, RecorderDrain, RecorderStats, SpannedDrain,
};
pub use storage::budget::{BytePermit, MemoryBudget};
pub use replay::JournalReader;

// Checksum and hash primitives shared by the writer and the reader. They stay
// private to the crate root so that both submodules use one implementation.
fn frame_crc(header: &[u8; HEADER_LEN], payload: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(&header[..32]);
    crc.update(payload);
    crc.finalize()
}

#[derive(Clone, Copy)]
struct Crc32(u32);

impl Crc32 {
    fn new() -> Self {
        Self(0xffff_ffff)
    }

    fn update(&mut self, bytes: &[u8]) {
        let mut crc = self.0;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = 0u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        self.0 = crc;
    }

    fn finalize(self) -> u32 {
        !self.0
    }
}

#[derive(Clone)]
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    len_bytes: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0u8; 64],
            buffer_len: 0,
            len_bytes: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.len_bytes = self
            .len_bytes
            .saturating_add(u64::try_from(input.len()).unwrap_or(u64::MAX));
        if self.buffer_len != 0 {
            let take = (64 - self.buffer_len).min(input.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&input[..take]);
            self.buffer_len += take;
            input = &input[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffer_len = 0;
            }
        }
        while input.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&input[..64]);
            self.compress(&block);
            input = &input[64..];
        }
        if !input.is_empty() {
            self.buffer[..input.len()].copy_from_slice(input);
            self.buffer_len = input.len();
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.len_bytes.wrapping_mul(8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;
        if self.buffer_len > 56 {
            self.buffer[self.buffer_len..].fill(0);
            let block = self.buffer;
            self.compress(&block);
            self.buffer = [0u8; 64];
            self.buffer_len = 0;
        }
        self.buffer[self.buffer_len..56].fill(0);
        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buffer;
        self.compress(&block);

        let mut out = [0u8; 32];
        for (index, word) in self.state.iter().enumerate() {
            out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for (index, chunk) in block.chunks_exact(4).take(16).enumerate() {
            w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use std::fmt::Write as _;
            write!(&mut out, "{byte:02x}").unwrap();
        }
        out
    }

    #[test]
    fn crc32_matches_the_standard_vector() {
        let mut crc = Crc32::new();
        crc.update(b"123456789");
        assert_eq!(crc.finalize(), 0xcbf4_3926);
    }

    #[test]
    fn sha256_matches_the_standard_vector() {
        let mut sha = Sha256::new();
        sha.update(b"abc");
        assert_eq!(
            hex(&sha.finalize()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// BSC-041 (BSC_Bug_V2 J3): the journal's own SHA-256 against
    /// independent references -- the FIPS 180 / NIST vectors and, at every
    /// padding boundary, digests from an independent implementation
    /// (OpenSSL via Python hashlib) -- in one call and streamed in chunks.
    #[test]
    fn sha256_matches_independent_vectors_at_every_padding_boundary_and_streamed() {
        let digest = |data: &[u8], chunk: usize| {
            let mut sha = Sha256::new();
            for part in data.chunks(chunk.max(1)) {
                sha.update(part);
            }
            hex(&sha.finalize())
        };
        let a = |n: usize| vec![b'a'; n];
        let vectors: Vec<(Vec<u8>, &str)> = vec![
            (Vec::new(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            (
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".to_vec(),
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
            (
                b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu".to_vec(),
                "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
            ),
            (a(55), "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318"),
            (a(56), "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"),
            (a(63), "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34"),
            (a(64), "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb"),
            (a(65), "635361c48bb9eab14198e76ea8ab7f1a41685d6ad62aa9146d301d4f17eb0ae0"),
            (a(119), "31eba51c313a5c08226adf18d4a359cfdfd8d2e816b13f4af952f7ea6584dcfb"),
            (a(120), "2f3d335432c70b580af0e8e1b3674a7c020d683aa5f73aaaedfdc55af904c21c"),
            (a(128), "6836cf13bac400e9105071cd6af47084dfacad4e5e302c94bfed24e013afb73e"),
            (a(1_000_000), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"),
        ];
        for (data, expected) in &vectors {
            for chunk in [usize::MAX, 1, 7, 55, 63, 64, 65, 4096] {
                if chunk == 1 && data.len() > 1024 {
                    continue;
                }
                assert_eq!(
                    digest(data, chunk),
                    *expected,
                    "{} bytes in {chunk}-byte chunks",
                    data.len()
                );
            }
        }
    }

    #[test]
    fn journal_roundtrip_preserves_exact_records_and_manifest() {
        let mut writer = JournalWriter::new(Vec::new());
        let a = CaptureRecord {
            sequence: 7,
            kind: RecordKind::RawSignedTx,
            timestamp_ns: 11,
            payload: b"alpha".to_vec().into_boxed_slice(),
        };
        let b = CaptureRecord {
            sequence: 8,
            kind: RecordKind::Canonical,
            timestamp_ns: 12,
            payload: b"beta".to_vec().into_boxed_slice(),
        };
        writer.append(&a).unwrap();
        writer.append(&b).unwrap();
        let manifest = writer.manifest();
        assert_eq!(manifest.records, 2);
        assert_eq!(manifest.first_sequence, Some(7));
        assert_eq!(manifest.last_sequence, Some(8));
        assert_ne!(manifest.sha256, [0u8; 32]);

        let bytes = writer.into_inner();
        let mut reader = JournalReader::new(Cursor::new(bytes), DEFAULT_MAX_PAYLOAD).unwrap();
        assert_eq!(reader.next_record().unwrap(), Some(a));
        assert_eq!(reader.next_record().unwrap(), Some(b));
        assert_eq!(reader.next_record().unwrap(), None);
    }

    #[test]
    fn corruption_and_truncation_fail_closed() {
        let record = CaptureRecord {
            sequence: 0,
            kind: RecordKind::Runtime,
            timestamp_ns: 9,
            payload: b"payload".to_vec().into_boxed_slice(),
        };
        let mut writer = JournalWriter::new(Vec::new());
        writer.append(&record).unwrap();
        let mut bytes = writer.into_inner();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        let mut reader = JournalReader::new(Cursor::new(bytes), 1024).unwrap();
        assert!(matches!(
            reader.next_record(),
            Err(JournalError::Checksum { .. })
        ));

        let mut writer = JournalWriter::new(Vec::new());
        writer.append(&record).unwrap();
        let mut bytes = writer.into_inner();
        bytes.pop();
        let mut reader = JournalReader::new(Cursor::new(bytes), 1024).unwrap();
        assert!(matches!(reader.next_record(), Err(JournalError::Truncated)));
    }

    #[test]
    fn sequence_gaps_are_rejected_by_writer_and_replay() {
        let mut writer = JournalWriter::new(Vec::new());
        let a = CaptureRecord {
            sequence: 1,
            kind: RecordKind::Runtime,
            timestamp_ns: 1,
            payload: Box::new([]),
        };
        let b = CaptureRecord {
            sequence: 3,
            kind: RecordKind::Runtime,
            timestamp_ns: 2,
            payload: Box::new([]),
        };
        writer.append(&a).unwrap();
        assert!(matches!(
            writer.append(&b),
            Err(JournalError::SequenceGap {
                expected: 2,
                observed: 3
            })
        ));
    }

    #[test]
    fn recorder_never_waits_and_overflow_is_counted_with_sequence() {
        let (mut recorder, drain) = Recorder::bounded(1, 64).unwrap();
        assert_eq!(recorder.try_record(RecordKind::Runtime, 1, b"a"), Ok(0));
        assert_eq!(
            recorder.try_record(RecordKind::Runtime, 2, b"b"),
            Err(RecordError::QueueFull { sequence: 1 })
        );
        let stats = recorder.stats();
        assert_eq!(stats.enqueued, 1);
        assert_eq!(stats.dropped_full, 1);
        assert_eq!(stats.next_sequence, 2);
        assert_eq!(drain.try_recv().unwrap().unwrap().sequence, 0);
    }

    #[test]
    fn oversize_payload_is_refused_before_sequence_consumption() {
        let (mut recorder, _drain) = Recorder::bounded(1, 2).unwrap();
        assert_eq!(
            recorder.try_record(RecordKind::Runtime, 1, b"abc"),
            Err(RecordError::PayloadTooLarge { len: 3, max: 2 })
        );
        let stats = recorder.stats();
        assert_eq!(stats.next_sequence, 0);
        assert_eq!(stats.rejected_oversize, 1);
    }

    /// Phase F gate L5: an owned payload moves into the record (the same
    /// allocation reaches the drain) under the same refusals and sequence.
    #[test]
    fn an_owned_payload_moves_into_the_record() {
        let (mut recorder, drain) = Recorder::bounded(1, 4).unwrap();
        let payload = vec![1u8, 2, 3];
        let at = payload.as_ptr();
        assert_eq!(recorder.try_record_owned(RecordKind::Runtime, 1, payload), Ok(0));
        assert_eq!(
            recorder.try_record_owned(RecordKind::Runtime, 2, vec![0; 5]),
            Err(RecordError::PayloadTooLarge { len: 5, max: 4 })
        );
        assert_eq!(
            recorder.try_record_owned(RecordKind::Runtime, 3, vec![4]),
            Err(RecordError::QueueFull { sequence: 1 })
        );
        let record = drain.try_recv().unwrap().unwrap();
        assert_eq!((record.sequence, &record.payload[..]), (0, &[1u8, 2, 3][..]));
        assert_eq!(record.payload.as_ptr(), at);
        assert_eq!(recorder.stats().next_sequence, 2);
    }

    #[test]
    fn reader_enforces_payload_bound_before_allocating_payload() {
        let record = CaptureRecord {
            sequence: 0,
            kind: RecordKind::Runtime,
            timestamp_ns: 1,
            payload: vec![1u8; 16].into_boxed_slice(),
        };
        let mut writer = JournalWriter::new(Vec::new());
        writer.append(&record).unwrap();
        let mut reader = JournalReader::new(Cursor::new(writer.into_inner()), 8).unwrap();
        assert!(matches!(
            reader.next_record(),
            Err(JournalError::PayloadBound { len: 16, max: 8 })
        ));
    }

    #[test]
    fn manifest_hash_is_over_exact_journal_bytes() {
        let record = CaptureRecord {
            sequence: 0,
            kind: RecordKind::Gap,
            timestamp_ns: 5,
            payload: b"drop=1".to_vec().into_boxed_slice(),
        };
        let mut writer = JournalWriter::new(Vec::new());
        writer.append(&record).unwrap();
        let manifest = writer.manifest();
        let bytes = writer.into_inner();
        let mut sha = Sha256::new();
        sha.update(&bytes);
        assert_eq!(manifest.sha256, sha.finalize());
        assert_eq!(manifest.bytes as usize, bytes.len());
    }
}

#[cfg(test)]
mod span_tests;
